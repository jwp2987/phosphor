# Round 16: every case declares its settings; the release tier's Mac / Windows lab (r16-declare)

Owner decisions 2026-11-11. Builds on `docs/design/r10-r16-settings.md` (`case_settings.py`, `cases.requires`, `docs/testing.md` 3.1b).
Harness, CI driver and docs only; no app code. Worked, unproven when written: the self-tests pass, nothing was run against an app.

## 1. Every registered case declares its settings

### State before, with evidence

- `case_settings.py` declared the 180 cases of `SMOKE` + `PROVING`. The registry (`cases.CASES`) holds 867: 59 more app cases (full tier only) and
  628 lab arms of 48 families (`cases_lab.lab_case`, one registered case per arm). `cases.requires_declared` returned `None` for all 687, so `run.py`
  checked nothing before their first key (`cases.py` `requires_declared`: "the full-tier-only cases have not been audited").
- The lab arms differ from every other case in one way: the harness itself rewrites two settings per arm. `cases_lab.lab_prepare` writes
  `warpify.ssh.ssh_extension_install_mode` (`HELPER_ARM_MODE`: `never_install` for the `nohelper` / `helper` arms, `always_install` for `upload` /
  `upgrade` / `never`) and `warpify.ssh.remote_helper_denied_hosts`, and relaunches the app, whenever the arm's helper mode is not the one the app was
  last launched with (`ctx.lab_helper_mode`). It does not put them back: the next arm of another mode writes its own. `lab_helper_policy` also writes
  `remote_helper_allowed_hosts`. `run.py` checks the declaration BEFORE the case function, so before `lab_prepare`: at that moment the file holds the
  PREVIOUS arm's install mode.

### Chosen design

1. **Exact declarations for the 59 app cases**, the same `decl(...)` / `independent(...)` lines as the fix tier, from reading each case (below).
2. **Family declarations for the lab**: `family("lab_tab_remote_dir.*", needs=..., sets=..., reads=..., note=...)`. A glob, because the arms of a
   family share one body and so one oracle; 628 lines that say the same thing 48 times would not be read. `case_settings.declaration(name)` returns
   the exact declaration, else the one family whose glob matches; `cases.requires_declared` uses it. The docs table has one row per family.
3. **The lab's own writes are `sets`, never `needs`**: no lab family needs a value of `ssh_extension_install` or `helper_hosts` (a need would be
   judged against the previous arm's value and make the first arm of every helper mode UNVERIFIABLE). The table says the arm writes them and that
   they are NOT put back between arms.
4. **The self-test covers the registry**: every registered case has a declaration (exact, or exactly one family glob); every family glob matches a
   registered case; no declaration names a case that is not registered; the docs table is the generated one. It replaces
   `r16_every_fix_tier_case_declares_its_settings` (same checks, wider set) under the name `r16_every_case_declares_its_settings`.
5. The docs table gets a `tier` column (fix / full / lab) and lists fix tier first, as before.

### How each declaration was reached, and where I am unsure

Method: for each of the 59 app cases and each of the 48 lab bodies, the docstring and every line that sends a key, types, pastes, clicks, reads the
screen, touches the settings file or needs a model were read (a keyword extraction over the function's source); the whole body was read where that
left a doubt (the lab's `lab_prepare`, the agent drivers `ask` / `drive_plan`, `quit_app`, `relaunch`, the editor cases, the upgrade cases, the
privilege case, the flood and startup cases). I did not read all 107 bodies line by line. A dependency no keyword showed is not declared.

The bundles are the fix tier's (`INPUT`, `KEYS`, `PROMPT`, `SSH`, `AGENT_ASK`, `MODEL`); every value in them is the app's default, so a bare
profile and the current seed both satisfy them.

Decisions worth a second look:

- **Lab arms: `INPUT` + managed ssh + the app's prompt** (`ssh_warpification`, `legacy_ssh_wrapper` true, `honor_ps1` false), as the fix tier's ssh
  cases, minus the install mode. `completions_while_typing` is documented as a local-session default; it is in `INPUT` for the fix tier's ssh cases
  and I kept the lab the same rather than invent a second bundle. Unsure whether an explicit `true` there changes a remote session; it cannot
  misfire on the current seed (the key is absent).
- **`lab_agent_nld_predict_ssh` needs `nld_predict_in_ssh` false; `lab_agent_suggestions_ssh` needs `next_command_in_ssh` and
  `prompt_suggestions_in_ssh` false** (two new named settings, both default false in `settings/ai.rs`). Both cases say "default settings" and fail
  on any request from the ssh pane; with the switch on, a request is the designed behaviour, so the case must not be judged. This declares what the
  case was written against. It does not decide #1982.
- **The agent's file-read permission (`agent_file_read`) is NOT needed by the lab's agent families.** The seed sets a non-default value. `ask` and
  `drive_plan` approve a card "if one shows" and judge by the routing line, the host's file and the tool results the model received, so both values
  work. Declared in the note, not as a need. `lab_agent_remote_drop` does need the command card (`AGENT_ASK`): it fails with "no approval card"
  otherwise.
- **`ui_right_click_paste` needs `right_click = context_menu`** (as `context_menu_actions`): with right click = paste there is no menu to find.
- **`intl_deadkey_option`, `intl_altgr_ps`, `intl_altgr_bash` need `extra_meta_keys` at the app default**: with Option / Alt acting as Meta the dead
  key or AltGr character is not composed. Read from what the setting does, not from a run; unsure for AltGr (right Alt) on Windows.
- **`quit_24_tabs` does not need `quit_warning`**: `quit_app` answers the question when it comes and exits either way.
- **Settings-independent**: `upgrade_from_v0_1_17` (replaces the profile's settings and keybindings with the checked-in fixture and puts them back),
  `cb_no_autoupdate` (reads the app log of the whole run). `idle_cpu`, `ui_startup_time`, `cb_state_relaunch` run on the pass profile and are
  declared with the baseline only: what they measure depends on the whole profile, and no single setting is their oracle.
- **`cb_settings_migration`** replaces the whole file twice (legacy, truncated) and puts it back: no named setting describes that; the note does.
- **`ui_settings_live`** switches `system_theme` off before writing `theme` (new named setting), `font_size`, `vertical_tabs`; restores the file.
- **`upgrade_over_previous`**: the previous release is launched on the pass's profile, so its settings are the pass's; declared like a typing case.

### Rejected

- **One `decl` line per lab arm** (generated from the registry): `case_settings.py` is pure (no import of `cases`), and the table would be 628 rows.
- **Needing `ssh_extension_install` per helper arm**: judged before `lab_prepare`, it would be the previous arm's value (see above).
- **A check after `lab_prepare`** (`cases.requires` inside the runner for the arm's install mode): `set_ext_mode` has just written the value it would
  read back; it proves nothing new. Not added.
- **Leaving the lab undeclared but excluded from the self-test**: the task is that every case says what it runs on.

### Risk

- A wrong `needs` makes a case UNVERIFIABLE instead of judged, on every machine. All 867 declarations were evaluated against a bare profile and a
  seed-shaped profile in both modes (the self-test does the bare one for every case and mode): none mismatches. That shows they do not misfire on
  today's profile, not that each is complete.
- The first lab arm of a pass is checked against the profile as built (`never_install`), later arms against whatever the previous arm left; since
  no lab family needs those two settings this cannot make an arm UNVERIFIABLE.
- `run.py` notes `SETTINGS LEFT CHANGED` on the first arm of each helper mode (it always did for any case; lab rows will now be read with the
  table that says why).

## 2. The release tier's Mac and Windows lab jobs play `PROVING_LAB` only

`ci_seq.LAB_PLAT_PROVING_ONLY_TIERS` was `("candidate",)`; the constant's own comment said "add \"release\" here to change that". It is now
`("candidate", "release")`. `lab_tier_vars(MAC | WIN, "release")` passes `APPCI_CASES` = the `PROVING_LAB` globs, the quick matrix and the scripted
model, exactly as in the candidate tier. The Linux lab job is unchanged in both tiers (every family, `LAB_CANDIDATE_MATRIX`). The jobs' own
`APPCI_CASES: "lab_*"` in `.gitlab-ci.yml` is unchanged, so a hand play still plays every family.

The self-test row that asserted the old behaviour ("the release tier ... keep the job's own lab_*") is replaced by one that asserts the new one:
the expectation changed because the owner's decision changed the behaviour, not to make a test pass. A second row keeps the part that still holds
(Linux keeps `lab_*`; a tier the constant does not name passes no `APPCI_CASES`).

Consequence to know: no tier now plays the quick matrix of every lab family on macOS or Windows (about 340 rows a platform, never played there).

## 3. Not touched

The owner's seed files and the test machines. No machine was read or changed for this work; the seed's values quoted in the job-directory report
of r16-settings were used to check that no new declaration misfires.
