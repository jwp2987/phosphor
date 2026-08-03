# Warp parity sweep — fix & feature scope

> **STATUS: PLANNING (2026-08-02).** The test-parity sweep is complete: ~150 Warp
> tests ported back onto branch `warp-test-parity-sweep`, ~41 regressions filed,
> and the #11 feature-gap ledger triaged + BYOP-decided. **Nothing is fixed or
> built yet** — the sweep was strictly tests-first, so every red test already maps
> to an issue. This doc is the durable tracker for the two remaining workstreams.

## Context

- **Branch:** `warp-test-parity-sweep` (8 commits; tree clean; compiles).
- **Oracle:** the `warp/master` git remote (real Warp) — `git show warp/master:<path>`.
- **Tracking:** GitHub issues on `jwp2987/phosphor` — `#3`–`#47` (bugs), `#11` (feature-gap ledger).
- **Workflow (AGENTS.md §5.11):** issue → branch → PR (`Fixes #N`, Warp-mirrored test) → merge. Security-relevant deviations need maintainer sign-off (§5.10).
- **Suite health:** full `cargo test -p warp --lib --features gui,tui,local_fs` = 3997 pass / 31 fail / 33 ignored. The 31 fails = ~26 tracked red regression tests + 5 FD-exhaustion flakies (`#24`).
- **Verify command (flock-serialized, MANDATORY):**
  `ulimit -n 8192; flock /home/winters/.claude/jobs/d323e5af/tmp/zap-cargo.lock -c 'cargo test -p <crate> --lib [--features gui,tui,local_fs] 2>&1 | tail -60'`

Note: `#11` (features to build) and the bug issues (regressions in existing code)
are **mostly disjoint** — building features does NOT fix most bugs. Do the two
workstreams independently. Recommended order: **Workstream A first** (live defects,
tests already written), then Workstream B.

---

## Workstream A — Bug fixes (~41)

Each bug has a committed **red regression test** (except where noted "no test /
code-level"). Fix = make the red test green by porting Warp's behavior; do NOT
weaken the assertion. Order below is the recommended fix order.

### A1 — Security / privacy (do first)
| Issue | Bug | Area |
| --- | --- | --- |
| **#22** | OSC 52 clipboard read/write unconditional (Warp defaults Deny) | terminal/view + settings |
| **#25** | Browser open has no URL-scheme allowlist (`safe_browser_open_url`) | crates/warpui |
| **#7** | `file_glob` agent tool shell-command injection (no quoting) | app/src/ai |
| **#12** | `Event::PluggableNotification` Debug leaks title/body to logs | terminal/event |

### A2 — Crashes / panics
| Issue | Bug | Area |
| --- | --- | --- |
| **#33** | markdown delimiter count `u8` overflows → PANIC on 256+ `*`/`_`/`~` | crates/markdown_parser |
| **#39** | `FlatStorage` clear-after-truncate+resize+push → PANIC (offset corruption) | crates/warp_terminal |
| **#35** | `TextFileAccumulator` returns reversed (invalid) line range | crates/warp_files |

### A3 — Correctness: data & behavior
| Issue | Bug | Area |
| --- | --- | --- |
| **#3** | agent LRC auto-resume broken after Ctrl-C (stuck "Warping…", Warp #12738) | app/src/ai/blocklist |
| **#6 / #20** | `strip_prefix` string-not-component match → `/repo` matches `/repository` | crates/warp_util |
| **#8** | `read_documents` silently succeeds on missing doc IDs | app/src/ai |
| **#9** | `request_file_edits` swallows diff-application failures | app/src/ai |
| **#10** | `GlobalBufferModel::apply_client_edit` mis-applies batched edits | app/src/code |
| **#31** | `AgentConversation::is_restorable()` rejects legacy stub+root shape | crates/persistence |
| **#37** | SSH teardown kills a shared ControlMaster (no `warp_owns_control_master`) | app/src/remote_server (code-level, no oracle test) |
| **#38** | `rc_file_paths` host-native separators (Windows client → Unix host) | crates/warp_terminal |
| **#45** | `fuzzy_match_file_diffs` drops suffix on partial final line | crates/ai |
| **#46** | child-agent conversations not excluded from nav list (metadata lacks parent fields) | app/src/ai/blocklist |
| **#47** | `fork_conversation` has no empty-source guard (silently forks empty) | app/src/ai/blocklist |
| **#21** | shell env drops `HISTSIZE`/`WARP_INITIAL_HISTSIZE` sentinels | app/src/terminal/local_tty |

### A4 — Correctness: UX & rendering
| Issue | Bug | Area |
| --- | --- | --- |
| **#13** | `replace_unicode_word_boundaries` mishandles `\b`/`\B` | terminal/model/find |
| **#14** | Vim counted line-jump (`5gg`, `d5gg`) ignores count | crates/vim |
| **#15** | warpui_core ignores `TuiView` in `view_name()`/dispatch | crates/warpui_core |
| **#16** | `WeakHandle::upgrade()` doesn't invalidate after last strong drop | crates/warpui_core |
| **#17** | `Theme::current_value_is_syncable` ignores path (all custom = non-syncable) | app/src/settings |
| **#18** | mermaid YAML frontmatter directives no longer parse | crates/editor |
| **#19** | input_classifier dropped shell keywords (agy/omp) → NLD misclass | crates/input_classifier |
| **#23** | `to_extension()` missing markdown/md | app/src/ai |
| **#26** | no-op `insert_at_char_offset_ranges` skips `set_version` → spurious save dialog | crates/editor |
| **#27** | text run background painted after underline (hides inline-code-link underline) | crates/warpui_core |
| **#28** | vim visual-mode paste after history recall appends instead of replacing | app/src/editor |
| **#29** | external_editor doesn't drop deprecated FreeDesktop field codes | app/src/util |
| **#30** | link detection doesn't stop at fullwidth/CJK punctuation | app/src/util |
| **#32** | markdown `parse_url` loses backslash-escapes + trailing punctuation | crates/markdown_parser |
| **#34** | markdown parser has no `<!-- -->` comment stripping | crates/markdown_parser |
| **#36** | `.command` files not runnable (`is_runnable_shell_script` missing "command") | app/src/util |
| **#40** | `to_escape_sequence()` missing backspace fallback (impact unconfirmed) | crates/warp_terminal |
| **#41** | closing active tab activates left/above instead of right/below | app/src/workspace |
| **#42** | theme chooser / left panel zeroes tab-bar traffic-light padding | app/src/workspace |
| **#43** | `initial_vertical_tabs_panel_open` dropped guard → reintroduces #9505 | app/src/workspace |
| **#44** | `restore_conversation_in_active_pane` missing already-live fast path | app/src/workspace |

### A5 — Test-suite health
| Issue | Item |
| --- | --- |
| **#24** | full `warp --lib` run exhausts file descriptors → 5 false view-test failures (fix the watcher fd leak or raise the harness ulimit) |

### A-overlap (also closed by Workstream B)
`#17` (theme, ↔ B theme-syncability), `#18` (mermaid, ↔ B mermaid-config), `#22`/`#25`
(security, ↔ B OSC52/browser), `#46`/`#47` (↔ B history_model), `#10` (↔ B pending-edit-batch, if that's the fix). Sequence so a fix isn't done twice.

---

## Workstream B — Feature builds (#11 ledger, BYOP-adapted)

Ticked items in `#11`, minus the 4 dropped, with cloud halves removed. Grouped by
cluster; build non-cloud behavior + port the Warp tests that then become portable.

### B1 — Terminal / rendering
- OSC-8 clickable hyperlinks (`hyperlink_registry`, `flat_storage/hyperlink`, cell/ANSI wiring) — also the `HyperlinkRegistry` ledger item
- Procedural box-drawing glyph rendering (`grid_renderer/box_drawing`)
- `tmux_passthrough` DCS wrapper
- Block-lifecycle transition coordinator (`model/lifecycle`)
- `warp_tui` terminal-background live re-probe
- Banner-immune interactive PATH capture (markers + `extract_captured_path`)
- File-link trailing-sentence-punctuation stripping (also bug #30-adjacent)

### B2 — Editor / UI
- Editor relative line numbers (`CodeEditorLineNumberMode`)
- Soft-wrap row bounds + Home/End on soft-wrapped lines
- Mermaid diagram config (↔ bug #18)
- Oversized data-URI image handling; content-version-aware asset invalidation; Image load failure/timeout fallback; `TuiStack`
- Theme syncability scope (↔ bug #17) — **note:** only meaningful if the fork's own (gist) sync covers themes
- Pending-edit-batch conflict-discard — **verify SSH-remote vs cloud-collab first** (↔ bug #10)

### B3 — AI (BYOP)
- Configurable/expanded context window — **build config; drop the cloud pricing warning**
- Code-symbol AI context source (`search/ai_context_menu/code`)
- AI **bundled + global** skills — **drop the `remote` cloud arm**
- `history_model` reconciliation — **build:** rename, event-sequence, prompt_history_candidates, todo_projections, remove-child/parent, fork-arity, `transient_network_error` resume. **Drop:** cloud-metadata-merge / remote-child / hydrate-placeholder / canonical-id reservation. (Closes bugs #46, #47.)
- Skill path resolution over SSH (`LocalOrRemotePath`/`RemotePath`/`HostId`)

### B4 — SSH / remote (non-cloud)
- `remote_server_controller` connection-label helpers
- `ModelEventDispatcher` SSH-remote-server gate (`SshRemoteServerSupport`)
- Remote/SSH global search (`remote_matches_to_global`)
- `repo_metadata` lazy/budget file-tree loading + standing queries (large; spans skills/file_tree/remote_server/search)
- code_review over SSH (`diff_state/{local,remote}`, git/github repo models)
- context_chips git-branch tracking (`GitBranchTrackingStatus`, PR chip)

### B5 — Platform / infra
- **`local_control` / `warpctrl` scripting IPC** (~3000 lines; local Unix-socket + loopback-HTTP broker) — biggest single item
- Launch-at-login (`app/src/login_item`)
- Size-based log rotation (`simple_logger`) + `warp_logging` native rotation (Warp's unbounded-MCP-log fix)
- Persistence DB-model — **build pinned tabs / tab groups / summary+backfill / provider-cost; drop `team_uid`** (↔ bug #31 is separate)
- WSL program translation (`command/wsl.rs`); WSLENV passthrough vars; Windows `PATHEXT` resolution
- Autoupdate exit-code parsing — **adapt per-channel repo to the fork's own release repo**
- URI local deep-links (`UriHost::Session`, `TabConfig`, `OpenFileEditor`)
- Deep-link focus-URL env plumbing (`FOCUS_URL_ENV`)
- Async (background-thread) find; queued-prompts-while-busy panel
- Jupyter-notebook file detection; `file_uri_drive_path_to_windows`; `warp_util::sync::Condition`
- Managed-secrets BYO APIs (`validate`/size-limit, `seal_with_context`, BYO-endpoint payloads)
- Cross-window tab-drag placeholder collapse; editable bindings (`toggle_maximize_pane`)
- NLD heuristic feature flags (restore `nld_heuristic_v1/v2` → regain input-classifier coverage)
- SettingSurfaces / SettingsMode / `surfaces_fn` (unblocks privacy/tui setting tests)

### B — Dropped (maintainer, 2026-08-02) — do NOT build
OTEL trace-link header · VoiceInputLifecycle · AI codebase semantic-search · computer_use recording/overlay (and the `StartRecording` action). All need a cloud/BYO backend the fork lacks. Plus the already-cloud items left unticked in #11 (RunAgents orchestration, cloud-mode-v2, IsCloudConversationStorageEnabled, product telemetry, api::impl pending).

---

## Progress

Nothing fixed or built yet. Update this section as PRs land (`Fixes #N`).

| Phase | Done | Total | Notes |
| --- | --- | --- | --- |
| A — bug fixes | **42** | 43 | DONE on branch `parity-fixes` (2026-08-03). Only #37 remains (groundwork-only, open — needs `external_control_master` plumbing). Full run 4036 pass / 5 fail = the FD flakies (#24, pass in isolation). Not pushed. |
| B — feature builds | 0 | ~40 ticked (BYOP-adapted) | see #11 |

Fix-phase notes: #48/#49 were surfaced by the final full-suite verification (un-filed reds). Deliberate scoped adaptations noted on their issues: #15 (TuiView→fork's tui_views map), #17 (theme path-scoped starts_with), #30 (CJK ranges vs general-category crate). Verify app/src fixes centrally; run full `cargo test -p warp` in the FOREGROUND (background jobs get reaped).

---

## Related (non-parity)

- **`specs/remove-ssh-manager/SCOPE.md`** — remove the fork-original SSH Manager +
  SFTP browser + `zap_sync` gist-sync cluster (maintainer decision; not Warp-derived,
  so outside this parity effort).
