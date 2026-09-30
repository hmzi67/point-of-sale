//! App Store purchases and Sign in with Apple — the iOS build's way to buy,
//! restore and move a licence.
//!
//! An App Store purchase is turned into an ordinary Diwan licence key by the
//! licence server (`api/apple-iap-verify.php`), which verifies StoreKit's
//! signed transaction against Apple's certificates. From there it is the
//! same key, activated through the same `LicenseRuntime::activate`, verified
//! offline on every launch like any other — nothing downstream knows or
//! cares that it came from the App Store. The order of operations is what
//! makes a purchase safe to interrupt at any point:
//!
//!   1. StoreKit purchase → signed transaction, NOT finished
//!   2. server verifies it → licence key (idempotent per purchase)
//!   3. `LicenseRuntime::activate` → key bound to this device, state stored
//!   4. only now: finish the transaction
//!
//! If 2 or 3 fail (offline, server down), the transaction stays unfinished
//! and StoreKit re-delivers it; `spawn_unfinished_sweep` redeems it on the
//! next launch. The user is never charged without eventually getting a key.
//!
//! Sign in with Apple is optional and only links purchases to a Diwan web
//! account (so the licence shows on the website dashboard) — buying never
//! requires an account, which App Review would reject. The signed-in state
//! lives in memory only; each linking or deletion re-authenticates.
//!
//! Every function exists on every platform so `commands.rs` needs no `cfg`;
//! off iOS they refuse with `UNSUPPORTED`.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::db::Db;
use crate::license::runtime::LicenseRuntime;
use crate::license::state::LicenseState;

#[cfg(target_os = "ios")]
use crate::license::client;
#[cfg(target_os = "ios")]
use tauri_plugin_storekit::{PurchaseOutcome, SignedTransaction, StoreKitExt};

/// App Store product ids, highest tier first — "Restore purchases" activates
/// the best licence the Apple ID owns. Must match the products created in App
/// Store Connect (Non-Consumable) and the server's `APPLE_IAP_PRODUCTS`.
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub const PRODUCT_IDS: [&str; 3] = [
    "com.diwan.pos.license.enterprise",
    "com.diwan.pos.license.standard",
    "com.diwan.pos.license.starter",
];

#[cfg_attr(target_os = "ios", allow(dead_code))]
const UNSUPPORTED: &str = "App Store purchases are only available in the iOS app.";

#[derive(Clone)]
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
struct AppleSession {
    /// Short-lived (1h) server token; sent in request bodies, never persisted.
    app_token: String,
    /// StoreKit `appAccountToken` for purchases made while signed in.
    app_account_token: String,
    email: Option<String>,
}

/// The Apple account signed in during this app run, if any. Memory only:
/// a restart signs out, which costs one Face ID tap the next time linking
/// is wanted — cheaper than keeping a bearer token on a shop's till.
pub struct AppleAccount(Mutex<Option<AppleSession>>);

impl AppleAccount {
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }

    #[cfg_attr(not(target_os = "ios"), allow(dead_code))]
    fn get(&self) -> Option<AppleSession> {
        self.0.lock().expect("apple account lock poisoned").clone()
    }

    #[cfg_attr(not(target_os = "ios"), allow(dead_code))]
    fn set(&self, session: Option<AppleSession>) {
        *self.0.lock().expect("apple account lock poisoned") = session;
    }

    pub fn info(&self) -> Option<AppleAccountInfo> {
        self.0
            .lock()
            .expect("apple account lock poisoned")
            .as_ref()
            .map(|s| AppleAccountInfo { email: s.email.clone() })
    }

    pub fn sign_out(&self) {
        *self.0.lock().expect("apple account lock poisoned") = None;
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppleAccountInfo {
    /// May be an @privaterelay.appleid.com address.
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IapProduct {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub display_price: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
pub enum PurchaseResult {
    /// Paid, verified, and the licence is now active on this device.
    Activated { state: LicenseState },
    /// Awaiting approval (Ask to Buy). Redeemed automatically once approved.
    Pending,
    Cancelled,
}

#[derive(Deserialize)]
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
struct VerifyResponse {
    license_key: String,
}

#[derive(Deserialize)]
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
struct NonceResponse {
    nonce: String,
}

#[derive(Deserialize)]
#[cfg_attr(not(target_os = "ios"), allow(dead_code))]
struct SignInResponse {
    app_token: String,
    app_account_token: String,
    email: Option<String>,
}

/// Whether redeeming a transaction should replace the licence currently on
/// this device.
#[cfg(target_os = "ios")]
#[derive(Clone, Copy, PartialEq, Eq)]
enum Activate {
    /// An explicit purchase or restore: the user asked for this licence.
    Always,
    /// A background sweep: record and finish it, but never swap out a
    /// licence that is already working.
    IfNotLicensed,
}

/// Products to show on the purchase screen, as the App Store prices them for
/// this user's storefront.
pub async fn products(app: &AppHandle) -> Result<Vec<IapProduct>, String> {
    #[cfg(target_os = "ios")]
    {
        let products = app.storekit().products(&PRODUCT_IDS).await.map_err(|e| e.to_string())?;
        Ok(products
            .into_iter()
            .map(|p| IapProduct {
                id: p.id,
                display_name: p.display_name,
                description: p.description,
                display_price: p.display_price,
            })
            .collect())
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = app;
        Err(UNSUPPORTED.into())
    }
}

pub async fn purchase(
    app: &AppHandle,
    db: &Db,
    license: &LicenseRuntime,
    account: &AppleAccount,
    product_id: &str,
) -> Result<PurchaseResult, String> {
    #[cfg(target_os = "ios")]
    {
        if !PRODUCT_IDS.contains(&product_id) {
            return Err("Unknown licence product.".into());
        }
        let app_account_token = account.get().map(|s| s.app_account_token);
        let outcome = app
            .storekit()
            .purchase(product_id, app_account_token.as_deref())
            .await
            .map_err(|e| e.to_string())?;
        match outcome {
            PurchaseOutcome::Purchased { transaction } => {
                let state = redeem(app, db, license, account, &transaction, Activate::Always)
                    .await
                    .map_err(|e| {
                        format!(
                            "Your payment went through, but the licence could not be activated yet: {e} \
                             It will be retried automatically the next time the app opens, or tap Restore purchases."
                        )
                    })?;
                Ok(PurchaseResult::Activated { state })
            }
            PurchaseOutcome::Pending => Ok(PurchaseResult::Pending),
            PurchaseOutcome::Cancelled => Ok(PurchaseResult::Cancelled),
        }
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = (app, db, license, account, product_id);
        Err(UNSUPPORTED.into())
    }
}

/// "Restore purchases": asks the App Store for this Apple ID's licences
/// and activates the best one here — which is also how an App Store
/// licence moves to a new iPhone/iPad (the server allows that once per
/// cooldown when Apple proves ownership).
pub async fn restore(
    app: &AppHandle,
    db: &Db,
    license: &LicenseRuntime,
    account: &AppleAccount,
) -> Result<LicenseState, String> {
    #[cfg(target_os = "ios")]
    {
        let mut owned = app.storekit().entitlements(true).await.map_err(|e| e.to_string())?;
        owned.retain(|t| PRODUCT_IDS.contains(&t.product_id.as_str()));
        if owned.is_empty() {
            return Err("No Diwan licence was found for this Apple ID. \
                        Make sure you're signed in to the App Store with the Apple ID you bought it with."
                .into());
        }
        owned.sort_by_key(|t| PRODUCT_IDS.iter().position(|id| *id == t.product_id));

        let mut last_error = String::new();
        for transaction in &owned {
            match redeem(app, db, license, account, transaction, Activate::Always).await {
                Ok(state) => return Ok(state),
                Err(e) => last_error = e,
            }
        }
        Err(last_error)
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = (app, db, license, account);
        Err(UNSUPPORTED.into())
    }
}

/// Sign in with Apple → a Diwan account (created on first use, with the
/// same trial a website sign-up gets). Also links any App Store licences
/// this Apple ID already owns to that account. `Ok(None)` = dismissed.
pub async fn sign_in(
    app: &AppHandle,
    license: &LicenseRuntime,
    account: &AppleAccount,
) -> Result<Option<AppleAccountInfo>, String> {
    #[cfg(target_os = "ios")]
    {
        let Some(session) = authenticate(app).await? else {
            return Ok(None);
        };
        account.set(Some(session));

        // Best effort: attach purchases made before signing in. `link_only`
        // stops the server moving a licence's device binding here — linking
        // from a second iPad must not steal the first one's licence.
        if let (Ok(owned), Ok(fingerprint)) =
            (app.storekit().entitlements(false).await, license.device_fingerprint())
        {
            let token = account.get().map(|s| s.app_token);
            for transaction in owned.iter().filter(|t| PRODUCT_IDS.contains(&t.product_id.as_str())) {
                let linked: Result<VerifyResponse, _> =
                    client::post("apple-iap-verify.php", &verify_body(transaction, &fingerprint, token.as_deref(), true))
                        .await;
                if let Err(e) = linked {
                    crate::startup_log::log(&format!("iap: could not link a purchase to the account: {e}"));
                }
            }
        }
        Ok(account.info())
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = (app, license, account);
        Err(UNSUPPORTED.into())
    }
}

/// Deletes the Diwan account behind an Apple sign-in (App Review requires
/// this in-app for any app that can create an account). Always
/// re-authenticates first, as Apple recommends before a destructive
/// account action. `Ok(false)` = the user dismissed the Apple sheet.
///
/// The licence on this device is untouched: a purchase stays valid without
/// the account that was merely linked to it.
pub async fn delete_account(app: &AppHandle, account: &AppleAccount) -> Result<bool, String> {
    #[cfg(target_os = "ios")]
    {
        let Some(session) = authenticate(app).await? else {
            return Ok(false);
        };
        let _: serde_json::Value =
            client::post("delete-account.php", &serde_json::json!({ "app_token": session.app_token }))
                .await
                .map_err(|e| e.to_string())?;
        account.sign_out();
        Ok(true)
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = (app, account);
        Err(UNSUPPORTED.into())
    }
}

/// Launch-time sweep of transactions StoreKit still holds unfinished — a
/// purchase whose activation failed, an Ask to Buy approval, a purchase
/// interrupted by the app closing. A no-op off iOS.
pub fn spawn_unfinished_sweep(app: AppHandle) {
    #[cfg(target_os = "ios")]
    tauri::async_runtime::spawn(async move {
        use tauri::Manager;

        // After the licence refresh's own first run, off the launch path.
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let pending = match app.storekit().unfinished().await {
            Ok(pending) => pending,
            Err(e) => {
                crate::startup_log::log(&format!("iap: could not read unfinished transactions: {e}"));
                return;
            }
        };
        let db = app.state::<Db>();
        let license = app.state::<LicenseRuntime>();
        let account = app.state::<AppleAccount>();
        for transaction in pending.iter().filter(|t| PRODUCT_IDS.contains(&t.product_id.as_str())) {
            match redeem(&app, &db, &license, &account, transaction, Activate::IfNotLicensed).await {
                Ok(_) => crate::startup_log::log("iap: redeemed an unfinished App Store transaction"),
                Err(e) => crate::startup_log::log(&format!("iap: unfinished transaction left for next launch: {e}")),
            }
        }
    });
    #[cfg(not(target_os = "ios"))]
    let _ = app;
}

/// Steps 2–4 of the module doc: server verification → activation → finish.
#[cfg(target_os = "ios")]
async fn redeem(
    app: &AppHandle,
    db: &Db,
    license: &LicenseRuntime,
    account: &AppleAccount,
    transaction: &SignedTransaction,
    activate: Activate,
) -> Result<LicenseState, String> {
    let fingerprint = license.device_fingerprint()?;
    let token = account.get().map(|s| s.app_token);
    let verified: VerifyResponse = client::post(
        "apple-iap-verify.php",
        &verify_body(transaction, &fingerprint, token.as_deref(), false),
    )
    .await
    .map_err(|e| e.to_string())?;

    let state = if activate == Activate::Always || !license.current().allows_billing() {
        license.activate(app, db, &verified.license_key).await.map_err(|e| e.to_string())?
    } else {
        license.current()
    };

    // The licence is safely stored; a failed finish only means StoreKit
    // re-delivers it and the next sweep re-verifies (idempotent) and
    // finishes it then.
    if let Err(e) = app.storekit().finish(&transaction.transaction_id).await {
        crate::startup_log::log(&format!("iap: could not finish transaction: {e}"));
    }
    Ok(state)
}

#[cfg(target_os = "ios")]
fn verify_body(transaction: &SignedTransaction, fingerprint: &str, app_token: Option<&str>, link_only: bool) -> serde_json::Value {
    serde_json::json!({
        "signed_transaction": transaction.signed_transaction,
        "machine_fingerprint": fingerprint,
        "machine_hint": client::machine_hint(),
        "app_token": app_token,
        "link_only": link_only,
    })
}

/// Apple's sheet → identity token → the licence server's app token.
#[cfg(target_os = "ios")]
async fn authenticate(app: &AppHandle) -> Result<Option<AppleSession>, String> {
    let NonceResponse { nonce } = client::get("apple-nonce.php").await.map_err(|e| e.to_string())?;
    let Some(credential) = app.storekit().sign_in_with_apple(&nonce).await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let response: SignInResponse = client::post(
        "apple-sign-in.php",
        &serde_json::json!({
            "client": "app",
            "id_token": credential.identity_token,
            "code": credential.authorization_code,
            "nonce": nonce,
            "name": credential.full_name,
        }),
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(Some(AppleSession {
        app_token: response.app_token,
        app_account_token: response.app_account_token,
        email: response.email,
    }))
}
