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

/// The build-time override, if any.
const BUILD_API_BASE: Option<&str> = option_env!("DIWAN_LICENSE_API_BASE");

#[cfg_attr(debug_assertions, allow(dead_code))] // only the release-build check below uses it
const fn is_https(url: &str) -> bool {
    let bytes = url.as_bytes();
    let prefix = b"https://";
    if bytes.len() < prefix.len() {
        return false;
    }
    let mut i = 0;
    while i < prefix.len() {
        if bytes[i] != prefix[i] {
            return false;
        }
        i += 1;
    }
    true
}

// A release build must never ship pointing licence checks at plaintext
// HTTP (signatures would still verify, but key and fingerprint would travel
// in the clear). Debug builds may — that's how a local `php -S` is tested.
#[cfg(not(debug_assertions))]
const _: () = match BUILD_API_BASE {
    Some(base) => assert!(
        base.is_empty() || is_https(base),
        "DIWAN_LICENSE_API_BASE must be an https:// URL in a release build"
    ),
    None => {}
};

pub fn api_base() -> String {
    #[cfg(debug_assertions)]
    if let Ok(runtime) = std::env::var("DIWAN_LICENSE_API_BASE") {
        if !runtime.trim().is_empty() {
            return runtime.trim().trim_end_matches('/').to_string();
        }
    }
    BUILD_API_BASE
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(DEFAULT_API_BASE)
        .trim_end_matches('/')
        .to_string()
}

/// Short on purpose: this runs in the background on every launch and a
/// shop with flaky internet must not sit on it. Anything slower is treated
/// as offline and retried on the next tick.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ClientError {
    /// Couldn't reach the server, or it didn't answer in time. The caller
    /// keeps whatever it already has — this is the "offline" path and is
    /// never surfaced as a licence problem.
    #[error("Could not reach the licence server ({0})")]
    Offline(String),
    /// The server answered with a deliberate refusal (4xx) and a message.
    #[error("{message}")]
    Refused { status: u16, message: String },
    /// The server answered but the body wasn't what this build expects
    /// (5xx, HTML from a misconfigured host, malformed JSON). Treated like
    /// offline by callers — it's the server's problem, not the licence's.
    #[error("The licence server returned an unexpected response ({0})")]
    Server(String),
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
            let builder = reqwest::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .user_agent(concat!("Diwan/", env!("CARGO_PKG_VERSION")));
            #[cfg(target_os = "android")]
            let builder = with_bundled_roots(builder);
            builder.build().map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| ClientError::Server(e.clone()))
}

/// Android verifies the licence server against Mozilla's root set compiled
/// into the binary instead of the OS verifier. rustls-platform-verifier's
/// Android path (JNI init, a Kotlin class that has to survive R8, the
/// device's trust store, a PKIX revocation check against a certificate
/// that — like every Let's Encrypt one since 2025 — has no OCSP URL) failed
/// on real phones, and every failure in it surfaced as a *connect* error:
/// "check the internet connection" on a phone that was online. Nothing is
/// lost by skipping the OS store here: the licence payload is Ed25519-signed
/// and verified regardless of transport, and a shop's phone has no reason
/// to trust a user-installed CA for this one host.
#[cfg(any(target_os = "android", test))]
fn with_bundled_roots(builder: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    builder.tls_certs_only(
        webpki_root_certs::TLS_SERVER_ROOT_CERTS
            .iter()
            .filter_map(|der| reqwest::Certificate::from_der(der.as_ref()).ok()),
    )
}

/// The same client (TLS set-up, user agent) and error wording for the other
/// API this app talks to: the business workspace (`workspace::client`).
pub(crate) fn shared_client() -> Result<&'static reqwest::Client, ClientError> {
    client()
}

pub(crate) fn transport_error(e: reqwest::Error) -> String {
    describe_transport_error(e)
}

pub(crate) fn clean_server_message(message: &str) -> String {
    sanitize_server_message(message)
}

/// POSTs JSON to `<api base>/<path>`. Shared with the App Store flow
/// (`iap.rs`), which talks to the same server with the same error rules.
pub(crate) async fn post<T: serde::de::DeserializeOwned>(path: &str, body: &serde_json::Value) -> Result<T, ClientError> {
    let url = format!("{}/{path}", api_base());
    send(client()?.post(&url).json(body)).await
}

pub(crate) async fn post_with_timeout<T: serde::de::DeserializeOwned>(
    path: &str,
    body: &serde_json::Value,
    timeout: Duration,
) -> Result<T, ClientError> {
    let url = format!("{}/{path}", api_base());
    send(client()?.post(&url).timeout(timeout).json(body)).await
}

#[cfg_attr(not(target_os = "ios"), allow(dead_code))] // only the App Store flow GETs
pub(crate) async fn get<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, ClientError> {
    let url = format!("{}/{path}", api_base());
    send(client()?.get(&url)).await
}

async fn send<T: serde::de::DeserializeOwned>(request: reqwest::RequestBuilder) -> Result<T, ClientError> {
    let response = request
        .send()
        .await
        .map_err(|e| ClientError::Offline(describe_transport_error(e)))?;

    let status = response.status();
    let text = response.text().await.map_err(|e| ClientError::Offline(e.without_url().to_string()))?;

    if status.is_success() {
        return serde_json::from_str(&text).map_err(|e| ClientError::Server(format!("bad JSON: {e}")));
    }
    if status.is_client_error() {
        // Shown to the user verbatim, so keep it to one short line of
        // printable text whatever the server (or a proxy) sent.
        let message = serde_json::from_str::<ErrorBody>(&text)
            .map(|b| sanitize_server_message(&b.error))
            .ok()
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
        return Err(ClientError::Refused { status: status.as_u16(), message });
    }
    Err(ClientError::Server(format!("HTTP {}", status.as_u16())))
}

/// One line of printable text, clipped like any other external text.
fn sanitize_server_message(message: &str) -> String {
    let flattened: String = message
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    crate::startup_log::clip(flattened.split_whitespace().collect::<Vec<_>>().join(" ").as_str())
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
        return "the server took too long to answer".into();
    }
    // A certificate failure is also a "connect" error to reqwest; saying
    // "check the internet connection" for it sent owners chasing their wifi.
    if let Some(tls) = find_tls_error(&e) {
        return format!("the secure connection to the server failed ({tls})");
    }
    if e.is_connect() {
        format!("check the internet connection ({})", root_cause(&e))
    } else {
        e.without_url().to_string()
    }
}

/// hyper wraps the handshake's `rustls::Error` in an `io::Error`, whose
/// `source()` skips straight past it — hence the `get_ref` peek.
fn find_tls_error(e: &(dyn std::error::Error + 'static)) -> Option<String> {
    let mut current = Some(e);
    while let Some(err) = current {
        if let Some(tls) = err.downcast_ref::<rustls::Error>() {
            return Some(tls.to_string());
        }
        if let Some(tls) = err
            .downcast_ref::<std::io::Error>()
            .and_then(|io| io.get_ref())
            .and_then(|inner| inner.downcast_ref::<rustls::Error>())
        {
            return Some(tls.to_string());
        }
        current = err.source();
    }
    None
}

fn root_cause(e: &(dyn std::error::Error + 'static)) -> String {
    let mut current = e;
    while let Some(next) = current.source() {
        current = next;
    }
    current.to_string()
}

/// For the vendor's dashboard only ("Windows x86_64") — never anything
/// identifying. The server truncates and strips it further.
pub(crate) fn machine_hint() -> String {
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

    #[test]
    fn is_https_accepts_only_the_https_scheme() {
        assert!(is_https("https://example.com/api"));
        assert!(!is_https("http://example.com/api"));
        assert!(!is_https("https:/"));
        assert!(!is_https(""));
    }

    #[test]
    fn server_messages_are_flattened_to_one_clipped_line() {
        assert_eq!(sanitize_server_message("  Key\r\n revoked\t now "), "Key revoked now");
        let long = "x".repeat(5_000);
        assert!(sanitize_server_message(&long).chars().count() < 1_000);
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
        // The status, not the server's wording, is the contract.
        assert!(matches!(result, Err(ClientError::Refused { status: 403, .. })), "{result:?}");
        let result = rt.block_on(activate("DIWAN-ZZZZ-ZZZZ-ZZZZ-ZZZZ", &fp));
        assert!(matches!(result, Err(ClientError::Refused { status: 403, .. })), "{result:?}");
        // and a dead port is Offline, not Refused
        let previous = std::env::var("DIWAN_LICENSE_API_BASE").ok();
        std::env::set_var("DIWAN_LICENSE_API_BASE", "http://127.0.0.1:9");
        let offline = rt.block_on(fetch_status("X", &fp));
        // Restore before asserting, so a failure can't leak the dead port
        // into other tests in this process.
        match previous {
            Some(value) => std::env::set_var("DIWAN_LICENSE_API_BASE", value),
            None => std::env::remove_var("DIWAN_LICENSE_API_BASE"),
        }
        assert!(matches!(offline, Err(ClientError::Offline(_))), "{offline:?}");
    }

    /// The Android TLS setup against the real production server —
    /// `cargo test live_bundled_roots -- --ignored`. Reaching a 403 proves
    /// the handshake verified with only the compiled-in roots.
    #[test]
    #[ignore]
    fn live_bundled_roots_verify_the_production_server() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let status = rt.block_on(async {
            let client = with_bundled_roots(reqwest::Client::builder().timeout(REQUEST_TIMEOUT)).build().unwrap();
            client
                .post(format!("{DEFAULT_API_BASE}/license-status.php"))
                .json(&serde_json::json!({ "license_key": "DIWAN-ZZZZ-ZZZZ-ZZZZ-ZZZZ", "machine_fingerprint": "ab".repeat(32) }))
                .send()
                .await
                .map(|r| r.status().as_u16())
                .map_err(describe_transport_error)
        });
        assert_eq!(status, Ok(403));
    }

    #[test]
    fn the_hint_is_short_and_ascii() {
        let hint = machine_hint();
        assert!(hint.len() <= 64 && hint.is_ascii());
    }
}
