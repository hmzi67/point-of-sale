//! Counters on the same network nudge each other: after a counter sends a
//! change it broadcasts a tiny UDP datagram, and the other counters of the
//! same business that hear it sync at once instead of waiting for their next
//! poll. The server stays the only source of data; a nudge carries none, so
//! a lost, blocked or forged one only means a sync happens a little later
//! (or a little sooner). Desktop only: phones need extra network
//! permissions for broadcasts, and poll instead.
#![cfg_attr(any(target_os = "android", target_os = "ios"), allow(dead_code))] // phones only poll

use std::sync::Mutex;

use sha2::{Digest, Sha256};

pub const PORT: u16 = 47_291;
const MAGIC: &str = "DIWAN-NUDGE1";

/// `(business tag, this install id)` while connected.
static IDENTITY: Mutex<Option<(String, String)>> = Mutex::new(None);

/// A tag that names the business without revealing its id.
fn business_tag(business_id: i64) -> String {
    let digest = Sha256::digest(format!("diwan-lan:{business_id}").as_bytes());
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// Remembers which business this counter belongs to (None: not connected).
pub fn set_identity(business_id: Option<i64>, install_id: &str) {
    if let Ok(mut identity) = IDENTITY.lock() {
        *identity = business_id.map(|id| (business_tag(id), install_id.to_string()));
    }
}

fn identity() -> Option<(String, String)> {
    IDENTITY.lock().ok().and_then(|i| i.clone())
}

/// Whether `packet` is a nudge from another counter of this business.
fn is_nudge_for(packet: &[u8], tag: &str, own_install: &str) -> bool {
    let Ok(text) = std::str::from_utf8(packet) else { return false };
    let mut parts = text.split(' ');
    parts.next() == Some(MAGIC) && parts.next() == Some(tag) && parts.next().is_some_and(|from| from != own_install && !from.is_empty())
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
mod net {
    use std::net::UdpSocket;
    use std::time::{Duration, Instant};

    use super::*;

    /// Tells the other counters on this network to sync now.
    pub fn announce() {
        let Some((tag, install)) = identity() else { return };
        let Ok(socket) = UdpSocket::bind(("0.0.0.0", 0)) else { return };
        if socket.set_broadcast(true).is_ok() {
            let _ = socket.send_to(format!("{MAGIC} {tag} {install}").as_bytes(), ("255.255.255.255", PORT));
        }
    }

    /// Listens for nudges on a thread of its own and calls `on_nudge` for
    /// each one from this business, at most once a second. If the port is
    /// taken (a second copy of the app) or blocked, counters simply poll.
    pub fn spawn_listener(on_nudge: impl Fn() + Send + 'static) {
        let spawned = std::thread::Builder::new().name("workspace-lan".into()).spawn(move || {
            let socket = match UdpSocket::bind(("0.0.0.0", PORT)) {
                Ok(socket) => socket,
                Err(e) => {
                    crate::startup_log::log(&format!("workspace: same-network nudges off ({e})"));
                    return;
                }
            };
            let mut buffer = [0u8; 128];
            let mut last = Instant::now() - Duration::from_secs(5);
            loop {
                let Ok((len, _)) = socket.recv_from(&mut buffer) else { continue };
                let Some((tag, install)) = identity() else { continue };
                if is_nudge_for(&buffer[..len], &tag, &install) && last.elapsed() >= Duration::from_secs(1) {
                    last = Instant::now();
                    on_nudge();
                }
            }
        });
        if let Err(e) = spawned {
            crate::startup_log::warn(&format!("workspace: could not start the network listener: {e}"));
        }
    }
}

#[cfg(any(target_os = "android", target_os = "ios"))]
mod net {
    pub fn announce() {}
    pub fn spawn_listener(_on_nudge: impl Fn() + Send + 'static) {}
}

pub use net::{announce, spawn_listener};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_nudge_from_another_counter_of_the_same_business_counts() {
        let tag = business_tag(7);
        assert_ne!(tag, business_tag(8));
        let packet = format!("{MAGIC} {tag} aaaa");
        assert!(is_nudge_for(packet.as_bytes(), &tag, "bbbb"));
        assert!(!is_nudge_for(packet.as_bytes(), &tag, "aaaa"), "not our own");
        assert!(!is_nudge_for(packet.as_bytes(), &business_tag(8), "bbbb"), "not another business");
        assert!(!is_nudge_for(b"hello", &tag, "bbbb"));
        assert!(!is_nudge_for(&[0xff, 0xfe], &tag, "bbbb"));
    }
}
