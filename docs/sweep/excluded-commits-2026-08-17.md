# The 496 excluded commits — scoping report, 2026-08-17

**Status: RECOMMEND CLOSE AS NOT-WORTH-DOING.** A 40-commit stratified sample
across all seven strata produced **zero findings**. The structural reason is in
§2: these are not unported upstream work, they are the fork's own inherited
history.

This file is the artifact the maintainer asked for: the actual list, the
readability split, the population breakdown, the sample with per-stratum hit
rate, and the extrapolation. Nothing here was compiled or fetched; no source
file was edited to produce it.

Oracle pin: `42effe84055f891405b32914af333f14127ec381` (per `ORACLE.md`).
`warp/master` was never used as a comparison basis.

---

## 1. The count is exactly 496 — confirmed

```
comm -13 <(cut -d' ' -f1 fix_candidates.txt | sort) \
         <(cut -d' ' -f1 all_commits.txt   | sort)   ->  496 lines
```

- `all_commits.txt` = 612 lines, 612 unique hashes.
- `fix_candidates.txt` = 116 lines, 116 unique hashes, a strict subset of the 612.
- 612 − 116 = **496**, with no duplicates and no orphans on either side.

`TODO.md`'s "~82" remains fabricated. **496 is right.**

### What the 612 actually are — a scoping correction that matters

The 612 are **not** "all upstream commits". Verified here:

- **All 496 touch at least one of the five owned paths** from
  `count_commits.sh` / `list_subjects.sh`: `app/src/workspace/`,
  `app/src/pane_group/`, `app/src/settings/`, `app/src/settings_view/`,
  `crates/warp_tui/`. (496/496, checked by `git show --name-only` per commit.)
- Their dates span **2026-04-28 → 2026-07-29**, i.e. a ~3-month window
  ending at the *old* pin `02b53fcd8` (2026-07-29 00:14). Nothing in the set
  is newer than the old pin by more than 90 minutes; nothing approaches the
  current pin's 2026-08-11.

So the "496" is a **subsystem-scoped remainder for workspace / settings / TUI**,
not a global backlog. Anyone quoting it as "496 unexamined upstream commits"
overstates its reach by an unknown but large factor.

---

## 2. The crux: these commits are already IN this fork

This is the finding that decides the item, and it is structural rather than
statistical.

Phosphor's clone is grafted at `02b53fcd8` — that commit *is* the fork point.
Every one of the 496 predates it. Their content therefore arrived here by
inheritance, not by porting. Demonstrated end-to-end on `0167b43a8`
("TUI: add blank line and left-align thinking block body", 2026-07-22), one of
the 18 commits whose diff is readable offline:

| where | the line the commit added |
|---|---|
| the commit's own diff | `+ // Left-align the body with the header and separate the two with a blank row.` |
| `02b53fcd8` (old pin) | present, line 173 |
| `42effe840` (current pin) | present, line 172 |
| **this fork, working tree** | **present, line 171** |

`git merge-base --is-ancestor` returns NO for 495 of the 496 — that is the
**graft truncating the walk**, not evidence of a separate branch. Do not read
that command's output here as an ancestry answer.

**Consequence.** A sweep of these 496 cannot find "upstream work we never
ported". The only thing it can find is **de-clouding collateral damage**: a
behaviour one of these commits introduced that the fork's own removal commits
later broke. That is a regression audit of the fork's edits, and it is a
strictly different — and much narrower — question than the one the item's title
implies.

---

## 3. Readability in this shallow clone

The clone is shallow (graft at `02b53fcd8`) *and* a `blob:none` partial clone
(`remote.warp.promisor=true`). Those two facts split readability three ways.
All measurements below were taken with `GIT_NO_LAZY_FETCH=1`, which guarantees
no network access.

| what | count | note |
|---|---:|---|
| commit objects present | **496 / 496** | `git cat-file -e` succeeds for every one |
| **file lists** readable offline | **496 / 496** | `git show --name-status` works — tree objects are local |
| **full diffs** readable offline | **18 / 496** | the other 478 need a promisor blob fetch |
| blobs at **both pins** readable offline | yes | `git show 42effe840:<path>` and `02b53fcd8:<path>` both work |

So the honest split is: **nothing is beyond the graft, but 478 of 496 diffs are
beyond the blob horizon.** `git fetch --unshallow` is *not* what is needed —
`git fetch --refetch` / a blob backfill is. The 18 fully-diffable commits are
listed in §7.

**What this cost the analysis.** For 478 commits I could not read what the
commit changed. I substituted a stronger-in-some-ways check that the local
objects do support: compare the **pin's** version of each touched file against
**this fork's** version, then probe for the commit's distinguishing symbol on
both sides. That answers the parity question directly, but it is weaker for
files where the fork has diverged massively — `app/src/workspace/view.rs` alone
differs from the pin by 13,696 lines, so "the feature is in there somewhere"
rests on symbol-level evidence rather than diff-level evidence.

---

## 4. Population breakdown

### 4a. Subject-line convention — mostly absent

Warp does not use conventional commits consistently. Only **44 of 496** carry a
`type:` prefix at all:

| prefix | n |
|---|---:|
| `feat:` / `feat(...)` | 30 |
| `tui:` | 8 |
| `refactor:` | 3 |
| `warpui_core:`, `spec:`, `client:` | 1 each |
| **no prefix** | **452** |

Grouping by prefix is therefore useless here. The strata below are built from
subject semantics plus the areas each commit touches.

### 4b. Strata

| stratum | n | share | mean files | could it carry a user-visible defect *here*? |
|---|---:|---:|---:|---|
| **S1** cloud / accounts / billing / orchestration / shared sessions | 184 | 37% | 74 | **No.** Out of scope by `DECLINED.md` §"Cloud". The code these commits touch is deleted here. |
| **S2** `warp_tui` (local TUI) | 138 | 28% | 13 | **Yes — highest.** The fork ships `warp_tui` and it is user-facing. |
| **S3** tabs / tab groups / panes / window chrome | 58 | 12% | 4 | **Yes.** Small, local, visible. |
| **S4** AI / agent / models (non-cloud subject) | 35 | 7% | 12 | **Partly.** Several land on the BYOP boundary already adjudicated in `DECLINED.md`. |
| **S5** refactor / rename / move / removal | 28 | 6% | 34 | **Low by construction** — no behaviour change intended. |
| **S6** build / platform / performance | 3 | <1% | 32 | **Low.** |
| **S8** other local (editor, settings, terminal, notebooks, SSH) | 50 | 10% | 10 | **Yes.** |
| **total** | **496** | | | |

### 4c. Size

| files changed | n |
|---|---:|
| 1–2 | 76 |
| 3–5 | 93 |
| 6–15 | 171 |
| 16–50 | 138 |
| 51+ | 18 |

The 184 S1 commits are also the largest (mean 74 files) — reading the population
by commit count understates how much of the *reading* would be spent inside the
dropped cloud layer. By file-touch weight, S1 is closer to 60% of the work.

**Two different flavours of near-zero yield, as the brief predicted.** S1 is
near-zero because its subject matter does not exist in this fork — the files
are gone. S5/S6 are near-zero because they are behaviour-preserving by
intent. Only S2/S3/S8 are near-zero for the boring reason: the fork already has
them.

---

## 5. The stratified sample — 40 commits, 0 findings

Sampling: deterministic (`random.seed(20260817)`), drawn without replacement
within each stratum, weighted **toward** the strata judged most likely to yield
(S2 got 7.2% of its population sampled vs S1's 3.3%).

Per commit: file list from `git show --name-status`; every touched file
classified as present-in-fork / absent-in-fork; for in-scope files, the
commit's distinguishing symbol probed on both the pin side
(`git show 42effe840:<path>`) and the fork side.

### Per-stratum hit rate

| stratum | population | sampled | coverage | **findings** | hit rate |
|---|---:|---:|---:|---:|---:|
| S1 cloud | 184 | 6 | 3.3% | **0** | 0% |
| S2 warp_tui | 138 | 10 | 7.2% | **0** | 0% |
| S3 tabs/panes | 58 | 6 | 10.3% | **0** | 0% |
| S4 AI local | 35 | 5 | 14.3% | **0** | 0% |
| S5 refactor | 28 | 4 | 14.3% | **0** | 0% |
| S6 infra | 3 | 3 | 100% | **0** | 0% |
| S8 other local | 50 | 6 | 12.0% | **0** | 0% |
| **total** | **496** | **40** | **8.1%** | **0** | **0%** |

### The sample, with what was actually checked

**S1 — cloud (6/6 out of scope, nothing applies here)**

| commit | subject | verdict |
|---|---|---|
| `e59fb690b` | Update copy/styling for local→cloud handoff toast | Cloud handoff. `DECLINED.md` §Cloud. |
| `0f4bb5928` | Blog link for orchestration launch modal | `app/src/workspace/view/orchestration_launch_modal/` absent here by design. |
| `7aa162504` | Orchestration pill bar for shared-session viewers | `terminal/shared_session/viewer/orchestration_viewer_model.rs` absent; shared sessions declined. |
| `1388531b1` | [REMOTE-2149] runner config to orchestration | 14 of its 35 files are `app/src/ai/orchestration/*` + `app/src/server/server_api*`, all absent by design. |
| `f652d6dfe` | Disable handoff for orchestrated agents | Cloud handoff. |
| `1259cbf29` | Cloud agent env name + setup status in vtab pwd | Cloud agent metadata. |

**S2 — warp_tui (10/10 present and intact)**

| commit | check | result |
|---|---|---|
| `fa0d6fc85` | TUI warping indicator | `crates/warp_tui/src/warping_indicator.rs` + `_tests.rs` both present. 24 of its `warpui_core` files are **byte-identical** to the pin. |
| `3f70bfc96` | Disable NLD in slash command | `input_detection.rs` — 7 `slash` sites at pin, 7 in fork; `input_detection_tests.rs` byte-identical. |
| `659cf3747` | Reusable TUI editor view | `editor_view.rs`, `editor_interaction.rs` present (4 and 14 line delta). |
| `43cf43c06` | `CrossAxisAlignment` for TUI flex | 14 sites at pin, 14 in fork; `flex.rs`/`flex_tests.rs` byte-identical. |
| `4207f5667` | Style shell commands in TUI | `terminal_block.rs` differs by 1 line; `tui_builder.rs` present. |
| `4b77c4de2` | Reusable TUI tab bar | `tab_bar.rs` — 71 `TabBar` sites at pin, 71 in fork. |
| `fb22b1920` | Centralise session input blocking | `terminal_session_view/state.rs` — 45 `block` sites at pin, 48 in fork. |
| `d7a44d435` | Improve user-facing TUI naming | Present but intentionally **DIVERGENT** — this fork rebrands. Not debt. |
| `e712486bd` | Dedicated `/status` menu | `SlashCommandKind::Status` and `TuiReadOnlyMenuKind::Status` both live in `terminal_session_view.rs`. |
| `1d9d6f3ce` | "(recommended)" on ask-question option | `tui_ask_question_view.rs` — 1 site at pin, 1 in fork. |

**S3 — tabs / panes (6/6 present)**

| commit | check | result |
|---|---|---|
| `b802cdf57` | Group-aware pane drag for htabs | `app/src/tab.rs` — 12 drag/group sites at pin, 11 in fork; `view.rs` carries the group-aware `on_tab_drag` doc-comment verbatim. |
| `981cb1c7d` | Dragging actions for htab groups | same evidence. |
| `984a88962` | Persist tab group on app close | Migration re-dated but present: fork `2026-08-05-000000_add_tab_groups` vs pin `2026-06-01-000000_add_tab_groups`; `tab_groups` table in `schema.rs`, `tab_group_row_ids` in `sqlite.rs`. |
| `ed9177539` | Reserve titlebar space for right-side window controls | `TrafficLightSide::Right` guard present at two sites on both sides. |
| `51dae19e9` | tab_config window max height + scroll | `menu.rs` scrollable-menu structure matches the pin. **Weakest verification in the sample** — see §6. |
| `28c9c7d0d` | Mark focused-pane notifications read on window re-focus | `model.mark_items_from_terminal_view_read(terminal_view_id, ctx)` present (`workspace/view.rs:4648`; pin `:5399`). |

**S4 — AI local (5/5; 2 explicitly declined, 3 present)**

| commit | check | result |
|---|---|---|
| `2d8587373` | Auto-queue prompts until end of LRC | Present, with a fork note in `settings/ai.rs:606` that `AutoQueue` is kept with different backing semantics — a recorded divergence, not a gap. |
| `9fd0e8874` | UI for custom model routers | **DECLINED** — `DECLINED.md` #404: `FeatureIntroId::CustomModelRouter` promotes a Warp-hosted router this fork never had. |
| `20430b8a2` | Scrollable model section, custom inference modal | **DECLINED** — `DECLINED.md` #142/#347: `CustomEndpoint` superseded by `AgentProviderSecrets`. |
| `1148ae3e8` | Wake up remote Claude Code agents on new events | Cloud half (`server_api/*`, `agent_sdk/ambient.rs`) absent by design; harness half present (`claude_code.rs`, `parent_bridge.rs`). |
| `63fe72858` | Proper codex plugin | `cli_agent_sessions/plugin_manager/codex.rs` + `codex_tests.rs` present. |

**S5 — refactor (4/4, nothing to find)**

| commit | result |
|---|---|
| `a19bf168c` | SSH install setting via macros — `terminal/warpify/settings.rs` present, 30 sites vs pin's 27. |
| `09a35b58a` | `DiffStateModel`/`CodeReviewView` decouple — present, 100 sites vs pin's 82. |
| `71054d652` | Remove `NotAmbientAgent` — symbol absent on **both** sides; the removal is in. |
| `b5c64ff4d` | `_test.rs` → `_tests.rs` rename — the fork deliberately went the other way (`*_test.rs`). Known, documented in `ORACLE.md` §"Rules for anyone re-measuring". |

**S6 — infra (3/3)**

| commit | result |
|---|---|
| `0ac6f5948` | TUI startup speedup — touched files present; `add_summary_to_agent_conversations` migration present under the fork's own date. |
| `ddafc51ab` | Heap-profile command-palette action — `WorkspaceAction::DumpHeapProfile` live at 4 sites (`action.rs:374`, `:897`, `mod.rs:249`, `view.rs:20848`). |
| `e6098a8ae` | FreeBSD build — `freebsd` cfg present, 1 site each side. |

**S8 — other local (6/6)**

| commit | result |
|---|---|
| `13e8b6114` | `.ipynb` rendering — present (`openable_file_type.rs`, `notebooks/file/mod.rs:356`). |
| `0d24d2cff` | Reuse existing SSH ControlMaster setting — present, `settings/ssh.rs:20`, same `toml_path`. |
| `ba5dcd90e` | `FilterableDropdown` `SelectActionAndClose` trait — present, 3 sites vs pin's 2. |
| `23f00966c` | Setup-branches RPC — cloud RPC half (`code_review/diff_state/{remote,local,mod}.rs`) absent by design; local `diff_state_tracker.rs` present. |
| `164e60e42` | OSC 52 clipboard setting + banner — present and **ahead**: fork's `Osc52ClipboardAccess` carries schemars descriptions the pin lacks. |
| `0974a0f0c` | Local continuation for third-party harnesses — present, with an in-code note at `claude_code.rs:151-166` explaining that `--resume` is ported verbatim and only the cloud-only resume-payload fetch is absent. |

---

## 6. Extrapolation, confidence, and limits

**Point estimate: 0 findings from a full sweep of all 496.**

**Statistical bound.** 0 hits in 40 draws gives a one-sided 95% upper bound on
the overall hit rate of `1 − 0.05^(1/40)` ≈ **7.2%**, i.e. at worst ~36 findings
across 496 if the true rate sat exactly at that bound. Per-stratum bounds are
much looser and should be quoted as such:

| stratum | sampled | 95% upper bound on hit rate | worst-case findings in stratum |
|---|---:|---:|---:|
| S1 cloud | 6 | 39% | 72 |
| S2 warp_tui | 10 | 26% | 36 |
| S3 tabs/panes | 6 | 39% | 23 |
| S4 AI local | 5 | 45% | 16 |
| S5 refactor | 4 | 53% | 15 |
| S6 infra | 3 | 63% | 2 |
| S8 other local | 6 | 39% | 20 |

**Do not lead with those bounds.** They are what 40 draws buys *if the only
evidence were the draws*. The dominant evidence is §2 — the commits are the
fork's own inherited history, so the base rate is not "unknown feature work"
but "did our own removals break something". Three independent measurements
already bound that:

- `docs/STATE.md`: `MISSING-SUBSYSTEM` = **69 tests**, and **0** genuinely-open
  unadjudicated tests.
- `docs/sweep/warpui-coverage-2026-08-17.md`: the `warpui`/`warpui_core` census
  found **0 PORTABLE** of 7 absent pin tests, and the fork *ahead* on test count
  (734 vs 696).
- This sample: 24 of the `warpui_core` files touched by one sampled commit are
  **byte-identical to the pin**.

**Limits, stated plainly.**

1. **478 of 496 diffs were not read** — blobs are not local and fetching is
   forbidden. End-state parity against the pin was substituted. For small,
   low-divergence files (most of S2/S3) that is nearly as good; for
   `app/src/workspace/view.rs` (13,696 lines of divergence) it is materially
   weaker.
2. **Marker selection is judgement.** For the 478 I chose the symbol to probe
   from the subject line. A commit whose real payload was a behaviour I did not
   think to name could pass this check. `51dae19e9` is the one sampled commit
   where I could not construct a marker I trust.
3. **40 of 496 is 8%.** A stratum-specific defect concentrated in the 92% I did
   not read is not excluded by this work.
4. **Subject-based strata are approximate.** The classifier is a keyword
   regex; two false positives were found and fixed by hand (`#12327` warpctrl,
   `#13602` GUI/TUI slash-command sharing, both matched on "auth"/"share" and
   are local). Others of the same kind likely remain inside S1.
5. **This sample cannot speak to the 116 fix-flavoured commits** or to the 44
   still-unverified PARTIAL candidates. Those are a different, pre-filtered,
   evidence-bearing pile.

---

## 7. Recommendation

**Close the item as not-worth-doing. Do not queue a full sweep, and do not
queue a targeted sweep of any stratum.**

Justification from the measured rate, not intuition:

- **0 findings in 40 commits**, across all seven strata, including 100% coverage
  of S6 and double-digit coverage of S3/S4/S5/S8.
- **37% of the population (S1, 184 commits, mean 74 files each) is unreadable-by-design**
  — it edits files this fork deleted. That is not a low hit rate, it is a
  structurally empty set, and by file-touch weight it is ~60% of the reading.
- **The remaining 62% is inherited code the fork already has**, confirmed by
  byte-identical files against the pin in the highest-weighted stratum.
- The item's premise — "unexamined upstream commits" — does not hold. These are
  not upstream-ahead-of-us commits. There is no port to do.

**If the maintainer wants any residual coverage anyway**, the cheap version is
not a sweep. It is: fetch blobs for the **18 locally-diffable commits** already
listed below plus any S2/S3 commit whose touched files diverge from the pin by
more than ~200 lines, and read only those. That is a couple of dozen commits,
not 496.

**What to spend the time on instead** (both are pre-filtered and
evidence-bearing, unlike this pile):

1. The **44 unverified PARTIAL candidates** in
   `docs/sweep/artifacts-2026-08-15/partials_clean.txt` — already carry per-file
   `OK:`/`MISS:` evidence.
2. Re-deriving the `SCOPE-*.md` classification at `42effe840`. `ORACLE.md` says
   explicitly that 1,605 / 3,902 / 854 are stale numbers measured at the old pin.

### The 18 commits whose full diff is readable offline

```
0167b43a8695  TUI: add blank line and left-align thinking block body (#14108)
02b53fcd81ac  Remove process-global current team APIs (#14393)          [= the old pin itself]
0d24d2cffaf4  Add setting to enable reusing user's existing control master (#12465)
14205aa353b6  Migrate DetectedRepositories to include remote backing repos (#10921)
14c8c8ded42e  Add proper support for host scoped requests (#12036)
2dbf25fe4493  Hide Dock icon when using dedicated hotkey window (GH-1154) (#9926)
389716a905d4  Add client-side user setting for agent commit attribution (#9323)
3bd21f82f785  Enable cmd-O and @ context on remote SSH session (#11295)
57062bd92031  Remove TMUX based SSH warpification flow (#12478)
5becb10b8981  Migrate app crate macOS code to objc2 (#11669)
79df58226766  Initialize privacy settings from `WarpDrivePrivacySettings` (#9438)
b3187d02804a  Migrate code review entrypoints for remote paths (#11026)
c325d146ab31  Update agent attribution setting (#9329)
cc9ef06a2a74  Add horizontal scrolling for orchestration pill bar (#10957)
d24408ff2521  Pin conversation list kebab button to the right side of items (#9245)
df02914a691b  Remove session id from `RemoteDiffStateModel` (#11790)
e6098a8aef18  Get warp compiling on FreeBSD (#9362)
f658c30b576c  Add icon collage to vertical tab collapsed group (#12386)
```

---

## 8. The full list of 496, grouped by stratum

Format: `<12-char hash>  <files changed>f  <subject>`. Regenerate the raw list
from the versioned artifacts with the `comm` line in §1.

### S1 — cloud / accounts / billing / orchestration / shared sessions — 184 commits

```
23eedf45ec4a   24f  Add 'continue' (in cloud) button to cloud agent conversation tombstone. (#9315)
086150b87856   18f  Add IAP (Identity-Aware Proxy) support for staging (#11729)
05026636f568   41f  Add TUI cloud orchestration children (#13855)
a66337f4faed   14f  Add TUI logout slash command (#14117)
132800ecb8bb   18f  Add TUI orchestration conversation tab bar (#13832)
5767910b5e41   84f  Add `./script/format` for customized `cargo fmt` invocation. (#11747)
5b8a3758e549   28f  Add account-first onboarding (flag-gated): pre-auth flow, offer UI, and post-auth routing (#14075)
9d2296d14a05   10f  Add agent CLI flag for cloud runs (#9935)
10141bf51f0e   15f  Add agent_identity_uid support for remote child agent execution (#13773)
98aaece54434   19f  Add auth secret dropdown picker for non-Oz orchestration (#10885)
e09838dfa105   13f  Add auto-handoff sleep modal and success toast (#12514)
1a04c2a0e810    4f  Add balance section and addon credits section to billing and usage page (#10773)
0f4bb5928f06    1f  Add blog link for orchestration launch modal. (#11196)
389716a905d4   11f  Add client-side user setting for agent commit attribution (#9323)
a4fc5b3e8a0f   25f  Add configurable per-profile context window setting (#9352)
a6b677247d9b    6f  Add dismiss button to shared object limit banner (#13620)
2249469e5d24    1f  Add docs link for agent API keys (#12024)
2afc4b08b1af   11f  Add environment setup flow for local -> cloud handoff (#10401)
f95364ac217d   21f  Add harness availability model to the client. (#10135)
cc9ef06a2a74   12f  Add horizontal scrolling for orchestration pill bar (#10957)
3d940d78a78d    4f  Add host label for remote repos and files (#11423)
936a2edff524   19f  Add model reasoning level for codex. (#11101)
72751748f5dd    8f  Add one-time migration to add handoff chip to custom toolbar layouts (#10614)
b37688958b12    9f  Add orchestration create environment modal (#10857)
21a8ae477835    6f  Add orchestration message display setting (#12219)
55b411ec694a    5f  Add paste-the-code fallback for SuperGrok (xAI) OAuth login (#12599)
0e3f9fb98286   16f  Add per-agent model_id override support to run_agents (#14130)
b9930c9060b0   24f  Add pinning to orchestration pill bar (#10777)
0112f79d65bb    2f  Add reload credit confirmations for team actions (#11366)
c4cd0c4419a5   18f  Add run_agents profile permission (#11225)
2ae6afc3910b   20f  Add settings gates for local -> cloud handoff (#10492)
4ea8a1fb4d83   18f  Add shared session QR code flow (#11435)
1bb368b6ad3e   15f  Add sleep auto handoff to cloud (#11049)
e4ab1c537efd   12f  Add stop and kill actions for orchestration pills (#10526)
38703bca723c   18f  Add support for authenticating against Grok subscriptions.  (#12028)
7056eac00669    2f  Add telemetry for SuperGrok subscription connect (success/failure) (#12513)
203f34a4a083    7f  Add telemetry for local-to-cloud handoff (#10848)
912e0531c668    1f  Adjust orchestration launch modal overlay background (#10753)
ed455d902b13    1f  Allow for local -> cloud handoff when requests are in-progress (#10860)
2c38e1fd6011   18f  Auto-install Codex orchestration plugin for harness runs (#11892)
5344b59791d0   12f  Auto-select environment on `&` entry, not on handoff submit (#10646)
aea265bf396e    4f  Billing & Usage Page Blank Canvas (#10701)
d37e7a8ccf42    4f  Billing v2: drop misleading per-row base-limit denominator (#11910)
53d47d15dbf8    8f  Canonicalize local conversation ID mapping to task ID for cloud agent tasks. (#10801)
693f59feb6ed    2f  Center every TUI login line across all substates (#14150)
027172c71f58    7f  Change handoff conversations to use "<original conversation title> (Moved to cloud)" for the new conversation title (#10603)
185862d804ea    2f  Clarify Oz/warpctrl CLI install command palette labels (#13155)
11e217e9d146   32f  Clean up old, unused orchestration tools. (#14174)
69bb47708880   42f  Clean up orchestration rollout flags (#11908)
a237521d76bb   28f  Client: dev-only context-window segment breakdown in usage card (#12646)
9e2015d7f0d5    5f  Count connected Grok subscription as available AI (#12831)
50c542564324   21f  Decouple `IapManager` from depending directly on the `warp` crate. (#12886)
f652d6dfe07b    7f  Disable handoff for orchestrated agents (#11147)
2be6f35d6397    1f  Do a small refactor of the addon credits panel state and update the autoreload UI for non-admins (#11300)
bfdc42feb060    4f  Do not include cloud agent metadata on passive suggestions requests (#11400)
f98dbbc5474d    8f  Don't fail to deserialize cloud environments without a docker image (#13553)
3bd21f82f785   15f  Enable cmd-O and @ context on remote SSH session (#11295)
9bd47fd2903c   11f  Enable global search in remote sessions (#12477)
52a708bdff4b   21f  Enable handoff for orchestrating (or orchestrated) agents (#11768)
af64c3107807   33f  Extract `AuthClient` and associated logic to the `warp_server_client` crate. (#11977)
a6d9b93ae518   89f  Extract `CloudObject` client and models to separate crates. (#11166)
c1b1d4bf75cd   28f  Fork conversation into local->cloud pane on-pane-creation (#9653)
dbde9bcda4a7    6f  Gate onboarding AI on a Warp account; rework login screen (PR-D) (#12992)
f41b4e0a96bb    3f  Hide admin panel link for non-enterprise billing pages (#11414)
effa8ef9f530   11f  Hide child (orchestrated) agent conversations from the conversation list (#13059)
48b42d12284d    3f  Hide zero-credit buckets from Billing & Usage legend (#13181)
2a682235e362    2f  Highlight the currently selected period in the Billing & Usage dropdown (#13551)
fc2dfe97134d   17f  Implement auth secret deletion from selector menu (#11241)
723bdf1480c3   20f  Implement consistent tombstone and follow-up visibility behavior for cloud agent conversations. (#10939)
91771443a66e    6f  Improve TUI orchestration tab-bar keybindings (APP-4903) (#14064)
95518310bfe4   17f  Initial codex CLI harness setup (#9370)
79df5822676b    1f  Initialize privacy settings from `WarpDrivePrivacySettings` (#9438)
df1a5e8b0136   18f  Inline create-API-key flow on orchestration cards (#11124)
1df06fe5a627    7f  Keep selected account-backed toolbelt entries visible (#14175)
fb2d3ae169a4   18f  Make auth flow more reliable (#14357)
6691e1e0e040    3f  Make the New API key modal's Agent picker searchable (#12972)
b3187d02804a   13f  Migrate code review entrypoints for remote paths (#11026)
c83e1efa44a8   27f  Migrate editor to use remote backed buffer (#10520)
883f22b00506  345f  Migrate most `log::error!` calls to`report_error!` (#13483)
a76823c88982   34f  Move `GenericCloudObject` to `warp_server_client`. (#11115)
dc408d2cb47a   18f  Move `GenericServerObject` to `warp_server_client`. (#11114)
b4191bb35e01    8f  Move orchestration hint into TUI input (#14151)
abea51cd1e10  1893f  Move workspace crates over to the Rust 2024 edition. (#13990)
7aa162504e17   14f  Orchestration pill bar for shared session viewers (#10890)
dcc4cbac9ed0   17f  Orchestration pill bar updates: same-pane pills, 3-dot menu, hover card, breadcrumbs (#9680)
c63c0dce986f    6f  Proper TUI initialization for sentry and telemetry flushing (#13794)
131762b08e4b   23f  QUALITY-671: roll up orchestration credit usage in the agent-mode footer (#11048)
8f883075f613    6f  QUALITY-715: don't auto-open details panel for parent-orchestrated child agents (#11055)
81d349656202   35f  QUALITY-726: session sharing for orchestrated agent sessions (#11465)
44ed32abc290   10f  QUALITY-731: round-trip orchestrator agent short name through task records (#11090)
30a788873b64   40f  QUALITY-780: client-side handling of wait_for_events yields (#12084)
7f5a68932daa    8f  REMOTE-1601 Add named agent API key support behind FeatureFlag::NamedAgents (#10390)
e9c6fd09cedd    4f  REMOTE-1738 Add search to Oz API keys (#11307)
3457feef2f78    2f  REV-1593 Update teams page seat-limit upsell copy (#11288)
f7e19b5edd47   12f  REV-1595 [5/n] Render per-user/team/own rows in cycle usage section (#11123)
3ede77814d4c   26f  REV-1625 C2: rework onboarding to support no AI on free (#12551)
4f133a634950   10f  REV-1625 C3: free AI removal notice + Prompt Suggestions modal (#12552)
385b2a90e805   25f  Re-enable local Claude Code orchestration (#11571)
dcf985db615e   28f  Refactor AI query cloud routing, handle session end, and add sandbox connection status to input footer (#13340)
afc8b55dcb47   12f  Refactor orchestration pill bar swap to use replace_pane (#10327)
21b7b6427d37    2f  Remove "open repo" cta for remote sessions (#11299)
457ffadd37e7   15f  Remove OrchestrationPillBar and OrchestrationViewerPillBar feature flags (#12255)
169767c6d5d4    4f  Remove free-tier telemetry requirement for AI (#12655)
d3d0b95fd918   12f  Remove legacy "Setup Guide" onboarding flow (#13036)
eca8c1a60131   11f  Remove logging of 13 unused telemetry events (#13847)
02b53fcd81ac  6115f  Remove process-global current team APIs (#14393)
42cb22bb957e   14f  Remove prompt requirement from cloud run initiation (#11573)
c88f6b4bc786   11f  Remove unused free-tier limit-hit modal (#13494)
7225e824b801    1f  Rename Cloud Agent tab menu item (#10886)
ab085501f51c   13f  Replace CustomInferenceEndpoints feature flags with BYO_ENDPOINT billing policy (#12783)
633a8f17e9ae   28f  Replace Oz armadillo icon with white Warp logo app-wide (#14344)
c59a0f37be3d   20f  Report Oz run failure instead of "Cancelled by user" when the agent's command exits the shell (#13210)
1c857150033f   14f  Respect team BYO policies in client model UI (#13476)
0a0e9de39708   19f  Restore ambient agent conversations into cloud mode panes, supporting continuation (#10426)
4aea06734ea8  2374f  Run cargo fmt to clean up imports. (#11491)
d6788cbe50de    5f  Settings → Team: add "Your team is full" alert above invite header (#10705)
1259cbf293b4    4f  Show cloud agent env name and setup status for vertical tabs pwd (#11006)
5abd4233bf67    4f  Skip auto-handoff-on-sleep for orchestrator sessions with local child agents (#13211)
aa2ac33074d8    3f  Skip onboarding UIs in SDK/headless mode (#9590)
7f1df7a49c9f    3f  Snapshot changes for handoff, even when the conversation is empty (#10576)
6c4125ce193e    6f  Split pane_group orchestration code into submodules (#12039)
8e1b7e9a50ba   18f  Support remote project rules through repo metadata standing results (#11460)
d9b50a20b4ea   24f  Surface disjoint inference / platform credits in conversation usage (#11441)
a99252686a3b    1f  Surface more descriptive errors for local-to-cloud handoff failures (#11012)
a0d589460b58    3f  Take into account bootstrap stage when showing input (#13901)
ba40a024df99   10f  Update Git Operations AI client billing policy (#9840)
d194b4c7f1a0    1f  Update add-on credits billing v2 UI (#11346)
c325d146ab31    1f  Update agent attribution setting (#9329)
e59fb690b2ba    1f  Update copy and styling for local -> cloud handoff toast (#10641)
27ff15b50136   25f  Update handoff surfaces to allow for empty prompts (and auto-handoff on click) (#11576)
e75bf8098be0   27f  Update orchestration message transcript UI (#10285)
1fb73811558b   18f  Update telemetry for code review over remote sessions (#11484)
4e99f5f25447    2f  Use server forking endpoint for local conversation forking (#10907)
edef7f83fd18   15f  Use server run_time for agent tasks (#11431)
a7b26f5aac7f    4f  Use theme and terminal colors in orchestration launch modal (#10656)
6a48ae73e7c5    2f  Use white color for orchestration launch modal close button text (#10669)
151ef9e568bd    5f  Warp credit fallback for custom endpoints (#10892)
ba7735d0d881   19f  When opening a cloud agent convo, attach to existing session if there is one. (#11097)
a79721a5c781   51f  [1/5] Support remote project skills (#11459)
dc2eb9bc47bb    6f  [1/5] [Remote codebase indexing] pass embedding configs through (#10981)
23c908949da7    7f  [3/n] Billing & usage dispatcher refactor (#11118)
b9ee28cc4e45    3f  [4/n] Add billing cycle usage section scaffold to v2 page (#11119)
be5b39ae7586   19f  [5/5] Handle remote codebase auto-index follow-ups (#11092)
fd8e0fbfc5f9   12f  [APP-3106] Client: preserve user query modes in CloudMode (#9528)
3b2cb79a53d2   30f  [APP-3792] remote codebase indexing: client remote indexing + search codebase tool gating (#10697)
0230312175db   17f  [APP-3792] set up sqlite for remote codebase indexing caching (#10425)
fa8d921575a0   11f  [CODE-1831] Add a credits/cost usage entry to the TUI footer (#13492)
a5c11c70bfcf    3f  [CODE-1832] Show the last response's duration and credits in the TUI indicator slot (#13501)
16a4726dc3a2    4f  [CODE-1860] Resume TUI session after login (#13780)
888c30278e02   39f  [QUALITY-569] Stage 1: orchestrate tool (client) (#9628)
8e837a0ffd84   32f  [QUALITY-569] Stage 2 Client: OrchestrationConfig on Plan Card + Auto-Launch + Disabled Card (#9927)
2fd4a785ff8c   15f  [QUALITY-719] Hide legacy orchestration setting for v2 (#11035)
cc00641746db    6f  [QUALITY-726] Inherit-share + eager Oz task creation for run_agents local children (#11042)
131481923009   10f  [REMOTE-1318] Merge org and user command denylists with per-row editability (#9683)
3d7e074e65fe   11f  [REMOTE-1486] Add cloud handoff snapshot upload (#10102)
fda540595bfb    5f  [REMOTE-1702] Gate DetectedRepositories usage for WASM (#11028)
1388531b18eb   35f  [REMOTE-2149] Add runner config to orchestration (client) (#13896)
cbe8a7535e2e   15f  [REV-1599] Add Gemini Enterprise (GEAP) credential recovery UX (#12684)
7bebcd7c5ced   16f  [REV-1599] Handle Gemini Enterprise credential failures (#14141)
ec3006c83de9    7f  [REV-1599] Sync Gemini Enterprise federation config to clients and add the member enablement gate (#12537)
e6d8aee3c898    5f  [REV-1603 & REV-1606] Show billing V2 for solo free users (#11434)
98d933cea877    1f  [REV-1604] Remove managed auto-reload tooltip (#11433)
8687ea89ae4b   18f  [SAL-55] Add orchestration launch modal with placeholders (#10382)
756586ff99b4   23f  add & entrypoint for local -> cloud handoff (#10271)
4b33a6a78f90   24f  add TUI orchestration permission and configuration card (#13717)
3ffa815d5e2d  141f  add emitter_handle parameter to model subscriptions (#12767)
eef504ac8dc9   18f  add grok sub byok support for TUI (#14420)
7f4e11136114   18f  add nld decision telemetry event (#10875)
6b1e57e2767a    1f  don't fork in the cloud for fork-from (#11070)
da06b312c80c   32f  feat(tui): add local-to-cloud handoff (#14208)
6479524020d9    2f  feat: REMOTE-967 add resizable columns to api keys page (#10519)
4a2678d10995   11f  feat: harness-specific model selection in orchestration config (QUALITY-643) (#10491)
876b840c74e4    6f  feat: make /app redirect to cloud agent in wasm (#11781)
8006b6ee3eee    7f  feat: make cloud credits banner dismissible (#11975)
4c3c95a7c81e   11f  feat: retry Cloud Mode initial runs after GitHub auth (#10973)
cf5ebea44bbc   11f  feat: support local→cloud handoff snapshot in remote SSH sessions (#11453)
6cfb37da7a2b   10f  feat: surface MCP tool + server identity in confirmation card and invocation states (#14298)
74bdbd1d08ab   23f  implement basic local cloud handoff UI (#9455)
cf3ad092ff5b  360f  import warp_errors directly instead of chain of re-exports (#13523)
4e600af4ab49    4f  open local->cloud mode conversation in the same pane (#9988)
a2001a0b04b5   17f  refactor: extract shared local-to-cloud handoff pipeline (#14207)
b04a0c2a480c   22f  refactor: share cloud environment catalog across frontends (#14247)
939988876176    1f  update subscriber in app/src/workspace/one_time_modal_model.rs for th… (#12805)
59fc1a94469a   23f  use multi-harness cloud agent icons + status (#9263)
```

### S2 — warp_tui (local TUI) — 138 commits

```
358ba8e5d544   26f   add generalized virtualized viewport element (#13177)
398374758cc8   15f  A few polishes to TUI (#13576)
8253f4894804    4f  Add /cost slash command to the Warp TUI (#14120)
26b7c9cdbd01    6f  Add /exit slash command and ctrl-d exit to the TUI (#13915)
ce5a0c5ef522   21f  Add AskUserQuestion support to the TUI (#13830)
336af30517d2   17f  Add Ctrl+Shift+P plan toggle to TUI (#13781)
7c8362b27f62    9f  Add NLD support to TUI (#13826)
41e9fe4a6aa0   21f  Add TUI conversation management (#13662)
7998f4cbfd2e   18f  Add TUI model selector (#13690)
6ecbf0f6d5fb   15f  Add TUI shortcuts menu (#14266)
799727885417   14f  Add TUI skills browser (#13739)
a2c17600939a    5f  Add TUI slash command query state (#13486)
890fc81206e9    6f  Add TUI statusline configuration picker (#14286)
aab0e50a8e3b   49f  Add TUI-local file-backed execution profiles (#13886)
dcd7494aabae   33f  Add V0 MCP implementation (#13744)
f971dd32c4a2    3f  Add autoupdate status text (#13569)
230a9f379ae7   25f  Add basic LRC support (#13723)
f1547fefcfcf    1f  Add bottom padding below TUI footer/prompt (CODE-1878) (#13850)
9edb612daa99    7f  Add clipboard shortcuts to TUI editors (#14170)
b2980b099ef9    3f  Add daily dev macOS bundling for the Warp TUI (#13293)
960c3ec866f2    7f  Add editor-backed code blocks for TUI (#13729)
615e47606677   25f  Add image support in TUI (#13908)
1dcbfdf463d7   24f  Add local child agents to TUI (#13776)
a77348c670c0   17f  Add low-effort slash commands to TUI (#13603)
6219184bd669    5f  Add manual TUI attachment for running commands (#14375)
b26cd0a484a9    8f  Add mouse event handling to TUI input (#13256)
fc7e15fa020f   15f  Add per-tool-call labels for TUI tool calls (#13418)
959e78b2d3ee   24f  Add permissions request for generic TUI tool calls (#14019)
da447b4880bf    6f  Add reusable Markdown rendering for TUI (#13728)
86dfca99cf45   30f  Add selection to tui transcript view (through new generic selectable trait) (#13566)
34c909f7de72   29f  Add shell command input mode + execution to TUI (#13406)
79a0e04c8cea   17f  Add shell command tab completions to TUI (#14171)
a376af91568e   39f  Add shell commands to the TUI up-arrow history menu (CODE-1906) (#14192)
c0d7688fde34   16f  Add should_persist to stop_recording (client) (#14194)
bef75a781aa7    5f  Add simple TUI zero state (#13550)
a6ebba40e655    2f  Add spacing between TUI response footer and inline menu (#14058)
a9099599e9f2   31f  Add support for BYOK in the TUI (#14333)
0b5adc0536e4   16f  Add surface-agnostic conversation list policy (#13669)
4cf1a4fa888a   13f  Add terminal theming probe (#13542)
7edf88f823e3    6f  Add zero state animation for TUI (#14139)
6ab529be0403    6f  Align TUI Agent failure warnings with GUI behavior (#14092)
b43821d0a090   20f  Align TUI slash command menu styling (#13604)
6b1d9db7a6f1   11f  Centralize slash command surface support (#14153)
d164d99300fc    4f  Clean up warping animation (#13540)
3993cf5447ee    4f  Clear shell commands when starting a new TUI conversation (#14074)
a5cc9008dbf8   19f  Connect settings with TUI app (#13325)
148c81179b02   97f  Conversation streaming for the TUI (#13057)
23f5d93e09e5   17f  Dispatch TUI slash command selections (#13488)
6ba811c9e2de    9f  Enable and render basic tool calling for the TUI (#13283)
6f286a5577ea    5f  Enable skills and project context discovery in TUI (#13497)
f5eb1885a858   17f  Expand TUI statusline item catalog (#14257)
b667d96ca6d4   13f  Extract core TUI editor element and migrate the input onto it (#13493)
eaacdf502b88   18f  Extract launch profile config for both app and tui (#13015)
69d57eea7a66    6f  Generalize TUI inline menu routing (#13668)
ec58e691acd5   10f  Handle multiline paste safely in the TUI (#13612)
517d7a915393   22f  Hook up input view with TUI app (#13186)
d34aaf06eb98   23f  Implement /fast-forward slash command in GUI and TUI (APP-4901) (#14057)
43cf43c0673b    5f  Implement `CrossAxisAlignment` options for TUI flex (#13417)
f480943453ed    2f  Improve TUI Markdown block spacing (#14049)
0d89f6c72f35    5f  Improve starfield animation scaling across terminal sizes (#14144)
d7a44d4350f4   11f  Improve user facing TUI naming (#14161)
fc773902626b    7f  Initial TUI crate (#13006)
aeae42f6a4f3   13f  Inline shell command rendering for agent tool calls in TUI (CODE-1814) (#13541)
87f0753ccf6c   12f  Make BlocklistAiAinputModel surface agnostic (#13333)
fdebae8dcd6d    2f  Make zero state animation stacked (#14224)
7e0ff783e366    5f  Open TUI conversation list with left arrow (#13797)
a35d4125bb7f   12f  Prevent overlapping TUI inline menus (#13743)
28f25535f840   14f  Proper long running command rendering (#13774)
031f396146ed    6f  Protect running TUI versions during auto-update (#14160)
fa7d39f2886c    7f  Refactor TUI question selection state (#14255)
b9226ffb0988    5f  Rename warp-tui to warp (#13904)
074e595338b7    8f  Render Agent request failures in the TUI (#14044)
75594c10c904   16f  Render TUI slash command menu (#13487)
5f52cf1dabf9   29f  Render alt-screen apps in the TUI and forward input to the PTY (#13626)
44893c50830e   21f  Render and execute file edits in the TUI surface (#13332)
0346faadb03b   19f  Render inline plans in TUI agent output (#13731)
157452cd53d3    9f  Render review comment threads in the TUI (#14214)
0cbc2563b7fd   13f  Render semantic Markdown in TUI agent output (#13730)
0f2407aefde5    4f  Restore TUI inline menu styling (#13740)
8b5291608f6e   14f  Revamp TUI zero state animation (#14159)
2723ae19716e    9f  Share slash command behavior between GUI and TUI (#13602)
f6f87aec4bd6    4f  Show bundled skills by id (not @warp-skill:) in read_skill copy (#13524)
3dd6ea882fc6   12f  Spec: TUI insert typeahead into input editor when an LRC finishes (APP-4884) (#14007)
76176405d9a6    3f  Status line V2 (CODE-1890) (#14034)
4207f56679d7    5f  Style shell commands in the TUI (#14101)
129860cfee86    9f  Support word wrapping in TUI (#13655)
6f5d21145d28   11f  TUI alt screen mouse interactions (#13696)
15f1053b56a3   10f  TUI autoupdate (#13329)
eedb5ac5d142   23f  TUI inline diff rendering for agent file-edit tool calls (CODE-1800) (#13444)
fa0d6fc859a4   46f  TUI warping indicator (#13442)
0167b43a869b    4f  TUI: add blank line and left-align thinking block body (#14108)
8d2759cffb57    2f  TUI: click footer model label to open the inline model menu (APP-4908) (#14076)
917ba672d91e    6f  TUI: prefix normal input with cyan prompt (APP-4913) (#14087)
42beda149600    2f  TUI: replace U+21AC (↬) with U+22A2 (⊢) in the status footer (#14084)
e2c823bb97d0    7f  TUI: show input with 'Starting shell...' during session bootstrap (APP-4881) (#13958)
9078920bf183    3f  TUI: style /compact summary like thinking text and collapse by default (#14226)
3ea35060f3aa    2f  TUI: use filled circle ● for in-progress task glyph to match pending ◌ size (#14427)
10f5ab483ea5    2f  Use Warp Agent CLI download endpoints (#14147)
5dafe6399845   11f  Use a single TUI natural language detection toggle (#14168)
fd210f13fe59    3f  Use a single chevron for the TUI input prompt marker (#13805)
73834e56ffad   31f  Use faster hasher for `EntityId`-keyed maps and sets. (#13058)
cf1d88ae00a5    2f  Use × (U+00D7) for the TUI failed-tool-call glyph (#13838)
6cebc7a5a3fe   42f  Wire up tui input, transcript view, into full TUI app (#13104)
4b188086fb39    2f  [APP-4960] Match TUI resume hint to running channel (#14251)
62da4ee72156   13f  [CODE-1829] Render agent task lists in the TUI transcript (#13570)
a41e5846bf2b   17f  [CODE-1871] Add up-arrow prompt history to the TUI (#13827)
6cb68ea70ad4    7f  [CODE-1871] Persist TUI prompts for history (#14015)
3f70bfc96672    2f  [TUI] Disable NLD in slash command (#13893)
1d70a32b890e    3f  add TUI file-edit permission requests (#14020)
8d9f2e08ad90   24f  add TUI multi-session registry and focused-session root projection (#13775)
d02e147360b9   15f  add editable TUI shell-command permission requests (#14021)
063ed013a0d6   16f  add light,dark, and auto themes (#14265)
659cf3747e99   15f  add reusable TUI editor view (#13817)
e7c409bade30    9f  add reusable TUI option selector over shared option snapshots (#13716)
4b77c4de2709   18f  add reusable TUI tab bar component (#13831)
f7e9d283030b    9f  add version command (#14169)
992610422e4e   58f  conversation persistence and restoration for TUI (#13590)
cc519974315f    8f  don't invert colors on selection (#14102)
62d87d7d2aca    6f  feat(tui): add /clear slash command as alias for /new and /agent (#14421)
69ce3728acae   14f  feat(tui): add /view-logs slash command (APP-4880) (#13932)
6bd18c2ae004   12f  feat(tui): add mouse interactions to TUI inline menus (#14385)
446c2f3312ea    2f  feat(tui): ask-question footer says "cancel question" not "skip all" (APP-4918) (#14105)
7d5b818876c6    4f  feat(tui): auto-copy highlighted text in prompt input (opt-in via with_copy_on_highlight) (#14340)
e712486bd505   30f  feat(tui): dedicated /status menu with shared read-only structure (extend shortcuts) (#14281)
1d9be246cb41    3f  feat(tui): ghosted interrupt hint row during long-running commands (CODE-1900) (#14038)
11742b32c516   35f  feat(tui): implement TUI input view milestone 1 (#13064)
8e61f9ced490   14f  feat(tui): make NLD opt-in with /enable & /disable slash commands (CODE-1893) (#14035)
7e4da0ff837c    8f  feat(tui): placeholder ghost text + agent-mode input hints (CODE-1897, CODE-1898) (#14036)
05e63c60d232    4f  feat(tui): shell-mode ghosted input hint (CODE-1899) (#14037)
1d9d6f3ced44    5f  feat(tui): show (recommended) next to agent-recommended ask-question option (#14413)
d30abae5d7cd   29f  feat: ship TUI voice input slash command (CODE-1884) (#14089)
f7693d9930c2    9f  feat: support TUI terminal use for alternate-screen commands (#14269)
fb22b1920c59   16f  refactor(tui): centralize session input blocking state (#14206)
b755d16a81f5   43f  replace dispatch time area passing w/ paint-retained size/origin (#13641)
a0d0dc83c433   19f  rich message component (#13777)
0993c316121f    3f  route TUI editor input by owning-view focus (#13771)
dbd41220901d   32f  thinking blocks for TUI (#13291)
b1dd46f6c305    2f  tui: bind cmd-delete to KillToLineEnd in the input editor (#14100)
```

### S3 — tabs, tab groups, panes, window chrome — 58 commits

```
f1701be39a36    2f  Activate next tab (below) after closing the active vertical tab (#13142)
b814dc30755c    2f  Activate right horizontal tab after close (#11197)
ee133f47ae19    8f  Add /set-tab-color slash command (#9305)
d3757291a1a1    3f  Add basic tab group rendering for horizontal tabs (#12089)
af532bdc3c6d    8f  Add basic tab/group pinning actions + vtab rendering (#12534)
f658c30b576c    2f  Add icon collage to vertical tab collapsed group (#12386)
d2391bad1ded    4f  Add setting to hide the title bar search bar in vertical tab layout (#12249)
ae7f6574ad20    8f  Add tab pinning feature flag and update data models (#12453)
4f5d0d6f8d2a    3f  Additional tab grouping actions (#11791)
9c59c69df0fe    3f  Allow setting colors for tabs within a tab group (#13235)
f3bfb750bcf4    7f  Basic vertical tab grouping (#11749)
034e25bec617    4f  Ceate a key bindings for tab group and pinning actions  (#12765)
0b1e4ab4e50d    1f  Collapsing htab groups gives flex to remaining tab bar items (#13101)
3aac0073e034    2f  Don't restore vertical-tabs panel when feature is disabled (#9519)
1cdb4794e6d3    1f  Enforce pinning invariant and group contiguity on tab restoration (#12677)
7cf461f24f45    2f  Expose tools-panel tab toggles in Appearance settings (#13506)
c5d5175f5148    8f  Extend pane dragging to support tab groups - vertical tabs (#13056)
5ff4f8900e05    4f  Fork conversation: start new pane in the latest working directory (#13482)
2dbf25fe4493   12f  Hide Dock icon when using dedicated hotkey window (GH-1154) (#9926)
8de0888ae2d5    3f  Horizontal tabs pinning UI (#12671)
662bd7376716    7f  Implement dragging actions for vertical tab groups (#12000)
6c85d81de95e    2f  Increase contrast on horizontal tab bar dividers (#13200)
b802cdf57138    3f  Make pane dragging group aware for htabs (#13090)
68ed7382db61    2f  Make vertical tabs Summary-mode PR chip clickable (#12945)
28c9c7d0d3a0    1f  Mark focused-pane notifications as read when the warp window is re-focused (#10082)
fd949405c5f8    5f  Match /fork-and-compact pane options to /fork (#13061)
d0d3d064da7f    4f  Modify existing tab and group functionality to enforce pinning invariant (#12595)
984a88962630   10f  Persist tab group on app close (#12348)
f3072231e918    3f  Remember code review panel's selected repo per pane group (#10598) (#10599)
86289c931d90    1f  Rendering updates to tab groups in pane view (#13100)
39dd121f3559    9f  Reorder tools panel to put project explorer first, conversation list second (#11843)
ed9177539895    2f  Reserve titlebar space for right-side window controls (#10178)
302372182fd8    1f  Respect Markdown Viewer setting for .md links in AI rules/facts panel (#9699)
482f63ac738e    4f  Run tab-level commands from launch config URIs (#13103)
4c91056617ec    3f  Show Maximize/Minimize pane shortcut in pane menu (#12811)
3efab9abae00    5f  Show error state in conversation details panel when run API fetch fails (#10765)
981cb1c7d0ff    3f  Support dragging actions for horizontal tab groups (#12110)
b24fce3db869    4f  Support multi select grouping actions for vertical tabs (#12229)
e0535ca2cb79    3f  Support multi tab selection actions for horizontal tabs (#12257)
a44fbf1633de    6f  Support multi tab selection in vertical tabs (#12200)
97bc2646dd51    4f  Support setting colors for tab groups (#13070)
98dbf7831e55    4f  Support tab group renaming (#11842)
29394232a5c0    2f  Surface Reopen Closed Session in the new-session menu on Linux and Windows (#9347)
ad730534b088    2f  Swap tab groups to render icon only to match tabs (#13105)
fc110333acd9   10f  Tab group feature flag and entry points (#11486)
21e28ccceed8    2f  Update new tab placement for groups (#13244)
8fd3d8a75fe9    3f  Update placement of group when created from tab(s) (#12356)
9dcb9b890c3d    4f  Update tab grouping drag behavior (#13260)
53f273e921e9    1f  Update tab/group dragging behavior to respect pinning invariant (#12661)
fb8d00b07363    8f  Use new queued prompt panel for /compact-and and /fork-and-compact (#11575)
7f5190510ce5    2f  Use restored CLI agent harness title for pane/tab (#10764)
4d84af55a6a7    4f  Use unified agent icon-with-status in management panel (#10518)
51dae19e927a    3f  [NEW] tab_config windows maximum height set to window height and scro… (#13224)
a017b9a6a2b9    2f  [Tab configs] make tab configs run commands sequentially (#10698)
3984e67f40bb   23f  add support for cross-window tab drag (#9275)
0320c7929dc5   16f  feat(tabs): add CycleMostRecentTab as third Ctrl+Tab behavior option (#9658)
27f4933b81f3    3f  feat(uri): add warposs://pane/{uuid} deep link for pane focus (#9655)
d2f26ae9bdde    4f  feat: register Rename Active Pane as a keyboard-bindable action (#9351) (#9712)
```

### S4 — AI / agent / models (non-cloud-flavoured subject) — 35 commits

```
e7e97a30194a   17f  APP-4423: Temporarily disable local Claude/Codex child harnesses (#10847)
26e81f9dabdf   23f  Add /rename-conversation (#12323)
fd0a9d109d4e    9f  Add Codex local child harness support (#10176)
9fd0e887439e   22f  Add UI for configuring custom model routers in Warp Agent settings (#13010)
48a648b10bad   15f  Add `DiffStateModel` local and remote wrapper (#10404)
3239e96b07d0   14f  Add `RemoteDiffStateModel` (#10489)
20430b8a257c    6f  Add scrollable model section to custom inference modal (#12647)
2d8587373d8c   17f  Add setting to auto-queue prompts until the end of an LRC (and default it to enabled) (#12587)
63fe72858d36   21f  Add support for a proper codex plugin. (#11871)
d4ad603680cd   19f  Add support for custom model routers. (#12775)
498576859004    5f  Allow conversation renaming from the conversation list (#12409)
593b03f5ffde   25f  Apply NLD history match to agent prompt history backed by ai_queries (#12586)
d9c4c1a70b7b   10f  Carry attached image into new conversations for /fork with prompt (#12376)
c85bf84f8190    1f  Clarify BYOK / Custom Inference description copy (#11780)
98af7b654b1f   27f  Create new queued prompts list UI (#11439)
d27983252645    1f  Custom model TOS hyperlink coloring (#11240)
c2b51ab6c8e0    2f  Custom model router editor: UI polish (#13050)
c1ebde1fde86   13f  Distinguish between agent conversation model update types (#9864)
42e583a97b3c   11f  GPT models 1M context (#11838)
19a1d8001563   21f  Hide third-party transcript vehicle conversations from the management view. (#10779)
3092698d640f    2f  Keep maximized state when naving to sub-agents (#11776)
1640580cbbbf   12f  Lazily restore child agents when parent agent is restored (#10371)
dad38605a811    1f  Match MCP Servers search field sizing to standard search fields (#13536)
530ca5229ebd   14f  Persist conversation ID for local tasks and merge LocalSharedSessionLinkModel into LocalAgentTaskSyncModel (#11553)
aa0149c7db38    2f  Persist killed child agents and rename Kill→Delete after finished state (#11172)
d24408ff2521    1f  Pin conversation list kebab button to the right side of items (#9245)
d21855ab0be4   14f  Route bundled skills through active Agent Mode host (#12418)
899d966c66e4    8f  Show all personal runs in the conversation list (#9274)
fba90a5107c7    7f  Skip AgentTips with unresolved keybinding placeholders (#9509)
dd6eb69fbd3d    4f  Suggest a provider-appropriate default model after saving a BYOK key (#13018)
898336e34193   10f  Support MCP servers for third-party harnesses. (#10341)
1148ae3e8350   47f  Wake up remote Claude Code agents on new events (#9399)
b59e3519dacc    9f  add /continue-locally slash command (#9500)
e659efbb0416   15f  feat(ai): add feedback skill setting (#11341)
129783073583   29f  feat: Custom inference endpoints for third-party API models (#10781)
```

### S5 — refactor / rename / move / removal — 28 commits

```
94f63ce25fd7    8f  Clean up ConversationOrTask. (#10195)
77b7c9e03a72    2f  Clean up TMUX SSH warpification setting (#11365)
b5c64ff4dcc3  379f  Consistently use _tests.rs for test file names. (#10373)
09a35b58af8f    6f  Decouple `DiffStateModel` and `CodeReviewView` (#10314)
f653e1f1d9d0   15f  Delete unused `new_from_server_update` function. (#11112)
4c5ca9395b9a   60f  Initial settings refactor for TUI (#13294)
14205aa353b6   43f  Migrate DetectedRepositories to include remote backing repos (#10921)
a19bf168cca7    2f  Migrate SSH install setting to standard macros (#9335)
5becb10b8981   13f  Migrate app crate macOS code to objc2 (#11669)
3c4b76c00386    7f  Migrate code view to use FileLocation (#10852)
143ec08022f3    9f  Migrate conversation list to AgentConversationEntry (#10197)
4d844f14e181   58f  Migrate to use `LocalOrRemotePath` (#10961)
00fa7aad1cef   44f  Move core search infra to new `warp_search_core` crate. (#11763)
405c83cbae51   77f  Move inline test modules out to separate files. (#12018)
160b6c50346f    2f  Remove Full Terminal Agent model callout (#13157)
57062bd92031   85f  Remove TMUX based SSH warpification flow (#12478)
71054d65279c   29f  Remove `NotAmbientAgent` state from AmbientAgentViewModel. (#9310)
d58b2fa8cb12    5f  Remove feature flag gating from the feature-intro popup (#13555)
df02914a691b    6f  Remove session id from `RemoteDiffStateModel` (#11790)
7607cc7c1767   25f  Remove the /pr-comments slash command (superseded by the bundled skill) (#13621)
3e02f83c2d36    3f  Rename Slack labels to "Join our Slack community" (#13533)
5a9bf67129c8   16f  Rename `FileLocation` to `LocalOrRemotePath` (#10909)
4815c8250b00   20f  Reorganize git repo models (#12541)
05da7af3d6c4   10f  Simplify FTUE Modality Callouts (#11108)
d2e9affcd43d   13f  Split out our embedded assets to a `warp_assets` crate. (#11387)
7f0a121cf526    4f  Unify legacy and SSH warpification settings (#12946)
328a3ae945e9    5f  refactor away extra hashmap in TaskStore (#13463)
2735ae10a2d4   27f  remove dead code for WelcomePalette (#12614)
```

### S6 — build, platform, performance — 3 commits

```
ddafc51abc58    4f  Add command palette action to write heap profile to disk (#13107)
e6098a8aef18   70f  Get warp compiling on FreeBSD (#9362)
0ac6f5948385   23f  Speedup TUI startup time (#13449)
```

### S8 — other local (editor, settings, terminal, notebooks, SSH) — 50 commits

```
df8ba45c00a0    8f  Add "Copy current path" command palette action (#13148)
da3b56065459   12f  Add Ask-User-Question autonomy speedbump with new default setting (#10433)
3f83932cd127    3f  Add a format-on-save setting for the code editor (#12254)
29ed596a0c54   12f  Add auto save setting for the Warp text editor (#13435)
040a7819f0fa   10f  Add command palette entries for missing toggle settings (#11512)
910d0fc467be    3f  Add move to group sidecar menu (#11849)
14c8c8ded42e   28f  Add proper support for host scoped requests (#12036)
cc99722e74ab   11f  Add reusable feature-intro popover (#13234)
0d24d2cffaf4   25f  Add setting to enable reusing user's existing control master (#12465)
164e60e42567    5f  Add settings UI and blocked-operation banner for OSC 52 clipboard access (#25625)
67960ea540d2    8f  Add speech language preference for voice input (#13722)
eaa936c78df0   11f  Add toggle: Rich Input submit on Ctrl+Enter (vs Enter) (#11723)
676c882b7778    7f  Add warp://settings deeplink entrypoints (open, search, scroll-to-widget) (#13232)
207f9d5eb1ed    5f  Add warp://tab_config/<name> deeplink (#9379)
887c4582bb62    1f  Adjust spacing around 'Add Router' button. (#13184)
3f8cbb782504    1f  Autofocus environment name in creation modal (#11233)
23f00966c79d    7f  Connect with setup branches RPC (#11208)
a080f4b1a32d    2f  Don't set computer_use_enabled on config snapshot for third-party harnesses (#10826)
2566f54af7c3    7f  Enable async find on dogfood, add toggle for Preview/Stable (#11555)
a30cc7a3312b    3f  Enhance writing logic for ai_queries sqlite db table  (#12484)
679e426d5f0a    5f  Expose terminal focus URL env vars (#11130)
0974a0f0c5d8   12f  Implement local continuation for third party harnesses. (#12076)
37274bfef5e5    4f  Implement revised summary mode (#10067)
aa0a2c210d61   49f  Implement warpctrl with set of non-authenticated, safe commands (#12327)
294dbddcca8c    2f  Improve robustness of settings file migration tests. (#11866)
2f3b0c009c6b    2f  Make New File keybinding editable (#12979)
b15bdd3a0ace   18f  Make TerminalManager view agnostic (#13013)
f8a099380528    1f  Make worktree menu paths readable for long entries (#10862) (#10958)
2a251933c64b    9f  Persist pin state on app close/reopen (#12675)
d6c0c69b34a7    1f  Prefer stateful mouse reporting command palette entry (#12011)
475fdb33ed99    2f  Refresh settings editor text colors on appearance change (#12521)
fc6260c0135b    1f  Retoggle group collapse on group rename (#12741)
34393bc2cf11   12f  Separate persistence scope for TUI and GUI (#13500)
1d65e362b3fb   24f  Ship channel-specific warpctrl wrappers and PATH install support (#12705)
2e8dcc7e859c    5f  Show the create environment modal if submitting with a new environment. (#10850)
25cdd9e681d3   13f  Surface run executor in Warp client (#10479)
cb4fe42a960f   12f  Update filesystem watch filters (#11464)
b1cb96f98481    8f  Use feature flag in settings crate (#9507)
5967abf0be66   59f  Warp Control CLI v2: contract and spec sync (#11772)
13e8b61148f6   17f  Wire up Jupyter notebook (.ipynb) rendering in the client (#13071)
d5d48329a703    7f  [2/n] wire up the usage data to in-memory types (#11111)
aa9f9084dee7    8f  [Remote envs] codebase indexing UI (#10889)
06ba1bb365e1   18f  [orch-viewer-polling] Migrate viewer pill-bar to ancestor SSE (#11408)
c6b842fe7aba    8f  add setting for queue versus interupt (#11746)
ce73fe07bfd8   11f  feat: add configurable code editor line numbers (#10012)
e4695f2199dd    7f  feat: add show hidden files toggle to Project Explorer (#9532)
511530db0bce   10f  feat: configurable BYOE endpoint schemas (#13938)
ba5dcd90e668   33f  update FilterableDropdown to use trait for SelectActionAndClose instead of generic on entire DropdownAction (#11718)
1d99e3971183    1f  use the "emitter_handle" parameter instead of cloning strong handle i… (#12763)
1acbff119328    3f  warpui_core: backend-neutral view hierarchy for a shared responder chain (#12413)
```
