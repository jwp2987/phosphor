# build/r18cz  7f0be20b3  (pipeline 3019641121)

- date: 2026-11-02, tier: fix, pipeline status: manual
- compile (check-linux): ok, 49 warnings
- unit: 10529 passed, 22 failed, 29 skipped of 10551 run (1 suites)
- app (pass/fail/skip): macos 25/5/0 +2 no-ref; wayland 20/5/2 +6 no-ref; windows 26/29/5; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.2 | - |  |
| app-test-linux-wayland | failed | 14.6 | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | failed | 30.9 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-windows | failed | 67.4 | phosphor-windows-vm |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 33.9 | - |  |
| build-macos-arm64 | success | 19.9 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | success | 36.4 | phosphor-windows-vm |  |
| check-linux | success | 7.1 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| guards | success | 1.6 | - |  |
| insta-snapshots | manual | - | - | not run |
| integration-linux-pwsh | manual | - | - | not run |
| integration-linux: [1] | manual | - | - | not run |
| integration-linux: [2] | manual | - | - | not run |
| integration-linux: [3] | manual | - | - | not run |
| licenses | success | 0.2 | - |  |
| mutants | manual | - | - | not run |
| mutants-warpui-core | manual | - | - | not run |
| test-linux | failed | 7.2 | - |  |
| test-shuttle-loom | manual | - | - | not run |
| test-warpui-core | manual | - | - | not run |
| test-workspace-rest | manual | - | - | not run |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-linux failures

- `warp terminal::input_queue::facts::table_enum_tests::every_row_of_the_table_is_reachable` FAIL (0.4s)
- `warp terminal::view::tests::ssh_bootstrap_failure_delivers_held_typeahead_after_stty_sane` FAIL (0.1s)
- `warp terminal::view::tests::ssh_ctrl_c_ends_the_hold` FAIL (0.1s)
- `warp terminal::view::tests::ssh_ctrl_d_is_held_after_a_prompt_was_answered` FAIL (0.1s)
- `warp terminal::view::tests::ssh_editor_backspace_in_the_gap_edits_the_held_text` FAIL (0.1s)
- `warp terminal::view::tests::ssh_editor_keys_typed_before_delivery_stay_behind_the_held_keys` FAIL (0.2s)
- `warp terminal::view::tests::ssh_keys_after_ssh_block_ends_before_remote_session_are_dropped` FAIL (0.1s)
- `warp terminal::view::tests::ssh_keys_between_bootstrapped_and_first_remote_prompt_are_dropped_whole` FAIL (0.1s)
- `warp terminal::view::tests::ssh_line_typed_across_gate_release_is_never_run_locally` FAIL (0.1s)
- `warp terminal::view::tests::ssh_middle_click_paste_with_a_pending_read_keeps_typed_order_around_it` FAIL (0.2s)
- `warp terminal::view::tests::ssh_paste_with_a_pending_read_keeps_typed_order_around_it` FAIL (0.2s)
- `warp terminal::view::tests::ssh_queue_enter_runs_the_held_line_once` FAIL (0.2s)
- `warp terminal::view::tests::ssh_queue_interrupt_discards_the_held_keys` FAIL (0.1s)
- `warp terminal::view::tests::ssh_queue_key_queued_in_the_prompt_turn_lands_after_the_held_keys` FAIL (0.1s)
- `warp terminal::view::tests::ssh_queue_keys_during_connect_are_delivered_once_in_order` FAIL (0.1s)
- `warp terminal::view::tests::table_mode_a_control_key_behind_a_gate_is_delivered_at_the_gates_event` FAIL (0.1s)
- `warp terminal::view::tests::table_mode_a_ctrl_c_in_the_ssh_setup_window_is_held_and_reaches_the_remote_shell_once` FAIL (0.1s)
- `warp terminal::view::tests::table_mode_a_held_signal_reaches_the_remote_shell_once_at_the_handover` FAIL (0.1s)
- `warp terminal::view::tests::table_mode_arrivals_reach_the_same_destinations_as_legacy` FAIL (0.6s)
- `warp terminal::view::tests::table_mode_text_replayed_into_a_new_ssh_connect_is_held_then_delivered_once` FAIL (0.1s)
- `warp terminal::view::tests::text_replayed_into_a_new_ssh_connect_is_held_not_dropped` FAIL (0.1s)
- `warp terminal::view::tests::typing_on_the_ssh_extension_choice_card_shows_a_hint_instead_of_vanishing` FAIL (0.1s)

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
| terminal | latency_enter | FAIL | Enter to command start: Phosphor overhead 6.02 ms over Terminal.app (.command via open) exceeds the 2 ms budget; app share of Enter->start:  |
| terminal | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as |
| terminal | latency_paint | PASS | Key to paint: kl3 measures post-to-first-changed-pixel at 240 Hz in each terminal's own window; reference measured after Phosphor in the sam |
| terminal | shell_fish | PASS |  |
| terminal | shell_nu | FAIL | nu: 12 typed lines: expected ['t220_dfgg8yi2zl', 't221_27cm25yswl', 't222_lb9de9o2u2', 't223_gyd3r03v6g', 't224_z8146jujwi', 't225_on3jgg1d3 |
| terminal | md_viewer | PASS |  |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS |  |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| agent | ssh_connect_typing | PASS |  |
| agent | ssh_enter_then_immediate_command | PASS |  |
| agent | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T111_jblqxi never reached the REMOTE file (s |
| agent | latency_enter | FAIL | Enter to command start: Phosphor overhead 10.00 ms over Terminal.app (.command via open) exceeds the 2 ms budget; app share of Enter->start: |
| agent | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as |
| agent | latency_paint | PASS | Key to paint: kl3 measures post-to-first-changed-pixel at 240 Hz in each terminal's own window; reference measured after Phosphor in the sam |
| agent | shell_fish | PASS |  |
| agent | shell_nu | FAIL | nu: 12 typed lines: expected ['t135_ywsv2p7jdv', 't136_6l85vhb4ny', 't137_zogpvvp34x', 't138_m12z8h5rij', 't139_y0gbel3y6s', 't140_j7gqb3zo8 |
| agent | md_viewer | PASS |  |

## app wayland (app-test-linux-wayland)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | PASS | no reference terminal available: loss is reported but no variant can be attributed to Phosphor or the injector; phosphor / perline-unprimed: |
| terminal | enter_loss_control | PASS | enter control: no reference terminal (unavailable): a lost Enter in Phosphor cannot be attributed; no reference terminal available: Enter lo |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | NOREF | no reference: absolute only (Ctrl-C): no reference terminal available; NO-REFERENCE: Ctrl-C: no reference terminal measured in this session, |
| terminal | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.2 s): T131_kjy2zh never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stop |
| terminal | ssh_enter_then_immediate_command | PASS |  |
| terminal | ssh_nested_typeahead | PASS |  |
| terminal | latency_enter | NOREF | app share of Enter->start: enter_begin -> pty_write median 0.34 ms (from the app's input_order log); the remaining overhead is shell/ConPTY  |
| terminal | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal | shell_fish | PASS |  |
| terminal | shell_nu | FAIL | nu: 12 typed lines: expected ['t166_qxcepqz9sf', 't167_kr0fis9qpn', 't168_r4d6tn8e9u', 't169_s7ic2xcbu0', 't170_9c71lmohi6', 't171_r3mdx3vwo |
| terminal | md_viewer | PASS |  |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS |  |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | NOREF | no reference: absolute only (Ctrl-C): no reference terminal available; NO-REFERENCE: Ctrl-C: no reference terminal measured in this session, |
| agent | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.2 s): T93_r43xyf never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopp |
| agent | ssh_enter_then_immediate_command | PASS |  |
| agent | ssh_nested_typeahead | PASS |  |
| agent | latency_enter | NOREF | app share of Enter->start: enter_begin -> pty_write median 0.78 ms (from the app's input_order log); the remaining overhead is shell/ConPTY  |
| agent | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as |
| agent | left_agent_mode_after_latency_key_log | FAIL | after latency_key_log the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later agent cases me |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| agent | shell_fish | PASS |  |
| agent | shell_nu | FAIL | nu never executed a probe line after start (wait 4.0s + 4 probes); nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its dire |
| agent | md_viewer | PASS |  |

## app windows (app-test-windows)


| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | no OCR engine: screenshot only (shots/terminal_proof_prekey.png), mode not machine-verified |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | cadence | PASS |  |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 En |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times -- PowerShell: Ctrl-C did not stop the sleep (no stamp within 6 s, or one that p |
| terminal | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T145_n3bsgx never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stop |
| terminal | pane_not_local_after_ssh_connect_typing | FAIL | after ssh_connect_typing the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T152_nz8vpg never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; st |
| terminal | pane_not_local_after_ssh_enter_then_immediate_command | FAIL | after ssh_enter_then_immediate_command the pane could not be proven back in the local shell (none); the app is relaunched so later cases sta |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T157_w4d94b never reached the REMOTE file (s |
| terminal | pane_not_local_after_ssh_nested_typeahead | FAIL | after ssh_nested_typeahead the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | latency_enter | FAIL | Enter to command start: Phosphor overhead 87.52 ms over conhost (powershell.exe) exceeds the 2 ms budget; app share of Enter->start: enter_b |
| terminal | latency_key_log | SKIP | not run in the PowerShell pane: types into a running `cat > /dev/null`: no equivalent stdin-echo program in PowerShell: runs in the Git Bash |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal | shell_fish | SKIP | not run in the PowerShell pane: fish as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | shell_nu | SKIP | not run in the PowerShell pane: nu as a subshell of bash: Git Bash pane only (skips itself when not installed) |
| terminal | md_viewer | SKIP | no verified way to hand a file:// URL to the RUNNING Windows instance (a second invocation may start a second app instance on the same data  |
| terminal@gitbash | mode_proof | PASS | no OCR engine: screenshot only (shots/terminal@gitbash_proof_prekey.png), mode not machine-verified |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | cadence | PASS |  |
| terminal@gitbash | key_loss_control | PASS | phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; reference / perline: first char lost on 0/12 lines, other bad 0,  |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; referen |
| terminal@gitbash | fast_typing | FAIL | ft0: file has 19/30 lines, exact match False first diff: exp='t91_8ht6u89s70' got='t92_t2ti62i9kq' [first bad line 4: injector call #660 pos |
| terminal@gitbash | paste_instant_enter | FAIL | pasted lines reached the shell exactly: 5/6 byte-exact (first mismatch: 'count 5 vs 6') |
| terminal@gitbash | pane_needed_ctrl_c_after_paste_instant_enter | PASS | the first marker command after paste_instant_enter had no effect; it worked after Ctrl-C x3 (candidate APP finding: input focus/Enter routin |
| terminal@gitbash | tab_enter | FAIL | Tab+Enter: completed file has 6/12 expected lines |
| terminal@gitbash | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times -- Phosphor: 11/33 sample lines reached the shell NOT exactly as typed; e.g. cha |
| terminal@gitbash | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T172_8qqtac never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stop |
| terminal@gitbash | pane_not_local_after_ssh_connect_typing | FAIL | after ssh_connect_typing the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| terminal@gitbash | key_delivery | FAIL | keys not delivered: vk: marker not in the file; unicode: marker not in the file; foreground=phosphor-oss.exe(pid 18208) app-pid=18208 SendIn |
| agent | mode_proof | PASS | no OCR engine: screenshot only (shots/agent_proof_prekey.png), mode not machine-verified |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times -- PowerShell: Ctrl-C did not stop the sleep (no stamp within 6 s, or one that p |
| agent | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T54_zku2i5 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopp |
| agent | pane_not_local_after_ssh_connect_typing | FAIL | after ssh_connect_typing the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T61_xt16w7 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; sto |
| agent | pane_not_local_after_ssh_enter_then_immediate_command | FAIL | after ssh_enter_then_immediate_command the pane could not be proven back in the local shell (none); the app is relaunched so later cases sta |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | ssh_nested_typeahead | FAIL | ssh_nested_typeahead (outer): before `exit`, a fresh probe ran in no shell, not the remote one: `exit` NOT typed; stopping |
| agent | pane_not_local_after_ssh_nested_typeahead | FAIL | after ssh_nested_typeahead the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| agent@gitbash | mode_proof | PASS | no OCR engine: screenshot only (shots/agent@gitbash_proof_prekey.png), mode not machine-verified |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | fast_typing | FAIL | ft0: file has 5/30 lines, exact match False first diff: exp='t5_lshv616mts' got='t6_6zc4pz0lx9' [first bad line 3: injector call #1233 poste |
| agent@gitbash | paste_instant_enter | FAIL | pasted lines reached the shell exactly: 5/6 byte-exact (first mismatch: 'count 5 vs 6') |
| agent@gitbash | tab_enter | FAIL | Tab+Enter: completed file has 2/12 expected lines |
| agent@gitbash | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times -- Phosphor: 19/33 sample lines reached the shell NOT exactly as typed; e.g. dif |
| agent@gitbash | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T56_y9am72 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopp |
| agent@gitbash | pane_not_local_after_ssh_connect_typing | FAIL | after ssh_connect_typing the pane could not be proven back in the local shell (none); the app is relaunched so later cases start clean |
| agent@gitbash | key_delivery | FAIL | keys not delivered: vk: marker not in the file; unicode: marker not in the file; foreground=phosphor-oss.exe(pid 14596) app-pid=14596 SendIn |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## vs previous run

- NEW failures (unit): 
  - `warp terminal::view::tests::ssh_ctrl_c_ends_the_hold [test-linux]`
  - `warp terminal::view::tests::ssh_ctrl_d_is_held_after_a_prompt_was_answered [test-linux]`
  - `warp terminal::view::tests::ssh_editor_backspace_in_the_gap_edits_the_held_text [test-linux]`
  - `warp terminal::view::tests::ssh_editor_keys_typed_before_delivery_stay_behind_the_held_keys [test-linux]`
  - `warp terminal::view::tests::ssh_keys_after_ssh_block_ends_before_remote_session_are_dropped [test-linux]`
  - `warp terminal::view::tests::ssh_keys_between_bootstrapped_and_first_remote_prompt_are_dropped_whole [test-linux]`
  - `warp terminal::view::tests::ssh_line_typed_across_gate_release_is_never_run_locally [test-linux]`
  - `warp terminal::view::tests::ssh_middle_click_paste_with_a_pending_read_keeps_typed_order_around_it [test-linux]`
  - `warp terminal::view::tests::ssh_paste_with_a_pending_read_keeps_typed_order_around_it [test-linux]`
  - `warp terminal::view::tests::ssh_queue_enter_runs_the_held_line_once [test-linux]`
  - `warp terminal::view::tests::ssh_queue_interrupt_discards_the_held_keys [test-linux]`
  - `warp terminal::view::tests::ssh_queue_key_queued_in_the_prompt_turn_lands_after_the_held_keys [test-linux]`
  - `warp terminal::view::tests::ssh_queue_keys_during_connect_are_delivered_once_in_order [test-linux]`
  - `warp terminal::view::tests::table_mode_a_control_key_behind_a_gate_is_delivered_at_the_gates_event [test-linux]`
  - `warp terminal::view::tests::table_mode_a_ctrl_c_in_the_ssh_setup_window_is_held_and_reaches_the_remote_shell_once [test-linux]`
  - `warp terminal::view::tests::table_mode_arrivals_reach_the_same_destinations_as_legacy [test-linux]`
  - `warp terminal::view::tests::table_mode_text_replayed_into_a_new_ssh_connect_is_held_then_delivered_once [test-linux]`
  - `warp terminal::view::tests::text_replayed_into_a_new_ssh_connect_is_held_not_dropped [test-linux]`
  - `warp terminal::view::tests::typing_on_the_ssh_extension_choice_card_shows_a_hint_instead_of_vanishing [test-linux]`
- FIXED failures (unit): none
- NEW failures (app): 
  - `wayland/terminal/ssh_connect_typing`
  - `wayland/agent/ssh_connect_typing`
  - `wayland/agent/left_agent_mode_after_latency_key_log (case not in previous run)`
  - `windows/terminal/pane_not_local_after_ssh_connect_typing (case not in previous run)`
  - `windows/terminal/pane_not_local_after_ssh_enter_then_immediate_command (case not in previous run)`
  - `windows/terminal/pane_not_local_after_ssh_nested_typeahead (case not in previous run)`
  - `windows/terminal@gitbash/tab_enter`
  - `windows/terminal@gitbash/pane_not_local_after_ssh_connect_typing (case not in previous run)`
  - `windows/terminal@gitbash/key_delivery`
  - `windows/agent/pane_not_local_after_ssh_connect_typing (case not in previous run)`
  - `windows/agent/pane_not_local_after_ssh_enter_then_immediate_command (case not in previous run)`
  - `windows/agent/pane_not_local_after_ssh_nested_typeahead (case not in previous run)`
  - `windows/agent@gitbash/tab_enter`
  - `windows/agent@gitbash/pane_not_local_after_ssh_connect_typing (case not in previous run)`
  - `windows/agent@gitbash/key_delivery`
- FIXED failures (app): 
  - `macos/terminal/ssh_connect_typing`
  - `macos/terminal/ssh_nested_typeahead`
  - `macos/terminal/latency_paint`
  - `macos/terminal/shell_fish`
  - `macos/agent/ssh_connect_typing`
  - `macos/agent/latency_paint`
  - `macos/agent/shell_fish`
  - `wayland/terminal/ssh_nested_typeahead`
  - `wayland/terminal/shell_fish`
  - `wayland/agent/ssh_nested_typeahead`
  - `wayland/agent/shell_fish`
- Latency regressions > 5%: 
  - `windows/terminal/latency_enter enter_to_start: 89.21 -> 94.73 ms (+6.2%)`
- compared against: test-linux: build/r18ct (3019107009); app macos: build/r18ct (3019107009); app wayland: build/r18ct (3019107009); app windows: build/r18ct (3019107009)
