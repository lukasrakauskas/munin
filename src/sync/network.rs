//! Authenticated, bounded Automerge sync over iroh. No filesystem work on this thread.
use super::store::{MAX_BYTES, err};
use automerge::{
    AutoCommit,
    sync::{Message, State as SyncState, SyncDoc},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use iroh::{
    Endpoint, EndpointAddr, EndpointId, SecretKey,
    endpoint::{Connection, RecvStream, SendStream, presets},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};
use tokio::sync::{Semaphore, mpsc as channel, watch};

pub const ALPN: &[u8] = b"munin/automerge/1";
const HELLO_LIMIT: usize = 16 * 1024;
const SESSION_TIMEOUT: Duration = Duration::from_secs(20);
#[derive(Clone, Serialize, Deserialize)]
pub struct Peer {
    pub addr: EndpointAddr,
    pub name: String,
}
#[derive(Serialize, Deserialize)]
pub struct Ticket {
    pub version: u8,
    pub addr: EndpointAddr,
    pub token: String,
}
impl Ticket {
    pub fn encode(&self) -> io::Result<String> {
        Ok(format!(
            "munin1-{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(self).map_err(err)?)
        ))
    }
    pub fn parse(value: &str) -> io::Result<Self> {
        if value.len() > HELLO_LIMIT {
            return Err(err("This invitation is too long"));
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(
                value
                    .trim()
                    .strip_prefix("munin1-")
                    .ok_or_else(|| err("Paste a Munin device invitation"))?,
            )
            .map_err(|_| err("Invalid invitation"))?;
        let ticket: Self = serde_json::from_slice(&bytes).map_err(|_| err("Invalid invitation"))?;
        if ticket.version != 1
            || ticket.token.len() != 64
            || !ticket.token.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(err("Unsupported or invalid invitation"));
        }
        Ok(ticket)
    }
}
pub struct Invitation {
    pub token: String,
    pub expires: Instant,
}
#[derive(Default)]
pub struct Authorization {
    pub peers: BTreeMap<EndpointId, Peer>,
    pub invite: Option<Invitation>,
}
impl Authorization {
    pub fn admit(&mut self, id: EndpointId, token: Option<&str>) -> bool {
        if self.peers.contains_key(&id) {
            return true;
        }
        if self.peers.len() >= 64 {
            return false;
        }
        let valid = self.invite.as_ref().is_some_and(|i| {
            Instant::now() < i.expires
                && token.is_some_and(|t| {
                    constant_time_eq::constant_time_eq(t.as_bytes(), i.token.as_bytes())
                })
        });
        if valid {
            self.invite = None;
            // Identity is authenticated by iroh, not by the peer's hello payload.
            self.peers.insert(
                id,
                Peer {
                    addr: id.into(),
                    name: "Device".into(),
                },
            );
        }
        valid
    }
}
pub struct SessionState {
    pub doc: AutoCommit,
    pub auth: Authorization,
    pub name: String,
}
pub(super) type Shared = Arc<Mutex<SessionState>>;
pub enum Command {
    Snapshot(Vec<u8>),
    Invite,
    RevokeInvite,
    Pair(Ticket),
    Nearby(bool),
    NearbyConnect(EndpointAddr),
    Decide(String, bool),
}
pub enum Event {
    Nearby(Peer),
    NearbyExpired(EndpointId),
    NearbyState(bool),
    PairRequest(super::pairing::Request),
    PairFinished { id: String, error: Option<String> },
    Ready,
    Invitation(String),
    Peer(Peer),
    Document(Vec<u8>),
    Synced(EndpointId),
    Error(Option<EndpointId>, String),
    Fatal(String),
}
pub struct Worker {
    pub commands: channel::UnboundedSender<Command>,
    pub events: mpsc::Receiver<Event>,
    stop: watch::Sender<bool>,
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}
impl Worker {
    pub fn start(secret: [u8; 32], name: String, peers: Vec<Peer>, snapshot: Vec<u8>) -> Self {
        let (commands, rx) = channel::unbounded_channel();
        let (events_tx, events) = mpsc::channel();
        let (stop, stop_rx) = watch::channel(false);
        std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build();
            match result {
                Ok(rt) => rt.block_on(async {
                    let result = run(
                        secret,
                        name,
                        peers,
                        snapshot,
                        rx,
                        stop_rx,
                        events_tx.clone(),
                    )
                    .await;
                    if let Err(e) = result {
                        let _ = events_tx.send(Event::Fatal(e.to_string()));
                    }
                }),
                Err(e) => {
                    let _ = events_tx.send(Event::Fatal(e.to_string()));
                }
            }
        });
        Self {
            commands,
            events,
            stop,
        }
    }
}
#[derive(Serialize, Deserialize)]
struct Hello {
    version: u8,
    name: String,
    addr: EndpointAddr,
    token: Option<String>,
}
fn hello(endpoint: &Endpoint, shared: &Shared, token: Option<String>) -> Hello {
    Hello {
        version: 1,
        name: shared.lock().unwrap().name.clone(),
        addr: endpoint.addr(),
        token,
    }
}
fn check_hello(h: &Hello, conn: &Connection) -> io::Result<()> {
    if h.version != 1 || h.name.is_empty() || h.name.len() > 80 || h.addr.id != conn.remote_id() {
        return Err(err("Invalid device handshake"));
    }
    Ok(())
}
pub(super) async fn write_frame(send: &mut SendStream, bytes: &[u8]) -> io::Result<()> {
    if bytes.len() > MAX_BYTES {
        return Err(err("Sync message exceeds 32 MiB"));
    }
    send.write_all(&(bytes.len() as u32).to_be_bytes())
        .await
        .map_err(err)?;
    send.write_all(bytes).await.map_err(err)
}
pub(super) async fn read_frame(recv: &mut RecvStream, limit: usize) -> io::Result<Vec<u8>> {
    let mut len = [0; 4];
    recv.read_exact(&mut len).await.map_err(err)?;
    let len = u32::from_be_bytes(len) as usize;
    if len > limit {
        return Err(err("Sync message exceeds the safety limit"));
    }
    let mut bytes = vec![0; len];
    recv.read_exact(&mut bytes).await.map_err(err)?;
    Ok(bytes)
}
async fn write_hello(send: &mut SendStream, h: &Hello) -> io::Result<()> {
    write_frame(send, &serde_json::to_vec(h).map_err(err)?).await
}
async fn read_hello(recv: &mut RecvStream) -> io::Result<Hello> {
    serde_json::from_slice(&read_frame(recv, HELLO_LIMIT).await?).map_err(err)
}

async fn exchange(
    mut doc: AutoCommit,
    send: &mut SendStream,
    recv: &mut RecvStream,
    initiator: bool,
) -> io::Result<AutoCommit> {
    let mut sync = SyncState::new();
    let mut received = 0;
    for _ in 0..128 {
        if initiator {
            let outgoing = doc
                .sync()
                .generate_sync_message(&mut sync)
                .map(|m| m.encode())
                .unwrap_or_default();
            write_frame(send, &outgoing).await?;
            let incoming = read_frame(recv, MAX_BYTES).await?;
            received += incoming.len();
            if received > MAX_BYTES * 2 {
                return Err(err("Sync session exceeds the transfer limit"));
            }
            if outgoing.is_empty() && incoming.is_empty() {
                return Ok(doc);
            }
            if !incoming.is_empty() {
                doc.sync()
                    .receive_sync_message(&mut sync, Message::decode(&incoming).map_err(err)?)
                    .map_err(err)?;
            }
        } else {
            let incoming = read_frame(recv, MAX_BYTES).await?;
            received += incoming.len();
            if received > MAX_BYTES * 2 {
                return Err(err("Sync session exceeds the transfer limit"));
            }
            if !incoming.is_empty() {
                doc.sync()
                    .receive_sync_message(&mut sync, Message::decode(&incoming).map_err(err)?)
                    .map_err(err)?;
            }
            let outgoing = doc
                .sync()
                .generate_sync_message(&mut sync)
                .map(|m| m.encode())
                .unwrap_or_default();
            write_frame(send, &outgoing).await?;
            if outgoing.is_empty() && incoming.is_empty() {
                return Ok(doc);
            }
        }
    }
    Err(err("Sync did not converge within the session limit"))
}
fn finish(
    shared: &Shared,
    mut doc: AutoCommit,
    peer: Peer,
    events: &mpsc::Sender<Event>,
) -> io::Result<()> {
    super::store::document_notes(&doc)?;
    let mut state = shared.lock().unwrap();
    let before = state.doc.get_heads();
    let mut candidate = state.doc.clone();
    candidate.merge(&mut doc).map_err(err)?;
    super::store::document_notes(&candidate)?;
    let bytes = candidate.save();
    if bytes.len() > MAX_BYTES {
        return Err(err("Sync history exceeds 32 MiB"));
    }
    let changed = before != candidate.get_heads();
    state.doc = candidate;
    if changed {
        let _ = events.send(Event::Document(bytes));
    }
    let _ = events.send(Event::Synced(peer.addr.id));
    Ok(())
}
fn approved(shared: &Shared, remote: &Hello, events: &mpsc::Sender<Event>) -> io::Result<()> {
    grant(
        shared,
        Peer {
            addr: remote.addr.clone(),
            name: remote.name.clone(),
        },
        events,
    )
}
pub(super) fn grant(shared: &Shared, peer: Peer, events: &mpsc::Sender<Event>) -> io::Result<()> {
    let mut state = shared.lock().unwrap();
    if state.auth.peers.len() >= 64 && !state.auth.peers.contains_key(&peer.addr.id) {
        return Err(err("At most 64 paired devices are supported"));
    }
    if !state.auth.peers.contains_key(&peer.addr.id) {
        // The UI clears a pending invitation when a new nearby peer is approved.
        // Revoke its capability too; never leave a hidden code redeemable.
        state.auth.invite = None;
    }
    state.auth.peers.insert(peer.addr.id, peer.clone());
    // Expose every grant even if the first data exchange fails.
    // Otherwise a partially paired device would have access but no Remove button.
    let _ = events.send(Event::Peer(peer));
    Ok(())
}
pub async fn serve(
    endpoint: &Endpoint,
    conn: Connection,
    shared: Shared,
    events: mpsc::Sender<Event>,
) -> io::Result<()> {
    let result = async {
        let (mut send, mut recv) = conn.accept_bi().await.map_err(err)?;
        let remote = read_hello(&mut recv).await?;
        check_hello(&remote, &conn)?;
        if !shared
            .lock()
            .unwrap()
            .auth
            .admit(conn.remote_id(), remote.token.as_deref())
        {
            return Err(err(
                "This device is not approved, or its invitation expired",
            ));
        }
        approved(&shared, &remote, &events)?;
        write_hello(&mut send, &hello(endpoint, &shared, None)).await?;
        let doc = shared.lock().unwrap().doc.clone();
        let doc = exchange(doc, &mut send, &mut recv, false).await?;
        finish(
            &shared,
            doc,
            Peer {
                addr: remote.addr,
                name: remote.name,
            },
            &events,
        )?;
        send.finish().map_err(err)?;
        let _ = send.stopped().await;
        Ok(())
    }
    .await;
    conn.close(0u32.into(), b"session finished");
    result
}
pub async fn dial(
    endpoint: &Endpoint,
    addr: EndpointAddr,
    token: Option<String>,
    shared: Shared,
    events: mpsc::Sender<Event>,
) -> io::Result<()> {
    let conn = endpoint.connect(addr, ALPN).await.map_err(err)?;
    let result = async {
        let (mut send, mut recv) = conn.open_bi().await.map_err(err)?;
        write_hello(&mut send, &hello(endpoint, &shared, token)).await?;
        let remote = read_hello(&mut recv).await?;
        check_hello(&remote, &conn)?;
        approved(&shared, &remote, &events)?;
        let doc = shared.lock().unwrap().doc.clone();
        let doc = exchange(doc, &mut send, &mut recv, true).await?;
        finish(
            &shared,
            doc,
            Peer {
                addr: remote.addr,
                name: remote.name,
            },
            &events,
        )?;
        send.finish().map_err(err)?;
        let _ = send.stopped().await;
        Ok(())
    }
    .await;
    conn.close(0u32.into(), b"session finished");
    result
}
fn spawn_dial(
    endpoint: Endpoint,
    addr: EndpointAddr,
    token: Option<String>,
    shared: Shared,
    events: mpsc::Sender<Event>,
    slots: Arc<Semaphore>,
) {
    let Ok(permit) = slots.try_acquire_owned() else {
        return;
    };
    tokio::spawn(async move {
        let id = addr.id;
        let result = tokio::time::timeout(
            SESSION_TIMEOUT,
            dial(&endpoint, addr, token, shared, events.clone()),
        )
        .await;
        let error = match result {
            Ok(Ok(())) => None,
            Ok(Err(e)) => Some(e.to_string()),
            Err(_) => Some("Device unavailable; retrying automatically".into()),
        };
        if let Some(error) = error {
            let _ = events.send(Event::Error(Some(id), error));
        }
        drop(permit);
    });
}
async fn run(
    secret: [u8; 32],
    name: String,
    peers: Vec<Peer>,
    snapshot: Vec<u8>,
    mut commands: channel::UnboundedReceiver<Command>,
    mut stop: watch::Receiver<bool>,
    events: mpsc::Sender<Event>,
) -> io::Result<()> {
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(SecretKey::from_bytes(&secret))
        .alpns(vec![ALPN.to_vec(), super::pairing::ALPN.to_vec()])
        .bind()
        .await
        .map_err(err)?;
    let state = Arc::new(Mutex::new(SessionState {
        doc: AutoCommit::load(&snapshot).map_err(err)?,
        auth: Authorization {
            peers: peers.into_iter().map(|p| (p.addr.id, p)).collect(),
            invite: None,
        },
        name,
    }));
    let _ = events.send(Event::Ready);
    let slots = Arc::new(Semaphore::new(8));
    let gate = super::pairing::Pairing::new(false);
    let mut discovery: Option<super::discovery::Discovery> = None;
    let mut retry = tokio::time::interval(Duration::from_secs(4));
    retry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Endpoint address lookup/discovery and relay reconnection are managed by iroh.
    loop {
        tokio::select! {
            _ = stop.changed() => break,
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else { break; };
                if let Ok(permit) = slots.clone().try_acquire_owned() {
                    let ep = endpoint.clone(); let st = state.clone(); let tx = events.clone(); let pairing = gate.clone();
                    tokio::spawn(async move {
                        let result = async {
                            let conn = tokio::time::timeout(Duration::from_secs(10), incoming).await.map_err(err)?.map_err(err)?;
                            if conn.alpn() == super::pairing::ALPN {
                                super::pairing::serve(&ep, conn, st, pairing, tx).await
                            } else {
                                tokio::time::timeout(SESSION_TIMEOUT, serve(&ep, conn, st, tx)).await.map_err(err)?
                            }
                        }.await;
                        // Unapproved incoming devices don't create noisy UI errors.
                        let _ = result;
                        drop(permit);
                    });
                }
            },
            command = commands.recv() => match command {
                Some(Command::Snapshot(bytes)) => {
                    let result = AutoCommit::load(&bytes).map_err(err).and_then(|mut doc| state.lock().unwrap().doc.merge(&mut doc).map(|_| ()).map_err(err));
                    if let Err(e) = result { let _ = events.send(Event::Error(None, e.to_string())); }
                },
                Some(Command::Invite) => {
                    let token = SecretKey::generate().public().to_string();
                    state.lock().unwrap().auth.invite = Some(Invitation { token: token.clone(), expires: Instant::now() + Duration::from_secs(600) });
                    let ticket = Ticket { version: 1, addr: endpoint.addr(), token };
                    match ticket.encode() { Ok(t) => { let _ = events.send(Event::Invitation(t)); }, Err(e) => { let _ = events.send(Event::Error(None, e.to_string())); } }
                },
                Some(Command::RevokeInvite) => state.lock().unwrap().auth.invite = None,
                Some(Command::Pair(ticket)) => {
                    if ticket.addr.id == endpoint.id() { let _ = events.send(Event::Error(None, "This invitation belongs to this device".into())); }
                    else { spawn_dial(endpoint.clone(), ticket.addr, Some(ticket.token), state.clone(), events.clone(), slots.clone()); }
                },
                Some(Command::Nearby(enabled)) => {
                    discovery = None;
                    gate.stop();
                    if enabled {
                        let name = state.lock().unwrap().name.clone();
                        match super::discovery::Discovery::start(endpoint.clone(), name, events.clone(), Some(state.clone())).await {
                            Ok(service) => { discovery = Some(service); gate.start(); let _ = events.send(Event::NearbyState(true)); },
                            Err(e) => { let _ = events.send(Event::NearbyState(false)); let _ = events.send(Event::Error(None, format!("Nearby discovery unavailable: {e}. Check local-network permissions or use an invitation."))); },
                        }
                    } else { let _ = events.send(Event::NearbyState(false)); }
                },
                Some(Command::NearbyConnect(addr)) => {
                    if !gate.enabled() || addr.id == endpoint.id() { let _ = events.send(Event::Error(None, "Enable nearby discovery and select another device".into())); }
                    else if let Ok(permit) = slots.clone().try_acquire_owned() {
                        let ep = endpoint.clone(); let st = state.clone(); let pairing = gate.clone(); let tx = events.clone();
                        tokio::spawn(async move {
                            let id = addr.id;
                            if let Err(e) = super::pairing::dial(&ep, addr, st, pairing, tx.clone()).await { let _ = tx.send(Event::Error(Some(id), e.to_string())); }
                            drop(permit);
                        });
                    } else { let _ = events.send(Event::Error(None, "Too many active connections. Try again shortly.".into())); }
                },
                Some(Command::Decide(id, accept)) => if let Err(e) = gate.decide(&id, accept) { let _ = events.send(Event::Error(None, e.to_string())); },
                None => break,
            },
            _ = async {
                if let Some(service) = &discovery { tokio::time::sleep_until(tokio::time::Instant::from_std(service.started + super::discovery::WINDOW)).await; }
                else { std::future::pending::<()>().await; }
            } => {
                gate.stop(); discovery = None; let _ = events.send(Event::NearbyState(false));
            },
            _ = retry.tick() => {
                let peers: Vec<_> = state.lock().unwrap().auth.peers.values().cloned().collect();
                for peer in peers {
                    if endpoint.id() < peer.addr.id { spawn_dial(endpoint.clone(), peer.addr, None, state.clone(), events.clone(), slots.clone()); }
                }
            },
        }
    }
    gate.stop();
    drop(discovery);
    endpoint.close().await;
    Ok(())
}
