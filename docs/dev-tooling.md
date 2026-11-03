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
RUSTFLAGS="--cfg shuttle" cargo test -p warpui_core --lib --release -- model_tests::
RUSTFLAGS="--cfg shuttle" cargo test -p warp --lib --features warp/gui --release -- model_tests::
# every schedule (tiny bodies only)
RUSTFLAGS="--cfg loom" cargo test -p warpui_core --lib --release -- model_tests::
RUSTFLAGS="--cfg loom" cargo test -p warp --lib --features warp/gui --release -- model_tests::
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
cargo bench -p warp --features dev-support --bench routing_bench
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
| `routing/*` | input-type routing of 1 MB and 10 MB text (a line, a script) and the Enter-time predicates on 10 MB (#1604). Limits are set AT the Enter's compute allowance (1 ms per 4 KiB), not at 10x: over the limit means an Enter on a paste that size is handed back |

**Regression gate (#1533).** The manual `bench` CI job runs `script/bench_gate.py` on the criterion output and
fails when a benchmark's median exceeds `max_ns` in `script/bench_thresholds.json` (or a listed bench is missing,
e.g. renamed). The limits are the header budgets with generous headroom for the noisy shared box (order-of-magnitude
catches, not baselines). To update: run the `bench` job or `cargo bench ...` locally, run
`python3 script/bench_gate.py target/criterion --suggest` to see medians and a 10x suggestion, then edit
`max_ns` in the same commit as the code change that justifies it (and add an entry when you add a bench).
Never raise a limit just to go green. For a tighter local A/B use `-- --save-baseline before` / `-- --baseline before`.

Budget notes are in each file's header. Owner budget: key-to-paint Phosphor overhead <= 2 ms,
Enter <= 2 ms, UI work <= 4 ms per event; the pure parts here should each be microseconds. HTML
reports land in `target/criterion/`.

## expectrl (real-shell tests)

`app/tests/real_shell_markers.rs` loads the real bash bootstrap into an interactive `bash` in a
pty and asserts the line-editor marker order (Preexec, Precmd, one LineEditorReading with the
Precmd's seq). It is a Rust port of the bash core of `script/test_line_editor_markers`; the other
shells and the continuation / builtin-read / markers-off cases stay in the script. Skips with a
message when `bash` is missing.

```
cargo test -p warp --test real_shell_markers
```

## cargo-fuzz

`fuzz/` is a standalone crate (its own `[workspace]`, excluded from the root workspace; the root
`Cargo.lock` does not cover it). Needs nightly and `cargo install cargo-fuzz`.

| target | feeds |
|---|---|
| `ansi_parser` | arbitrary pty output into a `TerminalModel` through `ansi::Processor`, in 1..64-byte reads |
| `hook_parsers` | payloads inside valid DCS `$d`/`$f`, OSC 9278 `f`/`d`, OSC 7771 and OSC 9277 frames |
| `markdown_parser` | the markdown reader (full parse, raw text, inline, streamed tail == full parse), GFM tables on/off; seeds = `crates/markdown_parser/tests/fixtures` |
| `mermaid_render` | `mermaid_to_svg::render_mermaid_to_svg` on arbitrary text: Ok is an SVG and deterministic, never a panic; seeds = `crates/editor/tests/fixtures/mermaid` |
| `input_queue` | `Vec<Op>` (`arbitrary`) through `warp::dev_support::run_ops`: the proptest model's reference model and invariants |
| `route_decide` | arbitrary `RouteFacts` / `Intent` / pin (read with `Unstructured`) through `table::decide`: never panics, deterministic, the table's properties hold |
| `key_facts_cell` | operation sequences on `KeyFactsCell` / `KeyModelCell` against a reference model (gen kept by a refresh, consistent-read rule) |
| `typeahead_handoff` | event sequences on `TypeaheadHandoff` (hooks, keys, editor commands, reports, session end): no panic, `Held` has a live hold, signals never held, holds that start/end are reported |

```
cd fuzz
cp ../Cargo.lock Cargo.lock        # first time: reuse the app's pinned versions
cargo +nightly fuzz run ansi_parser -- -max_total_time=120
cargo +nightly fuzz run hook_parsers -- -max_total_time=120
cargo +nightly fuzz run input_queue -- -max_total_time=120
cargo +nightly fuzz run route_decide -- -max_total_time=120
cargo +nightly fuzz run key_facts_cell -- -max_total_time=120
cargo +nightly fuzz run typeahead_handoff -- -max_total_time=120
```

Seed corpora are committed in `fuzz/corpus/<target>/`: the `ansi_parser` seeds are the small
recordings under `app/src/terminal/ref_tests/data/` (first byte = read size), the `hook_parsers`
seeds are the hook JSON of `ansi/mod_test.rs` in each frame (first byte = frame), `input_queue`
has a few random byte seeds. A crash lands in `fuzz/artifacts/<target>/`; reproduce with
`cargo +nightly fuzz run <target> fuzz/artifacts/<target>/<file>`.

## cargo-mutants

`.cargo/mutants.toml` (the location cargo-mutants reads) sets nextest, generous timeouts and the
excludes (tests, generated code, platform-only files, vendored crates, logging). The per-package
test args are NOT in the file: for `warp` pass `--cargo-test-arg=-p --cargo-test-arg=warp_tui
--cargo-test-arg=--lib --cargo-test-arg=--features --cargo-test-arg=warp/gui` (`-p warp_tui` alone
does not compile), for `warpui_core` pass `--cargo-test-arg=--features --cargo-test-arg=tui`.
Always narrow the run:

```
A=(--cargo-test-arg=-p --cargo-test-arg=warp_tui --cargo-test-arg=--lib --cargo-test-arg=--features --cargo-test-arg=warp/gui)
cargo mutants -p warp "${A[@]}" --file app/src/terminal/pane_key.rs
cargo mutants -p warp "${A[@]}" --file app/src/terminal/model/key_facts.rs --file app/src/terminal/model/typeahead_handoff.rs
cargo mutants -p warpui_core --cargo-test-arg=--features --cargo-test-arg=tui --file crates/warpui_core/src/clipboard.rs
cargo mutants --list --file app/src/terminal/pane_key.rs                  # preview, runs nothing
```

A surviving mutant is a behavior no test pins: add the test, do not weaken the mutant list.
Results are in `mutants.out/` (`missed.txt`, `caught.txt`).

Note for the model-checker runs: `RUSTFLAGS` in the environment replaces `build.rustflags` of
`.cargo/config.toml` (the v0 symbol mangling and the mac link args); that is fine for tests.

## CI (GitLab, manual jobs only)

All in `.gitlab-ci.yml`, tag `little-thing`, `when: manual`, `allow_failure: true`; none run in
the default pipeline. Start them from the pipeline page of a protected ref (`main`, `build/*`).

| job | runs | artifacts |
|---|---|---|
| `test-shuttle-loom` | the `model_tests::` tests of `warpui_core` and `warp` under `--cfg shuttle`, then `--cfg loom` | none |
| `fuzz-smoke` | each fuzz target for `FUZZ_SECONDS` (180) from the committed corpus; `RUSTC_BOOTSTRAP=1` so the pinned stable toolchain is used | `fuzz/artifacts/` (crashes) |
| `mutants` | cargo-mutants on `MUTANTS_FILES` of `MUTANTS_PACKAGE`, tests limited by `MUTANTS_TEST_FILTER`, optional `MUTANTS_SHARD=k/n` (default scope: pane_key, key_facts, input_queue gates+destination) | `mutants.out/` |
| `mutants-warpui-core` | cargo-mutants on the warpui_core clipboard with warpui_core's own tests | `mutants.out/` |
| `bench` | both criterion benches, then `script/bench_gate.py` (fails on a regression) | `bench-results/criterion/` (HTML reports, estimates) |

`fuzz-smoke` and `mutants` fetch `cargo-fuzz` / `cargo-mutants` with `cargo binstall` into the
runner user's own `CARGO_HOME` when missing (the `licenses` job does the same for `cargo-deny`);
nothing is installed into the maintainer's home or the system.
