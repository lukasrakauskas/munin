# Live Editor and Command Palette Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans inline, no subagents.

**Goal:** One-view live Markdown, Obsidian-aligned shortcuts and a Cmd/Ctrl+K file/action palette.
**Architecture:** Keep canonical State.edit and patch byte ranges from a transient active-block editor. Render inactive blocks with the existing native Markdown/Mermaid renderer. Build a modal launcher from RuneKit primitives and shared command dispatch.
**Tech Stack:** Existing Rust/Rune/RuneKit/pulldown-cmark; no new dependencies.
**Spec:** `docs/superpowers/specs/2026-11-16-live-editor-design.md`

## Global Constraints
- No questions/subagents; preserve exact Markdown, file safety and native chrome.
- No separate Write/Read toggle. Mod+K combined launcher, Mod+P commands, Mod+O files, Mod+B/I formatting, Mod+Backslash sidebar.
- One final push only after all local work.

## Review Focus
- Edits/undo spanning different blocks must not replace another block or lose whitespace/Unicode.
- IME, paste and selection need one canonical source; source fallback must preserve history.
- Boundary arrows/deletion and whole-document selection must remain usable.
- Palette file errors/conflicts must preserve dirty buffers; query typing cannot modify notes.
- Small windows, long paths, empty results and changing native-control insets must fit.

### Task 1: Live preview editor
**Files:** new `src/live.rs`; State/main/ui/Markdown and tests.
**Interfaces:** ActiveBlock { range, edit }; block partitions; activate, commit, blur, history; live view consumes canonical State.edit.
- [ ] Add failing block-partition and Runtime tests for rendering and in-place editing with unchanged surrounding bytes.
- [ ] Implement byte-range block editing, canonical history, local IME projection, boundary navigation/deletion, full-source selection fallback and remove split-mode controls/state.
- [ ] Adapt old mode-specific tests to the new contract without dropping persistence/clipboard/layout coverage; run full `cargo test` and commit.

### Task 2: Commands and palette
**Files:** new `src/commands.rs`, `src/launcher.rs`; State/main/ui/tests.
**Interfaces:** shared command registry/dispatch; launcher state with cached file snapshot, query, active result and scope; file/action results.
- [ ] Add failing tests for Mod+K/O/P, fuzzy ranking, arrows/Enter/Escape, focus return, disabled actions, formatting, current-note search, line deletion and conflict-safe opens.
- [ ] Build responsive RuneKit dialog/search/listbox, recents, file/action scopes, keyboard/mouse navigation and safe dispatch. Route shortcuts and sidebar search to it; show actual shortcut hints.
- [ ] Run full tests, formatting, strict Clippy and build. Commit.

### Task 3: Visual review and handoff
**Files:** README, verification docs, screenshots and any evidenced fixes.
- [ ] Inspect actual live paragraph/table/Mermaid editing and palette light/dark/narrow captures, including keyboard and file/action navigation.
- [ ] Self-review all focus classes, fix evidenced issues with regression tests. Update docs with actual behavior and shortcut differences.
- [ ] Run fmt/test/clippy/build/diff checks; expected all pass. Commit.

Final remote handoff: push once and update PR #2 only after all local checks and commits.
