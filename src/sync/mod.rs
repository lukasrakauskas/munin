//! Workspace-local sync lifecycle and the UI-thread file/buffer bridge.
#[cfg(test)]
mod authorization_tests;
mod discovery;
#[cfg(test)]
mod discovery_tests;
mod network;
#[cfg(test)]
mod network_tests;
mod pairing;
#[cfg(test)]
mod pairing_tests;
#[cfg(test)]
mod safety_tests;
pub mod store;
#[cfg(test)]
mod tests;

use crate::{
    live,
    state::{State, path_id},
};
pub(crate) use network::Peer;
use network::{Command, Event, Ticket, Worker};
pub(crate) use pairing::Request;

static SUCCESS_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
pub struct Success {
    pub name: String,
    pub sequence: u64,
}
use rune::TextEdit;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use store::{Store, err};

#[derive(Serialize, Deserialize)]
struct Config {
    enabled: bool,
    secret: [u8; 32],
    name: String,
    peers: Vec<Peer>,
}
struct BufferBase {
    path: String,
    id: String,
    snapshot: Vec<u8>,
    applied: Option<String>,
}
#[derive(Default)]
pub struct Sync {
    root: Option<PathBuf>,
    config: Option<Config>,
    load_failed: bool,
    pub store: Option<Store>,
    pub(super) worker: Option<Worker>,
    buffer: Option<BufferBase>,
    pub pending: Vec<Vec<u8>>,
    sent: Vec<u8>,
    pub ready: bool,
    pub error: Option<String>,
    pub file_error: Option<String>,
    pub invitation: Option<(String, Instant)>,
    pub pairing: bool,
    pub requests: BTreeMap<String, Request>,
    pub nearby: bool,
    pub nearby_devices: BTreeMap<String, Peer>,
    pub accepted_requests: std::collections::BTreeSet<String>,
    pub success: Option<Success>,
    newly_paired: std::collections::BTreeSet<String>,
    pub peer_errors: BTreeMap<String, String>,
    pub last_seen: BTreeMap<String, Instant>,
    pub last_success: Option<Instant>,
    pub pair_input: TextEdit,
    pub name_input: TextEdit,
    last_poll: Option<Instant>,
    revision: u64,
}
impl Sync {
    pub fn load(root: Option<&Path>) -> Self {
        let mut this = Self {
            root: root.map(Path::to_path_buf),
            ..Self::default()
        };
        let result = (|| -> io::Result<()> {
            let Some(root) = root else {
                return Ok(());
            };
            let meta = root.join(store::META);
            if !meta.exists() {
                return Ok(());
            }
            if fs::symlink_metadata(&meta)?.file_type().is_symlink() {
                return Err(err("Sync metadata cannot be a symlink"));
            }
            let file = meta.join("device.json");
            if !file.exists() {
                return Ok(());
            }
            if fs::symlink_metadata(&file)?.file_type().is_symlink()
                || fs::metadata(&file)?.len() > 256 * 1024
            {
                return Err(err("Unsafe sync device configuration"));
            }
            let config: Config = serde_json::from_slice(&fs::read(file)?).map_err(err)?;
            if config.name.is_empty() || config.name.len() > 80 || config.peers.len() > 64 {
                return Err(err("Invalid sync device configuration"));
            }
            let enabled = config.enabled;
            this.name_input = TextEdit::new(&config.name);
            this.config = Some(config);
            if enabled {
                this.set_enabled(true)?;
            }
            Ok(())
        })();
        if let Err(e) = result {
            this.error = Some(e.to_string());
            this.load_failed = true;
        }
        this
    }
    pub fn enabled(&self) -> bool {
        self.config.as_ref().is_some_and(|c| c.enabled) && self.store.is_some()
    }
    pub fn peers(&self) -> &[Peer] {
        self.config.as_ref().map_or(&[], |c| c.peers.as_slice())
    }
    pub fn device_id(&self) -> Option<String> {
        self.config
            .as_ref()
            .map(|c| iroh::SecretKey::from_bytes(&c.secret).public().to_string())
    }
    fn save_config(&self) -> io::Result<()> {
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| err("Open a workspace first"))?;
        store::atomic(
            &root.join(store::META).join("device.json"),
            &serde_json::to_vec(
                self.config
                    .as_ref()
                    .ok_or_else(|| err("Sync not initialized"))?,
            )
            .map_err(err)?,
        )
    }
    fn start_worker(&mut self) {
        self.worker = None;
        self.ready = false;
        self.invitation = None;
        self.pairing = false;
        self.nearby = false;
        self.nearby_devices.clear();
        self.requests.clear();
        self.accepted_requests.clear();
        if let (Some(config), Some(store)) = (&self.config, &mut self.store) {
            self.sent = store.snapshot();
            self.worker = Some(Worker::start(
                config.secret,
                config.name.clone(),
                config.peers.clone(),
                self.sent.clone(),
            ));
        }
    }
    pub fn set_enabled(&mut self, enabled: bool) -> io::Result<()> {
        if self.load_failed {
            return Err(err(
                "Sync metadata could not be loaded. Back up .munin-sync before repairing it; no identity was reset.",
            ));
        }
        if enabled {
            let root = self
                .root
                .as_ref()
                .ok_or_else(|| err("Open a workspace before enabling sync"))?;
            if self.store.is_none() {
                let mut store = Store::open(root)?;
                store.scan()?;
                store.materialize()?;
                self.store = Some(store);
            }
            if self.config.is_none() {
                let name = std::env::var("HOSTNAME")
                    .ok()
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| format!("{} device", std::env::consts::OS));
                let name: String = name.chars().take(64).collect();
                self.name_input = TextEdit::new(&name);
                self.config = Some(Config {
                    enabled: true,
                    secret: iroh::SecretKey::generate().to_bytes(),
                    name,
                    peers: Vec::new(),
                });
            }
            self.config.as_mut().unwrap().enabled = true;
            self.save_config()?;
            self.start_worker();
        } else {
            self.worker = None;
            self.store = None;
            self.buffer = None;
            self.ready = false;
            self.invitation = None;
            self.pairing = false;
            self.pending.clear();
            self.nearby = false;
            self.nearby_devices.clear();
            self.requests.clear();
            self.accepted_requests.clear();
            if let Some(config) = &mut self.config {
                config.enabled = false;
                self.save_config()?;
            }
        }
        self.error = None;
        self.file_error = None;
        Ok(())
    }
    pub fn rename_device(&mut self) -> io::Result<()> {
        let name = self.name_input.text.trim();
        if name.is_empty() || name.len() > 80 || name.chars().any(char::is_control) {
            return Err(err(
                "Use a device name of 1–80 bytes without control characters",
            ));
        }
        self.config
            .as_mut()
            .ok_or_else(|| err("Enable sync first"))?
            .name = name.to_owned();
        self.save_config()?;
        if self.enabled() {
            self.start_worker();
        }
        Ok(())
    }
    pub fn forget(&mut self, id: &str) -> io::Result<()> {
        // Revoke in-flight access before any fallible persistence step.
        self.worker = None;
        self.ready = false;
        self.invitation = None;
        self.pairing = false;
        self.nearby = false;
        self.nearby_devices.clear();
        self.requests.clear();
        self.accepted_requests.clear();
        self.success = None;
        if let Some(config) = &mut self.config {
            config.peers.retain(|p| p.addr.id.to_string() != id);
        }
        self.save_config().map_err(|e| err(format!("Device removal could not be saved; networking is stopped. Fix the storage error, then pause and re-enable sync to save this change before restarting Munin: {e}")))?;
        self.last_seen.remove(id);
        self.peer_errors.remove(id);
        if self.enabled() {
            self.start_worker();
        }
        Ok(())
    }
    pub fn set_nearby(&mut self, enabled: bool) -> io::Result<()> {
        if !self.enabled() {
            return Err(err("Enable workspace sync first"));
        }
        self.send(Command::Nearby(enabled))?;
        self.nearby = enabled;
        if !enabled {
            self.nearby_devices.clear();
            self.requests.clear();
            self.accepted_requests.clear();
            self.pairing = false;
        }
        self.error = None;
        Ok(())
    }
    pub fn connect_nearby(&mut self, id: &str) -> io::Result<()> {
        if !self.nearby {
            return Err(err("Enable nearby discovery first"));
        }
        let peer = self
            .nearby_devices
            .get(id)
            .ok_or_else(|| err("Nearby device expired; wait for discovery or use an invitation"))?;
        self.send(Command::NearbyConnect(peer.addr.clone()))?;
        self.pairing = true;
        self.error = None;
        Ok(())
    }
    pub fn decide(&mut self, id: &str, accept: bool) -> io::Result<()> {
        let request = self
            .requests
            .get(id)
            .ok_or_else(|| err("Pairing request no longer exists"))?;
        if request.expires <= Instant::now() || self.accepted_requests.contains(id) {
            return Err(err("Pairing request expired or already answered"));
        }
        self.send(Command::Decide(id.to_owned(), accept))?;
        if accept {
            self.accepted_requests.insert(id.to_owned());
        } else {
            self.requests.remove(id);
        }
        Ok(())
    }
    pub fn invite(&mut self) -> io::Result<()> {
        self.send(Command::Invite)
    }
    pub fn revoke(&mut self) -> io::Result<()> {
        self.invitation = None;
        self.send(Command::RevokeInvite)
    }
    fn send(&self, command: Command) -> io::Result<()> {
        self.worker
            .as_ref()
            .ok_or_else(|| err("Enable sync first"))?
            .commands
            .send(command)
            .map_err(|_| err("Sync worker stopped; pause and enable sync to retry"))
    }
    pub fn pair(&mut self) -> io::Result<()> {
        let ticket = Ticket::parse(&self.pair_input.text)?;
        self.send(Command::Pair(ticket))?;
        self.pairing = true;
        self.error = None;
        Ok(())
    }
    fn events(&mut self) -> io::Result<bool> {
        let events: Vec<_> = self
            .worker
            .as_ref()
            .map(|w| w.events.try_iter().take(64).collect())
            .unwrap_or_default();
        let mut changed = !events.is_empty();
        for event in events {
            self.handle_event(event)?;
        }
        let count = self.requests.len();
        self.requests.retain(|_, r| r.expires > Instant::now());
        self.accepted_requests
            .retain(|id| self.requests.contains_key(id));
        changed |= count != self.requests.len();
        if self
            .invitation
            .as_ref()
            .is_some_and(|(_, t)| t.elapsed() >= Duration::from_secs(600))
        {
            self.invitation = None;
            changed = true;
        }
        Ok(changed)
    }
    fn handle_event(&mut self, event: Event) -> io::Result<()> {
        match event {
            Event::Nearby(peer) => {
                if self.nearby
                    && (self.nearby_devices.len() < 64
                        || self.nearby_devices.contains_key(&peer.addr.id.to_string()))
                {
                    self.nearby_devices.insert(peer.addr.id.to_string(), peer);
                }
            }
            Event::NearbyExpired(id) => {
                self.nearby_devices.remove(&id.to_string());
            }
            Event::NearbyState(enabled) => {
                self.nearby = enabled;
                if !enabled {
                    self.nearby_devices.clear();
                    self.requests.clear();
                    self.accepted_requests.clear();
                    self.pairing = false;
                }
            }
            Event::PairRequest(request) => {
                if self.nearby && request.expires > Instant::now() && self.requests.len() < 8 {
                    self.pairing = false;
                    self.requests.insert(request.id.clone(), request);
                }
            }
            Event::PairFinished { id, error } => {
                self.requests.remove(&id);
                self.accepted_requests.remove(&id);
                self.pairing = error.is_none();
                if error.is_some() {
                    self.error = error;
                }
            }
            Event::Ready => self.ready = true,
            Event::Invitation(code) => {
                self.invitation = Some((code, Instant::now()));
                self.error = None;
            }
            Event::Peer(peer) => {
                let config = self.config.as_mut().unwrap();
                let existing = config.peers.iter_mut().find(|p| p.addr.id == peer.addr.id);
                let changed = match existing {
                    Some(old) => {
                        let changed = old.addr != peer.addr || old.name != peer.name;
                        *old = peer;
                        changed
                    }
                    None => {
                        if config.peers.len() >= 64 {
                            return Err(err("At most 64 paired devices are supported"));
                        }
                        self.newly_paired.insert(peer.addr.id.to_string());
                        config.peers.push(peer);
                        self.invitation = None;
                        true
                    }
                };
                if changed {
                    self.save_config()?;
                }
            }
            Event::Document(bytes) => self.pending.push(bytes),
            Event::Synced(id) => {
                if self.newly_paired.remove(&id.to_string())
                    && let Some(peer) = self.peers().iter().find(|p| p.addr.id == id)
                {
                    let name = peer.name.clone();
                    self.success = Some(Success {
                        name,
                        sequence: SUCCESS_SEQUENCE
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                    });
                }
                self.last_seen.insert(id.to_string(), Instant::now());
                self.peer_errors.remove(&id.to_string());
                self.last_success = Some(Instant::now());
                self.pairing = false;
                self.pair_input = TextEdit::new("");
                self.error = None;
            }
            Event::Error(id, error) => {
                self.pairing = false;
                if let Some(id) = id.filter(|id| self.peers().iter().any(|p| p.addr.id == *id)) {
                    self.peer_errors.insert(id.to_string(), error);
                } else {
                    self.error = Some(error);
                }
            }
            Event::Fatal(error) => {
                self.ready = false;
                self.error = Some(error);
                self.worker = None;
            }
        }
        Ok(())
    }
}

pub fn capture(s: &mut State) {
    if let (Some(path), Some(store)) = (&s.current, &mut s.sync.store) {
        let path = path_id(path);
        if let Ok(notes) = store.notes()
            && let Some(note) = notes.get(&path)
        {
            s.sync.buffer = Some(BufferBase {
                path,
                id: note.id.clone(),
                snapshot: store.snapshot(),
                applied: None,
            });
        }
    } else {
        s.sync.buffer = None;
    }
}
fn boundary(text: &str, byte: usize) -> usize {
    let mut byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}
/// Flush local disk/buffer changes before merging remote histories. Failed writes retain the buffer.
pub fn flush(s: &mut State) -> io::Result<()> {
    let mut sync = std::mem::take(&mut s.sync);
    let result = (|| -> io::Result<()> {
        if sync.events()? {
            sync.revision = sync.revision.wrapping_add(1);
        }
        let Some(store) = &mut sync.store else {
            return Ok(());
        };
        let old = store.notes()?;
        let dirty = s.current.is_some() && s.edit.text != s.saved;
        store.scan()?;
        if dirty {
            let base = sync.buffer.as_mut().ok_or_else(|| {
                err("No sync baseline for this buffer; your edits remain in the editor")
            })?;
            if base.applied.as_deref() != Some(&s.edit.text) {
                base.snapshot = store.edit_from(&base.snapshot, &base.path, &s.edit.text)?;
                base.applied = Some(s.edit.text.clone());
            }
        }
        while let Some(bytes) = sync.pending.first() {
            store.merge(bytes)?;
            sync.pending.remove(0);
        }
        store.materialize()?;
        let notes = store.notes()?;
        if s.current.is_some() {
            let active = sync
                .buffer
                .as_ref()
                .and_then(|b| notes.iter().find(|(_, n)| n.id == b.id))
                .or_else(|| {
                    s.current
                        .as_ref()
                        .and_then(|p| notes.get_key_value(&path_id(p)))
                });
            if let Some((path, note)) = active {
                s.current = Some(PathBuf::from(path));
                if s.edit.text != note.text {
                    let anchor = boundary(&note.text, s.edit.anchor);
                    let focus = boundary(&note.text, s.edit.focus);
                    let was_editing = s.live.range.is_some();
                    let focused = s.edit.focused || s.live.edit.focused;
                    s.edit = TextEdit::new(&note.text);
                    s.edit.set_selection(anchor, focus);
                    s.edit.focused = focused && !s.settings;
                    s.live = live::LiveEditor::default();
                    if was_editing
                        && focused
                        && !s.settings
                        && !s.launcher.open
                        && s.dialog.is_none()
                    {
                        live::at(s, focus);
                    }
                }
                s.saved = note.text.clone();
                s.changed_at = None;
                sync.buffer = Some(BufferBase {
                    path: path.clone(),
                    id: note.id.clone(),
                    snapshot: store.snapshot(),
                    applied: None,
                });
            } else {
                s.current = None;
                s.edit = TextEdit::new("");
                s.live = live::LiveEditor::default();
                s.saved.clear();
                sync.buffer = None;
            }
        }
        if old.keys().ne(notes.keys()) {
            s.refresh()?;
        }
        let snapshot = store.snapshot();
        if snapshot != sync.sent {
            if let Some(worker) = &sync.worker {
                worker
                    .commands
                    .send(Command::Snapshot(snapshot.clone()))
                    .map_err(|_| err("Sync worker stopped; pause and re-enable to retry"))?;
            }
            sync.sent = snapshot;
        }
        Ok(())
    })();
    sync.file_error = result.as_ref().err().map(ToString::to_string);
    s.sync = sync;
    result
}
pub fn poll(s: &mut State, tx: &mut rune::Tx) -> bool {
    if !s.sync.enabled() {
        return false;
    }
    tx.wake_after(Duration::from_millis(500));
    if s.sync
        .last_poll
        .is_some_and(|t| t.elapsed() < Duration::from_millis(500))
    {
        return false;
    }
    s.sync.last_poll = Some(Instant::now());
    let before = (
        s.sync.revision,
        s.sync.file_error.clone(),
        s.sync.last_success,
        s.sync.invitation.is_some(),
        s.sync.error.clone(),
        s.sync.ready,
        s.edit.text.clone(),
    );
    let previous_success = s.sync.success.as_ref().map(|v| v.sequence);
    let _ = flush(s);
    if s.settings
        && s.settings_tab == crate::settings::Tab::Sync
        && previous_success != s.sync.success.as_ref().map(|v| v.sequence)
    {
        tx.scroll_to_y("settings-scroll", 0.0);
    }
    (s.settings && !s.sync.requests.is_empty())
        || before
            != (
                s.sync.revision,
                s.sync.file_error.clone(),
                s.sync.last_success,
                s.sync.invitation.is_some(),
                s.sync.error.clone(),
                s.sync.ready,
                s.edit.text.clone(),
            )
}
