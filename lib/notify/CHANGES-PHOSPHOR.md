# Phosphor's vendored copy of notify

- Source: https://github.com/warpdotdev/notify at `91b719849bc04ef251bcb2cc61076b099204e970` -- copied in 2026-11-03 so Phosphor controls every git
  dependency it builds. The source files are byte-identical to that rev.
- What it is: fork of notify-rs/notify (workspace: notify, notify-types, notify-debouncer-full/-mini, file-id); 11 Warp commits add `watch_filtered` / `WatchFilter` (91b7198) and exit on missing paths.
- License: notify: CC0-1.0 (notify/LICENSE-CC0); the other crates MIT OR Apache-2.0.

## Changes made when vendoring

- Kept as its own workspace. Dropped: .github, .cirrus.yml, rust-toolchain.toml.
- Not formatted with Phosphor's rustfmt config (lib/ is excluded from script/check_rustfmt).

## Source changes

None yet. Record every change to this copy here, with the issue number.
