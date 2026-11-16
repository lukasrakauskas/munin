//! Desktop settings shell. Each tab owns one focused panel rather than a long card feed.
pub mod effects;
mod sync;

use crate::{live, state::State};
use rune::{El, Icon, ScrollSpec, TextStyle, col, row, text};
use rune_kit::{
    Button, ButtonVariant, IconButton, IconButtonVariant, Kit, TabStrip, glyphs,
    shell::{TabItem, TabOrientation},
};
use std::time::Duration;

pub static GEAR: Icon = Icon {
    name: "munin-settings",
    view: 24.0,
    body: r#"<path d="m10 3-.6 2.4-2 .9-2.2-.7-2 3.5L5 10.8v2.4l-1.8 1.7 2 3.5 2.2-.7 2 .9L10 21h4l.6-2.4 2-.9 2.2.7 2-3.5-1.8-1.7v-2.4l1.8-1.7-2-3.5-2.2.7-2-.9L14 3z" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/><circle cx="12" cy="12" r="3" fill="none" stroke="currentColor" stroke-width="1.4"/>"#,
};
static DEVICES: Icon = Icon {
    name: "munin-settings-devices",
    view: 24.0,
    body: r#"<rect x="2" y="4" width="14" height="11" rx="2" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M6 19h6m-3-4v4" stroke="currentColor" stroke-width="1.5"/><rect x="15" y="9" width="7" height="12" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.5"/>"#,
};
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Tab {
    General,
    #[default]
    Sync,
    Privacy,
}
impl Tab {
    fn id(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Sync => "sync",
            Self::Privacy => "privacy",
        }
    }
    fn from_id(id: &str) -> Self {
        match id {
            "general" => Self::General,
            "privacy" => Self::Privacy,
            _ => Self::Sync,
        }
    }
}
pub fn open(s: &mut State) {
    s.settings = true;
    s.dialog = None;
    s.context = None;
    s.launcher.open = false;
    s.launcher.query.focused = false;
    s.input.focused = false;
    live::pause(s);
}
pub fn close(s: &mut State) {
    s.settings = false;
    s.sync.pair_input.focused = false;
    s.sync.name_input.focused = false;
    live::focus(s);
}
fn result(s: &mut State, r: std::io::Result<()>) {
    s.sync.error = r.err().map(|e| e.to_string());
}
fn toggle(s: &mut State) {
    let enabled = !s.sync.enabled();
    let save_error = s.save().err();
    if enabled && let Some(e) = &save_error {
        s.sync.error = Some(e.to_string());
        return;
    }
    let r = s.sync.set_enabled(enabled);
    if r.is_ok() {
        crate::sync::capture(s);
        let _ = s.refresh();
    }
    result(s, r);
    if !enabled && let Some(e) = save_error {
        s.sync.error = Some(format!(
            "Sync paused. Your editor buffer was retained, but saving needs attention: {e}"
        ));
    }
}
fn paragraph(value: impl Into<String>, k: &Kit) -> El {
    text(value)
        .w_full()
        .wrap_anywhere()
        .font(k.ty.ui)
        .color(k.c.text_muted)
}
fn title(value: &str, k: &Kit) -> El {
    text(value.to_owned())
        .font(k.ty.body_medium)
        .color(k.c.text_base)
}
fn section(k: &Kit) -> El {
    col()
        .w_full()
        .min_w(0.0)
        .px(20.0)
        .py(18.0)
        .gap(14.0)
        .radius(10.0)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_muted)
}
fn heading(name: &str, description: &str, k: &Kit) -> El {
    col()
        .w_full()
        .gap(6.0)
        .child(
            text(name.to_owned())
                .w_full()
                .wrap_anywhere()
                .font(TextStyle::new(25.0, 650.0))
                .color(k.c.text_base),
        )
        .child(paragraph(description, k))
}
fn ago(elapsed: Duration) -> String {
    match elapsed.as_secs() {
        0..=9 => "Just now".into(),
        s @ 10..=59 => format!("{s}s ago"),
        s @ 60..=3599 => format!("{}m ago", s / 60),
        s => format!("{}h ago", s / 3600),
    }
}
fn general(s: &State, k: &Kit) -> El {
    let columns = if s.viewport.0 >= 650.0 { 3 } else { 2 };
    let mut choices = col().w_full().gap(8.0);
    for (row_index, themes) in crate::theme::THEMES.chunks(columns).enumerate() {
        let mut group = row().w_full().gap(8.0);
        for (column, theme) in themes.iter().enumerate() {
            let id = (row_index * columns + column) as u8;
            group = group.child(
                Button::new(theme.name)
                    .variant(ButtonVariant::Ghost)
                    .on_press(move |s: &mut State| {
                        s.theme = id;
                        crate::remember(s);
                    })
                    .build(k)
                    .key(format!("settings-theme-{id}"))
                    .fill(k.alpha(k.c.text_accent, if s.theme == id { 0.15 } else { 0.0 }))
                    .border(if s.theme == id { 1.0 } else { 0.0 }, k.c.border_focus)
                    .role("radio")
                    .checked(s.theme == id)
                    .focus_selected(s.theme == id)
                    .grow(1.0)
                    .min_w(0.0),
            );
        }
        choices = choices.child(group);
    }
    let layout = if s.viewport.0 >= 650.0 {
        row().center_y()
    } else {
        col()
    };
    col()
        .key("general-settings")
        .w_full()
        .gap(24.0)
        .child(heading(
            "General",
            "Make Munin feel at home on your desktop.",
            k,
        ))
        .child(
            section(k).child(
                col()
                    .w_full()
                    .gap(16.0)
                    .child(
                        col()
                            .grow(1.0)
                            .min_w(0.0)
                            .gap(6.0)
                            .child(title("Appearance", k))
                            .child(paragraph(
                                "Follow your system or choose a classic terminal palette. GitHub themes use yellow selection.",
                                k,
                            )),
                    )
                    .child(choices.role("radiogroup").label("Theme").focus_group()),
            ),
        )
        .child(
            section(k).child(
                layout
                    .w_full()
                    .gap(20.0)
                    .child(
                        col()
                            .grow(1.0)
                            .min_w(0.0)
                            .gap(6.0)
                            .child(title("Workspace", k))
                            .child(paragraph(
                                s.workspace
                                    .as_ref()
                                    .map(|p| p.display().to_string())
                                    .unwrap_or_else(|| "No workspace open".into()),
                                k,
                            )),
                    )
                    .child(
                        Button::new("Open workspace folder…")
                            .variant(ButtonVariant::Neutral)
                            .on_press_tx(crate::choose_folder)
                            .build(k)
                            .key("settings-open-workspace"),
                    ),
            ),
        )
}
fn privacy(k: &Kit) -> El {
    col().key("privacy-settings").w_full().gap(24.0)
        .child(heading("Privacy & storage", "Local files. Explicit trust. No accounts.",k))
        .child(section(k).child(title("Connections are end-to-end encrypted",k))
            .child(paragraph("Automerge merges offline edits; iroh authenticates and encrypts connections. Direct links are preferred, with public relays and address lookup as a fallback. Those services can observe connection metadata, but cannot read notes.",k)))
        .child(section(k).child(title("Nearby is visible only by choice",k))
            .child(paragraph("Turning on Nearby advertises this device’s name, public endpoint key and direct IP addresses to your local network for up to ten minutes. Never your workspace name, note contents, invitation capability or private key. Nearby requests expire after 90 seconds; both devices must compare the verification fingerprint and accept before syncing.",k)))
        .child(section(k).child(title("Trust belongs to each device",k))
            .child(paragraph("Pairing grants access to the selected Markdown workspace and retained history. Removing a device stops access here; it cannot erase copies already received or revoke another device’s trust. Remove a lost device everywhere. Discovery is not automatic authorization.",k)))
        .child(section(k).child(title("Your files stay yours",k))
            .child(paragraph("Only Markdown syncs—not attachments or empty folders. Both apps need to be open at overlapping times. Sync is not a backup. Local notes, private keys and CRDT history are not encrypted at rest.",k))
            .child(paragraph("Private state lives in .munin-sync. Never copy that folder to another device; pair it instead. Back up both the Markdown workspace and metadata before repairing or resetting history.",k)))
}
pub fn view(s: &State, k: &Kit, leading: f32, trailing: f32) -> El {
    let tabs = TabStrip::new("settings-tabs", TabOrientation::Horizontal)
        .tabs(
            [
                (Tab::General, "General"),
                (Tab::Sync, "Sync & devices"),
                (Tab::Privacy, "Privacy"),
            ]
            .into_iter()
            .map(|(tab, label)| {
                let label = if tab == Tab::Sync && !s.sync.requests.is_empty() {
                    format!("{label} ({})", s.sync.requests.len())
                } else {
                    label.to_owned()
                };
                TabItem::new(tab.id(), label)
                    .closable(false)
                    .icon(match tab {
                        Tab::General => &GEAR,
                        Tab::Sync => &DEVICES,
                        Tab::Privacy => &glyphs::LOCK,
                    })
            }),
        )
        .active(Some(s.settings_tab.id()))
        .viewport_width(Some((s.viewport.0 - 64.0).clamp(260.0, 700.0)))
        .row_height(36.0)
        .fill(k.c.page)
        .on_select(|s: &mut State, id, tx| {
            s.settings_tab = Tab::from_id(id);
            s.sync.pair_input.focused = false;
            s.sync.name_input.focused = false;
            tx.scroll_to_y("settings-scroll", 0.0);
        })
        .build(k);
    let panel = match s.settings_tab {
        Tab::General => general(s, k),
        Tab::Sync => sync::view(s, k),
        Tab::Privacy => privacy(k),
    };
    col()
        .key("settings-view")
        .w_full()
        .h_full()
        .min_h(0.0)
        .fill(k.c.page)
        .on_key(crate::key)
        .on_layout_tx(|s: &mut State, rect, tx| {
            let size = (rect[2], rect[3]);
            if s.viewport != size {
                s.viewport = size;
            } else {
                tx.live_only();
            }
        })
        .child(
            row()
                .w_full()
                .h(64.0)
                .pl(leading + 20.0)
                .pr(trailing + 20.0)
                .gap(12.0)
                .center_y()
                .window_drag()
                .child(
                    IconButton::new(&glyphs::CLOSE)
                        .variant(IconButtonVariant::GhostMuted)
                        .help("Back to notes · Escape")
                        .on_press(close)
                        .build(k)
                        .key("settings-close"),
                )
                .child(text("Settings").font(k.ty.body_medium).color(k.c.text_base))
                .child(
                    text("Munin")
                        .grow(1.0)
                        .min_w(0.0)
                        .truncate()
                        .font(k.ty.ui)
                        .color(k.c.text_muted),
                ),
        )
        .child(
            row()
                .w_full()
                .h(48.0)
                .px(if s.viewport.0 < 600.0 { 16.0 } else { 32.0 })
                .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_muted)
                .child(tabs),
        )
        .child(
            col()
                .key("settings-scroll")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .items_center()
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .child(
                    col()
                        .w_full()
                        .max_w(1184.0)
                        .min_w(0.0)
                        .px(if s.viewport.0 < 600.0 { 16.0 } else { 32.0 })
                        .py(28.0)
                        .child(panel.role("tabpanel").label(s.settings_tab.id())),
                ),
        )
}
