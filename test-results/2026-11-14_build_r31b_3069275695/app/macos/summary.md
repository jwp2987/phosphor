# App test results (platform=macos, backend=mac, app_display=macos, seed=1, commit=6936b7cf, sha=6936b7cf2d8706e6acf95d65c1bb30e783f18c3b, owner_settings=sha256 81d188e9844d, tier=targeted, scale=1, display_scale=1, outputs=1)

pass 12, fail 1, skip 0, no-reference 0 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 1 taken by a key binding, 0 Enter by a binding; macOS: 1 key-down(s) reached NSApplication, 1 dispatched as key events, 0 not (0 Cmd chord(s) injected) |
| terminal | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype call, the #1414 artifact); phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; reference / perline: first char lost on 0/12 lines, other bad 0, missing 0; injector  |
| terminal | enter_loss_control | PASS | reference Terminal.app (.command via open) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference Terminal.app (.command via open) gap 0.25s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; injector  |
| terminal | fast_typing | PASS | injector accounting: injector sent 2058; app saw 2058; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1996 unbound keydown/typed, 1 taken by a key binding, 60 Enter by a binding; macOS: 2058 key-down(s) reached NSApplication, 2058 dispatched as key events (1 of them a menu key equivalent), 0 not (0 Cmd chord(s) injected); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| terminal | expert_fuzz | PASS | injector accounting: injector sent 710; app saw 710; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 647 unbound keydown/typed, 37 taken by a key binding, 25 Enter by a binding; macOS: 710 key-down(s) reached NSApplication, 710 dispatched as key events (1 of them a menu key equivalent), 0 not (0 Cmd chord(s) injected); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 25 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| terminal | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@41 ms, InitShell@83 ms, Bootstrapped@667 ms; the first inner line ran 0.00 s after its Enter (its history band replaces the fixed 2 s budget): band 0.76-1.3 from n=8 run(s) (median 1.11); 0 is below the ba |
| terminal | scroll_keys_position | PASS | injector accounting: injector sent 194; app saw 194; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 180 unbound keydown/typed, 6 taken by a key binding, 3 Enter by a binding; macOS: 194 key-down(s) reached NSApplication, 194 dispatched as key events (5 of them a menu key equivalent), 0 not (2 Cmd chord(s) injected); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 3 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent | mode_proof | PASS |  |
| agent | fast_typing | PASS | injector accounting: injector sent 2004; app saw 2004; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1942 unbound keydown/typed, 1 taken by a key binding, 60 Enter by a binding; macOS: 2004 key-down(s) reached NSApplication, 2004 dispatched as key events (1 of them a menu key equivalent), 0 not (0 Cmd chord(s) injected); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| agent | expert_fuzz | PASS | injector accounting: injector sent 749; app saw 749; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 663 unbound keydown/typed, 60 taken by a key binding, 25 Enter by a binding; macOS: 749 key-down(s) reached NSApplication, 749 dispatched as key events (1 of them a menu key equivalent), 0 not (0 Cmd chord(s) injected); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 25 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| agent | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@41 ms, InitShell@83 ms, Bootstrapped@665 ms; the first inner line ran 0.00 s after its Enter (its history band replaces the fixed 2 s budget): band 0.75-1.32 from n=8 run(s) (median 1.09); 0 is below the b |
| agent | scroll_keys_position | PASS | injector accounting: injector sent 192; app saw 192; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 178 unbound keydown/typed, 7 taken by a key binding, 3 Enter by a binding; macOS: 192 key-down(s) reached NSApplication, 192 dispatched as key events (4 of them a menu key equivalent), 0 not (1 Cmd chord(s) injected); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 3 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent | agent_bg_job_output | FAIL | the job's line is on screen 2 times: it was repeated into another block (shots/*abj_2_next.png); injector accounting: injector sent 68; app saw 68; lost-before-app 0 (Enter: sent 2, app saw 2); of the app's: 59 unbound keydown/typed, 3 taken by a key bindin |

## Key-loss control (#1414): lines whose first character never arrived / lines typed

| mode | target | injection | lost-first | other-bad | missing | n |
|---|---|---|---|---|---|---|
| terminal | phosphor | perline | 0 | 0 | 0 | 12 |
| terminal | reference | perline | 0 | 0 | 0 | 12 |

## Enter-loss control (#1482): reference-terminal lines missing after a fast burst with instant Enter

| mode | reference | gap s | missing | n |
|---|---|---|---|---|
| terminal | Terminal.app (.command via open) | 0.0 | 0 | 30 |
| terminal | Terminal.app (.command via open) | 0.25 | 0 | 30 |

## PROVING cases (owner decides graduation)

Platform macos. Nothing here moves a case: a passed case graduates only when the owner says so (docs/testing.md, 'New tests run in the fix tier first').

| case | mode | status | detail |
|---|---|---|---|
| agent_bg_job_output | agent | FAIL | the job's line is on screen 2 times: it was repeated into another block (shots/*abj_2_next.png); injector accounting: injector sent 68; app saw 68; lost-before- |
| scroll_keys_position | terminal | PASS | injector accounting: injector sent 194; app saw 194; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 180 unbound keydown/typed, 6 taken by a key bin |
| scroll_keys_position | agent | PASS | injector accounting: injector sent 192; app saw 192; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 178 unbound keydown/typed, 7 taken by a key bin |
| subshell_ready_rc_clear | terminal | PASS | clear: setup armed decision=Write(None); InitSubshell@41 ms, InitShell@83 ms, Bootstrapped@667 ms; the first inner line ran 0.00 s after its Enter (its history  |
| subshell_ready_rc_clear | agent | PASS | clear: setup armed decision=Write(None); InitSubshell@41 ms, InitShell@83 ms, Bootstrapped@665 ms; the first inner line ran 0.00 s after its Enter (its history  |

passed in every pass it ran in on this platform (candidates for the owner's graduation decision, 2): scroll_keys_position, subshell_ready_rc_clear
not passed (fail / skip / no-reference): agent_bg_job_output

## Changed since previous build

no previous build results (not in CI, first run, or none fetched): case status NOT compared
