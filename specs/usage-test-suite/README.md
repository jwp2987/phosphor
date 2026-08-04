# Usage suite — how to run it

Design doc: [`SCOPE.md`](./SCOPE.md) (read that first for the full rationale).
This is the short, practical how-to.

## What it is

A headless, machine-readable smoke suite that answers "does the running Zap
app actually do the thing" for both the **GUI** (`warp`/`app`, `--features
gui`) and the **TUI** (`zap-tui-oss`, `crates/warp_tui`). It is a thin
orchestrator (`crates/usage_suite`, bin `usage-suite`) over two existing
in-process test harnesses — it does not add a new way of driving the app; see
`SCOPE.md` §1.

## Running it

```
./script/usage-test                   # all default (non-flaky, non-BYOP) scenarios, both surfaces
./script/usage-test --surface gui      # GUI only
./script/usage-test --surface tui      # TUI only
./script/usage-test --only usage_launch_bootstrap,usage_run_command
./script/usage-test --include-flaky    # also run real-shell scenarios (preexec race; auto-retried)
./script/usage-test --include-byop     # also run scenarios needing a real provider (requires key+network)
./script/usage-test --json             # NDJSON only (suppress the human table on stderr)
```

`script/usage-test` holds the shared workspace cargo flock (AGENTS.md §5.8)
for the whole invocation — never invoke `cargo run -p usage_suite` directly
from a script/agent that is itself supposed to be flock-serialized; go
through the wrapper, or hold the same lock yourself first.

You can also run the runner directly (e.g. under your own flock):

```
cargo run -q -p usage_suite -- --surface both
```

## Output format

One NDJSON object per scenario on stdout, then a final `summary` object, then
a human-readable table on stderr (suppressed by `--json`). Exit code is `0`
when `failed == 0`, else `1`. Skipped scenarios never fail the run. See
`SCOPE.md` §3 for the exact shape.

```jsonl
{"surface":"gui","scenario":"usage_launch_bootstrap","status":"pass","duration_ms":1840,"tags":["reliable-here"]}
{"surface":"tui","scenario":"usage_tui_transcript_render","status":"pass","duration_ms":90,"tags":["reliable-here"]}
{"surface":"gui","scenario":"usage_agent_roundtrip","status":"skip","reason":"needs-byop-provider (no --include-byop)","tags":["needs-byop-provider"]}
{"summary":{"total":3,"passed":2,"failed":0,"skipped":1,"surfaces":{"gui":2,"tui":1}}}
```

## The manifest (`crates/usage_suite/src/manifest.rs`)

The single source of truth for scenarios: surface, name, and tags. It has two
clearly-marked regions:

- `GUI_SCENARIOS` — names are `integration` binary scenario names (keys
  registered in `register_tests()`,
  `crates/integration/src/bin/integration.rs`). The runner drives these by
  spawning `cargo run -q -p integration --bin integration -- <name>` per
  scenario (reusing the existing `RERUN_EXIT_CODE` retry loop).
- `TUI_SCENARIOS` — names are `#[test]` functions in `warp_tui`, prefixed
  `usage_tui_`. The runner drives these via `cargo nextest run -p warp_tui -E
  'test(/^usage_tui_/)'` (falling back to `cargo test` if `cargo-nextest`
  isn't installed), discovering the matching test set first so scenarios that
  don't exist yet are reported as `skip` rather than failing the suite.

**This is Chunk A.** The manifest currently ships a **stub**: one existing,
already-registered GUI scenario (`test_open_and_close_settings`, tagged
`stub`) and one placeholder TUI name that doesn't exist yet
(`usage_tui_stub_placeholder`, reported `skip` until Chunk C lands real
tests). That is enough for the runner to be end-to-end runnable today. Chunks
B and C append real rows to their respective regions — see `SCOPE.md` §6 for
the full target catalog (§4.1 GUI, §4.2 TUI) and which files each chunk owns.

## Tags and default-skip rules

- `reliable-here` — in-process, no shell/provider; runs by default.
- `needs-real-shell` — drives a real PTY shell to command completion; skipped
  unless `--include-flaky` (subject to the sandbox's shell-preexec race,
  auto-retried via `RERUN_EXIT_CODE`).
- `needs-byop-provider` — a real agent round-trip; skipped unless
  `--include-byop` (requires a real provider key + network).
- `needs-desktop` — wants a real GPU window/pixel result; always skipped by
  this runner (documented, not exercised here).
- `stub` — Chunk-A placeholder; exercises the runner's plumbing only, no
  behavioral assertion of its own.

## Extending it (Chunks B/C/D/E)

See `SCOPE.md` §6 for the full phased plan and file-ownership table. In
short: B adds `crates/integration/src/test/usage.rs` + registers the new
scenarios in `integration.rs` + appends rows to the manifest's `// GUI
scenarios` region; C adds `crates/warp_tui/src/usage_tests.rs` (all named
`usage_tui_*`) + a `mod` declaration in `warp_tui/src/lib.rs` + appends rows
to the manifest's `// TUI scenarios` region; D adds a local mock AI provider;
E wires an opt-in CI workflow. None of those chunks should need to touch
`crates/usage_suite/src/main.rs` or `report.rs`.
