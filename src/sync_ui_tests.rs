use super::*;
use rune::{Env, Runtime};
use std::fs;

fn runtime(width: f32) -> (tempfile::TempDir, Runtime<State>) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("note.md"), "middle").unwrap();
    let mut s = State::restore(Some(dir.path().to_owned()), None);
    s.open_file("note.md").unwrap();
    let mut rt = Runtime::new(
        s,
        view,
        (width, 700.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.on_key = Some(key);
    rt.frame(0.0);
    (dir, rt)
}
fn click(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.3);
}
#[test]
fn settings_opens_from_button_and_shortcut_without_editing_note() {
    let (_dir, mut rt) = runtime(1000.0);
    click(&mut rt, "open-settings");
    assert!(rt.state.settings);
    assert!(rt.find("sync-settings").is_some());
    rt.ime(rune::Ime::Commit("not a note edit".into()));
    assert_eq!(rt.state.edit.text, "middle");
    rt.key(rune::KeyEvent {
        key: rune::Key::Escape,
        mods: Default::default(),
    });
    rt.frame(0.0);
    assert!(!rt.state.settings);
    rt.key(rune::KeyEvent {
        key: rune::Key::Command(','),
        mods: rune::Mods {
            cmd: true,
            ctrl: !cfg!(target_os = "macos"),
            logo: cfg!(target_os = "macos"),
            ..Default::default()
        },
    });
    rt.frame(0.0);
    assert!(rt.state.settings);
    assert!(
        commands::registry()
            .into_iter()
            .any(|c| c.title == "Settings" && c.enabled(&rt.state))
    );
}
#[test]
fn palette_actions_leave_settings_before_opening_a_note_dialog() {
    let (_dir, mut rt) = runtime(1000.0);
    click(&mut rt, "open-settings");
    rt.update(|s, _| launcher::open(s, launcher::Scope::Actions));
    rt.frame(0.3);
    rt.ime(rune::Ime::Commit("New folder".into()));
    rt.frame(0.1);
    rt.key(rune::KeyEvent {
        key: rune::Key::Enter,
        mods: Default::default(),
    });
    rt.frame(0.3);
    assert!(!rt.state.settings);
    assert_eq!(rt.state.dialog, Some("New folder"));
    assert_eq!(rt.state.edit.text, "middle");
}

#[test]
fn narrow_settings_has_no_sidebar_or_horizontal_overflow() {
    let (_dir, mut rt) = runtime(430.0);
    click(&mut rt, "open-settings");
    assert!(rt.find("sidebar").is_none());
    for key in ["settings-close", "sync-settings", "sync-toggle"] {
        let e = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some(key))
            .unwrap();
        assert!(
            e.rect[0] >= 0.0 && e.rect[0] + e.rect[2] <= 430.5,
            "{key}: {:?}",
            e.rect
        );
    }
    click(&mut rt, "settings-close");
    assert!(!rt.state.settings);
}
#[test]
fn synced_save_merges_dirty_buffer_and_remote_text_then_pause_persists() {
    let (dir, mut rt) = runtime(1000.0);
    rt.state.sync.set_enabled(true).unwrap();
    sync::capture(&mut rt.state);
    let other_dir = tempfile::tempdir().unwrap();
    let mut other = sync::store::Store::open(other_dir.path()).unwrap();
    other
        .merge(&rt.state.sync.store.as_mut().unwrap().snapshot())
        .unwrap();
    other.materialize().unwrap();
    fs::write(other_dir.path().join("note.md"), "before middle").unwrap();
    other.scan().unwrap();
    rt.state.edit.text = "middle after".into();
    rt.state.sync.pending.push(other.snapshot());
    rt.state.save().unwrap();
    assert_eq!(rt.state.edit.text, "before middle after");
    assert_eq!(rt.state.saved, "before middle after");
    assert_eq!(
        fs::read_to_string(dir.path().join("note.md")).unwrap(),
        "before middle after"
    );
    rt.state.sync.set_enabled(false).unwrap();
    assert!(!rt.state.sync.enabled());
    assert!(rt.state.sync.worker.is_none());
    drop(rt);
    let restored = State::restore(Some(dir.path().to_owned()), Some("note.md".into()));
    assert!(!restored.sync.enabled());
    assert_eq!(restored.edit.text, "before middle after");
}
#[cfg(unix)]
#[test]
fn retrying_a_blocked_merge_does_not_duplicate_buffer_insertions() {
    let (dir, mut rt) = runtime(1000.0);
    rt.state.sync.set_enabled(true).unwrap();
    sync::capture(&mut rt.state);
    let remote_dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let mut remote = sync::store::Store::open(remote_dir.path()).unwrap();
    remote
        .merge(&rt.state.sync.store.as_mut().unwrap().snapshot())
        .unwrap();
    remote.materialize().unwrap();
    fs::create_dir(remote_dir.path().join("blocked")).unwrap();
    fs::write(remote_dir.path().join("blocked/new.md"), "remote").unwrap();
    remote.scan().unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("blocked")).unwrap();
    rt.state.edit.text = "middle after".into();
    rt.state.sync.pending.push(remote.snapshot());
    assert!(rt.state.save().is_err());
    assert_eq!(rt.state.edit.text, "middle after");
    rt.state.edit.text = "middle after again".into();
    fs::remove_file(dir.path().join("blocked")).unwrap();
    rt.state.save().unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("note.md")).unwrap(),
        "middle after again"
    );
    assert_eq!(rt.state.edit.text, "middle after again");
}

#[cfg(unix)]
#[test]
fn pause_remains_available_when_remote_materialization_is_blocked() {
    let (dir, mut rt) = runtime(1000.0);
    click(&mut rt, "open-settings");
    click(&mut rt, "sync-toggle");
    let outside = tempfile::tempdir().unwrap();
    fs::remove_file(dir.path().join("note.md")).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("note.md")).unwrap();
    click(&mut rt, "sync-toggle");
    assert!(
        !rt.state.sync.enabled(),
        "filesystem errors must not trap a user in enabled sync"
    );
    assert!(rt.state.sync.worker.is_none());
    assert_eq!(rt.state.edit.text, "middle");
}

#[test]
fn pending_remote_update_is_retained_when_metadata_write_fails() {
    let (dir, mut rt) = runtime(1000.0);
    rt.state.sync.set_enabled(true).unwrap();
    sync::capture(&mut rt.state);
    let remote_dir = tempfile::tempdir().unwrap();
    let mut remote = sync::store::Store::open(remote_dir.path()).unwrap();
    remote
        .merge(&rt.state.sync.store.as_mut().unwrap().snapshot())
        .unwrap();
    remote.materialize().unwrap();
    fs::write(remote_dir.path().join("note.md"), "remote middle").unwrap();
    remote.scan().unwrap();
    rt.state.sync.pending.push(remote.snapshot());
    let file = dir.path().join(".munin-sync/workspace.json");
    let backup = file.with_extension("backup");
    fs::rename(&file, &backup).unwrap();
    fs::create_dir(&file).unwrap();
    assert!(rt.state.save().is_err());
    fs::remove_dir(&file).unwrap();
    fs::rename(&backup, &file).unwrap();
    rt.state.save().unwrap();
    assert_eq!(rt.state.edit.text, "remote middle");
}

#[test]
fn enabled_identity_and_trust_survive_restart_and_workspace_switch_is_local() {
    let (dir, mut rt) = runtime(1000.0);
    rt.state.sync.set_enabled(true).unwrap();
    let id = rt.state.sync.device_id().unwrap();
    drop(rt);
    let mut s = State::restore(Some(dir.path().to_owned()), Some("note.md".into()));
    assert!(s.sync.enabled(), "{:?}", s.sync.error);
    assert_eq!(s.sync.device_id().unwrap(), id);
    s.sync.set_enabled(false).unwrap();
    fs::write(dir.path().join("note.md"), "edited while paused").unwrap();
    s.sync.set_enabled(true).unwrap();
    sync::capture(&mut s);
    s.save().unwrap();
    assert_eq!(s.edit.text, "edited while paused");
    let another = tempfile::tempdir().unwrap();
    let other = State::restore(Some(another.path().to_owned()), None);
    assert!(!other.sync.enabled());
    assert!(other.sync.device_id().is_none());
}

#[test]
fn choosing_the_current_workspace_does_not_lose_its_sync_lock() {
    let (dir, mut rt) = runtime(1000.0);
    rt.state.sync.set_enabled(true).unwrap();
    sync::capture(&mut rt.state);
    let identity = rt.state.sync.device_id();
    rt.update(choose_folder);
    rt.resolve_file_dialog(Some(dir.path().to_owned())).unwrap();
    rt.frame(0.3);
    assert!(rt.state.sync.enabled(), "{:?}", rt.state.sync.error);
    assert_eq!(rt.state.sync.device_id(), identity);
}

#[test]
fn settings_enable_and_pause_keep_plain_files_and_exclude_private_metadata() {
    let (dir, mut rt) = runtime(1000.0);
    click(&mut rt, "open-settings");
    click(&mut rt, "sync-toggle");
    assert!(rt.state.sync.enabled(), "{:?}", rt.state.sync.error);
    assert!(rt.find("sync-invite").is_some());
    assert!(rt.state.files.get(".munin-sync").is_none());
    click(&mut rt, "sync-toggle");
    assert!(!rt.state.sync.enabled());
    assert_eq!(
        fs::read_to_string(dir.path().join("note.md")).unwrap(),
        "middle"
    );
}
