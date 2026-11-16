//! Sidebar context menus. Keep the clicked target separate from the active editing buffer.
use crate::{
    choose_folder, report, start_dialog,
    state::{Context, State, path_id},
};
use rune::{El, PointerPress, el};
use rune_kit::{FileNode, Kit, MenuItem, TreeItem, TreeRow, context_menu, root};
use std::path::PathBuf;

pub fn open(s: &mut State, point: PointerPress, target: Option<PathBuf>) {
    s.context = Some(Context { point, target });
}

pub fn tree_item(node: &FileNode<()>, row: &TreeRow<'_>) -> TreeItem {
    let target = PathBuf::from(&node.path);
    // TreeItem exposes trailing content, not row handlers. A transparent, absolute secondary
    // surface covers the entire row; primary clicks still bubble to RuneKit's tree handler.
    TreeItem::file(node, row).trailing(
        el().key("entry-context-hit")
            .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
            .on_secondary_press(move |s: &mut State, point, _| {
                open(s, point, Some(target.clone()))
            }),
    )
}

pub fn start_action(s: &mut State, mode: &'static str, target: Option<PathBuf>) {
    s.context = None;
    start_dialog(s, mode);
    if mode == "Rename entry" {
        let name = target
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        s.input = rune::TextEdit::new(name);
        s.input.anchor = 0;
        s.input.focused = true;
    }
    s.dialog_target = target;
}

pub fn menu(s: &State, k: &Kit, body: El) -> El {
    let context = s.context.as_ref();
    let target = context.and_then(|c| c.target.clone());
    let directory = target
        .as_ref()
        .is_some_and(|p| s.files.get(&path_id(p)).is_some_and(|n| n.dir));
    let folder = target.as_ref().map(|p| {
        if directory {
            p.clone()
        } else {
            p.parent()
                .unwrap_or_else(|| std::path::Path::new(""))
                .into()
        }
    });
    let mut menu = context_menu("sidebar-context", body, context.map(|c| c.point))
        .on_close(|s: &mut State| s.context = None);
    if let Some(path) = target {
        let open_path = path.clone();
        menu = menu
            .group_label(if directory {
                "Folder actions"
            } else {
                "Note actions"
            })
            .item(
                MenuItem::new(if directory {
                    "Expand / collapse"
                } else {
                    "Open note"
                })
                .on_select(move |s: &mut State| {
                    s.context = None;
                    if directory {
                        s.files.toggle(&path_id(&open_path));
                    } else {
                        let r = s.open_file(&open_path);
                        report(s, r);
                    }
                }),
            );
        let rename = path.clone();
        let delete = path.clone();
        menu = menu
            .item(MenuItem::new("Rename…").on_select(move |s: &mut State| {
                start_action(s, "Rename entry", Some(rename.clone()))
            }))
            .item(
                MenuItem::new("Delete…")
                    .danger(true)
                    .on_select(move |s: &mut State| {
                        start_action(s, "Delete entry", Some(delete.clone()))
                    }),
            )
            .separator()
            .item(
                MenuItem::new("Copy relative path").on_select_tx(move |s: &mut State, tx| {
                    tx.copy(path_id(&path));
                    s.context = None;
                }),
            )
            .separator();
    }
    let new_folder = folder.clone();
    menu = menu
        .item(
            MenuItem::new("New note…")
                .disabled(s.workspace.is_none())
                .on_select(move |s: &mut State| start_action(s, "New note", folder.clone())),
        )
        .item(
            MenuItem::new("New folder…")
                .disabled(s.workspace.is_none())
                .on_select(move |s: &mut State| start_action(s, "New folder", new_folder.clone())),
        )
        .separator()
        .item(
            MenuItem::new("Refresh workspace")
                .disabled(s.workspace.is_none())
                .on_select(|s: &mut State| {
                    s.context = None;
                    let r = s.refresh();
                    report(s, r);
                }),
        )
        .item(
            MenuItem::new("Open workspace…").on_select_tx(|s: &mut State, tx| {
                s.context = None;
                choose_folder(s, tx);
            }),
        );
    let mut overlays = Vec::new();
    let base = menu.build(k, &mut overlays).w_full().h_full();
    root(base, overlays)
}
