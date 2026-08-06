# Assessment: Pending-edit-batch conflict-discard (issue #11 parity item)

## Summary / Recommendation

**Recommendation: BUILD** the `PendingEditBatch` debounce + conflict-discard for
remote (SSH) buffers — it is **additive** to the fork's already-present remote
buffer-sync architecture, not a re-architecture. **DEFER one sub-part**:
Warp's `handle_buffer_conflict_detected` handler depends on a
`BufferConflictDetected` server→client push that the fork never ported across
the `remote_server` stack (proto + client + manager). Ship the batch/debounce +
push-conflict-discard now; track the `BufferConflictDetected` wiring as a
prerequisite for the conflict-detected handler.

**Classification: SSH-remote file editing — IN SCOPE.** (Maintainer guidance on
#11 2026-08-02: "keep only if it's SSH-remote file editing; drop if cloud
collab.") It is unambiguously SSH-remote: the entire mechanism lives inside
`BufferSource::Remote`, is gated on `FeatureFlag::SshRemoteServer`, and is driven
end-to-end by the `remote_server` daemon (`RemoteServerManager` /
`remote_server::proto`). There is **zero** involvement of `drive` /
`cloud_object` / `warp_files` / collab.

**Effort/risk:** Core (batch + debounce + push-conflict-discard + save-flush + 3
ported tests) ≈ ~1 day, low–moderate risk (contained to
`global_buffer_model.rs` + the one `ContentChanged` subscription + the remote
save path). The deferred `BufferConflictDetected` sub-feature is a separate
multi-crate task (proto message + client event + manager event + 4th test).

---

## Fork design vs Warp design

Both sides share the **same** remote buffer-sync architecture. The fork already
carries `BufferSource::Remote { remote_path, sync_clock }`, `SyncClock`
(server/client version vector), `handle_buffer_updated_push`, the
`RemoteBufferConflict` event, `open_remote_buffer` with the bidirectional
`ContentChanged → BufferEdit` subscription, `resolve_conflict`, and the
server-daemon side (`ServerLocal`, `apply_client_edit`). This item is a
**delta**, not a new subsystem.

The single difference is how client edits are transmitted:

| | Fork (current) | Warp (oracle) |
|---|---|---|
| Edit transmission | **Immediate** — the `ContentChanged` handler builds `TextEdit`s and calls `client.send_buffer_edit(...)` synchronously, one send per edit | **Batched/debounced** — edits accumulate in `PendingEditBatch`; a 200 ms debounce timer (`REMOTE_EDIT_DEBOUNCE`) flushes them as one `BufferEdit` |
| In-flight batch | none (nothing buffered) | `BufferSource::Remote.pending_batch: Option<PendingEditBatch>` |
| Conflict-discard | **N/A** — no batch exists to discard | on conflict, `batch.discard()` cancels the debounce timer and drops unsent edits |
| Save | `save_remote_buffer` just sends `SaveBuffer` | `save()` **flushes** the pending batch first, then `SaveBuffer` |
| Server-initiated conflict push | not wired | `handle_buffer_conflict_detected` (fed by `BufferConflictDetected`) also discards the batch |

Fork evidence (`app/src/code/global_buffer_model.rs`):
- Immediate send in the subscription: `global_buffer_model.rs:1083-1143`
  (`client.send_buffer_edit(...)` at `:1131`).
- Conflict *detection* already present but nothing to discard:
  `handle_buffer_updated_push` conflict branch at `:1267-1275`.
- Remote save with no flush: `save_remote_buffer` at `:1502-1560`.
- `SyncClock` (unchanged between fork and Warp): `app/src/code/buffer_location.rs:118-162`.

Warp evidence (`warp/master:app/src/code/global_buffer_model.rs`):
- `PendingEditBatch` struct + `flush` + `discard` + `REMOTE_EDIT_DEBOUNCE`: `:59-120`.
- `pending_batch` field on `BufferSource::Remote`: `:132-138`.
- Debounced subscription: `:1701-1770` (`push_edit_to_pending_batch` at `:1738`,
  `Timer::after(REMOTE_EDIT_DEBOUNCE)` at `:1747`).
- `push_edit_to_pending_batch` (bumps client_version immediately, creates/extends
  batch, cancels prior timer): `:2377-2414`.
- Save flushes the batch first: `:800-828`.
- Push-conflict discard: `:2354-2358` (inside `handle_buffer_updated_push`),
  plus a "drop stale push" guard at `:2346-2352` the fork lacks.
- `handle_buffer_conflict_detected` discard: `:2229-2253`.

---

## Classification: SSH-remote vs cloud/collab (with evidence)

**SSH-remote file editing.** Evidence, strongest first:

1. **Feature-flag gate.** The whole remote-push subscription (which owns
   `handle_buffer_updated_push` and `handle_buffer_conflict_detected`) is
   registered only under `FeatureFlag::SshRemoteServer.is_enabled()`
   (`warp/master:app/src/code/global_buffer_model.rs:338`). That flag is the
   fork's SSH remote-server feature (`app/src/lib.rs:1695`, `:2457`;
   `app/src/code/file_tree/view.rs:1426`), not a cloud/Drive flag.
2. **Transport is the SSH daemon, not the cloud.** `PendingEditBatch.flush`
   sends `remote_server::proto::TextEdit` via
   `remote_server::client::RemoteServerClient::send_buffer_edit`
   (`warp/master:...:86-105`). `handle_buffer_conflict_detected` is fed by
   `RemoteServerManagerEvent::BufferConflictDetected`
   (`warp/master:...:366-367`), which originates from
   `ClientEvent::BufferConflictDetected` ←
   `server_message::Message::BufferConflictDetected` proto push from the
   `remote_server` daemon
   (`warp/master:crates/remote_server/src/client/mod.rs:631-632`,
   `crates/remote_server/src/manager.rs:522, 3512-3513`).
3. **Test helpers seed a Remote (SSH) buffer.** `seed_remote_buffer_for_test`
   builds a `BufferSource::Remote` from a `RemotePath` = `HostId` +
   `StandardizedPath` (`warp/master:...:2429-2461`); `sync_clock_for_remote_test`
   reads the `Remote` sync clock (`:2466-2472`). No cloud object, Drive doc, or
   collab session appears anywhere.
4. **Log namespace.** Every related log line is prefixed `[remote-buffer]`
   (e.g. `:94, :113, :2360`) — the SSH remote-file namespace.
5. **No cloud imports.** The mechanism touches `remote_server::*` and
   `warp_util::remote_path::RemotePath`; nothing from `drive`, `cloud_object`,
   or `warp_files`.

**Single strongest piece of evidence:** the entire mechanism is gated behind
`FeatureFlag::SshRemoteServer` and driven exclusively by `RemoteServerManager` /
`remote_server::proto` (the SSH remote-server daemon), with zero `drive` /
`cloud_object` / collab involvement.

---

## Port-cost analysis (additive vs re-architecture)

**Additive.** The fork already has `BufferSource::Remote`, `SyncClock`,
`handle_buffer_updated_push`, `RemoteBufferConflict`, and the `ContentChanged`
subscription. Restoring the batch touches one enum variant and one subscription;
it does **not** re-architect the working buffer model.

Key structural insight: **conflict-discard exists only because of batching.**
The thing discarded *is* the in-flight `PendingEditBatch`. The fork sends every
edit synchronously, so there is no batch to discard — "restore conflict-discard"
is therefore inseparable from "restore the debounce/batch design," and adopting
the batch brings discard along for free.

Work items (core, additive):
1. Add `PendingEditBatch` struct + `flush`/`discard` + `REMOTE_EDIT_DEBOUNCE`
   const (mirror `warp/master:...:59-120`).
2. Add `pending_batch: Option<PendingEditBatch>` to `BufferSource::Remote`
   (fork `global_buffer_model.rs:52-56`).
3. Add `push_edit_to_pending_batch` (mirror `:2377-2414`).
4. Convert the `ContentChanged` subscription (fork `:1083-1143`) from
   immediate `send_buffer_edit` to accumulate-then-debounce (mirror
   `:1701-1770`).
5. Discard the pending batch in the `handle_buffer_updated_push` conflict
   branch (fork `:1267-1275`; mirror `:2354-2358`). Optionally also port the
   "drop stale push" guard (`:2346-2352`).
6. **Flush on save** — update `save_remote_buffer` (fork `:1502-1560`) to flush
   the pending batch before `SaveBuffer`, mirroring Warp `save()` `:825-828`.
   *Without this, edits typed inside the 200 ms window are lost on save* — this
   is the one real correctness pitfall of the port.

### What breaks / §5.10 considerations
- The observable change is that rapid keystrokes coalesce into one `BufferEdit`
  (Warp behavior) instead of one send per keystroke (current fork). Both keep
  content in sync; Warp's is the coalescing/perf optimization and the canonical
  behavior. Under §5.10 the fork's immediate-send is a mild divergence from Warp
  worth closing.
- The only regression *risk introduced by the port* is item 6 (lost trailing
  edits if save doesn't flush) and timer lifetime on tab-close — both are
  handled by faithfully mirroring Warp's `flush`/`discard`/`close` paths rather
  than simplifying them.

### Prerequisite for the deferred sub-part
`handle_buffer_conflict_detected` (and its test) require a `BufferConflictDetected`
server→client push that the fork does **not** have anywhere in the
`remote_server` stack:
- Fork proto has only `BufferUpdatedPush`
  (`crates/remote_server/proto/remote_server.proto:60, 392`); no
  `BufferConflictDetected` message.
- Fork `ClientEvent` has only `BufferUpdated`
  (`crates/remote_server/src/client/mod.rs:80, 747`).
- Fork `RemoteServerManagerEvent` has only `BufferUpdated`
  (`crates/remote_server/src/manager.rs:320, 1270-1276`); no
  `BufferConflictDetected` variant.

Porting that push (proto field + client decode + manager forward) is a separate,
larger cross-crate task. Recommendation: ship items 1–6 now; file a tracking
issue for the `BufferConflictDetected` push + its handler.

---

## Oracle tests to port

From `warp/master:app/src/code/global_buffer_model_tests.rs` (323 lines; the
fork has **no** `global_buffer_model_tests.rs` today — only
`buffer_location_tests.rs`, which covers the `ServerLocal` daemon side). Plus
the two test helpers on `GlobalBufferModel` (`#[cfg(test)]`):
`seed_remote_buffer_for_test` (`:2429`) and `sync_clock_for_remote_test`
(`:2466`), and the file-local helpers `has_pending_batch_for_test` (`:25`),
`pending_batch_edit_count_for_test` (`:32`), `insert_pending_batch_for_test`
(`:47`).

| Test | Lines | Covers | Portable now? |
|---|---|---|---|
| `pending_batch_discarded_on_server_push_with_conflict` | `:127-190` | conflict push → batch discarded | **Yes** (core) |
| `server_push_accepted_without_pending_batch` | `:238-282` | non-conflict push accepted | **Yes** (core) |
| `pending_batch_bumps_client_version_immediately` | `:284-323` | `client_version` bumped on append (conflict detection sees true C) | **Yes** (core) |
| `pending_batch_discarded_on_conflict_detected` | `:192-236` | `handle_buffer_conflict_detected` → batch discarded | **Deferred** — needs `BufferConflictDetected` push first |

Per §5.10/§5.6, port these verbatim against the local/SSH path (not thinner
substitutes); the first three go green with the core build, the fourth with the
deferred sub-feature.
