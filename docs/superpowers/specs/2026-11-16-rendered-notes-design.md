# Rendered notes and native workspace interactions

The user requests sidebar/entry context menus, Obsidian-like Markdown typography (including headings and tables), Mermaid rendering, and macOS traffic-light clearance. Prior instructions remain: no questions or subagents; finish all local work before one push to the existing open PR.

## Approach
Use the existing pinned Rune stack. Considered a new webview/live rich-document editor (large storage/cursor risk), syntax colors only (cannot render tables/diagrams), and lossless styled source plus native Reading view (chosen). Write mode retains exact Markdown with visual emphasis and heading hierarchy; Read mode renders real Markdown tables, lists, quotes, code and Mermaid. Ctrl/Cmd+E and a header button switch modes without saving or transforming the buffer. This is not a hidden-marker live-preview editor.

Enable RuneKit's existing mermaid/markdown features. Share its pulldown-cmark 0.13 parser directly for source-offset typography; do not use regexes for nested Markdown. Mermaid renders locally using native shapes, with source fallback for unsupported diagrams. No network images, JavaScript or webview. Both modes use the full-pane surface/gutters.

Use RuneKit context-menu overlays, attached to sidebar background and each full tree row without replacing tree keyboard/scroll behavior. Background: new note/folder, refresh, open workspace. Entry: open note or expand/collapse folder, new siblings/children, rename, confirm delete, copy relative path. Right-click must not open a different note, discard dirty edits or act on the active note instead of the clicked target. Creation targets the clicked folder (or a file's parent). Folder deletion is empty-only; never recursive. Renaming a folder containing the active note updates its relative path. Reuse canonical path containment, collision and conflict protections; reject symlinks, traversal and names that relocate the entry unexpectedly.

Read `Cx::titlebar_inset` and `chrome_insets` for macOS clearance. Reserve a draggable top strip over the sidebar only when native controls overlay it; with sidebar hidden, inset the pane header instead. React to fullscreen/inset changes without hardcoding Linux padding. Keep controls clickable and preserve pane sizing.

## Verification
TDD for context targeting, nested creation, safe rename/delete, menu dismissal, lossless modes, Markdown styling/structure, Mermaid fallback, and synthetic macOS overlay insets with sidebar shown/hidden and fullscreen. Preserve all existing tests. Capture real light/dark rendered notes, tables/diagrams, context menus and narrow layouts; document that native macOS behavior cannot be run on Linux. Self-review, full formatting/tests/strict Clippy/build, then one push and update PR #2.
