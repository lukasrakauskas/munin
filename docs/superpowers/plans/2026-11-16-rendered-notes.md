# Rendered Notes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement task-by-task. No subagents per user instruction.

**Goal:** Native rendered Markdown/Mermaid, targeted sidebar context menus, and macOS-safe chrome.
**Architecture:** Extend State with transient reading/context/dialog-target state; keep disk Markdown untouched. Add focused modules for Markdown presentation and sidebar menu actions; reuse RuneKit tree and overlays.
**Tech Stack:** Rust, pinned Rune/RuneKit; enable bundled mermaid feature and share pulldown-cmark 0.13.
**Spec:** `docs/superpowers/specs/2026-11-16-rendered-notes-design.md`

## Global Constraints
- No questions or subagents; no webview/network image fetching or Markdown reserialization.
- Folder deletion is empty-only, destructive actions require confirmation, context targets cannot bypass workspace/symlink/conflict guards.
- One final push to the existing open PR, after local verification.

## Review Focus
- Right-clicking an inactive file must not change or lose the active dirty buffer.
- Nested folder rename and creation must update paths without overwriting siblings.
- Unicode/nested emphasis and fenced code must preserve exact source bytes and cursor semantics.
- Unsupported/malformed Mermaid must remain readable, not crash or erase source.
- Native controls must not overlap content when toggling sidebar or entering fullscreen.

### Task 1: Styled source and rendered Markdown/Mermaid
**Files:** `Cargo.toml`, `Cargo.lock`, `src/markdown.rs`, `src/state.rs`, `src/main.rs`, `src/ui.rs`, `src/ui_tests.rs`.
**Interfaces:** `markdown::source_spans(&str, &Kit) -> Vec<Span>`, `markdown::reading(&str, &Kit) -> El`; `State.reading: bool`, `toggle_reading(&mut State)`.
- [ ] Add failing Runtime tests for Reading mode heading/table/diagram structure and mode-toggle buffer preservation; run to observe missing mode.
- [ ] Enable RuneKit mermaid and parser dependency; implement full-width native Reading view and source emphasis/heading/code spans, button and Mod+E.
- [ ] Add/verify source typography tests for Unicode and nested emphasis, literal fenced code, heading sizing, plus malformed diagram fallback and updated unsaved buffer rendering.
- [ ] Run `cargo test`; expected existing + new tests pass. Commit.

### Task 2: Targeted sidebar context menus
**Files:** `src/context.rs`, `src/state.rs`, `src/main.rs`, `src/ui.rs`, `src/ui_tests.rs`.
**Interfaces:** `Context { point: PointerPress, target: Option<PathBuf> }`; `State.context`, `State.dialog_target`; State entry-validation/rename/delete methods. `context::tree_item`, `context::menu`, `context::start_action`.
- [ ] Add failing right-click/target tests and state safety tests (inactive dirty file, nested creation, folder rename, empty-only deletion, traversal/symlink/collision/conflict refusal).
- [ ] Implement menu overlays with keyboard/outside dismissal. Full-row secondary hit surface uses TreeItem trailing absolute element so primary tree behavior remains upstream-owned.
- [ ] Extend dialog submit/confirmation with explicit target path, prefilling rename and creation directories; keep file mutations guarded and nonrecursive.
- [ ] Run `cargo test`; expected all pass. Commit.

### Task 3: macOS-safe chrome and final verification
**Files:** `src/ui.rs`, `src/ui_tests.rs`, `README.md`, `docs/ui-verification.md`, screenshots.
**Interfaces:** sidebar/header consume runtime chrome/insets, no State persistence changes.
- [ ] Add failing Runtime tests with titlebar inset 78 and chrome band 40: brand below traffic lights; hidden-sidebar toggle after lights; fullscreen clears extra inset.
- [ ] Implement dynamic reserved drag strip and hidden-sidebar header inset; retain Windows trailing caption clearance.
- [ ] Inspect actual headless light/dark/narrow Reading views, diagrams, menus, and dialogs; update screenshots/docs with limitations and mode behavior.
- [ ] Run `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo build`, `git diff --check`; expected all pass. Self-review all five focus classes, fix evidenced issues, commit.

Final remote handoff: after all local commits, push once, update existing PR #2 with current screenshots/testing, verify PR URL. Do not push intermediate commits.
