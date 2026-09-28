# Session Handoff — Phosphor parity work

Continuity doc. The durable state is git + GitHub issues/PRs + `TODO.md`; this
file ties it together and records the operational lessons a fresh session will
not otherwise have.

Last rewritten: 2026-09-27 evening, after rounds 5, 6 and 7 landed on `main`
(`0695371a9`) — see the new top of **"Where main is"** below for that state.
Everything under the `### The single most important finding of 2026-08-07`
heading is retained history from the previous rewrite (2026-08-06 evening,
after the migration to the `/cache/git/zap` host — **that host is gone**, the
checkout is now `/home/winters/git/phosphor`, verified 2026-08-17): read it for
the lessons, not as a current status report, and not for the literal paths.

## App identity (read first)
- The app is **Phosphor** (`jwp2987/phosphor`). "Zap" is only the **upstream
  ancestor** (`zerx-lab/zap`) — NOT the app name. Do not introduce new "Zap"
  branding; legacy identifiers (`zap-tui-oss` binary, `zap_*` crates,
  `SkillProvider::Zap`) stay. See `docs/DESIGN-PHOSPHOR-FORK.md`.
- **English only** in code/comments/tests/docs (`CLAUDE.local.md`). Exception:
  `app/i18n/zh-CN|ja/*.ftl` are intentional translations — never edit them.
- The behavioral oracle is **PINNED**, never `warp/master` — read `ORACLE.md`
  for the current commit (`4111d08f9` as of this writing; it has already moved
  twice, most recently 2026-08-29, so **take it from `ORACLE.md`, not from
  here** — this file has a documented history of naming a superseded pin).
  `warp/master` is a fetched remote and is useful only for archaeology (finding
  *why* a pin behaviour exists); measuring against it produces a gap that never
  shrinks. As of 2026-09-05 the pin is evidence, not a spec — see `ORACLE.md`'s
  "What the pin is for". Never weaken a test to go green — fix the code (AGENTS
  §5.10). Every defect → issue → branch → PR (§5.11).
- **The repo is PUBLIC.** Issue comments and PR bodies are indexed. The
  maintainer has accepted this for engineering detail including security
  findings (decision made 2026-08-06); do not re-litigate it, but be aware.

---

## Where main is

`main` is at **`0695371a9`** (2026-09-27), the tip of `integ/round6` after
rounds 5, 6 and 7 (216 commits over `3cf6d688b..0695371a9`, ~113 of them merge
commits from stacked `fix/r5-*`/`fix/g*`/`fix/r6-*`/`fix/r7-*` lane branches).
`docs/STATE.md` (generated — trust it over any number below) currently reads:
pin `4111d08f9`, 8700/9908 non-cloud pin tests present (~87.8%), last fully
green `script/precheck` at `63091930f` with **0 commits since**.

### What landed — round 5 (`fix/r5-*` / `fix/g*-*`, merged into `integ/round5-0927`)

Grouped by lane branch; issue numbers verified against `git log` and `TODO.md`
(a handful, e.g. `#726`/`#732`, are referenced only in TODO.md prose, not in a
commit subject — checked before including them here):

- **`fix/version-single-source` (#640):** `e9a1eb744` — single-source the
  display version from `app/Cargo.toml` instead of a hardcoded `0.1.2`.
- **`fix/filetree-open-launchable` (#681/#706, plus its #757 follow-up):**
  `dd232629f` — a file-tree double-click/Enter on a launchable path now opens
  it with its normal target instead of revealing it. Follow-up `c9211b995`
  (#757) fixed a conflation where a Global Search result inherited the same
  file-tree-only exception via a shared `CodeSource::FileTree` — added
  `CodeSource::GlobalSearch`.
- **`fix/g2-url-refuse` (#716):** `93e3f4efa` + `047ff826d` — `set_before_open_url`
  can refuse a URL outright, not just rewrite it, and the backstop still
  passes local `file:` URLs.
- **`fix/g3-ci-toolchain` (#712–#715):** `c3d57d542`/`5d48ef6f3` — stop
  trusting the third-party `cargo-binstall` action's metadata, verify the
  installed binary and retry the download; `4c0888c98` — add `rust-analyzer`
  to `rust-toolchain.toml`'s components.
- **`fix/decisions-small` — decisions 1–4:** `1ba57232e` (#708) —
  `middle_click_paste_enabled` now applies on Linux/FreeBSD, not just macOS;
  `cedda2465` (#718) — redact BYOP provider endpoints (`scheme://host[:port]`)
  from log lines; `7775caffb` (#720) — bump `release-cli`'s `codegen-units`
  1→4; `64d52dc9c` (#721) — `script/precheck`'s integration step now actually
  runs `crates/integration` instead of only type-checking it (advisory at
  first — see the verification-state note below for its promotion to a hard
  gate).
- **`fix/skills-port` (dff0d13fe):** `ab48bd376` — publish `WARP_SKILL_DIRS`
  skills to Claude Code and Codex, plus `c39d1eeca` — never rename aside a
  conflicting entry.
- **`fix/blink-repaint-cost` (#703, #689 follow-up):** `e0127fcba` — skip
  layout on a paint-only blink repaint (a *focused* idle editor was repainting
  the whole element tree every blink); `891a4592b` follow-up fix.
- **`fix/remote-notice` (#713-ish security hardening):** `113e56e1d` — surface
  the legacy-SSH fallback and a dev-build checksum refusal instead of failing
  silently; `b672f42b2` — surface the previously fully-silent legacy-SSH
  fallback as a dismissible banner (see `DECLINED.md` IMPROVED: "A
  silently-degraded remote SSH session gets a dismissible banner...").
- **`fix/round-followups` (#740, #725, #733):** `6fd544d39` (#740a) — @-mention
  top-level search now finds files outside a git repo (missing
  `CurrentFolderFiles` match arm); `1f169cd60` (#740b) — invalid-regex error
  text now shows on Enter, not just the border; `9c30a53e6` (#725) — queue an
  AI follow-up submitted while blocked instead of refusing it; `652e1b1be`
  (#733) — implement the agent-driven password-prompt hand-over (`BlockedOnInput`).
- **`fix/g4-terminal-robustness` (#723/#728/#735/#737):** shell/terminal
  hardening — chunked bootstrap writes for docker/podman exec subshells,
  Arc-wrapped `FileTreeState.gitignores` and `EditDelta.new_lines` to stop
  per-event/large-file deep clones, bounded rayon fan-out in `layout_delta`,
  zsh `compadd -ld` + vicmd kill-buffer port (#737).
- **`fix/g5-editor-internals` (#710/#711/#729/#730):** structured per-block
  diff-match failures for agent retry, version-partitioned daemon socket/PID
  names, vim mode enabled on 7 multi-line editors, a Shift+right-click paste
  hint.
- **`fix/g6-settings-cosmetics` (#709/#722/#724/#727/#731/#734/#736/#738/#741):**
  nine small settings/MCP/completer fixes — MCP server logos matched by title
  prefix, an unreachable BYOP provider named its live alternative, `tab.move`
  refused when the workspace can't perform it, completer option-argument
  resolution by cursor position, the "Profiles (1)" mislabel under BYOK, and
  the hidden non-remappable Project Explorer keybinding removed.
- **`fix/gui-accept-rename-delete` (#688):** the big one this round —
  `InlineDiffView::write_action` now resolves `Write`/`Rename`/`Delete` from
  the diff instead of always saving in place; a non-lossy
  `rename_and_save_if_unchanged` primitive (write-destination-first,
  `O_EXCL`, closes a POSIX rename TOCTOU gap); a multi-lane
  `RevertSequence`/rewind scheduler so a rename's own undo and a concurrent
  edit to its destination don't race. Six adversarial-review follow-ups
  landed the same day (`40057d594`, `3038575c0`, `78279ad50`) — see
  `DECLINED.md`'s IMPROVED entry for the full list. **No live-view test for
  this yet** — see #751 below.
- **`fix/g1-silent-failures` (#726/#732/#739/#742/#749):** oversized-task
  pruning made honest instead of silently dropping, PowerShell linting fixed
  to check every source not just the first with findings, `AsyncSearcher`'s
  rebuild channel bounded to one pending rebuild, a tab-drag test race fixed
  by forcing a repaint, zero-command Agent Mode input made fail-closed instead
  of vacuously auto-approving (also in `DECLINED.md` IMPROVED, #746 revisits
  this same mechanism in round 6).
- Also this round: `f0186ed3c`/`c82eab67e` — delete the unreachable project
  onboarding step and retire `ZapNewSettingsModes` (accepted scope `c5e4a02e3`);
  `b05714bab` — move settings-schema generation into the main binary;
  `84716b51b` — integration coverage for sidebar navigation/search;
  `79a5064f5`/`127615b51` — attempted an `InlineDiffView` live-view test
  harness for #688's glue, **left unticked as #751** (written with no
  `cargo` available this round — see the verification-state note).

### What landed — round 6 (`fix/r6-*`, merged into `integ/round6`)

- **Ledger / re-pin tooling (#743, #747, #752):** `bd2546ae7`/`f7f221770` —
  `generate_repin_queue` detects renames and anchors `marker_pattern` matches
  to identifier/word-character boundaries; `8a0a4465a` — fixed two broken
  re-pin tripwires and resolved open refutation-pass entries; recounted the
  `FeatureFlag` divergence against the current pin (`9bd322637`, 230 fork vs
  291 pin variants, 16 fork-only, all individually accounted for).
- **Accepted scope ports:** `f0186ed3c` (`c5e4a02e3`, carried from round 5),
  `35fac964f`/`d8622b8b3` (`ff16a0b2a` — FxHashMap/hashbrown in hot paths),
  `6ebf9758f`/`a67f1b4af` (`c6609ef2` — cache/share gitignore matchers in
  file-tree traversal), `412acfd8d` (`d68a638ef`, partial — `HexColorError`
  as a typed `thiserror::Error`), `4a015a07d` — remove #634 orphans
  (`DismissCodeToolbeltTooltip`, two dead `FeatureFlag`s).
- **`#775` — `SkillsDataSource` resolved the wrong (local) skills for a
  remote SSH tab in tests, not production:** two-step fix, both instructive.
  `0bcdce37d` fixed a `Weak<Session>` lifetime bug (`ActiveSession` holds only
  a `Weak<Session>`; a test's temporary `Arc` dropped to zero before the query
  ran). That was necessary but not sufficient — `9854eceac` found the real
  gate: `active_window` is only set by real focus events or
  `WindowManager::overwrite_for_test`, never by `App::test`, so a fresh test
  window is never "active" and the whole lookup short-circuited to `None`
  regardless of the session fix. **Both are now standing lessons — see below.**
- **`#781` — "Move pane to its own tab"**, a new pane-overflow-menu action
  (`cccb08d1c`), fixed for pinned/grouped tab placement and a source-tab
  lookup bug (`aa1b4659f`), with discriminating regression tests
  (`87de017d7`, `aca84bb89`, `3a5325282`).
- **`#776` — port upstream `f42c4ab6c`'s `Lazy` field deferral** for
  `UserBlockCompleted` (`serialized_block`, `command`, and three more fields
  become `Lazy<T, BlockList>`, keyed by stable `BlockId` not `BlockIndex`) —
  five commits (`46faa28f7` through `ff83f1877`) rewiring every consumer
  across terminal/input/view/AI, plus a `maa.rs` deadlock fix found along the
  way (`0eba8c56a`).
- **User-visible bugs (#744, #746, #748, #755, #759, #761, #767, #768, plus
  #770/#771/#772 which continued into round 7):** `2768`-area — upstream's
  final-revision design for a display bug ported verbatim (#744); `#746` —
  the same zero-command-input fail-closed fix as #749 above, applied to a
  second call site (see `DECLINED.md` IMPROVED); `#755` — usage-footer stats
  now derive live and re-render on updates; `#759` — `DaemonStoreClient`
  routed through per-model endpoints; `#761` — `FileGlobV2` gained a
  `result_limit` field so a model's requested limit is honoured instead of
  silently discarded (200-row hardcode) — see `DECLINED.md` IMPROVED; `#767`/
  `#634` orphans and a stale `settle()` doc comment fixed same-day
  (`3e1e45568`, `4a015a07d`, `ad9e438c9`); `#768` — a rewind batch that never
  settles if a remote revert write never resolves, fixed with a 20s per-write
  deadline and a per-file generation guard (`17c3a4b10`, `9fafffc4d`).
- **Ports (#750, #758, #760, #762, #763, #764, #765):** PSReadLine reset to
  Emacs mode before Warp's chords register (#750, carried into round 5's
  list above as `68f42b3da`), codebase-index embedding-model selection no
  longer order-dependent by provider (#758, `8657186cb`), native-completions
  watchdog timeout/recovery test coverage (#748/#760, `afb7b59e9`),
  `report_error!` no longer used for an expected `AmbientAgentSource` case
  (#763, `b7fb03956`), wasm/web guard hunks from `0a7d5380e` (#764,
  `da973fc33`).
- **`#753` — warpify-path agent commands** — `fdfad3d70` gives a warpify-path
  command its control state before `reinit_shell` finishes it, and
  `d76d8dcff` unlocks the queue directly instead of upgrading a
  warpify-path block.

### What landed — round 7 (`fix/r7-*`, merged into `integ/round6`)

- **"The shell lockup" — a stalled native-completions handshake could wedge
  the PTY read loop (#770, plus #771/#772):** the biggest investigation this
  round, spanning `7ceb583ce` through `f563cb46d`. Fixes: stop a late
  completions event from destroying a newer handshake, never start a new
  native-completions request while one is in flight, mark foreground-image
  layers click-through, bound `warp_read_completion_buffer`'s shell-side read
  so it can't wedge the shell, diagnostics (`453cde9eb`, `9f00db036`) that
  arm a one-shot timer and throttle-log a stalled PTY write gate. One attempt
  (`03d3b9955`, answer a late completions-prompt reply) was **reverted the
  same day** (`57e9d24fe`) after adversarial review — the shell-side timeout
  fix was judged safer. `#771` — a third-party tool's env var was leaking
  into every spawned shell (`b04c653bf`-adjacent). `#772` — BYOP model names
  no longer get their `reasoning_effort` suffix stripped, and effort
  inference is gated on the first-party host rather than the adapter kind
  (`cd2b319bc`, `89a2c569b`). **The originally-reported field lockup is still
  unexplained** — everything above closes real bugs found along the way, not
  a confirmed root cause.
- **BYOP compaction/skill-history (#777, #778):** `4aa3a4fdb` (#777) — anchor
  BYOP compaction summaries on the nearest skill invocation;
  `69e26e888`/`f9ce22f44` (#778) — keep `/skill` invocations in BYOP
  conversation history, with a test pinning that persisted and live replay
  render identically.
- **BYOP provider-header handling (#779, #780):** `a47ecda72` (#779) — don't
  retry a BYOP request the provider already rejected; `9ca6dab20`/`5bd654d71`/
  `6f615aa42` (#780) — forward a BYOP provider's `extra_headers` on
  title-gen/one-shot/embeddings requests too (previously only the main
  request got them), stop embeddings/rerank sending a duplicate
  `Authorization` header, and warn (never log the value) when a header is
  dropped as invalid.
- **`#774` — remote project-rules pipeline, prep only, still open:**
  `dc6b974ff` single-sources the rule-file list (`CLAUDE.md` now recognized
  alongside `WARP.md`/`AGENTS.md`) from `warp_util::project_rules`, so the
  indexer sees a remote repo's rule files by name — but nothing yet turns
  that into agent context for a remote session; `reconcile_project_rules`
  and `remote_path_to_rules` still have no production caller/writer. Left
  open on purpose; sizing the rest needs reading `SkillWatcher`'s
  subscription wiring first (the adjacent, already-solved case for remote
  project *skills*).
- **Round-6 integration baseline triage (#721 follow-up, #782/#783/#784):**
  the first full `-p integration` run under Xvfb + Mesa lavapipe came back
  238/248. All 10 failures triaged, none environment-only: 5 were a real bug
  (settings sidebar's Agents umbrella pre-expanded before any navigation,
  #782, fixed `f756dd7ff`); 3 were `ec2e2d227`'s already-declined
  snackbar-over-running-command change never propagated to its own tests
  (#783); 2 were `4203a0573`'s already-declined long-running-padding
  widening never propagated to two hardcoded pixel targets (#784) — both
  fixed in `d826a11f7`, confirmed not a window-manager artifact by
  reproducing under real `openbox` first. This is what let `#721` promote
  the integration step from advisory to a hard gate (`8f79e840c`) — see
  below.
- **`#781` follow-up and post-pin upstream ports (#785):** `d5fdbff50`
  ported — completer flag completions leaked past a POSIX `--`
  end-of-options marker (`ce99c0a95`); the `--features v2` completer build
  broke on the new tests and was fixed by gating them to the legacy parser
  (`b9ce1c425`).
- **`fix/r6-fix-775` / `fix/r6-fix-781`:** the #775 and #781 fixes above were
  actually finished in round 6 but merged in via these two branches at the
  round-6/7 boundary; listed under round 6 above for where the work
  happened, noted here for where the merge commit sits in `git log`.

### Verification state (2026-09-27)

- **Green `script/precheck` at `63091930f`** (2026-09-27 22:26), 0 commits
  since as of `0695371a9` — see `docs/STATE.md`, which generated that line.
  All four guards (`check_cloud_boundary`, `check_stub_coverage`,
  `check_declined_collisions`, `check_sweep_ledger`) pass.
- **The integration suite (`-p integration`) is now a HARD gate in
  `script/precheck`** (`8f79e840c`), not advisory — the round-6 baseline run
  under Xvfb + Mesa lavapipe came back clean (238/248, then 248/248 once the
  10 triaged failures above were fixed) rather than the environment-gap the
  #721 item anticipated, so there was no reason to keep it advisory. The gate
  checks for `xvfb-run`, a Vulkan ICD and `libxkbcommon-x11` first. This was
  possible only after `#786` made the harness stop assuming a silent shell —
  see the operational lessons below.
- **`#751` (`InlineDiffView` live-view test harness) is deliberately
  unticked, not broken.** Its test — `deleting_through_the_view_removes_the_file_and_revert_restores_it`
  and `renaming_through_the_view_moves_the_file_and_revert_restores_it`
  (`app/src/code/inline_diff.rs`) — **is on `main`** and **passed in the
  green precheck above**, since the integration/lib suite ran as part of it.
  What's still open is a real `cargo nextest` confirmation from a session
  with a build, because the test was written without one (no `cargo`
  available in the agent round that wrote it) — the author's own note asks
  the next build to re-verify it and named the exact failure points to check
  first if it doesn't compile clean. Treat it as "written and now proven to
  pass in CI," not "unverified."
- **`#769` — `warp_tui` tests do not build/run standalone.** Always run
  `cargo nextest -p warp -p warp_tui --lib --features warp/gui`; `-p
  warp_tui` alone fails to compile. Not a regression, a standing fact about
  this workspace's feature wiring.
- **Not verified anywhere in rounds 5–7:** Windows and macOS. Nothing from
  these three rounds has been built on either platform. In particular
  **`#773` (Windows graceful shutdown on console close/logoff)** has a fix on
  `fix/r7-windows-shutdown` (`565e6403e`, `f176fa806`, `57fd9f163`,
  **not yet merged**) that needs a real Windows build before it lands — left
  open on purpose, not forgotten.
- **X11 results (2026-09-27/28, dev build of `0695371a9` on Xvfb, isolated
  profiles, compared with installed 0.1.7):** 17 of 21 checks passed outright —
  #688 accept rename/delete + byte-exact rewind, #697/#698/#758 rendered
  Markdown (0.25% CPU, Home/End/PageUp/PageDown, scroll kept across
  Rendered/Raw), #708 middle-click on Linux, #722/#738/#740b/#699 settings
  controls, #741/#760 shortcuts, #716/#706 links (OSC 8 `file:` opens,
  `javascript:`/unknown schemes refused, file-tree `.docx` opens), #781 move
  pane to tab incl. pinned tab and pinned group, #771 inline-image
  click-through, #770 zsh Tab completion, #685/#707 SIGTERM kills the MCP
  grandchild, #696 English-as-shell, #725 approve/cancel paths, #753 warpify
  fast path (no floating card), #740a `@notes`, conversation restore, #756/#717
  TUI version + signal exit codes. Found and fixed in round 8: #789 (unreachable-
  provider hint never fired), #790 (queued follow-up stayed locked after Reject).
  Found and filed: #787 (blink still ~17% CPU under software rendering; #703
  skipped layout only), #788 (global-search `.docx` lands in the in-app editor,
  not revealed). Unexplained: one instance finished teardown and never exited
  (#791; its log shows a signal-driven quit, not the key; not reproduced).
  Not GUI-verified: #709 TUI `?` sheet (needs a real terminal; unit-tested),
  #782 Agents umbrella (expanded on gear-open by design, since the landing page
  is an Agents subpage), #775 remote Skills (the `@` picker is disabled in SSH
  sessions by design since 0.1.0, so only the CLI-agent rich input reaches it).

### Open decisions (current, 2026-09-27)

Add to the "Decisions only the maintainer can make" section below; called out
here because they're new this round:

- **Decision 8 — `21f413b7` terminal-crate/tmux move** (147 files,
  `app/src/terminal/**` → `crates/warp_terminal/**`). Accepted in scope
  2026-08-29 but **held**: not landed this round, awaiting the maintainer's
  release of it. The fzf/atuin port below is explicitly sequenced *after* it
  (porting the fzf/atuin DCS-hook half into `app/src/terminal` first would
  mean porting it twice once decision 8 reshapes that tree into
  `crates/warp_terminal`).
- **fzf/atuin handoff** (`bf2364bc9` + `7c360f772`/`f2b401aed`/`2120e3794`/
  `b42435622`, upstream CORE-3807, post-pin) — hand Ctrl-R/Ctrl-T/Alt-C to
  fzf/atuin when installed. Real local feature, not cloud/BYOP debt; large
  (base commit alone: 22 files, +1152/-42 across all three shell bootstraps).
  Queued behind decision 8, above.
- **`511b952c2` — `create_file` gains `allow_overwrite`** (upstream #15380,
  post-pin). Blocked on a `warp_multi_agent_api` proto bump
  (`f0028fa6d0` → `0ca49ce589`, `Cargo.toml:385`) — check every other
  consumer of that crate before taking the bump. Port only the client
  behaviour: there's no server here to negotiate `supports_create_file_overwrite`
  with, so the BYOP tool schema should just offer the field.

### Operational lessons learned this round (2026-09-27)

- **The integration harness must not assume a silent shell (`#786`).** This
  is what unblocked promoting the integration suite to a hard gate — the
  harness previously choked on host output it didn't expect (MOTD-style
  banners); made MOTD-independent, then the `ldconfig` pipefail trap below
  was found and fixed the same day.
- **`grep -q` under `set -o pipefail` breaks gate checks.** `ldconfig -p |
  grep -q ...` fails when `grep -q` exits at its first match and `ldconfig`
  gets `SIGPIPE` — the integration suite silently refused to run on a box
  that actually had `libxkbcommon-x11`. Fixed by dropping `-q` (`4e2ba37e1`).
- **A mid-file `#[cfg(test)]` module blinds the `chat_stream` log-privacy
  scanner.** `#779` added `mod byop_stream_error_retry_tests` in the middle
  of `chat_stream.rs`, and the scanner — which checks real production log
  sites, not test code — stopped seeing 33 of 60+ sites past that point and
  failed. Fixed by moving the module to the bottom with the file's other
  test modules (`4e2ba37e1`). **Keep test modules at the bottom of the
  file, always**, not just in this one.
- **Tests that need a window active must call
  `WindowManager::overwrite_for_test`** — `App::test` never fires a real
  platform focus event, so `windows().state().active_window` stays `None`
  forever otherwise, and any lookup gated on it (`ActiveSession::current_working_directory_location`,
  etc.) silently takes its no-window default. **And `ActiveSession` holds
  only a `Weak<Session>`**, by design (production sessions are kept alive by
  the real session registry) — a test must keep its own `Arc<Session>` alive
  for the test's duration, not just pass a temporary `Arc::new(session)`
  straight into `set_session_for_test`, or the session silently disappears
  before the query runs. Both bit the same test (#775) on the same day, in
  that order — fixing the `Weak` issue alone was not sufficient.
- **`CreateNewTabGroup` appends a tab rather than converting the current
  one.** Tripped up the `#781` move-pane-to-own-tab tests before the
  discriminating pinned/grouped assertions landed (`87de017d7`) — know this
  before writing a test that assumes a tab-group action replaces the active
  tab in place.
- **Never kill by pattern over ssh on the build box.** Standing rule,
  reaffirmed this round; no incident this round, but worth keeping next to
  the other build-box rules below since a `pgrep -f <pattern>`-style kill has
  already caused false "still running" readings before (see the 2026-08-07
  section).
- **The maintainer must approve any build-box environment change.** Same
  status — reaffirmed, not exercised this round.

### Branch hygiene (2026-09-27)

Many round 5/6/7 branches are merged and safe to prune:
`fix/r5-*`/`fix/g[1-6]-*` (round 5), `fix/r6-*` (round 6, including both
local and `-2` follow-up variants), `fix/r7-*` **except `fix/r7-windows-shutdown`**
(unmerged, holds #773, needs a Windows build first), and older `port/*`
branches (`port/bash-bootstrap-fixes`, `port/completer-cache`, `port/cursor`,
`port/editor-*`, `port/glyph-crash-fixes`, `port/grep-parse`,
`port/guard-conversation-history`, `port/leaks-logs`, `port/linux-quit-binding`,
`port/move-terminal-crate`, `port/objc2`, `port/plan-mode-document`,
`port/rmcp-cve-2026-64684`, `port/shell-bugs`, `port/tab-*`, `port/tests-*`,
`port/t-S1` through `port/t-S9`, `port/view-try-update-model-panic`,
`port/widechar-eol-crash`, `port/winit-cosmic-text-pins`) and superseded
`integ/round2-0926` through `integ/round4-0926`. **Do not touch
`moth-parliament`** — the maintainer's separate workstream.

---

### The single most important finding of 2026-08-07 (history — see the current state above)

**The issue tracker had drifted from the codebase in both directions**, and that
drift was the dominant cost of the day — larger than any individual port.

Of roughly 150 open issues examined, **21 were wrong as filed**:

| category | count | examples |
|---|---|---|
| Already merged, never closed | 8 | #356/#358/#361/#331 (all PR #472), #281 (PR #471), #202 (PR #121), the `crates/vim` halves of #427/#276 (PR #475) |
| Duplicates | 9 | #241/#242/#243/#274 (vim), #238/#239/#240 (notebooks), #264, #183, #225 |
| Dependent on a decision already made | 3 | #402, #429, #235 |
| Premise understated or misclassified | 3 | #324, #203, #295 |

**The structural cause of the "already merged" bucket:** this project uses
`Refs #N` in PR bodies instead of closing keywords, deliberately — closing
keywords have auto-closed issues wrongly before (see the #396 incident below).
That tradeoff is correct, but **nothing compensates for it**, so completed work
silently accumulates as open debt. Run a reconciliation pass periodically
instead of rediscovering it one agent at a time.

**The cause of the duplicate bucket:** a batch of six issues filed 2026-08-07
04:37–04:38 had bodies consisting only of a broken `@/tmp/.../scratchpad/...`
reference that never resolved. All six were re-filed properly ~45 minutes later.
If you see an issue whose body starts with `@/`, it is from that batch.

### Reclassified: portable debt → maintainer decision

Three issues were found to be mis-framed as "port this from the pin" when the
pin offers no answer. Expect more of these:

- **#203** (TUI editor is O(document) per layout pass) — premise TRUE, but
  `display_rows`, `display_lattice` and `build()` are **byte-identical to the
  pin**. Warp's own stable has this architecture. Needs a design decision and a
  running-TUI harness, not a port.
- **#324** (remote diff-state manager) — the fork's inline `diff_state_subscriptions`
  is a *subset* of the pin's `RemoteDiffStateManager`, missing model sharing,
  pending-response queueing and abort. Blocked on #330 and #438.
- **#295** (portable custom-theme paths) — `SCOPE-REST.md` rates the file
  verdict **A**; it is a feature gap, not test debt.

That last one is the `CLAUDE.md` "verdict A is overstated" caveat showing up
concretely. **Do not size a port from `SCOPE-*.md` alone.**

### Merging at scale

Branch protection has **auto-merge disabled**, so PRs must be merged explicitly.
A polling auto-merger was used
(`scratchpad/automerge.sh`) that gates on **all four checks including
`cargo nextest`** — deliberately stricter than branch protection, which does not
require nextest. Do not merge on `mergeStateStatus` alone; `UNSTABLE` can mean a
red test suite.

**The bottleneck at this scale is GitHub Actions, not agents.** ~20 concurrent
PRs × 4 jobs saturates the runners; agent throughput stops mattering.

### Known merge-order conflicts as of 2026-08-07 evening

- **#509 (#438 protocol envelope) vs #508 (#437 DiscardFiles)** — both edit
  `remote_server.proto` and `server_model.rs`. #509 restructures the flat
  `ClientMessage` oneof into `HostScopedRequest`/`SessionScopedRequest`/
  `Notification`; whichever lands second must move `DiscardFiles` to
  `HostScopedRequest` field 8.
- **#502 (#335 failover) vs #503 (#330 git-status)** — both add fields to the
  `ServerModel` struct and its `test_model()`.
- **#491 (skill path typing) vs #493 (bundled/global skills)** — #491 retyped
  `get_provider_for_path` to `&LocalOrRemotePath`, breaking four call sites in
  `skill_manager.rs`, which #493 owns.

**Note #509 ported the envelope only, NOT the failover machinery** upstream
bundled into the same commit (`pending_host_requests`, `HostRequestHandle`,
`host_response.rs` dispatch — 28 files upstream). So after #509 lands you have
the *shape* without cross-connection behaviour. #502 supplies part of it and is
inert until something populates the map.

Merged that day: #457 (`DECLINED.md`), #459 (`precheck` + `agent-worktree`), #420
and #432 (round-4 ports, carrying #460 and #461), #463 (`GitHubRepoModel`), #466
(`warp_tui` resync). #462 (OSC 7), #464 (#441 fixes) and #467 (toolchain
enforcement) were in flight at write time.

**`script/known_test_failures.txt` went 8 → 7 → 5.** #460 retired
`lrc_queued_prompts` (via #420); #464 retires the last two `terminal::input`
entries. **The five that remain are all deliberate `history_model` pins** for
#251/#253 — they assert the oracle's rewind/fork semantics, which the fork does
not implement. They need a maintainer *decision*, not engineering, and they are
the only thing standing between you and promoting `cargo nextest (Linux)` to a
required check.

Branch protection: `fork boundary guards`, `cargo check (Linux)`, `cargo check
(Windows)` required, **admins not exempt**. `cargo nextest (Linux)` runs but is
**not required**, so a red nextest does not block a merge — which is why PRs look
red and merge anyway. That is deliberate, not breakage.

**`strict: false`** — a PR is *not* required to be up to date with `main` before
merging, so **CI tests the PR head, never the merge result.** A branch can go green
against a stale base and merge anyway. This is the one stale-base case `precheck`
cannot catch for you.

### Recovered agent work — what the OOM actually cost

Three feature-port agents (`ghrepo`, `osc7`, `warptui`) were killed with their work
**uncommitted on disk**. The code survived and all three landed or are landing. What
did *not* survive was their reasoning.

That distinction decided the recovery, and it is worth internalising:

- **Feature-port agents have issue-defined acceptance criteria**, so their
  deliverable is re-derivable from the diff plus the issues. Nothing was lost that
  measurement could not recover.
- **Test-porting agents' deliverable is the classification** (portable /
  feature-gap / cloud / stub). That is *not* re-derivable from the diff. Lose the
  agent, lose the round's actual output.

Do not confuse the two, as this session initially did — it produced several
confident statements about lost classification counts that were about the wrong
cohort entirely.

**#396 does NOT close with OSC 7.** This file previously said "likely #396". It was
measured: nothing in the OSC 7 diff touches the fork / new-pane / working-directory
path. #416 may also close only partially — it names 27 pinned tests, the branch
ported 21, and the delta was never traced test-by-test.

### The board is smaller than it looks

206 open, but **~60 collapse into 8 root causes** — see **#456**, the
root-cause map. Four more clusters are *decisions*, not engineering. OSC 7 alone
closes 4 issues and unblocks ~30 tests; deciding #11 closes ~14 with no code.

Read **`DECLINED.md`** before filing a parity issue — it records what is absent
*on purpose*, plus the recurring false positives (`computer_use`,
`remote_server`, Grok OAuth) that keep getting mislabelled as cloud.

Open maintainer decisions: **#11**, **#267**, **#318**, **#373**, **#445**.

### The scope number, corrected

`SCOPE-*.md` called the remaining work ~1,605 tests of "test debt". Measured
across 30 agents the portable rate is **6–7%**, and that is biased *low* —
earlier rounds skimmed the easy areas. The bulk is **feature gap**: the test is
fine, the source was never forked. The fork diverged 2026-04-28; the pin is
2026-07-29 — 1,799 upstream commits, so much of the "gap" is drift, not removed
work. Treat the ORACLE.md totals as disproven.

### Never let an agent background a cargo command — it will stall forever

**Five agents stalled this way on 2026-08-07.** An agent that runs cargo in the
background and then waits for a completion notification never gets one, because
it cannot receive them. It sits there burning nothing and reporting nothing
until the coordinator notices and pokes it.

This is a **coordinator brief defect**, not an agent failure. A brief that says
"gate with rustfmt and precheck" without saying how to run a build invites it.
Put this in every brief that might touch cargo:

    timeout 580 ./script/agent-cargo check -p warp --features gui --lib --tests 2>&1 | tail -25

Three things matter in that line:
- **Foreground.** Never `&`, never a background task the agent then waits on.
- **`script/agent-cargo`, not raw cargo.** It applies the slot limit, job caps
  and per-agent `CARGO_TARGET_DIR`, so a single-package check is safe even with
  a large fleet running. Raw parallel cargo is what OOMed the box.
- **Trust the `RESULT agent=… exit=N` line on stderr, not the pipeline's exit
  code.** Piping through `tail` masks the real status.

If the command times out, do **not** retry it blind — read `gh run view
--log-failed` and work from the actual compiler output instead.

### The cost of the no-cargo agent gate — measured 2026-08-07

`docs/FLEET-ROUND.md` has agents gate on `rustfmt --check` + `script/precheck`
and never run cargo, because per-agent builds made the 3-slot build queue the
bottleneck and then OOMed the box. That tradeoff is still right at 20+ agents.
**But know exactly what it lets through**, because it is predictable:

**Every red PR in the evening round failed for one of three reasons, and none
of them is a logic error:**

1. **A missing trait import in a *test* file.** PR #500: `CodeSettings::handle(app)`
   needs `warpui::SingletonEntity` in scope; the file imported `App` and
   `ModelHandle` but not the trait. Invisible to rustfmt, invisible to plain
   `cargo check` — CI catches it only because both check jobs run
   `--lib --tests`.
2. **A non-exhaustive match after adding an enum variant.** Adding a
   `CLIAgent`/`FeatureFlag`/protocol-op variant breaks every exhaustive match.
   Agents must grep for all of them by hand.
3. **A test asserting a translated string without calling `i18n::init`.** PR
   #473: `t_static!` inside a `LazyLock` permanently caches the raw fluent key
   if it resolves before init, and nextest gives each test its own process. Six
   existing test files already call `crate::i18n::init(Some("en"))` first.

**So: before finishing, an agent should hand-check imports for any trait method
it called, grep every match site for any enum variant it added, and call
`i18n::init` in any test asserting user-visible text.** Those three checks cost
seconds and account for essentially all avoidable CI round trips.

Note also that **`cargo check` in this repo runs with `--lib --tests`**, so a
"check passes" claim from an agent that only ran `cargo check` without `--tests`
is not evidence of anything.

### Operational lessons added 2026-08-07

- **Run `script/precheck` before every push.** CI is the *final* check, not the
  one that discovers the problem. Runs rustfmt on changed files, both guards,
  base freshness, and (with `--with-tests`) only the tests in
  `known_test_failures.txt` — 8, not 4,500.
- **Keep worktrees current** with `script/agent-worktree new|refresh|status`.
  An agent branched 105 commits behind, lacked the merged secret fix, hit the
  resulting failure and called it "unrelated pre-existing"; two people then
  diagnosed a bug fixed hours earlier. `precheck` now fails above 25 behind.
- **Never call a failure "pre-existing" without measuring it.** Wrong four times
  in one round, three of them the coordinator's. Stash, re-run, restore. Not in
  `known_test_failures.txt` → not pre-existing. **Check your base first.**
- **Filtered runs break `#[serial]` tests.** Secrets tests mutate global regex
  state; a narrow `-E` filter can schedule them against tests touching the same
  global. Re-run alone before concluding.
- **Verify before asserting a diagnosis.** Four wrong diagnoses today — the
  `command_finished_and_precmd` helper, a `.start()` grep truncated by `head -8`,
  the #441 inverted-predicate theory, and the secrets-serial hypothesis. Agents
  caught three. Cheap checks settled each in seconds *after* the wrong claim.
- **`pgrep -f <pattern>` matches your own command line.** Three false "still
  running" readings. Poll the governor's `RESULT` line instead.
- **Shared mutable files create merge-order dependencies** —
  `known_test_failures.txt` and `cloud_boundary_allowlist.txt` both do. This bit
  again on 2026-08-07: #420 and `main` deleted *adjacent lines* of that file and
  conflicted. Stack onto the PR that owns the file rather than branching from
  `main` in parallel with it.

### The meta-lesson, added 2026-08-07 evening

**This repo encodes its rules in prose and enforces almost none of them.** Every
incident of that day was a documented rule that nothing checked:

| rule, written down | what actually happened |
|---|---|
| `agent-cargo`'s slot-sizing header | override file set to `6`; fleet OOM-reaped, three agents' uncommitted work nearly lost |
| `rust-toolchain.toml` pins 1.92.0 | host ran 1.93.1 for **weeks**; every local green measured on the wrong compiler |
| `.rustfmt.toml` | says 2018 while all 64 crates say 2024 (#191) |
| "never call a failure pre-existing without measuring it" | wrong four times in one round |

Prose does not execute. When you learn something here, put it in a script that
fails, not in a paragraph. #467 does this for the toolchain; `MAX_SLOTS` does it
for the governor.

### Three "rules" in this repo that are FALSE — verified 2026-08-07

Stop routing around these:

- **`gh pr edit` and `gh pr merge` do NOT fail on this repo.** The round-4 brief
  says to use `gh api` REST instead. The token has `repo` scope, GraphQL works,
  and both commands were used repeatedly that day, including a base retarget.
- **`.rustfmt.toml`'s `edition = "2018"` is not authoritative.** The pin
  (`02b53fcd8`) and `warp/master` both carry `edition = "2024"`; upstream changed
  it in the *same commit* as their edition migration (`abea51cd1`, #13990). The
  fork's own migration missed that one file. `precheck`'s "always `--config-path`,
  never `--edition`" is a **workaround for the churn**, not a statement that 2018
  is correct.
- **`known_test_failures.txt` is NOT authoritative repo-wide.** CI runs
  `check_test_failures -p warp --lib`, so **`warp_tui` is not gated at all** — 620
  tests, two of them red on `main`, invisible. An agent following the documented
  "not on the list → not pre-existing" rule will hunt a regression that does not
  exist. See **#465**.

### The OOM, and why "narrow tests" did not prevent it

Agents were correctly briefed to run only filtered `-E` slices. **It did not
matter.** A filtered `nextest -E … -p warp --lib --features gui` still *compiles
and links the entire warp test binary*; `-E` narrows execution only. The link is
the multi-GB cost and it is identical whether you then run 1 test or 4,500.

Two mechanisms, both now fixed in `agent-cargo`:

- The `slots` override was `6`. Raising 3→**4** had already reaped this box once.
  There is now a `MAX_SLOTS` clamp in code; tuning *down* still works.
- **`MIN_FREE_MB` is a start-time admission check only.** N invocations each
  observe plenty of free memory in the same poll pass, all get admitted, then hit
  peak link together with nothing re-checking. A start stagger now de-synchronises
  them. This, not the raw slot count, is the mechanism.

**Swap is not a fix for this.** 39 GB was added that day. Link memory is *hot*
anonymous memory, so swapping it thrashes rather than relieving; the failure mode
becomes an unresponsive box instead of a fast kill, which is harder to diagnose —
and HANDOFF already records an agent being stopped because it merely *looked* hung.

### Recovering a killed agent — do this first, before anything else

Agent work sits **uncommitted in `.worktrees/<slug>/`**. A worktree removal or a
second OOM loses it permanently.

```bash
for w in .worktrees/*/; do git -C "$w" status --short; done
```

Commit anything found as `WIP checkpoint (<agent>) — UNVERIFIED` immediately, then
verify. Do not read, diagnose, or tidy first. The claim in this file that "every
branch is pushed, nothing lives only on disk" was **false** on 2026-08-07: three
branches existed only on disk.

### #441 — what three failed attempts missed

All three of its failures were **port omissions**, not logic errors, and none was
findable by reading the code the tests obviously touch:

- `/fork` was queued because the slash-command data source **never subscribed to
  `BlocklistAIHistoryModel`**, so `Availability::ACTIVE_CONVERSATION` latched at
  construction and `/fork` never entered `active_commands_by_id`. **The queueing
  code was correct throughout and never saw a slash command.**
- The classic completion menu stayed open because `update_tab_completion_menu`
  **lost the pin's `is_user_edit` parameter**, making the classic exemption
  unconditional instead of system-edits-only. A `TODO` in the fork described the
  resulting bug.

Generalise: **a missing subscription or a dropped parameter is invisible to a diff
of the feature that fails.** When source-diffing "does not converge", stop diffing
and go compare the *wiring* against the pin — subscriptions, call-site counts,
function signatures.

That fix also revealed `slash_compact_still_queues_while_in_progress` had been
passing **for the wrong reason**: with nothing detected, everything queues, which
is exactly what it asserted. A green sibling test is not evidence the mechanism
works.

~~Still missing and unfiled: the fork also lacks the pin's `PrivacySettings` and
`UserWorkspaces` recompute subscriptions — same latching class, no test covers
it.~~ **Struck 2026-08-10 (unwired-code audit): this was a false alarm.** Both pin
subscriptions fire only on cloud-only events, and the fork's `session_context()`
(`crates/warp_tui/src/terminal_session_view/data_source/mod.rs:215-277`) reads
none of them, so there is nothing to recompute. The missing `AIAutoDetectionEnabled`
arm is the same story. Nothing to file.

## The oracle is PINNED — read `ORACLE.md` before any parity work

**Stale below: the pin has since moved again**, to `4111d08f9` (`2026.08.26`
stable, moved 2026-08-29) — `docs/STATE.md` and `ORACLE.md` are current;
this section is kept for the *mechanism* (why re-pins grow the gap, how the
scripts read the pin) which is still accurate, not for the commit hash. Also,
as of 2026-09-05 the pin is evidence/suggestions, not a spec — read
`ORACLE.md`'s "What the pin is for" before treating any gap below as debt.

`warp/master` is unreleased trunk moving **50-80 tests/day**; measuring against it
produces a gap that never shrinks no matter how much lands. That is why sustained
porting felt like no progress.

The oracle is pinned to **Warp `2026.08.12` stable = `42effe840`** (moved
2026-08-15 from `02b53fcd8`, the first real re-pin), and the policy is to track
the **latest stable**, never `master`/`dev`/`preview`. Use that commit in place
of `warp/master` in every diff, grep and measurement.

**Expect the gap to grow at a re-pin, and do not report it as a regression.**
`docs/STATE.md` went from 2,241 absent to **2,795** the moment the pin moved,
because the target gained 834 tests (10,026 -> 10,860 by `script/state`'s
measure). That is the deal `ORACLE.md` makes on purpose. 2,360 of the absent are
adjudicated in the ledger; the **435 unadjudicated** are the files that appeared
or changed between the two pins and are the next round's queue.

**Three scripts read the pin from `ORACLE.md`, and two of them used to lie about
it.** `script/state` matched the *literal* string `02b53fcd8` and fell back to it,
and `script/generate_pin_identity_manifest` had the same hardcoded fallback — so
after a pin move both would have kept measuring the old oracle while labelling
the output with the new pin, silently. Both now read the `**Commit (full)**` row
and exit non-zero if it is missing (`.github/workflows/pr-check.yml` already
did). If you add a fourth reader, do the same; do not add a default.

## The scope is measured, not estimated — `SCOPE-*.md`

**The numbers below are measured at the OLD pin `02b53fcd8` and have not been
re-derived at the current pin** (`ORACLE.md` says so explicitly) — quote them
for the shape, not the count, and take current numbers from `docs/STATE.md`
(generated; as of `0695371a9` it reports 8700/9908 non-cloud pin tests present,
~87.8%, with 0 open in the `MISSING-SUBSYSTEM` ledger bucket and 8 open
`TODO.md` checkboxes).

All 854 test-bearing files at the pin were classified per file by reading source
imports (not paths). Do not re-derive this badly:

```
Warp tests genuinely ABSENT     3,902
  A  test debt (the workload)   1,605   <- ~1,185 still unclaimed
  D  feature gap                  792
  C  out of scope (cloud)       1,505
fork-original offset            1,663
```

**The workload is 1,605, not 2,239 and not 3,902.** The old "2,239 net gap" was
wrong in both directions, and the "468 missing test files" figure is discredited
entirely (it counted `*_tests.rs` -> `*_test.rs` renames as absent).

**Known bias: verdict A is OVERSTATED.** The scope docs classify per *file* and
collapse MIXED files into their majority bucket, so a file that is mostly test
debt hides feature-gap and cloud tests inside an "A" row. The first agent to port
against these verdicts found its 52 "missing" tests were actually **11 A / 31 D /
10 C** — the A verdict was wrong for 79% of them (PR #246; issues #238-#243).

So treat an A verdict as *a file worth opening*, not as a promise that its tests
are portable. **Trace each test's actual API dependencies against the fork source
before porting it**, and file a feature-gap issue rather than inventing the
feature. 1,605 is an upper bound on the workload, not a target.

## Fleet rounds — `docs/FLEET-ROUND.md`

Agents do **not** run the full suite. Their gate is `rustfmt --check` (a real
parser, ~1s) plus at most a `cargo check` on their own small crate; the
coordinator batches one integration run for the whole round. Per-agent full-suite
runs made the 3-slot build queue the bottleneck — agents waited over an hour to
land while the same ~1,000 crates were recompiled per agent.

## Guards — the compiler cannot catch these, so CI does

`pr-check.yml` runs a fast `guards` job (grep/git only, no build):

- **`script/check_cloud_boundary`** — the cloud *crates* were deleted so the
  compiler blocks those, but `app/src/server` and `app/src/cloud_object` survive
  as stubs and nothing stopped a port reaching in and re-growing the cloud
  surface one `use` at a time. Pins 266 import sites; the allowlist may shrink
  freely, growing it needs sign-off.
- **`script/check_stub_coverage`** — a test ported against a gutted no-op stub
  compiles and passes while asserting nothing. Compares against the pin and fails
  only when a co-located test targets a function with a real body upstream and an
  empty one here.

Both were verified by planting a violation. Read their header comments before
"improving" them — three earlier versions of the stub check were wrong in
instructive ways.
## Decisions only the maintainer can make

- **#149 / proto re-pin** — the fork pins `zerx-lab/warp-proto-apis@14ab9a71`, **40
  commits behind** upstream `warpdotdev@b0886a95` (which Warp pins today). The
  fork's proto has **zero additions**, so nothing is load-bearing. A re-pin
  proposal is in progress on `chore/repin-proto-upstream`. Consequence of the
  staleness: `AnyFilesSuccess` has no `failed_reads`, so per-file read failures
  cannot be expressed on the wire at all (#136).
- **#140 sequencing** — merge #171's fixes first, or merge #140 red and fix after.
- **#167** — TUI keybinding validator strategy. Warp exempts TUI bindings from the
  cross-platform validator; the fork drops `cmd-` keys off macOS. Linux TUI users
  currently get no keystroke for Paste / SelectToLineStart-End.
- **#174** — `>` vs `>=` restore-order divergence, currently justified only in a
  source comment with three Warp tests marked "intentionally NOT ported".
- **#11** — the standing ledger: AI global skills, skill remote-path,
  `local_control` app-side.

New this round (2026-09-27), full detail in "Open decisions" above:

- **Decision 8 — `21f413b7` terminal-crate/tmux move** (147 files). Accepted
  in scope 2026-08-29, held pending the maintainer's release of it.
- **fzf/atuin handoff** (`bf2364bc9` + 4 follow-ups, CORE-3807) — queued
  behind decision 8.
- **`511b952c2` — `create_file` gains `allow_overwrite`** — blocked on a
  `warp_multi_agent_api` proto bump.

---

## Highest-value open issues

**Stale as of 2026-09-27 — verify before acting on any item below.** A quick
check found `#171` already closed (PR #224) and `#164`/`#151`/`#143`/`#165`
no longer appear anywhere in `TODO.md`, which is what "resolved and never
reconciled here" looks like (see the "already merged, never closed" pattern
in the 2026-08-07 section below — this list is itself an instance of it).
Current open work is `TODO.md`'s 8 remaining checkboxes
(`docs/STATE.md`'s `MISSING-SUBSYSTEM` ledger bucket is at 0) — see the
"Where main is" section above for the live ones: `#751`, `#769`, `#773`,
`#774`.

Security / correctness first:

- **#171** — 9 ported Warp terminal tests fail. Includes an **OSC 1337 parser panic
  on untrusted PTY output** (`ansi/mod.rs:1073` indexes `params[1]` unguarded;
  Warp guards it) and an **unquoted `cat {history_file}`** (`session.rs:1384`).
- **#164** — `ai::agent::redaction::redact_secrets` has **no tests**. It is the
  function stripping secrets before data leaves the device to a BYOP provider,
  and it rewrites byte ranges in reverse order.
- **#151** — command denylist bypassed by an env-var assignment prefix.
- **#142** — `api_keys` has 7 of Warp's 82 tests. Custom API endpoints — core BYOP —
  untested.
- **#143** — Privacy page never ported: 13 settings live at runtime with no user
  control. PR #160 addresses it.
- **#165** — telemetry setting is user-togglable but nothing consumes it.

The recurring defect class, worth internalising:

- **"Ported but never wired"** — #137 (dead `GetBranches` RPC → empty branch
  dropdown over SSH), #138 (`repo_watch_filter` never called), #146 (4 callerless
  functions), #173 (`write_command` discards `StartCommandOutcome`), #162
  (`dispatch_event` discards a result). `#![allow(dead_code)]` on `app/src/lib.rs`
  and `crates/warpui_core` is why the compiler never flags these.

---

## The #2 sweep — methodology correction

The headline "468 missing test files" figure is **inflated and should not be
quoted**. Two independent agents found the cause: the fork renames Warp's
`*_tests.rs` → `*_test.rs`, and same-path matching counts those as absent.

- Cloud-triage slice: 5 of 23 were never missing.
- Terminal slice: **30 of 48** were renames.

The real loss is test **functions dropped inside renamed files** (~330 in the
terminal subtree alone), which path matching cannot see. **Verify absence by
content, never by filename.** And classify by what a test *targets*, not the
path it sits in — `server/server_api/ai_tests.rs` reads as pure cloud but 14 of
its 40 tests cover retained non-cloud code.

---

## Build governor — `script/agent-cargo` (PR #127)

```
script/agent-cargo <agent-name> <cargo-args...>
```

Current shape, and **why** each part exists:

- **3 slots × 5 jobs.** Started at 3, was raised to 4 and **OOM-killed the whole
  fleet**, dropped to 2 as a panic response, then returned to 3 on measured
  evidence.
- **`NEXTEST_TEST_THREADS` / `RUST_TEST_THREADS` capped.** This was the *actual*
  OOM cause: nextest defaults test-execution parallelism to the CPU count, so a
  **single slot** consumed ~25 GB. Slot count could never have prevented it.
- **Per-agent `CARGO_TARGET_DIR`.** Prevents cross-agent artifact contamination,
  which on the previous host produced phantom "missing symbol" errors and a bogus
  issue.
- **Memory precheck + per-run low-water logging** to `logs/mem-<agent>.log`. Size
  slots from these numbers, not intuition. Worst dip since the fix: 6.6 GB (a cold
  full `warp` build); pre-fix it was 4.4 GB.
- **Live slot re-read.** Editing the script does **not** reach already-queued
  invocations — bash read `SLOTS` at start-up. Change `/cache/git/.phosphor-build/slots`
  instead; it is re-read each poll pass.
- **`RESULT agent=… exit=N OK|FAILED` on stderr.** Piping stdout through
  `head`/`tail`/`grep` replaces the pipeline's exit code with the filter's. This
  reported a failed build as success **twice in one day** (`|| true` and `| head`).
  **Trust the RESULT line, never the pipeline status.**

---

## Host setup (this box)

24 cores / 45 GB (~28 GB usable after resident VMs) / 561 GB free on `/cache`.

Installed by hand this session because the bootstrap had never run here: **mold**
(`.cargo/config.toml` hard-requires `-fuse-ld=mold`; without it every link fails
with a misleading `cannot find 'ld'`), **protoc 25.1**, **gh 2.83**,
**cargo-nextest**, and **rustfmt** (via apt — see below).

**Toolchain divergence — FIXED 2026-08-07 (#467). Read this anyway.**

`script/install_rust` guarded on `command -v cargo`, i.e. "any cargo will do", so
the distro cargo at `/usr/bin/cargo` short-circuited it, rustup was never
installed, and `rust-toolchain.toml`'s `channel = "1.92.0"` pin was **silently
ignored**. This host ran **rustc 1.93.1 against a 1.92.0 pin for weeks.**

That is not a tidiness issue. CI runners have rustup and therefore honour the pin,
so **every local green measured in that window was measured on the wrong
compiler** — and the skew runs the dangerous direction: anything stabilised in the
newer local compiler builds clean here and fails in CI. Any "verified" claim on
this repo dated before 2026-08-07 should be read with that in mind.

Now enforced in three places, each verified by planting the failure:

- `install_rust` compares rustc against the pin instead of accepting any cargo
- `precheck` **step 0** asserts toolchain identity, and self-corrects when rustup
  is installed but not first in PATH
- `agent-cargo` prepends the rustup shims and **refuses to build** on a mismatch

Shell config cannot cover this: `~/.bashrc` returns early for non-interactive
shells, which is exactly how agents, hooks and CI helpers invoke cargo. The line
sourcing `~/.cargo/env` on this box is deliberately placed **above** that guard.

Installing rustup invalidates every target dir, so do it between fleets, not
mid-round. rustfmt also comes from apt here (PR #153).

---

## Operational lessons — do not relearn these

1. **Subagents cannot receive asynchronous notifications.** Not
   `run_in_background`, not Monitor, not "I'll wait until it reports back". Four
   agents were lost to this today, one twice. Every brief must say so explicitly,
   naming *both* mechanisms — an agent told only "no run_in_background" reached
   for Monitor instead.
2. **Exit-status masking.** See the RESULT line above. It bit me and an agent on
   the same day.
3. **Capture before you stop.** I killed an agent and discarded its worktree before
   reading its reasoning; its transcript was empty and the analysis was
   unrecoverable. Read the diff and the report *first*, stop second.
4. **File the issue before dispatching.** I dispatched agents at findings and
   skipped the issue repeatedly, so several pieces of live work existed only in
   chat. §5.11 exists because a finding must survive an agent failing.
5. **Do not trust a self-reported green.** Auditing caught overstated claims three
   times, including one of my own. Read the diff, verify the key claim against
   `warp/master`, grep for new "Zap", confirm zero weakened asserts and no
   `#[ignore]`.
6. **Check whether the work is already done.** Twice I nearly dispatched agents at
   things already fixed.
7. **The scratchpad is shared, not agent-isolated.** One agent overwrote another's
   file. Prefix scratch filenames with the agent name.
8. **Verify the union before merging** — but know when it adds nothing. If the PR
   and main touch disjoint crates with no dependency between them, disjointness is
   a stronger argument than a green build.
9. **`cd` inside every cargo command string; never rely on persisted shell cwd.**
   Three agents hit this in one session. The Bash cwd defaults to `/cache/git/zap`
   (the main checkout), and a backgrounded call resets it for subsequent commands
   — the tool says so explicitly: *"Session cwd remains /cache/git/zap; directory
   changes made by the backgrounded command do not apply to subsequent commands."*
   The failure is **silent and worse than an error**: cargo happily builds the
   unmodified main-branch tree and reports a clean green that means nothing. One
   agent only caught it by inspecting cargo's `.fingerprint` dep-info files and
   noticing they listed the *old* file set. Write
   `cd <checkout>/.worktrees/<name> && agent-cargo …` as one string, every time.
   (The literal paths in this item are the old `/cache/git/zap` host, which no
   longer exists; the checkout is now `/home/winters/git/phosphor`. The lesson
   is the `cd`, not the path.)
10. **A baseline number is only valid for the tree it was measured on.** The
   "4025/0/33" figure circulated all session as if it were main's; it was PR #132's
   *branch* number. It made a clean re-pin look like it had lost 22 tests and
   nearly cost a full re-measurement. The real baseline at `44bf4daa6` is
   **4005/0/33** (measured directly, and independently corroborated by two agents'
   deltas). Always state which commit a count belongs to.
11. **Check call-site *counts* against the oracle, not just symbol presence.** PR
   #195 ported `readable_chip_label_color` verbatim but wired it to 1 of Warp's 2
   call sites, leaving `chip_configurator` sub-WCAG-AA (#196). A same-symbol grep
   reports that as covered. This is the "ported but never wired" class in its
   hardest-to-spot form.

---

## What a fresh session should do first

1. Merge **#139 + #166** (child first). Stops the suite mutating the real prefs file.
2. Decide **#140** sequencing (red suite).
3. Pick up the **proto re-pin** on `chore/repin-proto-upstream` — checkpointed but
   incomplete; its verification bar is the full 4025-test suite.
4. Work the security-relevant issues: **#171**, **#164**, **#151**.
5. Rewrite `TODO.md` per **#148** — it is stale in both directions and one entry
   states the opposite of the truth, actively misdirecting readers.

**Do NOT assume every branch is pushed.** This file used to claim that, and on
2026-08-07 it was **false**: three agent branches existed only on disk, with their
work uncommitted, when the host OOM killed the fleet. Check before you trust it:

```bash
for w in .worktrees/*/; do
  b=$(git -C "$w" branch --show-current)
  echo "$b  uncommitted=$(git -C "$w" status --porcelain | wc -l)  onremote=$(git ls-remote --heads origin "$b" | wc -l)"
done
```

In-progress agent work should be committed as `WIP checkpoint (<agent>)` — those
are unverified and may not compile; re-verify before building on them.

## What a fresh session should verify before trusting this file

Everything above is a snapshot, and several of its predecessors' claims turned out
to be wrong in ways that cost real time. Cheap checks, in order:

1. `./script/precheck` — if step 0 fails, **stop**; every measurement you take
   will be on the wrong compiler.
2. `git log --oneline -5 origin/main` — the "Where main is" section goes stale
   within hours during a round.
3. `grep -vcE '^\s*(#|$)' script/known_test_failures.txt` — the count in this file
   has been wrong in both directions.
4. `for w in .worktrees/*/; do …` — the loop above, before deleting any worktree.

And the standing rule that outranks this whole document: **verify before asserting
a diagnosis.** Five wrong diagnoses were recorded on 2026-08-07 alone, one of them
from misreading this file's own guidance about `.rustfmt.toml`.
