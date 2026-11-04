# Phosphor's vendored copy of dwrote-rs

- Source: https://github.com/warpdotdev/dwrote-rs at `a71ce6c0136f7d0954a9a8b181b5b2d8ace5eb9c` -- copied in 2026-11-03 so Phosphor no longer depends on
  Warp's GitHub. The source files are byte-identical to that rev.
- What it is: fork of servo/dwrote-rs 0.11.5; Warp commit a71ce6c "Add more fallible font methods" (Result instead of panic), used by lib/font-kit on Windows.
- License: MPL-2.0 (Cargo.toml; upstream ships no LICENSE file).

## Changes made when vendoring

- `[workspace]` added.
- Not formatted with Phosphor's rustfmt config (lib/ is excluded from script/check_rustfmt).

## Source changes

None yet. Record every change to this copy here, with the issue number.
