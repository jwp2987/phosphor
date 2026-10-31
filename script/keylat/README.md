# keylat: keystroke to photon harness (#1169)

Measures the time from an OS-injected key press to the first screen change in the window.
One protocol, three platform tools, one summariser.

| platform | tool | injection | detection |
|---|---|---|---|
| macOS | `mac/keylat.swift` | `CGEvent` post to the HID tap | ScreenCaptureKit stream of a rect, frame diff (resolution about 8 ms at 120 Hz) |
| Linux (X11) | `linux/keylat.c` | XTEST `XTestFakeKeyEvent` | `XGetImage` poll of a root-window region, FNV hash of the pixels |
| Windows | `windows/lat.ps1` (+ `lib.ps1` helpers) | `keybd_event` | `BitBlt` of a screen region, hash, `GetSystemTimePreciseAsFileTime` timestamps |

`report.py` summarises the output of all three (p50, p90, p99, max, N, nopaint).

These tools were written and used in round 18; they are consolidated here unchanged apart from
this README. They are not built by CI. Known limits are listed at the end: read them before
quoting a number.

## Build and run

    # Linux (needs libx11, libxtst headers; run against a REAL GPU session, not Xvfb)
    cc -O2 -o keylat linux/keylat.c -lX11 -lXtst
    ./keylat -r X,Y,W,H -d 80,180 -s 'systemctl status nginx' -w 1200 -seed 1 > run1.txt

    # macOS
    swiftc -O -o keylat mac/keylat.swift       # then grant Screen Recording + Accessibility
    ./keylat X Y W H SAMPLES ab 250             # rect in points; modes: ab | append

    # Windows (PowerShell 5.1, interactive desktop session, Phosphor foreground)
    . .\lib.ps1; . .\lat.ps1; LatInit 120 30    # region size
    LatRun "local-managed" X Y 'abcdefghij' 200 .\out.csv

    python3 -I report.py --label "phosphor local managed" run1.txt run2.txt

The Linux tool prints `K` (key sent) and `P` (region changed) lines; the Mac tool prints a
`raw ...` line of milliseconds; the Windows tool appends CSV rows. `report.py` reads all three.

## Protocol (what a number must satisfy to be quoted)

1. **At least 200 keystrokes per case**, pooled across runs. `report.py` flags N below 200.
2. **Random gaps, 80-180 ms** between keys (`-d 80,180` on Linux; fixed gaps alias with the
   frame period and with key repeat). Fresh window, same font and size for every candidate.
3. **Also the stress profile**: 10-25 ms gaps (bursts of 20-40 keys, then a pause), and one run
   with a background flood (`yes` or a large `cat` in another tab). At stress speed keys arrive
   faster than frames: p90/max are then backlog, so report them as such, not as latency.
4. **Scenarios**: local managed prompt, local raw (`cat`, `bash --norc`), ssh managed, ssh raw;
   each idle and under flood; plus the autosuggest-heavy command `systemctl status nginx`
   (#1128). Verify every typed command executed exactly and in order (e.g. `touch` markers),
   otherwise a fast run measures a dropped key as a fast one.
5. **Report p50, p90, p99 and max, with N.** Never a mean alone.
6. **A reference terminal runs the identical script in the same session** (xterm or Konsole on
   Linux, Terminal.app on macOS, conhost/Windows Terminal on Windows). Report Phosphor minus
   reference; the reference shows the floor of the method and the noise of the host/network.
7. **Calibration, once per rig and per change of display setup**: measure the reference
   terminal, publish its numbers next to the rig, and once check the software number against a
   camera (240 fps phone, 30 key presses, median within one frame of the software median).
   Without that the absolute numbers are "about this", the deltas are what count.
8. **Remote cases**: also timestamp key arrival on the remote (a tiny raw-mode logger) so
   delivery (RTT, ssh queueing, remote shell) and drawing can be split. A/B
   `-o ObscureKeystrokeTiming=no` and the IPQoS setting as variables.
9. Record the build (`phosphor-oss --version`, commit), OS, GPU/driver, display refresh rate,
   window size and the config used (bare profile vs owner config differ 2-3x at stress speed).

## Known limits of the tools as consolidated

* Linux: Xvfb/llvmpipe skews paint time; the method floor measured there was about 11 ms (xterm
  3.4 ms). Use a real GPU session for absolute numbers. Wayland is not covered (XTEST does
  not reach native Wayland clients; would need wlr-screencopy plus uinput).
* Mac: `keylat.swift` types at a fixed settle interval, not random 80-180 ms gaps; vary the
  settle or wrap it in a driver that sleeps a random time between invocations. One GUI test
  agent at a time on the Mac, screenshot before typing (lock screen hazard).
* Windows: `lat.ps1` has the random-gap logic only through `Burst`'s fixed interval; call it
  per key with a random interval for protocol-conforming runs. Paths in the helper scripts
  point at `C:\Users\Winters\r18w`; edit before use. `keybd_event` needs the interactive
  desktop session, not a service.
* Detection resolves a region change, so a caret blink must be excluded: the Windows tool
  takes a baseline set of frames before each key; on Linux/Mac move the region off the caret
  or use the append mode.
