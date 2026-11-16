//! Metadata-only nearby pairing. An endpoint is trusted only after bilateral consent.
use super::{
    network::{self, Event, Peer, Shared},
    store::err,
};
use iroh::{Endpoint, EndpointAddr, EndpointId, SecretKey, endpoint::Connection};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};
use tokio::sync::{oneshot, watch};

pub const ALPN: &[u8] = b"munin/pair/1";
pub const REQUEST_LIFE: Duration = Duration::from_secs(90);
const HELLO_LIMIT: usize = 16 * 1024;
#[derive(Clone)]
pub struct Request {
    pub id: String,
    pub peer: Peer,
    pub fingerprint: String,
    pub outgoing: bool,
    pub expires: Instant,
}
struct Pending {
    request: Request,
    decision: oneshot::Sender<bool>,
}
#[derive(Default)]
struct Registry {
    enabled: bool,
    pending: BTreeMap<String, Pending>,
}
#[derive(Clone)]
pub struct Pairing(Arc<Mutex<Registry>>, watch::Sender<u64>);
impl Default for Pairing {
    fn default() -> Self {
        Self::new(false)
    }
}
impl Pairing {
    pub fn new(enabled: bool) -> Self {
        let (signal, _) = watch::channel(0);
        Self(
            Arc::new(Mutex::new(Registry {
                enabled,
                ..Default::default()
            })),
            signal,
        )
    }
    pub fn start(&self) {
        self.0.lock().unwrap().enabled = true;
        self.1.send_modify(|n| *n = n.wrapping_add(1));
    }
    pub fn enabled(&self) -> bool {
        self.0.lock().unwrap().enabled
    }
    pub fn stop(&self) {
        let mut r = self.0.lock().unwrap();
        r.enabled = false;
        r.pending.clear();
        self.1.send_modify(|n| *n = n.wrapping_add(1));
    }
    pub fn register(&self, request: Request) -> io::Result<oneshot::Receiver<bool>> {
        let mut r = self.0.lock().unwrap();
        r.pending.retain(|_, p| p.request.expires > Instant::now());
        if !r.enabled || request.expires <= Instant::now() {
            return Err(err("Nearby pairing is stopped or this request expired"));
        }
        if r.pending.len() >= 8
            || r.pending.contains_key(&request.id)
            || r.pending
                .values()
                .any(|p| p.request.peer.addr.id == request.peer.addr.id)
        {
            return Err(err(
                "A pairing request is already pending, or the request limit was reached",
            ));
        }
        let (decision, rx) = oneshot::channel();
        r.pending
            .insert(request.id.clone(), Pending { request, decision });
        Ok(rx)
    }
    pub fn decide(&self, id: &str, accept: bool) -> io::Result<()> {
        let mut r = self.0.lock().unwrap();
        let p = r
            .pending
            .remove(id)
            .ok_or_else(|| err("Pairing request expired or already answered"))?;
        if !r.enabled || p.request.expires <= Instant::now() {
            return Err(err("Pairing request expired"));
        }
        p.decision
            .send(accept)
            .map_err(|_| err("The other device closed this request"))
    }
    fn cancel(&self, id: &str) {
        self.0.lock().unwrap().pending.remove(id);
    }
}

/// Session identity is 256 bits; the human comparison displays 48 bits of it.
pub fn verification(a: EndpointId, na: &str, b: EndpointId, nb: &str) -> (String, String) {
    let ((a, na), (b, nb)) = if a < b {
        ((a, na), (b, nb))
    } else {
        ((b, nb), (a, na))
    };
    let mut hash = blake3::Hasher::new_derive_key("Munin nearby pairing session v1");
    hash.update(a.as_bytes());
    hash.update(na.as_bytes());
    hash.update(b.as_bytes());
    hash.update(nb.as_bytes());
    let id = hash.finalize().to_hex().to_string();
    let short = id[..12].to_ascii_uppercase();
    (
        id,
        format!("{} {} {}", &short[..4], &short[4..8], &short[8..12]),
    )
}
/// Commit before revealing: a peer cannot choose its nonce after seeing ours
/// to grind a matching short fingerprint across two attacker-controlled sessions.
pub fn nonce_commitment(id: EndpointId, nonce: &str) -> String {
    let mut hash = blake3::Hasher::new_derive_key("Munin pairing nonce commitment v1");
    hash.update(id.as_bytes());
    hash.update(nonce.as_bytes());
    hash.finalize().to_hex().to_string()
}
pub fn verify_nonce(id: EndpointId, nonce: &str, commitment: &str) -> io::Result<()> {
    if nonce.len() != 64
        || !nonce.bytes().all(|b| b.is_ascii_hexdigit())
        || !constant_time_eq::constant_time_eq(
            nonce_commitment(id, nonce).as_bytes(),
            commitment.as_bytes(),
        )
    {
        return Err(err(
            "Nearby verification commitment did not match; do not accept this connection",
        ));
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
struct Hello {
    version: u8,
    name: String,
    addr: EndpointAddr,
    commitment: String,
}
fn local(endpoint: &Endpoint, shared: &Shared) -> (Hello, String) {
    let nonce = blake3::hash(&SecretKey::generate().to_bytes())
        .to_hex()
        .to_string();
    (
        Hello {
            version: 1,
            name: shared.lock().unwrap().name.clone(),
            addr: endpoint.addr(),
            commitment: nonce_commitment(endpoint.id(), &nonce),
        },
        nonce,
    )
}
fn validate(h: &Hello, remote: EndpointId, own: EndpointId) -> io::Result<()> {
    if h.version != 1
        || h.addr.id != remote
        || remote == own
        || h.name.trim().is_empty()
        || h.name.len() > 80
        || h.name.chars().any(char::is_control)
        || h.commitment.len() != 64
        || !h.commitment.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(err("Invalid nearby device handshake"));
    }
    Ok(())
}
async fn session(
    endpoint: &Endpoint,
    conn: &Connection,
    shared: Shared,
    gate: Pairing,
    events: mpsc::Sender<Event>,
    outgoing: bool,
) -> io::Result<()> {
    if !gate.enabled() {
        return Err(err("Nearby pairing is not enabled"));
    }
    let (own, own_nonce) = local(endpoint, &shared);
    let (mut send, mut recv, remote, remote_nonce) =
        tokio::time::timeout(Duration::from_secs(10), async {
            let (mut send, mut recv) = if outgoing {
                conn.open_bi().await.map_err(err)?
            } else {
                conn.accept_bi().await.map_err(err)?
            };
            network::write_frame(&mut send, &serde_json::to_vec(&own).map_err(err)?).await?;
            let remote: Hello =
                serde_json::from_slice(&network::read_frame(&mut recv, HELLO_LIMIT).await?)
                    .map_err(err)?;
            validate(&remote, conn.remote_id(), endpoint.id())?;
            network::write_frame(&mut send, own_nonce.as_bytes()).await?;
            let remote_nonce =
                String::from_utf8(network::read_frame(&mut recv, 64).await?).map_err(err)?;
            verify_nonce(remote.addr.id, &remote_nonce, &remote.commitment)?;
            Ok::<_, io::Error>((send, recv, remote, remote_nonce))
        })
        .await
        .map_err(|_| err("Nearby device did not respond"))??;
    let (id, fingerprint) = verification(endpoint.id(), &own_nonce, remote.addr.id, &remote_nonce);
    let peer = Peer {
        addr: remote.addr,
        name: remote.name,
    };
    let request = Request {
        id: id.clone(),
        peer: peer.clone(),
        fingerprint,
        outgoing,
        expires: Instant::now() + REQUEST_LIFE,
    };
    let mut cancelled = gate.1.subscribe();
    let approval = gate.register(request.clone())?;
    let _ = events.send(Event::PairRequest(request));
    let result = tokio::time::timeout(REQUEST_LIFE, async {
        let local_decision = async {
            let accepted = approval
                .await
                .map_err(|_| err("Nearby pairing cancelled"))?;
            network::write_frame(&mut send, &[u8::from(accepted)]).await?;
            if accepted {
                Ok(())
            } else {
                Err(err("Pairing declined"))
            }
        };
        let remote_decision = async {
            match network::read_frame(&mut recv, 1).await?.as_slice() {
                [1] => Ok(()),
                [0] => Err(err("The other device declined pairing")),
                _ => Err(err("Invalid pairing decision")),
            }
        };
        tokio::select! {
            biased;
            _ = cancelled.changed() => return Err(err("Nearby pairing was stopped")),
            result = async { tokio::try_join!(local_decision, remote_decision) } => { result?; },
        }
        if !gate.enabled() {
            return Err(err("Nearby pairing was stopped"));
        }
        // No document is sent on this ALPN. Subsequent normal sync requires this grant.
        network::grant(&shared, peer, &events)?;
        network::write_frame(&mut send, &[2]).await?;
        if network::read_frame(&mut recv, 1).await? != [2] {
            return Err(err("Device did not finish pairing"));
        }
        send.finish().map_err(err)?;
        let _ = send.stopped().await;
        Ok(())
    })
    .await
    .unwrap_or_else(|_| {
        Err(err(
            "Pairing expired. Try connecting again and accept on both devices.",
        ))
    });
    gate.cancel(&id);
    let _ = events.send(Event::PairFinished {
        id,
        error: result.as_ref().err().map(ToString::to_string),
    });
    result
}
pub async fn serve(
    endpoint: &Endpoint,
    conn: Connection,
    shared: Shared,
    gate: Pairing,
    events: mpsc::Sender<Event>,
) -> io::Result<()> {
    let result = session(endpoint, &conn, shared, gate, events, false).await;
    conn.close(0u32.into(), b"pairing finished");
    result
}
pub async fn dial(
    endpoint: &Endpoint,
    addr: EndpointAddr,
    shared: Shared,
    gate: Pairing,
    events: mpsc::Sender<Event>,
) -> io::Result<()> {
    let conn = tokio::time::timeout(Duration::from_secs(10), endpoint.connect(addr, ALPN))
        .await
        .map_err(|_| err("Nearby device unavailable"))?
        .map_err(err)?;
    let result = session(endpoint, &conn, shared, gate, events, true).await;
    conn.close(0u32.into(), b"pairing finished");
    result
}
