//! What the licence's plan lets this installation use.
//!
//! The server signs, alongside the licence status, the plan's name and the
//! module/capability keys it unlocks (`entitlements`), plus the cheapest
//! plan that would unlock each missing key (`upgrade`). Those come out of
//! the same verified payload as everything else in [`super::verify`], so
//! they cannot be edited locally any more than the expiry can.
//!
//! The verified set is held here in memory, replaced whenever the runtime
//! loads or adopts a payload, and checked by `commands::require_module` and
//! `commands::require_capability` on every command — the frontend only
//! renders what [`snapshot`] reports, so editing the WebView changes nothing.
//!
//! `entitlements: None` (a licence issued before plans existed, no licence
//! yet, or one that failed verification) means **unrestricted**: plans never
//! silently take a module away from an existing customer, and a missing or
//! broken licence is already locked by `require_license`.

use std::collections::{BTreeMap, BTreeSet};
#[cfg(not(test))]
use std::sync::RwLock;

use serde::Serialize;

use super::verify::{LicensePayload, PlanInfo};

/// Capability keys checked inside a module (module keys are `db::modules::key`).
/// Must match `Diwan\Plan\PlanCatalog::POS_KEYS` on the server.
pub mod capability {
    /// Stock takes, CSV import and barcode label generation.
    pub const INVENTORY_ADVANCED: &str = "inventory_advanced";
    /// Profit, tax, voids, payment-type and stock-usage reports.
    pub const REPORTS_ADVANCED: &str = "reports_advanced";
    /// More than one staff login (creating users, non-Owner sign-in).
    pub const MULTI_USER: &str = "multi_user";
    /// Running on Android / iOS.
    pub const MOBILE: &str = "mobile";
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entitlements {
    pub plan: Option<PlanInfo>,
    /// `None` = unrestricted.
    pub keys: Option<BTreeSet<String>>,
    pub upgrade: BTreeMap<String, String>,
}

impl Entitlements {
    pub fn from_payload(payload: &LicensePayload) -> Self {
        Self {
            plan: payload.plan.clone(),
            keys: payload.entitlements.as_ref().map(|keys| keys.iter().cloned().collect()),
            upgrade: payload.upgrade.clone(),
        }
    }

    pub fn allows(&self, key: &str) -> bool {
        self.keys.as_ref().is_none_or(|keys| keys.contains(key))
    }

    /// `None` when any of `keys` is entitled (the same "one is enough" rule
    /// as `modules::require_enabled`), otherwise the message to show.
    pub fn block_reason(&self, keys: &[&str]) -> Option<String> {
        if keys.iter().any(|key| self.allows(key)) {
            return None;
        }
        let plan = self.plan.as_ref().map_or("your", |p| p.name.as_str());
        Some(match keys.iter().find_map(|key| self.upgrade.get(*key)) {
            Some(needed) => format!(
                "This feature is not part of {plan} plan. It is included in {needed} — ask the Diwan team to upgrade."
            ),
            None => format!("This feature is not part of {plan} plan. Contact the Diwan team to change your plan."),
        })
    }
}

#[cfg(not(test))]
static CURRENT: RwLock<Option<Entitlements>> = RwLock::new(None);

/// Replaces the process-wide set. `None` = unrestricted (see module doc).
#[cfg(not(test))]
pub fn install(next: Option<Entitlements>) {
    *CURRENT.write().unwrap_or_else(|e| e.into_inner()) = next;
}

#[cfg(not(test))]
fn with_current<T>(f: impl FnOnce(&Entitlements) -> T, unrestricted: T) -> T {
    match CURRENT.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
        Some(entitlements) => f(entitlements),
        None => unrestricted,
    }
}

// Tests run in parallel threads; a per-thread set lets a test install a plan
// and exercise the real command guards without affecting any other test.
#[cfg(test)]
thread_local! {
    static CURRENT: std::cell::RefCell<Option<Entitlements>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub fn install(next: Option<Entitlements>) {
    CURRENT.with(|current| *current.borrow_mut() = next);
}

#[cfg(test)]
fn with_current<T>(f: impl FnOnce(&Entitlements) -> T, unrestricted: T) -> T {
    CURRENT.with(|current| match current.borrow().as_ref() {
        Some(entitlements) => f(entitlements),
        None => unrestricted,
    })
}

/// `Ok` when the current plan unlocks any one of `keys`.
pub fn require_any(keys: &[&str]) -> Result<(), String> {
    match with_current(|e| e.block_reason(keys), None) {
        None => Ok(()),
        Some(reason) => Err(reason),
    }
}

pub fn allows(key: &str) -> bool {
    with_current(|e| e.allows(key), true)
}

/// The plan name that unlocks `key`, if the current plan lacks it.
pub fn upgrade_for(key: &str) -> Option<String> {
    with_current(|e| if e.allows(key) { None } else { e.upgrade.get(key).cloned() }, None)
}

/// What the frontend shows: the plan, whether anything is restricted, and
/// which keys are unlocked. Presentation only — commands re-check.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub restricted: bool,
    pub plan_code: Option<String>,
    pub plan_name: Option<String>,
    pub entitlements: Vec<String>,
    pub upgrade: BTreeMap<String, String>,
}

pub fn snapshot() -> Snapshot {
    with_current(
        |e| Snapshot {
            restricted: e.keys.is_some(),
            plan_code: e.plan.as_ref().map(|p| p.code.clone()),
            plan_name: e.plan.as_ref().map(|p| p.name.clone()),
            entitlements: e.keys.iter().flatten().cloned().collect(),
            upgrade: e.upgrade.clone(),
        },
        Snapshot { restricted: false, plan_code: None, plan_name: None, entitlements: vec![], upgrade: BTreeMap::new() },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn basic() -> Entitlements {
        Entitlements {
            plan: Some(PlanInfo { code: "basic".into(), name: "BASIC".into() }),
            keys: Some(["billing", "inventory", "reports"].into_iter().map(String::from).collect()),
            upgrade: [("purchasing", "ADVANCE"), ("mobile", "ALL-IN-ONE")]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    #[test]
    fn a_restricted_plan_allows_only_its_keys() {
        let e = basic();
        assert!(e.allows("billing"));
        assert!(!e.allows("purchasing"));
        assert_eq!(e.block_reason(&["inventory"]), None);
        assert_eq!(e.block_reason(&["purchasing", "billing"]), None, "any one key is enough");
    }

    #[test]
    fn a_refusal_names_the_plan_that_includes_the_feature() {
        let reason = basic().block_reason(&["purchasing"]).unwrap();
        assert!(reason.contains("BASIC") && reason.contains("ADVANCE"), "{reason}");
        let unknown = basic().block_reason(&["ledger"]).unwrap();
        assert!(unknown.contains("Contact the Diwan team"), "{unknown}");
    }

    #[test]
    fn no_entitlement_list_means_unrestricted() {
        let legacy = Entitlements::default();
        assert!(legacy.allows("purchasing"));
        assert_eq!(legacy.block_reason(&["anything"]), None);
    }
}
