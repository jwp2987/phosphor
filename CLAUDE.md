AGENTS.md

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
  docs/DESIGN-ZAP-FORK.md

AGENTS.md above is the code map (now English — the codebase was converted off
its original Simplified-Chinese comment convention). Read
docs/DESIGN-ZAP-FORK.md for WHY this fork is shaped the way it is (BYOP
direction, Warp OSS sync strategy, the TUI port's isolate-don't-refactor rules,
upstream convergence, and the AGPL guardrail) before making architectural
changes.
-->
