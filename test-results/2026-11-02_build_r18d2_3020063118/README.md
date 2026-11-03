# build/r18d2  332fd97b2  (pipeline 3020063118)

- date: 2026-11-02, tier: fix, pipeline status: manual
- compile (check-linux): ok, 49 warnings
- unit: 10557 passed, 7 failed, 29 skipped of 10564 run (1 suites)
- app (pass/fail/skip): macos 27/6/0 +2 no-ref; wayland 24/8/2 +6 no-ref; windows 32/19/12 +1 no-ref; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.2 | - |  |
| app-test-linux-wayland | failed | 25.8 | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | failed | 30.2 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-windows | failed | 68.4 | phosphor-windows-vm |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 33.8 | - |  |
| build-macos-arm64 | success | 20.0 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | success | 36.4 | phosphor-windows-vm |  |
| check-linux | success | 6.4 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| guards | success | 1.7 | - |  |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| licenses | success | 0.2 | - |  |
| mutants | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| test-linux | failed | 8.9 | - |  |
| test-shuttle-loom | manual | - | - | not run |
| test-warpui-core | manual | - | - | not run |
| test-workspace-rest | manual | - | - | not run |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-linux failures

- `warp terminal::view::tests::keys_held_across_the_prompt_all_go_to_the_pty_once_and_the_editor_stays_empty` FAIL (0.1s)
- `warp terminal::view::tests::ssh_bootstrap_timer_second_expiry_after_the_grace_still_fails_the_session` FAIL (0.1s)
- `warp terminal::view::tests::ssh_handover_holds_the_text_after_a_nested_ssh_for_the_second_connect` FAIL (0.2s)
- `warp terminal::view::tests::ssh_line_typed_across_gate_release_is_never_run_locally` FAIL (0.1s)
- `warp terminal::view::tests::table_mode_a_control_key_behind_a_gate_is_delivered_at_the_gates_event` FAIL (0.1s)
- `warp terminal::view::tests::table_mode_a_ctrl_c_in_the_ssh_setup_window_is_held_and_reaches_the_remote_shell_once` FAIL (0.1s)
- `warp terminal::view::tests::table_mode_a_held_signal_reaches_the_remote_shell_once_at_the_handover` FAIL (0.1s)

## app macos (app-test-macos)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | PASS | phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; reference / perline: first char lost on 0/12 lines, other bad 0,  |
| terminal | enter_loss_control | PASS | reference Terminal.app (.command via open) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted  |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| terminal | ssh_connect_typing | PASS |  |
| terminal | ssh_enter_then_immediate_command | PASS |  |
| terminal | ssh_nested_typeahead | PASS |  |
| terminal | latency_enter | FAIL | Enter to command start: Phosphor overhead 9.77 ms over Terminal.app (.command via open) exceeds the 2 ms budget; app share of Enter->start:  |
| terminal | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as |
| terminal | latency_paint | PASS | Key to paint: kl3 measures post-to-first-changed-pixel at 240 Hz in each terminal's own window; reference measured after Phosphor in the sam |
| terminal | routing_table | FAIL | 'find the bug in main.rs' (ambiguous) expected agent, went to shell (surface AgentConversation, rule evidence-in-conversation (#1183)); shel |
| terminal | shell_fish | PASS |  |
| terminal | shell_nu | PASS | nu app log: 2026-11-03T00:10:25Z [DEBUG] frame 1: updated=["WarpifyFooterView", "PaneView", "ActionButton", "Terminal", "ActionButton", "Pan |
| terminal | md_viewer | PASS |  |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS |  |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| agent | ssh_connect_typing | PASS |  |
| agent | ssh_enter_then_immediate_command | PASS |  |
| agent | ssh_nested_typeahead | PASS |  |
| agent | latency_enter | FAIL | Enter to command start: Phosphor overhead 9.50 ms over Terminal.app (.command via open) exceeds the 2 ms budget; app share of Enter->start:  |
| agent | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as |
| agent | latency_paint | FAIL | Key to paint: Phosphor overhead 7.72 ms over Terminal.app (.command via open) exceeds the 2 ms budget; Key to paint: kl3 measures post-to-fi |
| agent | agent_stream_cancel | FAIL | the screen kept changing 6 s after Ctrl-C (4 distinct frames in 5): the stream did not stop |
| agent | routing_table | FAIL | 'find the bug in main.rs' (ambiguous) expected agent, went to shell (surface AgentConversation, rule evidence-in-conversation (#1183)); shel |
| agent | shell_fish | PASS |  |
| agent | shell_nu | PASS | nu app log: 2026-11-03T00:17:48Z [DEBUG] frame 1: updated=["ActionButton", "RightPanelView", "Terminal", "ActionButton", "ActionButton", "Wo |
| agent | md_viewer | PASS |  |

## app wayland (app-test-linux-wayland)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | FAIL | Phosphor lost keys the reference keeps with the SAME injection (perline-unprimed): Phosphor 12/12 bad lines (first char lost 12), foot 0/12  |
| terminal | enter_loss_control | PASS | reference foot gap 0.00s: 30/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted None Enters; reference foot |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | NOREF | NO-REFERENCE: Ctrl-C: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metri |
| terminal | ssh_connect_typing | PASS |  |
| terminal | ssh_enter_then_immediate_command | PASS |  |
| terminal | ssh_nested_typeahead | PASS |  |
| terminal | latency_enter | NOREF | app share of Enter->start: enter_begin -> pty_write median 0.40 ms (from the app's input_order log); the remaining overhead is shell/ConPTY  |
| terminal | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal | routing_table | PASS | ambiguous 'find the bug in main.rs': went to shell (no routing log line, so no rule to check); ambiguous 'make a PR': went to shell (no rout |
| terminal | shell_fish | PASS |  |
| terminal | shell_nu | PASS | nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its directory was added to PATH in the pane; nu app log: 2026-11-03T00:20:1 |
| terminal | md_viewer | PASS |  |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS |  |
| agent | fast_typing | PASS |  |
| agent | left_agent_mode_after_fast_typing | FAIL | after fast_typing the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent cases measur |
| agent | paste_instant_enter | FAIL | paste + instant Enter: 0/6 md5 digest lines exact (first got [], expected ['5adab3d97a990aa6e1bec241daf9082a']) |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | NOREF | NO-REFERENCE: Ctrl-C: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metri |
| agent | ssh_connect_typing | PASS |  |
| agent | left_agent_mode_after_ssh_connect_typing | FAIL | after ssh_connect_typing the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent cases |
| agent | ssh_enter_then_immediate_command | PASS |  |
| agent | ssh_nested_typeahead | PASS |  |
| agent | latency_enter | NOREF | app share of Enter->start: enter_begin -> pty_write median 1.64 ms (from the app's input_order log); the remaining overhead is shell/ConPTY  |
| agent | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as |
| agent | left_agent_mode_after_latency_key_log | FAIL | after latency_key_log the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent cases me |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| agent | agent_stream_cancel | FAIL | text typed at 10-25 ms/key while the reply streamed is not visible in the input ('keepmet115_qm09d8') |
| agent | routing_table | PASS | ambiguous 'find the bug in main.rs': went to shell (no routing log line, so no rule to check); ambiguous 'make a PR': went to agent (no rout |
| agent | left_agent_mode_after_routing_table | FAIL | after routing_table the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent cases meas |
| agent | shell_fish | PASS |  |
| agent | shell_nu | PASS | nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its directory was added to PATH in the pane; nu app log: 2026-11-03T00:33:3 |
| agent | left_agent_mode_after_shell_nu | FAIL | after shell_nu the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent cases measure a |
| agent | md_viewer | PASS |  |

## app windows (app-test-windows)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | cadence | PASS |  |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 En |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times -- PowerShell: Ctrl-C did not stop the sleep (no stamp within 6 s, or one that p |
| terminal | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.1 s): T153_8vpgec never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stop |
| terminal | ssh_enter_then_immediate_command | PASS |  |
| terminal | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T163_v68aku never reached the REMOTE file (s |
| terminal | latency_enter | FAIL | Enter to command start: Phosphor overhead 86.47 ms over conhost (powershell.exe) exceeds the 2 ms budget; app share of Enter->start: enter_b |
| terminal | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| terminal | shell_fish | SKIP | not run in the PowerShell pane: fish as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | shell_nu | SKIP | not run in the PowerShell pane: nu as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | md_viewer | SKIP | no verified way to hand a file:// URL to the RUNNING Windows instance (a second invocation may start a second app instance on the same data  |
| terminal@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=terminal); no OCR engine |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | cadence | PASS |  |
| terminal@gitbash | key_loss_control | PASS | phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; reference / perline: first char lost on 0/12 lines, other bad 0,  |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; referen |
| terminal@gitbash | fast_typing | PASS |  |
| terminal@gitbash | paste_instant_enter | FAIL | pasted lines reached the shell exactly: 5/6 byte-exact (first mismatch: 'count 5 vs 6') |
| terminal@gitbash | tab_enter | PASS |  |
| terminal@gitbash | ctrl_c_d | FAIL | Ctrl-C: Phosphor overhead 20.10 ms over conhost (bash) exceeds the 20 ms budget; Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times |
| terminal@gitbash | ssh_connect_typing | PASS |  |
| terminal@gitbash | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T194_duje2j never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; st |
| terminal@gitbash | ssh_nested_typeahead | PASS |  |
| terminal@gitbash | latency_enter | FAIL | Enter to command start: Phosphor overhead 16.97 ms over conhost (bash) exceeds the 2 ms budget; app share of Enter->start: enter_begin -> pt |
| terminal@gitbash | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as |
| terminal@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal@gitbash | routing_table | FAIL | 'find the bug in main.rs' (ambiguous) expected agent, went to shell (surface AgentConversation, rule evidence-in-conversation (#1183)); shel |
| terminal@gitbash | shell_fish | SKIP | package fish (apt install fish) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bi |
| terminal@gitbash | shell_nu | SKIP | package nushell (not in Ubuntu repos: github.com/nushell/nushell release tarball, or `cargo install nu --locked`; Mac: brew install nushell) |
| terminal@gitbash | md_viewer | SKIP | no verified way to hand a file:// URL to the RUNNING Windows instance (a second invocation may start a second app instance on the same data  |
| agent | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times -- PowerShell: Ctrl-C did not stop the sleep (no stamp within 6 s, or one that p |
| agent | left_agent_mode_after_ctrl_c_d | FAIL | after ctrl_c_d the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent cases measure a |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | ssh_connect_typing | PASS |  |
| agent | ssh_enter_then_immediate_command | PASS |  |
| agent | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T85_39yn2q never reached the REMOTE file (ss |
| agent | agent_stream_cancel | SKIP | not run in the PowerShell pane: uses rec.log to prove nothing reached the shell: runs in the Git Bash pane (needs the CI model) |
| agent | routing_table | SKIP | not run in the PowerShell pane: asserts what the shell received from rec.log: runs in the Git Bash pane |
| agent@gitbash | mode_proof | PASS | mode machine-verified from the app log (hop=view_mode mode=agent); no OCR engine |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | fast_typing | FAIL | ft0: shell received 31 commands, not the typed ones (first mismatch: "echo t1_ieqh524yng >> 'C:/gl/b/07xLPX_fG/0/winters-phosphor/phosphor/a |
| agent@gitbash | paste_instant_enter | FAIL | pasted lines reached the shell exactly: 5/6 byte-exact (first mismatch: 'count 5 vs 6') |
| agent@gitbash | tab_enter | FAIL | Tab+Enter: completed file has 1/12 expected lines |
| agent@gitbash | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times -- Phosphor: 14/33 sample lines reached the shell NOT exactly as typed; e.g. dif |
| agent@gitbash | left_agent_mode_after_ctrl_c_d | FAIL | after ctrl_c_d the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent cases measure a |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | ssh_connect_typing | PASS |  |
| agent@gitbash | ssh_enter_then_immediate_command | PASS |  |
| agent@gitbash | ssh_nested_typeahead | PASS |  |
| agent@gitbash | agent_stream_cancel | FAIL | no model request ('[byop] opening stream') within 60 s of the prompt's Enter; prompt not routed to the agent or provider unreachable |
| agent@gitbash | routing_table | FAIL | 'find the bug in main.rs' (ambiguous) expected agent, went to shell (surface AgentConversation, rule evidence-in-conversation (#1183)); shel |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## vs previous run

- NEW failures (unit): 
  - `warp terminal::view::tests::keys_held_across_the_prompt_all_go_to_the_pty_once_and_the_editor_stays_empty [test-linux]`
  - `warp terminal::view::tests::ssh_bootstrap_timer_second_expiry_after_the_grace_still_fails_the_session [test-linux]`
  - `warp terminal::view::tests::ssh_handover_holds_the_text_after_a_nested_ssh_for_the_second_connect [test-linux]`
- FIXED failures (unit): 
  - `warp terminal::input_queue::facts::table_enum_tests::every_row_of_the_table_is_reachable [test-linux]`
  - `warp terminal::view::tests::ssh_bootstrap_failure_delivers_held_typeahead_after_stty_sane [test-linux]`
  - `warp terminal::view::tests::ssh_ctrl_c_ends_the_hold [test-linux]`
  - `warp terminal::view::tests::ssh_ctrl_d_is_held_after_a_prompt_was_answered [test-linux]`
  - `warp terminal::view::tests::ssh_editor_backspace_in_the_gap_edits_the_held_text [test-linux]`
  - `warp terminal::view::tests::ssh_editor_keys_typed_before_delivery_stay_behind_the_held_keys [test-linux]`
  - `warp terminal::view::tests::ssh_keys_after_ssh_block_ends_before_remote_session_are_dropped [test-linux]`
  - `warp terminal::view::tests::ssh_keys_between_bootstrapped_and_first_remote_prompt_are_dropped_whole [test-linux]`
  - `warp terminal::view::tests::ssh_middle_click_paste_with_a_pending_read_keeps_typed_order_around_it [test-linux]`
  - `warp terminal::view::tests::ssh_paste_with_a_pending_read_keeps_typed_order_around_it [test-linux]`
  - `warp terminal::view::tests::ssh_queue_enter_runs_the_held_line_once [test-linux]`
  - `warp terminal::view::tests::ssh_queue_interrupt_discards_the_held_keys [test-linux]`
  - `warp terminal::view::tests::ssh_queue_key_queued_in_the_prompt_turn_lands_after_the_held_keys [test-linux]`
  - `warp terminal::view::tests::ssh_queue_keys_during_connect_are_delivered_once_in_order [test-linux]`
  - `warp terminal::view::tests::table_mode_arrivals_reach_the_same_destinations_as_legacy [test-linux]`
  - `warp terminal::view::tests::table_mode_text_replayed_into_a_new_ssh_connect_is_held_then_delivered_once [test-linux]`
  - `warp terminal::view::tests::text_replayed_into_a_new_ssh_connect_is_held_not_dropped [test-linux]`
  - `warp terminal::view::tests::typing_on_the_ssh_extension_choice_card_shows_a_hint_instead_of_vanishing [test-linux]`
- NEW failures (app): 
  - `macos/terminal/routing_table (case not in previous run)`
  - `macos/agent/latency_paint`
  - `macos/agent/agent_stream_cancel (case not in previous run)`
  - `macos/agent/routing_table (case not in previous run)`
  - `wayland/terminal/key_loss_control`
  - `wayland/agent/left_agent_mode_after_fast_typing (case not in previous run)`
  - `wayland/agent/paste_instant_enter`
  - `wayland/agent/left_agent_mode_after_ssh_connect_typing (case not in previous run)`
  - `wayland/agent/agent_stream_cancel (case not in previous run)`
  - `wayland/agent/left_agent_mode_after_routing_table (case not in previous run)`
  - `wayland/agent/left_agent_mode_after_shell_nu (case not in previous run)`
  - `windows/terminal@gitbash/ssh_enter_then_immediate_command (case not in previous run)`
  - `windows/terminal@gitbash/latency_enter (case not in previous run)`
  - `windows/terminal@gitbash/routing_table (case not in previous run)`
  - `windows/agent/left_agent_mode_after_ctrl_c_d (case not in previous run)`
  - `windows/agent@gitbash/left_agent_mode_after_ctrl_c_d (case not in previous run)`
  - `windows/agent@gitbash/agent_stream_cancel (case not in previous run)`
  - `windows/agent@gitbash/routing_table (case not in previous run)`
- FIXED failures (app): 
  - `macos/terminal/shell_nu`
  - `macos/agent/ssh_nested_typeahead`
  - `macos/agent/shell_nu`
  - `wayland/terminal/ssh_connect_typing`
  - `wayland/terminal/shell_nu`
  - `wayland/agent/ssh_connect_typing`
  - `wayland/agent/shell_nu`
  - `windows/terminal/ssh_enter_then_immediate_command`
  - `windows/terminal@gitbash/key_delivery`
  - `windows/terminal@gitbash/fast_typing`
  - `windows/terminal@gitbash/tab_enter`
  - `windows/terminal@gitbash/ssh_connect_typing`
  - `windows/agent/ssh_connect_typing`
  - `windows/agent/ssh_enter_then_immediate_command`
  - `windows/agent@gitbash/key_delivery`
  - `windows/agent@gitbash/key_delivery`
  - `windows/agent@gitbash/ssh_connect_typing`
- Latency regressions > 5%: 
  - `macos/terminal/latency_key_log key_to_frame: 7.08 -> 8.24 ms (+16.4%)`
  - `wayland/terminal/latency_enter enter_to_start: 33.62 -> 40.42 ms (+20.2%)`
  - `wayland/terminal/latency_key_log key_to_frame: 0.99 -> 5.34 ms (+439.4%)`
  - `windows/terminal@gitbash/ctrl_c_d ctrl_c: 79.65 -> 87.01 ms (+9.2%)`
- compared against: test-linux: build/r18cz (3019641121); app macos: build/r18cz (3019641121); app wayland: build/r18cz (3019641121); app windows: build/r18cz (3019641121)
