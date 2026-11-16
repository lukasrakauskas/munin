use super::*;
use rune::{Env, Runtime};
fn runtime(source: &str) -> (tempfile::TempDir, Runtime<State>) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("note.md"), source).unwrap();
    let state = State::restore(Some(dir.path().into()), Some("note.md".into()));
    let mut rt = Runtime::new(
        state,
        view,
        (1100.0, 900.0),
        Env {
            system_dark: false,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt.frame(0.1);
    (dir, rt)
}
fn click(rt: &mut Runtime<State>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("missing {key}"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.1);
}
fn chord(rt: &mut Runtime<State>, c: char, shift: bool) {
    rt.key(rune::KeyEvent {
        key: rune::Key::Command(c),
        mods: rune::Mods {
            cmd: true,
            ctrl: !cfg!(target_os = "macos"),
            logo: cfg!(target_os = "macos"),
            shift,
            ..Default::default()
        },
    });
    rt.frame(0.0);
    rt.frame(0.1);
}
#[test]
fn live_document_renders_and_edits_a_block_in_place() {
    let source = "# Heading\n\nA **bold** paragraph.\n\n| A | B |\n|---|---|\n| x | y |\n";
    let (_dir, mut rt) = runtime(source);
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.role == "heading" && e.text.as_deref() == Some("Heading"))
    );
    assert!(rt.elements().iter().any(|e| e.role == "table"));
    assert!(rt.find("toggle-reading").is_none());
    click(&mut rt, "live-block-11");
    rt.ime(rune::Ime::Commit(" more".into()));
    rt.frame(0.0);
    assert_eq!(
        rt.state.edit.text,
        "# Heading\n\nA **bold** paragraph. more\n\n| A | B |\n|---|---|\n| x | y |\n"
    );
    click(&mut rt, "live-block-0");
    assert!(rt.elements().iter().any(|e| {
        e.text
            .as_deref()
            .is_some_and(|t| t.contains("A bold paragraph. more"))
    }));
    assert!(rt.elements().iter().any(|e| e.role == "table"));
}
#[test]
fn active_paragraph_does_not_turn_separator_lines_into_blank_editor_height() {
    let (_dir, mut rt) = runtime("One paragraph.\n\n# Next\n");
    click(&mut rt, "live-block-0");
    let input = rt
        .elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some("input"))
        .unwrap();
    assert!(
        input.rect[3] <= 30.0,
        "paragraph separators expanded the editor: {:?}",
        input.rect
    );
    assert_eq!(rt.state.edit.text, "One paragraph.\n\n# Next\n");
}
#[test]
fn block_boundaries_whole_document_clipboard_and_ime_preserve_source() {
    let (_dir, mut rt) = runtime("First\n\nŽąsis\n");
    let clipboard = rune::clipboard::memory();
    rt.use_clipboard(&clipboard);
    click(&mut rt, "live-block-0");
    rt.update(|s, _| s.live.edit.set_caret(s.live.edit.text.len()));
    rt.frame(0.0);
    rt.key(rune::KeyEvent {
        key: rune::Key::Right,
        mods: Default::default(),
    });
    rt.frame(0.0);
    assert_eq!(rt.state.live.range.as_ref().unwrap().start, 7);
    rt.ime(rune::Ime::Preedit("漢".into(), Some((3, 3))));
    rt.frame(0.0);
    assert_eq!(rt.state.edit.text, "First\n\nŽąsis\n");
    rt.ime(rune::Ime::Commit("漢".into()));
    rt.frame(0.0);
    assert_eq!(rt.state.edit.text, "First\n\n漢Žąsis\n");
    chord(&mut rt, 'a', false);
    chord(&mut rt, 'c', false);
    assert_eq!(
        clipboard.text().as_deref().map(String::as_str),
        Some("First\n\n漢Žąsis\n")
    );
    rt.key(rune::KeyEvent {
        key: rune::Key::Escape,
        mods: Default::default(),
    });
    rt.frame(0.0);
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref().is_some_and(|t| t.contains("漢Žąsis")))
    );
}
#[test]
fn undo_restores_the_caret_in_the_edited_block() {
    let (_dir, mut rt) = runtime("First\n\nSecond\n");
    click(&mut rt, "live-block-0");
    rt.update(|s, _| s.live.edit.set_caret(1));
    rt.frame(0.0);
    rt.ime(rune::Ime::Commit("X".into()));
    rt.frame(0.0);
    chord(&mut rt, 'z', false);
    assert_eq!(rt.state.live.range.as_ref().unwrap().start, 0);
    assert_eq!(rt.state.live.edit.focus, 1);
}
#[test]
fn blank_line_continues_as_a_new_live_paragraph() {
    let (_dir, mut rt) = runtime("One\n\nTwo\n");
    click(&mut rt, "live-block-0");
    for _ in 0..2 {
        rt.key(rune::KeyEvent {
            key: rune::Key::Enter,
            mods: Default::default(),
        });
        rt.frame(0.0);
    }
    rt.ime(rune::Ime::Commit("New".into()));
    rt.frame(0.0);
    assert_eq!(rt.state.edit.text, "One\n\nNew\n\nTwo\n");
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.text.as_deref() == Some("One"))
    );
}
#[test]
fn document_navigation_does_not_stop_at_the_active_block() {
    let (_dir, mut rt) = runtime("First\n\nSecond\n");
    click(&mut rt, "live-block-0");
    let (key, mods) = if cfg!(target_os = "macos") {
        (
            rune::Key::Down,
            rune::Mods {
                cmd: true,
                logo: true,
                ..Default::default()
            },
        )
    } else {
        (
            rune::Key::End,
            rune::Mods {
                ctrl: true,
                cmd: true,
                ..Default::default()
            },
        )
    };
    rt.key(rune::KeyEvent { key, mods });
    rt.frame(0.0);
    assert!(
        rt.state.live.range.as_ref().unwrap().start >= 7,
        "document-end stayed in the first block"
    );
}
#[test]
fn forward_delete_at_a_block_edge_removes_separator_not_next_word() {
    let (_dir, mut rt) = runtime("First\n\nSecond\n");
    click(&mut rt, "live-block-0");
    rt.key(rune::KeyEvent {
        key: rune::Key::Delete,
        mods: Default::default(),
    });
    rt.frame(0.0);
    assert_eq!(rt.state.edit.text, "First\nSecond\n");
    chord(&mut rt, 'z', false);
    assert_eq!(rt.state.edit.text, "First\n\nSecond\n");
}
#[test]
fn live_edits_preserve_crlf_and_tables_and_diagrams_edit_in_place() {
    let (_dir, mut rt) = runtime("Žąsis\r\n\r\nSecond\r\n");
    click(&mut rt, "live-block-0");
    rt.ime(rune::Ime::Commit("🐦".into()));
    rt.frame(0.0);
    assert_eq!(rt.state.edit.text, "Žąsis🐦\r\n\r\nSecond\r\n");
    let source = "| A | B |\n|---|---|\n| x | y |\n\n```mermaid\nflowchart LR\nA-->B\n```\n";
    let (_dir, mut rt) = runtime(source);
    click(&mut rt, "live-block-0");
    assert!(rt.state.live.edit.text.starts_with("| A | B |"));
    let diagram = source.find("```mermaid").unwrap();
    click(&mut rt, &format!("live-block-{diagram}"));
    assert!(rt.state.live.edit.text.contains("flowchart LR"));
    assert!(rt.elements().iter().any(|e| e.role == "table"));
    assert_eq!(rt.state.edit.text, source);
}
#[test]
fn live_undo_crosses_blocks_without_losing_unicode_or_separators() {
    let (_dir, mut rt) = runtime("First\n\nŽąsis\n");
    click(&mut rt, "live-block-0");
    rt.ime(rune::Ime::Commit(" A".into()));
    rt.frame(0.0);
    click(&mut rt, "live-block-9");
    rt.ime(rune::Ime::Commit(" 🐦".into()));
    rt.frame(0.0);
    chord(&mut rt, 'z', false);
    assert_eq!(rt.state.edit.text, "First A\n\nŽąsis\n");
    chord(&mut rt, 'z', false);
    assert_eq!(rt.state.edit.text, "First\n\nŽąsis\n");
    chord(&mut rt, 'z', true);
    assert_eq!(rt.state.edit.text, "First A\n\nŽąsis\n");
}
