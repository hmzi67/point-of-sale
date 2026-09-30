//! StoreKit 2 and Sign in with Apple for Diwan's iOS build.
//!
//! A thin, typed bridge to `ios/Sources/StoreKitPlugin.swift` and nothing
//! more: it knows how to buy, list, finish and restore App Store
//! transactions and how to show Apple's sign-in sheet, but it has no idea
//! what a licence is. The app's `iap` module decides what a purchase means
//! (verify with the licence server, activate, only then finish), keeping
//! enforcement in the app's Rust like everything else licence-related.
//!
//! No commands are exposed to the webview (`build.rs` registers none), so
//! the frontend can't reach StoreKit except through the app's own commands.
//!
//! Every call is async (`run_mobile_plugin_async`): a purchase sheet can sit
//! open for minutes, and must not park a runtime thread while it does.

use serde::{Deserialize, Serialize};
use tauri::{
    plugin::{Builder, PluginHandle, TauriPlugin},
    Manager, Runtime,
};

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_storekit);

/// One App Store product, as StoreKit localised it for this storefront.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    pub id: String,
    pub display_name: String,
    pub description: String,
    /// Already formatted for the user's storefront ("Rs 8,000.00", "$29.99").
    pub display_price: String,
}

/// A transaction as the server needs it: `signed_transaction` is StoreKit's
/// JWS, which only the licence server verifies. `transaction_id` and
/// `product_id` are read unverified, for bookkeeping on this side only.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedTransaction {
    pub transaction_id: String,
    pub product_id: String,
    pub signed_transaction: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum PurchaseOutcome {
    /// Paid. NOT finished yet — call [`StoreKit::finish`] once the licence
    /// is activated, or StoreKit re-delivers it on the next launch.
    Purchased { transaction: SignedTransaction },
    /// Waiting on Ask to Buy / Strong Customer Authentication; it arrives
    /// later through [`StoreKit::unfinished`].
    Pending,
    Cancelled,
}

/// What Sign in with Apple returned. `email` and `full_name` are only sent
/// the first time a user authorises the app.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppleIdCredential {
    pub identity_token: String,
    pub authorization_code: Option<String>,
    pub full_name: Option<String>,
}

#[derive(Debug)]
pub enum Error {
    /// Not an iOS build (or iOS older than 15, reported by the Swift side).
    Unsupported,
    Plugin(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Unsupported => write!(f, "App Store purchases are only available in the iOS app"),
            Error::Plugin(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for Error {}

pub struct StoreKit<R: Runtime> {
    #[cfg_attr(not(target_os = "ios"), allow(dead_code))]
    handle: Option<PluginHandle<R>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProductsArgs<'a> {
    product_ids: &'a [&'a str],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PurchaseArgs<'a> {
    product_id: &'a str,
    app_account_token: Option<&'a str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FinishArgs<'a> {
    transaction_id: &'a str,
}

#[derive(Serialize)]
struct EntitlementsArgs {
    sync: bool,
}

#[derive(Serialize)]
struct SignInArgs<'a> {
    nonce: &'a str,
}

#[derive(Deserialize)]
struct ProductsResponse {
    products: Vec<Product>,
}

#[derive(Deserialize)]
struct TransactionsResponse {
    transactions: Vec<SignedTransaction>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SignInResponse {
    #[serde(default)]
    cancelled: bool,
    #[serde(flatten)]
    credential: Option<AppleIdCredential>,
}

impl<R: Runtime> StoreKit<R> {
    async fn run<T: serde::de::DeserializeOwned>(&self, command: &str, args: impl Serialize) -> Result<T, Error> {
        #[cfg(target_os = "ios")]
        if let Some(handle) = &self.handle {
            return handle
                .run_mobile_plugin_async(command, args)
                .await
                .map_err(|e| Error::Plugin(e.to_string()));
        }
        let _ = (command, args);
        Err(Error::Unsupported)
    }

    /// The products StoreKit knows for these ids, cheapest first. Ids App
    /// Store Connect doesn't have (or hasn't approved yet) are just absent.
    pub async fn products(&self, product_ids: &[&str]) -> Result<Vec<Product>, Error> {
        let response: ProductsResponse = self.run("getProducts", ProductsArgs { product_ids }).await?;
        Ok(response.products)
    }

    /// Shows the App Store purchase sheet. `app_account_token` (a UUID) is
    /// written into the signed transaction so the server can tell whose
    /// account a purchase belongs to.
    pub async fn purchase(&self, product_id: &str, app_account_token: Option<&str>) -> Result<PurchaseOutcome, Error> {
        self.run("purchase", PurchaseArgs { product_id, app_account_token }).await
    }

    /// Tells StoreKit the purchase has been delivered. Until this runs it
    /// keeps re-delivering the transaction through [`Self::unfinished`].
    pub async fn finish(&self, transaction_id: &str) -> Result<(), Error> {
        self.run::<serde_json::Value>("finish", FinishArgs { transaction_id }).await.map(|_| ())
    }

    /// Every non-consumable this Apple ID currently owns. `sync: true` asks
    /// the App Store first (may prompt for the Apple ID password) — use it
    /// only for an explicit "Restore purchases" tap, as Apple requires.
    pub async fn entitlements(&self, sync: bool) -> Result<Vec<SignedTransaction>, Error> {
        let response: TransactionsResponse = self.run("entitlements", EntitlementsArgs { sync }).await?;
        Ok(response.transactions)
    }

    /// Paid transactions not yet finished: interrupted purchases, Ask to Buy
    /// approvals, purchases whose licence activation failed last time.
    pub async fn unfinished(&self) -> Result<Vec<SignedTransaction>, Error> {
        let response: TransactionsResponse = self.run("unfinished", serde_json::json!({})).await?;
        Ok(response.transactions)
    }

    /// Shows the Sign in with Apple sheet. `Ok(None)` means the user
    /// dismissed it. `nonce` is copied by Apple into the identity token.
    pub async fn sign_in_with_apple(&self, nonce: &str) -> Result<Option<AppleIdCredential>, Error> {
        let response: SignInResponse = self.run("signInWithApple", SignInArgs { nonce }).await?;
        if response.cancelled {
            return Ok(None);
        }
        response
            .credential
            .map(Some)
            .ok_or_else(|| Error::Plugin("Apple did not return an identity token".into()))
    }
}

/// `app.storekit()` from anything that can reach managed state.
pub trait StoreKitExt<R: Runtime> {
    fn storekit(&self) -> &StoreKit<R>;
}

impl<R: Runtime, T: Manager<R>> StoreKitExt<R> for T {
    fn storekit(&self) -> &StoreKit<R> {
        self.state::<StoreKit<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("storekit")
        .setup(|app, _api| {
            #[cfg(target_os = "ios")]
            let handle = Some(_api.register_ios_plugin(init_plugin_storekit)?);
            #[cfg(not(target_os = "ios"))]
            let handle = None;
            app.manage(StoreKit::<R> { handle });
            Ok(())
        })
        .build()
}
