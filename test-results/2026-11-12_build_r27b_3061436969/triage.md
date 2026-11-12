# Triage: round 27 labs-only run, pipeline 3061436969 (build/r27b e78b7c86b)

Units green on Linux and macOS (units first). Labs re-run here because r27a's lab jobs refused a case list with bare family names (my error).

## Linux lab 14 pass / 62 fail / 92 noref; macOS lab 13 / 44 / 107
- lab_chips + lab_fam_run_command / git_read / fs_basic no-helper (Linux 45, macOS 33 rows): #2156 still 4-5 execs per prompt (was 17-55 in r24a): the cd's navigate_tree ran before the Precmd bumped the generation, and the old root's tree refresh ran while the navigation was in flight — fixed in r28 (fix/r28-batchgen2).
- lab_tab_remote_dir zsh.150.nohelper (2 rows each lab; was 18 in r25a): #2188 — zsh's in-band Tab answer is always empty (#2196) and arrived after the Enter at 150 ms; r28 hands the held Enter to the app completer (fix/r28-remotetab2).
- lab_ssh_drop_reconnect nu: PASS (#2186 VERIFIED, closed).
- lab_helper_killed (2 each): #2195 — the helper proxy outlived the daemon; fixed in r28 (fix/r28-helpertakeover). macOS lan 'executor never swapped in 30 s' still to confirm.
- lab_helper_reconnect zsh (2 each), lab_enter_start zsh.150.helper: #2162 zsh recorder (the app ran the line once; rec.log recorded nothing).
- lab_ctrl_c_no_esc_i: #2187 — nu x2/x3 bursts TEST (fixed), zsh.150.helper x1 typed key killed by an unproven-editor commit APP (fixed in r28), zsh lan/150 helper 'received []' #2162.
- lab_path_detect p_exec + pane_not_local: #1801.
