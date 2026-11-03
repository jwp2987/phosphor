# App test results (platform=windows, backend=windows, app_display=windows, seed=1, commit=86aa856a, owner_settings=None, tier=smoke, scale=1)

pass 1, fail 4, skip 0, no-reference 0 (not passes)

| mode | case | status | detail |
|---|---|---|---|
| terminal | harness_crash | FAIL | the terminal pass crashed outside a case: FileNotFoundError(2, 'No such file or directory') |
| terminal@gitbash | harness_crash | FAIL | the terminal@gitbash pass crashed outside a case: FileNotFoundError(2, 'No such file or directory') |
| agent | harness_crash | FAIL | the agent pass crashed outside a case: FileNotFoundError(2, 'No such file or directory') |
| agent@gitbash | harness_crash | FAIL | the agent@gitbash pass crashed outside a case: FileNotFoundError(2, 'No such file or directory') |
| - | restore | PASS | all 10 restore steps ok (data md5 identical, version v0.1.15, PATH/handler/uninstall/shortcuts restored) |

## Changed since previous build

no previous build results (not in CI, first run, or none fetched): case status NOT compared
