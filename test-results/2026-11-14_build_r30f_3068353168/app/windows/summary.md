# App test results (platform=windows, backend=windows, app_display=windows, seed=1, commit=8476d176, sha=8476d176d4144966c556c88f45e3613f142f5a75, owner_settings=sha256 81d188e9844d, tier=targeted, scale=1)

pass 18, fail 1, skip 8, no-reference 0 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS | injector accounting: injector sent 1 keys (0 Enter); app key trace not available, so nothing to compare |
| terminal | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal | enter_loss_control | PASS | reference conhost (powershell.exe) gap 0.00s: 0/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 Enters; reference conhost (powershell.exe) gap 0.25s: 1/30 lines missing (merged/lost Enter), None lines not received exactly, injector posted 30 Enters; the injector/O |
| terminal | expert_fuzz | SKIP | not run in the PowerShell pane: BackSpace / Left / Ctrl-W editing of `echo ... >>` lines; PSReadLine's Ctrl-W differs: runs in the Git Bash pane |
| terminal | routing_table_powershell | PASS | 'make a PR' (ambiguous): no verdict, went to agent: no routing line was logged, so there is no rule to check; injector accounting: injector sent 1319; app saw 1319; lost-before-app 0 (Enter: sent 35, app saw 35); of the app's: 1206 unbound keydown/typed, 78 taken by a k |
| terminal | selection_modes | SKIP | not run in the PowerShell pane: T31 (r14 T22): `cat` of the fixture files, the `sh` stream program and rec.log: runs in the Git Bash pane (pointer and held Shift by SendInput, positions from the app's find hook) |
| terminal | gitbash_typeahead_reps | SKIP | not run in the PowerShell pane: bash precmd_functions and rec.log of the pane's bash, the ConPTY prompt hold of a bash pane (#1393): runs in the Git Bash pane |
| terminal@gitbash | mode_proof | PASS | injector accounting: injector sent 1 keys (0 Enter); app key trace not available, so nothing to compare |
| terminal@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| terminal@gitbash | enter_loss_control | PASS | reference conhost (bash) gap 0.00s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; reference conhost (bash) gap 0.25s: 0/30 lines missing (merged/lost Enter), 0 lines not received exactly, injector posted 30 Enters; injector accounting: inject |
| terminal@gitbash | expert_fuzz | PASS | injector accounting: injector sent 718; app saw 718; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 651 unbound keydown/typed, 42 taken by a key binding, 25 Enter by a binding; pty writes: 87, of which carrying a CR (\r): 0; Enter(s) the input editor started: 25 (an Enter may ride in a multi-line write; a gap with held/handed-back Ente |
| terminal@gitbash | routing_table_powershell | SKIP | not run in the Git Bash pane: #2072: the routing table judged from PSReadLine's history file; the Git Bash pane has routing_table (rec.log) |
| terminal@gitbash | selection_modes | FAIL | Ctrl-C did not end the streaming program: no prompt within 8 s (the SECOND Ctrl-C: the first copied the selection; if this one copied too, the copy did not drop the selection, #2117); `sh sm_stream_t63_q6s0r.sh` did not reach the shell as typed (received []); after_selection: shell received 5 commands, not the typed ones (first mismatch: 'sh  |
| terminal@gitbash | gitbash_typeahead_reps | PASS | injector accounting: injector sent 8450; app saw 8450; lost-before-app 0 (Enter: sent 242, app saw 242); of the app's: 8217 unbound keydown/typed, 2 taken by a key binding, 231 Enter by a binding; pty writes: 1554, of which carrying a CR (\r): 11; Enter(s) the input editor started: 240 (an Enter may ride in a multi-line write; a gap with held/handed-back  |
| agent | mode_proof | PASS |  |
| agent | expert_fuzz | SKIP | not run in the PowerShell pane: BackSpace / Left / Ctrl-W editing of `echo ... >>` lines; PSReadLine's Ctrl-W differs: runs in the Git Bash pane |
| agent | key_delivery | PASS | marker typed and received via SendInput vk |
| agent | routing_table_powershell | PASS | 'gti status' reached the shell by ran-before (#1203), not by the #1980 rule (is the word a command on this machine?); 'dokcer ps' reached the shell by ran-before (#1203), not by the #1980 rule (is the word a command on this machine?); 'gerp -rn foo foo.txt' reached the shell by |
| agent | selection_modes | SKIP | not run in the PowerShell pane: T31 (r14 T22): `cat` of the fixture files, the `sh` stream program and rec.log: runs in the Git Bash pane (pointer and held Shift by SendInput, positions from the app's find hook) |
| agent | gitbash_typeahead_reps | SKIP | not run in the PowerShell pane: bash precmd_functions and rec.log of the pane's bash, the ConPTY prompt hold of a bash pane (#1393): runs in the Git Bash pane |
| agent@gitbash | mode_proof | PASS |  |
| agent@gitbash | key_delivery | PASS | marker typed and received via SendInput vk |
| agent@gitbash | expert_fuzz | PASS | injector accounting: injector sent 750; app saw 750; lost-before-app 0 (Enter: sent 25, app saw 25); of the app's: 678 unbound keydown/typed, 47 taken by a key binding, 25 Enter by a binding; pty writes: 105, of which carrying a CR (\r): 0; Enter(s) the input editor started: 25 (an Enter may ride in a multi-line write; a gap with held/handed-back Ent |
| agent@gitbash | routing_table_powershell | SKIP | not run in the Git Bash pane: #2072: the routing table judged from PSReadLine's history file; the Git Bash pane has routing_table (rec.log) |
| agent@gitbash | selection_modes | PASS | Windows, agent view: Ctrl-C at an idle prompt with a word selected did not copy (the agent view takes an idle Ctrl-C first); 'the copy drops the selection' (#2131) was NOT judged in this pass; injector accounting: injector sent 284; app saw 284; lost-before-app 0 (Enter: sent 8, app saw 8); of the app's: 259 unbound keydown/typed, 17 taken by a key bi |
| agent@gitbash | gitbash_typeahead_reps | PASS | the precmd set-up line typed before tr12 was not routed to the shell: deferred=false buffer="__tr_slow() { sleep 0.25; }; precmd_functions+=(__tr_slow)" input_type=AI t=1794671544776.637; tr13: 1 Enter(s) of this rep were not routed to the shell: deferred=false buffer=".txt" input_type=AI t=1794671559253.847; tr15: 1 Enter(s) of this rep were not |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## Enter-loss control (#1482): reference-terminal lines missing after a fast burst with instant Enter

| mode | reference | gap s | missing | n |
|---|---|---|---|---|
| terminal | conhost (powershell.exe) | 0.0 | 0 | 30 |
| terminal | conhost (powershell.exe) | 0.25 | 1 | 30 |
| terminal@gitbash | conhost (bash) | 0.0 | 0 | 30 |
| terminal@gitbash | conhost (bash) | 0.25 | 0 | 30 |

## PROVING cases (owner decides graduation)

Platform windows. Nothing here moves a case: a passed case graduates only when the owner says so (docs/testing.md, 'New tests run in the fix tier first').

| case | mode | status | detail |
|---|---|---|---|
| gitbash_typeahead_reps | terminal | SKIP | not run in the PowerShell pane: bash precmd_functions and rec.log of the pane's bash, the ConPTY prompt hold of a bash pane (#1393): runs in the Git Bash pane |
| gitbash_typeahead_reps | terminal@gitbash | PASS | injector accounting: injector sent 8450; app saw 8450; lost-before-app 0 (Enter: sent 242, app saw 242); of the app's: 8217 unbound keydown/typed, 2 taken by a  |
| gitbash_typeahead_reps | agent | SKIP | not run in the PowerShell pane: bash precmd_functions and rec.log of the pane's bash, the ConPTY prompt hold of a bash pane (#1393): runs in the Git Bash pane |
| gitbash_typeahead_reps | agent@gitbash | PASS | the precmd set-up line typed before tr12 was not routed to the shell: deferred=false buffer="__tr_slow() { sleep 0.25; }; precmd_functions+=(__tr_slow)" input_t |
| routing_table_powershell | terminal | PASS | 'make a PR' (ambiguous): no verdict, went to agent: no routing line was logged, so there is no rule to check; injector accounting: injector sent 1319; app saw 1 |
| routing_table_powershell | terminal@gitbash | SKIP | not run in the Git Bash pane: #2072: the routing table judged from PSReadLine's history file; the Git Bash pane has routing_table (rec.log) |
| routing_table_powershell | agent | PASS | 'gti status' reached the shell by ran-before (#1203), not by the #1980 rule (is the word a command on this machine?); 'dokcer ps' reached the shell by ran-befor |
| routing_table_powershell | agent@gitbash | SKIP | not run in the Git Bash pane: #2072: the routing table judged from PSReadLine's history file; the Git Bash pane has routing_table (rec.log) |
| selection_modes | terminal | SKIP | not run in the PowerShell pane: T31 (r14 T22): `cat` of the fixture files, the `sh` stream program and rec.log: runs in the Git Bash pane (pointer and held Shif |
| selection_modes | terminal@gitbash | FAIL | Ctrl-C did not end the streaming program: no prompt within 8 s (the SECOND Ctrl-C: the first copied the selection; if this one copied too, the copy did not drop |
| selection_modes | agent | SKIP | not run in the PowerShell pane: T31 (r14 T22): `cat` of the fixture files, the `sh` stream program and rec.log: runs in the Git Bash pane (pointer and held Shif |
| selection_modes | agent@gitbash | PASS | Windows, agent view: Ctrl-C at an idle prompt with a word selected did not copy (the agent view takes an idle Ctrl-C first); 'the copy drops the selection' (#21 |

passed in every pass it ran in on this platform (candidates for the owner's graduation decision, 0): none
not passed (fail / skip / no-reference): gitbash_typeahead_reps, routing_table_powershell, selection_modes

## Changed since previous build

no previous build results (not in CI, first run, or none fetched): case status NOT compared
