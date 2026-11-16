mod commands;
mod context;
#[cfg(test)]
mod entry_tests;
mod fs;
mod launcher;
#[cfg(test)]
mod launcher_tests;
mod live;
#[cfg(test)]
mod live_tests;
mod markdown;
mod settings;
mod state;
mod sync;
#[cfg(test)]
mod sync_ui_tests;
mod theme;
mod ui;
use ui::view;
#[cfg(test)]
mod ui_tests;

use rune::{App, AppSpec, KeyEvent, TextEdit, Tx};
use state::State;
use std::time::Duration;
#[cfg(test)]
use std::time::Instant;

fn prefs_path() -> Option<std::path::PathBuf> {
    directories::ProjectDirs::from("", "", "Munin").map(|p| p.config_dir().join("state.json"))
}
fn remember(s: &State) {
    if cfg!(test) {
        return;
    }
    if let Some(p) = prefs_path() {
        let _ = s.write_preferences(&p);
    }
}
fn recovery_path() -> Option<std::path::PathBuf> {
    prefs_path().map(|p| p.with_file_name("recovery.json"))
}
fn init() -> State {
    let mut s = prefs_path()
        .map(|p| State::read_preferences(&p))
        .unwrap_or_else(|| State::restore(None, None));
    if let Some(p) = recovery_path().filter(|p| p.exists())
        && let Err(e) = s.load_recovery(&p)
    {
        s.error = Some(format!(
            "Could not restore unsaved edits from {}: {e}",
            p.display()
        ));
    }
    s
}
fn cleanup_recovery(s: &mut State) {
    if let Some(p) = recovery_path() {
        let _ = s.clear_recovery(&p);
    }
}
fn report(s: &mut State, r: std::io::Result<()>) {
    if r.is_ok() {
        cleanup_recovery(s);
    }
    s.error = r.err().map(|e| e.to_string());
    remember(s);
}
fn choose_folder(s: &mut State, tx: &mut Tx) {
    if let Err(e) = s.save() {
        s.error = Some(e.to_string());
        return;
    }
    tx.open_folder(
        rune::FileDialogOptions {
            title: "Open workspace".into(),
            ..Default::default()
        },
        |s: &mut State, answer, _| match answer {
            Ok(Some(path)) => {
                if let Err(e) = s.save() {
                    s.error = Some(e.to_string());
                    return;
                }
                cleanup_recovery(s);
                if let Some(current) = s.workspace.as_ref().and_then(|p| p.canonicalize().ok())
                    && path.canonicalize().is_ok_and(|chosen| chosen == current)
                {
                    let r = s.refresh();
                    report(s, r);
                    return;
                }
                let mut next = State::restore(Some(path), None);
                next.theme = s.theme;
                next.sidebar = s.sidebar;
                *s = next;
                remember(s);
            }
            Ok(None) => {}
            Err(e) => s.error = Some(e.to_string()),
        },
    );
}
fn start_dialog(s: &mut State, mode: &'static str) {
    s.dialog = Some(mode);
    s.dialog_target = None;
    s.context = None;
    s.launcher.open = false;
    s.launcher.query.focused = false;
    live::pause(s);
    s.input = TextEdit::new("");
    s.input.focused = true;
}
fn dismiss_dialog(s: &mut State) {
    s.dialog = None;
    s.dialog_target = None;
    s.input.focused = false;
    live::focus(s);
}
fn submit(s: &mut State) {
    let name = s.input.text.trim().to_owned();
    let contextual_name = s
        .dialog_target
        .as_deref()
        .unwrap_or_else(|| std::path::Path::new(""))
        .join(&name);
    let target = s.dialog_target.clone();
    let r = match s.dialog {
        Some("New note") => s.create_note(&contextual_name.to_string_lossy()),
        Some("New folder") => s.create_folder(&contextual_name.to_string_lossy()),
        Some("Rename entry") => target
            .as_deref()
            .ok_or_else(|| std::io::Error::other("No entry selected"))
            .and_then(|p| s.rename_entry(p, &name)),
        Some("Delete entry") => target
            .as_deref()
            .ok_or_else(|| std::io::Error::other("No entry selected"))
            .and_then(|p| s.delete_entry(p)),
        Some("Rename note") => s.rename_current(&name),
        Some("Delete note") => s.delete_current(),
        Some("Quick open") => match s.find_notes(&name).first() {
            Some(path) => s.open_file(path),
            None => Err(std::io::Error::other("No matching note")),
        },
        _ => Ok(()),
    };
    if r.is_ok() {
        dismiss_dialog(s);
    }
    report(s, r);
}
fn close_note(s: &mut State) {
    match s.save() {
        Ok(()) => {
            cleanup_recovery(s);
            s.current = None;
            s.files.clear_selection();
            s.edit = TextEdit::new("");
            s.live = live::LiveEditor::default();
            s.saved.clear();
            remember(s);
        }
        Err(e) => s.error = Some(e.to_string()),
    }
}
fn key(s: &mut State, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if ev.key == rune::Key::Escape && s.settings && !s.launcher.open {
        settings::close(s);
        true
    } else if ev.key == rune::Key::Escape && s.dialog.is_some() {
        dismiss_dialog(s);
        true
    } else {
        commands::shortcut(s, ev, tx)
    }
}
fn poll(s: &mut State, _: f64, tx: &mut Tx) -> bool {
    let sync_changed = sync::poll(s, tx);
    let Some(when) = s.changed_at else {
        return sync_changed;
    };
    let delay = Duration::from_millis(700);
    if when.elapsed() < delay {
        tx.wake_after(delay - when.elapsed());
        return sync_changed;
    }
    s.changed_at = None;
    let r = s.save();
    report(s, r);
    true
}
fn main() {
    if let Err(error) = rune::set_app_icon(include_bytes!("../assets/munin-icon.png").to_vec()) {
        eprintln!("Could not set Munin app icon: {error}");
    }
    let spec = AppSpec {
        on_key: Some(key),
        poll: Some(poll),
        ..AppSpec::new("Munin", (1000.0, 700.0), init, view)
    };
    App::new(spec)
        .compositor(settings::effects::PairingFx::default())
        .on_close(|s, _| {
            if let Err(e) = s.save() {
                match recovery_path().map(|p| s.write_recovery(&p)) {
                    Some(Ok(())) => eprintln!("Munin: {e}. Unsaved edits saved for recovery."),
                    Some(Err(recovery_error)) => {
                        eprintln!("Munin: {e}. Recovery also failed: {recovery_error}")
                    }
                    None => eprintln!("Munin: {e}. No recovery directory available."),
                }
            } else {
                cleanup_recovery(s);
            }
            remember(s);
        })
        .run();
}
#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};
    #[test]
    fn close_note_clears_tree_selection() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("note.md"), "note").unwrap();
        let mut state = State::restore(Some(dir.path().to_path_buf()), None);
        state.open_file("note.md").unwrap();
        let mut rt = Runtime::new(
            state,
            view,
            (1000.0, 700.0),
            Env {
                system_dark: true,
                scale: 1.0,
            },
        );
        rt.frame(0.0);
        let (x, y) = rt.find("note.md").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        rt.key(rune::KeyEvent {
            key: rune::Key::Command('w'),
            mods: rune::Mods {
                cmd: true,
                ctrl: !cfg!(target_os = "macos"),
                logo: cfg!(target_os = "macos"),
                ..Default::default()
            },
        });
        rt.frame(0.0);
        assert!(rt.state.current.is_none());
        assert_eq!(rt.state.files.selected().count(), 0);
    }
    #[test]
    fn quick_open_keyboard_search_opens_note() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("first.md"), "first").unwrap();
        std::fs::write(dir.path().join("second.md"), "second").unwrap();
        let state = State::restore(Some(dir.path().to_path_buf()), None);
        let mut rt = Runtime::new(
            state,
            view,
            (1000.0, 700.0),
            Env {
                system_dark: true,
                scale: 1.0,
            },
        );
        rt.frame(0.0);
        let (x, y) = rt.find("first.md").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        rt.key(rune::KeyEvent {
            key: rune::Key::Command('o'),
            mods: rune::Mods {
                cmd: true,
                ctrl: !cfg!(target_os = "macos"),
                logo: cfg!(target_os = "macos"),
                ..Default::default()
            },
        });
        rt.frame(0.0);
        rt.ime(rune::Ime::Commit("second".into()));
        rt.frame(0.0);
        rt.key(rune::KeyEvent {
            key: rune::Key::Enter,
            mods: Default::default(),
        });
        rt.frame(0.0);
        assert_eq!(rt.state.edit.text, "second");
        assert!(rt.state.dialog.is_none());
    }
    #[test]
    fn autosave_writes_after_the_debounce_deadline() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("note.md"), "old").unwrap();
        let mut s = State::restore(Some(dir.path().to_path_buf()), None);
        s.open_file("note.md").unwrap();
        s.edit.text = "new".into();
        s.changed_at = Some(Instant::now() - Duration::from_secs(1));
        assert!(poll(&mut s, 0.0, &mut Tx::default()));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("note.md")).unwrap(),
            "new"
        );
        assert!(s.changed_at.is_none());
    }
    #[test]
    fn new_note_dialog_accepts_text_and_creates_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let state = State::restore(Some(dir.path().to_path_buf()), None);
        let mut rt = Runtime::new(
            state,
            view,
            (1000.0, 700.0),
            Env {
                system_dark: true,
                scale: 1.0,
            },
        );
        rt.frame(0.0);
        let (x, y) = rt.find("new-note").expect("new note button");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        rt.ime(rune::Ime::Commit("Žodis".into()));
        rt.frame(0.0);
        rt.key(rune::KeyEvent {
            key: rune::Key::Enter,
            mods: Default::default(),
        });
        rt.frame(0.0);
        assert!(dir.path().join("Žodis.md").exists());
    }
    #[test]
    fn folder_picker_opens_workspace_without_blocking() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("note.md"), "hello").unwrap();
        let mut rt = Runtime::new(
            State::restore(None, None),
            view,
            (1000.0, 700.0),
            Env {
                system_dark: true,
                scale: 1.0,
            },
        );
        rt.frame(0.0);
        let (x, y) = rt.find("open-workspace").expect("open button");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert!(rt.file_dialog().is_some());
        rt.resolve_file_dialog(Some(dir.path().to_path_buf()))
            .unwrap();
        rt.frame(0.0);
        assert_eq!(rt.state.workspace.as_deref(), Some(dir.path()));
        assert!(rt.state.files.get("note.md").is_some());
    }
    #[test]
    fn failed_switch_keeps_active_selection() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("one.md"), "one").unwrap();
        std::fs::write(dir.path().join("two.md"), "two").unwrap();
        let mut s = State::restore(Some(dir.path().to_path_buf()), None);
        s.open_file("one.md").unwrap();
        s.edit.text = "dirty".into();
        std::fs::write(dir.path().join("one.md"), "external").unwrap();
        let mut rt = Runtime::new(
            s,
            view,
            (1000.0, 700.0),
            Env {
                system_dark: true,
                scale: 1.0,
            },
        );
        rt.frame(0.0);
        let (x, y) = rt.find("two.md").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert_eq!(
            rt.state.current.as_deref(),
            Some(std::path::Path::new("one.md"))
        );
        assert!(rt.state.files.selected().any(|id| id == "one.md"));
        assert_eq!(rt.state.edit.text, "dirty");
    }
    #[test]
    fn clicking_file_opens_it_and_keyboard_can_toggle_sidebar() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("note.md"), "hello").unwrap();
        let state = State::restore(Some(dir.path().to_path_buf()), None);
        let mut rt = Runtime::new(
            state,
            view,
            (1000.0, 700.0),
            Env {
                system_dark: true,
                scale: 1.0,
            },
        );
        rt.frame(0.0);
        let (x, y) = rt.find("note.md").expect("file row");
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert_eq!(rt.state.edit.text, "hello");
        assert_eq!(
            rt.state.current.as_deref(),
            Some(std::path::Path::new("note.md"))
        );
        rt.key(rune::KeyEvent {
            key: rune::Key::Command('\\'),
            mods: rune::Mods {
                cmd: true,
                ctrl: !cfg!(target_os = "macos"),
                logo: cfg!(target_os = "macos"),
                ..Default::default()
            },
        });
        rt.frame(0.0);
        assert!(!rt.state.sidebar);
    }
    #[test]
    fn editor_select_copy_and_paste_unicode() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("note.md"), "").unwrap();
        let mut s = State::restore(Some(dir.path().to_path_buf()), None);
        s.open_file("note.md").unwrap();
        s.edit.focused = true;
        let mut rt = Runtime::new(
            s,
            view,
            (1000.0, 700.0),
            Env {
                system_dark: true,
                scale: 1.0,
            },
        );
        let clipboard = rune::clipboard::memory();
        rt.use_clipboard(&clipboard);
        rt.frame(0.0);
        rt.ime(rune::Ime::Commit("Žąsis".into()));
        rt.frame(0.0);
        let mods = rune::Mods {
            cmd: true,
            ctrl: !cfg!(target_os = "macos"),
            logo: cfg!(target_os = "macos"),
            ..Default::default()
        };
        for c in ['a', 'c'] {
            rt.key(rune::KeyEvent {
                key: rune::Key::Command(c),
                mods,
            });
            rt.frame(0.0);
        }
        assert_eq!(
            clipboard.text().as_deref().map(String::as_str),
            Some("Žąsis")
        );
        rt.key(rune::KeyEvent {
            key: rune::Key::End,
            mods: Default::default(),
        });
        rt.frame(0.0);
        rt.update(|_, tx| tx.request_paste());
        rt.frame(0.0);
        assert_eq!(rt.state.edit.text, "ŽąsisŽąsis");
    }
    #[test]
    fn editor_undo_and_redo_preserve_unicode() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("note.md"), "").unwrap();
        let mut s = State::restore(Some(dir.path().to_path_buf()), None);
        s.open_file("note.md").unwrap();
        s.edit.focused = true;
        let mut rt = Runtime::new(
            s,
            view,
            (1000.0, 700.0),
            Env {
                system_dark: true,
                scale: 1.0,
            },
        );
        rt.frame(0.0);
        rt.ime(rune::Ime::Commit("Žąsis".into()));
        rt.frame(0.0);
        let mods = rune::Mods {
            cmd: true,
            ctrl: !cfg!(target_os = "macos"),
            logo: cfg!(target_os = "macos"),
            ..Default::default()
        };
        rt.key(rune::KeyEvent {
            key: rune::Key::Command('z'),
            mods,
        });
        rt.frame(0.0);
        assert_eq!(rt.state.edit.text, "");
        rt.key(rune::KeyEvent {
            key: rune::Key::Command('z'),
            mods: rune::Mods {
                shift: true,
                ..mods
            },
        });
        rt.frame(0.0);
        assert_eq!(rt.state.edit.text, "Žąsis");
    }
    #[test]
    fn editor_accepts_unicode_input_and_marks_it_for_save() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("note.md"), "").unwrap();
        let mut state = State::restore(Some(dir.path().to_path_buf()), None);
        state.open_file("note.md").unwrap();
        state.edit.focused = true;
        let mut rt = Runtime::new(
            state,
            view,
            (1000.0, 700.0),
            Env {
                system_dark: true,
                scale: 1.0,
            },
        );
        rt.frame(0.0);
        rt.ime(rune::Ime::Commit("Žąsis 🐦".into()));
        rt.key(rune::KeyEvent {
            key: rune::Key::Enter,
            mods: Default::default(),
        });
        rt.ime(rune::Ime::Commit("next".into()));
        rt.frame(0.0);
        assert_eq!(rt.state.edit.text, "Žąsis 🐦\nnext");
        assert!(rt.state.changed_at.is_some());
        rt.state.save().unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("note.md")).unwrap(),
            "Žąsis 🐦\nnext"
        );
    }
    #[test]
    fn markdown_highlighter_finds_heading() {
        assert!(!rune_kit::syntax::highlight(Some("markdown"), "# Heading\n").is_empty());
    }
    #[test]
    fn empty_window_has_sidebar_and_writing_surface() {
        let mut rt = Runtime::new(
            State::restore(None, None),
            view,
            (1000.0, 700.0),
            Env {
                system_dark: true,
                scale: 1.0,
            },
        );
        rt.frame(0.0);
        assert!(rt.find("sidebar").is_some());
        assert!(rt.find("writing-surface").is_some());
    }
}
