# build/r18ct  167012a7b  (pipeline 3019107009)

- date: 2026-11-02, tier: fix, pipeline status: manual
- compile (check-linux): ok, 49 warnings
- unit: 14473 passed, 5 failed, 55 skipped of 14478 run (3 suites)
- app (pass/fail/skip): macos 18/12/0 +2 no-ref; wayland 18/10/2 +2 no-ref; windows 29/30/9 +1 no-ref; soak: n/a

## Jobs

| job | status | minutes | runner | note |
|---|---|---|---|---|
| advisories | success | 0.2 | - |  |
| app-test-linux-wayland | failed | 16.4 | - |  |
| app-test-linux-wayland-reuse | manual | - | - | not run |
| app-test-linux-x11 | manual | - | - | not run |
| app-test-linux-x11-reuse | manual | - | - | not run |
| app-test-macos | failed | 30.1 | MacBookPro-gitrunner |  |
| app-test-macos-reuse | manual | - | - | not run |
| app-test-windows | failed | 50.2 | phosphor-windows-vm |  |
| app-test-windows-reuse | manual | - | - | not run |
| bench | manual | - | - | not run |
| build-linux-profiling | manual | - | - | not run |
| build-linux-x86_64 | success | 31.9 | - |  |
| build-macos-arm64 | success | 20.0 | MacBookPro-gitrunner |  |
| build-macos-profiling | manual | - | - | not run |
| build-windows-profiling | manual | - | - | not run |
| build-windows-x86_64 | success | 37.1 | phosphor-windows-vm |  |
| check-linux | success | 8.6 | - |  |
| check-macos | manual | - | - | not run |
| check-windows | manual | - | - | not run |
| diag-mac | manual | - | - | not run |
| diag-windows | manual | - | - | not run |
| fuzz-smoke | manual | - | - | not run |
| guards | success | 1.5 | - |  |
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
| test-warpui-core | success | 1.2 | - |  |
| test-workspace-rest | failed | 1.6 | - |  |
| usage-test-linux | manual | - | - | not run |
| usage-test-macos | manual | - | - | not run |

## test-linux failures

- `warp terminal::input_queue::facts::table_enum_tests::every_row_of_the_table_is_reachable` FAIL (0.4s)
- `warp terminal::view::tests::ssh_bootstrap_failure_delivers_held_typeahead_after_stty_sane` FAIL (0.2s)
- `warp terminal::view::tests::table_mode_a_held_signal_reaches_the_remote_shell_once_at_the_handover` FAIL (0.1s)

## test-workspace-rest failures

- `warp_editor::mermaid_corpus invalid_diagram_outcomes_are_snapshotted` FAIL (0.2s)
- `warp_editor::mermaid_corpus valid_diagram_summaries_are_stable` FAIL (0.2s)

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
| terminal | ssh_connect_typing | FAIL | harness exception: ValueError("could not convert string to float: ''") |
| terminal | ssh_enter_then_immediate_command | PASS |  |
| terminal | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T193_2cvu0h never reached the REMOTE file (s |
| terminal | latency_enter | FAIL | only 0/30 Enter timings captured |
| terminal | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as |
| terminal | latency_paint | FAIL | Key to paint: Phosphor overhead 5.02 ms over Terminal.app (.command via open) exceeds the 2 ms budget; Key to paint: kl3 measures post-to-fi |
| terminal | shell_fish | FAIL | fish never executed a probe line after start (wait 3.0s + 4 probes) |
| terminal | shell_nu | FAIL | nu never executed a probe line after start (wait 4.0s + 4 probes) |
| terminal | md_viewer | PASS |  |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS |  |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | PASS | Ctrl-C: same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; absolute Phosphor median |
| agent | ssh_connect_typing | FAIL | harness exception: ValueError("could not convert string to float: ''") |
| agent | ssh_enter_then_immediate_command | PASS |  |
| agent | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T109_fayr3r never reached the REMOTE file (s |
| agent | latency_enter | FAIL | only 0/30 Enter timings captured |
| agent | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as |
| agent | latency_paint | FAIL | Key to paint: Phosphor overhead 3.99 ms over Terminal.app (.command via open) exceeds the 2 ms budget; Key to paint: kl3 measures post-to-fi |
| agent | shell_fish | FAIL | fish never executed a probe line after start (wait 3.0s + 4 probes) |
| agent | shell_nu | FAIL | nu never executed a probe line after start (wait 4.0s + 4 probes) |
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
| terminal | ssh_connect_typing | PASS |  |
| terminal | ssh_enter_then_immediate_command | PASS |  |
| terminal | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T145_ay0gbe never reached the REMOTE file (s |
| terminal | latency_enter | FAIL | only 0/30 Enter timings captured; no reference terminal available (need xterm+DISPLAY / foot / Terminal.app) |
| terminal | latency_key_log | FAIL | key->frame p50 30.7 ms > 20 ms CI bound (owner target: next refresh, own work <= 2 ms; software GL here); NO-REFERENCE: key to frame: no ref |
| terminal | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal | shell_fish | FAIL | fish never executed a probe line after start (wait 3.0s + 4 probes) |
| terminal | shell_nu | FAIL | nu never executed a probe line after start (wait 4.0s + 4 probes); nu found at /home/<user>/.local/bin/nu (not on the pane's PATH): its dire |
| terminal | md_viewer | PASS |  |
| agent | mode_proof | PASS |  |
| agent | cadence | PASS |  |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | NOREF | no reference: absolute only (Ctrl-C): no reference terminal available; NO-REFERENCE: Ctrl-C: no reference terminal measured in this session, |
| agent | ssh_connect_typing | PASS |  |
| agent | ssh_enter_then_immediate_command | PASS |  |
| agent | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T109_fayr3r never reached the REMOTE file (s |
| agent | latency_enter | FAIL | only 0/30 Enter timings captured; no reference terminal available (need xterm+DISPLAY / foot / Terminal.app) |
| agent | latency_key_log | FAIL | key->frame p50 28.4 ms > 20 ms CI bound (owner target: next refresh, own work <= 2 ms; software GL here); NO-REFERENCE: key to frame: no ref |
| agent | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| agent | shell_fish | FAIL | fish never executed a probe line after start (wait 3.0s + 4 probes) |
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
| terminal | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-C: same trap/timestamp command typed in both terminals; reference measured |
| terminal | pane_wedged_after_ctrl_c_d | FAIL | the powershell pane did not run a marker command (no effect file) after ctrl_c_d, twice: the app is relaunched so the remaining cases measur |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T146_igh1pn never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stop |
| terminal | pane_wedged_after_ssh_connect_typing | FAIL | the powershell pane did not run a marker command (no effect file) after ssh_connect_typing, twice: the app is relaunched so the remaining ca |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T151_c5nz8v never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; st |
| terminal | pane_wedged_after_ssh_enter_then_immediate_command | FAIL | the powershell pane did not run a marker command (no effect file) after ssh_enter_then_immediate_command, twice: the app is relaunched so th |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T156_zrw4d9 never reached the REMOTE file (s |
| terminal | pane_wedged_after_ssh_nested_typeahead | FAIL | the powershell pane did not run a marker command (no effect file) after ssh_nested_typeahead, twice: the app is relaunched so the remaining  |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | latency_enter | FAIL | Enter to command start: Phosphor overhead 82.39 ms over conhost (powershell.exe) exceeds the 2 ms budget; app share of Enter->start: enter_b |
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
| terminal@gitbash | fast_typing | FAIL | ft25: file has 29/30 lines, exact match False first diff: exp='t134_xvjqqqwyr3' got='t135_jiqome8m81' [first bad line 17: injector call #753 |
| terminal@gitbash | paste_instant_enter | FAIL | pasted lines reached the shell exactly: 5/6 byte-exact (first mismatch: 'count 5 vs 6') |
| terminal@gitbash | tab_enter | PASS |  |
| terminal@gitbash | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times -- Phosphor: 6/33 sample lines reached the shell NOT exactly as typed; e.g. one  |
| terminal@gitbash | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T168_emlmt1 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stop |
| terminal@gitbash | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T170_ve0m65 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; st |
| terminal@gitbash | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T172_tz8qqt never reached the REMOTE file (s |
| terminal@gitbash | latency_enter | FAIL | Enter to command start: Phosphor overhead 25.01 ms over conhost (bash) exceeds the 2 ms budget; app share of Enter->start: enter_begin -> pt |
| terminal@gitbash | latency_key_log | NOREF | NO-REFERENCE: key to frame: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as |
| terminal@gitbash | latency_paint | SKIP | kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare |
| terminal@gitbash | shell_fish | SKIP | package fish (apt install fish) not installed (looked on PATH and in ~/.local/bin, /home/<user>/.local/bin, /opt/homebrew/bin, /usr/local/bi |
| terminal@gitbash | shell_nu | SKIP | package nushell (not in Ubuntu repos: github.com/nushell/nushell release tarball, or `cargo install nu --locked`; Mac: brew install nushell) |
| terminal@gitbash | md_viewer | SKIP | no verified way to hand a file:// URL to the RUNNING Windows instance (a second invocation may start a second app instance on the same data  |
| agent | mode_proof | PASS | no OCR engine: screenshot only (shots/agent_proof_prekey.png), mode not machine-verified |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | PASS |  |
| agent | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; fix tier, agent mode (winmap.FIX_TIER): Ctrl-C delivery and line integrity only |
| agent | pane_wedged_after_ctrl_c_d | FAIL | the powershell pane did not run a marker command (no effect file) after ctrl_c_d, twice: the app is relaunched so the remaining cases measur |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T55_srpy9a never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopp |
| agent | pane_wedged_after_ssh_connect_typing | FAIL | the powershell pane did not run a marker command (no effect file) after ssh_connect_typing, twice: the app is relaunched so the remaining ca |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T60_e6xt16 never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; sto |
| agent | pane_wedged_after_ssh_enter_then_immediate_command | FAIL | the powershell pane did not run a marker command (no effect file) after ssh_enter_then_immediate_command, twice: the app is relaunched so th |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T65_1zve5p never reached the REMOTE file (ss |
| agent | pane_wedged_after_ssh_nested_typeahead | FAIL | the powershell pane did not run a marker command (no effect file) after ssh_nested_typeahead, twice: the app is relaunched so the remaining  |
| agent@gitbash | mode_proof | PASS | no OCR engine: screenshot only (shots/agent@gitbash_proof_prekey.png), mode not machine-verified |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | fast_typing | FAIL | ft0: file has 28/30 lines, exact match False first diff: exp='t11_5w0wa88v3b' got='t12_l9lf9qcefb' [first bad line 9: injector call #1509 po |
| agent@gitbash | paste_instant_enter | FAIL | pasted lines reached the shell exactly: 5/6 byte-exact (first mismatch: 'count 5 vs 6') |
| agent@gitbash | tab_enter | PASS |  |
| agent@gitbash | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times -- Phosphor: 6/33 sample lines reached the shell NOT exactly as typed; e.g. char |
| agent@gitbash | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T53_fif2py never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopp |
| agent@gitbash | ssh_enter_then_immediate_command | FAIL | ssh_enter_then_immediate_command: T55_h180hs never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; sto |
| agent@gitbash | ssh_nested_typeahead | FAIL | the line typed behind a nested ssh ran exactly once remotely: []; ssh_nested_typeahead (inner): T57_bbpqnl never reached the REMOTE file (ss |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## vs previous run

- NEW failures (unit): 
  - `warp terminal::input_queue::facts::table_enum_tests::every_row_of_the_table_is_reachable [test-linux]`
  - `warp terminal::view::tests::ssh_bootstrap_failure_delivers_held_typeahead_after_stty_sane [test-linux]`
- FIXED failures (unit): 
  - `markdown_parser::corpus corpus_snapshots_gfm_tables [test-workspace-rest]`
  - `markdown_parser::corpus corpus_snapshots_plain [test-workspace-rest]`
  - `warp_editor::mermaid_corpus hostile_sources_never_panic [test-workspace-rest]`
- NEW failures (app): 
  - `macos/terminal/latency_paint (case not in previous run)`
  - `macos/terminal/shell_fish (case not in previous run)`
  - `macos/terminal/shell_nu (case not in previous run)`
  - `macos/agent/ssh_connect_typing (case not in previous run)`
  - `macos/agent/ssh_nested_typeahead (case not in previous run)`
  - `macos/agent/latency_enter (case not in previous run)`
  - `macos/agent/latency_paint (case not in previous run)`
  - `macos/agent/shell_fish (case not in previous run)`
  - `macos/agent/shell_nu (case not in previous run)`
  - `wayland/terminal/ssh_nested_typeahead (case not in previous run)`
  - `wayland/terminal/latency_enter`
  - `wayland/terminal/shell_fish`
  - `wayland/agent/ssh_nested_typeahead (case not in previous run)`
  - `wayland/agent/latency_enter`
  - `wayland/agent/shell_fish`
  - `windows/terminal/ctrl_c_d (case not in previous run)`
  - `windows/terminal/pane_wedged_after_ctrl_c_d (case not in previous run)`
  - `windows/terminal/ssh_connect_typing (case not in previous run)`
  - `windows/terminal/pane_wedged_after_ssh_connect_typing (case not in previous run)`
  - `windows/terminal/ssh_enter_then_immediate_command (case not in previous run)`
  - `windows/terminal/pane_wedged_after_ssh_enter_then_immediate_command (case not in previous run)`
  - `windows/terminal/ssh_nested_typeahead (case not in previous run)`
  - `windows/terminal/pane_wedged_after_ssh_nested_typeahead (case not in previous run)`
  - `windows/terminal/latency_enter (case not in previous run)`
  - `windows/terminal@gitbash/fast_typing (case not in previous run)`
  - `windows/terminal@gitbash/paste_instant_enter (case not in previous run)`
  - `windows/terminal@gitbash/ctrl_c_d (case not in previous run)`
  - `windows/terminal@gitbash/ssh_connect_typing (case not in previous run)`
  - `windows/terminal@gitbash/ssh_enter_then_immediate_command (case not in previous run)`
  - `windows/terminal@gitbash/ssh_nested_typeahead (case not in previous run)`
  - `windows/terminal@gitbash/latency_enter (case not in previous run)`
  - `windows/agent/ctrl_c_d (case not in previous run)`
  - `windows/agent/pane_wedged_after_ctrl_c_d (case not in previous run)`
  - `windows/agent/ssh_connect_typing (case not in previous run)`
  - `windows/agent/pane_wedged_after_ssh_connect_typing (case not in previous run)`
  - `windows/agent/ssh_enter_then_immediate_command (case not in previous run)`
  - `windows/agent/pane_wedged_after_ssh_enter_then_immediate_command (case not in previous run)`
  - `windows/agent/ssh_nested_typeahead (case not in previous run)`
  - `windows/agent/pane_wedged_after_ssh_nested_typeahead (case not in previous run)`
  - `windows/agent@gitbash/fast_typing (case not in previous run)`
  - ... and 5 more
- FIXED failures (app): 
  - `macos/terminal/ssh_enter_then_immediate_command`
- Latency regressions > 5%: none
- compared against: test-linux: build/r18cq (3018918070); test-warpui-core: build/r18cq (3018918070); test-workspace-rest: build/r18cq (3018918070); app macos: build/r18cl (3018713104); app wayland: build/r18cf (3018057763); app windows: build/r18cm (3018717340)
