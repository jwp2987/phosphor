# Oracle pin — which Warp are we porting from?

`warp/master` is a **moving target**. Measuring against it produces a number that
never shrinks, because upstream adds tests faster than any porting effort closes
the gap. This file pins the oracle to a fixed point so parity work is a burndown
instead of a treadmill.

## Policy

**Pin to the *latest Warp stable release*. Never to `master`, `dev`, or
`preview`.**

- A stable release is code real users run. `master` is unreleased trunk.
- The pin tracks the **newest** stable. Warp ships weekly (Wednesdays), so
  expect to re-pin roughly weekly — see [Re-pinning](#re-pinning).
- A pin only moves by an explicit, recorded update to this file — never
  implicitly by fetching. Fetching `warp/master` must not change what any
  measurement compares against.
- All parity claims, gap measurements and "is this a regression" checks are made
  against the current pin, not against `warp/master`.
- Porting something newer than the pin is allowed when there's a reason (a
  security fix, a bug you actually hit). Note it on the issue. It does not move
  the pin.

**What this does and does not buy.** Tracking the latest stable does *not* freeze
the target — Warp ships ~51 tests/day, so each weekly re-pin adds roughly 350 to
the gap. What it buys is that the target moves **in known steps, on a known day,
against code that actually shipped**, instead of drifting every time someone
fetches. A weekly step you can measure is a schedule; a daily drift you can't is
the treadmill.

## Current pin

| | |
|---|---|
| **Release** | `2026.07.29.09.05` stable |
| **Commit** | `02b53fcd8` (2026-07-29 00:14 -0400) |
| **Pinned on** | 2026-08-06 |
| **Tests at pin** | 10,123 |

Compare against it with `git ... 02b53fcd8` in place of `warp/master`.

### Why a commit and not a tag

Warp publishes release tags to `warpdotdev/warp`, but **tag publication stopped
after 2026-06-09** — the newest public tags are `v0.2026.06.03.09.49.stable_00`
(stable) and `v0.2026.06.09.19.54.dev_00` (dev). Releases themselves did **not**
stop: the cadence is weekly on Wednesdays, unbroken, and `2026.07.29.09.05` is
exactly 8.0 weeks after the last tagged stable.

Master keeps receiving commits, so the *source* of each release is still
available — it just has to be located by date. Builds are cut from the previous
day's tip (the `2026-06-03` tag points at a `2026-06-02` commit), which is how
`02b53fcd8` was identified.

**If tags resume, pin to the tag** — it is exact, and dating a release point is
an approximation.

## Gap at the pin — 2026-08-06

Measured by **test-function count**, not filename. Filename matching is
unreliable here: the fork renames Warp's `*_tests.rs` to `*_test.rs`, and
same-path matching counts those as missing. That is where the long-quoted "468
missing test files" figure came from; it is wrong and should not be repeated.

```
warp @ pin (02b53fcd8)   10,123
fork main                 7,884
                         ------
net gap                   2,239   (2,744 behind, 505 ahead)
```

Largest areas behind:

| area | warp | fork | gap |
|---|---:|---:|---:|
| `app/ai` | 1847 | 1229 | 618 |
| `app/terminal` | 1538 | 1148 | 390 |
| `crates/ai` | 352 | 142 | 210 |
| `app/server` | 177 | 5 | 172 |
| `app/settings_view` | 228 | 84 | 144 |
| `crates/warp_cli` | 216 | 76 | 140 |
| `crates/warp_tui` | 745 | 608 | 137 |
| `app/pane_group` | 106 | 29 | 77 |
| `crates/computer_use` | 69 | 0 | 69 |
| `app/remote_server` | 100 | 49 | 51 |

**Not all of this is debt.** Roughly 340 sit in areas the fork drops by design —
`app/server`, `crates/warp_server_client`, `crates/warp_server_auth`,
`crates/cloud_object_*`, `crates/graphql`, `crates/computer_use`. Subtracting
those puts the real target near **1,900**.

Two caveats on the numbers above:

- **Path is not scope.** Classify a test by what it *targets*, not where it
  lives. `server/server_api/ai_tests.rs` reads as pure cloud, but 14 of its 40
  tests cover retained non-cloud code. The ~340 figure is an upper bound on
  legitimate drops.
- **Single-file rows double-count.** In the per-area breakdown, top-level files
  appear in both the "behind" and "ahead" columns because of the `_tests.rs` →
  `_test.rs` rename (`app/menu_tests.rs` −6 alongside `app/menu_test.rs` +14).
  That inflates both sides by roughly 49. Directory-level rows are sound.

## Rate of change (why the pin matters)

Upstream test growth, measured on `warp/master`:

| date | tests | rate |
|---|---:|---|
| 2026-06-23 | 8,056 | — |
| 2026-07-17 | 9,306 | +52/day |
| 2026-07-28 | 9,998 | +63/day |
| 2026-08-06 | 10,740 | +82/day |

This is sustained output, not a pre-release spike — the shipped stables show the
same slope (7,249 at `2026-06-03` → 10,123 at `2026-07-29`, ~51/day).

Against an unpinned `master` the gap grows by 50-80 tests every day, which is why
sustained effort felt like no progress. Against the pin the number only moves when
work lands, or when the maintainer re-pins.

## Re-pinning

The pin follows the latest stable, so this runs roughly weekly. It is a
deliberate, recorded step — never automatic.

1. **Find the new stable.** Check tags first:
   `git ls-remote --tags warp | grep stable | sort -V | tail -3`.
   If tags are still unpublished, use the release version string from the
   downloaded build (`vYYYY.MM.DD.HH.MM`) and take the last `warp/master` commit
   **before** that timestamp — builds are cut from the prior day's tip.
2. **Re-measure** and update this file: pin, date, test count, per-area table.
3. **Record the jump on #2.** Re-pinning *adds* to the gap. That has to be
   visible as a step, not silently absorbed — otherwise the burndown looks flat
   while real work is landing, which is the exact failure this file exists to
   prevent.

Track both numbers over time: **tests ported** (work done) and **pin delta**
(target moved). Reporting only the gap hides the first behind the second.
