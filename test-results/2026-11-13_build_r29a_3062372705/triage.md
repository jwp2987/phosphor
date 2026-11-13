# Triage: round 29 targeted run, pipeline 3062372705 (build/r29a eede509b1)

The one targeted run of the input-path change set (Fable review -> Opus refute -> one change set: A1-A6, #2182 in-band wait, #2193 paste order).

## Units
All 9 suites green (50108 passed, 0 failed).

## App
- Wayland 26 pass / 2 fail: routing_table 'what is 5 > 3' went to the agent by the model rule (APP #2200, flaky); the close-trim UI pause (APP #2201, fixed after r29a in 2f886bcf9). Neither touches the input-path change.
- macOS 29 / 5: the link-click close-key harness gap (TEST, fixed in fix/r29-harness 4f9ab8655); the idle first-key band skewed by a stuck ReportCrash process on the Mac (ENVIRONMENT; restart needs the owner's OK).
- Linux lab 45 / 56: no regression. #2202 ssh escape race (APP, older); branch-chip exact OCR, lab_chips prompt count, helper-takeover line (TEST, fixed in fix/r29-labharness2 9d4d76a9e). #2156 and #2195 verified and closed.
- macOS lab 17 / 64: 32 Tab->insert speed rows (TEST: fixed number, now a band in labharness2); the rest the same harness faults as Linux lab.
- Windows 211 / 26: now PASS: ssh_gitbash_managed, ssh_gitbash_failing_not_local [terminal@gitbash], bootstrap_in_command, nested_typeahead, remote_tab_enter_no_helper [agent@gitbash].
  - New since r26w (passed in r24a and r26w), triaged from the full job artifacts:
    - gitbash_typeahead_reps [agent@gitbash]: APP #2203. A deferred young Enter got no verdict after the hand-off report rewrote its line ("xt" -> the full line); the 2 s backstop handed the text back, the hand-back guard ate the next Enter, and two lines went out joined. 1 of 24 reps; terminal@gitbash 24/24. Regression vs r26w not proven (r26w logs not kept).
    - history_survives_restart [terminal], [agent]: TEST #2204 (seeded idents counted over the persistent PowerShell history file; the shell ran each line once).
    - routing_table [terminal@gitbash], [agent@gitbash]: TEST #2205 (rec.log recorder dark for ~15 min; every line was written and preexec seen; 'went to agent' rows read the pre-decision input mode). The bare `date` routed by the model rule is APP #2200.
    - ssh_gitbash_failing_not_local_managed [agent@gitbash] and its pane_switch row: #2206 (pane left with precmds_seen=0 after ssh_gitbash_managed; app vs harness not yet separated).
  - Older (also failed in r24a/r26w), verdicts unchanged by r29a: ssh_gitbash_typeahead_managed, ssh_connect_typing_wrapper, remote_tab_no_helper / remote_tab_enter_no_helper [terminal@gitbash] (fake sshd, #2007/#2051); remote_session_legacy (also inside the #2205 window); mode_proof [terminal], [terminal@gitbash], [terminal@pwsh7] (#2170); ui_completions_as_you_type_local; scroll_keys_position [agent@gitbash] (#2145); cb_agent_pickers [agent@gitbash] (flaky, inside the #2205 window); latency_key_log Git Bash (thin provisional band, n=3).

## Bands
Auto-widened (within the 0.5 ms / 0.3 s limit): wayland/agent@fake/agent_latency_key/idle 0.31->0.35; windows/agent@gitbash/latency_enter#app 1.75->2.09; windows/terminal@gitbash/latency_key_log 4.35->4.55; rcq_inner_line_after_enter_s gitbash 4.22->4.42 (agent), 4.26->4.55 (terminal).
Held for the owner (WOULD LOOSEN): macos/agent/latency_first_key_after_idle gap1s -1.26->1.98 and gap5s -9.63->-8.14 (Mac ReportCrash at 93% CPU during the run); windows/agent/latency_enter 35.1->38.6; windows/agent@gitbash/latency_enter 50.36->72.24; windows/agent@gitbash/latency_key_log 4.45->5.41; windows/agent@pwsh7/ctrl_c 1.85->2.47.

## Size
Four sshd.log.gz files (macOS and Wayland, agent and terminal) were dropped to fit the 10 MB limit; they are in the GitLab job artifacts.
