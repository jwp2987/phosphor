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
