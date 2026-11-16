//! Durable CRDT and the strictly workspace-relative Markdown materialization boundary.
use automerge::{
    AutoCommit, ObjId, ObjType, ROOT, ReadDoc, ScalarValue, Value, transaction::Transactable,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

pub const META: &str = ".munin-sync";
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_NOTE: usize = 8 * 1024 * 1024;
const MAX_NOTES: usize = 10_000;
pub fn err(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(e.to_string())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub text: String,
}
#[derive(Default, Serialize, Deserialize)]
struct DiskState {
    document: Vec<u8>,
    base: Vec<u8>,
    manifest: BTreeMap<String, Note>,
}
pub struct Store {
    root: PathBuf,
    doc: AutoCommit,
    base: Vec<u8>,
    materialized_heads: Vec<automerge::ChangeHash>,
    manifest: BTreeMap<String, Note>,
    observed: Option<BTreeMap<String, String>>,
    _lock: fs::File,
}

pub fn validate_path(path: &str) -> io::Result<()> {
    let p = Path::new(path);
    if path.is_empty()
        || path.len() > 1024
        || path.contains(['\\', ':', '\0', '<', '>', '"', '|', '?', '*'])
        || !p.components().all(|c| matches!(c, Component::Normal(_)))
        || !p.extension().is_some_and(|e| e.eq_ignore_ascii_case("md"))
    {
        return Err(err("Sync received an unsafe Markdown path"));
    }
    for part in path.split('/') {
        let stem = part.split('.').next().unwrap().to_ascii_uppercase();
        if part.is_empty()
            || part == META
            || part.ends_with(['.', ' '])
            || part.chars().any(char::is_control)
            || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        {
            return Err(err("Sync path is not portable or targets private metadata"));
        }
    }
    Ok(())
}

pub fn atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut tmp =
        tempfile::NamedTempFile::new_in(path.parent().ok_or_else(|| err("No parent directory"))?)?;
    if let Ok(meta) = fs::symlink_metadata(path) {
        if !meta.is_file() || meta.file_type().is_symlink() {
            return Err(err("Refusing to replace a symlink or non-file"));
        }
        tmp.as_file().set_permissions(meta.permissions())?;
    }
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

impl Store {
    pub fn open(root: &Path) -> io::Result<Self> {
        let root = root.canonicalize()?;
        let meta = root.join(META);
        match fs::symlink_metadata(&meta) {
            Ok(m) if !m.is_dir() || m.file_type().is_symlink() => {
                return Err(err("Sync metadata must be a real directory"));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => fs::create_dir(&meta)?,
            Err(e) => return Err(e),
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&meta, fs::Permissions::from_mode(0o700))?;
        }
        let lock_path = meta.join("lock");
        if fs::symlink_metadata(&lock_path)
            .is_ok_and(|m| !m.is_file() || m.file_type().is_symlink())
        {
            return Err(err("Unsafe sync lock"));
        }
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        lock.try_lock_exclusive()
            .map_err(|_| err("This workspace is already syncing in another Munin process"))?;
        let path = meta.join("workspace.json");
        let state: DiskState = if path.exists() {
            if fs::symlink_metadata(&path)?.file_type().is_symlink() {
                return Err(err("Unsafe sync metadata"));
            }
            if fs::metadata(&path)?.len() > (MAX_BYTES * 10) as u64 {
                return Err(err("Sync metadata exceeds the safety limit"));
            }
            serde_json::from_slice(&fs::read(path)?).map_err(err)?
        } else {
            DiskState::default()
        };
        let doc = if state.document.is_empty() {
            AutoCommit::new()
        } else {
            AutoCommit::load(&state.document).map_err(err)?.fork()
        };
        let materialized_heads = if state.base.is_empty() {
            Vec::new()
        } else {
            AutoCommit::load(&state.base).map_err(err)?.get_heads()
        };
        let store = Self {
            root,
            doc,
            base: state.base,
            materialized_heads,
            manifest: state.manifest,
            observed: None,
            _lock: lock,
        };
        store.notes()?;
        for path in store.manifest.keys() {
            validate_path(path)?;
        }
        Ok(store)
    }
    pub fn snapshot(&mut self) -> Vec<u8> {
        self.doc.save()
    }
    fn persist(&mut self) -> io::Result<()> {
        let state = DiskState {
            document: self.snapshot(),
            base: self.base.clone(),
            manifest: self.manifest.clone(),
        };
        atomic(
            &self.root.join(META).join("workspace.json"),
            &serde_json::to_vec(&state).map_err(err)?,
        )
    }
    pub fn merge(&mut self, bytes: &[u8]) -> io::Result<()> {
        if bytes.len() > MAX_BYTES {
            return Err(err("Workspace sync exceeds 32 MiB"));
        }
        let mut incoming = AutoCommit::load(bytes).map_err(err)?;
        let mut candidate = self.doc.clone();
        candidate.merge(&mut incoming).map_err(err)?;
        self.commit(candidate)
    }
    fn commit(&mut self, mut candidate: AutoCommit) -> io::Result<()> {
        notes(&candidate)?;
        if self.doc.get_heads() == candidate.get_heads() {
            return Ok(());
        }
        if candidate.save().len() > MAX_BYTES {
            return Err(err("Sync history exceeds 32 MiB"));
        }
        let previous = std::mem::replace(&mut self.doc, candidate);
        if let Err(e) = self.persist() {
            self.doc = previous;
            return Err(e);
        }
        Ok(())
    }
    pub fn notes(&self) -> io::Result<BTreeMap<String, Note>> {
        notes(&self.doc)
    }
    fn branch(&self) -> io::Result<AutoCommit> {
        if self.base.is_empty() {
            Ok(AutoCommit::new())
        } else {
            Ok(AutoCommit::load(&self.base).map_err(err)?.fork())
        }
    }
    fn read_disk(&self) -> io::Result<BTreeMap<String, String>> {
        let files = crate::fs::scan_workspace(&self.root)?;
        if files.len() > MAX_NOTES {
            return Err(err("Sync supports at most 10,000 Markdown notes"));
        }
        let mut result = BTreeMap::new();
        let mut total = 0;
        for path in files {
            let path = path
                .to_str()
                .ok_or_else(|| err("Sync filenames must be Unicode"))?;
            let path = if cfg!(windows) {
                path.replace('\\', "/")
            } else {
                path.to_owned()
            };
            validate_path(&path)?;
            let full = self.safe_path(&path, false)?;
            if fs::metadata(&full)?.len() > MAX_NOTE as u64 {
                return Err(err("A note exceeds the 8 MiB sync limit"));
            }
            let text = fs::read_to_string(full)?;
            total += text.len();
            if total > MAX_BYTES {
                return Err(err("Workspace text exceeds the 32 MiB sync limit"));
            }
            result.insert(path, text);
        }
        Ok(result)
    }
    pub fn scan(&mut self) -> io::Result<()> {
        let mut candidate = self.doc.clone();
        let desired = self.notes()?;
        for path in self.manifest.keys().chain(desired.keys()) {
            self.safe_path(path, false)?;
        }
        let disk = self.read_disk()?;
        let mut branch = self.branch()?;
        let mut renamed = BTreeSet::new();
        for (path, old) in &self.manifest {
            if let Some(text) = disk.get(path) {
                if text != &old.text && desired.get(path).is_none_or(|n| &n.text != text) {
                    edit(&mut branch, &old.id, text)?;
                }
            } else {
                let candidates: Vec<_> = disk
                    .iter()
                    .filter(|(p, text)| {
                        !self.manifest.contains_key(*p)
                            && !desired.contains_key(*p)
                            && *text == &old.text
                            && !renamed.contains(*p)
                    })
                    .collect();
                if candidates.len() == 1 {
                    let (new, _) = candidates[0];
                    let obj = object(&branch, &old.id)?;
                    branch.put(&obj, "path", new.as_str()).map_err(err)?;
                    branch.delete(&obj, "deleted").map_err(err)?;
                    branch.put(&obj, "deleted", false).map_err(err)?;
                    renamed.insert(new.clone());
                } else {
                    let obj = object(&branch, &old.id)?;
                    branch.put(&obj, "deleted", true).map_err(err)?;
                }
            }
        }
        for (path, text) in &disk {
            if self.manifest.contains_key(path) || renamed.contains(path) {
                continue;
            }
            if let Some(existing) = desired.get(path) {
                if &existing.text != text {
                    edit(&mut candidate, &existing.id, text)?;
                }
                continue;
            }
            let id = uuid::Uuid::new_v4().simple().to_string();
            let obj = branch.put_object(ROOT, &id, ObjType::Map).map_err(err)?;
            branch.put(&obj, "path", path.as_str()).map_err(err)?;
            branch.put(&obj, "deleted", false).map_err(err)?;
            let txt = branch
                .put_object(&obj, "text", ObjType::Text)
                .map_err(err)?;
            branch.update_text(&txt, text).map_err(err)?;
        }
        candidate.merge(&mut branch).map_err(err)?;
        self.commit(candidate)?;
        self.observed = Some(disk);
        Ok(())
    }
    pub fn edit_from(&mut self, base: &[u8], path: &str, text: &str) -> io::Result<Vec<u8>> {
        let mut branch = AutoCommit::load(base).map_err(err)?.fork();
        let note = notes(&branch)?
            .remove(path)
            .ok_or_else(|| err("The edited note is not in the sync baseline"))?;
        if note.text != text {
            edit(&mut branch, &note.id, text)?;
        }
        let mut candidate = self.doc.clone();
        candidate.merge(&mut branch).map_err(err)?;
        self.commit(candidate)?;
        Ok(branch.save())
    }
    fn safe_path(&self, path: &str, create: bool) -> io::Result<PathBuf> {
        validate_path(path)?;
        let mut full = self.root.clone();
        let parts: Vec<_> = path.split('/').collect();
        for (i, part) in parts.iter().enumerate() {
            full.push(part);
            match fs::symlink_metadata(&full) {
                Ok(m) if m.file_type().is_symlink() => return Err(err("Sync refuses symlinks")),
                Ok(m) if i + 1 < parts.len() && !m.is_dir() => {
                    return Err(err("Sync path is blocked by a file"));
                }
                Ok(m) if i + 1 == parts.len() && !m.is_file() => {
                    return Err(err("Sync note is blocked by a directory"));
                }
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    if create && i + 1 < parts.len() {
                        fs::create_dir(&full)?;
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Ok(full)
    }
    pub fn materialize(&mut self) -> io::Result<()> {
        let desired = self.notes()?;
        let disk = self.read_disk()?;
        if let Some(observed) = &self.observed
            && observed != &disk
        {
            return Err(err(
                "Files changed during sync; retrying without overwriting them",
            ));
        }
        if self.observed.is_none() {
            for (path, old) in &self.manifest {
                if disk.get(path) != Some(&old.text)
                    && disk.get(path) != desired.get(path).map(|n| &n.text)
                {
                    return Err(err(
                        "Local changes need scanning before remote materialization",
                    ));
                }
            }
        }
        // Validate the whole plan before touching any file, including symlink parents.
        for path in desired.keys().chain(self.manifest.keys()) {
            self.safe_path(path, false)?;
        }
        let heads = self.doc.get_heads();
        if self.manifest == desired && self.materialized_heads == heads {
            self.observed = None;
            return Ok(());
        }
        for (path, note) in &desired {
            if disk.get(path) == Some(&note.text) {
                continue;
            }
            if !self.manifest.contains_key(path) && disk.contains_key(path) {
                return Err(err(
                    "A local note blocks a synced note; refresh to preserve both",
                ));
            }
            let full = self.safe_path(path, true)?;
            atomic(&full, note.text.as_bytes())?;
        }
        for path in self.manifest.keys() {
            if !desired.contains_key(path) && disk.contains_key(path) {
                fs::remove_file(self.safe_path(path, false)?)?;
            }
        }
        self.manifest = desired;
        self.base = self.snapshot();
        self.materialized_heads = heads;
        self.observed = None;
        self.persist()
    }
}

fn object(doc: &AutoCommit, id: &str) -> io::Result<ObjId> {
    match doc.get(ROOT, id).map_err(err)? {
        Some((Value::Object(ObjType::Map), obj)) => Ok(obj),
        _ => Err(err("Invalid synced note record")),
    }
}
fn edit(doc: &mut AutoCommit, id: &str, value: &str) -> io::Result<()> {
    if value.len() > MAX_NOTE {
        return Err(err("Note exceeds the 8 MiB sync limit"));
    }
    let obj = object(doc, id)?;
    let (_, txt) = doc
        .get(&obj, "text")
        .map_err(err)?
        .ok_or_else(|| err("Missing note text"))?;
    doc.update_text(&txt, value).map_err(err)?;
    // Force a causal live-state write even if the previous value was false.
    // Automerge optimizes identical puts away; that would let a concurrent delete win.
    doc.delete(&obj, "deleted").map_err(err)?;
    doc.put(&obj, "deleted", false).map_err(err)?;
    Ok(())
}
fn string(doc: &AutoCommit, obj: &ObjId, key: &str) -> io::Result<String> {
    match doc.get(obj, key).map_err(err)? {
        Some((Value::Scalar(v), _)) => v
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| err("Invalid note property")),
        _ => Err(err("Missing note property")),
    }
}
pub(super) fn document_notes(doc: &AutoCommit) -> io::Result<BTreeMap<String, Note>> {
    notes(doc)
}
fn notes(doc: &AutoCommit) -> io::Result<BTreeMap<String, Note>> {
    let mut result = BTreeMap::new();
    let mut total = 0;
    let keys: Vec<_> = doc.keys(ROOT).collect();
    if keys.len() > MAX_NOTES {
        return Err(err("Sync history exceeds 10,000 note records"));
    }
    // Reserve all original paths so conflict names cannot shadow naturally named notes.
    let mut records = Vec::new();
    let mut reserved = BTreeSet::new();
    for id in keys {
        if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(err("Invalid synced note identity"));
        }
        let obj = object(doc, &id)?;
        let path = string(doc, &obj, "path")?;
        validate_path(&path)?;
        let (_, txt) = doc
            .get(&obj, "text")
            .map_err(err)?
            .filter(|(v, _)| matches!(v, Value::Object(ObjType::Text)))
            .ok_or_else(|| err("Invalid synced text"))?;
        let text = doc.text(txt).map_err(err)?;
        total += text.len();
        if text.len() > MAX_NOTE || total > MAX_BYTES {
            return Err(err("Synced text exceeds safety limits"));
        }
        let deleted = doc.get_all(&obj, "deleted").map_err(err)?;
        if deleted.is_empty() || deleted.iter().any(|(v, _)| !matches!(v, Value::Scalar(s) if matches!(s.as_ref(), ScalarValue::Boolean(_)))) { return Err(err("Invalid note deletion state")); }
        // A concurrent edit writes false and wins over a concurrent tombstone.
        if deleted.iter().all(
            |(v, _)| matches!(v, Value::Scalar(s) if s.as_ref() == &ScalarValue::Boolean(true)),
        ) {
            continue;
        }
        reserved.insert(path.to_lowercase());
        records.push((path, Note { id, text }));
    }
    let mut used = BTreeSet::new();
    for (mut path, note) in records {
        if !used.insert(path.to_lowercase()) {
            let p = Path::new(&path);
            let stem = p.file_stem().unwrap().to_str().unwrap();
            let parent = p.parent().unwrap();
            let mut n = 0;
            loop {
                let suffix = if n == 0 {
                    String::new()
                } else {
                    format!("-{n}")
                };
                let candidate =
                    parent.join(format!("{stem} (conflict {}{suffix}).md", &note.id[..8]));
                let candidate = candidate
                    .to_str()
                    .unwrap()
                    .replace(std::path::MAIN_SEPARATOR, "/");
                if !reserved.contains(&candidate.to_lowercase())
                    && used.insert(candidate.to_lowercase())
                {
                    path = candidate;
                    break;
                }
                n += 1;
            }
        }
        result.insert(path, note);
    }
    Ok(result)
}
