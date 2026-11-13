# Round 10, remote: #1969, #1970, #1983

Branch `fix/r10-remote`, from merge/w0 `41340e2fd`. Nothing here was compiled or run in the app;
the shell-side work was run in real shells (see each item).

## #1969 remote bash 3.2 (macOS `/bin/bash`): no in-band Tab

**Cause.** The in-band exchange (#1801) was limited to bash 4.0+ in three places:
`app/assets/bundled/bootstrap/bash_body.sh` bound `^Y` / RS only under
`WARP_INPUT_REPORTING_SUPPORTED` (4.0+) and advertised `!phosphor_nc_frames_v1` only for
`BASH_VERSINFO[0] >= 4`; `Shell::supports_native_shell_completions`
(`crates/warp_terminal/src/shell/mod.rs`) asked for `input_reporting_sequence()` (4.0+); and the
appci cases skipped on macOS (`need_in_band_remote_shell`). The reason given then was that a
`bind -x` function has no `READLINE_LINE` below 4.0.

**What the limit really was.** The text to complete travels in the app's frame, so the ANSWER never
needed `READLINE_LINE`. Only one thing did: handing back bytes the hook read that are not a frame
(a key typed in the window, an Enter).

**Fix.** bash 3.2+ binds the keys and advertises the protocol. Below 4.0 the hook queues what it
must hand back as the keys it read, and `__warp_nc_finish` rebinds the macro's second key
(`\C-x\C-_9`) to a macro of exactly those keys; readline runs it right after the function, ahead
of anything still in the tty, so order is kept (no key lost, reordered or merged). Three more
3.2 differences are handled in the bootstrap: `read` has no `-N` (`-n 1 -d ''`, so a newline is a
byte); readline moves to a new line before a `bind -x` function (cursor-up + erase first); and
the DEBUG trap cannot tell a readline binding by `READLINE_LINE`, so it returns for the hook's
commands by name and while the hook's flag is set (without this every Tab sent a Preexec with no
CommandFinished). The same trap defect existed for the prompt-mode keys ESC p / ESC w on bash 3.2
(filed as #2002, same guard). App side: an advertising bash from 3.2 is supported; an unknown
version is not. Kill switch for the new path alone: `PHOSPHOR_BASH3_NATIVE_COMPLETIONS=0`.

**Rejected.** (a) `compgen` driven from the prompt hook, as the issue suggests: the prompt hook
runs once per prompt, before the user types, so it cannot answer a Tab; it would need a second
protocol next to the framed one. (b) Typing the candidates' request as a visible command: it
would land in history and on screen. (c) Editing the line with `bind` tricks per byte instead of
one replay macro: more round trips through readline, same result.

**Risk.** Low for 4.0+ (the 4.0+ path is unchanged except the bind condition, which is the same
version test under another name; `script/test_native_completions` passes on 4.0, 4.3, 4.4, 5.1,
5.3). Medium for 3.2: run only in the Linux `bash:3.2` image (3.2.57) in a pty, not on a Mac and
not in the app. The macOS rows of `remote_tab_no_helper` / `remote_tab_enter_no_helper` are the
real-app proof and now run there.

## #1970 ssh over a slow link is unverified

**Finding: the lab and a 150 ms case exist; they were never played.** `lab_enter_start`
(`script/realapp/appci/cases_lab.py`, since `829e27ba5`, 2026-11-08) types 20 lines in a netem
150 / 300 ms session, interleaved with a plain `ssh -tt` control. But lab cases run only when
`--cases` names them, the fix tier plays only the `cases.PROVING_LAB` globs
(`lab_agent_remote_*`, `lab_fam_agent_*`), and no run under `test-results/` has a `lab_enter_start`
row. So this is a TEST gap, not an app change.

**What was missing in the case set.** `lab_enter_start` judges the whole Enter -> command start
time against plain ssh, on the remote clock. Nothing asserts the issue's own sentence, "Enter
never waits on the network", from the app's side. The app already logs what is needed:
`hop=enter_begin` and the line's `pty_write` (`cases.enter_segments`, `enter_to_write_ms`; 0.45 to
1.53 ms in the r18w8 `latency_enter_ssh` rows).

**Fix.** New lab case `lab_enter_no_network_wait` (bash zsh fish nu x 150/300 x nohelper/helper,
both modes): lines typed with an instant Enter at the arm's delay, then the same number with the
delay removed, in the same ssh session. Verdict (`lab.enter_wait_verdict`, pure, self-tested):
1. no Enter -> write sample at the delayed RTT reaches half the RTT. A wait on any network round
   trip costs at least one whole RTT, so this bound needs no tuning;
2. the delayed median exceeds the same session's undelayed median by at most 2 ms (the owner's
   Enter budget): the app's share of Enter must not grow with the link delay.
The effect file must hold every line once, in order. `PROVING_LAB` gains this case, the 150 ms
arms of `lab_enter_start`, and `lab_ctrl_c_no_esc_i` (#1983), so the fix tier plays them.

**Rejected.** A new app log hop for the ssh line write: the existing hops already give the
number. Judging only against the remote clock: `lab_enter_start` does that; a clock-free check
is the stronger statement of "never waits".

**Risk.** Harness only. The case has never run; its first run may show a harness fault. Fix-tier
lab time grows (estimate in the report).

## #1983 ssh twin of the Ctrl-C case; "Session abandoned before bootstrap"

**Twin: exists.** `lab_ctrl_c_no_esc_i` (same commit `829e27ba5`) is the managed-ssh twin of
`ctrl_c_no_esc_i_local`, LAN and 150 ms, with and without the helper, both modes. It was outside
the fix tier and has no recorded run. It joins `PROVING_LAB`. TEST gap, no new case needed.

**The warning: nothing re-enters the pre-bootstrap state for it to fire.** Evidence:
- The line is written in exactly one place, `impl Drop for TerminalView`
  (`app/src/terminal/view.rs`), and describes the view BEING DROPPED: its model's
  `block_list().is_bootstrapped()`, `pending_shell_type()`, `has_pending_ssh_session()`. A live
  session cannot log it about itself. The owner's session went on after both lines
  (CommandFinished 33 s later), so two OTHER terminal views were dropped, one never visible
  (WARN) and one that had been visible (ERROR).
- "shell None on ssh false" = no `InitShell` ever reached that model and no ssh wrapper was
  pending. Views for which that is always true: every view on a `MockTerminalManager`
  (`app/src/pane_group/mod.rs`: conversation transcript viewers, read-only viewers of a live
  conversation, "loading conversation" panes). Their block list starts at
  `BootstrapStage::RestoreBlocks` (`blocks.rs:983`) and no shell ever moves it. Closing or
  replacing any of them logs the line, at ERROR if it was ever shown. Agent mode is the owner's
  default, so these views are routine. A real shell pane dropped before its `InitShell` (a hidden
  child-agent pane, a tab closed at once) logs the same triple.
- The log line names no view, which is why it read as the ssh session's own.
- Separately, a live session DOES go back to a pre-bootstrap stage by design: every nested
  `InitShell` (ssh, subshell) calls `BlockList::reinit_shell` (`terminal_model.rs:4400`,
  `blocks.rs:4119`), stage `WarpInput`, until that shell's `Bootstrapped`. That is not what the
  line reports (it would say `shell Some(..)`), and it is not changed here.
- Which of the candidate views was dropped at the owner's two timestamps cannot be told from two
  lines with no view in them.

**Fix (small, log only).** A pure rule (`view/abandoned_session.rs`) decides the level: a view that
never was a shell session (a transcript viewer or loading pane) logs nothing; a shell session
dropped before bootstrap logs as before (ERROR if ever visible, else WARN). The line keeps its
text (harness greps it) and gains the dropped view's id, so the next occurrence can be matched to
a pane. No state machine change.

**Rejected.** Changing `reinit_shell` or the #983 exit path: no evidence ties them to the report,
and they sit under the ssh hand-over. Deleting the line: it is the only trace of a shell that
never bootstrapped.

**Risk.** Minimal: one log call's condition and text.

## #2196 remote Tab without the helper: zsh with compinit, fish, nu, and the bash 300 ms

The no-helper Tab asks the remote shell first, in-band (#1801). r27b/r29a showed it answering for
bash only. Where each shell stands now (`docs/testing.md`, lab `lab_tab_remote_dir`):

| shell | in-band answer | why |
|---|---|---|
| bash | yes (bash-completion, `__warp_native_candidates`) | since #1801 / #1969 |
| zsh WITH compinit | yes (`warp_main_completer`, compsys) | since #1801; the lab never had such a user |
| zsh WITHOUT compinit | empty by design, the completer answers | #2174: we never run `compinit` for the user (`warp_compsys_loaded`, `zsh_body.sh`) |
| fish | yes, new | `complete -C <line>` through the same exchange (below) |
| nu | no, the completer answers | no non-interactive completion API (below) |

**Lab.** `u_zshc` (shell key `zshc`, the same `/usr/bin/zsh`) sources the zsh hook and then runs
`autoload -Uz compinit && compinit -u` from its `.zshrc` (`labd.rc_plan`; `-u` so an insecure-
directory question cannot stop the login at a prompt nobody answers). `u_zsh` keeps no compinit:
it is the arm that proves the fallback is still correct. `zshc` is declared by `lab_tab_remote_dir`
only (`lab.MATRIX_SHELLS`; `lab.ALL_SHELLS`, which the families run on, is unchanged) and is in
the quick matrix. The case now judges the unique-prefix Tab against the app's own log:
bash, zshc and fish must log `N candidates from the remote shell in M ms` (the in-band ms is
recorded as `tab_native_ms`); zsh must log the empty-answer line and no candidates; a refusal
(`Native completions not started (why)`) is named and the row is UNVERIFIABLE, never a pass.

**fish.** The #1801 review left fish out: fish's `read` runs the line editor, and fish switches
its terminal modes (bracketed paste, keypad, modifyOtherKeys) off and on around every external
command, so a write released after the exchange's last OSC could meet a fish with bracketed paste
off. The bootstrap now answers (`fish.sh`, `__warp_native_complete`, bound to `^Y`; RS bound to
`__warp_native_discard`; the Bootstrapped hook advertises `!phosphor_nc_frames_v1` in
`shell_options` only where fish is 3.4+, not MSYS2, and `sh dd stty sleep` and a tty exist):
- the frame is read by ONE external `sh` (`__warp_nc_sh`): `stty -echo eol '^_'` (canonical mode,
  US as a line terminator), the prompt OSC, one `dd`. The line discipline returns exactly up to
  the frame's US, so bytes written behind it (the command after a cancel frame) stay in the tty
  for fish: nothing over-read, no inter-byte timing. A watchdog `sleep` (PHOSPHOR_NC_DEADLINE,
  default 20 s) is the give-up net and writes nothing.
- every external command has ended before the answer; the answer is builtin `printf` only
  (`OSC 9280;A;incrementally_typed`, `C;<token>`, `D?description;<text>`, `B`), so the app's
  release on `B` cannot meet a fish with its modes off.
- candidates are `complete -C <line>`; the `a;` of `echo a;ec` and an opening quote are put back
  (the app replaces the whole whitespace-delimited token), names are escaped as fish inserts them
  (not under a quote, not the `~/` of a home path), at most 2000.
- limits: a frame over the tty's canonical line (4095 bytes, about 1300 characters of line) never
  reaches its US and ends at the deadline; what is read around a frame is given back as
  printable text and an Enter only (a function cannot hand fish a control key). The app writes
  nothing between `^Y` and the frame (every other write first writes the cancel frame), so only
  terminal replies come in between.
- app: `ShellType::supports_native_shell_completions` now includes fish (`crates/warp_terminal`).
  Tests: `script/test_native_completions --only-fish` (the REAL fish bootstrap in a pty),
  `shell::mod_tests`.
- state: worked, unproven in the app. The remote round trip count is the exchange's: `^Y` -> prompt OSC
  (1 RTT) -> frame -> answer (1 RTT), i.e. 2 RTT on the wire, the same as bash and zsh. Plain ssh
  completes in 1 RTT because the remote shell has the line already; reaching that needs a different
  protocol (the line sent with `^Y`, before the token) and is not part of this change.

**nu.** nu 0.116.x (the lab image's) has no usable non-interactive completion API for a
bootstrap to call: completions exist only inside its own line editor (reedline, not scriptable)
and through the CLI mode `nu --ide-complete <pos> <file>`, which starts a SECOND nu process that
completes a file's text without the running session's aliases, custom completers, `$env` or
scope (the lab's own driver, `labd.COMPLETERS["nu"]`, uses exactly that and notes `-c <line>`
exits 1 with no output). So nu stays on the app's completer (correct, slower; r29a nu.150 536
ms); a nu answer would be a different feature (a nu-side completer written in nu), to be filed
separately. Left unchanged on purpose.

**bash, ~300 ms on a LAN (macOS lab `bash.lan.nohelper`).** Nothing in the in-band Tab path of
`bash_body.sh` or `pty_controller.rs` waits: the only timers on it are give-up nets (prompt 2 s,
results 15 s) that never fire on a normal Tab, and the hook reads with no per-byte wait. What the
r29a number measured: `lab_tab_remote_dir` started its clock BEFORE `ctx.be.key("Tab")`, and the
macOS backend's `key()` runs the injector (`htype2`) as a process and returns after it exits, so
the figure carried the process start-up and the key gap (`backend.py`, `MacBackend._ht`). The
clock now starts at the injector's own post time of the Tab (`cases_lab.tab_press_wall`, from
`backend.calls[-1]["post_wall"]`), falling back to the old clock where there is none (X11).
Other cost that is real and not a defect: bash-completion's `_cd` and `_filedir` fork several
times per Tab, the same work the user's own bash does. The first Tab right after connect can be
refused (`Native completions not started (a command is executing)`), which the case now names
instead of passing.
