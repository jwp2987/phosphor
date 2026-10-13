# Fuzzing the terminal model

Two complementary ways to throw hostile pty output at the terminal:

| | what | where | needs |
|---|---|---|---|
| randomized harness | structured generator (CSI/OSC/DCS/APC, shell hooks, images, Unicode, resizes, lifecycle hooks, find) with invariant checks | `app/src/terminal/model/fuzz_tests.rs` (`#[ignore]`d tests) | the normal toolchain |
| libFuzzer targets | coverage-guided mutation | `app/fuzz/` (`terminal_model`), `crates/warp_terminal/fuzz/` (`image_protocols`) | nightly + `cargo-fuzz` |

Both drive the model through `terminal::model::fuzz_api`, which exists only in
test builds and with the opt-in `fuzzing` cargo feature on the `warp` crate.

Every distinct crash, hang or invariant violation is a defect: file an issue
before fixing it (AGENTS.md §5.10/§5.11), and turn the minimized input into a
regular unit test with the fix.

**Safety.** Run fuzzers as an unprivileged user in a scratch directory. Pty
output can name files: kitty graphics with `t=f`/`t=t` makes the app read
the path in the payload (and, for `t=t`, delete it when it is a
`tty-graphics-protocol` file under `/tmp`, `/var/tmp` or `/dev/shm`), so a
mutated input can touch the filesystem of the account that runs it.

## Randomized harness

```sh
cargo nextest run --profile fuzz -p warp --lib --features gui --run-ignored only \
  -E 'test(/terminal_fuzz/)'
```

**Always pass `--profile fuzz`.** The default nextest profile
(`.config/nextest.toml`) terminates any test after 60 s, which kills the
harness at its own default budget and any longer run outright. The `fuzz`
profile reports a slow run every 5 minutes and never terminates it; the
harness enforces its own per-case limits (`PHOSPHOR_FUZZ_SLOW_MS`,
`PHOSPHOR_FUZZ_HANG_SECS`). nextest has no `--slow-timeout` command-line
flag -- timeouts live only in profiles, so change the profile, not the
command line.

Environment (all optional):

| variable | default | meaning |
|---|---|---|
| `PHOSPHOR_FUZZ_SEED` | time-based, printed | master seed |
| `PHOSPHOR_FUZZ_ITERS` | 2000 | cases to run; `0` = unlimited |
| `PHOSPHOR_FUZZ_SECS` | 60 | wall-clock budget; the run stops early when reached |
| `PHOSPHOR_FUZZ_MAX_LEN` | 65536 | max pty bytes per case |
| `PHOSPHOR_FUZZ_CASE` | unset | run only this case index of `PHOSPHOR_FUZZ_SEED` |
| `PHOSPHOR_FUZZ_OUT` | `$CARGO_TARGET_DIR/fuzz-cases`, else `<repo>/target/fuzz-cases` | where failing cases are written |
| `PHOSPHOR_FUZZ_SLOW_MS` | 2000 | a case slower than this fails as "slow" |
| `PHOSPHOR_FUZZ_HANG_SECS` | 60 | a case still running after this fails as "hang" and ends the run |
| `PHOSPHOR_FUZZ_HUGE_IMAGES` | 0 | `1` allows huge image dimensions (can allocate GBs) |
| `PHOSPHOR_FUZZ_IN_FLIGHT` | 1 | `0` skips writing each case before it runs |
| `PHOSPHOR_FUZZ_BACKTRACE` | 0 | `1` captures a backtrace for every panic |

A multi-hour run on the build box:

```sh
PHOSPHOR_FUZZ_ITERS=0 PHOSPHOR_FUZZ_SECS=14400 \
  cargo nextest run --profile fuzz -p warp --lib --features gui --run-ignored only \
  -E 'test(/terminal_fuzz/)' --no-capture
```

At the end the test fails listing every failing `(seed, case)`. To reproduce
one:

```sh
# regenerate it from the seed
PHOSPHOR_FUZZ_SEED=<seed> PHOSPHOR_FUZZ_CASE=<case> cargo nextest run --profile fuzz -p warp --lib \
  --features gui --run-ignored only -E 'test(/terminal_fuzz$/)' --no-capture

# or replay the dump (<seed>-<case>.bin holds the pty bytes, .ops the op list)
PHOSPHOR_FUZZ_REPLAY=target/fuzz-cases/<seed>-<case>.ops cargo nextest run --profile fuzz -p warp --lib \
  --features gui --run-ignored only -E 'test(/terminal_fuzz_replay/)' --no-capture
```

A case that aborted the process (stack overflow, OOM, a panic during unwind)
is left behind as `in-flight-<seed>.{bin,ops}`; replay it the same way. A bare
`.bin` with no `.ops` next to it is replayed as a single pty read, so any
captured pty output can be replayed too.

## libFuzzer targets

One-time setup on the fuzzing box:

```sh
rustup toolchain install nightly
cargo install cargo-fuzz
# Give each fuzz workspace the main build's dependency versions. Re-copy
# after pulling a change to the root Cargo.lock or [patch] table.
cp Cargo.lock app/fuzz/Cargo.lock
cp Cargo.lock crates/warp_terminal/fuzz/Cargo.lock
```

**Each fuzz directory is its own workspace, and cargo applies `[patch]` only
from the root of the workspace being built.** The root `Cargo.toml`'s
`[patch.crates-io]` (tink-rust fork, `lib/rust-genai`, core-foundation, ...)
is therefore copied into `app/fuzz/Cargo.toml` and
`crates/warp_terminal/fuzz/Cargo.toml`, and `script/check_fuzz_patches` (part
of `script/precheck`) fails when a copy drifts. Without the copy the build
takes crates.io `tink-core`, whose `TinkError` is `Send` but not `Sync`, and
fails in `warp_managed_secrets` (`Tink(#[from] tink_core::TinkError)` /
``anyhow::Error: From<EnvelopeError>``), plus E0425/E0432/E0433 wherever the
app uses `lib/rust-genai`'s additions to `genai`. Those errors look like a
`--cfg fuzzing` problem; they are not -- nothing in this tree, and nothing in
tink, reads `cfg(fuzzing)`.

If a dependency ever does break under `--cfg fuzzing` (some crates add
modules or dependencies under it: `h2`, `quinn-proto`, `rav1e`, `tokio`,
`zune-jpeg`), check whether your `cargo fuzz build --help` lists
`--no-cfg-fuzzing` and pass it to `build`/`run`. The targets here do not rely
on `cfg(fuzzing)`.

**Use a separate `CARGO_TARGET_DIR` for fuzz builds.** They compile with
nightly, sanitizer and coverage `RUSTFLAGS`; sharing the main target directory
would throw away its cache in both directions.

```sh
export CARGO_TARGET_DIR=$HOME/phosphor-fuzz-target
```

### `terminal_model` (app)

It depends on the whole `warp` crate, not just `warp_terminal`, because what
it drives -- `TerminalModel`, the ANSI `Processor` and the block list -- lives
in `app/src/terminal/model`; `warp_terminal` holds only the escape-sequence
tables, grid primitives and image-payload parsers (fuzzed on their own by
`image_protocols` below).

From `app/`:

```sh
cargo +nightly fuzz run terminal_model -- -max_total_time=3600 -rss_limit_mb=4096 -max_len=65536
```

Recommended additions:

```sh
cargo +nightly fuzz run -a terminal_model -- \
  -max_total_time=3600 -rss_limit_mb=4096 -max_len=65536 \
  -dict="$PWD/fuzz/terminal_model.dict"
```

* `-a` (`--debug-assertions`) turns on `debug_assert!` and the codebase's
  `safe_assert!`, which are the model's own tripwires. cargo-fuzz builds
  without them by default.
* `-dict` seeds escape-sequence and shell-hook tokens.
* `-s none` disables AddressSanitizer: much faster, and panics, hangs and
  invariant violations are still caught (ASan only adds value for `unsafe`
  code and FFI).
* `--features gui` (passed to `cargo fuzz`) if the `warp` lib needs the GUI
  feature set to build on the box.
* Parallel: `-- -fork=N` (or `-jobs=N -workers=N`). Budget memory as
  `N x -rss_limit_mb`; the build box has 32 GB.

The input layout (flags byte, initial size, then op records such as "feed N
bytes", "resize", "precmd", "find") is documented on
`fuzz_api::run_fuzz_input`.

### `image_protocols` (warp_terminal)

Kitty graphics chunk parsing/base64/zlib/RGB sizing and iTerm image metadata,
without the rest of the app. From `crates/warp_terminal/`:

```sh
cargo +nightly fuzz run -a image_protocols -- -max_total_time=600 -rss_limit_mb=2048
```

It is built without `local_fs`, so kitty's file and shared-memory media are
rejected rather than opened.

### Crashes: reproduce, minimize, replay

libFuzzer writes the failing input to `fuzz/artifacts/<target>/crash-<sha1>`
(also `timeout-*`, `oom-*`, `slow-unit-*`).

```sh
# reproduce (from app/)
cargo +nightly fuzz run -a terminal_model fuzz/artifacts/terminal_model/crash-<sha1>

# minimize the input
cargo +nightly fuzz tmin -a terminal_model fuzz/artifacts/terminal_model/crash-<sha1>

# minimize the corpus after a long run
cargo +nightly fuzz cmin terminal_model
```

A `terminal_model` artifact also replays in an ordinary test build, with full
debug assertions and the stable toolchain, through the harness's replay test
(any `PHOSPHOR_FUZZ_REPLAY` path that is not a `.bin`/`.ops` dump is decoded as
a libFuzzer input):

```sh
PHOSPHOR_FUZZ_REPLAY=$PWD/app/fuzz/artifacts/terminal_model/crash-<sha1> \
  cargo nextest run --profile fuzz -p warp --lib --features gui --run-ignored only \
  -E 'test(/terminal_fuzz_replay/)' --no-capture
```
