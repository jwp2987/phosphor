# Triage: round 25 targeted run, pipeline 3059029972 (build/r25a 083c9f018)

## Unit suites
Green on Linux and macOS (units first). Windows: test-windows cancelled on purpose (the owner moved Windows to build/r26w, pipeline 3059595258); warpui-core-windows and workspace-rest-windows ran and passed.

## App
- Wayland 17 pass / 2 fail: latency_enter [agent] 4.64 ms (#2184, now judged by data bands, owner decision 49), editor_memory_7mb (#2185: measurement fixed in r27, real cold cost 130-147 MB).
- macOS 19 / 6: link_and_path_click x2 + pane_not_local x2 (TEST: the #2176 in-app code pane took the typed probe lines; #2176 VERIFIED and closed; harness fixed r27), scroll_keys_position [agent] (TEST: one stale line counted as drawn, #2145; #2175 VERIFIED and closed; harness fixed r27), editor_memory_7mb (#2185).

## Lab
- Linux lab 19 / 79 / 81 noref: lab_chips + lab_fam_* no-helper channel counts #2156 (17-55 -> 3-6 per prompt; one batch per prompt in r27), helper takeover rows (#2163/#2164), lab_tab_remote_dir 150 ms (owner question, #2177), lab_path_detect p_exec (#1801), lab_ssh_drop_reconnect nu (kill arm TEST fixed r27; escape_connect_bare APP #2186, fixed r27), lab_ctrl_c_no_esc_i nu.150 lost last token + zsh flake (#2187), zsh recorder rows (#2162). Proven: #2181 and #2180 VERIFIED and closed.
- macOS lab 38 / 53 / 103 noref: same families as Linux (lab_chips 16, lab_fam_git_read 9, lab_fam_run_command 8 -> #2156; lab_tab_remote_dir 6 -> owner question; lab_ssh_drop_reconnect 4 -> #2186 + nu kill TEST; lab_ctrl_c_no_esc_i 3 -> #2187; lab_path_detect -> #1801; lab_helper_reconnect / lab_helper_killed -> #2163/#2164; lab_enter_start -> #2162).

## Bands
The r25a collect asked to loosen 9 bands by 0.01-0.41 ms (listed in the driver log); held back pending the owner.
