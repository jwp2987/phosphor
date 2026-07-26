AGENTS.md

<!--
This project is Phosphor — a BYOP terminal, forked from Warp via Zap/OpenWarp
and evolving independently. The Zap -> Phosphor change is display/brand only:
the app id (dev.zap.Zap), on-disk paths, keyring service, and binary names
(zap-oss) are intentionally unchanged, so internal "zap" identifiers are
expected. See specs/phosphor-rebrand/SCOPE.md for the layered plan.

Fork-specific design decisions and direction (English):
  docs/DESIGN-ZAP-FORK.md

AGENTS.md above is the code map (now English — the codebase was converted off
its original Simplified-Chinese comment convention). Read
docs/DESIGN-ZAP-FORK.md for WHY this fork is shaped the way it is (BYOP
direction, Warp OSS sync strategy, the TUI port's isolate-don't-refactor rules,
upstream convergence, and the AGPL guardrail) before making architectural
changes.
-->
