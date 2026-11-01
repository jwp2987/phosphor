# Changes in the Phosphor fork of arboard

- Upstream: arboard 3.6.1, crates.io (https://github.com/1Password/arboard), checksum
  `0348a1c054491f4bfe6ab86a7b6ab1e44e45d899005de92f58b3df180b36ddaf` in Cargo.lock before vendoring.
- License: `MIT OR Apache-2.0` (LICENSE-MIT.txt / LICENSE-APACHE.txt are the upstream files, unmodified).
- Vendored via `[patch.crates-io] arboard = { path = "lib/arboard" }` in the root Cargo.toml.
  Examples and the dev-dependency were dropped from Cargo.toml; `[workspace]` added so the crate
  stays out of the parent workspace.

## Source changes

`src/platform/linux/x11.rs` (selection read, #1255):

- `LONG_TIMEOUT_DUR` 4 s -> 10 s (time for the owner to answer the first request).
- `SHORT_TIMEOUT_DUR` 10 ms -> 5 s, and it is now applied as `now + SHORT` on every INCR chunk
  (and when INCR starts) rather than `+= 10 ms` on the start deadline. It is a stall detector, so a
  read that is still receiving chunks never times out. Upstream's 10 ms made large pastes from busy
  owners fail with `ContentNotAvailable`.

Re-pin: diff `src/` against the new upstream release and re-apply the two constants and the
`timeout_end = Instant::now() + SHORT_TIMEOUT_DUR` line.

`src/platform/linux/x11.rs` (first-answer cap, #1192):

- New `set_first_answer_deadline(Option<Instant>)` (thread-local, re-exported from the crate root on
  Linux). While set, a read's wait for the owner's FIRST answer ends at that deadline instead of
  `LONG_TIMEOUT_DUR`; a transfer that has started keeps its stall window. Phosphor sets it for the
  html/image probes that follow a successful text read, so an owner that ignores a probe cannot
  hold the UI thread for 10 s.
- Re-pin: re-apply the thread-local, `first_answer_deadline`, the `timeout_end` line in
  `read_single`, and the re-export in `platform/linux/mod.rs` and `lib.rs`.

`src/platform/linux/x11.rs` (exit handoff, #1330):

- New `hand_over_to_clipboard_manager()` (re-exported like `set_first_answer_deadline`): runs the
  existing bounded (100 ms) `SAVE_TARGETS` handover on the process-global clipboard without
  dropping it. Upstream hands over only when the last `Clipboard` handle drops, which an app that
  keeps handles for its whole life never reaches.
- Re-pin: re-apply the function and the two re-exports.
