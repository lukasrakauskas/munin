use super::store::Store;
use std::fs;

fn seeded(dir: &tempfile::TempDir) -> Store {
    fs::write(dir.path().join("note.md"), "base").unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    store.scan().unwrap();
    store.materialize().unwrap();
    store
}
#[test]
fn stale_materialization_never_overwrites_an_unscanned_local_edit() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let mut left = seeded(&a);
    let mut right = Store::open(b.path()).unwrap();
    right.merge(&left.snapshot()).unwrap();
    right.materialize().unwrap();
    fs::write(a.path().join("note.md"), "remote base").unwrap();
    left.scan().unwrap();
    fs::write(b.path().join("note.md"), "base local").unwrap();
    right.merge(&left.snapshot()).unwrap();
    assert!(right.materialize().is_err());
    assert_eq!(
        fs::read_to_string(b.path().join("note.md")).unwrap(),
        "base local"
    );
    right.scan().unwrap();
    right.materialize().unwrap();
    assert_eq!(
        fs::read_to_string(b.path().join("note.md")).unwrap(),
        "remote base local"
    );
}
#[cfg(unix)]
#[test]
fn blocked_scan_does_not_tombstone_a_tracked_symlink_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let mut store = seeded(&dir);
    fs::write(outside.path().join("secret.md"), "private").unwrap();
    fs::remove_file(dir.path().join("note.md")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("secret.md"), dir.path().join("note.md"))
        .unwrap();
    assert!(store.scan().is_err());
    fs::remove_file(dir.path().join("note.md")).unwrap();
    fs::write(dir.path().join("note.md"), "base").unwrap();
    store.scan().unwrap();
    store.materialize().unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("note.md")).unwrap(),
        "base"
    );
    assert_eq!(
        fs::read_to_string(outside.path().join("secret.md")).unwrap(),
        "private"
    );
}
#[test]
fn idle_sync_does_not_rewrite_durable_history() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = seeded(&dir);
    let metadata = dir.path().join(".munin-sync/workspace.json");
    let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(10_000);
    fs::File::open(&metadata)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(old))
        .unwrap();
    store.scan().unwrap();
    store.materialize().unwrap();
    assert_eq!(fs::metadata(metadata).unwrap().modified().unwrap(), old);
}

#[test]
fn partial_materialization_does_not_duplicate_new_notes_on_restart() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let mut left = seeded(&a);
    let mut right = Store::open(b.path()).unwrap();
    right.merge(&left.snapshot()).unwrap();
    right.materialize().unwrap();
    fs::write(a.path().join("new.md"), "new").unwrap();
    left.scan().unwrap();
    right.merge(&left.snapshot()).unwrap();
    fs::write(b.path().join("new.md"), "new").unwrap(); // crash after first write, before manifest commit
    drop(right);
    let mut right = Store::open(b.path()).unwrap();
    right.scan().unwrap();
    right.materialize().unwrap();
    assert_eq!(right.notes().unwrap().len(), 2);
}
