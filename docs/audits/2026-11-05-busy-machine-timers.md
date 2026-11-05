## Busy-machine timer audit: Phosphor at /home/winters/git/phosphor-upgrade (branch upgrade/rust-1.99)

Owner principle (2026-11-05): "the app shouldn't need a perfect machine to run on." Read-only Opus audit of every timer whose expiry changes behaviour. Every HIGH and MEDIUM finding is filed as its own GitLab issue (label busy-machine); LOW items are tracked in one umbrella issue.

This was a read-only audit; nothing was edited. Paths below are relative to that repo.

I left out ENTER_VERDICT_BUDGET (20 ms), since it is already being fixed. Its siblings are rated below, because the 20 ms fix does not cover them unless it changes them too.

All timers here measure wall-clock time unless marked otherwise. The ComputeClock used by the verdict and Tab-completion caps (`app/src/ai/blocklist/compute_clock.rs:33`) is also wall-clock: once the job starts, time the job spends descheduled still counts. #1577 changed most give-ups to hand the keys back to the editor instead of writing them. That makes them safe only when the handed-back line is guarded. Several of them are not guarded, which leaves the glued-line bug class open.

Each risk level below is set by what happens when the timer fires early. The "Hit?" column says separately whether a slow but healthy machine can plausibly reach the deadline.

## 1. HIGH (33)

| # | Where (file:line) | Value | What happens when it fires | Hit? | Suggested fix |
|---|---|---|---|---|---|
| H1 | AFTER_ENTER_REPLAY_PTY_WAIT `terminal/view.rs:820` (armed :13139; `input/enter_queue.rs:1095`) | 300 ms | The keys typed after a shell-command Enter are handed back to the editor before the pty has taken the command. The gate then closes, so later keys go to the now-running program. One typed run is split: the head waits in the editor for the next prompt and the tail goes to the program. | Yes. 300 ms is inside the normal stall range. | Release only on events (pty write done, Preexec, block start). Use a net of 10 s or more. On give-up keep holding later keys behind the handed-back ones. |
| H2 | Enter verdict hold backstop `terminal/input.rs:1478` | 250 ms + large-text allowance | `submit_without_verdict` runs, then `hand_back_unsubmitted_enter`. The line goes back with no guard (`enter_queue.rs:887-891`: "a line given up for a verdict ... Enter sends it"). Keys typed afterwards glue onto it and the next Enter runs the glued line. This is the same bug class as the one being fixed. | Yes. On a saturated thread pool the job may not even start within 250 ms. | Fold it into the current fix: back stop only for a lost job (dropped channel), or 5 s or more. Always arm the hand-back guard. |
| H3 | YOUNG_DETECTION_CAP `model/typeahead_handoff.rs:402` | 5 × ENTER_VERDICT_BUDGET (100 ms now), ComputeClock (wall-clock) | A young line's Enter is not submitted; it goes through `keep_deferred_enter_text_hand_back`. I found no guard armed there (please verify). | Yes. | CPU time, as in the fix. Guarded hand-back. |
| H4 | YOUNG_DETECTION_BACKSTOP `view.rs:828` | 500 ms + allowance | Same as H3, when no verdict arrives at all. | Yes, if the classifier job is queued. | Back stop only on a lost job; 5 s or more. |
| H5 | LARGE_PASTE_VERDICT_FLOOR and large_text_compute_allowance `ai/blocklist/input_model.rs:2571` | 100 ms floor, ComputeClock (wall-clock) | A pasted buffer is handed back. The comment notes this already happened on a loaded test run (r18u4). | Yes. | Same fix as the 20 ms budget: CPU time. |
| H6 | TAB_COMPLETION_ENTER_BUDGET `input.rs:1453` | 150 ms, ComputeClock (wall-clock) | `give_up_completion_hold` submits the visible, uncompleted line ("Tab then fast Enter" runs `cd proj` instead of `cd project-x/`). A command runs that the user did not mean. | Yes. | Hand back instead of submitting (#1577 style). Use CPU time. |
| H7 | Tab-completion hold backstop `input.rs:1480` | 500 ms | Same as H6. | Yes. | Same. |
| H8 | YOUNG_ENTER_WINDOW (hold-or-not decision) `typeahead_handoff.rs:88`, `:2855` | 600 ms | Timing path only: ConPTY, sessions with no sample, older remote bootstraps, `PHOSPHOR_PROMPT_SEQ=0`. Outside the window an Enter typed at a prompt the app has not processed yet is written to the shell as unclassified typeahead, so a chat line runs in the shell (#933). | Yes. Always on Windows ConPTY. | Decide on "stream not yet applied" (the reader-side barrier) instead of elapsed time. |
| H9 | CONPTY_MIDLINE_QUIET `typeahead_handoff.rs:289` | 120 ms | Timing path: the `ESC i` request is written between two keys of a line still being typed. ConPTY / MSYS readline then lose the line (per its own doc comment). | Yes. Batched key delivery after a stall looks like quiet, and slow typists pause over 120 ms. | Use `writes_in_flight == 0` and the queue boundary (the sequence path rule) on every path. |
| H10 | TYPEAHEAD_HANDOFF_STALE_COUNT_RESET `writeable_pty/pty_controller.rs:172` | 250 ms | Timing path: lines the counts say are still queued are treated as consumed, and the request is written. `ESC i` can reach a running program, and the editor takes input while queued lines still run, so keys are reordered. | Yes. | Event-only reset (sampled owner or Precmd), or seconds. |
| H11 | TYPEAHEAD_HANDOFF_STALE_COUNT_GIVE_UP `pty_controller.rs:179` | 5 s | Same as H10 on the sequence path where nothing can be sampled (ssh remote, ConPTY), for example a queued `make` that runs longer than 5 s. | Plausible. | Keep waiting where nothing can be sampled; hand back instead of resetting the counts. |
| H12 | LINE_EDITOR_ACTIVATION_DELAY `terminal/line_editor_status.rs:11` | 50 ms | For shells with no ready signal, the line editor is assumed active 50 ms after precmd, and the request or a queued editor command is written. Its own doc says the aim is to avoid sending an escape sequence to a running program, and a slow shell defeats that. | Yes. | Require the ready signal (bracketed paste, LineEditorReading); otherwise wait about 1 s or more. |
| H13 | INTERRUPT_SETTLE `view.rs:14812` | 250 ms | Legacy, no-sequence path: input is handed to the editor before zsh's prompt cycle finishes. The late cycle reinitializes the editor "under the user" (its own doc), which can wipe keys typed after the takeover. | Yes, legacy path only. | Wait for the Precmd or report event (as the sequence path does). |
| H14 | NATIVE_COMPLETIONS_PROMPT_TIMEOUT `pty_controller.rs:142` | 2 s | The ^Y handshake is abandoned and queued writes resume. A shell hook that answers late then reads the "text to complete" from the tty, which by then holds the user's keys, so they can be eaten. It also shows a false "no shell integration" toast. (Inferred from the protocol; please verify.) | Plausible on a busy or remote shell. | Learn per session whether the hooks exist (from the bootstrap). Where they exist, wait with ^C as the escape, not a timer. |
| H15 | BOOTSTRAP_FAILED_DURATION `view.rs:924` | 7 s | ssh: `ssh_bootstrap_failed_session` is set, swallowing stops and held keys are handed back. Keys typed afterwards go straight to the remote tty while the injected bootstrap may still be read (the #983 hazard). Local shells: a "slow bootstrap" banner. | Yes. Heavy rc files on a loaded machine or VM. | Extend while bootstrap output or markers progress. After give-up keep holding keys until the session is ready or provably dead. |
| H16 | BOOTSTRAP_PROGRESS_GRACE `view.rs:928` | +3 s, once | Same outcome as H15. It only applies if the bootstrap Precmd was already seen. | Yes. | Repeat while progressing, not once. |
| H17 | SSH_BOOTSTRAP_SWALLOW_CEILING `view.rs:931`, `view/ssh_edges.rs:239` | 15 s | Same: keys go into the tty during an unfinished remote bootstrap. | Plausible on a slow VM. | Progress-based. |
| H18 | SSH_CONNECT_SILENCE_CEILING `view.rs:936` (macOS and other platforms without the #1641 tty-read probe) | 15 s | Held keys are handed back to the local editor and the window is released, so later keys go to ssh. The line is split between local and remote. A guard stops the head running locally. | Plausible (slow auth, slow VM). | Extend the tty-read probe, or keep holding until the user acts. |
| H19 | SUBSHELL_BOOTSTRAP_STALL_CEILING `view.rs:940`, `:19228`, `:38679` | 20 s | The subshell bootstrap is abandoned ("plain shell"). Later keys are forwarded while the setup lines still sit in the tty, so the late bootstrap reads them as part of itself. | Yes (`poetry shell` or `nix develop` on a slow VM). | Progress-based; keep holding. |
| H20 | SUBSHELL_SETUP_TYPED_HOLD_LIMIT `view.rs:974` | 90 s | Same as H19 (`route_facts.rs:126` lets keys through past the limit). | Low. | Same. |
| H21 | SSH_LOGIN_RECHECK_GIVE_UP `view.rs:790`, `:35359` | 3 s | After non-prompt output, 3 s of quiet is taken as "login done", and auto-warpify writes the bootstrap to whatever reads the tty (an rc menu, an unrecognised auth prompt). | Moderate (.hushlogin hosts, slow remote rc). | Use the remote shell's own marker or the tty-read probe; never quiet time. |
| H22 | HANDOFF_GATE_LEAK_NET `view/paste_queue.rs:292` | 60 s | The only gate kind that still falls through to the generic `InputQueue::give_up` (`input_queue/mod.rs:1127`), which drains held keys through the routing stages, so a timer can write user keys to the pty. | Very low (3 s patience timers end it first). | Route it to `hand_back_gate_keys` like the other kinds. |
| H23 | CLI_AGENT_PTY_WRITE_DELAY, mode-switch prefix `view/use_agent_footer/mod.rs:97`, `:872` | 50 ms | A busy CLI agent (Claude Code `!`) reads the prefix and the rest in one read, so a bash-mode command is sent as an AI prompt. Input goes to the wrong place; the consequence is usually mild. | Yes. | No input-consumed signal exists. Write the prefix, then wait for the agent's mode-change redraw, or warn. |
| H24 | CLI_AGENT_IMAGE_PASTE_DELAY `use_agent_footer/mod.rs:106`, `:986` | 300 ms | The clipboard is overwritten with the next image before a busy agent has read the previous one. The wrong image is sent, one is duplicated and one is lost. | Yes (agents read the clipboard via a subprocess). | Much longer gap or confirmation, or pass file paths instead of the clipboard. |
| H25 | HANGUP_GRACE `terminal/local_tty/process_tree.rs:124` | 300 ms | Closing a pane sends SIGKILL to the foreground program if it has not exited 300 ms after SIGHUP. Editor saves, git index writes or DB flushes are killed mid-cleanup. | Yes. | 3–5 s, escalating in the background. |
| H26 | MIGRATION_LOCK_BUDGET `persistence/sqlite.rs:2056` | 3 s | The database is given up for the whole run ("won't be saved" notice), so nothing from this session is persisted. | Plausible (relaunch racing the previous instance's shutdown on a slow disk; #1650 measured Windows waits running long). | Keep retrying in the background, as the writer does; buffer writes. |
| H27 | TRANSIENT_OPEN_RETRY_BUDGET `sqlite.rs:1899` | 3 s | Same as H26. | Plausible. | Same. |
| H28 | DEFERRED_SENDS_QUIT_WAIT `persistence/mod.rs:315` | 2 s | Quit stops the writer before the last snapshot is sent, so recent state is lost. | Plausible. | Wait while the writer progresses, bounded only by QUIT_GIVE_UP. |
| H29 | SHUTDOWN_DEADLINE `warpui/src/platform/termination_signals.rs:51` | 5 s | A signal-initiated shutdown (logout, SIGTERM) exits regardless, and final writes are dropped. | Moderate. | Progress-based extension. |
| H30 | QUIT_GIVE_UP `platform/shutdown_progress.rs:22` | 15 s | Same as H29. | Low. | Same. |
| H31 | Preferences FLUSH_TIMEOUT `warpui_extras/src/user_preferences/background_writer.rs:160` | 3 s | The last settings change is not saved at quit. | Low–moderate (slow disk). | Same. |
| H32 | EXIT_PENDING_WRITE_WAIT `warpui/src/windowing/winit/linux/clipboard.rs:41` | 300 ms | The clipboard is handed over at quit without the in-flight copy, so the copied text is lost. | Moderate. | Raise; the wait is already event-driven. |
| H33 | EXIT_HANDOFF_BUDGET `linux/clipboard.rs:42` | 600 ms | Same as H32. | Moderate. | Same. |

## 2. MEDIUM (68)

Most of these are hand-backs with a toast, guarded or safe, or features that degrade.

| # | Where | Value | When it fires | Hit? / fix |
|---|---|---|---|---|
| M1 | YOUNG_ENTER_MIN_HOLD `typeahead_handoff.rs:94` | 150 ms | Timing-path young hold handed back (`paste_queue.rs:1688`, with the "check the line" guard): the Enter does not run. | Yes. Measure the stream barrier; floor of 500 ms or more. |
| M2 | YOUNG_ENTER_FLOOR, and RTT p90 × 3 `:99` | 20 ms | Same as M1. Round trips learned on a quiet machine leave a 20–60 ms hold for the next stall. | Very likely. Raise the floor. |
| M3 | EVENT_HOLD_GIVE_UP `:266` | 2 s | Event-held young Enter handed back, guarded. | Plausible. Raise or keep. |
| M4 | CONPTY_PROMPT_HOLD_GIVE_UP `:318`, also the enter-window cap `:1215` | 1.5 s | Keys with no Enter go back to the editor; an Enter segment stays held with a toast. Enter window with no Preexec within 1.5 s gives up. | Yes on a Windows VM. Use 5 s or more. |
| M5 | PROMPT_HOLD_RELEASE_IN_FLIGHT_RECHECK `:323`, stale give-up at 2 × cap `:1227` | 100 ms / 3 s | Rechecking is fine (GOOD). At 3 s a hold whose release is already decided but stuck behind a busy queue is handed back instead. | Low. Never give up an in-flight release. |
| M6 | UNREADY_ENTER_GIVE_UP `enter_queue.rs:87` | 15 s | Line plus following keys handed back, guarded, toast. | Slow VM. |
| M7 | ENTER_GATE_LEAK_NET `enter_queue.rs:55` | 10 s | Hand-back. | Low. |
| M8 | UNCOVERED_REPORT_PATIENCE `view.rs:38717` | 3 s | Enter not run; text kept. | Slow ssh. |
| M9 | TYPEAHEAD_REPORT_PATIENCE `view.rs:38723` | 3 s | Enter not run; held keys go to the editor; a late report is inserted in front. | Slow ssh. |
| M10 | INTERRUPT_GIVE_UP `view.rs:14815` | 2 s | The editor takes input after ^C without a prompt cycle. | Plausible. |
| M11 | AGENT_INTERRUPT_GRACE `view.rs:815` | 3 s | Conversation cancelled and agent view exited; focus moves to the terminal, so next keys go to the still-running command. | Yes (slow SIGINT cleanup). Do not move focus automatically; show a banner. |
| M12 | SSH_CONNECT_FIRST_PROMPT_GRACE `view.rs:38727` | 2 s | Held keys handed back, guarded. | Yes (slow remote prompt). Event or longer. |
| M13 | SSH_CONNECT_POST_BLOCK_GRACE `:38713` | 5 s | Same as M12. | Plausible. |
| M14 | SSH_TYPEAHEAD_PRECMD_WAIT_CAP `view/ssh_typeahead.rs:31` | 5 s | Handed back (the doc comment still says "delivered anyway", which is stale). | Plausible. |
| M15 | SSH_WARPIFY_TIMEOUT_DURATION `terminal/ssh/mod.rs:12` | 8 s | "Warpification unavailable: Timeout" error block; session not warpified. | Slow VM. |
| M16 | ENV_VAR_BOOTSTRAP_FAILED_DURATION `view.rs:995` | 60 s | Same path as H15, but for env-var sessions. | Low. |
| M17 | SUBSHELL_FIRST_PROMPT_GIVE_UP `view.rs:962` | 5 s | Bootstrap written before the first prompt; an rc program can read it. | Plausible. |
| M18 | SUBSHELL_SETUP_MARKER_GIVE_UP `view.rs:950` | 5 s | Remaining setup lines are written blind on timers (30 / 300 ms), which can break setup or answer an rc prompt. | Plausible. |
| M19 | SUBSHELL_SETUP_DEFER_QUIET `view.rs:970` | 600 ms | Setup written after 600 ms of quiet output, even if the program is still processing the answer. | Plausible. |
| M20 | subshell_bootstrap_line_delay `terminal/bootstrap.rs:639` | 30 / 300 ms | Fallback pacing; PSReadLine loses input mid-redraw (#1047). | Windows. |
| M21 | RELAY_BOOTSTRAP_PACING `pty_controller.rs:58` | 25 ms | Relay (sudo, su) slower than the pacing deadlocks or drops, so the bootstrap fails. | Moderate. |
| M22 | CONTAINER_BOOTSTRAP_PACING `:63` | 50 ms | Same, for docker exec. | Moderate. |
| M23 | MAC_TTY_INPUT_QUEUE_PACING `:72` | 10 ms / 1000 B | A canonical tty silently drops input past its queue if the reader is slower. | Moderate. |
| M24 | POWERSHELL_ABORT_SETTLE `view.rs:985` | 500 ms | Held keys handed back. | Windows. |
| M25 | POWERSHELL_ABORT_MARKER_GIVE_UP `:990` | 5 s | Hand-back. | Low. |
| M26 | POWERSHELL_GATE_LEAK_NET `paste_queue.rs:185`, Subshell/PowerShell 90 s cap (`input_queue/gates.rs`) | 180 s / 90 s | Hand-back. | Low. |
| M27 | CLI_SUBMIT_LEAK_NET `paste_queue.rs:181` | 20 s | Hand-back. | Low. |
| M28 | CLI DelayedEnter (CLI_AGENT_PTY_WRITE_DELAY) `use_agent_footer:1138` | 50 ms | Text and `\r` arrive in one read, so the prompt is not submitted or the Enter becomes a newline. | Yes. |
| M29 | CLI_AGENT_BRACKETED_PASTE_ENTER_DELAY `:101` | 300 ms | Same as M28. | Yes. |
| M30 | paste_backstop `terminal/paste_sequencer.rs:24` | 3 s | Paste discarded with a toast; typing kept. | Busy clipboard owner. |
| M31 | FULL_READ_BUDGET `linux/clipboard.rs:37` | 2 s | Same as M30. | Same. |
| M32 | PROBE_BUDGET `:31` | 750 ms | Rich (html / image) paste falls back to plain text. | Plausible. |
| M33 | EditorDisplayMap gate cap `input_queue/gates.rs:202` | 1 s | Generic drain: keys applied on a stale layout (arrow keys). | Low. |
| M34 | EDITOR_TAKEOVER_GIVE_UP `view.rs:14752` | 10 s | Non-printable keys no longer applied to the editor after a takeover. | Low. |
| M35 | SELECT_WAIT_GIVE_UP `search/ai_context_menu/view.rs:215` | 3 s | Enter in the @-menu dropped (log only); a lost key. | Plausible. Hand back with a toast. |
| M36 | NATIVE_COMPLETIONS_RESULTS_TIMEOUT `pty_controller.rs:149` | 15 s | Completion abandoned. | Low. |
| M37 | PROJECT_RULES_GIVE_UP `ai/blocklist/controller/project_rules_wait.rs:21` | 10 s | Agent request sent without the AGENTS.md rules, which may hold safety rules. Silent. | Swapping machine. Block or toast. |
| M38 | Agent shell MAX_WAIT_DURATION `ai/blocklist/action_model/execute/shell_command.rs:158` | 2 s | The agent sees less output from a slow command and may decide differently. | Yes. |
| M39 | INTERACTIVE_PROMPT_QUIET_EXPLICIT `model/block/interaction_mode.rs:765` | 3 s | Agent command handed to the user as "blocked on input". | Plausible. |
| M40 | INTERACTIVE_PROMPT_QUIET_TRAILING `:770` | 15 s | Same as M39. | Low. |
| M41 | STALL_CLOSE_PAGER_AFTER `:964` | 3 s | `q` written to an identified pager. | Low risk. |
| M42 | STALL_HAND_TO_USER_AFTER `:967` | 5 s | Full-screen app handed to the user. | Plausible. |
| M43 | harness STALL_POLL_INTERVAL `ai/agent_sdk/driver/harness_output_monitor.rs:30` | 10 s | Pattern hit believed after 10 s with the block unchanged. | Plausible. |
| M44 | GREP_TIMEOUT `execute/grep.rs:40` | 10 s | Agent tool returns a timeout, and the agent acts on it. | Big repo on a loaded machine. |
| M45 | FILE_GLOB_TIMEOUT `execute/file_glob.rs:32` | 10 s | Same as M44. | Same. |
| M46 | DAEMON_READY_TIMEOUT `remote_server/unix/proxy.rs:210` | 10 s | Remote server falls back to ControlMaster (degrades). | Slow remote. |
| M47 | wait_for_socket TIMEOUT `proxy.rs:332` | 10 s | Same as M46. | Same. |
| M48 | setup CHECK_TIMEOUT `crates/remote_server/src/setup.rs:765` | 10 s | Remote server feature degrades or reinstalls. | Slow VM. |
| M49 | INSTALL_TIMEOUT `:768` | 60 s | Install fails. | Low. |
| M50 | SCP_INSTALL_TIMEOUT `:771` | 120 s | Install fails. | Low. |
| M51 | RECONNECT_ATTEMPT_TIMEOUT `crates/remote_server/src/manager.rs:49` | 15 s | Falls back. | Low. |
| M52 | SPECULATIVE_CONNECT_TIMEOUT `:59` | 15 s | Falls back. | Low. |
| M53 | TERMINAL_SESSION_BOOTSTRAP_TIMEOUT `ai/agent_sdk/driver/terminal.rs:39` | 60 s | Agent run fails to start. | Low. |
| M54 | MCP_SERVER_STARTUP_TIMEOUT `ai/agent_sdk/driver.rs:79` | 60 s | MCP server missing. | Low. |
| M55 | Terminal probe PROBE_DEADLINE `warpui_core/src/runtime/terminal_probe.rs:24` | 100 ms | A late reply leaks into stdin as typed bytes (TUI input, or the shell after exit). | Plausible over ssh. |
| M56 | DA1_DRAIN_GRACE `:36` | 20 ms | Same as M55. | Same. |
| M57 | Multi-click interval `warpui_core/src/multi_click.rs:12`, measured at processing time (`winit/event_loop/mod.rs:266`) | 400 ms | After a stall, separate clicks merge into a double or triple click. | Yes. Use OS event timestamps. |
| M58 | LONG_PRESS_DURATION `winit/event_loop/mod.rs:70`, processing time | 500 ms | A tap becomes a right-click (touch only). | Yes. Use event timestamps. |
| M59 | ENTER_OR_EXIT_CONFIRMATION_WINDOW `ai/blocklist/agent_view/controller.rs:68`, processing time | 1 s | Presses compressed by a stall count as a confirmation, or a confirmation is missed. | Plausible. Use event timestamps. |
| M60 | ESCALATION_MIN_INTERVAL `termination_signals.rs:55` | 1 s | A duplicate signal counts as "exit now", skipping the graceful path. | Low. |
| M61 | RASTER_DEADLINE `warpui_core/src/image_cache.rs:1072` | 10 s | Image marked failed for the whole session. | Swapping. Allow retry. |
| M62 | secure storage DEFAULT_GIVE_UP `warpui_extras/src/secure_storage/background.rs:39` | 30 s | Keyring call abandoned; credential missing. | Low. |
| M63 | GENERATOR_RUN_TIMEOUT `completer/generator_cache.rs:53` | 20 s | Completions missing. | Low. |
| M64 | remote DEFAULT_COMMAND_TIMEOUT `model/session/command_executor/remote_command_executor.rs:42` | 60 s | Same as M63. | Low. |
| M65 | DEFAULT_CHIP_COMMAND_TIMEOUT `context_chips/current_prompt.rs:147` | 30 s | Chip missing. | Low. |
| M66 | EXECUTE_PENDING_COMMAND_DELAY `view.rs:679` (only with `PHOSPHOR_BOOTSTRAP_TIMED=1`) | 100 ms | Remote `cd` written on a timer. | Legacy only. |
| M67 | AUTO_WARPIFY_DELAY `view.rs:1062` (legacy timed mode) | 1 s | Bootstrap written on a timer. | Legacy only. |
| M68 | MERMAID_RENDER_TIMEOUT `crates/editor/src/content/mermaid_diagram.rs:76` | 8 s | Diagram not rendered. | Swapping. |

## 3. LOW (89 listed by name, plus about 60 background constants in one group)

These are cosmetic, logs, debounces or polls.

- **Rendering (14):** SYNC_OUTPUT_MAX_TIMEOUT 150 ms (forced redraw mid-sync, tearing only; `model/ansi/mod.rs:75`), ALT_SCREEN_RESEND_GIVE_UP 500 ms, ALT_SCREEN_SETTLE_GIVE_UP 500 ms, WAKEUP_THROTTLE_PERIOD, WAKEUP_BYPASS_FLOOR, STALE_REFLOW_CHUNK_BUDGET, STALE_REFLOW_CHUNK_INTERVAL, SELECTION_SNAPSHOT_SLICE, draw_retry BASE_RETRY_DELAY, MAX_RETRY_DELAY, CONFIGURE_SURFACE_WAIT 100 ms (reports success), glyph DEFAULT_RASTER_BUDGET 8 ms (glyphs deferred one frame), tui MIN_REPAINT_DELAY, mac `window.rs:1766` 16 ms.
- **Notices and toasts (9):** PASTE_PROGRESS_TOAST_DELAY, CLIPBOARD_WRITE_SLOW_TOAST_DELAY, SLOW_BOOTSTRAP_BANNER_AUTO_DISMISS_DURATION, MIN_TOAST_INTERVAL, COPY_FEEDBACK_DURATION, DEFAULT_MESSAGE_DURATION, TRANSIENT_HINT_DURATION, AUTO_APPROVE_FEEDBACK_DURATION, request_user_attention 1 s.
- **Logs and diagnostics (17):** PTY_PARSE_SLOW, PTY_WRITE_SLOW, PTY_ECHO_SLOW, PTY_LOCK_WAIT_LOG, PTY_WRITE_GATE_STALL_LOG_THRESHOLD, SLOW_ENTER_HOLD_THRESHOLD, TAB_COMPLETION_ENTER_WARN, model_lock_trace SLOW_HOLD, WARN_HOLD, UI_ITEM_BUDGET, UI_SLOW_THRESHOLD, SLOW_FRAME, SLOW_ACQUIRE, SKIPPED_FRAME_LOG_INTERVAL, HUNG_UP_WARN_AFTER, REPLY_GATE_SLOW_AFTER, EXIT_STATUS_WAIT_TIMEOUT 200 ms.
- **Debounce, hover and repaint (30):** view DEBOUNCE_PERIOD 40 ms, DEBOUNCE_INPUT_DECORATION_PERIOD, DEBOUNCE_AI_QUERY_PREDICTION_PERIOD, REMOTE_AS_YOU_TYPE_DEBOUNCE, SEARCH_DEBOUNCE_PERIOD, QUERY_DEBOUNCE_PERIOD ×2, PROMPT_DEBOUNCE_PERIOD, PLUGIN_CHIP_DEBOUNCE, REMOTE_EDIT_DEBOUNCE, DRAG_DROP_DEBOUNCE_TIMEOUT (a stall delays the timer and the events equally), APPEND_COALESCE_WINDOW, COMMAND_X_RAY_HOVER_DELAY, HOVER_DEBOUNCE_PERIOD, TAB_SHORTCUT_HINT_REVEAL_DELAY, NEW_MODEL_CHOICES_POPUP_DELAY, CURSOR_BLINK_INTERVAL ×2, LIVE_COMMAND_DURATION_REPAINT_INTERVAL, EDITOR_HEIGHT_SHRINK_DELAY, UNDO_REDO_BATCH_TIMER, UNDO_REDO_TIMER, FULL_VIEW_DWELL, ASK_USER_QUESTION_AUTO_ADVANCE_DELAY, INITIAL_RESULTS_TIMEOUT ×2, SEARCH_ASYNC_BATCH_INTERVAL, find THROTTLE_INTERVAL, TARGET_LOCK_HOLD, MAX_BATCH_AGE.
- **Polls (6):** SUBSHELL_SETUP_DEFER_POLL, REPLY_GATE_POLL, REPLY_GATE_POLL_SLOW, HUNG_UP_POLL, HUNG_UP_POLL_SLOW, terminal_attributes POLL_INTERVAL.
- **Miscellaneous (13):** CTRL_C_CANCEL_WINDOW (dead code), misroute OFFER_WINDOW 120 s, wrapper QUIT_STOP_BUDGET, STOP_CONTROL_MASTER_TIMEOUT, desktop-settings FIRST_ANSWER_WAIT, PREFETCH_TIMEOUT (theme defaults), TEXT_READ_TIMEOUT_FLOOR, TEXT_READ_RETRY_PAUSE, path_probe GIVE_UP 3 s (value stays "unknown"), STALL_SNAPSHOT_AFTER, STALL_PAGER_UNRESPONSIVE_AFTER, notebook SAVE_PERIOD, AI-document SAVE_PERIOD.
- **Background group (about 60, not rated one by one):** network, autoupdate, TTL, telemetry, indexing and agent-event constants, including models_dev and gitlab fetches, autoupdate download and poll, PR_INFO and REPOSITORY_INFO, MCP tool-call and cancel timeouts, OAuth CALLBACK, voice STREAM, websocket proxy, the cache TTLs, embedding and reindex cadences, IDLE_GRACE, ORPHAN_SOCKET_GRACE, MAX_AGENT_DELAY_DURATION, MAX_UNTIL_COMPLETION_DURATION, WARP_DRIVE_SYNC_TIMEOUT, AUTO_RESUME_TIMEOUT. None of these touches input routing.

**GOOD** (event-driven, with a give-up net that is safe):
- PENDING_COMMAND_PRECMD_GIVE_UP: runs nothing (#1588).
- POWERSHELL_HELD_LINE_GRACE: toast only.
- subshell_bootstrap_gate_cap: re-arms while setup lines are still being written.
- The managed ssh hold (#1641): no clock where the tty-read probe works, and the SshConnect gate has no cap.
- The multiline dialog: no timer, and nets are ignored while the dialog waits (`enter_queue.rs:745`).
- The SQLite writer: never drops writes; retries every 2 s, then 30 s (`sqlite.rs:146`).
- IME handling in winit: event-driven, no timers found.
- The agent ^C path: discards parked copies and writes ^C directly.
- The alt-screen settle check: driven by the frame-drawn event.

## Totals

| Level | Count |
|---|---|
| HIGH | 33 |
| MEDIUM | 68 |
| LOW | 89 listed + about 60 in the background group |
| GOOD | 9 |

## 4. Top 5 to fix first

1. **H1 AFTER_ENTER_REPLAY_PTY_WAIT (300 ms).** It sits on the path of every shell-command Enter followed by typing, and it is the smallest routing give-up. When it fires, one typed run is split between the editor and the running program, which is a reorder under the owner's rule. Release on pty or Preexec events, and keep later keys behind the handed-back ones.
2. **The verdict siblings the 20 ms fix may not cover (H2–H5).** These are the 250 ms and 500 ms wall-clock backstops, YOUNG_DETECTION_CAP and the large-paste floor. ComputeClock counts descheduled time, and the editor hand-back arms no guard ("Enter sends it"), which is the exact glued-line mechanism just found. Convert ComputeClock to CPU time, make the backstops fire only for a lost job, and arm the guard on every hand-back.
3. **Tab completion (H6, H7).** On expiry it submits the uncompleted line, so a command runs that the user did not intend. It should hand back like the other #1577 give-ups, and use CPU time.
4. **The hand-off's timing path on ConPTY and unsampled sessions (H8–H12).** That is YOUNG_ENTER_WINDOW 600 ms, CONPTY_MIDLINE_QUIET 120 ms, STALE_COUNT_RESET 250 ms / 5 s and LINE_EDITOR_ACTIVATION_DELAY 50 ms. Windows ConPTY always takes this path, and these timers can lose a line, send chat to the shell, or put `ESC i` into a running program. Move them to the stream-barrier, queue-boundary and ready-signal events the sequence path already uses.
5. **Bootstrap give-ups that open the keyboard to a still-running bootstrap (H15–H19).** That is 7 s (+3 s), 15 s, 15 s and 20 s. Slow VMs exceed these routinely, and afterwards typed keys go into the tty mid-script. Extend them while bootstrap output or markers progress, and keep holding keys until the session is ready or provably dead.

Close behind on data loss: H25 HANGUP_GRACE (SIGKILL after 300 ms) and H26/H27 SQLite (the database dropped for the whole run after 3 s).

**Things to verify:** I inferred some outcomes from doc comments and nearby code rather than tracing them end to end. These are the key split in H1, key consumption in H14, the editor reinitialization in H13, the post-failure key path in H15, and whether `keep_deferred_enter_text_hand_back` (H3) arms a guard.

### Critical Files for Implementation
- /home/winters/git/phosphor-upgrade/app/src/terminal/view.rs
- /home/winters/git/phosphor-upgrade/app/src/terminal/input.rs
- /home/winters/git/phosphor-upgrade/app/src/terminal/input/enter_queue.rs
- /home/winters/git/phosphor-upgrade/app/src/terminal/model/typeahead_handoff.rs
- /home/winters/git/phosphor-upgrade/app/src/terminal/writeable_pty/pty_controller.rs
- /home/winters/git/phosphor-upgrade/app/src/terminal/view/paste_queue.rs
- /home/winters/git/phosphor-upgrade/app/src/ai/blocklist/compute_clock.rs