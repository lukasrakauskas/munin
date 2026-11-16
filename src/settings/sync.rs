//! Compact overview + connection workbench for the Sync tab.
use super::{ago, heading, paragraph, result, section, title, toggle};
use crate::state::State;
use rune::{El, ScrollSpec, TextStyle, col, row, text};
use rune_kit::{Button, ButtonVariant, Kit, TextInput, glyphs};
use std::time::Duration;

fn overview(s: &State, k: &Kit) -> El {
    let enabled = s.sync.enabled();
    let status = if !enabled {
        "Local only"
    } else if !s.sync.ready {
        "Starting connection…"
    } else if s.sync.pairing {
        "Connecting…"
    } else if s.sync.peers().is_empty() {
        "Ready to pair"
    } else {
        "Sync enabled"
    };
    let workspace = s
        .workspace
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "No workspace open".into());
    let mut body = col().w_full().min_w(0.0).gap(20.0).child(
        section(k)
            .child(title("Workspace sync", k))
            .child(
                text(status)
                    .key("sync-status")
                    .font(k.ty.ui_medium)
                    .color(k.c.text_accent),
            )
            .child(paragraph(workspace, k))
            .child(paragraph(
                s.sync
                    .last_success
                    .map(|t| format!("Last exchange · {}", ago(t.elapsed())))
                    .unwrap_or_else(|| "No device exchange yet".into()),
                k,
            ))
            .child(
                Button::new(if enabled {
                    "Pause sync"
                } else {
                    "Enable device sync"
                })
                .disabled(s.workspace.is_none())
                .variant(if enabled {
                    ButtonVariant::Neutral
                } else {
                    ButtonVariant::Submit
                })
                .on_press_tx(|s: &mut State, tx| {
                    toggle(s);
                    tx.wake_after(Duration::from_millis(100));
                })
                .build(k)
                .key("sync-toggle"),
            ),
    );
    if enabled {
        body = body.child(
            section(k)
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
                    Button::new("Save name")
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
                )),
        );
    }
    body
}
fn nearby(s: &State, k: &Kit) -> El {
    let mut body=section(k).key("nearby-settings").child(title("Nearby devices",k))
        .child(paragraph("On the same network? Turn on Nearby on both devices. Choose a device, compare the short fingerprint, and accept on both—no invitation to send.",k))
        .child(Button::new(if s.sync.nearby {"Stop nearby discovery"}else{"Find nearby devices"}).disabled(!s.sync.enabled()).variant(ButtonVariant::Neutral)
            .on_press_tx(|s: &mut State,tx|{let r=s.sync.set_nearby(!s.sync.nearby);result(s,r);tx.wake_after(Duration::from_millis(100));}).build(k).key("sync-nearby-toggle"));
    if s.sync.nearby {
        body=body.child(paragraph("Discoverable for up to 10 minutes. Only your device name, public key and direct addresses are announced on this network.",k));
        let peers: Vec<_> = s
            .sync
            .nearby_devices
            .values()
            .filter(|p| {
                !s.sync
                    .peers()
                    .iter()
                    .any(|trusted| trusted.addr.id == p.addr.id)
            })
            .collect();
        if peers.is_empty() {
            body=body.child(paragraph("Looking for devices… Keep Munin open on both. If nothing appears, check local-network permissions, firewall or guest-Wi-Fi isolation; invitations still work below.",k));
        }
        for peer in peers {
            let id = peer.addr.id.to_string();
            let connect = id.clone();
            let pending = s
                .sync
                .requests
                .values()
                .any(|r| r.peer.addr.id == peer.addr.id);
            body = body.child(
                col()
                    .w_full()
                    .min_w(0.0)
                    .gap(6.0)
                    .py(10.0)
                    .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_muted)
                    .child(
                        row()
                            .w_full()
                            .min_w(0.0)
                            .gap(10.0)
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
                                Button::new("Connect")
                                    .disabled(pending || s.sync.pairing)
                                    .variant(ButtonVariant::Neutral)
                                    .on_press_tx(move |s: &mut State, tx| {
                                        let r = s.sync.connect_nearby(&connect);
                                        result(s, r);
                                        tx.wake_after(Duration::from_millis(100));
                                    })
                                    .build(k)
                                    .key(format!("nearby-connect-{id}")),
                            ),
                    )
                    .child(paragraph(format!("Key · {}", &id[..16]), k)),
            );
        }
    } else if !s.sync.enabled() {
        body=body.child(paragraph("Enable workspace sync first. Nearby discovery is always opt-in and never grants access by itself.",k));
    }
    body
}
fn requests(s: &State, k: &Kit) -> El {
    let mut body = col().w_full().gap(16.0);
    for request in s.sync.requests.values() {
        let accept = request.id.clone();
        let decline = request.id.clone();
        let answered = s.sync.accepted_requests.contains(&request.id);
        body=body.child(section(k).key(format!("nearby-request-{}",request.id))
            .child(title(if request.outgoing {"Confirm this connection"}else{"A device wants to connect"},k))
            .child(paragraph(format!("{} · key {}",request.peer.name,&request.peer.addr.id.to_string()[..16]),k))
            .child(text(request.fingerprint.clone()).w_full().wrap_anywhere().font(TextStyle { size: 25.0, ..k.ty.mono }).color(k.c.text_accent).key(format!("nearby-fingerprint-{}",request.id)))
            .child(paragraph("Compare this fingerprint on both devices. Accept only if they match and you recognize the device. This shares the entire selected Markdown workspace and its history.",k))
            .child(paragraph(if answered {"Accepted here. Waiting for approval on the other device…".into()}else{format!("Request expires in {}s",request.expires.saturating_duration_since(std::time::Instant::now()).as_secs())},k))
            .child(row().w_full().gap(8.0)
                .child(Button::new("Accept").disabled(answered).variant(ButtonVariant::Submit).on_press_tx(move |s: &mut State,tx|{let r=s.sync.decide(&accept,true);result(s,r);tx.wake_after(Duration::from_millis(100));}).build(k).key(format!("nearby-accept-{}",request.id)))
                .child(Button::new("Decline").disabled(answered).variant(ButtonVariant::Ghost).on_press_tx(move |s: &mut State,tx|{let r=s.sync.decide(&decline,false);result(s,r);tx.wake_after(Duration::from_millis(100));}).build(k).key(format!("nearby-decline-{}",request.id)))))
    }
    body
}
fn invitation(s: &State, k: &Kit) -> El {
    let mut body=section(k).child(title("Connect with an invitation",k)).child(paragraph("For another network, or if local discovery is blocked. Create a private, single-use invitation and paste it on your other device.",k));
    if let Some((code, created)) = &s.sync.invitation {
        let code = code.clone();
        body = body
            .child(paragraph(
                format!(
                    "Expires in {}m",
                    (600 - created.elapsed().as_secs().min(599)).div_ceil(60)
                ),
                k,
            ))
            .child(
                col()
                    .key("sync-invitation-viewport")
                    .w_full()
                    .min_w(0.0)
                    .h(112.0)
                    .min_h(0.0)
                    .radius(8.0)
                    .fill(k.c.page)
                    .border(1.0, k.c.border_muted)
                    .selection_group(true)
                    .scroll(ScrollSpec {
                        stick_bottom: false,
                    })
                    .child(
                        text(code.clone())
                            .key("sync-invitation-code")
                            .w_full()
                            .px(12.0)
                            .py(10.0)
                            .wrap_anywhere()
                            .font(k.ty.mono)
                            .color(k.c.text_base)
                            .selectable(true)
                            .selection_color(k.selection()),
                    ),
            )
            .child(
                row()
                    .w_full()
                    .gap(8.0)
                    .child(
                        Button::new("Copy invitation")
                            .icon(&glyphs::COPY)
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
                    ),
            );
    } else {
        body = body.child(
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
    body=body.child(paragraph("Invitations are secret keys to this workspace. Share privately; each expires after 10 minutes and approves one device.",k));
    body.child(
        TextInput::new(&s.sync.pair_input)
            .width(None)
            .placeholder("Paste an invitation · munin1-…")
            .on_edit(|s: &mut State| &mut s.sync.pair_input)
            .on_key(crate::key)
            .on_submit(|s: &mut State, tx| {
                let r = s.sync.pair();
                result(s, r);
                tx.wake_after(Duration::from_millis(100));
            })
            .build(k)
            .key("sync-pair-field"),
    )
    .child(
        Button::new(if s.sync.pairing {
            "Connecting…"
        } else {
            "Accept invitation"
        })
        .disabled(s.sync.pair_input.text.trim().is_empty() || s.sync.pairing)
        .variant(ButtonVariant::Submit)
        .on_press_tx(|s: &mut State, tx| {
            let r = s.sync.pair();
            result(s, r);
            tx.wake_after(Duration::from_millis(100));
        })
        .build(k)
        .key("sync-pair"),
    )
}
fn trusted(s: &State, k: &Kit) -> El {
    let mut body = section(k).child(title("Trusted devices", k));
    if s.sync.peers().is_empty() {
        body = body.child(paragraph(
            "No paired devices yet. Use Nearby or an invitation to connect one.",
            k,
        ));
    }
    for peer in s.sync.peers() {
        let id = peer.addr.id.to_string();
        let remove = id.clone();
        let status = s.sync.peer_errors.get(&id).cloned().unwrap_or_else(|| {
            s.sync
                .last_seen
                .get(&id)
                .map(|t| format!("Last exchange · {}", ago(t.elapsed())))
                .unwrap_or_else(|| "Waiting for device…".into())
        });
        body = body.child(
            col()
                .w_full()
                .min_w(0.0)
                .gap(6.0)
                .py(10.0)
                .border_sides([1.0, 0.0, 0.0, 0.0], k.c.border_muted)
                .child(
                    row()
                        .w_full()
                        .min_w(0.0)
                        .gap(10.0)
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
                                    let r = s.sync.forget(&remove);
                                    result(s, r);
                                })
                                .build(k)
                                .key(format!("sync-remove-{id}")),
                        ),
                )
                .child(paragraph(status, k))
                .child(paragraph(format!("Key · {}", &id[..16]), k)),
        );
    }
    body.child(paragraph("Removing stops access here, not copies already received. Remove lost devices everywhere. Trust is explicit, not automatically shared.",k))
}
pub(super) fn view(s: &State, k: &Kit) -> El {
    let mut panel = col()
        .key("sync-settings")
        .w_full()
        .min_w(0.0)
        .gap(24.0)
        .child(heading(
            "Sync & devices",
            "Connect your own devices. No account, no subscription.",
            k,
        ));
    if let Some(success) = &s.sync.success {
        panel = panel.child(
            section(k)
                .key("pairing-success")
                .border(1.0, k.c.border_focus)
                .child(
                    row()
                        .w_full()
                        .min_w(0.0)
                        .gap(12.0)
                        .center_y()
                        .child(
                            col()
                                .grow(1.0)
                                .min_w(0.0)
                                .gap(4.0)
                                .child(title(&format!("Paired with {}", success.name), k))
                                .child(paragraph(
                                    "Your first encrypted exchange is complete. Welcome aboard.",
                                    k,
                                )),
                        )
                        .child(
                            Button::new("Dismiss")
                                .variant(ButtonVariant::Ghost)
                                .on_press(|s: &mut State| s.sync.success = None)
                                .build(k)
                                .key("dismiss-pairing-success"),
                        ),
                ),
        );
    }
    if let Some(error) = s.sync.file_error.as_ref().or(s.sync.error.as_ref()) {
        panel = panel.child(
            col()
                .key("sync-error")
                .w_full()
                .px(16.0)
                .py(14.0)
                .radius(8.0)
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
    let mut left = overview(s, k).key("sync-overview");
    let mut right = col().key("sync-connection").w_full().min_w(0.0).gap(20.0);
    if !s.sync.requests.is_empty() {
        right = right.child(requests(s, k));
    }
    right = right.child(nearby(s, k));
    if s.sync.enabled() {
        right = right.child(invitation(s, k)).child(trusted(s, k));
    }
    if s.viewport.0 >= 880.0 {
        left = left.w(320.0);
        right = right.w(0.0).grow(1.0);
        panel.child(
            row()
                .w_full()
                .min_w(0.0)
                .gap(24.0)
                .items_start()
                .child(left)
                .child(right),
        )
    } else {
        panel.child(col().w_full().min_w(0.0).gap(20.0).child(left).child(right))
    }
}
