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
    // Exercise the document-wide native selection surface in legacy input/layout tests.
    s.edit.focused = true;
    s.live.full = true;
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

fn rendered(rt: &mut Runtime<State>) {
    rt.update(|s, _| crate::live::blur(s));
    rt.frame(0.0);
    rt.frame(1.0);
}

fn click(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap();
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(1.0);
}

#[test]
fn reading_renders_headings_tables_and_mermaid_without_changing_source() {
    let source = "# My heading\n\n**Bold** and *italic*.\n\n| Name | Value |\n| --- | --- |\n| Note | Local |\n\n```mermaid\nflowchart LR\n A[Draft] --> B[Keep]\n```\n";
    let (_dir, mut rt) = note_runtime(source, (1000.0, 900.0), false);
    rendered(&mut rt);
    let elements = rt.elements();
    assert!(
        elements
            .iter()
            .any(|e| e.text.as_deref() == Some("My heading") && e.level == Some(1))
    );
    assert!(elements.iter().any(|e| e.role == "table"));
    assert!(elements.iter().any(|e| e.text.as_deref() == Some("Draft")));
    assert!(!elements.iter().any(|e| e.key.as_deref() == Some("input")));
    assert_eq!(rt.state.edit.text, source);
    click(&mut rt, "live-block-0");
    assert!(rt.find("input").is_some());
    assert_eq!(rt.state.edit.text, source);
}

#[test]
fn reading_uses_unsaved_buffer_and_falls_back_for_unsupported_mermaid() {
    let (_dir, mut rt) = note_runtime("saved", (1000.0, 700.0), true);
    rt.update(|s, _| s.edit.text = "# Unsaved\n\n```mermaid\nnot-a-diagram\n```\n".into());
    rt.frame(0.0);
    rendered(&mut rt);
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("Unsaved"))
    );
    assert!(rt.elements().iter().any(|e| {
        e.text
            .as_deref()
            .is_some_and(|t| t.contains("not-a-diagram"))
    }));
    assert_eq!(rt.state.saved, "saved");
}

fn right_click(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap();
    rt.secondary_down(x, y);
    rt.secondary_up(x, y);
    rt.frame(0.0);
    rt.frame(1.0);
}

fn click_label(rt: &mut Runtime<State>, label: &str) {
    let r = rt
        .elements()
        .into_iter()
        .find(|e| e.label.as_deref() == Some(label) && e.role == "menuitem")
        .expect("menu item")
        .rect;
    rt.pointer_down(r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
    rt.pointer_up(r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
    rt.frame(0.0);
    rt.frame(1.0);
}

#[test]
fn context_rename_targets_inactive_note_without_switching_the_buffer() {
    let (dir, mut rt) = note_runtime("active", (1000.0, 700.0), false);
    std::fs::write(dir.path().join("other.md"), "other").unwrap();
    rt.update(|s, _| {
        s.edit.text = "dirty active".into();
        s.refresh().unwrap();
    });
    rt.frame(0.0);
    right_click(&mut rt, "other.md");
    assert!(rt.elements().iter().any(|e| e.role == "menu"));
    assert_eq!(rt.state.edit.text, "dirty active");
    assert_eq!(
        rt.state.current.as_deref(),
        Some(std::path::Path::new("note.md"))
    );
    click_label(&mut rt, "Rename…");
    rt.update(|s, _| s.input = TextEdit::new("renamed"));
    rt.frame(0.0);
    click(&mut rt, "confirm-dialog");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("renamed.md")).unwrap(),
        "other"
    );
    assert!(!dir.path().join("other.md").exists());
    assert_eq!(rt.state.edit.text, "dirty active");
    assert_eq!(
        rt.state.current.as_deref(),
        Some(std::path::Path::new("note.md"))
    );
}

#[test]
fn folder_context_creates_children_and_refuses_recursive_delete() {
    let (dir, mut rt) = note_runtime("note", (1000.0, 700.0), false);
    std::fs::create_dir(dir.path().join("folder")).unwrap();
    rt.update(|s, _| s.refresh().unwrap());
    rt.frame(0.0);
    right_click(&mut rt, "folder");
    click_label(&mut rt, "New note…");
    rt.update(|s, _| s.input = TextEdit::new("child"));
    rt.frame(0.0);
    click(&mut rt, "confirm-dialog");
    assert!(dir.path().join("folder/child.md").exists());
    right_click(&mut rt, "folder");
    click_label(&mut rt, "Delete…");
    assert!(
        dir.path().join("folder/child.md").exists(),
        "delete requires confirmation"
    );
    click(&mut rt, "confirm-dialog");
    assert!(rt.state.error.is_some());
    assert!(dir.path().join("folder/child.md").exists());
}

#[test]
fn background_context_has_workspace_actions_and_escape_dismisses() {
    let (_dir, mut rt) = note_runtime("note", (1000.0, 700.0), false);
    let side = element(&rt, "sidebar").rect;
    rt.secondary_down(side[0] + 20.0, 500.0);
    rt.secondary_up(side[0] + 20.0, 500.0);
    rt.frame(0.0);
    rt.frame(1.0);
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.label.as_deref() == Some("Refresh workspace"))
    );
    rt.key(rune::KeyEvent {
        key: rune::Key::Escape,
        mods: Default::default(),
    });
    rt.frame(0.0);
    assert!(!rt.elements().iter().any(|e| e.role == "menu" && !e.exiting));
}

fn chrome_insets(leading: f32, trailing: f32, height: f32) -> rune::ChromeInsets {
    let mut insets = rune::ChromeInsets::default();
    insets.leading = leading;
    insets.trailing = trailing;
    insets.height = height;
    insets
}

#[test]
fn native_overlay_controls_clear_sidebar_and_hidden_sidebar_header() {
    let (_dir, mut rt) = note_runtime("note", (1000.0, 700.0), false);
    rt.set_titlebar_inset(78.0);
    rt.set_chrome_insets(chrome_insets(78.0, 0.0, 40.0));
    rt.frame(0.0);
    rt.frame(1.0);
    let brand = rt
        .elements()
        .into_iter()
        .find(|e| e.text.as_deref() == Some("Munin"))
        .unwrap();
    assert!(
        brand.rect[1] >= 40.0,
        "brand overlaps traffic lights: {:?}",
        brand.rect
    );
    click(&mut rt, "toggle-sidebar");
    assert!(element(&rt, "toggle-sidebar").rect[0] >= 78.0);
    rt.set_titlebar_inset(0.0);
    rt.set_chrome_insets(rune::ChromeInsets::default());
    rt.frame(0.0);
    rt.frame(1.0);
    assert!(
        element(&rt, "toggle-sidebar").rect[0] < 78.0,
        "fullscreen should reclaim titlebar space"
    );
}

#[test]
fn native_overlay_strip_disappears_in_fullscreen_and_clears_trailing_controls() {
    let (_dir, mut rt) = note_runtime("note", (1000.0, 700.0), true);
    rt.set_titlebar_inset(78.0);
    rt.frame(0.0);
    rt.frame(1.0);
    let y = rt
        .elements()
        .into_iter()
        .find(|e| e.text.as_deref() == Some("Munin"))
        .unwrap()
        .rect[1];
    assert!(y >= 40.0);
    rt.set_titlebar_inset(0.0);
    rt.set_chrome_insets(chrome_insets(0.0, 140.0, 40.0));
    rt.frame(0.0);
    rt.frame(1.0);
    let brand = rt
        .elements()
        .into_iter()
        .find(|e| e.text.as_deref() == Some("Munin"))
        .unwrap();
    assert!(brand.rect[1] < 40.0);
    let close = element(&rt, "close-note").rect;
    assert!(close[0] + close[2] <= 860.0);
}

#[test]
fn reading_diagram_fits_a_narrow_pane_without_clipping_node_labels() {
    let source = "```mermaid\nflowchart LR\n A[Notice an idea] --> B[Write it down]\n B --> C[Make it yours]\n```\n";
    let (_dir, mut rt) = note_runtime(source, (640.0, 700.0), false);
    rendered(&mut rt);
    rt.frame(1.0);
    for label in ["Notice an idea", "Make it yours"] {
        let node = rt
            .elements()
            .into_iter()
            .find(|e| e.text.as_deref() == Some(label))
            .unwrap();
        assert!(
            node.visible[2] >= node.rect[2] - 1.0,
            "diagram label clipped: {label}: {:?} / {:?}",
            node.rect,
            node.visible
        );
    }
}

#[test]
fn new_note_from_reading_opens_a_focused_writing_surface() {
    let (dir, mut rt) = note_runtime("keep", (1000.0, 700.0), false);
    rendered(&mut rt);
    click(&mut rt, "new-note");
    rt.update(|s, _| s.input = TextEdit::new("fresh"));
    rt.frame(0.0);
    click(&mut rt, "confirm-dialog");
    assert!(
        rt.find("input").is_some(),
        "new notes must open in Write mode"
    );
    rt.ime(rune::Ime::Commit("new text".into()));
    rt.frame(0.0);
    assert_eq!(rt.state.edit.text, "new text");
    assert!(dir.path().join("fresh.md").exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("note.md")).unwrap(),
        "keep"
    );
}

#[test]
fn context_copy_and_outside_dismiss_do_not_switch_or_edit_the_note() {
    let (_dir, mut rt) = note_runtime("note", (1000.0, 700.0), false);
    let clipboard = rune::clipboard::memory();
    rt.use_clipboard(&clipboard);
    right_click(&mut rt, "note.md");
    click_label(&mut rt, "Copy relative path");
    assert_eq!(
        clipboard.text().as_deref().map(String::as_str),
        Some("note.md")
    );
    right_click(&mut rt, "note.md");
    rt.pointer_down(900.0, 600.0);
    rt.pointer_up(900.0, 600.0);
    rt.frame(0.0);
    rt.frame(1.0);
    assert!(!rt.elements().iter().any(|e| e.role == "menu" && !e.exiting));
    assert_eq!(rt.state.edit.text, "note");
}

#[test]
fn reading_table_header_and_cells_fill_the_same_frame() {
    let (_dir, mut rt) = note_runtime(
        "| Name | Value |\n| --- | --- |\n| Note | Local |\n",
        (1200.0, 800.0),
        true,
    );
    rendered(&mut rt);
    rt.frame(1.0);
    let elements = rt.elements();
    let table = elements.iter().find(|e| e.role == "table").unwrap();
    let headers: Vec<_> = elements
        .iter()
        .filter(|e| e.role == "columnheader")
        .collect();
    let right = headers
        .iter()
        .map(|e| e.rect[0] + e.rect[2])
        .fold(0.0, f32::max);
    assert!(
        right >= table.rect[0] + table.rect[2] - 4.0,
        "table frame extends past its header cells: {right} vs {:?}",
        table.rect
    );
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
        crate::live::whole(s, false);
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
