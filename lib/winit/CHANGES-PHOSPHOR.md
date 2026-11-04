# Phosphor's vendored copy of winit

- Source: https://github.com/jwp2987/winit at `05e8c04da47960d8a627b73cf729d38aac91f80d` -- copied in 2026-11-03 so Phosphor controls every git
  dependency it builds. The source files are byte-identical to that rev.
- What it is: our fork of warpdotdev/winit@a4e0ecb5 (itself a rust-windowing/winit fork) plus 9a0788c3 (Windows dark mode via the registry, rust-windowing/winit#4453) and 05e8c04d (x11: no panic in request_user_attention, #654); see the comment above `winit` in the root Cargo.toml.
- License: Apache-2.0 (LICENSE).

## Changes made when vendoring

- Kept as its own workspace (winit + dpi). Dropped: .github and .cargo/ (upstream's local cargo config).
- Not formatted with Phosphor's rustfmt config (lib/ is excluded from script/check_rustfmt).

## Source changes

Record every change to this copy here, with the issue number.

- #1414 (2026-11-03) `src/platform_impl/linux/wayland/seat/mod.rs` `new_capability`: flush the connection right after
  `wl_seat.get_keyboard`. Building the `KeyboardState` that follows (xkb context + the locale's Compose table) took ~2 ms
  (~13 ms the first time) per keyboard-capability appearance in the CI wire trace, and the bind request sat in the client buffer
  until the end of the dispatch; a key the compositor routed in that window reached no wl_keyboard of ours (the first character
  of each line with a keyboard created per `wtype` call; foot, which binds at once, kept it). No behaviour change otherwise.
- #1632 (2026-11-04) `src/platform_impl/windows/keyboard.rs`: characters outside the BMP (emoji) typed through Unicode input
  (`SendInput` with `KEYEVENTF_UNICODE`, i.e. VK_PACKET: on-screen keyboards, remote desktop, injectors) were dropped. Such a
  character arrives as two VK_PACKET keypresses with one UTF-16 surrogate each, and each keypress's WM_CHAR text was finalized
  alone, so the lone surrogate failed `into_string()` and the text was `None`. `KeyEventBuilder` now keeps a high surrogate that
  ends a keypress's text (`pending_high_surrogate`) and puts it in front of the low surrogate that starts the next one
  (`join_utf16_surrogates`): the first keypress reports no text, the second reports the whole character. A surrogate that cannot
  be paired is dropped with a `log::warn!` line and the rest of the text is kept; nothing panics. Upstream rust-windowing/winit
  (checked at 7ed7c203, 2026-11-03) has the same per-keypress code and the same defect. New dependency: `log = "0.4"` (Windows
  only; already in Phosphor's lock), plus "log" in winit's entry in Phosphor's Cargo.lock -- `tracing` is muted in Phosphor's
  log by a `NoSubscriber`, so winit's own `tracing` lines never reach the log file.
- #1629 (2026-11-04) `src/platform_impl/windows/event_loop.rs`: WM_INPUTLANGCHANGE refreshes the layout cache, then defers to
  `DefWindowProc` -- a port of rust-windowing/winit fb78ecb ("windows: fix freeze on keyboard layout switch", minidump-verified
  upstream for a switch made by Punto Switcher). Our hang (a per-window switch to German via WM_INPUTLANGCHANGEREQUEST) is not
  proven to be the same one, so diagnostics were added with it, all through the `log` crate (Phosphor's log file; `tracing` is
  muted there): WM_INPUTLANGCHANGEREQUEST / WM_INPUTLANGCHANGE entry and exit; a layout change starts a trace of the next 256
  window-procedure calls and event-loop waits (enter/exit, message, depth, thread id); a new layout's preparation is timed
  (`keyboard_layout.rs`); and every `LAYOUT_CACHE` lock goes through `lock_layout_cache(site)`, which logs when the lock is
  already held (so a same-thread re-entry, which never returns, is the last line).
- #1629 follow-up (2026-11-04, no source change here): the hang was not in winit. With `winit=info` in the CI log (r18u13 repro on
  the Windows VM) the trace ends inside the UI thread's `DefWindowProc(WM_INPUTLANGCHANGEREQUEST)` right after two WM_IME_NOTIFY
  calls; WM_INPUTLANGCHANGE never arrives and no LAYOUT_CACHE line follows. The stack sampler shows the UI thread in
  `NtUserMessageCall`, and the window list shows the windows of wgpu-hal's "WGL Instance Thread" (a hidden top-level
  "wgpu Device Class" window plus its Default IME window; the thread blocks in a channel `recv()` and never pumps) as hung: the
  layout change is sent synchronously to that thread. Fixed in the app (`crates/warpui/src/rendering/wgpu/mod.rs`
  `default_backends`: no GL backend on Windows by default). The fb78ecb port and the trace stay.
