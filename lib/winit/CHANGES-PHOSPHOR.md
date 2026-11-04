# Phosphor's vendored copy of winit

- Source: https://github.com/jwp2987/winit at `05e8c04da47960d8a627b73cf729d38aac91f80d` -- copied in 2026-11-03 so Phosphor controls every git
  dependency it builds. The source files are byte-identical to that rev.
- What it is: our fork of warpdotdev/winit@a4e0ecb5 (itself a rust-windowing/winit fork) plus 9a0788c3 (Windows dark mode via the registry, rust-windowing/winit#4453) and 05e8c04d (x11: no panic in request_user_attention, #654); see the comment above `winit` in the root Cargo.toml.
- License: Apache-2.0 (LICENSE).

## Changes made when vendoring

- Kept as its own workspace (winit + dpi). Dropped: .github and .cargo/ (upstream's local cargo config).
- Not formatted with Phosphor's rustfmt config (lib/ is excluded from script/check_rustfmt).

## Source changes

None yet. Record every change to this copy here, with the issue number.
