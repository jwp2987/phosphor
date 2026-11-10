# Round 12: does the Mac test ssh? (#2041, #2042)

Round 11, macOS smoke tier, build c61b6a3b (`appci-out/macos/run`): pass 14, fail 1, skip 44, no-reference 99.
`ssh_connect_typing` is PASS; `ssh_enter_then_immediate_command`, `ssh_nested_typeahead`, `remote_tab_no_helper` and
`remote_tab_enter_no_helper` are NOREF in both modes. Nothing was built or run for this note; the Mac runner was looked at read-only.

## Short answer

- **Yes, the Mac's fix tier runs ssh, and in round 11 it worked every time it was tried**: 12/12, 3/3 and 1/1 typed-ahead lines exact
  per mode, both remote Tab cases with every check passing. The four NOREF rows are a TEST fault in the injector accounting, the same
  fault that makes 97 of the run's 99 NOREF rows. It is not ssh, not the trial count and not a reference terminal.
- **What it runs against is narrow**: the Mac's own OpenSSH sshd on loopback, remote shell macOS bash 3.2, no helper, no latency.
  The lab (bash/zsh/fish/nu on Linux, 150 ms, helper) has never been played from the Mac.

## 1. Why the four rows are NOREF

All four carry one sentence, written by `injacct.loss_verdict` and turned into the row's status by `run.account_case`
(`r.unverifiable`, then `run.py:510` `status = "noref"`):

    UNVERIFIABLE: injector accounting: 2 key-down(s) reached the app (NSApplication sendEvent, hop=os_key_event) and were never
    dispatched as a key event (247 reached it, 245 dispatched; only the 0 Cmd chord(s) the injector posted may be taken by a menu ...

Ruled out, with the row's own lines:

| candidate | evidence |
|---|---|
| no reference terminal | these cases have none and need none; `lat_checks` is not called by them |
| n too small / "this n cannot exclude 1/40" | that sentence is a note of `ssh_connect_typing` only (`loss_report`), the one row that is PASS; no case's status depends on it |
| within noise | no latency verdict in these rows |
| a premise of the case | `ssh trial verdicts: exact=3` / `exact=1`; every `r.check` passed (the row has no failure line) |
| keys lost before the app | `injector sent 247; app saw 247; lost-before-app 0 (Enter: sent 12, app saw 12)` |

What the two key-downs are (`run_slim/logs/terminal/case_ssh_nested_typeahead.log`, lines 2-4 and 3260-3262; the same pair opens and
closes every case log):

    input_order hop=os_key_event age_ms=1.299 enter=false ctrl=true t=...099.217
    input_order hop=ctrl_c_keydown powershell_setup_hold=Off t=...105.434
    input_order hop=ctrl_c_route route=none t=...105.462

They are the harness's own Ctrl-C, typed by `Ctx.reset()` before and after each case. The app acted on both. The Ctrl-U typed next to
them leaves `mac_ctrl stage=key_down_impl_entry` and `keydown_keybound`; the Ctrl-C leaves neither, because on macOS it never reaches
the view's key dispatch:

- `CustomAction::ClearEditor` has the default keystroke `ctrl-c` (`app/src/util/bindings.rs:299`) and is a main-menu item
  (`app/src/app_menus.rs:252`), enabled while the input editor is in front (`custom_action_updater`, `app_menus.rs:1124`).
- `-[WarpWindow performKeyEquivalent:]` skips `keyDownImpl` for a keystroke that triggers a custom action
  (`objc/window.m:566-575`), AppKit's menu takes it, and `itemWasTriggered:` (`objc/menus.m`) runs
  `dispatch_custom_action` (`warpui_core/src/core/app.rs:2349`).
- That path writes no `keydown`, no `keydown_keybound`, no `core_enter` line: the three the accounting counts as "dispatched".

`docs/design/r10-injector.md` assumed Ctrl-C leaves `keydown_keybound` on every platform (true on Linux and Windows) and allowed a
menu only the Cmd chords the injector posted. Over the run's 113 per-case logs, 450 of the 497 key-downs with no dispatch line are
Control chords and 47 are Cmd chords, already allowed. 186 of the Control ones are followed directly by `ctrl_c_keydown`; the
other 264 show no hop of their own in the log, and the next run's `keydown_menu` lines will say for each whether the menu took it.

`ssh_connect_typing` has the same two key-downs (`976 reached, 974 dispatched, 2 not`) and is PASS only because
`injacct.LOSS_CASES` exempts the five loss cases from this verdict.

**Verdict: TEST.** Since the rule arrived in round 10 no macOS row outside the five loss cases can be PASS.

`agent_interactive_ssh` (agent@fake) is NOREF for a second, separate reason that Linux shares: `ai_ssh: no hand-over (take-over
notice only, #1811)`. Not touched here.

## 2. Fix

A PASS is honestly reachable: the keys were dispatched, the trace only lacked a line for them.

- **App, trace only.** `itemWasTriggered:` logs `input_order hop=keydown_menu ctrl=<bool> cmd=<bool>` when the event being handled
  is a key-down (`[NSApp currentEvent]`), before it runs the item (`warp_note_menu_key_event` in `mac/menus.rs`,
  `input_order::log_menu_key_event`). Flags only, never the key; a clicked item logs nothing; no behaviour changes.
- **Harness.** `injacct.app_trace` counts those lines; `join` adds them to `dispatched`. The Cmd-chord allowance becomes
  `max(cmd chords injected - Cmd menu lines, 0)`: a chord the app named is counted once, as dispatched, and frees no allowance for
  another key. A log without the line is judged exactly as before, so an older build stays NOREF rather than turning green.
- Every check stays: a key-down with no dispatch line of any kind is UNVERIFIABLE; a missing `os_key_event` is "lost before the app".

### Rejected

- **Allow the Control chords the injector posted, as Cmd chords are allowed.** Sender-side: it would excuse a Ctrl-C that really
  went nowhere, and `Ctx.reset()` posts one around every case. (r10-injector.md rejected the same idea for the reset's keys.)
- **Count `ctrl_c_keydown` as the dispatch line.** Only the terminal-mode path writes it; it names one key of one menu item.
- **Log in `dispatch_custom_action`.** It also runs for a clicked menu item and for tests; the count would no longer be keys.
- **Add the four rows to `LOSS_CASES`.** That removes the check from them.
- **More trials.** The trial count is not why the rows are NOREF. `APPCI_SSH_TRIALS` stays 3 (12, 3 and 1 trials, 62 s per mode).

### Risk

- Not compiled. One Rust function, one Objective-C block, one extern declaration.
- If AppKit triggers the item with a current event that is not the key-down (not expected: key equivalents run inside
  `sendEvent:`), the line is missing and the rows stay NOREF, as today. They cannot become a false PASS that way.
- While the user is recording a key binding, `dispatch_custom_action_keystroke` re-sends the keystroke as an unbound key, which
  also logs `keydown`: that key would be counted twice. No case records a binding through a menu chord.

## 3. The ssh target on the Mac, and what it covers

`appci/sshd.py start()`: the system `/usr/sbin/sshd` (OpenSSH_10.2p1 on the runner), run as the runner user on `127.0.0.1` with a
scratch config, key-only, `ForceCommand` a script that starts `/bin/bash` with HOME = a scratch remote home. On macOS `/bin/bash` is
3.2.57. Not the lab, not the paramiko fake (the runner's python has no paramiko, so there is also no password host: the full
tier's `ssh_password_prompt` covers the passphrase prompt only there).

The table of what each platform's fix tier covers is in `docs/testing.md` ("Round 12: what the fix tier tests of ssh").

## 4. Playing the lab on the Mac in the candidate tier (`app-test-lab-macos`)

Nothing below is enabled by this branch.

**What exists.** The job (manual, `extends: app-test-macos`, `APPCI_LAB=fixed`, `APPCI_LAB_SLOT=mac`, `APPCI_CASES=lab_*`,
resource group `mac-gui`). `lab.Lab.start_fixed` attaches over HTTP to a persistent container on the box (slot `mac`, ssh 22202,
control 7002, kept up by the `sshlab-serve` user service that `sshlab-image` installs) with `SSHLAB_BOX` / `SSHLAB_TOKEN`
(protected variables: protected refs only). Lab keys are made per run with the Mac's `ssh-keygen`.

**Read-only look at the Mac runner (2026-11-10).** arm64, macOS 26.4.1, 8 cores, 16 GB, 149 GB free. No docker, podman, colima,
lima, OrbStack or `container`; qemu and UTM are there. bash 5 / fish / nu from Homebrew, zsh, OpenSSH 10.2p1. `ci.env` has no lab
settings (they come from GitLab). The box's slot ports 22202 and 7002 accept a TCP connection from the Mac; ping to the box
2.8-6.7 ms (avg 5.0).

**No container runtime is needed on the Mac**, and none should be added: the lab is a Linux host on purpose (netem, two sshds,
Linux users per shell), a VM on the 16 GB Mac would share the machine with the app under test, and the fixed slot already gives the
Mac a remote across a real network.

**What is missing.**

1. *The helper file.* `resolve_helper_bin` reads `APPCI_LAB_HELPER_BIN`, else the box-local cache. The Mac has neither, so every
   `helper` arm is UNVERIFIABLE. Proposed: `app-test-lab-macos` gets
   `needs: [{job: build-macos-arm64, artifacts: false}, {job: lab-helper, artifacts: true, optional: true}]` and
   `APPCI_LAB_HELPER_BIN: "$CI_PROJECT_DIR/helper-artifacts/phosphor-remote"`. `lab-helper` is a no-op of seconds when its key is
   cached and still exports the entry, `.version` file included, which `helper_version_line` reads where the binary cannot start.
   Same pipeline means same commit, so the version name matches the app's. Not played: the arms say "does not name a built
   phosphor-remote binary". To check on the first play: that the cached path of `lab_helper.py build` exports `helper-artifacts/`.
   Rejected: carrying the file by hand (a stale helper with the same `v0.1.18-dev` name is used silently); serving it from the
   lab's control port (new code in the lab for what an artifact does).
2. *The bundle's name* (#2042, worked in this branch). `stage_helper_asset` named it after the runner's CPU: `linux-aarch64` on the
   Mac for an x86_64 helper and host. Now named after the helper's ELF machine (`cases_lab.helper_arch`).
3. *`APPCI_APP_PINNED_DIGESTS: "0"`* on the Mac and Windows lab jobs, as on Linux (r12-policy.md 3): the dev build refuses every
   upload, so `upload` / `upgrade` can only fail and `never` can only pass.
4. *Play-time variables.* `ci_seq.lab_fix_vars` (`APPCI_FAKE_MODEL=1`, `APPCI_LAB_MATRIX=quick`, the case globs) is passed to the
   Linux job only. The scripted model already runs on the Mac (the `agent@fake` pass).
5. *`ci_seq.py`.* It plays the lab for Linux in the fix tier only; candidate and release play no lab job on any platform
   (self-test line "never played in candidate/release by APP").
6. *The slot's image* is whatever `sshlab-image` last installed: a change under `script/realapp/sshlab/**` needs that job before
   the Mac job, as on Linux.

**What it would add** over the Mac's fix tier: macOS ssh client to a Linux host over a real link; remote bash 5, zsh, fish, nu;
150 ms arms; the helper arms; the agent on a remote host (`lab_agent_remote_*`, `lab_fam_agent_*`).

**Cost.** Counted from the registry (`cases.CASES`, `lab.matrix_selection`):

| selection | arms | rows (arm x mode) |
|---|---|---|
| `PROVING_LAB`, quick | 59 (35 nohelper, 24 helper) | 91 |
| `lab_*`, quick | 413 (239 nohelper, 174 helper) | 502 |
| `lab_*`, full | 1149 | 1346 |

The only timed Linux run (r18w9b) spent 1117 s on 36 rows, most of them failing rows that ran to their timeouts: about 30 s a
row as an upper figure. `PROVING_LAB` quick is then 30-45 min; `lab_*` quick 2.5-4 h. It runs after `app-test-macos` in the same
resource group (the candidate tier's Mac app run has a 4 h timeout), so the Mac's wall time is what it costs. It uses the box's
slot container (one job at a time per slot) and no cargo.

**Recommendation.**

1. Not in a tier yet. The Linux lab has two recorded runs and no clean one (r18w9b: 2 pass, 21 fail, 13 noref; the harness
   repairs of r10-lab / r11-wl are unproven). A Mac play now would triage the same harness faults a second time on the slower
   machine. Gate: one Linux lab run with no FAIL on the same case set.
2. Then one play by hand with no helper: `APPCI_CASES` = the `PROVING_LAB` globs, `APPCI_FAKE_MODEL=1`,
   `APPCI_LAB_MATRIX="shells=bash,zsh,fish,nu;lat=lan,150;helper=nohelper"` (35 arms, about 30 min). It proves the fixed slot, the lease,
   the RTT gate over the real link and the remote readers from macOS. Needs no change to the repo.
3. Then items 1 and 3 above, and a second play with `APPCI_LAB_MATRIX=quick`.
4. Candidate tier: `PROVING_LAB` (plus what graduates) at quick, after the Mac app tests; `lab_*` quick only in the release tier.
   Windows the same way, one platform at a time.

Owner decisions: whether and when to play step 2; whether the candidate tier takes the extra 30-45 min on the Mac.
