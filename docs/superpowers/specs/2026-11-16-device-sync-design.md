# Accountless device sync

## Intent and constraints
Sync the current Markdown workspace across trusted devices without accounts. Preserve ordinary `.md` files and offline editing. Use Automerge text CRDTs and iroh authenticated encrypted QUIC. Work directly in this checkout, without questions or subagents, as requested.

## Approaches
- File copying over iroh is simple but loses concurrent edits: rejected.
- Hosted Automerge services require server/account administration: rejected.
- Chosen: local Automerge workspace document, exchanged using its incremental sync protocol over iroh. Local files are a materialized view; the app bridges external edits and active buffers into the CRDT before applying remote changes.

## Design
Opt-in per workspace. Private `.munin-sync` metadata holds a durable device identity, approved device addresses, CRDT document and last materialization manifest. Exclude metadata from the browser and sync. Atomic owner-only metadata writes; one writer lock per workspace. Each note has a stable random identity, path, text and tombstone. Same-path creations receive deterministic conflict filenames; concurrent edit/delete keeps the edited text. Rename is detected by matching removed/added contents (otherwise delete/create). Only Markdown syncs, not attachments or empty folders. External file edits are ingested before remote materialization; reject symlinks and unsafe/cross-platform traversal paths. Remote deletions affect tracked files only.

Pairing: settings generates a high-entropy single-use invitation valid for ten minutes, containing the host endpoint address and capability. The receiving device pastes it into settings. Verify the capability before disclosing data; subsequent sessions authenticate by persisted iroh endpoint identity. Explicitly approved devices only. Removing a device closes connections and removes local authorization; it does not erase its existing copies or revoke trust granted by other devices. Pair each new device with the desired peers; no automatic transitive trust.

Networking runs off the UI thread, reconnects periodically and exchanges bounded Automerge sync messages. UI polls bridge updates and preserves unsaved edits. Disabling stops networking while retaining files/history; re-enabling catches up. Workspace switches stop the old worker. Show errors, last successful exchange and per-device connection state; never equate a started endpoint with completed sync.

## Settings
A full-pane, scrollable settings view with back button, appearance controls, workspace name, privacy/status card, enable/pause action, invitation creation/copy/revoke, invitation input, and trusted-device rows with remove actions. Accessible from a persistent settings button, palette action and Ctrl/Cmd+comma. Supports light/dark and narrow windows. Escape closes without editing the note.

## Non-goals and limits
No central backup, account recovery, mobile app, attachment sync, background daemon while Munin is closed, QR scanner or rich-text collaboration cursors. Both devices must run Munin at overlapping times. Default iroh public relays/discovery may carry encrypted traffic and observe connection metadata; this is not a promise of zero infrastructure. CRDT history and metadata are not encrypted at rest. Pairing codes are secrets. Limits and platform caveats are documented.

## Verification
Test offline convergence, Unicode/CRLF, reload, rename/delete, colliding creations, edit/delete races, invalid paths/symlinks, unsaved-buffer merges, authorization, expiration, replay, device removal, pause/restart, real local iroh exchange and settings interactions/layout. Run full tests, fmt, clippy and build.
