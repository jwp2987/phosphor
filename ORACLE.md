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
| **Release** | `2026.08.12` stable |
| **Commit** | `42effe840` (2026-08-11 20:51 -0400) |
| **Commit (full)** | `42effe84055f891405b32914af333f14127ec381` |
| **Pinned on** | 2026-08-15 |
| **Tests at pin** | 10,860 |

Compare against it with `git ... 42effe840` in place of `warp/master`.

**On the release string.** `2026-08-12` is a Wednesday and the pin is the tip of
the preceding evening (Tuesday 20:51 -0400 = Wednesday 00:51 UTC), which is
exactly the "cut from the previous day's tip" rule below. The `HH.MM` build
stamp of that release is **not recorded here because it cannot be checked** —
tag publication stopped 2026-06-09, so there is no artifact to read it from, and
inventing one would put a number in this table that no one can verify. The
commit is the authoritative identifier; the release string is the human label.

**On the test count.** 10,860 is `script/state`'s measure — unique test-fn names
under `#[test]` / `#[tokio::test]` / `#[async_std::test]`. The same measure gives
**10,026** at the old pin, so the step is **+834**. The previous row in this
table read 10,123, which was produced by a different, unrecorded method and never
matched `docs/STATE.md`'s generated 10,026. Do not read `10,860 − 10,123` as the
step; the two numbers are not the same measurement.

## Pin history

| release | commit | pinned on | tests at pin |
|---|---|---|---|
| `2026.08.12` stable | `42effe84055f891405b32914af333f14127ec381` | 2026-08-15 | 10,860 |
| `2026.07.29.09.05` stable | `02b53fcd81ac49adffe5288201e4387abe48f23c` | 2026-08-06 | 10,026 (10,123 as originally recorded, see above) |

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

## Gap at the pin — 2026-08-06 (measured at the OLD pin `02b53fcd8`)

> **These figures have not been re-derived at `42effe840`.** They come from the
> per-file SCOPE classification of the 854 test-bearing files at the *old* pin,
> which is a full reading pass, not a generated number — re-running it is Phase
> 2/4 work of the next round, not part of moving the pin. They are kept here
> because the *shape* they describe (net ≠ workload) is still the point, and
> deleting them would lose the only written statement of that distinction.
>
> **For current numbers, read `docs/STATE.md`** — it is generated from the tree
> and the ledger on every run and is the authority when the two disagree. As of
> the `42effe840` move the ledger carries **2,360 adjudicated rows** with **0
> unadjudicated**, of which `MISSING-SUBSYSTEM` is **233**.

Measured by **test-function count**, not filename, and classified **per file by
reading source imports** — not by path. Every test-bearing file at the pin (854)
was classified. See `SCOPE-AI.md`, `SCOPE-TERMINAL.md`, `SCOPE-REST.md` (#2).

> **Read this before quoting any number here.** A *net* count and a *workload*
> are different quantities, and conflating them is why this project spent weeks
> unable to say what the scope was (#218).

### Two numbers, deliberately kept apart

```
pin 10,123  −  fork 7,884  =  2,239      <- NET. Not a workload.
```

Net is what you get by subtracting totals. It is wrong in **both** directions:
it hides Warp tests we lack behind fork-original tests that cover *fork*
behaviour, and it counts out-of-scope cloud tests as if they were work.

```
Warp tests genuinely ABSENT from the fork      3,902
  of which:
    A  test debt      — fork ships the code    1,605   <- THE WORKLOAD
    D  feature gap    — port the feature first    792
    C  out of scope   — cloud / dropped         1,505
fork-original tests offsetting the net figure  1,663
```

**The actionable queue is 1,605.** Not 2,239, and not 3,902.

### By slice

| slice | absent | A debt | D feature | C out-of-scope |
|---|---:|---:|---:|---:|
| `app/ai` + `crates/ai` | 1,511 | 571 | 412 | 528 |
| `terminal` + `warp_tui` | 729 | 420 | 114 | 195 |
| everything else | 1,662 | 614 | 266 | 782 |
| **total** | **3,902** | **1,605** | **792** | **1,505** |

Largest verdict-A concentrations: `input_tests.rs` (60), `terminal_session_view_tests.rs`
(57), `view_tests.rs` (55), `view_impl_tests.rs` (36).

### What the classification overturned

Every one of these was believed true and was wrong. They are recorded because
the *method* that produced each error is still available to repeat:

- **`crates/computer_use` is not dropped.** The fork ships 28 of 45 files. Only
  11 tests are a feature gap.
- **`crates/ai/src/api_keys_tests.rs` yields zero straight debt.** All 57 need a
  feature ported first — the fork's `api_keys.rs` is 229 lines vs 776, with no
  Grok, no GEAP, and zero repo-wide hits for `CustomEndpoint`. A feature gap
  hiding inside a file the fork appears to ship.
- **`request_usage_model_tests.rs` is out of scope despite the fork shipping the
  source** — that source is a 260-line no-op stub. A source-presence check calls
  this debt. It isn't.
- **101 of 177 terminal files are fully covered** under renamed or inlined paths.
  Path matching calls all 101 missing; that is the origin of the discredited
  "468 missing test files" figure.
- **Same filename ≠ same code.** `codex.rs` is 61 fork lines vs 492 (0 of 39
  tests survive); `zero_state_animation.rs` is a fork-original starfield vs
  Warp's rotating mark (0 of 26).
- **Path is not scope, in either direction.** `settings_view/environments_page.rs`
  reads local but is entirely cloud (#211). `server/server_api/ai_tests.rs` reads
  cloud but 14 of its 40 tests cover retained non-cloud code. Mixed files must be
  classified per *test*.

### Rules for anyone re-measuring

1. Match by **test-function name across the whole fork tree**. The fork renames
   `*_tests.rs` → `*_test.rs` and flattens `a/b/c_tests.rs` → `a/b_c_tests.rs`,
   so a path miss is never evidence of absence.
2. Justify every out-of-scope verdict by **quoting the source file's imports**.
   "It's cloud" is never self-justifying — most of Warp's cloud-organised AI code
   (agent loop, tool-calling, streaming, context building) has a BYOP equivalent
   and is in scope.
3. Check for **name collisions** before declaring a test covered
   (`selection_cursor_tests.rs::test_cursor` vs
   `grapheme_cursor_tests.rs::test_cursor` are different tests).
4. Report **behind / ahead / net separately**. Never a single number.

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
