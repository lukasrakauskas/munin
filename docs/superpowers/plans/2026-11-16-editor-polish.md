# Editor Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a polished full-pane Markdown writing workspace and open a PR.

**Architecture:** Keep the pinned Rune stack and State. Replace only view composition and form-oriented editor sizing; use native Rune text input inside a single flexible scroll viewport. Preserve state and file operations.

**Tech Stack:** Rust, Rune, RuneKit.

**Spec:** `docs/superpowers/specs/2026-11-16-editor-polish-design.md`

## Global Constraints
- No new dependencies or Rune pin changes; no storage changes or preview mode.
- No questions or subagents; perform self-review and create a PR.
- White light-mode and charcoal dark-mode continuous editor background.
- Header/footer/text share 28px horizontal gutters; editor uses 15px/24px typography.

## Review Focus
- Long unbroken text wraps without horizontal overflow.
- Long notes keep the last line and caret reachable without scrolling chrome.
- Small windows and hidden sidebar leave usable editor space.
- Long workspace/file names truncate instead of squeezing actions away.
- Dialog cancellation returns typing to the note; errors and empty states stay usable.

### Task 1: Full-pane editor and workspace shell
**Files:** Modify `src/main.rs`; create `src/ui.rs` for visual composition and `src/ui_tests.rs` for layout regressions if needed.
**Interfaces:** Consume `State`, `choose_folder`, `start_dialog`, `submit`, `report`, `remember`, `key`; produce `view(&State, &mut Cx<State>) -> El`.

- [x] Add regression tests using Rune Runtime element geometry: a short note in 1440x1000 reaches the pane's full usable width and height; content reaches within 40px of both edges; scrolling long text leaves header/footer fixed; long unbroken text does not overflow; both themes and hidden sidebar fit. These catch the existing max-width, max-height, inner viewport sizing and padding defects.
- [x] Run `cargo test` and confirm baseline passes and new geometry regressions fail for the expected sizing reasons.
- [x] Implement the continuous full-pane editor, theme, sidebar, note header, status footer and empty state. Preserve existing action keys and keyboard handling. Use one scroll viewport with inner padding and a full-width wrapping text input; retain syntax, Unicode and autosave callbacks.
- [x] Add behavioral tests for sidebar toggle, dialog Escape/focus restoration and status/word count changes. Observe failures before adding each behavior.
- [x] Run `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets -- -D warnings` and `cargo build`. Expected: all pass.
- [x] Render actual empty, light, dark, narrow, long-note and scrolled states using Rune headless tooling with isolated XDG config. Inspect screenshots and element trees, correct visual issues, and retain representative screenshots in `docs/images/`.
- [x] Update README with UI behavior and verification evidence, then commit the complete tested deliverable.

### Task 2: Review and PR
**Files:** Review branch diff and docs. No new interfaces.
- [x] Self-review all changes against the spec and five review focus cases; record evidence and limitations.
- [x] Re-run formatting, full tests, clippy and build; expected all pass. Commit any verified fixes and mark plan progress.
Final remote handoff, only after every local commit and check: push feature branch once and `gh pr create` against main, including screenshots and testing notes. Expected: accessible PR URL (reported in the session, not via a second documentation-only push).
