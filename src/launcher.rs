//! A RuneKit modal launcher: cached files, shared actions and current-note search.
use crate::{
    commands::{self, Action},
    live,
    state::State,
};
use rune::{El, Key, KeyEvent, TextEdit, Tx, col, el, icon, row, text};
use rune_kit::{Button, ButtonVariant, Dialog, Kit, TextInput, glyphs};
use std::{ops::Range, path::PathBuf};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Scope {
    #[default]
    All,
    Files,
    Actions,
    Find,
}
#[derive(Default)]
pub struct Launcher {
    pub open: bool,
    pub scope: Scope,
    pub query: TextEdit,
    pub active: usize,
    pub files: Vec<PathBuf>,
}
#[derive(Clone)]
pub enum Target {
    File(PathBuf),
    Action(Action),
    Match(Range<usize>),
}
#[derive(Clone)]
pub struct ResultRow {
    pub title: String,
    pub detail: String,
    pub hint: String,
    pub target: Target,
    pub enabled: bool,
    score: i32,
}

pub fn open(s: &mut State, scope: Scope) {
    live::pause(s);
    s.context = None;
    s.dialog = None;
    s.dialog_target = None;
    s.input.focused = false;
    let files = s.find_notes("");
    let mut query = TextEdit::new("");
    query.focused = true;
    s.launcher = Launcher {
        open: true,
        scope,
        query,
        active: 0,
        files,
    };
}
pub fn close(s: &mut State) {
    s.launcher.open = false;
    s.launcher.query.focused = false;
    s.launcher.query.cancel_composition();
    live::focus(s);
}
fn fuzzy(query: &str, value: &str) -> Option<i32> {
    let needle: Vec<char> = query
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if needle.is_empty() {
        return Some(0);
    }
    let mut index = 0;
    let mut score = 0;
    let mut consecutive = false;
    let mut boundary = true;
    for c in value.to_lowercase().chars() {
        if index < needle.len() && c == needle[index] {
            score += 10 + if consecutive { 12 } else { 0 } + if boundary { 8 } else { 0 };
            index += 1;
            consecutive = true;
        } else {
            score -= 1;
            consecutive = false;
        }
        boundary = !c.is_alphanumeric();
    }
    (index == needle.len()).then_some(score)
}
fn matches(source: &str, query: &str) -> Vec<Range<usize>> {
    if query.is_empty() {
        return Vec::new();
    }
    // Lowercase expansion can change UTF-8 lengths (e.g. İ); retain original byte boundaries.
    let mut folded = String::new();
    let mut positions = Vec::new();
    for (start, c) in source.char_indices() {
        for lower in c.to_lowercase() {
            let before = folded.len();
            folded.push(lower);
            positions.extend(std::iter::repeat_n(
                (start, start + c.len_utf8()),
                folded.len() - before,
            ));
        }
    }
    let query = query.to_lowercase();
    folded
        .match_indices(&query)
        .take(200)
        .map(|(i, _)| positions[i].0..positions[i + query.len() - 1].1)
        .collect()
}
pub fn results(s: &State) -> Vec<ResultRow> {
    let raw = s.launcher.query.text.trim();
    let (scope, query) = if s.launcher.scope != Scope::Find {
        if let Some(q) = raw.strip_prefix('>') {
            (Scope::Actions, q.trim())
        } else if let Some(q) = raw.strip_prefix('/') {
            (Scope::Files, q.trim())
        } else {
            (s.launcher.scope, raw)
        }
    } else {
        (Scope::Find, raw)
    };
    if scope == Scope::Find {
        return matches(&s.edit.text, query)
            .into_iter()
            .map(|range| {
                let line = 1 + s.edit.text[..range.start]
                    .bytes()
                    .filter(|c| *c == b'\n')
                    .count();
                let start = s.edit.text[..range.start].rfind('\n').map_or(0, |n| n + 1);
                let end = s.edit.text[range.end..]
                    .find('\n')
                    .map_or(s.edit.text.len(), |n| range.end + n);
                ResultRow {
                    title: s.edit.text[start..end].into(),
                    detail: format!("Line {line}"),
                    hint: "↵".into(),
                    target: Target::Match(range),
                    enabled: true,
                    score: 0,
                }
            })
            .collect();
    }
    let mut out = Vec::new();
    if scope != Scope::Actions {
        let mut files = s.launcher.files.clone();
        files.sort_by_key(|p| {
            (
                s.recents.iter().position(|r| r == p).unwrap_or(usize::MAX),
                p.clone(),
            )
        });
        for path in files {
            let name = path.to_string_lossy().into_owned();
            if let Some(score) = fuzzy(query, &name) {
                out.push(ResultRow {
                    title: name,
                    detail: "Markdown note".into(),
                    hint: "↵".into(),
                    target: Target::File(path),
                    enabled: true,
                    score,
                });
            }
        }
    }
    let mut actions = Vec::new();
    if scope != Scope::Files {
        for c in commands::registry() {
            if let Some(score) = fuzzy(query, c.title)
                .into_iter()
                .chain(fuzzy(query, c.keywords))
                .max()
            {
                actions.push(ResultRow {
                    title: c.title.into(),
                    detail: "Action".into(),
                    hint: c.shortcut.map(commands::hint).unwrap_or_default(),
                    target: Target::Action(c.action),
                    enabled: c.enabled(s),
                    score,
                });
            }
        }
    }
    if query.is_empty() && scope == Scope::All && out.len() > 8 {
        let rest = out.split_off(8);
        out.extend(actions);
        out.extend(rest);
    } else {
        out.extend(actions);
    }
    if !query.is_empty() {
        out.sort_by_key(|r| std::cmp::Reverse(r.score));
    }
    out.truncate(200);
    out
}
pub fn choose(s: &mut State, index: usize, tx: &mut Tx) {
    let Some(row) = results(s).get(index).filter(|r| r.enabled).cloned() else {
        return;
    };
    match row.target {
        Target::File(path) => {
            if s.current.as_ref() == Some(&path) {
                close(s);
                return;
            }
            if let Err(e) = s.open_file(&path) {
                s.error = Some(e.to_string());
                return;
            }
            s.launcher.open = false;
            s.launcher.query.focused = false;
            crate::report(s, Ok(()));
            live::at(s, 0);
            tx.scroll_to_y("editor-scroll", 0.0);
            tx.focus("input", false);
        }
        Target::Action(action) => {
            close(s);
            commands::dispatch(s, action, tx);
        }
        Target::Match(range) => {
            close(s);
            s.edit.set_selection(range.start, range.end);
            live::restore_selection(s);
            tx.scroll_into_view("input");
            tx.focus("input", false);
        }
    }
}
fn field_key(s: &mut State, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if s.launcher.query.is_composing() {
        return false;
    }
    if ev.key == Key::Escape {
        close(s);
        return true;
    }
    if ev.mods.cmd || ev.mods.alt || ev.mods.logo {
        return false;
    }
    let direction = match ev.key {
        Key::Down => 1isize,
        Key::Up => -1,
        Key::PageDown => 6,
        Key::PageUp => -6,
        _ => return false,
    };
    let rows = results(s);
    if rows.is_empty() {
        return true;
    }
    let mut index =
        (s.launcher.active as isize + direction).rem_euclid(rows.len() as isize) as usize;
    for _ in 0..rows.len() {
        if rows[index].enabled {
            break;
        }
        index = (index as isize + direction.signum()).rem_euclid(rows.len() as isize) as usize;
    }
    s.launcher.active = index;
    tx.scroll_into_view(format!("palette-option-{index}"));
    true
}
pub fn mount(s: &State, k: &Kit, base: El) -> El {
    let mut overlays = Vec::new();
    if !s.launcher.open {
        return base;
    }
    let rows = results(s);
    let active = s.launcher.active.min(rows.len().saturating_sub(1));
    let mut body = col()
        .w_full()
        .h((s.viewport.1 - 112.0).clamp(80.0, 400.0))
        .min_h(0.0)
        .child(
            TextInput::new(&s.launcher.query)
                .width(None)
                .plain()
                .height(44.0)
                .leading_icon(&glyphs::SEARCH)
                .label("Search files and actions")
                .placeholder(if s.launcher.scope == Scope::Find {
                    "Find in this note…"
                } else {
                    "Search notes or actions…  > commands  / files"
                })
                .active_descendant((!rows.is_empty()).then(|| format!("palette-option-{active}")))
                .on_edit(|s: &mut State| &mut s.launcher.query)
                .on_change(|s: &mut State, tx| {
                    s.launcher.active = 0;
                    tx.scroll_to_y("palette-results", 0.0);
                })
                .on_key(field_key)
                .on_submit(|s: &mut State, tx| choose(s, s.launcher.active, tx))
                .build(k)
                .key("palette-search"),
        );
    if s.launcher.scope != Scope::Find {
        let mut tabs = row().px(12.0).gap(4.0).h(36.0).center_y();
        for (scope, label) in [
            (Scope::All, "All"),
            (Scope::Files, "Files"),
            (Scope::Actions, "Actions"),
        ] {
            tabs = tabs.child(
                Button::new(label)
                    .variant(if s.launcher.scope == scope {
                        ButtonVariant::Neutral
                    } else {
                        ButtonVariant::GhostMuted
                    })
                    .on_press(move |s: &mut State| {
                        s.launcher.scope = scope;
                        s.launcher.active = 0;
                        s.launcher.query.focused = true;
                    })
                    .build(k)
                    .key(format!("palette-scope-{label}")),
            );
        }
        body = body.child(tabs);
    }
    let mut list = col()
        .key("palette-results")
        .role("listbox")
        .label("Files and actions")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(rune::ScrollSpec {
            stick_bottom: false,
        })
        .padding(6.0)
        .gap(2.0);
    if rows.is_empty() {
        list = list.child(
            text(
                if s.launcher.scope == Scope::Find && s.launcher.query.text.is_empty() {
                    "Type to find text in this note"
                } else {
                    "No matching files or actions"
                },
            )
            .w_full()
            .wrap()
            .padding(16.0)
            .font(k.ty.ui)
            .color(k.c.text_muted),
        );
    }
    for (index, result) in rows.into_iter().enumerate() {
        let glyph = if matches!(result.target, Target::File(_)) {
            &glyphs::FILE_TEXT
        } else {
            &glyphs::ARROW_RIGHT
        };
        let mut item = row()
            .key(format!("palette-option-{index}"))
            .role("option")
            .label(result.title.clone())
            .focus_selected(index == active)
            .w_full()
            .min_w(0.0)
            .h(46.0)
            .px(12.0)
            .gap(12.0)
            .center_y()
            .radius(6.0)
            .fill(if index == active {
                k.alpha(k.c.text_accent, 0.15)
            } else {
                rune::Color::TRANSPARENT
            })
            .hover(k.c.hover)
            .child(icon(glyph, 16.0).color(if index == active {
                k.c.text_accent
            } else {
                k.c.icon_muted
            }))
            .child(
                col()
                    .grow(1.0)
                    .min_w(0.0)
                    .gap(2.0)
                    .child(
                        text(result.title)
                            .font(k.ty.ui_medium)
                            .color(k.c.text_base)
                            .w_full()
                            .truncate(),
                    )
                    .child(
                        text(result.detail)
                            .font(k.ty.small)
                            .color(k.c.text_muted)
                            .w_full()
                            .truncate(),
                    ),
            )
            .child(text(result.hint).font(k.ty.small).color(k.c.text_muted));
        if result.enabled {
            item = item
                .on_press_tx(move |s: &mut State, tx| choose(s, index, tx))
                .role("option");
        } else {
            item = item.disabled(true).layer_opacity(0.45);
        }
        list = list.child(item);
    }
    body = body.child(list);
    if let Some(error) = &s.error {
        body = body.child(
            text(error.clone())
                .font(k.ty.small)
                .color(k.c.danger.fg)
                .w_full()
                .wrap_anywhere()
                .max_h(48.0)
                .px(16.0),
        );
    }
    body = body.child(
        row()
            .h(30.0)
            .px(16.0)
            .center_y()
            .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_muted)
            .child(
                text("↑ ↓ navigate    ↵ open / run")
                    .font(k.ty.small)
                    .color(k.c.text_muted),
            )
            .child(el().grow(1.0))
            .child(text("esc to close").font(k.ty.small).color(k.c.text_muted)),
    );
    Dialog::new(
        "command-palette",
        if s.launcher.scope == Scope::Find {
            "Find in note"
        } else {
            "Jump to anything"
        },
    )
    .open(true)
    .width(640.0)
    .fit()
    .on_close(close)
    .child(body)
    .build(k, &mut overlays);
    rune_kit::root(base, overlays)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fuzzy_subsequence_and_unicode_search_ranges() {
        assert!(fuzzy("prjr", "Project Journal.md").is_some());
        assert!(fuzzy("xyz", "Project Journal.md").is_none());
        assert_eq!(matches("Žąsis ŽĄSIS", "žąsis"), vec![0..7, 8..15]);
        assert_eq!(matches("İdea", "i"), vec![0..2]);
    }
}
