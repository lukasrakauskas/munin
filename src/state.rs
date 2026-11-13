use rune::TextEdit;
use rune_kit::{FileNode, TreeModel};

pub fn path_id(path: &Path) -> String {
    let s = path.to_string_lossy();
    if cfg!(windows) {
        s.replace('\\', "/")
    } else {
        s.into_owned()
    }
}
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

pub struct State {
    pub workspace: Option<PathBuf>,
    pub current: Option<PathBuf>,
    pub edit: TextEdit,
    pub saved: String,
    pub files: TreeModel<FileNode<()>>,
    pub error: Option<String>,
    pub sidebar: bool,
    pub theme: u8,
    pub dialog: Option<&'static str>,
    pub input: TextEdit,
    pub changed_at: Option<std::time::Instant>,
    pub recovered: bool,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Preferences {
    workspace: Option<PathBuf>,
    file: Option<PathBuf>,
    theme: u8,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Recovery {
    workspace: PathBuf,
    file: PathBuf,
    saved: String,
    text: String,
}

impl State {
    pub fn clear_recovery(&mut self, path: &Path) -> io::Result<()> {
        if self.recovered && self.edit.text == self.saved {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
            self.recovered = false;
        }
        Ok(())
    }
    pub fn write_recovery(&self, path: &Path) -> io::Result<()> {
        let data = Recovery {
            workspace: self
                .workspace
                .clone()
                .ok_or_else(|| io::Error::other("No workspace"))?,
            file: self
                .current
                .clone()
                .ok_or_else(|| io::Error::other("No file"))?,
            saved: self.saved.clone(),
            text: self.edit.text.clone(),
        };
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("No recovery directory"))?;
        std::fs::create_dir_all(parent)?;
        let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer(&mut tmp, &data)?;
        tmp.flush()?;
        tmp.persist(path).map_err(|e| e.error)?;
        Ok(())
    }
    pub fn load_recovery(&mut self, path: &Path) -> io::Result<()> {
        let data: Recovery = serde_json::from_reader(std::fs::File::open(path)?)?;
        if !data.workspace.is_dir()
            || !data
                .file
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
        {
            return Err(io::Error::other("Recovery workspace unavailable"));
        }
        self.workspace = Some(data.workspace);
        self.current = Some(data.file);
        self.saved = data.saved;
        self.edit = TextEdit::new(&data.text);
        self.refresh()?;
        self.recovered = true;
        self.error =
            Some("Recovered unsaved edits. Resolve any disk conflict before saving.".into());
        Ok(())
    }
    pub fn find_notes(&self, query: &str) -> Vec<PathBuf> {
        self.workspace
            .as_deref()
            .and_then(|root| crate::fs::scan_workspace(root).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|p| {
                p.to_string_lossy()
                    .to_lowercase()
                    .contains(&query.to_lowercase())
            })
            .collect()
    }
    pub fn read_preferences(path: &Path) -> Self {
        let prefs: Preferences = std::fs::read(path)
            .ok()
            .and_then(|s| serde_json::from_slice(&s).ok())
            .unwrap_or_default();
        let mut s = Self::restore(prefs.workspace, prefs.file);
        s.theme = if prefs.theme <= 2 { prefs.theme } else { 0 };
        s
    }
    pub fn write_preferences(&self, path: &Path) -> io::Result<()> {
        let prefs = Preferences {
            workspace: self.workspace.clone(),
            file: self.current.clone(),
            theme: self.theme,
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_vec(&prefs)?)
    }
    pub fn restore(workspace: Option<PathBuf>, file: Option<PathBuf>) -> Self {
        let workspace = workspace.filter(|p| p.is_dir());
        let mut s = Self {
            workspace,
            current: None,
            edit: TextEdit::new(""),
            saved: String::new(),
            files: TreeModel::new(),
            error: None,
            sidebar: true,
            theme: 0,
            dialog: None,
            input: TextEdit::new(""),
            changed_at: None,
            recovered: false,
        };
        let _ = s.refresh();
        if let Some(file) = file {
            let _ = s.open_file(file);
        }
        s
    }
    fn destination(&self, name: &str, markdown: bool) -> io::Result<PathBuf> {
        let path = Path::new(name);
        if name.is_empty()
            || !path
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
        {
            return Err(io::Error::other("Invalid name"));
        }
        let mut relative = path.to_path_buf();
        if markdown
            && !relative
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("md"))
        {
            relative.set_extension("md");
        }
        let root = self
            .workspace
            .as_ref()
            .ok_or_else(|| io::Error::other("No workspace"))?;
        let parent = root.join(relative.parent().unwrap()).canonicalize()?;
        if !parent.starts_with(root.canonicalize()?) {
            return Err(io::Error::other("Path outside workspace"));
        }
        Ok(relative)
    }
    pub fn create_note(&mut self, name: &str) -> io::Result<()> {
        self.save()?;
        let rel = self.destination(name, true)?;
        let path = self.workspace.as_ref().unwrap().join(&rel);
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        self.refresh()?;
        self.open_file(rel)
    }
    pub fn create_folder(&mut self, name: &str) -> io::Result<()> {
        let rel = self.destination(name, false)?;
        std::fs::create_dir(self.workspace.as_ref().unwrap().join(rel))?;
        self.refresh()
    }
    pub fn rename_current(&mut self, name: &str) -> io::Result<()> {
        self.save()?;
        let current = self
            .current
            .clone()
            .ok_or_else(|| io::Error::other("No note selected"))?;
        let destination = current.parent().unwrap().join(name);
        let rel = self.destination(&destination.to_string_lossy(), true)?;
        let root = self.workspace.as_ref().unwrap();
        std::fs::hard_link(root.join(&current), root.join(&rel))?;
        if let Err(e) = std::fs::remove_file(root.join(&current)) {
            let _ = std::fs::remove_file(root.join(&rel));
            return Err(e);
        }
        self.current = Some(rel);
        self.refresh()
    }
    pub fn delete_current(&mut self) -> io::Result<()> {
        self.save()?;
        let current = self
            .current
            .as_ref()
            .ok_or_else(|| io::Error::other("No note selected"))?;
        let path = self.workspace.as_ref().unwrap().join(current);
        if std::fs::read_to_string(&path)? != self.saved {
            return Err(io::Error::other("File changed on disk"));
        }
        std::fs::remove_file(path)?;
        self.current = None;
        self.edit = TextEdit::new("");
        self.saved.clear();
        self.refresh()
    }
    pub fn refresh(&mut self) -> io::Result<()> {
        let mut tree = TreeModel::new().key("files");
        fn visit(root: &Path, rel: &Path, tree: &mut TreeModel<FileNode<()>>) -> io::Result<()> {
            let mut entries: Vec<_> =
                std::fs::read_dir(root.join(rel))?.collect::<io::Result<_>>()?;
            entries.sort_by_key(|e| e.file_name());
            for e in entries {
                let ty = e.file_type()?;
                if !ty.is_dir()
                    && !(ty.is_file()
                        && e.path()
                            .extension()
                            .is_some_and(|s| s.eq_ignore_ascii_case("md")))
                {
                    continue;
                }
                let path = rel.join(e.file_name());
                let id = path_id(&path);
                let parent = (!rel.as_os_str().is_empty()).then(|| path_id(rel));
                tree.insert(
                    parent.as_deref(),
                    id.clone(),
                    FileNode {
                        name: e.file_name().to_string_lossy().into_owned(),
                        path: id,
                        dir: ty.is_dir(),
                        data: if ty.is_dir() { None } else { Some(()) },
                    },
                );
                if ty.is_dir() {
                    visit(root, &path, tree)?;
                }
            }
            Ok(())
        }
        if let Some(root) = &self.workspace {
            visit(root, Path::new(""), &mut tree)?;
        }
        self.files = tree;
        if let Some(path) = self.current.clone() {
            self.select_active(&path);
        }
        Ok(())
    }
    fn select_active(&mut self, path: &Path) {
        let mut parent = path.parent();
        while let Some(p) = parent.filter(|p| !p.as_os_str().is_empty()) {
            self.files.set_expanded(&path_id(p), true);
            parent = p.parent();
        }
        self.files.select(&path_id(path));
    }
    pub fn save(&mut self) -> io::Result<()> {
        let Some(path) = self
            .current
            .as_ref()
            .and_then(|p| self.workspace.as_ref().map(|w| w.join(p)))
        else {
            return Ok(());
        };
        if self.edit.text == self.saved {
            return Ok(());
        }
        if std::fs::read_to_string(&path)? != self.saved {
            return Err(io::Error::other(
                "File changed on disk; your edits remain unsaved",
            ));
        }
        let mut tmp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
        tmp.as_file()
            .set_permissions(std::fs::metadata(&path)?.permissions())?;
        tmp.write_all(self.edit.text.as_bytes())?;
        tmp.flush()?;
        tmp.as_file().sync_all()?;
        tmp.persist(&path).map_err(|e| e.error)?;
        self.saved = self.edit.text.clone();
        Ok(())
    }
    pub fn open_file(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        self.save()?;
        let path = path.as_ref();
        if !path
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
        {
            return Err(io::Error::other("Invalid file path"));
        }
        let root = self
            .workspace
            .as_ref()
            .ok_or_else(|| io::Error::other("No workspace"))?;
        if !path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("md"))
        {
            return Err(io::Error::other("Only Markdown files can be opened"));
        }
        let full = root.join(path);
        if std::fs::symlink_metadata(&full)?.file_type().is_symlink()
            || !full.canonicalize()?.starts_with(root.canonicalize()?)
        {
            return Err(io::Error::other(
                "File is outside workspace or is a symlink",
            ));
        }
        let text = std::fs::read_to_string(full)?;
        self.edit = TextEdit::new(&text);
        self.saved = text;
        self.current = Some(path.to_path_buf());
        self.select_active(path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn root() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "munin-state-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn missing_file_save_failure_keeps_dirty_buffer_and_blocks_switch() {
        let p = root();
        fs::write(p.join("first.md"), "first").unwrap();
        fs::write(p.join("second.md"), "second").unwrap();
        let mut s = State::restore(Some(p.clone()), None);
        s.open_file("first.md").unwrap();
        s.edit.text = "unsaved".into();
        fs::remove_file(p.join("first.md")).unwrap();
        assert!(s.open_file("second.md").is_err());
        assert_eq!(s.edit.text, "unsaved");
        assert_eq!(s.current.as_deref(), Some(std::path::Path::new("first.md")));
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn recovery_restores_unsaved_conflict_without_overwriting_disk() {
        let p = root();
        fs::write(p.join("note.md"), "original").unwrap();
        let mut s = State::restore(Some(p.clone()), None);
        s.open_file("note.md").unwrap();
        s.edit.text = "unsaved".into();
        fs::write(p.join("note.md"), "external").unwrap();
        let recovery = p.join("recovery.json");
        s.write_recovery(&recovery).unwrap();
        let mut restored = State::restore(None, None);
        restored.load_recovery(&recovery).unwrap();
        assert_eq!(restored.edit.text, "unsaved");
        assert!(restored.save().is_err());
        assert_eq!(fs::read_to_string(p.join("note.md")).unwrap(), "external");
        fs::write(p.join("note.md"), "original").unwrap();
        restored.save().unwrap();
        restored.clear_recovery(&recovery).unwrap();
        assert!(!recovery.exists());
        fs::remove_dir_all(p).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn save_preserves_existing_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let p = root();
        fs::write(p.join("note.md"), "old").unwrap();
        fs::set_permissions(p.join("note.md"), fs::Permissions::from_mode(0o644)).unwrap();
        let mut s = State::restore(Some(p.clone()), None);
        s.open_file("note.md").unwrap();
        s.edit.text = "new".into();
        s.save().unwrap();
        assert_eq!(
            fs::metadata(p.join("note.md"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o644
        );
        fs::remove_dir_all(p).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn unix_backslash_filename_is_preserved_in_tree() {
        let p = root();
        fs::write(p.join("back\\slash.md"), "note").unwrap();
        let mut s = State::restore(Some(p.clone()), None);
        assert!(s.files.get("back\\slash.md").is_some());
        s.open_file("back\\slash.md").unwrap();
        assert_eq!(s.edit.text, "note");
        fs::remove_dir_all(p).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn symlink_note_is_not_opened_outside_workspace() {
        let p = root();
        let external = tempfile::tempdir().unwrap();
        fs::write(external.path().join("secret.md"), "secret").unwrap();
        std::os::unix::fs::symlink(external.path().join("secret.md"), p.join("link.md")).unwrap();
        let mut s = State::restore(Some(p.clone()), None);
        assert!(s.open_file("link.md").is_err());
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn opened_file_is_selected_and_parent_is_expanded() {
        let p = root();
        fs::create_dir(p.join("folder")).unwrap();
        fs::write(p.join("folder/note.md"), "").unwrap();
        let mut s = State::restore(Some(p.clone()), None);
        s.open_file("folder/note.md").unwrap();
        assert!(s.files.is_expanded("folder"));
        assert!(s.files.selected().any(|id| id == "folder/note.md"));
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn quick_open_filters_workspace_notes() {
        let p = root();
        fs::write(p.join("first.md"), "").unwrap();
        fs::write(p.join("second.md"), "").unwrap();
        let s = State::restore(Some(p.clone()), None);
        assert_eq!(
            s.find_notes("COND"),
            vec![std::path::PathBuf::from("second.md")]
        );
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn invalid_preferences_fall_back_to_system_theme() {
        let p = root();
        let path = p.join("state.json");
        fs::write(&path, "{\"theme\":99}").unwrap();
        assert_eq!(State::read_preferences(&path).theme, 0);
        fs::write(&path, "broken json").unwrap();
        assert_eq!(State::read_preferences(&path).theme, 0);
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn preferences_round_trip() {
        let p = root();
        let settings = p.join("settings.json");
        let mut s = State::restore(Some(p.clone()), None);
        s.theme = 2;
        s.write_preferences(&settings).unwrap();
        let loaded = State::read_preferences(&settings);
        assert_eq!(loaded.workspace.as_deref(), Some(p.as_path()));
        assert_eq!(loaded.theme, 2);
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn create_rename_delete_rejects_traversal_and_collisions() {
        let p = root();
        let mut s = State::restore(Some(p.clone()), None);
        assert!(s.create_note("../escape").is_err());
        s.create_folder("folder").unwrap();
        s.create_note("folder/note").unwrap();
        assert!(p.join("folder/note.md").exists());
        assert!(s.create_note("folder/note").is_err());
        s.rename_current("renamed").unwrap();
        assert!(p.join("folder/renamed.md").exists());
        assert!(s.files.is_expanded("folder"));
        assert!(s.files.selected().any(|id| id == "folder/renamed.md"));
        s.delete_current().unwrap();
        assert!(!p.join("folder/renamed.md").exists());
        fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn missing_workspace_is_not_restored() {
        assert!(
            State::restore(
                Some(std::path::PathBuf::from("/not-a-munin-workspace")),
                None
            )
            .workspace
            .is_none()
        );
    }
    #[test]
    fn switching_saves_unicode_and_conflicts_preserve_buffer() {
        let p = root();
        fs::write(p.join("one.md"), "one").unwrap();
        fs::write(p.join("two.md"), "two").unwrap();
        let mut s = State::restore(Some(p.clone()), None);
        s.open_file("one.md").unwrap();
        s.edit.text = "Žąsis 🐦".into();
        s.open_file("two.md").unwrap();
        assert_eq!(fs::read_to_string(p.join("one.md")).unwrap(), "Žąsis 🐦");
        s.edit.text = "unsaved".into();
        fs::write(p.join("two.md"), "external").unwrap();
        assert!(s.open_file("one.md").is_err());
        assert_eq!(s.edit.text, "unsaved");
        assert_eq!(fs::read_to_string(p.join("two.md")).unwrap(), "external");
        fs::remove_dir_all(p).unwrap();
    }
}
