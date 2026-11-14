# Triage: round 30 targeted run f, pipeline 3068353168 (build/r30f 8476d176d)

The run proving the #2209 change set (with #2115, #2223), #2226, #2220, #2213 on Windows, #2216 chips on Windows. Issue-tied cases only.

## Units
All nine suites green on Linux, macOS and Windows (12346 / 12062 tests). Windows: commit_chain_delivers_a_large_message_over_stdin_whole PASS (#2226 closed). The new #2209 / #2115 / #2223 unit and view tests passed by name on the box gate.

## App
- Linux Wayland and macOS: expert_fuzz, fast_typing, enter_loss_control, key_loss_control, mode_proof PASS in both modes (16/16): no key lost, no merged line under #2209.
- Windows (PowerShell, Git Bash): routing_table_powershell PASS in terminal and agent (#2220 closed, #2059 closed); expert_fuzz [terminal@gitbash] and [agent@gitbash] PASS (#2115); gitbash_typeahead_reps PASS in both modes (#2203/#2207/#2208); enter_loss_control, key_delivery, mode_proof PASS.
- selection_modes [terminal@gitbash] FAIL: the 4 typed lines all reached the shell (#2223 declined-Enter writes carried only the typed-since text, 44 of 69 chars); the row failed on (a) #2228 TEST: the still-running stream command, recorded late, counted as a typed line (fixed dcd6f078f) and (b) #2117: the second Ctrl-C reached the pty but did not end the msys sh loop (diagnostics merged dcd6f078f). [agent@gitbash] selection_modes PASS.

## Lab
- Linux: every row PASS in both modes (lab_connect_typeahead.bash.*, lab_ctrl_c_no_esc_i.zsh.lan.helper).
- macOS: terminal mode all PASS; agent mode lab_connect_typeahead.zsh.lan.{nohelper,helper} FAIL: the ssh typeahead released raw to the pty at an editor-owned prompt (row D6 on stale non-blocking facts) and the line start was wiped. #2209 follow-up a811316f4 (the delivered typeahead takes the line) and #2227 (exact facts at delivery, owner OK) fix it; both merged on integ/r15.
- Windows (first run of these families against the real lab): every lab_connect_typeahead.* and lab_chips.bash.lan.nohelper row FAIL in both modes:
  - 8 rows: the remote got the previous trial's `exit` before the typed line: a bare `exit` was decided by the model and sent to the agent (#2200, reopened by the owner, fixed b80672cbf).
  - 5 rows: the Git Bash ssh session was not managed (no ManagedSSHSessionStarting, no one-connection stages): investigation in progress (#1933 / #2051 family), verdict and fix before the next run.
  - 2 chips rows: no side call, `exit` did not return: same unmanaged-session cause.

## Bands
Held for the owner (WOULD LOOSEN from collect, not applied): the same ten keys as r30d/r30e (macos latency_first_key_after_idle gap1s/gap5s/gap2s, ctrl_c_d ctrl_c_flood; windows latency_enter x2, latency_key_log, pwsh7 ctrl_c_d, gitbash editor_memory_7mb rs private (#2224), subshell_ready_rc_questions).
