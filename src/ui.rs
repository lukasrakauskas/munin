//! Workspace chrome and the full-pane writing surface. Editing and persistence stay in State.
use super::{choose_folder, key, remember, report, start_dialog, state::State, submit};
use rune::{Color, Cx, El, Icon, ScrollSpec, TextStyle, col, el, icon, row, text, text_input};
use rune_kit::{
    Button, ButtonVariant, ControlSize, FileNode, IconButton, IconButtonVariant, Kit, TextInput,
    Tree, TreeItem, TreeStyle, glyphs,
};
use std::time::{Duration, Instant};

const GUTTER: f32 = 28.0;
static PANEL: Icon = Icon {
    name: "munin-panel",
    view: 24.0,
    body: r#"<rect x="3" y="4" width="18" height="16" rx="2" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M9 4v16" stroke="currentColor" stroke-width="1.5"/>"#,
};
static PLUS: Icon = Icon {
    name: "munin-plus",
    view: 24.0,
    body: r#"<path d="M12 5v14M5 12h14" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#,
};
static RENAME: Icon = Icon {
    name: "munin-rename",
    view: 24.0,
    body: r#"<path d="m15 4 5 5-11 11H4v-5L15 4Zm-2 2 5 5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#,
};
static TRASH: Icon = Icon {
    name: "munin-trash",
    view: 24.0,
    body: r#"<path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 11v5M14 11v5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

fn kit(dark: bool) -> Kit {
    let mut k = Kit::neutral(dark);
    let c = |s| rune::color::hex(s).expect("valid theme color");
    k.c.page = if dark { c("#202024") } else { Color::WHITE };
    k.c.layer_01 = if dark { c("#19191d") } else { c("#f5f5f8") };
    k.c.text_accent = if dark { c("#bca5ff") } else { c("#7050bb") };
    k.c.icon_accent = k.c.text_accent;
    k.c.border_focus = k.c.text_accent;
    k.c.bg_accent = k.c.text_accent;
    k
}

fn action(glyph: &'static Icon, label: &str) -> IconButton {
    IconButton::new(glyph)
        .variant(IconButtonVariant::GhostMuted)
        .control_size(ControlSize::Large)
        .help(label)
}

fn label(n: &FileNode<()>) -> &str {
    &n.name
}

fn sidebar(s: &State, k: &Kit) -> El {
    let workspace = s
        .workspace
        .as_deref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Your workspace".into());
    let mut side = col()
        .key("sidebar")
        .w(248.0)
        .h_full()
        .min_h(0.0)
        .fill(k.c.layer_01)
        .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_muted)
        .child(
            row()
                .h(72.0)
                .px(20.0)
                .gap(10.0)
                .center_y()
                .child(
                    col()
                        .w(32.0)
                        .h(32.0)
                        .radius(9.0)
                        .fill(k.alpha(k.c.text_accent, 0.14))
                        .center()
                        .child(
                            text("M")
                                .font(TextStyle::new(18.0, 650.0))
                                .color(k.c.text_accent),
                        ),
                )
                .child(
                    col()
                        .gap(2.0)
                        .child(
                            text("Munin")
                                .font(TextStyle::new(15.0, 600.0))
                                .color(k.c.text_base),
                        )
                        .child(
                            text("A place for your thoughts")
                                .font(k.ty.small)
                                .color(k.c.text_muted),
                        ),
                ),
        )
        .child(
            row()
                .px(16.0)
                .gap(8.0)
                .center_y()
                .child(icon(&glyphs::FOLDER_OPEN, 16.0).color(k.c.icon_muted))
                .child(
                    text(workspace)
                        .font(k.ty.ui_medium)
                        .color(k.c.text_base)
                        .grow(1.0)
                        .min_w(0.0)
                        .truncate(),
                )
                .child(
                    action(&glyphs::FOLDER, "Open workspace · Mod+O")
                        .on_press_tx(|s: &mut State, tx| choose_folder(s, tx))
                        .build(k)
                        .key("open-workspace"),
                ),
        );
    if s.workspace.is_some() {
        let mut tree_style = TreeStyle::from_kit(k)
            .radius(6.0)
            .gap(7.0)
            .fade_long_labels(16.0);
        tree_style.row_height = 30.0;
        side = side
            .child(
                Button::new("Find a note…")
                    .icon(&glyphs::SEARCH)
                    .variant(ButtonVariant::GhostMuted)
                    .on_press(|s: &mut State| start_dialog(s, "Quick open"))
                    .build(k)
                    .key("quick-open")
                    .mx(12.0)
                    .mt(12.0)
                    .w(224.0)
                    .help("Quick open · Mod+P"),
            )
            .child(
                row()
                    .px(18.0)
                    .mt(20.0)
                    .mb(6.0)
                    .center_y()
                    .child(
                        text("NOTES")
                            .font(k.ty.small)
                            .color(k.c.text_muted)
                            .grow(1.0),
                    )
                    .child(
                        action(&PLUS, "New note · Mod+N")
                            .on_press(|s: &mut State| start_dialog(s, "New note"))
                            .build(k)
                            .key("new-note"),
                    )
                    .child(
                        action(&glyphs::FOLDER, "New folder")
                            .on_press(|s: &mut State| start_dialog(s, "New folder"))
                            .build(k)
                            .key("new-folder"),
                    ),
            )
            .child(
                col().px(10.0).grow(1.0).min_h(0.0).child(
                    Tree::new(|s: &State| &s.files, |s: &mut State| &mut s.files, label)
                        .name("Workspace files")
                        .row(TreeItem::file)
                        .style(tree_style)
                        .on_select(|s: &mut State, id, _| {
                            if s.files.get(id).is_some_and(|n| !n.dir) {
                                let r = s.open_file(id);
                                if r.is_err()
                                    && let Some(path) = &s.current
                                {
                                    s.files.select(&super::state::path_id(path));
                                }
                                report(s, r);
                            }
                        })
                        .build(s, k)
                        .grow(1.0)
                        .min_h(0.0),
                ),
            );
    } else {
        side = side.child(
            col()
                .px(20.0)
                .py(24.0)
                .gap(10.0)
                .grow(1.0)
                .child(
                    text("Your notes, on your device.")
                        .w_full()
                        .wrap()
                        .font(k.ty.ui_medium)
                        .color(k.c.text_base),
                )
                .child(
                    text("Open a folder to bring your Markdown files together.")
                        .w_full()
                        .wrap()
                        .font(k.ty.ui)
                        .color(k.c.text_muted),
                ),
        );
    }
    side.child(
        col()
            .px(16.0)
            .py(14.0)
            .gap(8.0)
            .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_muted)
            .child(
                Button::new(match s.theme {
                    1 => "Appearance: Light",
                    2 => "Appearance: Dark",
                    _ => "Appearance: System",
                })
                .variant(ButtonVariant::GhostMuted)
                .on_press(|s: &mut State| {
                    s.theme = (s.theme + 1) % 3;
                    remember(s);
                })
                .build(k)
                .key("theme-toggle"),
            )
            .child(
                row()
                    .gap(6.0)
                    .center_y()
                    .child(icon(&glyphs::LOCK, 12.0).color(k.c.icon_muted))
                    .child(
                        text("Local files. No cloud required.")
                            .font(k.ty.small)
                            .color(k.c.text_muted),
                    ),
            ),
    )
}

fn header(s: &State, k: &Kit) -> El {
    let title = s
        .current
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Your writing space".into());
    let mut header = row()
        .key("note-header")
        .w_full()
        .h(56.0)
        .px(GUTTER)
        .gap(12.0)
        .center_y()
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_muted)
        .child(
            action(&PANEL, "Toggle sidebar · Mod+B")
                .on_press(|s: &mut State| s.sidebar = !s.sidebar)
                .build(k)
                .key("toggle-sidebar"),
        )
        .child(
            text(title.clone())
                .key("note-path")
                .font(k.ty.ui_medium)
                .color(k.c.text_muted)
                .grow(1.0)
                .min_w(0.0)
                .truncate()
                .help(title),
        );
    if s.current.is_some() {
        header = header
            .child(
                action(&RENAME, "Rename note")
                    .on_press(|s: &mut State| start_dialog(s, "Rename note"))
                    .build(k)
                    .key("rename-note"),
            )
            .child(
                action(&TRASH, "Delete note")
                    .on_press(|s: &mut State| start_dialog(s, "Delete note"))
                    .build(k)
                    .key("delete-note"),
            )
            .child(
                action(&glyphs::CLOSE, "Close note · Mod+W")
                    .on_press_tx(|s: &mut State, tx| {
                        key(
                            s,
                            &rune::KeyEvent {
                                key: rune::Key::Command('w'),
                                mods: rune::Mods {
                                    cmd: true,
                                    ctrl: !cfg!(target_os = "macos"),
                                    logo: cfg!(target_os = "macos"),
                                    ..Default::default()
                                },
                            },
                            tx,
                        );
                    })
                    .build(k)
                    .key("close-note"),
            );
    }
    header
}

fn editor(s: &State, k: &Kit) -> El {
    // One bounded viewport, with gutters INSIDE its content. No auto-growing form shell:
    // both the painted surface and scroll hit area fill the available pane.
    col()
        .key("editor-scroll")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .fill(k.c.page)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .scroll_margin(24.0)
        .on_press_tx(|s: &mut State, tx| {
            if s.dialog.is_none() {
                tx.focus("input", false);
            }
        })
        .role("group")
        .label("Writing area")
        .child(
            col().w_full().px(GUTTER).py(24.0).child(
                text_input(&s.edit, TextStyle::new(15.0, 400.0).lh(24.0))
                    .key("input")
                    .label("Markdown note")
                    .w_full()
                    .min_w(0.0)
                    .min_h(24.0)
                    .color(k.c.text_base)
                    .caret(k.c.text_accent, k.selection())
                    .placeholder("Let your thoughts take shape…", k.c.text_muted)
                    .wrap_anywhere()
                    .decorations(rune_kit::syntax::SyntaxTheme::from_kit(k).spans(
                        &rune_kit::syntax::highlight(Some("markdown"), &s.edit.text),
                        0,
                    ))
                    .on_edit(|s: &mut State| &mut s.edit)
                    .on_input_key(|s: &mut State, ev, tx| {
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
                    .on_input_change(|s: &mut State, tx| {
                        s.changed_at = Some(Instant::now());
                        tx.wake_after(Duration::from_millis(700));
                    }),
            ),
        )
}

fn status(s: &State, k: &Kit) -> El {
    let words = s.edit.text.split_whitespace().count();
    let status = if s.error.is_some() {
        "Needs attention"
    } else if s.edit.text != s.saved {
        "Unsaved changes"
    } else {
        "Saved"
    };
    row()
        .key("status-bar")
        .h(32.0)
        .px(GUTTER)
        .gap(8.0)
        .center_y()
        .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_muted)
        .child(el().w(5.0).h(5.0).radius(3.0).fill(if s.error.is_some() {
            k.c.danger.fg
        } else if s.edit.text != s.saved {
            k.c.text_accent
        } else {
            k.c.success.fg
        }))
        .child(
            text(status)
                .key("save-status")
                .font(k.ty.small)
                .color(k.c.text_muted),
        )
        .child(el().grow(1.0))
        .child(
            text(format!(
                "{words} {}",
                if words == 1 { "word" } else { "words" }
            ))
            .key("word-count")
            .font(k.ty.small)
            .color(k.c.text_muted),
        )
        .child(text("·  Markdown").font(k.ty.small).color(k.c.text_muted))
}

fn empty(s: &State, k: &Kit) -> El {
    col()
        .key("empty-state")
        .grow(1.0)
        .min_h(0.0)
        .w_full()
        .center()
        .padding(GUTTER)
        .gap(16.0)
        .child(
            col()
                .w(64.0)
                .h(64.0)
                .radius(18.0)
                .fill(k.alpha(k.c.text_accent, 0.1))
                .center()
                .child(icon(&glyphs::FILE_TEXT, 28.0).color(k.c.text_accent)),
        )
        .child(
            text("Make room for a new idea.")
                .w_full()
                .wrap()
                .align(rune::Align::Center)
                .font(TextStyle::new(24.0, 550.0).lh(32.0))
                .color(k.c.text_base),
        )
        .child(
            text(if s.workspace.is_some() {
                "Choose a note, or start with a blank page."
            } else {
                "A quiet home for your notes. Just Markdown, just yours."
            })
            .w_full()
            .wrap()
            .align(rune::Align::Center)
            .font(k.ty.ui)
            .color(k.c.text_muted),
        )
        .child(if s.workspace.is_some() {
            Button::new("Create a note")
                .icon(&PLUS)
                .on_press(|s: &mut State| start_dialog(s, "New note"))
                .build(k)
        } else {
            Button::new("Open a folder")
                .icon(&glyphs::FOLDER_OPEN)
                .on_press_tx(|s: &mut State, tx| choose_folder(s, tx))
                .build(k)
        })
        .child(
            text(if cfg!(target_os = "macos") {
                "⌘O  Open folder     ⌘P  Find a note"
            } else {
                "Ctrl+O  Open folder     Ctrl+P  Find a note"
            })
            .w_full()
            .wrap()
            .align(rune::Align::Center)
            .font(k.ty.small)
            .color(k.c.text_muted),
        )
}

fn dialog(s: &State, mode: &str, k: &Kit) -> El {
    let mut d = col()
        .key("note-dialog")
        .w_full()
        .gap(12.0)
        .px(GUTTER)
        .py(20.0)
        .max_h(320.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .fill(k.c.layer_01)
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_muted)
        .child(text(mode).font(k.ty.body_medium).color(k.c.text_base));
    if mode == "Delete note" {
        d = d.child(
            text("Delete the current note? This cannot be undone.")
                .w_full()
                .wrap()
                .font(k.ty.ui)
                .color(k.c.text_muted),
        );
    } else {
        d = d.child(
            TextInput::new(&s.input)
                .width(None)
                .placeholder(if mode == "Quick open" {
                    "Search notes by path…"
                } else {
                    "Name or folder/name"
                })
                .on_edit(|s: &mut State| &mut s.input)
                .on_key(|s: &mut State, ev, tx| {
                    if ev.key == rune::Key::Escape {
                        key(s, ev, tx)
                    } else {
                        false
                    }
                })
                .on_submit(|s: &mut State, _| submit(s))
                .build(k)
                .key("dialog-field"),
        );
    }
    if mode == "Quick open" {
        let notes = s.find_notes(s.input.text.trim());
        if notes.is_empty() {
            d = d.child(
                text("No matching notes")
                    .font(k.ty.ui)
                    .color(k.c.text_muted),
            );
        }
        for path in notes.into_iter().take(12) {
            let caption = path.to_string_lossy().into_owned();
            d = d.child(
                Button::new(caption.clone())
                    .label_with(move |font, color| {
                        text(caption)
                            .font(font)
                            .color(color)
                            .grow(1.0)
                            .min_w(0.0)
                            .truncate()
                    })
                    .variant(ButtonVariant::Ghost)
                    .on_press(move |s: &mut State| {
                        let r = s.open_file(&path);
                        if r.is_ok() {
                            super::dismiss_dialog(s);
                        }
                        report(s, r);
                    })
                    .build(k),
            );
        }
    }
    d.child(
        row()
            .gap(8.0)
            .child(
                Button::new(if mode == "Delete note" {
                    "Delete note"
                } else if mode == "Quick open" {
                    "Open note"
                } else {
                    "Confirm"
                })
                .variant(if mode == "Delete note" {
                    ButtonVariant::Danger
                } else {
                    ButtonVariant::Neutral
                })
                .on_press(|s: &mut State| submit(s))
                .build(k)
                .key("confirm-dialog"),
            )
            .child(
                Button::new("Cancel")
                    .variant(ButtonVariant::Ghost)
                    .on_press(|s: &mut State| super::dismiss_dialog(s))
                    .build(k)
                    .key("cancel-dialog"),
            ),
    )
}

pub(super) fn view(s: &State, cx: &mut Cx<State>) -> El {
    let dark = match s.theme {
        1 => false,
        2 => true,
        _ => cx.system_dark(),
    };
    let k = kit(dark);
    let mut pane = col()
        .key("writing-surface")
        .grow(1.0)
        .min_w(0.0)
        .min_h(0.0)
        .h_full()
        .fill(k.c.page)
        .child(header(s, &k));
    if let Some(error) = &s.error {
        pane = pane.child(
            col()
                .key("error-banner")
                .w_full()
                .max_h(100.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .px(GUTTER)
                .py(12.0)
                .fill(k.c.danger.bg)
                .child(
                    text(error.clone())
                        .w_full()
                        .wrap_anywhere()
                        .font(k.ty.ui)
                        .color(k.c.danger.fg),
                ),
        );
    }
    if let Some(mode) = s.dialog {
        pane = pane.child(dialog(s, mode, &k));
    }
    pane = if s.current.is_some() {
        pane.child(editor(s, &k)).child(status(s, &k))
    } else {
        pane.child(empty(s, &k))
    };
    let mut body = row().w_full().h_full().fill(k.c.page);
    if s.sidebar {
        body = body.child(sidebar(s, &k));
    }
    body.on_key(key).child(pane)
}
