mod fs;
mod state;

use rune::{App, AppSpec, Chord, Cx, El, KeyEvent, TextEdit, Tx, col, row, text};
use rune_kit::{Button, FileNode, Kit, TextArea, TextInput, Tree};
use state::State;
use std::time::{Duration, Instant};

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
    s.input = TextEdit::new("");
    s.input.focused = true;
}
fn submit(s: &mut State) {
    let name = s.input.text.trim().to_owned();
    let r = match s.dialog {
        Some("New note") => s.create_note(&name),
        Some("New folder") => s.create_folder(&name),
        Some("Rename note") => s.rename_current(&name),
        Some("Delete note") => s.delete_current(),
        Some("Quick open") => match s.find_notes(&name).first() {
            Some(path) => s.open_file(path),
            None => Err(std::io::Error::other("No matching note")),
        },
        _ => Ok(()),
    };
    if r.is_ok() {
        s.dialog = None;
    }
    report(s, r);
}
fn label(n: &FileNode<()>) -> &str {
    &n.name
}
fn view(s: &State, cx: &mut Cx<State>) -> El {
    let dark = match s.theme {
        1 => false,
        2 => true,
        _ => cx.system_dark(),
    };
    let k = Kit::neutral(dark);
    let mut sidebar = col()
        .key("sidebar")
        .w(236.0)
        .h_full()
        .padding(12.0)
        .gap(8.0)
        .fill(k.c.layer_01)
        .child(text("MUNIN").font(k.ty.ui_medium).color(k.c.text_muted))
        .child(
            Button::new("Open folder")
                .on_press_tx(|s: &mut State, tx| choose_folder(s, tx))
                .build(&k)
                .key("open-workspace"),
        );
    if s.workspace.is_some() {
        sidebar = sidebar
            .child(
                row()
                    .gap(4.0)
                    .child(
                        Button::new("New note")
                            .on_press(|s: &mut State| start_dialog(s, "New note"))
                            .build(&k)
                            .key("new-note"),
                    )
                    .child(
                        Button::new("Folder")
                            .on_press(|s: &mut State| start_dialog(s, "New folder"))
                            .build(&k),
                    ),
            )
            .child(
                row()
                    .gap(4.0)
                    .child(
                        Button::new("Rename")
                            .on_press(|s: &mut State| start_dialog(s, "Rename note"))
                            .build(&k),
                    )
                    .child(
                        Button::new("Delete")
                            .on_press(|s: &mut State| start_dialog(s, "Delete note"))
                            .build(&k),
                    ),
            )
            .child(
                Tree::new(|s: &State| &s.files, |s: &mut State| &mut s.files, label)
                    .name("Workspace files")
                    .on_select(|s: &mut State, id, _| {
                        if s.files.get(id).is_some_and(|n| !n.dir) {
                            let r = s.open_file(id);
                            if r.is_err()
                                && let Some(path) = &s.current
                            {
                                s.files.select(&state::path_id(path));
                            }
                            report(s, r);
                        }
                    })
                    .build(s, &k)
                    .grow(1.0)
                    .min_h(0.0),
            );
    }
    sidebar = sidebar.child(
        Button::new(match s.theme {
            1 => "Theme: light",
            2 => "Theme: dark",
            _ => "Theme: system",
        })
        .on_press(|s: &mut State| {
            s.theme = (s.theme + 1) % 3;
            remember(s);
        })
        .build(&k),
    );
    let mut pane = col()
        .key("writing-surface")
        .grow(1.0)
        .h_full()
        .padding(28.0)
        .gap(10.0);
    if let Some(error) = &s.error {
        pane = pane.child(text(error.clone()).color(k.c.danger.fg));
    }
    if let Some(mode) = s.dialog {
        let mut dialog = col()
            .gap(8.0)
            .child(text(mode).font(k.ty.ui_medium).color(k.c.text_base));
        if mode == "Delete note" {
            dialog = dialog.child(
                text("Delete the current note? This cannot be undone.").color(k.c.text_muted),
            );
        } else {
            dialog = dialog.child(
                TextInput::new(&s.input)
                    .placeholder("Name")
                    .on_edit(|s: &mut State| &mut s.input)
                    .on_submit(|s: &mut State, _| submit(s))
                    .build(&k),
            );
        }
        if mode == "Quick open" {
            for path in s.find_notes(s.input.text.trim()).into_iter().take(12) {
                let caption = path.to_string_lossy().into_owned();
                dialog = dialog.child(
                    Button::new(caption)
                        .on_press(move |s: &mut State| {
                            let r = s.open_file(&path);
                            if r.is_ok() {
                                s.dialog = None;
                            }
                            report(s, r);
                        })
                        .build(&k),
                );
            }
        }
        dialog = dialog.child(
            row()
                .gap(8.0)
                .child(
                    Button::new("Confirm")
                        .on_press(|s: &mut State| submit(s))
                        .build(&k),
                )
                .child(
                    Button::new("Cancel")
                        .on_press(|s: &mut State| s.dialog = None)
                        .build(&k),
                ),
        );
        pane = pane.child(dialog);
    }
    if let Some(file) = &s.current {
        pane = pane
            .child(
                text(file.to_string_lossy().into_owned())
                    .font(k.ty.ui)
                    .color(k.c.text_muted),
            )
            .child(
                TextArea::new(&s.edit)
                    .width(None)
                    .auto_grow(12, 40)
                    .max_height(600.0)
                    .decorations(rune_kit::syntax::SyntaxTheme::from_kit(&k).spans(
                        &rune_kit::syntax::highlight(Some("markdown"), &s.edit.text),
                        0,
                    ))
                    .on_edit(|s: &mut State| &mut s.edit)
                    .on_key(|s: &mut State, ev, tx| {
                        if ev.key == rune::Key::Enter && ev.mods == rune::Mods::default() {
                            s.edit.key(
                                &rune::KeyEvent {
                                    key: rune::Key::Enter,
                                    mods: rune::Mods {
                                        shift: true,
                                        ..Default::default()
                                    },
                                },
                                None,
                            );
                            s.changed_at = Some(Instant::now());
                            tx.wake_after(Duration::from_millis(700));
                            true
                        } else {
                            false
                        }
                    })
                    .on_change(|s: &mut State, tx| {
                        s.changed_at = Some(Instant::now());
                        tx.wake_after(Duration::from_millis(700));
                    })
                    .build(&k)
                    .w_full()
                    .max_w(760.0)
                    .self_center()
                    .grow(1.0),
            );
    } else {
        pane = pane.child(text("Open a note to start writing").color(k.c.text_muted));
    }
    let mut body = row().w_full().h_full().fill(k.c.page);
    if s.sidebar {
        body = body.child(sidebar);
    }
    body.on_key(key).child(pane)
}
fn key(s: &mut State, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let hit = |name: &str| Chord::parse(name).is_ok_and(|c| c.matches_tx(ev, tx));
    if hit("mod+o") {
        choose_folder(s, tx);
    } else if hit("mod+n") {
        start_dialog(s, "New note");
    } else if hit("mod+s") {
        let r = s.save();
        report(s, r);
    } else if hit("mod+b") {
        s.sidebar = !s.sidebar;
    } else if hit("mod+p") {
        start_dialog(s, "Quick open");
    } else if hit("mod+w") {
        match s.save() {
            Ok(()) => {
                cleanup_recovery(s);
                s.current = None;
                s.files.clear_selection();
                s.edit = TextEdit::new("");
                s.saved.clear();
                remember(s);
            }
            Err(e) => s.error = Some(e.to_string()),
        }
    } else {
        return false;
    }
    true
}
fn poll(s: &mut State, _: f64, tx: &mut Tx) -> bool {
    let Some(when) = s.changed_at else {
        return false;
    };
    let delay = Duration::from_millis(700);
    if when.elapsed() < delay {
        tx.wake_after(delay - when.elapsed());
        return false;
    }
    s.changed_at = None;
    let r = s.save();
    report(s, r);
    true
}
fn main() {
    let spec = AppSpec {
        on_key: Some(key),
        poll: Some(poll),
        ..AppSpec::new("Munin", (1000.0, 700.0), init, view)
    };
    App::new(spec)
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
            key: rune::Key::Command('p'),
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
            key: rune::Key::Command('b'),
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
