# Phosphor's vendored copy of command-signatures

- Source: https://github.com/warpdotdev/command-signatures at `48fd7aa5b7b4d99f5db57d1c5a25c47c05567fcf` -- copied in 2026-11-03 so Phosphor no longer depends on
  Warp's GitHub. The source files are byte-identical to that rev.
- What it is: Warp-original: completion specs (json/) + the warp-command-signatures and warp-completion-metadata crates.
- License: MIT (LICENSE.md).

## Changes made when vendoring

- Kept as its own two-member workspace (root Cargo.toml unchanged). Dropped: .github, .agents, node/prettier files, script/, rust-toolchain.toml.
- Not formatted with Phosphor's rustfmt config (lib/ is excluded from script/check_rustfmt).

## Source changes

None yet. Record every change to this copy here, with the issue number.
