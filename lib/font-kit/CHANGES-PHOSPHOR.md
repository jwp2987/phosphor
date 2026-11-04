# Phosphor's vendored copy of font-kit

- Source: https://github.com/warpdotdev/font-kit at `a04b225ecb639bb850f21d05212cfdc7465000f3` -- copied in 2026-11-03 so Phosphor no longer depends on
  Warp's GitHub. The source files are byte-identical to that rev.
- What it is: fork of servo/font-kit 0.12.0, 56 commits ahead (hashable Properties, matching module, Family::from_handle, emoji/is_colored, thin strokes, DirectWrite rendering, no-panic dwrote calls, core-graphics 0.25).
- License: MIT OR Apache-2.0 (LICENSE-MIT, LICENSE-APACHE).

## Changes made when vendoring

- `[workspace]` added; its `dwrote` git dependency now points at ../dwrote-rs.
- Not formatted with Phosphor's rustfmt config (lib/ is excluded from script/check_rustfmt).

## Source changes

None yet. Record every change to this copy here, with the issue number.
