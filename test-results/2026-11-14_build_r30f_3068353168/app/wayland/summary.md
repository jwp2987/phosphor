# App test results (platform=linux-wayland, backend=wayland, app_display=wayland, seed=1, commit=8476d176, sha=8476d176d4144966c556c88f45e3613f142f5a75, owner_settings=sha256 81d188e9844d, tier=targeted, scale=1)

pass 8, fail 0, skip 0, no-reference 0 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 1 taken by a key binding, 0 Enter by a binding |
| terminal | key_loss_control | PASS | injector mode: persistent (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype call, the #1414 artifact); phosphor / perline-unprimed: first char lost on 0/12 lines, other bad 0, missing 0; phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; p |
| terminal | enter_loss_control | PASS | reference foot gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference foot gap 0.25s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; injector accounting: injector sent 2; |
| terminal | fast_typing | PASS | injector accounting: injector sent 2072; app saw 2072; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 2010 unbound keydown/typed, 2 taken by a key binding, 60 Enter by a binding; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| terminal | expert_fuzz | PASS | injector accounting: injector sent 721; app saw 721; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 662 unbound keydown/typed, 34 taken by a key binding, 25 Enter by a binding; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 25 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| agent | mode_proof | PASS |  |
| agent | fast_typing | PASS | injector accounting: injector sent 2004; app saw 2004; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1942 unbound keydown/typed, 2 taken by a key binding, 60 Enter by a binding; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| agent | expert_fuzz | PASS | injector accounting: injector sent 749; app saw 749; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 663 unbound keydown/typed, 61 taken by a key binding, 25 Enter by a binding; pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 25 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |

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

## Changed since previous build

no previous build results (not in CI, first run, or none fetched): case status NOT compared
