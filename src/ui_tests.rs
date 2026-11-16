use super::*;
use rune::{ElementInfo, Env, Runtime};

fn note_runtime(
    content: &str,
    size: (f32, f32),
    dark: bool,
) -> (tempfile::TempDir, Runtime<State>) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("note.md"), content).unwrap();
    let mut s = State::restore(Some(dir.path().to_path_buf()), None);
    s.open_file("note.md").unwrap();
    let mut rt = Runtime::new(
        s,
        view,
        size,
        Env {
            system_dark: dark,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    (dir, rt)
}

fn element(rt: &Runtime<State>, key: &str) -> ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("missing {key}"))
}

fn click(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap();
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(1.0);
}

#[test]
fn short_note_uses_full_pane_width_and_height() {
    for dark in [false, true] {
        let (_dir, mut rt) = note_runtime("A short note", (1440.0, 1000.0), dark);
        let pane = element(&rt, "writing-surface").rect;
        let input = element(&rt, "input").rect;
        assert!(
            input[2] >= pane[2] - 80.0,
            "editor must use pane width: {input:?} vs {pane:?}"
        );
        let x = pane[0] + 50.0;
        let y = 940.0;
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        rt.ime(rune::Ime::Commit(" appended".into()));
        rt.frame(0.0);
        assert!(
            rt.state.edit.text.contains(" appended"),
            "blank lower editor surface must accept focus"
        );
    }
}

#[test]
fn editor_viewport_reaches_footer_and_wraps_long_tokens() {
    for size in [(1440.0, 1000.0), (720.0, 520.0)] {
        let (_dir, rt) = note_runtime(&"x".repeat(600), size, false);
        let pane = element(&rt, "writing-surface").rect;
        let viewport = element(&rt, "editor-scroll").rect;
        let footer = element(&rt, "status-bar").rect;
        let input = element(&rt, "input").rect;
        assert!((viewport[1] + viewport[3] - footer[1]).abs() < 1.0);
        assert!(input[0] >= pane[0] + 20.0);
        assert!(input[0] + input[2] <= pane[0] + pane[2] - 20.0);
        assert!(input[3] > 24.0, "unbroken text must wrap, not clip");
    }
}

#[test]
fn long_note_scroll_keeps_chrome_fixed_and_last_line_reachable() {
    let content = (0..150).map(|i| format!("Line {i}\n")).collect::<String>();
    let (_dir, mut rt) = note_runtime(&content, (1000.0, 700.0), true);
    let header = element(&rt, "note-header").rect;
    let footer = element(&rt, "status-bar").rect;
    rt.update(|_, tx| tx.scroll_to_bottom_instant("editor-scroll"));
    rt.frame(0.1);
    let viewport = element(&rt, "editor-scroll").rect;
    let input = element(&rt, "input").rect;
    assert_eq!(element(&rt, "note-header").rect, header);
    assert_eq!(element(&rt, "status-bar").rect, footer);
    assert!(input[1] < viewport[1]);
    assert!(input[1] + input[3] <= viewport[1] + viewport[3] + 1.0);
    assert!(input[1] + input[3] >= viewport[1] + viewport[3] - 40.0);
}

#[test]
fn sidebar_can_be_toggled_visually_and_editor_expands() {
    let (_dir, mut rt) = note_runtime("note", (1000.0, 700.0), true);
    let before = element(&rt, "input").rect[2];
    click(&mut rt, "toggle-sidebar");
    assert!(!rt.state.sidebar);
    assert!(
        element(&rt, "input").rect[2] > before + 200.0,
        "before={before}, after={:?}, pane={:?}",
        element(&rt, "input").rect,
        element(&rt, "writing-surface").rect
    );
    click(&mut rt, "toggle-sidebar");
    assert!(rt.state.sidebar);
}

#[test]
fn escape_closes_dialog_and_returns_typing_to_note() {
    let (_dir, mut rt) = note_runtime("original", (1000.0, 700.0), true);
    click(&mut rt, "new-note");
    assert!(rt.state.dialog.is_some());
    rt.key(rune::KeyEvent {
        key: rune::Key::Escape,
        mods: Default::default(),
    });
    rt.frame(0.0);
    assert!(rt.state.dialog.is_none());
    rt.ime(rune::Ime::Commit(" here".into()));
    rt.frame(0.0);
    assert!(rt.state.edit.text.contains(" here"));
}

#[test]
fn sidebar_tree_keeps_an_equal_inset_on_both_sides() {
    let (_dir, rt) = note_runtime("note", (1000.0, 700.0), false);
    let side = element(&rt, "sidebar").rect;
    let tree = element(&rt, "files").rect;
    assert!(tree[0] >= side[0] + 9.0);
    assert!(
        tree[0] + tree[2] <= side[0] + side[2] - 9.0,
        "tree overflows sidebar: {tree:?}"
    );
}

#[test]
fn empty_state_copy_wraps_inside_its_surfaces() {
    let mut rt = Runtime::new(
        State::restore(None, None),
        view,
        (720.0, 520.0),
        Env {
            system_dark: false,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    for (prefix, surface) in [
        ("Open a folder to bring", "sidebar"),
        ("Make room", "empty-state"),
        ("A quiet home", "empty-state"),
    ] {
        let bounds = element(&rt, surface).rect;
        let copy = rt
            .elements()
            .into_iter()
            .find(|e| e.text.as_deref().is_some_and(|t| t.starts_with(prefix)))
            .unwrap();
        assert!(
            copy.rect[0] >= bounds[0] && copy.rect[0] + copy.rect[2] <= bounds[0] + bounds[2],
            "copy overflows {surface}: {:?} vs {bounds:?}",
            copy.rect
        );
        assert!(copy.rect[1] + copy.rect[3] <= bounds[1] + bounds[3]);
        if surface == "sidebar" {
            assert!(
                copy.rect[3] > 20.0,
                "sidebar copy must wrap instead of painting beyond its box"
            );
        }
    }
}

#[test]
fn long_path_does_not_push_note_actions_outside_a_narrow_pane() {
    let (dir, mut rt) = note_runtime("note", (720.0, 520.0), true);
    let long_name = format!("{}.md", "long note name ".repeat(12));
    std::fs::write(dir.path().join(&long_name), "note").unwrap();
    rt.update(|s, _| {
        s.open_file(&long_name).unwrap();
    });
    rt.frame(1.0);
    for key in ["note-path", "rename-note", "delete-note", "close-note"] {
        let r = element(&rt, key).rect;
        assert!(
            r[0] >= 248.0 && r[0] + r[2] <= 720.0,
            "{key} outside pane: {r:?}"
        );
    }
    rt.set_viewport((1440.0, 1000.0), 1.0);
    rt.frame(2.0);
    assert!(element(&rt, "input").rect[2] > 1100.0);
}

#[test]
fn quick_open_long_results_fit_the_dialog() {
    let (dir, mut rt) = note_runtime("note", (720.0, 520.0), false);
    let name = format!("{}.md", "long name ".repeat(20));
    std::fs::write(dir.path().join(&name), "note").unwrap();
    click(&mut rt, "quick-open");
    let bounds = element(&rt, "note-dialog").rect;
    let label = rt
        .elements()
        .into_iter()
        .find(|e| e.text.as_deref() == Some(name.as_str()))
        .unwrap();
    assert!(
        label.rect[0] >= bounds[0] && label.rect[0] + label.rect[2] <= bounds[0] + bounds[2],
        "quick-open result overflows dialog: {:?} in {bounds:?}",
        label.rect
    );
}

#[test]
fn delete_dialog_escape_preserves_the_note() {
    let (dir, mut rt) = note_runtime("note", (1000.0, 700.0), false);
    click(&mut rt, "delete-note");
    rt.key(rune::KeyEvent {
        key: rune::Key::Escape,
        mods: Default::default(),
    });
    rt.frame(2.0);
    assert!(rt.state.dialog.is_none());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("note.md")).unwrap(),
        "note"
    );
}

#[test]
fn typing_at_end_of_long_note_keeps_caret_in_the_viewport() {
    let (_dir, mut rt) = note_runtime(&"line\n".repeat(150), (720.0, 520.0), false);
    click(&mut rt, "editor-scroll");
    rt.key(rune::KeyEvent {
        key: rune::Key::Command('a'),
        mods: rune::Mods {
            cmd: true,
            ctrl: !cfg!(target_os = "macos"),
            logo: cfg!(target_os = "macos"),
            ..Default::default()
        },
    });
    rt.frame(2.0);
    rt.key(rune::KeyEvent {
        key: rune::Key::Right,
        mods: Default::default(),
    });
    rt.frame(2.1);
    rt.ime(rune::Ime::Commit("last line".into()));
    rt.frame(3.0);
    let input = element(&rt, "input");
    let caret = input.caret.expect("focused editor caret");
    let viewport = element(&rt, "editor-scroll").rect;
    assert!(rt.state.edit.text.ends_with("last line"));
    assert!(
        caret[1] >= viewport[1] && caret[1] + caret[3] <= viewport[1] + viewport[3],
        "caret clipped: {caret:?} in {viewport:?}"
    );
}

#[test]
fn cancel_button_restores_note_focus() {
    let (_dir, mut rt) = note_runtime("note", (1000.0, 700.0), false);
    click(&mut rt, "new-note");
    click(&mut rt, "cancel-dialog");
    assert!(rt.state.dialog.is_none());
    rt.ime(rune::Ime::Commit(" here".into()));
    rt.frame(2.0);
    assert!(rt.state.edit.text.contains(" here"));
}

#[test]
fn status_reflects_words_and_unsaved_edits() {
    let (_dir, mut rt) = note_runtime("one two", (1000.0, 700.0), false);
    assert_eq!(element(&rt, "word-count").text.as_deref(), Some("2 words"));
    assert_eq!(element(&rt, "save-status").text.as_deref(), Some("Saved"));
    click(&mut rt, "input");
    rt.ime(rune::Ime::Commit(" three".into()));
    rt.frame(0.0);
    assert_eq!(element(&rt, "word-count").text.as_deref(), Some("3 words"));
    assert_eq!(
        element(&rt, "save-status").text.as_deref(),
        Some("Unsaved changes")
    );
}
