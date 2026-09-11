//! The two HTTP calls to the licence server. Nothing here trusts the
//! response beyond its status code: a signed payload is handed straight to
//! `verify::verify_signed` by the caller; an unsigned error body is only
//! ever shown to the user.

use std::sync::OnceLock;
use std::time::Duration;

use serde::Deserialize;

/// Where the server lives. Overridable at build time
/// (`DIWAN_LICENSE_API_BASE=https://… cargo tauri build`) so CI can point a
/// staging build at staging without a code change, and — in debug builds
/// only — at run time via the same variable, so a dev can test against a
/// local `php -S`. A release binary ignores the runtime variable: an
/// end-user setting an env var must not be able to redirect licence checks
/// to a server of their choosing. (They still couldn't *forge* a payload —
/// the signature check doesn't care where bytes came from — but there's no
/// reason to offer the knob.)
const DEFAULT_API_BASE: &str = "https://diwan.codehunts.co.uk/api";

pub fn api_base() -> String {
    #[cfg(debug_assertions)]
    if let Ok(runtime) = std::env::var("DIWAN_LICENSE_API_BASE") {
        if !runtime.trim().is_empty() {
            return runtime.trim().trim_end_matches('/').to_string();
        }
    }
    option_env!("DIWAN_LICENSE_API_BASE")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(DEFAULT_API_BASE)
        .trim_end_matches('/')
        .to_string()
}

/// Short on purpose: this runs in the background on every launch and a
/// shop with flaky internet must not sit on it. Anything slower is treated
/// as offline and retried on the next tick.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, PartialEq, Eq)]
pub enum ClientError {
    /// Couldn't reach the server, or it didn't answer in time. The caller
    /// keeps whatever it already has — this is the "offline" path and is
    /// never surfaced as a licence problem.
    Offline(String),
    /// The server answered with a deliberate refusal (4xx) and a message.
    Refused { status: u16, message: String },
    /// The server answered but the body wasn't what this build expects
    /// (5xx, HTML from a misconfigured host, malformed JSON). Treated like
    /// offline by callers — it's the server's problem, not the licence's.
    Server(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Offline(msg) => write!(f, "Could not reach the licence server ({msg})"),
            ClientError::Refused { message, .. } => write!(f, "{message}"),
            ClientError::Server(msg) => write!(f, "The licence server returned an unexpected response ({msg})"),
        }
    }
}

/// `license-status.php`'s successful body, verbatim. The payload and
/// signature are opaque strings here on purpose.
#[derive(Debug, Clone, Deserialize)]
pub struct SignedStatus {
    pub payload: String,
    pub signature: String,
    pub kid: String,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: String,
}

fn client() -> Result<&'static reqwest::Client, ClientError> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            // Same provider tauri's own `rustls-tls` feature installs; a
            // second `install_default` (if tauri already did) just reports
            // that one is present, which is fine.
            let _ = rustls::crypto::ring::default_provider().install_default();
            reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .user_agent(concat!("Diwan/", env!("CARGO_PKG_VERSION")))
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| ClientError::Server(e.clone()))
}

async fn post<T: serde::de::DeserializeOwned>(path: &str, body: &serde_json::Value) -> Result<T, ClientError> {
    let url = format!("{}/{path}", api_base());
    let response = client()?
        .post(&url)
        .json(body)
        .send()
        .await
        .map_err(|e| ClientError::Offline(describe_transport_error(e)))?;

    let status = response.status();
    let text = response.text().await.map_err(|e| ClientError::Offline(e.without_url().to_string()))?;

    if status.is_success() {
        return serde_json::from_str(&text).map_err(|e| ClientError::Server(format!("bad JSON: {e}")));
    }
    if status.is_client_error() {
        let message = serde_json::from_str::<ErrorBody>(&text)
            .map(|b| b.error)
            .unwrap_or_else(|_| format!("HTTP {}", status.as_u16()));
        return Err(ClientError::Refused { status: status.as_u16(), message });
    }
    Err(ClientError::Server(format!("HTTP {}", status.as_u16())))
}

/// `POST activate-license.php`. Success means the key is now bound to this
/// fingerprint (or already was). A 409 means it's bound to a different one.
pub async fn activate(license_key: &str, fingerprint: &str) -> Result<(), ClientError> {
    #[derive(Deserialize)]
    struct Ok {
        #[allow(dead_code)]
        status: String,
    }
    post::<Ok>(
        "activate-license.php",
        &serde_json::json!({
            "license_key": license_key,
            "machine_fingerprint": fingerprint,
            "machine_hint": machine_hint(),
        }),
    )
    .await
    .map(|_| ())
}

/// `POST license-status.php`. The returned strings are unverified.
pub async fn fetch_status(license_key: &str, fingerprint: &str) -> Result<SignedStatus, ClientError> {
    post(
        "license-status.php",
        &serde_json::json!({ "license_key": license_key, "machine_fingerprint": fingerprint }),
    )
    .await
}

/// reqwest's own messages ("error sending request") mean nothing to a
/// shopkeeper; these are the words the lock screen shows.
fn describe_transport_error(e: reqwest::Error) -> String {
    if e.is_timeout() {
        "the server took too long to answer".into()
    } else if e.is_connect() {
        "check the internet connection".into()
    } else {
        e.without_url().to_string()
    }
}

/// For the vendor's dashboard only ("Windows x86_64") — never anything
/// identifying. The server truncates and strips it further.
fn machine_hint() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_base_has_no_trailing_slash_and_is_https() {
        // A trailing slash would produce `//license-status.php`, which
        // Apache happily serves but the .htaccess HTTPS redirect mangles.
        std::env::remove_var("DIWAN_LICENSE_API_BASE");
        let base = api_base();
        assert!(base.starts_with("https://"), "{base}");
        assert!(!base.ends_with('/'));
    }

    /// Live round-trip against a running server — `cargo test -- --ignored
    /// live_server` with `DIWAN_LICENSE_API_BASE` pointing at a local
    /// `php -S` of the backend. A key that cannot exist must come back as
    /// the server's generic 403, parsed out of its JSON error body — this
    /// pins the request shape, the error-body parsing, and that a refusal
    /// is never mistaken for "offline".
    #[test]
    #[ignore]
    fn live_server_refuses_a_bogus_key_with_403() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let fp = "ab".repeat(32);
        let result = rt.block_on(fetch_status("DIWAN-ZZZZ-ZZZZ-ZZZZ-ZZZZ", &fp));
        assert_eq!(
            result.map(|_| ()),
            Err(ClientError::Refused { status: 403, message: "That licence key is not valid.".into() })
        );
        let result = rt.block_on(activate("DIWAN-ZZZZ-ZZZZ-ZZZZ-ZZZZ", &fp));
        assert!(matches!(result, Err(ClientError::Refused { status: 403, .. })), "{result:?}");
        // and a dead port is Offline, not Refused
        std::env::set_var("DIWAN_LICENSE_API_BASE", "http://127.0.0.1:9");
        assert!(matches!(rt.block_on(fetch_status("X", &fp)), Err(ClientError::Offline(_))));
    }

    #[test]
    fn the_hint_is_short_and_ascii() {
        let hint = machine_hint();
        assert!(hint.len() <= 64 && hint.is_ascii());
    }
}
