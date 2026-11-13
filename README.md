# Munin

Munin is a local Markdown editor built with Rust, Rune, and RuneKit. Notes are plain `.md` files in a folder you choose. Munin does not require an account, server, or network connection to edit notes.

## Run

Install a recent Rust toolchain, then run:

```sh
cargo run --release
```

The first build downloads the pinned Rune revision and Rust dependencies. Linux folder selection uses the XDG desktop portal and a compatible file chooser. The application runs offline after it is built.

Open a folder to create or edit notes. The sidebar shows folders and Markdown files; the quick-open dialog searches their paths. New note and folder names can include a relative folder path, such as `projects/ideas`.

## Shortcuts

| Shortcut | Action |
| --- | --- |
| Ctrl/Cmd+O | Open workspace |
| Ctrl/Cmd+N | New note |
| Ctrl/Cmd+S | Save now |
| Ctrl/Cmd+B | Toggle sidebar |
| Ctrl/Cmd+P | Quick open |
| Ctrl/Cmd+W | Close note |

The theme button cycles through system, light, and dark. Munin remembers the last workspace, note, and theme in the user's configuration directory.

Changes save after a short pause and before changing notes. If the file has changed on disk, Munin keeps the edited buffer and reports a conflict rather than knowingly overwriting the external version. Copy your edits elsewhere or restore the original file contents before retrying save. If saving fails when the window closes, Munin attempts to keep a local recovery copy and restores it on the next launch.

## Development

```sh
cargo fmt --check
cargo test
cargo build
```

Rune is a pre-release framework pinned to a Git revision. Linux is the primary development and test platform; Windows and macOS builds and native behavior require verification on those platforms.
