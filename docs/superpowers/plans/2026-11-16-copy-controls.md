# Copy Controls Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. No questions or subagents, per the user's ongoing instructions.

**Goal:** Prevent invitation selection from painting over actions; provide obvious invitation and Markdown code-copy buttons.
**Architecture:** Bound invitation text inside its own clipped, selectable scroll viewport, with actions outside it. Use RuneKit's native code-frame extension for a separate copy header without disturbing syntax highlighting or live editing. Copy only code content; copying must not activate/edit the enclosing live block.
**Tech Stack:** Rust, Rune/RuneKit, native clipboard, pulldown-cmark.

## Constraints / design
Keep invitation expiry/revocation and secret clipboard handling intact. Support narrow light/dark settings. Preserve Markdown document bytes and existing diagram rendering. Test real clipboard results and selection/visible bounds, not just selectors. No dependency change.

## Review focus
Long invitations; selection extending past a viewport; multiple/nested blocks; Unicode/tabs/CRLF; buttons accidentally triggering live-block editing; horizontally long code.

## Tasks
- [x] Reproduce invitation overflow with a long synthetic code and add failing layout/selection/clipboard tests in `src/sync_ui_tests.rs`.
- [x] Implement bounded invitation viewport and adjacent Copy/Revoke controls in `src/settings.rs`; verify tests pass.
- [x] Add failing Markdown copy tests in `src/ui_tests.rs` for independent blocks, exact content and no note activation/mutation.
- [x] Add copy headers in `src/markdown.rs` using native rendering hooks; verify tests and review special code fences.
- [x] Run full tests, fmt, strict Clippy, build and diff checks; inspect actual narrow renders; update documentation.

## Findings
Existing invitation code uses `max_h(90)` directly on selectable text, without a clipping ancestor. A Copy invitation button already exists, but overflowing text/selection can obscure it and Revoke. Existing Markdown renderer has no app copy controls; native code-frame hooks are available and preserve the existing renderer.

## Verification / review
104/104 Linux tests pass, as do fmt, strict Clippy, build and whitespace checks. Regression tests were observed failing before implementation: absent clipping viewport, absent copy controls, and indented code rendered as prose after its indentation was assigned to the preceding live block. Clipboard assertions verify complete invitations, separate code snippets, Unicode, tabs and spaces; copy never activates or modifies the note. Actual narrow selection and code-header renders are committed under `docs/images/`.

The PR's existing Windows CI failure was traced from its log to the idle-history fixture calling `set_times` on a read-only file handle. The test now requests write access, as Windows requires. Linux verification passes; a fresh Windows CI run is still required. No production sync behavior was changed by that fixture correction. No subagents or questions.
