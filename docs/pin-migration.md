# Pin migration — moving the oracle from pin N to pin N+1

Runbook for advancing the pinned Warp release. Written to be handed to agents.

`ORACLE.md` says *what* the pin is and why it is pinned at all. This says *how to
move it*. Read `ORACLE.md` first; nothing here overrides it.

**The tooling for this already exists and was built during the first catch-up
specifically so this pass would be cheaper** (`TODO.md` § RE-PIN AUTOMATION).
Your job is mostly to run it, triage its output, and record verdicts — not to
re-derive a diff by hand. If you find yourself reading a raw `git diff` between
two Warp commits, stop: that is thousands of files and is the thing this was
built to avoid.

---

## Phase 0 — fetch the oracle. Do not skip this.

**A fresh clone of this repository has no Warp remote and no pin object.**
Verified on 2026-08-14: `git remote -v` lists only `origin`, and
`git cat-file -t 02b53fcd8` fails with "Not a valid object name".

This is the single most dangerous step, because of how the tools fail:

> `script/generate_repin_queue` and `script/check_stub_coverage` **skip cleanly
> (exit 0) when a pin commit is missing.** That is a deliberate convention so CI
> is not broken by an unfetched oracle — but it means a missing remote produces
> an empty queue and a green exit, which reads exactly like "there is nothing to
> do."

So:

```bash
git remote add warp https://github.com/warpdotdev/warp    # if absent
git fetch warp
git cat-file -t <old-pin>   # MUST print "commit"
git cat-file -t <new-pin>   # MUST print "commit"
```

**Do not proceed on an empty queue until both `cat-file` calls succeed.** State
in your report that you ran them and what they printed. An empty queue that you
have not proven is empty is worth nothing.

## Phase 1 — choose the new pin

Rules, from `ORACLE.md`:

- Pin to the **latest Warp *stable* release**. Never `master`, never `dev`.
  Master is unreleased trunk; measuring against it produces a gap that never
  closes.
- **Tag publication stopped after 2026-06-09.** The newest public tags are
  `v0.2026.06.03.09.49.stable_00` / `v0.2026.06.09.19.54.dev_00`, but releases
  did not stop — the cadence is weekly on Wednesdays.
- With no tag, locate the release commit **by date**: builds are cut from the
  **previous day's tip**. That is how `02b53fcd8` was identified for
  `2026.07.29.09.05`.
- **If tags resume, pin to the tag.** Dating a release point is an
  approximation; a tag is exact.

Record, for `ORACLE.md`: release string, commit sha, commit date, the date you
pinned, and the test count at the new pin.

> Expect the gap to *grow* at the moment of re-pin. Warp ships ~51 tests/day, so
> a weekly step adds roughly 350 tests to the target. That is the deal `ORACLE.md`
> makes deliberately: a weekly step you can measure beats a daily drift you
> cannot. Do not report the growth as a regression.

## Phase 2 — generate the work queue

```bash
script/generate_repin_queue <new-pin>          # <old-pin> defaults to ORACLE.md's
```

It diffs the two Warp commits over `*.rs`, keeps only **test-bearing** files
(the same four attributes `SCOPE-*.md` uses: `#[test]`, `#[tokio::test]`,
`#[gpui::test]`, `#[rstest]`/`#[test_case]`), and buckets what survives.

Work the buckets **in this order**:

| bucket | what it means | priority |
|---|---|---|
| **DECLINED COLLISION** | the upstream diff touches a name or path `DECLINED.md` has marked | **First.** Read the row before touching anything. |
| **LEDGER RE-EXAMINE** | `docs/sweep-verdict-ledger.tsv` has per-test verdicts for this file, but the file changed upstream | High — the verdicts are stale, not wrong-by-default |
| **UNCLASSIFIED** | no `SCOPE-*.md` row and no ledger row at the old pin | High — needs a *first* look, not a re-look |
| **inherited verdict** | `SCOPE-*.md` has a letter (A/B/C/D/MIXED) for the path | Medium — see the trust warning below |
| **REMOVED AT NEW PIN** | existed at old pin, gone at new | Informational. Retire any ledger rows. |
| **CLOUD-DROPPED** | the pin file's own imports reach `crate::server::` / `cloud_object` / `warp_graphql` | Counted, not listed. Nothing to do. |

Scale, from the `warp/master` rehearsal: 549 files changed → 177 test-bearing →
11 declined-collision, 20 unclassified, 52 actionable, 50 low-priority, 44
cloud-dropped. Of 1,843 ledger rows, ~1,025–1,066 carried forward untouched and
~62 files' worth (~813–818 tests) flagged RE-EXAMINE. **A pin move is mostly
carry-forward.** If your queue says otherwise, suspect the queue before
suspecting the ledger.

### The three invalidation rules

A recorded verdict stops being trustworthy when one of these fires. The
generator checks all three, and the last two fire **even for pin files that did
not change**:

1. **The pin file changed.** Every ledger verdict recorded against it needs a
   fresh look. This supersedes the coarser `SCOPE-*.md` letter, because the
   ledger is per-test and the letter is per-file.
2. **The row's cited `DECLINED.md` issue is now struck through.** Should
   normally be empty — `script/check_sweep_ledger` fails CI the moment it
   happens — so a hit here means a stale CI run.
3. **A MISSING-SUBSYSTEM row's absent symbol now has a definition** somewhere in
   the fork. The gap it recorded may have been closed by unrelated work.

### What you must not trust silently

- **Inherited `SCOPE-*.md` verdicts are verdicts, not facts**, and verdict A is
  *known* overstated for two separate, already-discovered reasons: MIXED files
  collapse to their majority bucket, and a same-named file is not necessarily
  the same module — the pin's API under test may not exist in an otherwise
  verdict-A file. The generator labels every one of these as a VERDICT; keep
  that label when you quote it.
- **The ledger carries the same caveat, one level finer.** Read the
  `confidence` column (`clean` / `judgement` / `unparsed`) and
  `script/extract_sweep_ledger.py`'s header for what each means.
- **A shared test *name* is not a shared *assertion*.** Name-level parity counts
  are approximate by construction; `docs/sweep-verdict-ledger.tsv` is the
  authority on any individual test.

## Phase 3 — fast-forward what is free

```bash
script/generate_pin_identity_manifest      # -> docs/PIN-IDENTITY-MANIFEST.md
```

Compares git blob hashes between pin and fork HEAD for every `.rs` under
`app/src` and `crates`. Last measurement: **572 identical (17%), 2,334 differ,
460 fork-only** of 3,366 files.

A file byte-identical to the old pin and changed upstream can usually be taken
wholesale — no merge, no judgement. Do these first: they are the cheapest
possible progress and they shrink the queue before anyone spends thought on it.

It is a **snapshot, not a gate**. Regenerate it; do not trust a stale copy.

## Phase 4 — port, as a fleet round

Follow `docs/FLEET-ROUND.md` exactly. The rules that matter most:

- **Agents never run the full suite.** Their gate is
  `rustfmt --check --config-path .rustfmt.toml <changed files>` plus both
  fork-boundary guards. A formatting diff is expected noise — this repo has
  never been rustfmt-clean; only `^error: (expected|unexpected)` is a real
  failure. The coordinator batches one suite run for the whole round.
- **Put the protocol in the original brief.** Never retrofit it by message: an
  agent that receives mid-run "coordinator" instructions via system-reminder may
  reasonably refuse them, and the more security-conscious the agent the more
  certainly it does.
- **Keep agent worktrees on a current base.** Branches cut from different
  commits produce merge pain the round did not need.
- **Never call a failure "pre-existing" without measuring it.**

Every agent brief should carry, verbatim:

- the pin shas, and that `git cat-file -t` was verified for both
- `AGENTS.md` §5.6 (never weaken a test to go green — fix the code), §5.10 (no
  silent regressions), §5.11 (every defect gets an issue)
- that inherited verdicts are verdicts, not facts
- that `DECLINED.md` is checked *before* filing anything as parity debt
- an explicit build policy — whether they may compile at all, and if not, that
  their report must say plainly that nothing was verified

## Phase 5 — update the recorded state

All of these, or the next re-pin starts from stale inputs — which is the exact
cost this whole apparatus exists to avoid:

| artifact | action |
|---|---|
| `ORACLE.md` | new release / commit / pinned-on date / tests at pin; move the old pin into the history section |
| `docs/sweep-verdict-ledger.tsv` | re-adjudicate every RE-EXAMINE row; retire rows for files removed at the new pin; add rows for UNCLASSIFIED |
| `SCOPE-{AI,TERMINAL,REST}.md` | update the staleness banners — do not silently leave them pointing at the old pin |
| `DECLINED.md` | any new deliberate divergence, with `sym:` / `path:` / `test:` / `keep:` markers where an exact identifier exists |
| `docs/PIN-IDENTITY-MANIFEST.md` | regenerate |
| `docs/STATE.md` | **regenerate with `script/state`. Never hand-edit.** |
| `TODO.md` | fold the remaining queue in; it is the work ledger |
| `HANDOFF.md` | anything that cost time and would cost it again |

## Phase 6 — gates

Before merging the round:

```bash
script/precheck        # every gate CI runs except the full suite
```

Then the full suite, batched once by the coordinator, and CI.

Guards that must pass — each has a header comment explaining what an earlier
version of it got wrong, worth reading before you change one:
`check_cloud_boundary`, `check_stub_coverage`, `check_declined_collisions`,
`check_sweep_ledger`, `check_settings_registry`, `check_channel_command_names`.

`script/check_test_failures` fails on **change**, not on redness — it diffs
against `script/known_test_failures.txt`. A re-pin that ports tests will move
that baseline legitimately; a re-pin that *breaks* something moves it too.
Diff the baseline deliberately and explain every line that moved.

## Phase 6.5 — the partial-port sweep

**Do this every re-pin. It is the highest-yield check in this document, and it
exists because the failure it catches is invisible to every other gate.**

A commit ported *incompletely* passes review, passes CI, and often ships with
its own upstream test — because the test was ported too and cannot detect what
was dropped. The queue in Phase 2 asks "was this file touched?"; it does not ask
"did every hunk of that commit actually land?" Nothing else here does either.

Five instances were found in one sitting on 2026-08-15, all predating that day:

| upstream | what landed | what did not |
|---|---|---|
| `01778efe` (#12362) | doc comment, new method, caller change, **and the test** | `Arc<DashMap>` → bare `DashMap`, the one load-bearing line |
| `88c344e2` (#25354, **[Security]**) | the `session.rs` escaping site | the `remote_command_executor.rs` site |
| `1b65a8b9` (#14746) | — | `find -L`, never taken |
| `0d24d2cf` (#12465) | — | control-master reuse, never taken |
| `0ed36638` (#9444) | — | `$CDPATH` completion, never taken |

The `01778efe` case is the shape to fear: our own sweep `c3b86368` ported it
while aiming at *test coverage*, dragged the surrounding production code along,
and dropped one line. The ported test exercised a single context and never
cloned, so it passed identically with and without the fix. The consequence was
one SSH channel per context-chip clone per directory, exhausting the remote's
`MaxSessions` and printing `channel N: open failed` into the user's live shell.

### The method

Mechanical; script it, do not eyeball hundreds of commits:

1. `git log --format=%H <old-pin>..<new-pin> -- <path>` for candidates.
2. Per commit, `git show --stat`; drop files absent from this fork — but check
   for a **renamed** equivalent first (Warp→Zap→Phosphor renames are pervasive,
   and a rename reads as "absent" if you do not).
3. Per surviving file, take 1–3 **distinctive** added lines from the diff —
   identifiers, string literals, whole expressions; never whitespace or `use`
   lines — and grep this fork's copy.
4. Classify: all present = ported. None present = never ported (a parity gap,
   a *different* decision — record it, do not fix it in this pass).
   **Some present = PARTIAL. Investigate by hand.**
5. Expect false positives on every PARTIAL: the fork rewrote the area, code
   moved, or it diverged on purpose. **Check `DECLINED.md` and `git log` for a
   deliberate decision before "fixing" anything.**

Prioritise security fixes, then correctness/panic/race/leak fixes. The
highest-value thirty done properly beats six hundred done badly — and say in the
round report where you stopped, because an unfinished sweep that reads as
finished is how this class of bug survives a re-pin in the first place.

### The failure this method invites, and the only defence against it

**An upstream fix can be WRONG here.** The fork has diverged, and a fix written
against upstream's semantics can be a no-op, or actively harmful, against the
fork's. The triage above tells you a hunk is missing; it cannot tell you whether
you should want it.

Observed 2026-08-15, and caught only by luck and diligence. An agent ported
`730a4acc0` (#13167, "respect count before gg"), which adds a `saturating_sub(1)`
row conversion. Plausible, well-documented, upstream-sourced, and it made the
fork's code look more like the pin. It was **wrong**: this fork's `Buffer`
prefixes every document with a zero-width `BlockMarker`, so its rows are already
1-indexed, and upstream's conversion would have *introduced* the off-by-one it
exists to fix. The agent found this by tracing `test_dimension_conversions` and
`test_vim_jump_to_end_and_beginning` through real buffer semantics, then reverted
its own change and kept only the comments and two regression tests.

Nothing in the mechanical method would have caught that. The grep said MISSING,
which was true. The upstream commit message said "fixes an off-by-one", which was
true upstream. Both facts pointed the wrong way.

So: **before porting, establish what the fork's code actually does, not what
upstream's did.** Read the fork's own tests for the surrounding behaviour — they
encode the fork's semantics, and they are the fastest way to discover a
divergence. If a port makes an existing fork test look wrong, that is the signal
to stop and investigate, never the signal to change the test (`AGENTS.md`
§5.6/§5.10/§5.11).

A corollary worth budgeting for: this is why findings from an area's *own* owner
are worth more than findings from a neighbour. On the same day, a cross-area
report of a WSL freeze bug (#12492) was refuted outright by the agent that owned
that code — the fix had been present all along. A cross-area finding is a
hypothesis until the owning area checks it.

Cloud is out of scope: this fork dropped it, so a cloud commit is not a partial
port. `DECLINED.md` lists the recurring false positives (`remote_server`,
`computer_use`, Grok OAuth) that get mislabelled as cloud — `remote_server` in
particular is the local SSH extension and **is** in scope.

## Phase 6.7 — the feature-default drift check

**Do this every re-pin.** Phase 6.5 asks whether upstream's *code* landed here.
This asks whether upstream's *configuration* did, and the answer has been no for
a long time without anyone noticing.

At pin `02b53fcd8` the pin's `app/Cargo.toml` `default` list had **193** entries;
this fork's had **141**. Fifteen of the missing ones gated real, present,
compiled implementation — `GroupedTabs` (a whole `workspace/tab_group.rs`),
`PinnedTabs`, `QueueSlashCommand` (`blocklist/queued_query.rs`),
`TerminalLifecycleRecovery` (a `terminal/model/lifecycle/` module with its own
tests), `AgentHarness` (which gates every NON-Oz harness, i.e. the Claude Code
and Codex paths a BYOP fork exists to use), and others. None was in
`FORCE_DISABLED_FLAGS`, so none was ever a deliberate BYOP exclusion. They were
lost when the default list was trimmed, and nothing recorded it.

This is the cheapest defect class in the repo: the code is already here, already
compiling, already tested. The fix is a line in a list.

### The method

```bash
# both default lists, sorted
git show <pin>:app/Cargo.toml | awk '/^default = \[/,/^\]/' | grep -oE '"[a-z0-9_]+"' | sort > /tmp/pin.txt
awk '/^default = \[/,/^\]/' app/Cargo.toml   | grep -oE '"[a-z0-9_]+"' | sort > /tmp/fork.txt
comm -23 /tmp/pin.txt /tmp/fork.txt      # on at the pin, off here
```

For each difference, find the flag it gates and decide. A flag is reachable in a
normal GUI build via exactly three paths, and you must check all of them:

1. membership in `RELEASE_FLAGS`;
2. a `#[cfg(feature = "x")] FeatureFlag::Y` entry **where `x` is in `default`**;
3. an `UNSTABLE_FEATURES` name (one entry exists).

### Four traps in this check specifically

- **The flag-init file MOVED.** The pin keeps `enabled_features()` in
  `app/src/features.rs`; this fork has it inline in `app/src/lib.rs`. A script
  that greps only `app/src/lib.rs` against the pin finds **zero** cfg entries and
  reports the pin as having 7 reachable flags instead of 193. That number is the
  size of `RELEASE_FLAGS` — if you see it, your parse failed.
- **Reachable is not visible.** Several flags are ANDed with a user setting.
  `VerticalTabs` is reachable in both fork and pin; the sidebar is still absent
  because `appearance.vertical_tabs.enabled` defaults to `false`. Check the
  setting before declaring anything broken.
- **Most dark flags are NOT the fork's doing.** Of 90 unreachable flags at this
  pin, 69 were unreachable at the pin too — upstream gates those per-account from
  its backend, which this fork removed. Only the 15 that upstream ships ON are
  drift. Do not "fix" the other 69.
- **Some flags are declared but never wired.** Eight of the fifteen had no
  `#[cfg(feature)]` entry at all, so adding the feature to `default` alone would
  have done nothing. Check the entry exists, not just the feature.

### Where deliberate exclusions belong

`FORCE_DISABLED_FLAGS` (`crates/warp_features/src/lib.rs`) — a *hard* disable
that short-circuits `is_enabled()` before any other source. Its doc comment is
the standard to hold to: it is only right for a subsystem that genuinely cannot
exist in a BYOP build, and anything merely off-by-default belongs in the channel
lists "so it stays reachable". It also records a past error of exactly this kind,
where `AgentModeComputerUse` was hard-disabled on the false assumption that
computer use was a cloud capability. **If a flag is off and the reason is not
written down, that is the bug.**

## Traps

Each of these has actually cost time here.

- **An unfetched pin looks like success.** Phase 0. It is first for a reason.
- **The queue is not the work.** It is a triage. A DECLINED COLLISION row means
  *read the decision*, not *port the test*.
- **Verdict A is overstated.** Trace each test's real API dependencies before
  porting. Two independent overstatements are already recorded.
- **`git diff` against `warp/master` is not a re-pin.** Master is not a release.
- **Agents' work left uncommitted in a worktree is lost.** Commit inside the
  worktree before merging anything; this has happened, with unpushed work from
  three agents lost at once.
- **Unbuilt agent output is not verified output.** If a round could not build,
  the report must say so in those words. `docs/build/TRIAGE.md` exists to score
  how often the agents' own confidence predictions were right — feed it.
- **The gap grows at re-pin, by design.** Do not let it read as a regression in
  the status you write.
- **A ported test does not prove a ported fix.** The test comes from the same
  upstream commit and is usually written against the *behaviour*, not the
  mechanism — so it passes whether or not the mechanism landed. Phase 6.5 exists
  because of this; `01778efe` is the worked example.

## What this does not cover

Moving the pin does not touch the fork's *own* identity, packaging, or CI
infrastructure. If a pin move appears to require renaming something in this
fork, check `specs/phosphor-rebrand/MERGE-CHECKLIST.md` first — several
identifiers are deliberately held on their old names because renaming them
silently loses data.
