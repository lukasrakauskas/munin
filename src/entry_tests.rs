use crate::state::State;
use std::{fs, path::Path};

#[test]
fn renaming_folder_containing_active_note_updates_path_and_saves_edits() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("folder")).unwrap();
    fs::write(dir.path().join("folder/note.md"), "old").unwrap();
    let mut s = State::restore(Some(dir.path().into()), Some("folder/note.md".into()));
    s.edit.text = "new".into();
    s.rename_entry(Path::new("folder"), "renamed").unwrap();
    assert_eq!(s.current.as_deref(), Some(Path::new("renamed/note.md")));
    assert_eq!(
        fs::read_to_string(dir.path().join("renamed/note.md")).unwrap(),
        "new"
    );
    assert!(!dir.path().join("folder").exists());
    assert_eq!(
        s.recents.first().map(|p| p.as_path()),
        Some(Path::new("renamed/note.md"))
    );
}

#[test]
fn entry_mutations_reject_collisions_traversal_and_conflicts() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("note.md"), "note").unwrap();
    fs::write(dir.path().join("other.md"), "other").unwrap();
    let mut s = State::restore(Some(dir.path().into()), Some("note.md".into()));
    assert!(s.rename_entry(Path::new("note.md"), "other").is_err());
    assert!(s.rename_entry(Path::new("note.md"), "../outside").is_err());
    assert!(s.delete_entry(Path::new("../outside.md")).is_err());
    assert!(s.delete_entry(Path::new("")).is_err());
    s.edit.text = "dirty".into();
    fs::write(dir.path().join("note.md"), "external").unwrap();
    assert!(s.rename_entry(Path::new("note.md"), "renamed").is_err());
    assert!(s.delete_entry(Path::new("note.md")).is_err());
    assert_eq!(s.edit.text, "dirty");
    assert_eq!(
        fs::read_to_string(dir.path().join("note.md")).unwrap(),
        "external"
    );
}

#[test]
fn deleting_inactive_note_preserves_active_dirty_buffer_and_only_empty_folders_delete() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("note.md"), "active").unwrap();
    fs::write(dir.path().join("other.md"), "other").unwrap();
    fs::create_dir(dir.path().join("empty")).unwrap();
    let mut s = State::restore(Some(dir.path().into()), Some("note.md".into()));
    s.edit.text = "dirty".into();
    s.delete_entry(Path::new("other.md")).unwrap();
    s.delete_entry(Path::new("empty")).unwrap();
    assert_eq!(s.current.as_deref(), Some(Path::new("note.md")));
    assert_eq!(s.edit.text, "dirty");
    assert_eq!(
        fs::read_to_string(dir.path().join("note.md")).unwrap(),
        "active"
    );
    fs::create_dir(dir.path().join("full")).unwrap();
    fs::write(dir.path().join("full/keep.txt"), "keep").unwrap();
    assert!(s.delete_entry(Path::new("full")).is_err());
    assert!(dir.path().join("full/keep.txt").exists());
}

#[cfg(unix)]
#[test]
fn entry_mutations_refuse_symlinks_and_symlink_parents() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("note.md"), "outside").unwrap();
    std::os::unix::fs::symlink(outside.path().join("note.md"), dir.path().join("link.md")).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("alias")).unwrap();
    let mut s = State::restore(Some(dir.path().into()), None);
    assert!(s.rename_entry(Path::new("link.md"), "new").is_err());
    assert!(s.delete_entry(Path::new("link.md")).is_err());
    assert!(s.rename_entry(Path::new("alias/note.md"), "new").is_err());
    assert!(s.delete_entry(Path::new("alias/note.md")).is_err());
    assert_eq!(
        fs::read_to_string(outside.path().join("note.md")).unwrap(),
        "outside"
    );
}
