# Round 16: the settings a test runs with (r16-settings)

Owner 2026-11-11: "you never defined the tests' settings so they don't know what they are testing." Harness and docs only; no app code.
Worked, unproven when written: the self-tests pass, nothing was run against an app.

## Cause, with evidence

- A pass's `settings.toml` is the machine-local owner seed with harness overrides on top (`script/realapp/appci/profile.py` `write_settings`,
  `OWNER_SEED`), and cases rewrite it further. Nothing in the repo said so in one place. The header of `profile.py` still said the profile "never
  reads the owner's real config" and that "vertical tabs are not set"; `docs/testing.md` 3.1 describes hand runs on a copy of the owner's config.
- The seed is the same file on all three test machines (one sha256, equal to the `owner_settings` meta of the round 13 results of Linux Wayland,
  macOS and Windows). Its values stay out of the repo; the per-machine list is in the job-directory report.
- Cases handled it each in their own way. Some read a setting and judge by it (`routing_table`, `tab_pointer_use`, `osc52_write_read`), some set
  both values (`primary_and_middle_click`, `precmd_secret_honor_ps1`), some skipped on a profile they could not run on
  (`ui_completions_as_you_type_local`, `context_menu_actions`), and most typed at the prompt assuming vim keys off, default key bindings, the app's
  own prompt and the as-you-type helpers, with no check that it was so.
- Two texts were wrong about their own settings: `zero_network_startup` ("default settings": it launches on the pass profile, which names a model
  provider when the machine configures one) and the not-judged message of `ui_vertical_tabs_panel_click` (round 13 Linux, agent pass: "the panel's view mode / primary info in this
  profile may show neither the command nor the directory": what the panel shows first is a setting, and the case had never looked at it).

## Chosen design

1. `case_settings.py` (pure): the named settings (TOML key, the app's default read from the Rust definition, what the harness forces), the baseline
   of a pass, and one declaration per fix-tier case: `needs` / `sets` / `reads`, or `independent` with a reason.
2. `cases.requires(ctx, r, **settings)`: reads the settings file the RUNNING app uses (launch value plus live rewrites), compares, and raises
   `SettingsMismatch` (a `Skip`). `run.py` turns it into status `noref` with `UNVERIFIABLE: SETTINGS: <name>=<actual>, case needs <expected>`:
   never a pass, never a failure for that reason; a check that failed earlier stands.
3. `run.py` calls `cases.requires_declared` (baseline + the case's `needs`) before the case's first key, and notes
   `SETTINGS LEFT CHANGED ... in: <key names>` on a row whose case ended with another settings file than it started on.
4. `docs/testing.md` 3.1b: the three layers, the rule, the two generated tables (`case_settings.py --write`). Self-tests keep them equal to the
   code, keep the `harness` column equal to what `profile.py` writes, and fail for a fix-tier case with no declaration.

## Rejected

- **A decorator argument on every `@case`.** 180 edits across 40 files written by other shards in the same round, and the table could not be read
  or generated in one place. The central table is applied by `run.py` with the same function a case can call.
- **The baseline for every registered case.** The full-tier-only cases and the lab families were not audited; a wrong requirement turns a
  working case into UNVERIFIABLE on every machine. Only declared cases are checked; the doc says what is not covered.
- **Putting a leaked settings file back from `run.py`.** On macOS and Windows the app reads a copy, and the app writes the file itself (a model
  picked in the UI). A note with the key names is safe; a rewrite under a running app is not provable by reading.
- **Recording the effective values in `results.json`.** Results are committed; that would copy the seed into the repo. Only a mismatch message
  names one value, of the setting the case needs.
- **Making the Ctrl-A / Ctrl-E / Ctrl-Y expectation of `meta_and_word_keys` strict** (app default only, since no keybindings file is seeded).
  Which of the two is right is an open owner question (`DECLINED.md`, round 15 entry); the case keeps accepting both and now declares that it
  runs on the default bindings.

## Risk

- A wrong `needs` value makes a case UNVERIFIABLE instead of judged. Every declared value was evaluated against a profile built from a
  seed-shaped file and against a bare profile, in both modes: no case mismatches. That proves the declarations do not misfire on the current
  seed; it does not prove each one is complete.
- The declarations come from reading the cases, keyword search first and the code of the doubtful ones after. A dependency no keyword showed is
  not declared.
- The reader is textual (`toml_text.flat`): it reads `[table]` keys, dotted keys and inline tables. A settings file in another form reads as
  "key absent", which is the app default for most keys and a mismatch for the session mode (the pass would then be UNVERIFIABLE, not wrong).
