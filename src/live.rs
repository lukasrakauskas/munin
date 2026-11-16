//! Native block-level live preview, preserving one canonical Markdown TextEdit.
use crate::state::State;
use rune::{Cx, El, Key, KeyEvent, ScrollSpec, TextEdit, TextStyle, Tx, col, text_input};
use rune_kit::Kit;
use std::{
    ops::Range,
    time::{Duration, Instant},
};

#[derive(Default)]
pub struct LiveEditor {
    pub full: bool,
    pub range: Option<Range<usize>>,
    pub edit: TextEdit,
    base: String,
    trailing: String,
}

pub fn blocks(source: &str) -> Vec<Range<usize>> {
    use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};
    let mut starts = vec![0];
    let mut depth = 0usize;
    for (event, range) in Parser::new_ext(
        source,
        Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH,
    )
    .into_offset_iter()
    {
        match event {
            Event::Start(tag) => {
                // Indented code's parser range starts after its first indentation.
                // Keep that indentation in this block, not in the preceding one.
                let start = if matches!(tag, Tag::CodeBlock(CodeBlockKind::Indented)) {
                    source[..range.start].rfind('\n').map_or(0, |i| i + 1)
                } else {
                    range.start
                };
                if depth == 0 && start > 0 {
                    starts.push(start);
                }
                depth += 1;
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            Event::Rule if depth == 0 && range.start > 0 => starts.push(range.start),
            _ => {}
        }
    }
    starts.sort_unstable();
    starts.dedup();
    starts
        .iter()
        .enumerate()
        .map(|(i, &start)| start..starts.get(i + 1).copied().unwrap_or(source.len()))
        .collect()
}

pub fn activate(s: &mut State, range: Range<usize>, caret: usize) {
    let Some(source) = s.edit.text.get(range.clone()) else {
        return;
    };
    let body = source.trim_end_matches(['\r', '\n']);
    let trailing = source[body.len()..].to_owned();
    let mut edit = TextEdit::new(body);
    edit.set_caret(caret.min(body.len()));
    edit.focused = true;
    let base = source.to_owned();
    s.edit.set_caret(range.start + edit.focus);
    s.edit.focused = false;
    s.live = LiveEditor {
        full: false,
        range: Some(range),
        base,
        trailing,
        edit,
    };
}
pub fn at(s: &mut State, byte: usize) {
    let byte = byte.min(s.edit.text.len());
    let ranges = blocks(&s.edit.text);
    let range = ranges
        .iter()
        .find(|r| r.contains(&byte))
        .or_else(|| ranges.last())
        .unwrap()
        .clone();
    let offset = byte.saturating_sub(range.start);
    let source = &s.edit.text[range.clone()];
    let trimmed = source.trim_end_matches(['\r', '\n']).len();
    if offset > trimmed
        && (source[..offset].ends_with("\n\n") || source[..offset].ends_with("\r\n\r\n"))
    {
        activate(s, byte..byte, 0);
    } else {
        activate(s, range, offset);
    }
}
pub fn focus(s: &mut State) {
    if s.current.is_none() {
        return;
    }
    if s.live.range.is_some() {
        s.live.edit.focused = true;
        s.edit.focused = false;
    } else {
        at(s, s.edit.focus);
    }
}
pub fn pause(s: &mut State) {
    s.edit.focused = false;
    s.live.edit.focused = false;
}
pub fn blur(s: &mut State) {
    sync_selection(s);
    s.live = LiveEditor::default();
    s.edit.focused = false;
}
pub fn toggle(s: &mut State) {
    if s.live.range.is_some() || s.edit.focused {
        blur(s);
    } else {
        focus(s);
    }
}
pub fn changed(s: &mut State, tx: &mut Tx) {
    s.changed_at = Some(Instant::now());
    tx.wake_after(Duration::from_millis(700));
}
pub fn commit(s: &mut State, tx: &mut Tx) {
    let Some(range) = s.live.range.clone() else {
        return;
    };
    if s.edit.text.get(range.clone()) != Some(s.live.base.as_str()) {
        s.error =
            Some("The live edit no longer matches this note; your block text was retained.".into());
        return;
    }
    let replacement = format!("{}{}", s.live.edit.text, s.live.trailing);
    if replacement == s.live.base {
        return;
    }
    let anchor = range.start + s.live.edit.anchor;
    let focus = range.start + s.live.edit.focus;
    s.edit.undo_group(|doc| {
        doc.set_selection(range.start, range.end);
        doc.insert(&replacement);
        doc.set_selection(anchor, focus);
    });
    s.live.range = Some(range.start..range.start + replacement.len());
    s.live.base = replacement;
    changed(s, tx);
}
pub fn sync_selection(s: &mut State) {
    if let Some(r) = &s.live.range {
        s.edit
            .set_selection(r.start + s.live.edit.anchor, r.start + s.live.edit.focus);
    }
}
pub fn restore_selection(s: &mut State) {
    let (anchor, focus) = (s.edit.anchor, s.edit.focus);
    at(s, focus);
    let range = s.live.range.clone().unwrap();
    if anchor < range.start || anchor > range.end {
        s.live = LiveEditor::default();
        s.live.full = true;
        s.edit.focused = true;
    } else {
        s.live
            .edit
            .set_selection(anchor - range.start, focus - range.start);
    }
    s.edit.set_selection(anchor, focus);
}
pub fn history(s: &mut State, redo: bool, tx: &mut Tx) {
    sync_selection(s);
    if redo {
        s.edit.redo();
    } else {
        s.edit.undo();
    }
    restore_selection(s);
    changed(s, tx);
    tx.scroll_into_view("input");
}
pub fn whole(s: &mut State, select_all: bool) {
    sync_selection(s);
    s.live = LiveEditor::default();
    s.live.full = true;
    s.edit.focused = true;
    if select_all {
        s.edit.set_selection(0, s.edit.text.len());
    }
}

fn input_key(s: &mut State, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if s.live.edit.is_composing() {
        return false;
    }
    let hit = |c: &str| rune::Chord::parse(c).is_ok_and(|c| c.matches_tx(ev, tx));
    if hit("mod+a") {
        whole(s, true);
        return true;
    }
    if hit("mod+z") || hit("mod+shift+z") || (!cfg!(target_os = "macos") && hit("mod+y")) {
        history(s, ev.mods.shift || hit("mod+y"), tx);
        return true;
    }
    if ev.key == Key::Escape {
        blur(s);
        return true;
    }
    let document_nav = if cfg!(target_os = "macos") {
        ev.mods.cmd && matches!(ev.key, Key::Up | Key::Down)
    } else {
        ev.mods.ctrl && matches!(ev.key, Key::Home | Key::End)
    };
    if document_nav {
        sync_selection(s);
        let target = if matches!(ev.key, Key::Home | Key::Up) {
            0
        } else {
            s.edit.text.len()
        };
        let anchor = if ev.mods.shift { s.edit.anchor } else { target };
        s.edit.set_selection(anchor, target);
        restore_selection(s);
        tx.scroll_into_view("input");
        return true;
    }
    if crate::key(s, ev, tx) {
        return true;
    }
    let Some(range) = s.live.range.clone() else {
        return false;
    };
    let (a, b) = s.live.edit.range();
    let len = s.live.edit.text.len();
    let previous = (ev.key == Key::Left && a == 0)
        || (ev.key == Key::Up && !s.live.edit.text[..a].contains('\n'));
    let next = (ev.key == Key::Right && b == len)
        || (ev.key == Key::Down && !s.live.edit.text[b..].contains('\n'));
    if (previous && range.start > 0) || (next && range.end < s.edit.text.len()) {
        if ev.mods.shift {
            whole(s, false);
            s.edit.key(ev, None);
            return true;
        }
        let pos = if previous { range.start - 1 } else { range.end };
        at(s, pos);
        tx.scroll_into_view("input");
        return true;
    }
    if a == b
        && ((ev.key == Key::Backspace && a == 0 && range.start > 0)
            || (ev.key == Key::Delete && b == len && range.start + b < s.edit.text.len()))
    {
        s.edit.set_caret(if ev.key == Key::Backspace {
            range.start
        } else {
            range.start + b
        });
        s.edit.key(ev, None);
        at(s, s.edit.focus);
        changed(s, tx);
        return true;
    }
    if ev.key == Key::Enter && ev.mods == rune::Mods::default() {
        s.live.edit.key(
            &KeyEvent {
                key: Key::Enter,
                mods: rune::Mods {
                    shift: true,
                    ..Default::default()
                },
            },
            None,
        );
        commit(s, tx);
        if s.live.edit.text[..s.live.edit.focus].ends_with("\n\n") {
            at(s, s.edit.focus);
        }
        return true;
    }
    false
}

pub fn view(s: &State, k: &Kit, _: &mut Cx<State>) -> El {
    // Full-document selection uses the same native source field, not a second pane.
    if s.live.full || (s.live.range.is_none() && s.edit.focused) {
        return crate::ui::editor(s, k);
    }
    let ranges = if let Some(active) = &s.live.range {
        let mut out = Vec::new();
        if active.start > 0 {
            out.extend(blocks(&s.edit.text[..active.start]));
        }
        out.push(active.clone());
        if active.end < s.edit.text.len() {
            out.extend(
                blocks(&s.edit.text[active.end..])
                    .into_iter()
                    .map(|r| r.start + active.end..r.end + active.end),
            );
        }
        out
    } else {
        blocks(&s.edit.text)
    };
    let mut content = col()
        .w_full()
        .px(28.0)
        .py(16.0)
        .on_layout_tx(|s: &mut State, rect, tx| {
            let width = (rect[2] - 56.0).max(1.0);
            if (s.reading_width - width).abs() > 0.5 {
                s.reading_width = width;
            } else {
                tx.live_only();
            }
        });
    for range in ranges {
        let source = &s.edit.text[range.clone()];
        let active = s.live.range.as_ref() == Some(&range);
        if !active
            && source.trim().is_empty()
            && (s.live.range.is_some() || !s.edit.text.trim().is_empty())
        {
            continue;
        }
        let mut unit = col()
            .key(format!("live-block-{}", range.start))
            .w_full()
            .min_w(0.0)
            .py(8.0);
        if active {
            unit = unit.child(
                text_input(&s.live.edit, TextStyle::new(15.0, 400.0).lh(25.0))
                    .key("input")
                    .label("Markdown block")
                    .w_full()
                    .min_w(0.0)
                    .min_h(25.0)
                    .color(k.c.text_base)
                    .caret(k.c.text_accent, k.selection())
                    .wrap_anywhere()
                    .placeholder("Start writing…", k.c.text_muted)
                    .decorations(crate::markdown::source_spans(&s.live.edit.text, k))
                    .on_edit(|s: &mut State| {
                        // Rune reads the binding again after input, before on_change. Only
                        // the pre-edit read should update the canonical undo snapshot's caret.
                        if s.live.base.strip_suffix(&s.live.trailing)
                            == Some(s.live.edit.text.as_str())
                        {
                            sync_selection(s);
                        }
                        &mut s.live.edit
                    })
                    .on_input_key(input_key)
                    .on_input_change(commit),
            );
        } else {
            let caret = source.trim_end().len();
            let target = range.clone();
            unit = unit
                .on_press_tx(move |s: &mut State, tx| {
                    activate(s, target.clone(), caret);
                    tx.focus("input", false);
                })
                .role("group")
                .label("Edit Markdown block")
                .focusable(true)
                .child(if source.trim().is_empty() {
                    rune::text("Start writing…")
                        .font(k.ty.body)
                        .color(k.c.text_muted)
                        .w_full()
                        .min_h(25.0)
                } else {
                    crate::markdown::reading(source, k, s.reading_width).selectable(false)
                });
        }
        content = content.child(unit);
    }
    col()
        .key("editor-scroll")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .scroll_margin(24.0)
        .on_press_tx(|s: &mut State, tx| {
            focus(s);
            tx.focus("input", false);
        })
        .role("group")
        .label("Live Markdown document")
        .child(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocks_partition_exact_bytes_without_splitting_containers() {
        let source = "# Žąsis\r\n\r\n- one\r\n- two\r\n\r\n| a | b |\r\n|---|---|\r\n| x | y |\r\n\r\n```mermaid\r\nflowchart LR\r\nA-->B\r\n```\r\n";
        let ranges = blocks(source);
        assert_eq!(ranges.len(), 4);
        assert_eq!(
            ranges
                .iter()
                .map(|r| &source[r.clone()])
                .collect::<String>(),
            source
        );
        assert!(source[ranges[1].clone()].contains("- two"));
        assert!(source[ranges[2].clone()].contains("| x | y |"));
        assert!(source[ranges[3].clone()].contains("A-->B"));
    }
    #[test]
    fn empty_and_unclosed_fences_remain_editable_units() {
        assert_eq!(blocks(""), vec![0..0]);
        assert_eq!(blocks("```md\n# literal\n\n**literal"), vec![0..26]);
    }
}
