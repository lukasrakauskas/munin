use super::store::Store;
use std::fs;

fn seed(dir: &tempfile::TempDir, text: &str) -> Store {
    fs::write(dir.path().join("note.md"), text).unwrap();
    let mut s = Store::open(dir.path()).unwrap();
    s.scan().unwrap();
    s.materialize().unwrap();
    s
}

#[test]
fn offline_unicode_edits_converge_and_survive_reload() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let mut left = seed(&a, "Žąsis\r\n");
    let mut right = Store::open(b.path()).unwrap();
    right.merge(&left.snapshot()).unwrap();
    right.materialize().unwrap();
    fs::write(a.path().join("note.md"), "Hello Žąsis\r\n").unwrap();
    fs::write(b.path().join("note.md"), "Žąsis 🐦\r\n").unwrap();
    left.scan().unwrap();
    right.scan().unwrap();
    left.merge(&right.snapshot()).unwrap();
    right.merge(&left.snapshot()).unwrap();
    left.materialize().unwrap();
    right.materialize().unwrap();
    assert_eq!(
        fs::read_to_string(a.path().join("note.md")).unwrap(),
        "Hello Žąsis 🐦\r\n"
    );
    assert_eq!(
        fs::read_to_string(b.path().join("note.md")).unwrap(),
        "Hello Žąsis 🐦\r\n"
    );
    drop(left);
    let mut reopened = Store::open(a.path()).unwrap();
    reopened.scan().unwrap();
    reopened.materialize().unwrap();
    assert_eq!(reopened.notes().unwrap().len(), 1);
    assert_eq!(
        fs::read_to_string(a.path().join("note.md")).unwrap(),
        "Hello Žąsis 🐦\r\n"
    );
}

#[test]
fn colliding_creations_preserve_both_notes() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let mut left = seed(&a, "left");
    let mut right = seed(&b, "right");
    left.merge(&right.snapshot()).unwrap();
    right.merge(&left.snapshot()).unwrap();
    left.materialize().unwrap();
    right.materialize().unwrap();
    let values: std::collections::BTreeSet<_> = left
        .notes()
        .unwrap()
        .into_values()
        .map(|n| n.text)
        .collect();
    assert_eq!(values, ["left".to_string(), "right".to_string()].into());
    assert_eq!(left.notes().unwrap(), right.notes().unwrap());
}

#[test]
fn renames_and_deletes_propagate_but_edit_delete_retains_concurrent_text() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let mut left = seed(&a, "base");
    let mut right = Store::open(b.path()).unwrap();
    right.merge(&left.snapshot()).unwrap();
    right.materialize().unwrap();
    fs::rename(a.path().join("note.md"), a.path().join("renamed.md")).unwrap();
    left.scan().unwrap();
    right.merge(&left.snapshot()).unwrap();
    right.materialize().unwrap();
    assert!(!b.path().join("note.md").exists());
    assert_eq!(
        fs::read_to_string(b.path().join("renamed.md")).unwrap(),
        "base"
    );
    left.materialize().unwrap();
    fs::remove_file(a.path().join("renamed.md")).unwrap();
    fs::write(b.path().join("renamed.md"), "edited offline").unwrap();
    left.scan().unwrap();
    right.scan().unwrap();
    left.merge(&right.snapshot()).unwrap();
    left.materialize().unwrap();
    assert_eq!(
        fs::read_to_string(a.path().join("renamed.md")).unwrap(),
        "edited offline"
    );
    right.merge(&left.snapshot()).unwrap();
    right.materialize().unwrap();
    fs::remove_file(a.path().join("renamed.md")).unwrap();
    left.scan().unwrap();
    right.merge(&left.snapshot()).unwrap();
    right.materialize().unwrap();
    assert!(!b.path().join("renamed.md").exists());
}

#[test]
fn pending_remote_materialization_survives_restart_without_becoming_local_deletion() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let mut left = seed(&a, "base");
    let mut right = Store::open(b.path()).unwrap();
    right.merge(&left.snapshot()).unwrap();
    right.materialize().unwrap();
    fs::write(a.path().join("new.md"), "remote new").unwrap();
    fs::write(a.path().join("note.md"), "remote edit").unwrap();
    left.scan().unwrap();
    right.merge(&left.snapshot()).unwrap();
    drop(right);
    let mut right = Store::open(b.path()).unwrap();
    right.scan().unwrap();
    right.materialize().unwrap();
    assert_eq!(
        fs::read_to_string(b.path().join("new.md")).unwrap(),
        "remote new"
    );
    assert_eq!(
        fs::read_to_string(b.path().join("note.md")).unwrap(),
        "remote edit"
    );
}

#[test]
fn buffer_branch_merges_with_external_edits_instead_of_overwriting() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = seed(&dir, "middle");
    let base = s.snapshot();
    fs::write(dir.path().join("note.md"), "before middle").unwrap();
    s.scan().unwrap();
    s.edit_from(&base, "note.md", "middle after").unwrap();
    s.materialize().unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("note.md")).unwrap(),
        "before middle after"
    );
}

#[test]
fn forgetting_a_device_is_persistent_without_resetting_identity() {
    let dir = tempfile::tempdir().unwrap();
    let peer = iroh::SecretKey::generate().public();
    let mut sync = super::Sync::load(Some(dir.path()));
    sync.set_enabled(true).unwrap();
    let identity = sync.device_id().unwrap();
    sync.config
        .as_mut()
        .unwrap()
        .peers
        .push(super::network::Peer {
            addr: peer.into(),
            name: "Other device".into(),
        });
    sync.save_config().unwrap();
    sync.set_enabled(false).unwrap();
    drop(sync);
    let mut sync = super::Sync::load(Some(dir.path()));
    assert_eq!(sync.peers().len(), 1);
    sync.forget(&peer.to_string()).unwrap();
    drop(sync);
    let sync = super::Sync::load(Some(dir.path()));
    assert!(sync.peers().is_empty());
    assert_eq!(sync.device_id().unwrap(), identity);
}

#[test]
fn failed_device_removal_stops_networking_instead_of_hiding_live_authorization() {
    let dir = tempfile::tempdir().unwrap();
    let peer = iroh::SecretKey::generate().public();
    let mut sync = super::Sync::load(Some(dir.path()));
    sync.set_enabled(true).unwrap();
    sync.config
        .as_mut()
        .unwrap()
        .peers
        .push(super::network::Peer {
            addr: peer.into(),
            name: "Other device".into(),
        });
    sync.set_enabled(true).unwrap();
    let file = dir.path().join(".munin-sync/device.json");
    fs::remove_file(&file).unwrap();
    fs::create_dir(&file).unwrap();
    assert!(sync.forget(&peer.to_string()).is_err());
    assert!(
        sync.worker.is_none(),
        "failed persistence must not leave a hidden authorized network session"
    );
}

#[test]
fn unsafe_paths_and_symlink_parents_are_rejected_and_metadata_is_hidden() {
    for path in [
        "../escape.md",
        "/escape.md",
        "a/../../b.md",
        "a\\evil.md",
        "C:/evil.md",
        ".munin-sync/leak.md",
        "CON.md",
        "bad. /x.md",
    ] {
        assert!(super::store::validate_path(path).is_err(), "{path}");
    }
    let dir = tempfile::tempdir().unwrap();
    let mut store = seed(&dir, "safe");
    assert!(
        Store::open(dir.path()).is_err(),
        "second writer must not acquire workspace"
    );
    fs::write(dir.path().join(".munin-sync/secret.md"), "secret").unwrap();
    store.scan().unwrap();
    assert_eq!(store.notes().unwrap().len(), 1);
    assert_eq!(crate::fs::scan_workspace(dir.path()).unwrap().len(), 1);
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        fs::create_dir(source.path().join("nested")).unwrap();
        fs::write(source.path().join("nested/attack.md"), "remote").unwrap();
        let mut remote = Store::open(source.path()).unwrap();
        remote.scan().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("nested")).unwrap();
        store.merge(&remote.snapshot()).unwrap();
        assert!(store.materialize().is_err());
        assert!(!outside.path().join("attack.md").exists());
    }
}
