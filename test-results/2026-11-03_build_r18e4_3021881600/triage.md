# Triage: r18e4 (pipeline 3021881600, commit e253f6c14; release tier)

This build predates #1604, #1609, the #1605/#1608 test fixes, the echo B fix (#1603, 7c9f6a492) and the #1610 harness change.
Its remaining box jobs (Linux app tests, mutants, loom) were cancelled on purpose: they would have re-tested superseded code.
The release tier was re-run as r18e8 (6188e6fc1).

## Unit failures
- terminal::view::tests::group1_handover_tests::nested_prompt_during_the_handover_drain_delivers_the_line_to_the_inner_shell #1603 (lost echo B; fix merged 7c9f6a492)
- terminal::view::tests::group3_paste_tests::editor_paste_behind_an_enter_gate_follows_the_line_into_the_ssh_hold #1605
- terminal::view::tests::ssh_in_the_agent_view_on_an_unready_shell_is_routed_to_the_shell_and_held_not_sent_to_the_agent #1608 (test fix merged 357ef2e25)

## Other jobs
- fuzz-smoke: mermaid_render non-deterministic layout for a flowchart with a cycle #1613 (dependency mermaid-to-svg/dagre_rust).
- app-test-windows: harness crash (cp1252 UnicodeEncodeError in save_case_log) after the PowerShell pane; Git Bash pane and agent mode not run. Fixed in 6188e6fc1.
- app-test-macos (validated against screenshots and logs; no false passes found):
  - APP: read_x, subshells, alt_screen: the young-Enter hold swallows the Enter after `read x` / inside an inner shell / after less/vim (`young_hold_cap` gives up, nothing written) #1614; Enter latency and ctrl_c_flood overhead #1606.
  - HARNESS: ui_palette (OCR caret, stale 'No results found'), ZWJ (zsh fc rec.log, fixed 0dd072e73), floods too short (fixed 0dd072e73), routing_table '5 > 3' (fixed 0dd072e73), ui_tab_menu (pre-#1610, fixed ddfef40fe), shell_zsh + remote_session_* 'APP DIED' = the case's own `exit` closing the pane (no panic in any log), ssh_hanging_host_not_local refused-host arm (fixed 0dd072e73), left_agent_mode_after_* false alarms, ui_startup_time accounting, 12 leftover ssh ControlMaster clients, 437 Enters never reaching NSApplication after 14:05 (cases in that window not clean).
  - UNDETERMINED: ssh_password_prompt wrong-passphrase half (log truncated), ssh_connect_typing, agent_stream_cancel, latency_paint (agent), md_viewer_big, editor_memory_7mb (247 MB RSS for 7 MB; #1607).
- app-test-windows (validated): APP: ssh_connect_typing, ssh_enter_then_immediate_command, ssh_nested_typeahead lose the Enter (young_hold_cap on an ssh-owned tty) #1609, fixed after this build in e15abb3a9; HARNESS: fast_typing (reference PowerShell shares PSReadLine history), the cp1252 crash; UNDETERMINED: latency_enter 40 ms over conhost, mostly in PowerShell/ConPTY #1606. Cleanup and all 10 restore steps verified.
