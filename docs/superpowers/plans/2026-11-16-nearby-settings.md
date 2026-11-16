# Nearby Settings Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans, in this session without subagents.

**Goal:** Safe code-free nearby pairing, desktop tabbed settings and a one-shot rune-fx success celebration.
**Architecture:** Separate pairing-only QUIC ALPN + ephemeral approval registry; bounded mDNS discovery window; UI-owned trusted-device persistence and first-sync celebration. Desktop settings shell with focused tab panels; small rune-fx compositor.
**Tech Stack:** Rust, Rune/RuneKit/rune-fx pinned to the existing revision, iroh 1.4, iroh-mdns-address-lookup 0.6, Tokio, blake3.
**Spec:** `docs/superpowers/specs/2026-11-16-nearby-settings-design.md`

## Global constraints
No accounts, automatic LAN trust, broadcast capability tokens, questions or subagents. No document exchange before bilateral consent. Preserve invitation fallback, clipping/copy controls, note buffers, theme and palette behavior. Nearby expires after 600 seconds, requests after 90 seconds; max eight pending requests.

## Review focus
- Reject and timeout must cancel the other dialog without authorizing either endpoint (Task 1).
- Request nonce/endpoint must bind each decision; old approvals cannot accept a later request (Task 1).
- Names/IPs may be advertised only in an explicit Nearby window; metadata is bounded (Task 2).
- First sync, not a partial grant or ordinary autosync, triggers success exactly once (Task 3).
- Wide settings must use two columns; narrow layouts and selectable invitations must remain clipped (Task 3).

### Task 1 — Bilateral pairing
Create `src/sync/pairing.rs`, `src/sync/pairing_tests.rs`; modify network protocol exports as needed.
Interfaces: `Pairing` registry; `Request { id, peer, fingerprint, outgoing, expires }`; `Pairing::decide(id, accept)`; pairing serve/dial functions produce approved Peer events only after both decisions. Authenticated identities and fresh nonces produce the same fingerprint on both sides.
- [x] Write failing registry / actual local QUIC tests for bilateral acceptance, no premature trust, decline and replay/timeout bounds; observe RED.
- [x] Implement metadata-only pairing and cancellation/expiry, then verify GREEN.

### Task 2 — Discovery and application lifecycle
Create `src/sync/discovery.rs`, tests; modify `src/sync/network.rs`, `src/sync/mod.rs`, Cargo files.
Interfaces: network commands to request/decide nearby pairing; events for nearby candidate/expiry, pending/finished request and discovery failure. `Sync::set_nearby`, `Sync::connect_nearby`, `Sync::decide` bridge the UI. Runtime `Nearby(bool)` commands start/stop ephemeral discovery without restarting or interrupting existing sync sessions.
- [x] Add failing tests for publication privacy/filtering, candidate bounds/expiry, discovery stop and persisted trust lifecycle; observe RED.
- [x] Implement opt-in 600s mDNS advertisement/discovery, startup/stop and event handling; verify GREEN and full suite.

### Task 3 — Desktop UI and rune-fx
Refactor `src/settings.rs` into a shell and `src/settings/sync.rs`; create `src/settings/effects.rs`. Modify State initialization, app compositor and settings UI tests.
Interfaces: `Tab` enum; `Sync` nearby/request lists and success sequence; pure effect-layer generator for reduced-motion and lifetime tests; compositor draws rune-fx around `pairing-success`.
- [x] Add failing tests for native tabs, separate content, wide/narrow layouts, no hidden note edits, confirmation controls, first-success effect and reduced motion; observe RED.
- [x] Implement desktop columns, nearby/confirmation flows, preserved invitation controls and bounded one-shot GPU effects; verify GREEN.

### Task 4 — Final verification and PR
- [x] Self-review security/lifecycle and UI; fix important issues with regression tests.
- [x] Run full tests, fmt, clippy, build, optional local multicast test and actual screenshots; document limits.
- [x] Update existing PR with verified changes; do not claim physical-LAN/WAN/native-platform verification without evidence.

## Execution ledger
Baseline: 104/104 tests pass; working tree clean on `feat/accountless-device-sync`. Work stays on this existing PR branch. User opted out of questions/subagents. Effects/MDNS APIs and native TabStrip inspected before implementation.

Task 1: complete — bilateral local QUIC tests, decline, stop-after-one-approval, expired/replayed decisions and nonce commit/reveal binding pass; no pairing-ALPN document events.
Task 2: complete — publication privacy/filtering, bounded/expired candidates, trusted-route refresh without trust/name changes, exact window deadline and full code-free worker flow pass. Nearby toggles through runtime commands rather than worker recreation, preserving existing sync sessions.
Task 3: complete — native tabs, aligned desktop columns, compact setting rows, narrow/copy regressions, explicit confirmation UI, first-success-only celebration, workspace-unique effect IDs and reduced-motion/idle culling verified.
Task 4: complete — 121 default tests pass; all three optional smoke tests also pass explicitly (GPU draw/idle comparison, multicast discovery, complete nearby pairing + bidirectional Markdown sync). fmt, strict Clippy, locked build and whitespace checks pass. Actual desktop/general/privacy/invitation/narrow renders inspected; the effect capture is a controlled GPU fixture. No physical multi-device, WAN/NAT or new native macOS/Windows verification claimed.

Self-review regressions fixed: queued prompts after stopping discovery; reused effect sequence IDs across workspaces; hidden nearby state after failed removal persistence; misaligned empty-request columns; desktop actions stretched like mobile CTAs. Security review added nonce commitments before reveals to prevent adaptive fingerprint selection, and a RED→GREEN regression ensuring a nearby grant revokes any invitation that the UI will hide. Existing invitation and buffer-safety behavior remains covered by the full suite. Work remains on the existing PR branch; no subagents or questions.
