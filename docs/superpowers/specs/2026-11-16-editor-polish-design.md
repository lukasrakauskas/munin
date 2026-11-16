# Munin writing workspace polish

## Intent
Make the local Markdown workspace feel like a focused desktop notes app, drawing on Hex's quiet navigation and aligned gutters and Obsidian's violet accents. The user delegated design decisions and requested execution without questions or subagents, plus a PR.

## Approaches
- Restyle the existing form textarea: small diff, but retains a form-oriented auto-growing viewport and mismatched inner sizing.
- Full-pane native Rune editor (chosen): retain text editing, syntax decorations and autosave while replacing the capped form shell with one flexible scroll viewport.
- Add a rich preview or replace the framework: unnecessary scope and editing risk.

## Design
Keep Rust, pinned Rune/RuneKit and existing persistence. Use a roughly 248px muted sidebar with brand, workspace name, search, creation actions, a flexible file tree and theme footer. Move note-specific actions to a 56px header, with a truncated path and visible sidebar toggle. Give the editor all remaining width and height, no fixed maximum or centered 760px column. Use one continuous white light-mode / charcoal dark-mode background, a 28px horizontal text gutter shared with the header and footer, and padding inside the scroll content rather than outside a separate input shell. Use 15px/24px editor typography, subtle separators, compact ghost buttons and violet accents. Show a quiet status footer with save state and word count. Supply a useful empty state and bounded dialog/error panels; preserve all keyboard and file safety behavior.

## Constraints and non-goals
No new dependencies, no upstream source copies, no preview mode, no storage changes. Do not change the Rune pin just for styling. No questions or subagents. Clone Hex into `/home/rake/dev/hex` for reference, outside this repo. Keep creation, rename, delete, quick open, theme and sidebar actions accessible. Test short/long notes, narrow/large windows, both themes and hidden sidebar. Capture real headless screenshots; do not claim native platform checks that were not run.
