//! HTTP calls to the business workspace API (`pos-*.php` on the licence
//! server). Same server, TLS set-up and wording as `license::client`; the
//! counter authenticates with its own token in `X-Terminal-Token` (shared
//! hosting drops `Authorization` before PHP sees it).

use std::collections::BTreeMap;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::engine::{PullPage, PushResponse};
use crate::license::client::{api_base, clean_server_message, shared_client, transport_error};

const TOKEN_HEADER: &str = "X-Terminal-Token";

#[derive(Debug, Clone, thiserror::Error)]
pub enum WsError {
    /// No answer (offline, timeout, TLS). Everything stays queued.
    #[error("Could not reach the server ({0})")]
    Offline(String),
    /// A deliberate refusal; `code` is the server's machine-readable reason.
    #[error("{message}")]
    Refused { status: u16, code: String, message: String },
    #[error("The server returned an unexpected response ({0})")]
    Server(String),
}

impl WsError {
    pub fn code(&self) -> Option<&str> {
        match self {
            WsError::Refused { code, .. } => Some(code),
            _ => None,
        }
    }
}

#[derive(Deserialize)]
struct ErrorBody {
    error: String,
    #[serde(default)]
    code: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RemoteBusiness {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub has_data: bool,
    #[serde(default)]
    pub seeding: bool,
    #[serde(default)]
    pub counters: i64,
    #[serde(default)]
    pub max_counters: i64,
    #[serde(default)]
    pub plan: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RemoteCounter {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub install_id: String,
    #[serde(default)]
    pub joined: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Registration {
    pub token: String,
    pub terminal: RemoteCounter,
    pub business: RemoteBusiness,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceStatus {
    pub terminal: RemoteCounter,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Shortage {
    pub item_gid: String,
    pub available: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Reservation {
    pub status: String,
    #[serde(default)]
    pub taken: BTreeMap<String, f64>,
    #[serde(default)]
    pub balances: BTreeMap<String, f64>,
    #[serde(default)]
    pub shortages: Vec<Shortage>,
}

async fn send<T: DeserializeOwned>(request: reqwest::RequestBuilder) -> Result<T, WsError> {
    let response = request.send().await.map_err(|e| WsError::Offline(transport_error(e)))?;
    let status = response.status();
    let text = response.text().await.map_err(|e| WsError::Offline(e.without_url().to_string()))?;
    if status.is_success() {
        return serde_json::from_str(&text).map_err(|e| WsError::Server(format!("bad JSON: {e}")));
    }
    if status.is_client_error() {
        let body = serde_json::from_str::<ErrorBody>(&text).ok();
        return Err(WsError::Refused {
            status: status.as_u16(),
            code: body.as_ref().and_then(|b| b.code.clone()).unwrap_or_default(),
            message: body
                .map(|b| clean_server_message(&b.error))
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| format!("HTTP {}", status.as_u16())),
        });
    }
    Err(WsError::Server(format!("HTTP {}", status.as_u16())))
}

fn client() -> Result<&'static reqwest::Client, WsError> {
    shared_client().map_err(|e| WsError::Server(e.to_string()))
}

fn url(path: &str) -> String {
    format!("{}/{path}", api_base())
}

pub async fn register(
    license_key: &str,
    fingerprint: &str,
    install_id: &str,
    name: Option<&str>,
) -> Result<Registration, WsError> {
    send(client()?.post(url("pos-terminal-register.php")).json(&json!({
        "license_key": license_key,
        "machine_fingerprint": fingerprint,
        "install_id": install_id,
        "terminal_name": name,
        "platform": std::env::consts::OS,
        "app_version": env!("CARGO_PKG_VERSION"),
    })))
    .await
}

/// `claim_seed` | `complete_seed` | `joined` | `rename`.
pub async fn action(token: &str, action: &str, name: Option<&str>) -> Result<WorkspaceStatus, WsError> {
    send(client()?.post(url("pos-workspace.php")).header(TOKEN_HEADER, token).json(&json!({ "action": action, "name": name }))).await
}

pub async fn push(token: &str, body: &Value) -> Result<PushResponse, WsError> {
    send(client()?.post(url("pos-sync-push.php")).header(TOKEN_HEADER, token).timeout(Duration::from_secs(60)).json(body)).await
}

pub async fn pull(token: &str, since: i64, limit: i64) -> Result<PullPage, WsError> {
    send(
        client()?
            .get(url(&format!("pos-sync-pull.php?since={since}&limit={limit}")))
            .header(TOKEN_HEADER, token)
            .timeout(Duration::from_secs(60)),
    )
    .await
}

/// `pull`, also telling the server what is still waiting here and the last
/// sync error, for the admin panel's counter list.
pub async fn pull_reporting(token: &str, since: i64, limit: i64, pending: i64, last_error: Option<&str>) -> Result<PullPage, WsError> {
    let error: String = last_error.unwrap_or_default().chars().take(200).collect();
    let url = reqwest::Url::parse_with_params(
        &url("pos-sync-pull.php"),
        &[("since", since.to_string()), ("limit", limit.to_string()), ("pending", pending.to_string()), ("error", error)],
    )
    .map_err(|e| WsError::Server(e.to_string()))?;
    send(client()?.get(url).header(TOKEN_HEADER, token).timeout(Duration::from_secs(60))).await
}

/// Short timeout: a cashier is waiting, and no answer means "record the sale
/// offline", not "fail the sale".
pub async fn reserve(token: &str, key: &str, lines: &[(String, f64)]) -> Result<Reservation, WsError> {
    let lines: Vec<Value> = lines.iter().map(|(gid, qty)| json!({ "item_gid": gid, "qty": qty })).collect();
    send(
        client()?
            .post(url("pos-stock-reserve.php"))
            .header(TOKEN_HEADER, token)
            .timeout(Duration::from_secs(4))
            .json(&json!({ "key": key, "lines": lines })),
    )
    .await
}

#[derive(Deserialize)]
struct MediaCheck {
    missing: Vec<String>,
}

/// Which of `names` the business's media store does not have.
pub async fn media_missing(token: &str, names: &[String]) -> Result<Vec<String>, WsError> {
    let check: MediaCheck =
        send(client()?.post(url("pos-media.php")).header(TOKEN_HEADER, token).json(&json!({ "names": names }))).await?;
    Ok(check.missing)
}

pub async fn media_upload(token: &str, name: &str, bytes: Vec<u8>) -> Result<(), WsError> {
    let url = reqwest::Url::parse_with_params(&url("pos-media-upload.php"), &[("name", name)]).map_err(|e| WsError::Server(e.to_string()))?;
    let _: Value = send(
        client()?
            .post(url)
            .header(TOKEN_HEADER, token)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .timeout(Duration::from_secs(120))
            .body(bytes),
    )
    .await?;
    Ok(())
}

/// The file's bytes, or `None` when the business does not have it (yet).
pub async fn media_fetch(token: &str, name: &str) -> Result<Option<Vec<u8>>, WsError> {
    let url = reqwest::Url::parse_with_params(&url("pos-media.php"), &[("name", name)]).map_err(|e| WsError::Server(e.to_string()))?;
    let response = client()?
        .get(url)
        .header(TOKEN_HEADER, token)
        .timeout(Duration::from_secs(120))
        .send()
        .await
        .map_err(|e| WsError::Offline(transport_error(e)))?;
    match response.status().as_u16() {
        200 => Ok(Some(response.bytes().await.map_err(|e| WsError::Offline(e.without_url().to_string()))?.to_vec())),
        404 => Ok(None),
        status if status < 500 => {
            Err(WsError::Refused { status, code: String::new(), message: format!("The photo could not be downloaded (HTTP {status})") })
        }
        status => Err(WsError::Server(format!("HTTP {status}"))),
    }
}
