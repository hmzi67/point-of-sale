// StoreKit 2 + Sign in with Apple for Diwan's iOS build.
//
// Deliberately dumb: this file never decides whether a purchase is valid or
// what it unlocks. It hands StoreKit's signed JWS to Rust, which has the
// licence server verify it against Apple's certificates, activates the
// resulting licence, and only then calls `finish`. A transaction whose
// licence couldn't be activated (offline, server down) therefore stays
// unfinished, and StoreKit re-delivers it on the next launch.

import AuthenticationServices
import StoreKit
import Tauri
import UIKit
import WebKit

struct ProductsArgs: Decodable {
  let productIds: [String]
}

struct PurchaseArgs: Decodable {
  let productId: String
  let appAccountToken: String?
}

struct FinishArgs: Decodable {
  let transactionId: String
}

struct EntitlementsArgs: Decodable {
  let sync: Bool
}

struct SignInArgs: Decodable {
  let nonce: String
}

private let needsIOS15 = "In-app purchases need iOS 15 or later."

class StoreKitPlugin: Plugin {
  private var updatesTask: Task<Void, Never>?
  /// ASAuthorizationController holds its delegate weakly; this keeps the
  /// in-flight sign-in's delegate alive until Apple calls back.
  private var signInDelegate: SignInDelegate?

  override init() {
    super.init()
    if #available(iOS 15.0, *) {
      // Apple expects a Transaction.updates listener from launch, or
      // purchases completed outside a purchase() call (Ask to Buy approvals,
      // interrupted purchases, other devices) can be missed. They are NOT
      // finished here — they stay in Transaction.unfinished, which Rust
      // sweeps at launch and redeems through the licence server first.
      updatesTask = Task.detached(priority: .background) {
        for await _ in Transaction.updates {}
      }
    }
  }

  deinit {
    updatesTask?.cancel()
  }

  @objc public func getProducts(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ProductsArgs.self)
    guard #available(iOS 15.0, *) else {
      invoke.reject(needsIOS15)
      return
    }
    Task {
      do {
        let products = try await Product.products(for: args.productIds)
          .sorted { $0.price < $1.price }
        invoke.resolve([
          "products": products.map { product -> JsonObject in
            [
              "id": product.id,
              "displayName": product.displayName,
              "description": product.description,
              "displayPrice": product.displayPrice,
            ]
          }
        ])
      } catch {
        invoke.reject("Could not load App Store products: \(error.localizedDescription)")
      }
    }
  }

  @objc public func purchase(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(PurchaseArgs.self)
    guard #available(iOS 15.0, *) else {
      invoke.reject(needsIOS15)
      return
    }
    Task { @MainActor in
      do {
        guard let product = try await Product.products(for: [args.productId]).first else {
          invoke.reject("That licence is not available on the App Store right now.")
          return
        }
        var options: Set<Product.PurchaseOption> = []
        if let token = args.appAccountToken, let uuid = UUID(uuidString: token) {
          options.insert(.appAccountToken(uuid))
        }
        switch try await product.purchase(options: options) {
        case .success(let verification):
          invoke.resolve(["status": "purchased", "transaction": Self.describe(verification)])
        case .pending:
          invoke.resolve(["status": "pending"])
        case .userCancelled:
          invoke.resolve(["status": "cancelled"])
        @unknown default:
          invoke.resolve(["status": "cancelled"])
        }
      } catch {
        invoke.reject("The purchase could not be completed: \(error.localizedDescription)")
      }
    }
  }

  @objc public func finish(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(FinishArgs.self)
    guard #available(iOS 15.0, *) else {
      invoke.reject(needsIOS15)
      return
    }
    Task {
      // Transaction.unfinished yields the current set and then ends.
      for await verification in Transaction.unfinished {
        let transaction = verification.unsafePayloadValue
        if String(transaction.id) == args.transactionId {
          await transaction.finish()
          break
        }
      }
      invoke.resolve()
    }
  }

  @objc public func entitlements(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(EntitlementsArgs.self)
    guard #available(iOS 15.0, *) else {
      invoke.reject(needsIOS15)
      return
    }
    Task {
      if args.sync {
        // Only on an explicit "Restore purchases" tap (Apple's rule): it can
        // ask for the Apple ID password. Cancelling it still leaves whatever
        // entitlements are already on the device, so that's not an error.
        try? await AppStore.sync()
      }
      var transactions: [JsonObject] = []
      for await verification in Transaction.currentEntitlements
      where verification.unsafePayloadValue.productType == .nonConsumable {
        transactions.append(Self.describe(verification))
      }
      invoke.resolve(["transactions": transactions])
    }
  }

  @objc public func unfinished(_ invoke: Invoke) {
    guard #available(iOS 15.0, *) else {
      invoke.resolve(["transactions": [JsonObject]()])
      return
    }
    Task {
      var transactions: [JsonObject] = []
      for await verification in Transaction.unfinished {
        transactions.append(Self.describe(verification))
      }
      invoke.resolve(["transactions": transactions])
    }
  }

  @objc public func signInWithApple(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(SignInArgs.self)
    DispatchQueue.main.async {
      let request = ASAuthorizationAppleIDProvider().createRequest()
      request.requestedScopes = [.fullName, .email]
      // The server-issued nonce, verbatim: the licence server checks its own
      // signature on it and that Apple copied it into the identity token.
      request.nonce = args.nonce

      let delegate = SignInDelegate(invoke: invoke) { [weak self] in
        self?.signInDelegate = nil
      }
      self.signInDelegate = delegate

      let controller = ASAuthorizationController(authorizationRequests: [request])
      controller.delegate = delegate
      controller.presentationContextProvider = delegate
      controller.performRequests()
    }
  }

  /// The JWS is what the server verifies; id and product are read unverified
  /// only so Rust can match and finish the transaction afterwards.
  @available(iOS 15.0, *)
  private static func describe(_ verification: VerificationResult<Transaction>) -> JsonObject {
    let transaction = verification.unsafePayloadValue
    return [
      "transactionId": String(transaction.id),
      "productId": transaction.productID,
      "signedTransaction": verification.jwsRepresentation,
    ]
  }
}

final class SignInDelegate: NSObject, ASAuthorizationControllerDelegate,
  ASAuthorizationControllerPresentationContextProviding
{
  private let invoke: Invoke
  private let done: () -> Void

  init(invoke: Invoke, done: @escaping () -> Void) {
    self.invoke = invoke
    self.done = done
  }

  func presentationAnchor(for controller: ASAuthorizationController) -> ASPresentationAnchor {
    let windows = UIApplication.shared.connectedScenes
      .compactMap { $0 as? UIWindowScene }
      .flatMap { $0.windows }
    return windows.first { $0.isKeyWindow } ?? windows.first ?? ASPresentationAnchor()
  }

  func authorizationController(
    controller: ASAuthorizationController,
    didCompleteWithAuthorization authorization: ASAuthorization
  ) {
    defer { done() }
    guard let credential = authorization.credential as? ASAuthorizationAppleIDCredential,
      let tokenData = credential.identityToken,
      let identityToken = String(data: tokenData, encoding: .utf8)
    else {
      invoke.reject("Apple did not return an identity token.")
      return
    }

    var result: JsonObject = ["identityToken": identityToken]
    if let codeData = credential.authorizationCode,
      let code = String(data: codeData, encoding: .utf8)
    {
      result["authorizationCode"] = code
    }
    if let name = credential.fullName {
      let formatted = PersonNameComponentsFormatter().string(from: name)
      if !formatted.isEmpty {
        result["fullName"] = formatted
      }
    }
    invoke.resolve(result)
  }

  func authorizationController(
    controller: ASAuthorizationController,
    didCompleteWithError error: Error
  ) {
    defer { done() }
    if let authError = error as? ASAuthorizationError, authError.code == .canceled {
      invoke.resolve(["cancelled": true])
    } else {
      invoke.reject("Sign in with Apple failed: \(error.localizedDescription)")
    }
  }
}

@_cdecl("init_plugin_storekit")
func initPlugin() -> Plugin {
  return StoreKitPlugin()
}
