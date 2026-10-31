# Profiling Phosphor (#1169)

Nothing here changes the release build. Every variant below is a separate, manual build.

## 1. Build variants

| variant | how | what you get |
|---|---|---|
| sampling-profiler build | CI job `build-{linux,macos,windows}-profiling` with `PROFILING_FEATURES=""` | release-lto code, symbols and line tables kept, zero extra code |
| Tracy build | the same job with its default `PROFILING_FEATURES=profiling` | the above plus Tracy zones (section 3) |
| local | `cargo build -p warp --profile profiling --bin phosphor-oss --features release_bundle,gui,nld_improvements[,profiling]` on the build box only | same as the job |

The jobs are manual, run on protected refs only (`main`, `build/*`, `v*`; see the GitLab build
notes), build the bare binary (not an installer), and publish it UNSTRIPPED as a 7-day
artifact: `dist/phosphor-oss-profiling` (Linux), `dist/phosphor-oss-profiling` plus
`.dSYM` (macOS), `dist\phosphor-oss-profiling.exe` plus `.pdb` (Windows). The release
bundlers strip (`script/linux/bundle` runs `strip --strip-all`); the profiling jobs never do,
and the Linux job fails if `.symtab` or `.debug_line` is missing.

`[profile.profiling]` is `release-lto` with `debug = "line-tables-only"` and `strip = false`.
On Linux the job sets `CARGO_PROFILE_PROFILING_SPLIT_DEBUGINFO=off` so the DWARF stays in the
one file; macOS keeps `packed` (the dSYM), Windows its `.pdb`.

**Frame pointers.** The Linux job adds `-C force-frame-pointers=yes`, so Phosphor's own
frames can be walked cheaply. The prebuilt Rust `std` and libc do NOT have frame pointers, so
frame-pointer unwinding stops at the first std frame. samply's DWARF unwinder does not have
that limit; prefer it over `perf --call-graph fp`. (`perf --call-graph dwarf` on the stripped
release binary produced one frame in round 18: that is the stripped binary, not perf.)
Setting `RUSTFLAGS` replaces `.cargo/config.toml` rustflags, so the job repeats
`-C symbol-mangling-version=v0`.

The `profiling` feature is rejected by `script/linux/bundle`, `script/macos/bundle` and
`script/windows/bundle.ps1`. Do not enable it for `--target *-musl` / the CLI: the Tracy client
is vendored C++.

## 2. CPU profile with samply

    cargo install samply            # once, on the machine that records
    samply record --save-only -o prof.json.gz ./phosphor-oss-profiling
    samply load prof.json.gz        # opens the Firefox Profiler UI

* **Linux**: needs `kernel.perf_event_paranoid <= 1` (`sudo sysctl kernel.perf_event_paranoid=1`).
  That is a host change: ask the machine's owner first. Run a Phosphor instance of your own
  (private `HOME`, `dbus-run-session`) so it does not hand off to a running one.
* **macOS**: run `samply setup` once (it codesigns samply for task_for_pid). Keep the dSYM next
  to the binary.
* **Windows**: unelevated `samply record` works for your own processes; keep the `.pdb` next to
  the exe.

Record the scenario you care about (for example typing `systemctl status nginx` over ssh)
and look at the main thread and the PTY reader thread separately.

## 3. Tracy (zones, frame marks)

Build with the `profiling` feature. The client is `tracy-client` pinned to `=0.18.2` in
`crates/warpui_core/Cargo.toml` with `ondemand` (nothing is recorded until a viewer connects)
and `only-localhost` (it listens on 127.0.0.1:8086 only). **The viewer must match the client's
protocol version**: take the Tracy release listed for `tracy-client-sys` 0.18.2 in the
tracy-client README table and build/install that same Tracy release's profiler GUI. A mismatch
shows as "protocol version mismatch" in the viewer.

To view a run on another host forward the port: `ssh -L 8086:127.0.0.1:8086 <host>`, then
connect the viewer to `127.0.0.1`.

Instrumentation lives in `warpui_core::profiling` (`zone!`, `mark!`, `frame_mark`,
`present_frame`); with the feature off every call compiles to nothing. Names follow the
`input_order` debug hops so a trace and a `RUST_LOG=warp::input_order=debug` log line up:

| name | where |
|---|---|
| `key_down` (mark), `input_dispatch` | `Presenter::dispatch_event` (first common point of the platform layers) |
| `editor_edit` | `Input::user_insert` |
| `autosuggest_request`, `autosuggest_result` | `generate_autosuggestion_async`, `on_autosuggestion_result` |
| `typeahead_insert`, `typeahead_handoff`, `typeahead_handoff_insert` | `insert_typeahead_text`, `flush_ssh_held_typing`, `insert_typeahead_from_handoff` |
| `pty_write`, `pty_read`, `parse` | local tty event loop (`parse` wraps `parse_bytes`, one zone per read slice) |
| `grid_update` | `TerminalView::handle_terminal_wakeup` (model to view update after output) |
| `frame`, `layout`, `paint` | `Presenter::build_scene_impl`, `layout`, `paint` |
| frame mark | `present_frame` around `SurfaceTexture::present` (wgpu) and `drawable.present` (Metal): once per presented frame |
| `remote_send`, `remote_receive` (marks) | `remote_server::protocol::{write,read}_message` |

Marks, not zones, are used across `.await` points (a zone guard is `!Send`).

## 4. macOS: Instruments

Use Time Profiler, Points of Interest, Metal System Trace and Hangs on the profiling binary
(attach by path or PID). Points of Interest shows the `os_signpost` intervals added by
fix/1148 (not part of this branch); line them up against the Tracy zone names above. Without
those signposts the Time Profiler and Hangs templates still work.

## 5. Windows

Windows Performance Analyzer: record with WPR "CPU usage" (and "Disk/Wait" if needed), keep
the `.pdb` beside the exe, and use CPU Usage (Precise) to see why the UI thread waited
(ready-thread and context-switch analysis). Installing WPR/WPA or Superluminal on the VM is an
environment change: ask first.

What worked on the Windows test VM without installing anything (round 18, #1224, #1225):

* **samply 0.13.1 does not work there.** Unelevated `samply record` starts, never launches the
  target and hangs (it needs the kernel ETW session). Do not spend time on it.
* **Tracy works, headless, on the VM itself.** Take `tracy-capture.exe` and `tracy-csvexport.exe`
  from the Tracy release that matches the client (`tracy-client-sys` 0.26.1 vendors Tracy 0.12.2:
  `windows-0.12.2.zip`), copy them to the VM, start the profiling exe, then
  `tracy-capture.exe -o run.tracy -a 127.0.0.1 -f`; kill the app when done (that ends the
  capture) and run `tracy-csvexport.exe -u run.tracy > run.csv` (UTF-16; `iconv -f UTF-16`).
  `-u` gives one row per zone with `ns_since_start` and `exec_time_ns`, enough to print a
  timeline around a key.
* **A VM has no invariant TSC**: the profiling exe exits at once with "Tracy Profiler
  initialization failure: CPU doesn't support invariant TSC". Set
  `TRACY_NO_INVARIANT_CHECK=1` in its environment.
* **The bare `phosphor-oss-profiling.exe` needs its neighbours.** Without `conpty.dll`,
  `dxcompiler.dll`, `dxil.dll`, `x64\OpenConsole.exe` and the runtime DLLs beside it the window
  fails with "No usable wgpu adapter was found" and the process exits 0. Copy the installed
  `Programs\Phosphor` directory and replace the exe.
* **`pwsh.ps1` next to the exe is what the app sources**, so a bootstrap change can be tried
  without a rebuild: replace that file, restart the app.
* `RUST_LOG=warp::input_order=debug,warpui::windowing::winit::event_loop=debug` gives the
  `frame_drawn` hop (millisecond timestamp, total and swapchain-acquire time per frame) on the
  same clock as the `keydown` and `pty_write` lines.

## 6. Memory

* Allocation hot spots / leaks on Linux: `heaptrack ./phosphor-oss-profiling`, then
  `heaptrack_gui`. Not installed by default; installing it is a host change, ask first.
* A 24-tab soak: build with the existing `jemalloc_pprof` feature (the dev channel bundle
  already enables it) and use its heap profile endpoint.

## 7. tokio-console

Feasible but NOT wired up here. It needs (a) `RUSTFLAGS="--cfg tokio_unstable"` for the whole
build, which changes every crate's cache key (a full rebuild on the box) and, being a
`RUSTFLAGS` value, must be combined with the flags the profiling job already sets; and (b) a
`console-subscriber` dependency behind a feature, plus initialising it in `app` startup, which
brings tonic into the graph (hyper is already there). Do it only when an async stall is suspected, as its own
branch, not as part of the always-available `profiling` feature.

## 8. Latency numbers

For keystroke-to-photon measurements see `script/keylat/README.md`. A profile tells you where
time goes; the harness tells you whether the user can feel it.
