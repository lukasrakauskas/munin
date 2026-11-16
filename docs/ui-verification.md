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

Current local verification: formatting, **121 default Rust tests** (plus three host-dependent smoke tests run explicitly), strict Clippy, build and whitespace checks. Tests cover:
- live in-place edits, surrounding byte preservation, CRLF and Unicode;
- nested source emphasis, tables/diagrams, malformed Mermaid fallback;
- undo across blocks and caret restoration, IME, document selection/clipboard, block and document navigation, forward deletion and new paragraphs;
- fuzzy file/action lookup, action execution, arrows/Enter, mouse choice, Escape/outside dismissal, focus restoration, empty/disabled results, cached file snapshots and narrow bounds;
- formatting shortcuts, current-note search, line deletion, recent-path renames, conflict-safe file opening;
- prior autosave, recovery, filesystem safety, Unicode clipboard/undo, full-height/width and native-inset regressions.

Red→green observations include missing live rendering, missing command palette/shortcuts, source separator height, undo caret restoration, document-end navigation, forward deletion at a block boundary and stale recent paths after folder rename. Author self-review only; no subagents or independent reviewer were used.

Current screenshots under `docs/images/` are actual application renders: live light/dark, active paragraph/table source surrounded by rendered content, palette light/dark, file switcher, narrow layout, context menu, welcome, hidden sidebar and long-note scrolling. Rendering used Mesa software Vulkan on Linux. Hex was cloned at `/home/rake/dev/hex` for design reference; no Hex source was vendored.

## Device sync verification

`src/sync/store.rs` owns the Automerge workspace document, atomic document/manifest persistence, baseline branches for disk/editor edits, safety limits, note identity/tombstones, collision copies and the workspace lock. `src/sync/network.rs` runs iroh and framed Automerge sync off the UI thread, authenticates explicit endpoints before sending data, consumes ten-minute invitations once, bounds sessions and reconnects. `src/sync/mod.rs` bridges these to active buffers and workspace lifecycle. Idle scans do not rewrite metadata.

Additional tests cover:
- offline concurrent Unicode/CRLF edits, text convergence, rename/deletion and edit/delete races;
- filename collisions, durable reload, pending/partially applied materialization after restart;
- unsafe/foreign-platform paths, symlink parents and replacements, private metadata exclusion and writer locking;
- unscanned disk edits, dirty-buffer merging, failed-write retry without duplicated text, retention of incoming updates across metadata errors;
- expiring/single-use invitations, unknown/revoked endpoint rejection, real local iroh sessions and incremental bidirectional exchanges;
- visibility/removal of a device whose pairing was admitted but whose document failed validation;
- pause/re-enable, identity and trust persistence, workspace isolation, reselecting the current workspace, pausing despite filesystem errors, and fail-closed device removal when config writes fail;
- settings button/shortcut/Escape, palette navigation, narrow layout and no hidden note edits.

Actual Mesa-rendered captures: [dark enabled settings](images/sync-settings-dark.png), [pairing/devices](images/sync-pairing-dark.png), and [430px light settings](images/sync-settings-narrow-light.png). Captures use a disposable workspace and public device key. Any pictured invitation was invalidated when its disposable process closed; no private device key is pictured.

Verification is local on Linux. Isolated transport tests use two real iroh endpoints with direct loopback and external relays disabled; the full-worker smoke below additionally exercises the production endpoint preset and local multicast discovery. Public-internet/NAT/relay interoperability and physical macOS/Windows devices were **not** exercised; no claim of native cross-platform or WAN testing is made. Author self-review only, per the no-subagents request. Explicit trust/topology, removal semantics and recovery limits are documented in README.

## Invitation selection and code-copy controls

Long invitation text now lives inside a fixed-height scroll viewport with an explicit selection highlight. Copy/Revoke are adjacent outside that viewport. A regression uses a 6,407-character synthetic invitation at 430px and 1000px widths, selects the whole code, verifies its visible bounds, copies the entire secret through the actual memory clipboard, and revokes it without modifying the note. Native secret clipboard expiry is retained.

Markdown code frames have separate language/Copy headers. Clipboard tests cover multiple fenced blocks, Unicode, tabs, spaces, nested quotes and indented code. Copy does not activate a live block, alter the document or mark it dirty. The indented-code regression exposed a parser boundary starting after the first line's indentation; live block partitioning now retains that indentation with its code block.

Actual captures: [clipped invitation selection with visible actions](images/sync-invitation-selection-light.png) and [Markdown copy headers](images/markdown-copy-dark.png). The pictured invitation was generated in a disposable process and invalidated when that process closed; it is not an active grant. No private device key is pictured.

The existing Windows CI failure in `idle_sync_does_not_rewrite_durable_history` was traced to `File::open` requesting only read access before `set_times`. Its fixture now opens with write access, required by Windows; the assertion remains intact. Local Linux verification does not substitute for the Windows CI rerun.

## Desktop tabs, nearby pairing and rune-fx

`src/settings.rs` owns a keyboard-accessible native TabStrip (General / Sync & devices / Privacy); `src/settings/sync.rs` owns the desktop two-column sync workbench and narrow fallback. Tests switch tabs and appearance without editing the note, require aligned desktop columns and compact setting actions, exercise nearby confirmation controls and retain the invitation selection/copy regressions. Updated captures: [desktop Sync](images/sync-settings-dark.png), [General](images/settings-general-light.png), [Privacy](images/settings-privacy-dark.png), and [narrow fallback](images/sync-settings-narrow-light.png).

`src/sync/discovery.rs` advertises only the friendly name, public endpoint identity and direct addresses on a Munin-specific mDNS service. It is opt-in, non-persistent, stops at 600 seconds, and never broadcasts a capability or grants trust. Discovered routes can update existing trust entries without replacing their verified names. Candidate sets are bounded; expired/stopped candidates and stale confirmation events are removed. Multicast availability is not assumed in default CI tests.

`src/sync/pairing.rs` uses a separate metadata-only QUIC ALPN. Fresh endpoint-bound nonce commitments are exchanged **before** nonce revelation, preventing adaptive fingerprint selection. The human comparison fingerprint binds both authenticated identities and both session nonces; approval handles use the full 256-bit session ID. Both devices must accept before either trust grant, with bounded requests, rejection/stop cancellation and 90-second timeouts. Approving a new nearby peer also revokes an unused invitation if the UI hides it; a hidden capability never remains redeemable. Tests prove no premature trust or note exchange, symmetric verification, binding/tamper/replay rejection, decline, and cancellation even after one approval.

`src/settings/effects.rs` draws a finite rune-fx neon glow and sparkle burst around the first-success banner. New grants/failed transfers do not celebrate; the first successful exchange does, once per newly authorized peer. Globally unique success IDs keep celebrations distinct across workspace switches. The compositor uses Rune's animation clock, stops requesting frames after 1.8 seconds, handles device/format changes, and omits motion effects under reduced motion while retaining the message. [Actual GPU effect capture](images/pairing-success-rune-fx.png) uses a controlled success-state fixture; it is not evidence of a physical remote-device session.

In addition to the **121 default tests**, all **three optional smoke tests passed explicitly** on this Linux host:

```sh
cargo test -- --ignored --test-threads=1
```

They exercise (1) actual GPU drawing versus an unmodified UI baseline, reduced-motion and post-effect idle frames, (2) two real endpoints discovering each other over multicast, and (3) the full application worker flow: discovery, connect, matching fingerprints, bilateral acceptance and bidirectional Markdown sync **without generating or exchanging an invitation code**. Physical multi-device LANs, restrictive router/firewall configurations and native macOS/Windows operation still require verification. Author self-review only; no subagents used.

## Raven logo and theme presets

The supplied `assets/munin-logo.svg` is kept unchanged and rendered in the sidebar and welcome state. A light backing preserves the dark artwork in either appearance. `assets/munin-icon.png` is a 512px raster with that backing for Rune’s native macOS/Windows app-icon API; Linux has no runtime app-icon support in the pinned Rune version. Dock/taskbar integration has not been manually exercised.

The General tab now offers twelve appearances, including nine terminal/GitHub-inspired presets. Its radio group uses Rune’s roving keyboard focus and a two-column narrow / three-column desktop layout. Regression tests exercise every choice at 430px and 1200px, arrow/Enter navigation, command/sidebar cycling, preference round trips, invalid-ID fallback, system-versus-fixed appearance, yellow GitHub selection, and text/selection contrast. Existing numeric preference IDs 0–2 keep their original meanings.

Current macOS verification: **129 tests passed, three optional sync/GPU smoke tests ignored**; `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo build` passed. Actual headless GPU renders were inspected for all nine added palettes, plus desktop/narrow theme pickers. Captures use disposable notes and isolated preferences: [GitHub Light](images/theme-github-light.png), [GitHub Dark](images/theme-github-dark.png), [desktop picker](images/theme-picker-light.png), [narrow picker](images/theme-picker-narrow.png). Contrast assertions composite selection using Rune’s default sRGB blend mode. Syntax highlighting and status colors retain RuneKit’s light/dark defaults. Author self-review only; no subagents, theme editor, or new dependencies.

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
