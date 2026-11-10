# Munin MVP design

## Intent

Build a fast, distraction-free local Markdown editor. Markdown files on disk are the source of truth. The first tested platform is Linux; keep the implementation suitable for Windows and macOS.

## Scope

A single window has a collapsible workspace tree and one centered, plain-text writing surface. Users can open a folder, expand folders, select Markdown files, edit and automatically save them, create files and folders, rename and delete files, and navigate with mouse and keyboard. Quick-open searches workspace Markdown files. Remember the last workspace, file, and theme preference in a small platform-appropriate local settings file. Support system, light, and dark themes, with system as the default.

No backend, database, account, cloud, plugins, graph, backlinks, tabs, split panes, live preview, WYSIWYG, or complex settings screen.

## Approaches

1. **Selected: RuneKit `TextArea` and Rune `TextEdit`.** Reuse Rune's editing, selection, undo, clipboard and IME facilities, plus syntax decorations if supported. Follow RuneKit's tree and shell examples for navigation and layout. This offers the smallest implementation and best fit with the request.
2. **Custom editing surface on Rune primitives.** More control, but it duplicates complex input behavior and increases maintenance and correctness risks. Rejected.

Use RuneKit components for the tree, menus, text input, dialogs, and quick-open where suitable. Do not reproduce component behavior unnecessarily. Use a small cross-platform native folder picker dependency only if Rune does not provide one.

## Application design

- Pin Rune and RuneKit to the same tested `dev` revision. Follow `examples/hello`, RuneKit's `tree`, `shell`, and editor examples. Use `AppSpec`, app state, and a view function rather than a new framework.
- Keep state limited to the workspace, tree expansion and selection, current `TextEdit`, save status, sidebar, theme, and transient dialogs. Separate filesystem operations, sidebar construction, editor, and shortcuts only as needed.
- Show folders and `.md` files in the tree. Select a file to open it. Provide simple context actions or equivalent RuneKit controls for creation, rename, and deletion; ask for confirmation before deleting. New note names receive `.md` when needed. Avoid following directory symlinks recursively.
- Keep the writing pane editable plain Markdown, with comfortable typography, a constrained centered column, natural scrolling, and subtle syntax decorations if available without a custom editing engine. No formatting toolbar or preview.
- Debounce automatic saves. Save pending edits before switching files, changing workspace, closing the current file, or exiting when the framework permits; do not discard the buffer on a failed save. Surface errors and keep unsaved edits available for retry. Explicit save writes immediately. Avoid silently overwriting a file known to have changed externally; report the conflict and retain the buffer.
- Restore the last workspace and file only if they remain accessible. Invalid or missing paths must not prevent startup. Store preferences separately from notes; never use a database.
- Use platform-primary shortcuts: Mod+O open folder, Mod+N new note, Mod+S save, Mod+B toggle sidebar, Mod+P quick-open, Mod+W close file. Preserve normal editor shortcuts and composition behavior.
- Provide light and dark palettes using RuneKit theme tokens. Follow the system unless the user explicitly selects light or dark. Expose the choice through a small menu, not a settings screen.

## Verification

Use unit tests for filesystem operations, path handling, safe saves, restoration and state transitions. Use Rune's headless runtime for interaction and layout checks where practical. Test save failure and file-switch paths, Unicode editing, keyboard navigation and both themes. Run a Linux build and tests locally, inspect a headless render, and attempt a native Linux run where the environment permits. Keep dependencies cross-platform; do not claim Windows or macOS runtime verification without testing there.

## Implementation order

Start with a runnable Rune window and RuneKit layout, then workspace selection, tree, editing, saving, file operations, shortcuts, themes and polish. Keep the application runnable at each stage. If a Rune capability or platform limitation prevents a requirement, report it rather than substituting a custom editor or claiming unverified support.
