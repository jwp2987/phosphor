# Phosphor's vendored copy of vte

- Source: https://github.com/warpdotdev/vte at `4b399c87b63ba88f45709edaa6383fc519f6c900` -- copied in 2026-11-03 so Phosphor controls every git
  dependency it builds. The source files are byte-identical to that rev.
- What it is: fork of alacritty/vte 0.13.0 (workspace: vte, utf8parse, vte_generate_state_changes); Warp commit 4b399c8 "Copied APC support logic" adds APC sequences to `Perform` (used by app/src/terminal/model/ansi/mod.rs `apc_data`).
- License: Apache-2.0 OR MIT (LICENSE-APACHE, LICENSE-MIT).

## Changes made when vendoring

- Kept as its own workspace. Dropped: .builds/.
- Not formatted with Phosphor's rustfmt config (lib/ is excluded from script/check_rustfmt).

## Source changes

None yet. Record every change to this copy here, with the issue number.
