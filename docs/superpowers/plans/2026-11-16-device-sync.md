# Device Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Accountless encrypted Markdown sync with offline merging and polished settings.
**Architecture:** UI-owned persistent workspace CRDT/materialization bridge; background iroh worker exchanges Automerge sync messages and reports results. Explicit per-device authorization and expiring single-use invitations.
**Tech Stack:** Rust, Rune/RuneKit, automerge 0.12, iroh 1.4, Tokio.
**Spec:** `docs/superpowers/specs/2026-11-16-device-sync-design.md`

## Global Constraints
- No questions or subagents; work in this checkout.
- Existing Markdown persistence/conflict protection remains intact when sync is disabled.
- No accounts; opt-in per workspace; only approved endpoints see documents.
- Metadata and network frames are bounded, paths validated and writes atomic.

## Review Focus
- Dirty buffers at remote arrival must merge rather than overwrite (Task 3).
- Symlink parents and foreign-platform path syntax must never escape the workspace (Task 1).
- Unavailable peers must remain retryable without freezing UI (Task 2).
- Restart with pending materialization must not turn missing files into deletions (Task 1).
- Narrow settings must keep controls visible and Escape must restore focus (Task 3).

### Task 1: Durable workspace CRDT
Files: create `src/sync/store.rs`, `src/sync/tests.rs`; modify Cargo dependencies and workspace scanners.
Interfaces: `Store::open(root)`, `Store::scan()`, `Store::merge(bytes)`, `Store::materialize()`, `Store::snapshot()`; each returns an I/O result where fallible. Notes expose relative path/text. The store owns file locking and durable document/manifest state.
- [x] Add failing tests for offline concurrent Unicode edits, persistence, collisions, rename/delete, edit/delete and path/symlink rejection.
- [x] Run tests and verify RED.
- [x] Implement note records with Automerge text, atomic persistence and safe file bridge.
- [x] Run tests and verify GREEN, including baseline suite.

### Task 2: Authorized iroh transport
Files: create `src/sync/network.rs`, `src/sync/mod.rs`.
Interfaces: worker handle commands for snapshot, invite, pair and stop; events for incoming document, invitation, peer authorization, successful exchange and errors. Config persists identity, enable state and approved endpoint addresses.
- [x] Add failing tests for invitation parsing/expiry/replay, authorization and real loopback sync.
- [x] Run tests and verify RED.
- [x] Implement bounded framed Automerge sync protocol, background runtime, cancellation and retry.
- [x] Run tests and verify GREEN.

### Task 3: Application/settings integration
Files: create `src/settings.rs`, `src/sync_ui_tests.rs`; modify `src/main.rs`, `src/state.rs`, `src/commands.rs`, `src/ui.rs`.
Interfaces: workspace-local `Sync` model, settings open/close, poll bridge; active buffer merged through its existing CRDT baseline before incoming changes. Existing save behavior unchanged when disabled.
- [x] Add failing tests for unsaved-buffer merge, workspace switch, enable/pause persistence, settings/palette/keyboard navigation and narrow layout.
- [x] Run tests and verify RED.
- [x] Implement bridge and styled full-pane scrollable settings.
- [x] Run tests and verify GREEN.

### Task 4: Documentation and verification
Files: update `README.md`, `docs/ui-verification.md`.
- [x] Document pairing, privacy, topology, removal semantics, limits and recovery.
- [x] Review diff against spec; fix important findings with regression tests.
- [x] Run `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo build`.

## Execution notes
Baseline: 76/76 tests pass with `LIBRARY_PATH=/tmp/munin-native-libs` (local symlink to installed libxkbcommon.so.0; development linker name absent on host). No repository/toolchain change needed. Native execution and self-review are required by the user's no-subagents instruction. No worktree creation or approval prompts.

Final verification: 101/101 tests; fmt, strict Clippy, build and diff whitespace checks pass. Actual dark/light/narrow/pairing captures are under `docs/images/sync-*.png`. Work is retained in this checkout. Following initial uncommitted delivery, the user requested a pull request; changes are being committed to `feat/accountless-device-sync` for review against `main`.

Review fixes verified by regressions: identical Automerge false puts optimized away during edit/delete races; unsafe tracked symlinks generating false tombstones; stale materialization overwriting unscanned edits; idle metadata rewrites; invisible grants after failed pairing exchanges; duplicated buffer insertions on failed materialization retries; lost pending updates after metadata write failures; inability to pause on filesystem errors; palette actions hidden behind settings; same-workspace reselection losing the writer lock; failed device-removal persistence leaving hidden network authorization. No subagents were used. WAN/NAT and physical macOS/Windows behavior remain unverified.
