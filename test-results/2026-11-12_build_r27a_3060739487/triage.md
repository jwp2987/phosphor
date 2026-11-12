# Triage: round 27 targeted run, pipeline 3060739487 (build/r27a aedd52495)

## Units
Linux and macOS green (3/3 each). Windows: test-windows 1/11961 FAIL test_slow_clipboard_read_does_not_block_and_keys_follow_the_paste (#2193: no key lost; the held Backspace applies one update after the paste-hold flag clears; fixed in r28) -> Windows build and app SKIPPED (units first). warpui-core-windows and workspace-rest-windows green.

## App
- Wayland 28 pass / 2 fail: ssh_drop_reconnect [escape_after_cleared_line] (TEST: the session's own 'ssh fake' line counted; fixed e78b7c86b), left_agent_mode_after_latency_enter_ssh (TEST: unanchored view_mode regex matched a diff body the app logged; anchored in r27 loganchors; the diff body in the log is APP #2191, fixed). Proven: #2107 history_up_recall PASS both modes (6000 seeded lines); latency rows judged by bands.
- macOS 25 / 5: link_and_path_click [terminal] + tab_pointer_use [terminal] (ENVIRONMENT: a stuck macOS Tips notification covered the screen, the pointer injector refused every click; Tips disabled with the owner's OK, #2189), link_and_path_click [agent] + pane_not_local (TEST: focus-lost hops counted, the in-app code pane was not closed; fixed in r27 loganchors), subshell_ready_rc_clear [agent] 1.32 s outside its band (APP #2190: nested shell ~25% slower since r25). Proven: #2107 PASS both modes.
- Labs: NOT RUN here — my cases file named four lab families by their bare names; the lab refused the selection. Re-run as build/r27b (pipeline 3061436969).

## Bands
The collect would have widened 9 bands; 7 were a formula artifact (every value inside the band, a few far-apart points inflate median + 3 MAD; e.g. macOS ssh Enter IMPROVED to -4.97 ms yet asked 5.17 -> 14.71). perf_bands now never widens a band whose window values are all inside it. 2 real widenings held for the owner: windows/agent/latency_enter 35.1 -> 39.54 (n=2), windows/agent@gitbash/latency_enter#app 1.75 -> 2.37 (n=2). Also found: latency_enter_ssh's record name did not match its band (rows read 'no band yet'); renamed, and a self-test now checks every record name against bands.json.
