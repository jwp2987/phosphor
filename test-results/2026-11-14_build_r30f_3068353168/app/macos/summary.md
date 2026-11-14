# App test results (platform=macos, backend=mac, app_display=macos, seed=1, commit=8476d176, sha=8476d176d4144966c556c88f45e3613f142f5a75, owner_settings=sha256 81d188e9844d, tier=targeted, scale=1)

pass 8, fail 0, skip 0, no-reference 0 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1; app saw 1; lost-before-app 0 (Enter: sent 0, app saw 0); of the app's: 0 unbound keydown/typed, 1 taken by a key binding, 0 Enter by a binding; macOS: 1 key-down(s) reached NSApplication, 1 dispatched as key events, 0 not (0 Cmd chord(s) injected) |
| terminal | key_loss_control | PASS | injector mode: None (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype call, the #1414 artifact); phosphor / perline: first char lost on 0/12 lines, other bad 0, missing 0; reference / perline: first char lost on 0/12 lines, other bad 0, missing 0; injector  |
| terminal | enter_loss_control | PASS | reference Terminal.app (.command via open) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference Terminal.app (.command via open) gap 0.25s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; injector  |
| terminal | fast_typing | PASS | injector accounting: injector sent 2058; app saw 2058; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1996 unbound keydown/typed, 1 taken by a key binding, 60 Enter by a binding; macOS: 2058 key-down(s) reached NSApplication, 2058 dispatched as key events (1 of them a menu key equivalent), 0 not (0 Cmd chord(s) injected); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| terminal | expert_fuzz | PASS | injector accounting: injector sent 710; app saw 710; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 647 unbound keydown/typed, 37 taken by a key binding, 25 Enter by a binding; macOS: 710 key-down(s) reached NSApplication, 710 dispatched as key events (1 of them a menu key equivalent), 0 not (0 Cmd chord(s) injected); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 25 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| agent | mode_proof | PASS |  |
| agent | fast_typing | PASS | injector accounting: injector sent 2004; app saw 2004; lost-before-app 0 (Enter: sent 60, app saw 60); of the app's: 1942 unbound keydown/typed, 1 taken by a key binding, 60 Enter by a binding; macOS: 2004 key-down(s) reached NSApplication, 2004 dispatched as key events (1 of them a menu key equivalent), 0 not (0 Cmd chord(s) injected); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 60 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |
| agent | expert_fuzz | PASS | injector accounting: injector sent 749; app saw 749; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 663 unbound keydown/typed, 60 taken by a key binding, 25 Enter by a binding; macOS: 749 key-down(s) reached NSApplication, 749 dispatched as key events (1 of them a menu key equivalent), 0 not (0 Cmd chord(s) injected); pty writes: 0, of which carrying a CR (\r): 0; Enter(s) the input editor started: 25 (an Enter may ride in a multi-line write; a gap with held/handed-back Enter |

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

## Changed since previous build

no previous build results (not in CI, first run, or none fetched): case status NOT compared
