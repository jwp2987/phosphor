# App test results (platform=linux-wayland, backend=wayland, app_display=wayland, seed=1, commit=6936b7cf, sha=6936b7cf2d8706e6acf95d65c1bb30e783f18c3b, owner_settings=sha256 81d188e9844d, tier=targeted, scale=1, display_scale=1, outputs=1)

pass 14, fail 2, skip 0, no-reference 0 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 1 taken by a key binding, 0 Enter by a binding |
| terminal | key_loss_control | PASS | injector mode: persistent (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype call, the #1414 artifact); phosphor / perline-unprimed: first char lost on 0/12 lines, other bad 0, missing 0; phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; p |
| terminal | enter_loss_control | PASS | reference foot gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference foot gap 0.25s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; injector accounting: injector sent 2; |
| terminal | fast_typing | PASS | injector accounting: injector sent 2072; app saw 2072; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2010 unbound keydown/typed, 2 taken by a key binding, 60 Enter by a binding; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| terminal | expert_fuzz | PASS | injector accounting: injector sent 721; app saw 721; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 662 unbound keydown/typed, 34 taken by a key binding, 25 Enter by a binding; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 25 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| terminal | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@17 ms, InitShell@46 ms, Bootstrapped@261 ms; the first inner line ran 0.00 s after its Enter (its history band replaces the fixed 2 s budget): band 0.5-0.95 from n=8 run(s) (median 0.73); 0 is below the ba |
| terminal | ssh_drop_reconnect | PASS | injector accounting: injector sent 2313; app saw 2313; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2249 unbound keydown/typed, 4 taken by a key binding, 60 Enter by a binding; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| terminal | scroll_keys_position | FAIL | after one typed key none of the job's last lines is on screen: the key did not bring the view back to the bottom (the job's last 8 lines were looked for; shots/*skp_5_after_key.png); injector accounting: injector sent 194; app saw 194; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 180 unbound keydown/typed, 11 taken by a key bi |
| terminal | restore_shows_history | PASS | injector accounting: injector sent 420; app saw 420; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 208 unbound keydown/typed, 208 taken by a key binding, 4 Enter by a binding |
| agent | mode_proof | PASS |  |
| agent | fast_typing | PASS | injector accounting: injector sent 2004; app saw 2004; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1942 unbound keydown/typed, 2 taken by a key binding, 60 Enter by a binding; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| agent | expert_fuzz | PASS | injector accounting: injector sent 749; app saw 749; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 663 unbound keydown/typed, 61 taken by a key binding, 25 Enter by a binding; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 25 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| agent | subshell_ready_rc_clear | PASS | clear: setup armed decision=Write(None); InitSubshell@15 ms, InitShell@41 ms, Bootstrapped@268 ms; the first inner line ran 0.00 s after its Enter (its history band replaces the fixed 2 s budget): band 0.47-0.95 from n=8 run(s) (median 0.7); 0 is below the ba |
| agent | scroll_keys_position | FAIL | after one typed key none of the job's last lines is on screen: the key did not bring the view back to the bottom (the job's last 8 lines were looked for; shots/*skp_5_after_key.png); injector accounting: injector sent 191; app saw 191; lost-before-app 0 (Enter: sent 3, app saw 3); of the app's: 178 unbound keydown/typed, 10 taken by a key bi |
| agent | agent_bg_job_output | PASS | injector accounting: injector sent 67; app saw 67; lost-before-app 0 (Enter: sent 2, app saw 2); of the app's: 59 unbound keydown/typed, 6 taken by a key binding, 2 Enter by a binding; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 2 (an Enter may ride in a multi-line write; a gap with held/handed-back Enters |
| agent | restore_shows_history | PASS | injector accounting: injector sent 607; app saw 607; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 260 unbound keydown/typed, 340 taken by a key binding, 7 Enter by a binding |

## Key-loss control (#1414): lines whose first character never arrived / lines typed

| mode | target | injection | lost-first | other-bad | missing | n |
|---|---|---|---|---|---|---|
| terminal | phosphor | perline-unprimed | 0 | 0 | 0 | 12 |
| terminal | phosphor | perline | 0 | 0 | 0 | 12 |
| terminal | phosphor | oneshot | 0 | 0 | 0 | 12 |
| terminal | reference | perline-unprimed | 0 | 0 | 0 | 12 |
| terminal | reference | perline | 0 | 0 | 0 | 12 |
| terminal | reference | oneshot | 0 | 0 | 0 | 12 |

## Enter-loss control (#1482): reference-terminal lines missing after a fast burst with instant Enter

| mode | reference | gap s | missing | n |
|---|---|---|---|---|
| terminal | foot | 0.0 | 0 | 30 |
| terminal | foot | 0.25 | 0 | 30 |

## PROVING cases (owner decides graduation)

Platform linux-wayland. Nothing here moves a case: a passed case graduates only when the owner says so (docs/testing.md, 'New tests run in the fix tier first').

| case | mode | status | detail |
|---|---|---|---|
| agent_bg_job_output | agent | PASS | injector accounting: injector sent 67; app saw 67; lost-before-app 0 (Enter: sent 2, app saw 2); of the app's: 59 unbound keydown/typed, 6 taken by a key bindin |
| restore_shows_history | terminal | PASS | injector accounting: injector sent 420; app saw 420; lost-before-app 0 (Enter: sent 4, app saw 4); of the app's: 208 unbound keydown/typed, 208 taken by a key b |
| restore_shows_history | agent | PASS | injector accounting: injector sent 607; app saw 607; lost-before-app 0 (Enter: sent 7, app saw 7); of the app's: 260 unbound keydown/typed, 340 taken by a key b |
| scroll_keys_position | terminal | FAIL | after one typed key none of the job's last lines is on screen: the key did not bring the view back to the bottom (the job's last 8 lines were looked for; shots/ |
| scroll_keys_position | agent | FAIL | after one typed key none of the job's last lines is on screen: the key did not bring the view back to the bottom (the job's last 8 lines were looked for; shots/ |
| subshell_ready_rc_clear | terminal | PASS | clear: setup armed decision=Write(None); InitSubshell@17 ms, InitShell@46 ms, Bootstrapped@261 ms; the first inner line ran 0.00 s after its Enter (its history  |
| subshell_ready_rc_clear | agent | PASS | clear: setup armed decision=Write(None); InitSubshell@15 ms, InitShell@41 ms, Bootstrapped@268 ms; the first inner line ran 0.00 s after its Enter (its history  |

passed in every pass it ran in on this platform (candidates for the owner's graduation decision, 3): agent_bg_job_output, restore_shows_history, subshell_ready_rc_clear
not passed (fail / skip / no-reference): scroll_keys_position

## Changed since previous build

no previous build results (not in CI, first run, or none fetched): case status NOT compared
