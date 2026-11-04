# Phosphor's vendored copy of core-foundation-rs

- Source: https://github.com/servo/core-foundation-rs at `6f844cf1a1a18e25b70fcdf1bcdc458555bd2eff` -- copied in 2026-11-03 so Phosphor controls every git
  dependency it builds. The source files are byte-identical to that rev.
- What it is: servo upstream at 6f844cf, the merge of servo/core-foundation-rs#746 (the rev main has always used; core-text 21.1.0, which warpui requires -- the later tip moved core-text to 22 and the patch stopped applying) (fixes a double-retain leak in `CTFontCollection::get_descriptors`), not yet in a crates.io release; applied through [patch.crates-io] for core-foundation, core-foundation-sys, core-graphics, core-text.
- License: MIT OR Apache-2.0 (LICENSE-MIT, LICENSE-APACHE, COPYRIGHT).

## Changes made when vendoring

- Kept as its own workspace. Dropped: .github, .travis.yml. Drop this copy once a crates.io release contains #746.
- Not formatted with Phosphor's rustfmt config (lib/ is excluded from script/check_rustfmt).

## Source changes

None yet. Record every change to this copy here, with the issue number.
