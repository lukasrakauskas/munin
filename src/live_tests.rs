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
