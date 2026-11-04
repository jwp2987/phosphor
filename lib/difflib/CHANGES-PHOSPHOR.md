# Phosphor's vendored copy of difflib

- Source: https://github.com/warpdotdev/difflib at `114177e7d8e1f8017baca397f943e11644b14d63` -- copied in 2026-11-03 so Phosphor no longer depends on
  Warp's GitHub. The source files are byte-identical to that rev.
- What it is: fork of DimaKudosh/difflib 0.4.0; Warp commit 114177e "Improve performance" (reusable FxHashMap buffers, `&mut self` find_longest_match, `real_quick_ratio`), which app/src/ai/blocklist/input_model.rs uses.
- License: MIT (LICENSE).

## Changes made when vendoring

- `[workspace]` added.
- Not formatted with Phosphor's rustfmt config (lib/ is excluded from script/check_rustfmt).

## Source changes

None yet. Record every change to this copy here, with the issue number.
