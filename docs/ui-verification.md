# UI verification

## What changed

The old form textarea combined a centered 760px maximum width, 600px maximum height, and a separate auto-grow child. At a 1440×1000 window the old inner text was only 744px wide in a 1204px pane. The new view uses one flexible, bounded scroll viewport, a full-width wrapping native Rune text field, and 28px text gutters inside the scroll content. Short notes accept focus anywhere on the writing surface; long notes keep their caret and final line reachable while the header and footer stay fixed. The light-mode viewport and pane use the same opaque white fill.

Hex was inspected locally at `/home/rake/dev/hex`, revision `5f01839ea8dc328393dab2b34ce5eaf7021e4a80`: its `docs/images/output.png`, `src/rune_ui/sidebar.rs`, and `src/rune_ui/controls.rs` informed navigation hierarchy, restrained separators, and aligned gutters. No Hex source or new dependency was copied into Munin.

## Automated coverage

`cargo test` includes native Rune Runtime layout and input tests for:
- large-window width and lower-surface click-to-focus in both appearances;
- long-token wrapping and viewport/footer alignment at 1440×1000 and 720×520;
- long-note scrolling with fixed chrome and typing at the end with a visible caret;
- visual sidebar toggling, narrow-window actions, long filenames, and live resizing;
- equal file-tree gutters and wrapped welcome-screen copy;
- quick-open result truncation for long paths;
- Escape / Cancel focus restoration and safe cancellation of deletion;
- live word counts and unsaved status, alongside existing Unicode, undo, clipboard, autosave and filesystem safety tests.

The width, Escape, tree-gutter, welcome wrapping and quick-open overflow tests were observed failing before their respective fixes. Additional edge tests characterize the preserved behavior. A boolean-expression simplification in `State::refresh` is behavior-equivalent and removes a pre-existing Rust 1.96 Clippy warning.

## Actual renders

The PNGs under `docs/images/` were rendered by the application, not mockups. Checked light/dark at 1200×800, a 640×520 light workspace, the empty welcome screen, sidebar-hidden dark mode, and the bottom of a long note. Rune's headless renderer used Mesa software Vulkan on Linux. No native macOS/Windows checks were performed.

Build and render a workspace using an isolated configuration (Linux):

```sh
cargo build
# Place a state.json in $XDG_CONFIG_HOME/munin containing:
# {"workspace":"/absolute/path/to/notes","file":"Welcome.md","theme":0}
XDG_CONFIG_HOME=/tmp/munin-ui-check \
  target/debug/munin --snap /tmp/munin.png --tree /tmp/munin.json \
  --theme light --size 1200x800 --scale 1
```

Optional software GPU override: `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json` (path varies by distribution). For a bottom-of-document capture, use `--script scroll.json --at 0.2` with:

```json
{"events":[{"t":0.1,"type":"wheel","key":"editor-scroll","dx":0,"dy":-50000}]}
```

This machine has the XKB runtime library but not its development symlink. Verification used a temporary `/tmp/munin-native-libs/libxkbcommon.so` symlink to the installed `libxkbcommon.so.0` and `LIBRARY_PATH=/tmp/munin-native-libs`; no system files or repository dependencies were changed for that workaround.
