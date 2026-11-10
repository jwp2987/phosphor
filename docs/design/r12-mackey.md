# Round 12: macOS `latency_key_log` "REGRESSION key to frame" (#2037)

Round 11, macOS smoke tier, build c61b6a3b. The only FAIL: `latency_key_log` (terminal mode),
`REGRESSION key to frame: median 7.28 vs baseline build +1.05 ms (+16.9%); re-measured once: median 7.2 (fail)`.

**Verdict: TEST fault (the gate). No app regression, no app change.**

## Evidence

The case types 60 keys, 80-180 ms apart, into a running `cat > /dev/null` and pairs each `keydown` with the first `frame` line after
the echo (`cases.key_frame_pairs`). Hop lines of the r11a per-case log (`run_slim/logs/{terminal,agent}/case_latency_key_log.log`),
medians in ms, first run / re-measure:

| part | terminal | terminal again | agent | agent again |
|---|---|---|---|---|
| keydown -> `pty_write` | 0.34 | 0.34 | 0.37 | 0.34 |
| `pty_write` -> `pty_read` (the echo) | 0.08 | 0.08 | 0.09 | 0.08 |
| redraw request -> AppKit display pass (`hop=key_frame_sched ms=`) | 5.74 | 5.33 | 5.79 | 4.28 |
| display pass start -> `frame` line (frame built; `key_frame_build total_ms` 1.10-1.12) | 0.99 | 0.95 | 0.99 | 0.82 |
| **key -> frame median** | **7.28** | **7.20** | **7.47** | **5.87** |
| key -> frame minus the wait for the display pass | 1.68 | 1.60 | 1.72 | 1.47 |
| key -> frame p95 / max | 9.40 / 9.42 | 9.39 / 9.54 | 9.31 / 9.33 | 9.31 / 9.42 |
| wait for the display pass, max | 7.50 | 7.57 | 7.39 | 7.43 |

1. **Four fifths of the median is the wait for the next display pass.** The echo frame is requested by the pty read
   (`request_redraw` -> `dispatch_async` + `setNeedsDisplay:`, `crates/warpui/src/platform/mac/window.rs:1238`) and AppKit serves it on
   its next 120 Hz pass. `hop=key_frame_sched` (`frame_latency.rs:251`, logged at `window.rs:1463`) is that wait: 0.6-7.6 ms, set by
   where in the refresh interval the key arrived. The app's own work is the other 1.5-1.7 ms.
2. **The same build in the same session measured 7.47 and then 5.87 ms** (agent mode; the keys go to `cat`, the same code path as
   terminal mode). The app's share moved 0.25 ms; the wait moved 1.5 ms.
3. **The gate is finer than the measurement.** `lat.regress` fails at +5 % and +0.5 ms on the median. The record's own bootstrap
   standard error of that median is 0.69 ms (`per_side.phosphor.se_median_ms`). Across the 26 Mac measurements since 2 Nov
   (`test-results/*/app/macos/results.json`, 11 runs x 2 modes plus the re-measures of rounds 10 and 11; r18ct, a slow-machine run,
   left out) the median ranges 5.86-8.44 ms, mean 6.9; p95 stays 9.16-9.44 and key -> pty write 0.33-0.44.
4. **The baseline is one low draw.** `baselines/macos.json`: terminal 6.23 / p95 9.32, agent 6.70 / p95 9.28, "promoted 2026-11-06
   from results.json commit 08090ee8" (d572e3bce: the v0.1.17 release run r18u31, promoted with `--force` because the eligibility
   check refuses every run on this very record's NO-REFERENCE verdict). 6.23 is the second lowest of the 13 terminal-mode values, so a
   first measurement lands over the gate (6.73) more often than not and the one re-measure clears it only some of the time. Round 10's
   Mac run had the same first measurement (7.33) and was cleared by its re-measure (6.6); round 11's was not (7.2).
5. **Nothing on this path changed since the baseline build.** `git log 08090ee8..c61b6a3b -- crates/warpui/src/platform/mac`: #1838
   (skip frames that would block in `nextDrawable`), the step A measurement hops, #1848 (a key's frame is drawn when the key press has
   been handled, `PHOSPHOR_MAC_KEY_FRAME_NOW`, default on). #1848 covers redraw requests made *during* the key press
   (`RedrawRequests::begin_key_frame`, `frame_latency.rs:347`); the echo frame is requested by the pty read, 0.4 ms after the press has
   ended, so it waits for the display pass as it did in the baseline build (same p95, same max: one interval plus the app's work).
   The new hop lines (`decset_2004`, `notify_cause`, `key_frame_build`, the hover pass time) are inside the 0.34 ms key -> pty write
   and the 1.1 ms frame build; key -> pty write was 0.38-0.44 on 9 Nov and is 0.33-0.34 now.
6. **The numbers that do measure the app are the best recorded.** Same run: `latency_paint` (camera, key -> first changed pixel) 30.69
   ms, +0.45 ms over Terminal.app (r18w8: 35.26, +7.45); `agent_latency_key` terminal-editor key -> frame 1.46 ms (7.0-7.5 before
   #1848).

The key's arrival phase is not uniform either (the keydown phases in the log cluster in two bands of the refresh interval, and which
band dominates differs per run), which is why the median of this bounded distribution moves by more than its within-run spread
suggests while its upper end does not move at all.

## Fix (harness only)

`latency_key_log` on macOS hands `lat.evaluate` the app's own share of each key, the mechanism `ctrl_c_d` uses on Windows
(`lat.py` "app-share basis", owner 2026-11-10):

- **Figure** `key_to_frame_less_display_wait`: key -> frame minus the part of the logged display-pass wait that lies after the keydown
  (`cases.key_frame_shares`). A key whose frame logged no `hop=key_frame_sched` has no figure. Usable only with a figure for at least
  90 % of the pairs and n >= `APPCI_MIN_SAMPLES`; otherwise the median gate applies exactly as before (every other platform, an old
  build, a log without the hop).
- **Regression verdicts** (previous build, baseline) compare that figure with the unchanged thresholds and the unchanged one
  re-measure. The median's delta stays in the record and in the notes as NOT JUDGED.
- **Hard bound**: the share over `APPCI_KEY_APP_SHARE_MAX_MS` (default: `APPCI_KEY_BUDGET_MS`, the owner's 2 ms own-work budget, which
  this case could never judge because it has no reference terminal) is a FAIL. Measured 1.47-1.72 ms.
- **Tail gate** (so the wait itself is still judged): with the share as the basis, the p95 of the whole key -> frame time is compared
  with the baseline's and the previous build's p95, same thresholds, one re-measure (still over = FAIL, clean = WARN). p95 is the
  stable figure of this distribution (one refresh interval plus the app's work: 9.16-9.44 over all 26 measurements, baseline 9.32 /
  9.28). A display pass missed for more than 5 % of the keys puts it near 17 ms; app work that grows by 0.5 ms moves it by 0.5 ms.
- New metrics: `key_to_frame_own_ms`, `key_frame_display_wait_ms`.

The baseline file is not edited. It has no share for this record, so the baseline's share verdict is `none` (said in a note) until a
full-tier run is promoted with `baseline.py promote` (which carries `app_share`); until then the run is judged by the hard bound,
the p95 tail against the existing baseline p95, the previous build's share once one run has recorded it, and the old sanity bounds.

## Rejected

- **Re-promoting the baseline from r11a or r10a.** Both are smoke-tier runs with NOREF verdicts (r11a with this FAIL); `baseline.py`
  refuses them and `--force` is for a run the owner has reviewed. A higher median baseline would also only move the coin, not remove it.
- **Raising `APPCI_REGRESS_MS` / `APPCI_REGRESS_PCT`, or making the median gate SE-aware for every case.** That loosens every latency
  gate on every platform to cure one record whose median is not an app figure.
- **More samples.** The standard error falls with sqrt(n): 240 keys for half of it, and the session-to-session phase banding stays.
- **Drawing the echo frame without waiting for the display pass.** That is an app change on the pty output path (frame pacing under
  output is what #1838 and #1979 protect) for a number that is not a regression: the frame still reaches the screen on the next
  refresh either way (`key_presented`), and the camera case is the judge of that.

## Risk

Harness only. The hard bound is new and its headroom is small (1.47-1.72 ms against 2 ms, with about 20 debug log lines per key
inside the figure): a loaded Mac could put a run over it. It is the owner's stated own-work budget; whether it stays a FAIL at 2 ms is
the owner's call (`APPCI_KEY_APP_SHARE_MAX_MS`). The tail gate uses p95 of n=60, the third largest sample: stable here because the
distribution is bounded, but one slow run of the machine (r18ct: p95 22.5) fails it, as the old sanity bounds would not.
