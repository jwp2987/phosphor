# Dev-only test tooling

Everything here is dev-only: nothing is compiled into, or shipped in, a normal or release build.
None of it has been run on the machine that added it (no cargo on the laptop); the first build box
run regenerates `Cargo.lock`.

## shuttle and loom (interleaving tests)

`warpui_core::sync_shim` re-exports `parking_lot` and `std` atomics in every normal build and
swaps in shuttle's or loom's primitives under a cfg. `clipboard.rs`, `key_facts.rs` and
`typeahead_handoff.rs` import their locks from it. A model build compiles `model_*` tests
instead of the modules' ordinary tests (their `static`s need `const` constructors).

```
# random schedules (broad, fast)
RUSTFLAGS="--cfg shuttle" cargo test -p warpui_core --lib --release -- model_
RUSTFLAGS="--cfg shuttle" cargo test -p warp --lib --features warp/gui --release -- model_
# every schedule (tiny bodies only)
RUSTFLAGS="--cfg loom" cargo test -p warpui_core --lib --release -- model_
RUSTFLAGS="--cfg loom" cargo test -p warp --lib --features warp/gui --release -- model_
```

Use a separate `CARGO_TARGET_DIR` (the cfg changes every crate's build). Model builds do not
model time: `Condvar::wait_until` waits for a notification and never times out.

## insta (snapshot tests)

Reviewed snapshots instead of hand-maintained golden strings:

| snapshot | test | what it pins |
|---|---|---|
| `bootstrap_setup_lines_{bash,zsh,fish,nu,pwsh}` | `app/src/terminal/bootstrap_snapshot_tests.rs` | the subshell setup lines per shell, unmarked and with `OSC 7771` markers |
| `pane_key_target_table`, `pane_key_typed_route_scenarios`, `pane_key_after_pty_attempt_table` | `app/src/terminal/pane_key_snapshot_tests.rs` | the pane key destination tables and the order the routing rows ask their probes |
| `input_order_canonical_sequences` | `crates/warpui_core/src/input_order_snapshot_tests.rs` | the input-order trace lines of a typed key, ^C, Enter and an IME commit (times redacted) |

The `.snap` files sit in `snapshots/` next to each test file. Review a change with
`cargo insta review` (needs `cargo install cargo-insta` on the machine that runs it) or accept
with `INSTA_UPDATE=always cargo test ...`. CI (`CI=true`) never writes: a mismatch fails.

## criterion (benchmarks)

`app/benches/` (feature `dev-support`, which exposes `warp::dev_support`):

```
cargo bench -p warp --features dev-support --bench key_path_bench
cargo bench -p warp --features dev-support --bench ansi_bench
cargo bench -p warp --features dev-support --bench key_path_bench -- --save-baseline before
cargo bench -p warp --features dev-support --bench key_path_bench -- --baseline before
```

| bench | measures |
|---|---|
| `pane_queue/idle_key` | one key through an idle pane queue |
| `pane_queue/gate_open_offer_drain` | 64 keys held behind one open gate, then drained by its event |
| `destination_for` | the destination table, 15 (kind, facts) pairs per iteration |
| `handoff/*` | `young_submit_decision`, `KeyFactsCell::load`, `PaneHandoff::read` |
| `ansi/mixed_1mb_in_4k_reads` | 1 MB mixed VT stream into a real `TerminalModel` |

Budget notes are in each file's header. Owner budget: key-to-paint Phosphor overhead <= 2 ms,
Enter <= 2 ms, UI work <= 4 ms per event; the pure parts here should each be microseconds. HTML
reports land in `target/criterion/`.
