# UI verification

## Current architecture

Munin uses the pinned Rune/RuneKit stack with native Markdown/Mermaid rendering. There is one editor viewport with continuous light/dark background and aligned inner gutters. No separate Write/Read toolbar remains.

`src/live.rs` partitions the source into top-level block ranges using pulldown-cmark. Inactive blocks render normally; the active block edits its exact source in place. Separator newlines stay in a byte-preserving suffix rather than expanding the native input's visual height. Changes patch the canonical `State.edit` with a grouped undo step. Source is never reserialized. A pre-edit binding read captures the correct caret for undo; post-edit reads do not overwrite that snapshot. CRLF, Unicode, tables, fences and IME are covered by tests.

Whole-document selection uses the same viewport with a native source input, not another pane. Escape returns to live rendering. This provides native document-wide copy/cut/paste and selection without pretending the block renderer is CodeMirror. Active blocks expose their Markdown, including table and diagram source; this is block-level live preview, not complete Obsidian feature parity.

`src/commands.rs` is the shared action registry and shortcut dispatcher. `src/launcher.rs` uses RuneKit Dialog/TextInput and styled listbox primitives. RuneKit's Palette module contains theme colors, not a command palette; its Combobox was inspected too, but its anchored select interaction is not the centered file/action launcher needed here. Files are snapshotted when opening the launcher; queries do not rescan the filesystem. Recents are session-local and update on renames.

Bindings follow Obsidian's official [Editing shortcuts](https://github.com/obsidianmd/obsidian-help/blob/master/en/Editing%20and%20formatting/Editing%20shortcuts.md), [Command palette](https://github.com/obsidianmd/obsidian-help/blob/master/en/Plugins/Command%20palette.md), and [Quick switcher](https://github.com/obsidianmd/obsidian-help/blob/master/en/Plugins/Quick%20switcher.md) docs: Mod+O files, Mod+P commands, Mod+B/I formatting and native edit/history keys. Mod+K deliberately opens the combined launcher per the user's request, replacing Obsidian's link binding. Sidebar toggling moved off bold to Mod+Backslash. Only implemented actions are advertised.

## Safety

Context-menu targets are separate from the active buffer. Entry operations retain canonical containment, symlink/traversal/collision refusals and active-file conflict protection. Folder deletion is nonrecursive and refuses nonempty directories, including hidden non-Markdown children. Palette open failures retain the dirty buffer and show the error without dismissing the palette. Query typing does not change the note. Remote images are not fetched automatically; external link actions accept HTTP(S)/mailto only. Unsupported Mermaid remains source.

macOS clearance follows runtime native-control insets. Tests simulate a 78px leading inset / 40px band with the sidebar shown/hidden, fullscreen transitions, and trailing caption clearance. These are layout simulations, **not native AppKit verification**. Native macOS/Windows builds were not run on this Linux host.

## Checks and captures

Current local verification: formatting, all **76 Rust tests**, strict Clippy, build and whitespace checks. Tests cover:
- live in-place edits, surrounding byte preservation, CRLF and Unicode;
- nested source emphasis, tables/diagrams, malformed Mermaid fallback;
- undo across blocks and caret restoration, IME, document selection/clipboard, block and document navigation, forward deletion and new paragraphs;
- fuzzy file/action lookup, action execution, arrows/Enter, mouse choice, Escape/outside dismissal, focus restoration, empty/disabled results, cached file snapshots and narrow bounds;
- formatting shortcuts, current-note search, line deletion, recent-path renames, conflict-safe file opening;
- prior autosave, recovery, filesystem safety, Unicode clipboard/undo, full-height/width and native-inset regressions.

Red→green observations include missing live rendering, missing command palette/shortcuts, source separator height, undo caret restoration, document-end navigation, forward deletion at a block boundary and stale recent paths after folder rename. Author self-review only; no subagents or independent reviewer were used.

Current screenshots under `docs/images/` are actual application renders: live light/dark, active paragraph/table source surrounded by rendered content, palette light/dark, file switcher, narrow layout, context menu, welcome, hidden sidebar and long-note scrolling. Rendering used Mesa software Vulkan on Linux. Hex was cloned at `/home/rake/dev/hex` for design reference; no Hex source was vendored.

## Reproduce

```sh
cargo build
# Place state.json under /tmp/munin-ui-check/munin, for example:
# {"workspace":"/absolute/path/to/notes","file":"rendered-note.md","theme":0}
XDG_CONFIG_HOME=/tmp/munin-ui-check target/debug/munin \
  --snap /tmp/munin.png --tree /tmp/munin.json \
  --theme light --size 1200x900 --scale 1
```

Use [`examples/rendered-note.md`](examples/rendered-note.md) as a fixture. To open the palette in a Linux headless script:

```json
{"events":[{"t":0.1,"type":"key","name":"k","cmd":true,"ctrl":true},{"t":0.2,"type":"type","text":"note"}]}
```

Pass `--script palette.json --at 1.2`. For live block activation, tap its `live-block-<source-byte-start>` key; identify keys in `--tree`. A wheel event with `key: "editor-scroll"` and negative `dy` scrolls the document. Optional software GPU override: `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json` (distribution dependent).

This host has the XKB runtime but not its development linker symlink. Checks use a temporary `/tmp/munin-native-libs/libxkbcommon.so` symlink to the installed `libxkbcommon.so.0`, with `LIBRARY_PATH=/tmp/munin-native-libs`. No system files were changed.
