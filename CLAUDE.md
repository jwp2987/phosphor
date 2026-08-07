AGENTS.md

# Read these before doing parity work

Skipping any of these means re-deriving something already measured, usually
wrongly. Each exists because a wrong answer cost real time.

| file | why you must read it |
|---|---|
| **`ORACLE.md`** | **The oracle is PINNED.** Compare against Warp `2026.07.29.09.05` stable = `02b53fcd8`, **never `warp/master`** — master is unreleased trunk moving 50-80 tests/day, so measuring against it produces a gap that never shrinks. Also states the re-pin policy (always the latest *stable*). |
| **`SCOPE-AI.md`**, **`SCOPE-TERMINAL.md`**, **`SCOPE-REST.md`** | Per-file verdicts for all 854 test-bearing files at the pin, with quoted source imports as evidence. The workload is **1,605** tests of real debt — not the 2,239 net gap, and not the discredited "468 missing files". **Caveat: verdict A is overstated** — MIXED files collapse to their majority bucket, so trace each test's real API dependencies before porting. |
| **`docs/FLEET-ROUND.md`** | How to run a parallel round. Agents do **not** run the full suite; `rustfmt --check` is their gate and the coordinator batches one integration run. Per-agent full-suite runs made the 3-slot build queue the bottleneck. |
| **`HANDOFF.md`** | Current state of `main`, open decisions, and the operational lessons — the cwd trap, exit-status masking, disk exhaustion, "capture before you stop". |
| **`TODO.md`** | The parity ledger. **Verify any entry before acting on it** — four entries have stated the opposite of the code (#148). |
| **`AGENTS.md` §5.6/§5.10/§5.11** | Never weaken a test to go green; fix the code. Every defect gets an issue first. |

Two CI guards enforce what the compiler cannot: `script/check_cloud_boundary`
(no new imports of dropped-cloud modules) and `script/check_stub_coverage` (no
tests against gutted no-op stubs). Read their header comments before changing
them — earlier versions were wrong in instructive ways.

<!--
This project is Phosphor — a BYOP terminal, forked from Warp via Zap/OpenWarp
and evolving independently. The Zap -> Phosphor change is display/brand only:
the app id (dev.zap.Zap), on-disk paths, keyring service, and binary names
(zap-oss) are intentionally unchanged, so internal "zap" identifiers are
expected. See specs/phosphor-rebrand/SCOPE.md for the layered plan.

DEV-ENVIRONMENT NOTE — command-signatures stub (address later):
  This working copy may contain a local, gitignored stub at
  `crates/command-signatures-v2/js/build/.placeholder.json` (a `{}` file). It
  lets the app build WITHOUT Node/yarn: `command-signatures-v2`'s build.rs only
  panics when `js/build/` is missing, so when the stub makes that dir exist it
  instead prints "Proceeding with stale command signatures!" and continues.
  Consequence: smart per-command argument completions are EMPTY; everything else
  (terminal, BYOP/Vertex, themes) works. It is local-only (gitignored) and does
  NOT ship — anyone cloning still needs Node to build.
  To get a REAL build: install Node 18.14.1+, run `corepack enable`, then
  `rm -rf crates/command-signatures-v2/js/build` so the next build runs
  `yarn build` for real.

Fork-specific design decisions and direction (English):
  docs/DESIGN-PHOSPHOR-FORK.md

AGENTS.md above is the code map (now English — the codebase was converted off
its original Simplified-Chinese comment convention). Read
docs/DESIGN-PHOSPHOR-FORK.md for WHY this fork is shaped the way it is (BYOP
direction, Warp OSS sync strategy, the TUI port's isolate-don't-refactor rules,
upstream convergence, and the AGPL guardrail) before making architectural
changes.
-->
