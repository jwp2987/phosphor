# Phosphor's vendored copy of yaml-rust

- Source: https://github.com/warpdotdev/yaml-rust at `0aed385b7f0f1c89b709ea054a90313312ed0afb` -- copied in 2026-11-03 so Phosphor no longer depends on
  Warp's GitHub. The source files are byte-identical to that rev.
- What it is: fork of chyh1990/yaml-rust 0.4.5; Warp commit 5168471 removes the "tab in block-scalar indentation" scanner error, so serde_yaml 0.8 accepts such user YAML (keybindings, markdown front matter, agent config, drive import).
- License: MIT OR Apache-2.0 (LICENSE-MIT, LICENSE-APACHE).

## Changes made when vendoring

- `[workspace]` added. Applied through `[patch.crates-io]` (root and fuzz/).
- Not formatted with Phosphor's rustfmt config (lib/ is excluded from script/check_rustfmt).

## Source changes

None yet. Record every change to this copy here, with the issue number.
