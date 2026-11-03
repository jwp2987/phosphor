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
- app-test-macos: per-case validation in progress (Sonnet validator against screenshots and logs). Known so far: ui_tab_menu is checking the pre-#1610 expectation; Enter latency and ctrl_c_flood overhead #1606; shell_zsh row reports APP DIED (to be confirmed).
