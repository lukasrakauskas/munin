//! A quiet, responsive settings surface; sync actions stay on the workspace model.
use crate::{live, state::State, sync};
use rune::{El, Icon, ScrollSpec, TextStyle, col, icon, row, text};
use rune_kit::{Button, ButtonVariant, IconButton, IconButtonVariant, Kit, TextInput, glyphs};
use std::time::Duration;

pub static GEAR: Icon = Icon {
    name: "munin-settings",
    view: 24.0,
    body: r#"<path d="m10 3-.6 2.4-2 .9-2.2-.7-2 3.5L5 10.8v2.4l-1.8 1.7 2 3.5 2.2-.7 2 .9L10 21h4l.6-2.4 2-.9 2.2.7 2-3.5-1.8-1.7v-2.4l1.8-1.7-2-3.5-2.2.7-2-.9L14 3z" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/><circle cx="12" cy="12" r="3" fill="none" stroke="currentColor" stroke-width="1.4"/>"#,
};
static DEVICES: Icon = Icon {
    name: "munin-devices",
    view: 24.0,
    body: r#"<rect x="2" y="4" width="14" height="11" rx="2" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M6 19h6m-3-4v4" stroke="currentColor" stroke-width="1.5"/><rect x="15" y="9" width="7" height="12" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M18 18h1" stroke="currentColor" stroke-width="1.5"/>"#,
};

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
        sync::capture(s);
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
fn card(k: &Kit) -> El {
    col()
        .w_full()
        .min_w(0.0)
        .px(20.0)
        .py(20.0)
        .gap(14.0)
        .radius(14.0)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_muted)
}
fn ago(elapsed: Duration) -> String {
    match elapsed.as_secs() {
        0..=9 => "Just now".into(),
        s @ 10..=59 => format!("{s}s ago"),
        s @ 60..=3599 => format!("{}m ago", s / 60),
        s => format!("{}h ago", s / 3600),
    }
}

pub fn view(s: &State, k: &Kit, leading: f32, trailing: f32) -> El {
    let enabled = s.sync.enabled();
    let workspace = s
        .workspace
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "No workspace open".into());
    let status = if !enabled {
        "Local only"
    } else if !s.sync.ready {
        "Starting connection…"
    } else if s.sync.pairing {
        "Connecting to device…"
    } else if s.sync.peers().is_empty() {
        "Ready to pair"
    } else {
        "Sync enabled"
    };
    let header = row()
        .w_full()
        .h(64.0)
        .pl(leading + 20.0)
        .pr(trailing + 20.0)
        .gap(12.0)
        .center_y()
        .window_drag()
        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_muted)
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
            text("Your space, your devices")
                .grow(1.0)
                .min_w(0.0)
                .truncate()
                .font(k.ty.ui)
                .color(k.c.text_muted),
        );
    let hero = card(k).key("sync-settings")
        .child(row().gap(14.0).center_y()
            .child(col().w(48.0).h(48.0).radius(13.0).fill(k.alpha(k.c.text_accent, 0.12)).center().child(icon(&DEVICES, 27.0).color(k.c.text_accent)))
            .child(col().grow(1.0).min_w(0.0).gap(4.0)
                .child(text("Device sync").font(TextStyle::new(23.0, 650.0)).color(k.c.text_base))
                .child(paragraph("A little closer, on every device.", k))))
        .child(paragraph("Your notes travel directly between devices you trust. No account, no subscription, and nothing to sign into.", k))
        .child(col().w_full().gap(6.0).px(14.0).py(12.0).radius(9.0).fill(k.c.page)
            .child(text(status).key("sync-status").font(k.ty.ui_medium).color(k.c.text_accent))
            .child(paragraph(format!("Workspace · {workspace}"), k))
            .child(paragraph(s.sync.last_success.map(|t| format!("Last exchange · {}", ago(t.elapsed()))).unwrap_or_else(|| "No device exchange yet".into()), k)))
        .child(Button::new(if enabled { "Pause sync" } else { "Enable device sync" }).disabled(s.workspace.is_none()).variant(if enabled { ButtonVariant::Neutral } else { ButtonVariant::Submit }).on_press_tx(|s: &mut State, tx| { toggle(s); tx.wake_after(Duration::from_millis(100)); }).build(k).key("sync-toggle"));
    let mut content = col()
        .w_full()
        .max_w(720.0)
        .min_w(0.0)
        .px(if s.viewport.0 < 500.0 { 16.0 } else { 32.0 })
        .py(28.0)
        .gap(20.0)
        .child(
            col()
                .gap(6.0)
                .child(
                    text("Make yourself at home.")
                        .font(TextStyle::new(28.0, 650.0))
                        .color(k.c.text_base)
                        .w_full()
                        .wrap_anywhere(),
                )
                .child(paragraph(
                    "A calm workspace. Connected only when you choose.",
                    k,
                )),
        )
        .child(hero);
    if let Some(error) = s.sync.file_error.as_ref().or(s.sync.error.as_ref()) {
        content = content.child(
            col()
                .key("sync-error")
                .w_full()
                .px(16.0)
                .py(14.0)
                .radius(10.0)
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
    if enabled {
        let identity = card(k)
            .child(title("This device", k))
            .child(
                TextInput::new(&s.sync.name_input)
                    .width(None)
                    .placeholder("A name you recognize")
                    .on_edit(|s: &mut State| &mut s.sync.name_input)
                    .on_key(crate::key)
                    .build(k)
                    .key("sync-device-name"),
            )
            .child(
                Button::new("Save device name")
                    .variant(ButtonVariant::Neutral)
                    .on_press(|s: &mut State| {
                        let r = s.sync.rename_device();
                        result(s, r);
                    })
                    .build(k),
            )
            .child(paragraph(
                format!("Device key · {}", s.sync.device_id().unwrap_or_default()),
                k,
            ));
        let mut invite = card(k).child(title("Bring another device along", k))
            .child(paragraph("On your other device, open a workspace and enable sync. Create an invitation here, then paste it there. Keep both apps open while they connect.", k));
        if let Some((code, created)) = &s.sync.invitation {
            let code = code.clone();
            invite = invite
                .child(paragraph(
                    format!(
                        "Single-use invitation · expires in {}m",
                        (600 - created.elapsed().as_secs().min(599)).div_ceil(60)
                    ),
                    k,
                ))
                .child(
                    text(code.clone())
                        .key("sync-invitation-code")
                        .w_full()
                        .max_h(90.0)
                        .wrap_anywhere()
                        .font(k.ty.ui)
                        .color(k.c.text_base)
                        .selectable(true),
                )
                .child(
                    Button::new("Copy invitation")
                        .variant(ButtonVariant::Submit)
                        .on_press_tx(move |_: &mut State, tx| {
                            tx.copy_secret(code.clone(), Duration::from_secs(600));
                        })
                        .build(k)
                        .key("sync-copy-invite"),
                )
                .child(
                    Button::new("Revoke invitation")
                        .variant(ButtonVariant::Ghost)
                        .on_press(|s: &mut State| {
                            let r = s.sync.revoke();
                            result(s, r);
                        })
                        .build(k)
                        .key("sync-revoke"),
                );
        } else {
            invite = invite.child(
                Button::new("Create invitation")
                    .variant(ButtonVariant::Neutral)
                    .on_press_tx(|s: &mut State, tx| {
                        let r = s.sync.invite();
                        result(s, r);
                        tx.wake_after(Duration::from_millis(100));
                    })
                    .build(k)
                    .key("sync-invite"),
            );
        }
        invite = invite.child(paragraph("Invitations are secret keys to this workspace. Share privately; each expires after 10 minutes and can approve one device.", k));
        let pair = card(k).child(title("Have an invitation?", k))
            .child(paragraph("Paste a code from your other device. Notes already in this workspace are kept, including separate copies for colliding filenames.", k))
            .child(TextInput::new(&s.sync.pair_input).width(None).placeholder("munin1-…").on_edit(|s: &mut State| &mut s.sync.pair_input).on_key(crate::key).on_submit(|s: &mut State, tx| { let r = s.sync.pair(); result(s, r); tx.wake_after(Duration::from_millis(100)); }).build(k).key("sync-pair-field"))
            .child(Button::new(if s.sync.pairing { "Connecting…" } else { "Connect device" }).disabled(s.sync.pair_input.text.trim().is_empty() || s.sync.pairing).variant(ButtonVariant::Submit).on_press_tx(|s: &mut State, tx| { let r = s.sync.pair(); result(s, r); tx.wake_after(Duration::from_millis(100)); }).build(k).key("sync-pair"));
        let mut devices = card(k).child(title("Trusted devices", k));
        if s.sync.peers().is_empty() {
            devices = devices.child(paragraph(
                "Just this device for now. Add another to make your notes feel at home everywhere.",
                k,
            ));
        }
        for peer in s.sync.peers() {
            let id = peer.addr.id.to_string();
            let remove_id = id.clone();
            let state = s.sync.peer_errors.get(&id).cloned().unwrap_or_else(|| {
                s.sync
                    .last_seen
                    .get(&id)
                    .map(|t| format!("Last exchange · {}", ago(t.elapsed())))
                    .unwrap_or_else(|| "Waiting for device…".into())
            });
            devices = devices.child(
                col()
                    .w_full()
                    .min_w(0.0)
                    .gap(6.0)
                    .py(12.0)
                    .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_muted)
                    .child(
                        row()
                            .w_full()
                            .min_w(0.0)
                            .gap(8.0)
                            .center_y()
                            .child(
                                text(peer.name.clone())
                                    .grow(1.0)
                                    .min_w(0.0)
                                    .truncate()
                                    .font(k.ty.ui_medium)
                                    .color(k.c.text_base),
                            )
                            .child(
                                Button::new("Remove")
                                    .variant(ButtonVariant::Ghost)
                                    .on_press(move |s: &mut State| {
                                        let r = s.sync.forget(&remove_id);
                                        result(s, r);
                                    })
                                    .build(k)
                                    .key(format!("sync-remove-{id}")),
                            ),
                    )
                    .child(paragraph(state, k))
                    .child(paragraph(format!("Key · {}", &id[..16]), k)),
            );
        }
        devices = devices.child(paragraph("Removing a device stops its access here; it does not erase notes it already has. Remove it on your other devices too. Trust is explicit, not automatically shared.", k));
        content = content
            .child(identity)
            .child(invite)
            .child(pair)
            .child(devices);
    }
    let mut appearance = card(k).child(title("Appearance", k));
    for (theme, label) in [(0, "Follow system"), (1, "Light"), (2, "Dark")] {
        appearance = appearance.child(
            Button::new(if s.theme == theme {
                format!("✓  {label}")
            } else {
                label.into()
            })
            .variant(if s.theme == theme {
                ButtonVariant::Neutral
            } else {
                ButtonVariant::Ghost
            })
            .on_press(move |s: &mut State| {
                s.theme = theme;
                crate::remember(s);
            })
            .build(k)
            .key(format!("settings-theme-{theme}")),
        );
    }
    content = content.child(appearance).child(card(k).child(title("Private by design. Local first.", k))
        .child(paragraph("Automerge merges offline edits. iroh encrypts connections end to end, using direct links where possible and public relays/discovery when needed. Relays cannot read notes, but can observe connection metadata.", k))
        .child(paragraph("Only Markdown notes sync—not attachments or empty folders. Both apps must be open at overlapping times. This is sync, not a backup. Local files and CRDT history are not encrypted at rest.", k))
        .child(paragraph("Private keys and history live in .munin-sync. Do not share or copy that folder to another device; pair it instead. Pausing keeps your files, trust and history for later.", k)));
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
        .child(header)
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
                .child(content),
        )
}
