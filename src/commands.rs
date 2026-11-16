//! One action registry for the palette and Obsidian-aligned shortcuts.
use crate::{
    launcher::{self, Scope},
    live,
    state::State,
};
use rune::{Chord, KeyEvent, Tx};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    NewNote,
    NewFolder,
    Save,
    Workspace,
    Files,
    Commands,
    Find,
    Sidebar,
    Close,
    Rename,
    Delete,
    Bold,
    Italic,
    Code,
    Link,
    Checkbox,
    Heading1,
    Heading2,
    DeleteLine,
    Undo,
    Redo,
    Edit,
    Theme,
    Refresh,
    Settings,
}
pub struct Command {
    pub action: Action,
    pub title: &'static str,
    pub shortcut: Option<&'static str>,
    pub keywords: &'static str,
}
impl Command {
    pub fn enabled(&self, s: &State) -> bool {
        match self.action {
            Action::Settings
            | Action::Workspace
            | Action::Files
            | Action::Commands
            | Action::Sidebar
            | Action::Theme => true,
            Action::NewNote | Action::NewFolder | Action::Refresh => s.workspace.is_some(),
            _ => s.current.is_some(),
        }
    }
}
pub fn registry() -> Vec<Command> {
    use Action::*;
    [
        (NewNote, "New note", Some("mod+n"), "create markdown file"),
        (
            Files,
            "Quick switcher",
            Some("mod+o"),
            "open jump file note",
        ),
        (
            Commands,
            "Command palette",
            Some("mod+p"),
            "search actions commands",
        ),
        (Save, "Save note", Some("mod+s"), "save current file"),
        (Bold, "Toggle bold", Some("mod+b"), "format strong"),
        (Italic, "Toggle italic", Some("mod+i"), "format emphasis"),
        (
            Find,
            "Find in note",
            Some("mod+f"),
            "search current document",
        ),
        (
            Edit,
            "Edit / finish current block",
            Some("mod+e"),
            "source live preview",
        ),
        (
            Sidebar,
            "Toggle left sidebar",
            Some("mod+\\"),
            "explorer files",
        ),
        (Close, "Close note", Some("mod+w"), "close current file"),
        (Undo, "Undo", Some("mod+z"), "edit history"),
        (Redo, "Redo", Some("mod+shift+z"), "edit history"),
        (
            DeleteLine,
            "Delete current line",
            Some("mod+shift+k"),
            "remove paragraph line",
        ),
        (NewFolder, "New folder", None, "create directory"),
        (
            Workspace,
            "Open workspace folder",
            None,
            "choose vault directory",
        ),
        (Rename, "Rename note", None, "change name"),
        (Delete, "Delete note…", None, "remove file confirmation"),
        (Code, "Toggle inline code", None, "format backticks"),
        (Link, "Insert link", None, "format url markdown"),
        (Checkbox, "Toggle task checkbox", None, "todo list check"),
        (Heading1, "Heading 1", None, "format title"),
        (Heading2, "Heading 2", None, "format section"),
        (Theme, "Cycle appearance", None, "theme dark light system"),
        (Refresh, "Refresh workspace", None, "reload files tree"),
        (
            Settings,
            "Settings",
            Some("mod+,"),
            "sync devices pairing appearance preferences",
        ),
    ]
    .into_iter()
    .map(|(action, title, shortcut, keywords)| Command {
        action,
        title,
        shortcut,
        keywords,
    })
    .collect()
}
pub fn hint(shortcut: &str) -> String {
    if cfg!(target_os = "macos") {
        shortcut
            .replace("mod+", "⌘")
            .replace("shift+", "⇧")
            .to_uppercase()
    } else {
        shortcut
            .replace("mod+", "Ctrl+")
            .replace("shift+", "Shift+")
            .replace("+k", "+K")
            .replace("+z", "+Z")
            .replace("+n", "+N")
            .replace("+o", "+O")
            .replace("+p", "+P")
            .replace("+s", "+S")
            .replace("+b", "+B")
            .replace("+i", "+I")
            .replace("+f", "+F")
            .replace("+e", "+E")
            .replace("+w", "+W")
    }
}
pub fn shortcut(s: &mut State, ev: &KeyEvent, tx: &mut Tx) -> bool {
    let hit = |chord: &str| Chord::parse(chord).is_ok_and(|c| c.matches_tx(ev, tx));
    if s.settings && !hit("mod+,") && !hit("mod+k") && !hit("mod+p") {
        return false;
    }
    if hit("mod+k") {
        launcher::open(s, Scope::All);
        return true;
    }
    if !s.launcher.open && !cfg!(target_os = "macos") && hit("mod+y") {
        dispatch(s, Action::Redo, tx);
        return true;
    }
    if let Some(command) = registry().into_iter().find(|c| c.shortcut.is_some_and(hit)) {
        if s.launcher.open
            && !matches!(
                command.action,
                Action::Files | Action::Commands | Action::Find
            )
        {
            return false;
        }
        if command.enabled(s) {
            dispatch(s, command.action, tx);
        }
        return true;
    }
    false
}
fn mutate(s: &mut State, tx: &mut Tx, f: impl FnOnce(&mut rune::TextEdit)) {
    if s.current.is_none() {
        return;
    }
    live::sync_selection(s);
    let full = s.live.full;
    s.edit.undo_group(f);
    if full {
        s.live = crate::live::LiveEditor::default();
        s.live.full = true;
        s.edit.focused = true;
    } else {
        live::restore_selection(s);
    }
    live::changed(s, tx);
    tx.scroll_into_view("input");
}
fn mark(doc: &mut rune::TextEdit, marker: &str) {
    let (a, b) = doc.range();
    let n = marker.len();
    let selected = doc.text[a..b].to_owned();
    let (start, end, replacement, anchor, focus) =
        if selected.len() >= n * 2 && selected.starts_with(marker) && selected.ends_with(marker) {
            (
                a,
                b,
                selected[n..selected.len() - n].to_owned(),
                a,
                b - 2 * n,
            )
        } else if a >= n
            && doc.text[..a].ends_with(marker)
            && doc.text[b..].starts_with(marker)
            && (marker != "*"
                || doc.text[..a]
                    .chars()
                    .rev()
                    .take_while(|c| *c == '*')
                    .count()
                    % 2
                    == 1)
        {
            (a - n, b + n, selected, a - n, b - n)
        } else {
            (a, b, format!("{marker}{selected}{marker}"), a + n, b + n)
        };
    doc.set_selection(start, end);
    doc.insert(&replacement);
    doc.set_selection(anchor, focus);
}
fn line(doc: &rune::TextEdit) -> (usize, usize) {
    let start = doc.text[..doc.focus].rfind('\n').map_or(0, |p| p + 1);
    let end = doc.text[start..]
        .find('\n')
        .map_or(doc.text.len(), |p| start + p + 1);
    (start, end)
}
pub fn dispatch(s: &mut State, action: Action, tx: &mut Tx) {
    use Action::*;
    if s.settings
        && !matches!(
            action,
            Settings | Theme | Workspace | Files | Commands | Sidebar | Refresh
        )
    {
        crate::settings::close(s);
    }
    match action {
        Settings => crate::settings::open(s),
        NewNote => crate::start_dialog(s, "New note"),
        NewFolder => crate::start_dialog(s, "New folder"),
        Save => {
            let r = s.save();
            crate::report(s, r);
        }
        Workspace => crate::choose_folder(s, tx),
        Files => launcher::open(s, Scope::Files),
        Commands => launcher::open(s, Scope::Actions),
        Find => launcher::open(s, Scope::Find),
        Sidebar => s.sidebar = !s.sidebar,
        Close => crate::close_note(s),
        Rename => crate::start_dialog(s, "Rename note"),
        Delete => crate::start_dialog(s, "Delete note"),
        Bold => mutate(s, tx, |d| mark(d, "**")),
        Italic => mutate(s, tx, |d| mark(d, "*")),
        Code => mutate(s, tx, |d| mark(d, "`")),
        Link => mutate(s, tx, |d| {
            let (a, b) = d.range();
            let label = if a == b {
                "link text".into()
            } else {
                d.text[a..b].to_owned()
            };
            d.insert(&format!("[{label}](url)"));
            let start = a + label.len() + 3;
            d.set_selection(start, start + 3);
        }),
        DeleteLine => mutate(s, tx, |d| {
            let (a, b) = line(d);
            d.set_selection(a, b);
            d.insert("");
        }),
        Heading1 | Heading2 | Checkbox => mutate(s, tx, |d| {
            let (a, b) = line(d);
            let original = d.text[a..b].to_owned();
            let (remove, prefix) = if action == Checkbox {
                if original.starts_with("- [ ] ") {
                    (6, "- [x] ")
                } else if original.starts_with("- [x] ") || original.starts_with("- [X] ") {
                    (6, "- [ ] ")
                } else {
                    (0, "- [ ] ")
                }
            } else {
                let count = original.bytes().take_while(|b| *b == b'#').count();
                let remove =
                    if (1..=6).contains(&count) && original.as_bytes().get(count) == Some(&b' ') {
                        count + 1
                    } else {
                        0
                    };
                (remove, if action == Heading1 { "# " } else { "## " })
            };
            let caret = d.focus;
            d.set_selection(a, a + remove);
            d.insert(prefix);
            d.set_caret((a + prefix.len() + caret.saturating_sub(a + remove)).min(d.text.len()));
        }),
        Undo => live::history(s, false, tx),
        Redo => live::history(s, true, tx),
        Edit => live::toggle(s),
        Theme => {
            s.theme = (s.theme + 1) % 3;
            crate::remember(s);
        }
        Refresh => {
            let r = s.refresh();
            crate::report(s, r);
        }
    }
}
