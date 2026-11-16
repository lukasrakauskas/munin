use super::*;
use rune::{Env, Runtime};
fn setup() -> (tempfile::TempDir, Runtime<State>) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("note.md"), "word").unwrap();
    std::fs::write(dir.path().join("Project Journal.md"), "Journal").unwrap();
    let state = State::restore(Some(dir.path().into()), Some("note.md".into()));
    let mut rt = Runtime::new(
        state,
        view,
        (1000.0, 700.0),
        Env {
            system_dark: false,
            scale: 1.0,
        },
    );
    rt.on_key = Some(crate::key);
    rt.frame(0.0);
    rt.frame(0.1);
    (dir, rt)
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
fn key(rt: &mut Runtime<State>, key: rune::Key) {
    rt.key(rune::KeyEvent {
        key,
        mods: Default::default(),
    });
    rt.frame(0.0);
    rt.frame(0.1);
}
fn type_text(rt: &mut Runtime<State>, text: &str) {
    rt.ime(rune::Ime::Commit(text.into()));
    rt.frame(0.0);
    rt.frame(0.1);
}
#[test]
fn combined_palette_searches_commands_and_restores_editor_focus() {
    let (_dir, mut rt) = setup();
    rt.update(|s, _| crate::live::at(s, 4));
    rt.frame(0.0);
    chord(&mut rt, 'k', false);
    assert!(rt.find("command-palette-dialog").is_some());
    type_text(&mut rt, "new note");
    assert_eq!(rt.state.launcher.query.text, "new note");
    assert_eq!(
        rt.state.edit.text, "word",
        "query typing must not edit the document"
    );
    assert!(
        rt.elements()
            .iter()
            .any(|e| e.label.as_deref() == Some("New note") && e.role == "option")
    );
    key(&mut rt, rune::Key::Escape);
    type_text(&mut rt, "!");
    assert_eq!(rt.state.edit.text, "word!");
}
#[test]
fn obsidian_quick_switcher_fuzzy_matches_paths_and_opens_with_enter() {
    let (_dir, mut rt) = setup();
    chord(&mut rt, 'o', false);
    assert!(rt.find("command-palette-dialog").is_some());
    assert!(
        rt.file_dialog().is_none(),
        "Mod+O is quick switcher, not workspace picker"
    );
    type_text(&mut rt, "prjr");
    key(&mut rt, rune::Key::Enter);
    assert_eq!(
        rt.state.current.as_deref(),
        Some(std::path::Path::new("Project Journal.md"))
    );
    assert_eq!(rt.state.edit.text, "Journal");
}
#[test]
fn command_palette_runs_actions_and_arrows_choose_results() {
    let (_dir, mut rt) = setup();
    chord(&mut rt, 'p', false);
    type_text(&mut rt, "new note");
    key(&mut rt, rune::Key::Enter);
    assert_eq!(rt.state.dialog, Some("New note"));
    type_text(&mut rt, "fresh");
    key(&mut rt, rune::Key::Enter);
    assert_eq!(
        rt.state.current.as_deref(),
        Some(std::path::Path::new("fresh.md"))
    );
    chord(&mut rt, 'o', false);
    key(&mut rt, rune::Key::Down);
    key(&mut rt, rune::Key::Enter);
    assert_eq!(
        rt.state.current.as_deref(),
        Some(std::path::Path::new("note.md"))
    );
}
#[test]
fn palette_open_failure_preserves_dirty_buffer_and_displays_error() {
    let (dir, mut rt) = setup();
    rt.update(|s, _| s.edit.insert(" dirty"));
    rt.frame(0.0);
    std::fs::write(dir.path().join("note.md"), "external").unwrap();
    chord(&mut rt, 'o', false);
    type_text(&mut rt, "prjr");
    key(&mut rt, rune::Key::Enter);
    assert_eq!(rt.state.edit.text, "word dirty");
    assert!(rt.state.error.is_some());
    assert!(rt.find("command-palette-dialog").is_some());
}
#[test]
fn obsidian_bold_italic_shortcuts_edit_source_not_sidebar() {
    let (_dir, mut rt) = setup();
    rt.update(|s, _| {
        crate::live::at(s, 0);
        s.live.edit.set_selection(0, 4);
    });
    rt.frame(0.0);
    chord(&mut rt, 'b', false);
    assert_eq!(rt.state.edit.text, "**word**");
    assert!(rt.state.sidebar);
    chord(&mut rt, 'i', false);
    assert_eq!(rt.state.edit.text, "***word***");
    chord(&mut rt, 'z', false);
    assert_eq!(rt.state.edit.text, "**word**");
    chord(&mut rt, 'z', false);
    assert_eq!(rt.state.edit.text, "word");
}
#[test]
fn note_search_selects_match_and_delete_line_uses_obsidian_binding() {
    let (_dir, mut rt) = setup();
    rt.update(|s, _| {
        s.edit.replace_text("first\nŽąsis here\nlast\n");
        crate::live::blur(s);
    });
    rt.frame(0.0);
    chord(&mut rt, 'f', false);
    type_text(&mut rt, "Žąsis");
    key(&mut rt, rune::Key::Enter);
    assert_eq!(rt.state.edit.selected(), "Žąsis");
    chord(&mut rt, 'k', true);
    assert_eq!(rt.state.edit.text, "first\nlast\n");
}
