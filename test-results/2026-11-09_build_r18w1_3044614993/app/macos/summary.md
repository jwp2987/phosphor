# App test results (platform=macos, backend=mac, app_display=macos, seed=1, commit=c0ed28ad, sha=c0ed28adfee2f4fa90600db06cb30c1f363d078a, owner_settings=sha256 a0613b641a27, tier=smoke, scale=1)

pass 0, fail 18, skip 0, no-reference 0 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| terminal | cadence | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| terminal | key_loss_control | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| terminal | enter_loss_control | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| terminal | fast_typing | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| terminal | paste_instant_enter | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| terminal | tab_enter | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| terminal | ctrl_c_d | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| terminal | ssh_connect_typing | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| terminal | harness_crash | FAIL | the terminal pass crashed outside a case: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| agent | mode_proof | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| agent | cadence | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| agent | fast_typing | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| agent | paste_instant_enter | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| agent | tab_enter | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| agent | ctrl_c_d | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| agent | ssh_connect_typing | FAIL | harness exception: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |
| agent | harness_crash | FAIL | the agent pass crashed outside a case: RuntimeError('loginwindow is frontmost (locked screen?): refusing to send keys') |

## Changed since previous build

no previous build results (not in CI, first run, or none fetched): case status NOT compared
