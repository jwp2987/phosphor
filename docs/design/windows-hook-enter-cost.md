# Windows: what the hooks cost between Enter and the command start (#1280, #1492)

Design note, round 23. Base: `55f3f48d7`. Nothing was compiled or run; this is attribution from the code
and the numbers already in the two issues and in `docs/design/r13-recon.md`. Verdict: no code change is
both measured and safe from here, so none is made. The sections below say what is left, what each
candidate would save, and what has to be measured before anyone writes it.

## 1. The numbers that exist

| item | PowerShell 5.1 | Git Bash | source |
|---|---|---|---|
| Enter to command start over conhost | 39.9 ms (r11a), 62-82 on earlier builds | 31.0 ms (r11a), 26-31 earlier | #1280, #1492, r13-recon |
| app share, `enter_begin` to `pty_write` | 1.79 ms | 0.93 ms | r11a, #1492 owner note 2026-11-10 |
| Preexec in-process (no ConPTY) | about 1 ms | n/a | #1280 winhooks finding |
| Enter to Preexec, ConPTY, no app | 7.5 ms, same as conhost | n/a | #1280 winhooks finding |
| bootstrap hooks outside the app | prompt end 18 ms vs 10.5 ms bare PSReadLine | 21.2 ms (G2) | #1280, r13-recon |
| fork + exec on the host | n/a | 39 ms per process | r13-recon |

Budget: Phosphor's own overhead from Enter is at most 2 ms. The app's share is inside it on both
shells. The rest of the gap is the shell, ConPTY and what the reference does not have; the owner
accepted conhost as the only ConPTY reference (2026-11-10), which has no ConPTY layer, so some of
the gap cannot be removed by any change in this repo.

## 2. Attribution, step by step

### 2.1 PowerShell 5.1 (`app/assets/bundled/bootstrap/pwsh.ps1`)

Between Enter and the command start:

1. App: Enter handling to the pty write. 1.79 ms (measured). Not a cost source.
2. Single-line command: the command is not typed. `pty_controller.rs` `command_to_type` serves it on a
   per-session named pipe and writes only the insert chord `ESC 3` (`TypedCommand::FileChord`; no
   kill-buffer prefix, #1581/#1592). The 43-char burst of the #1280 comment of 2026-11-01 is therefore
   already gone for commands that qualify (`long_command_file::can_insert_via_pipe`). A command that
   does not qualify is typed as `ESC 2` + line + `\r`, with `ESC 2` written as its own message
   (`split_kill_buffer_write`, `pty_controller.rs`): the split exists because PSReadLine can fail to
   split ESC-prefixed chords that arrive in one read. Removing the `ESC 2` for a known-empty line is
   listed as a candidate below; it is an input-correctness change, not a hook change.
3. PSReadLine accepts the line and returns to `PSConsoleHostReadLine`; `Warp-Preexec`
   (`pwsh.ps1:362`) runs. What it does now: `$script:warpSeq` increment, one `-replace` for the title,
   a hand-built JSON string (`:373-390`, #1582), one `Warp-Encode-HexString` of that JSON, one
   `[Console]::Out` write with the reset-grid OSC (`Warp-Write-Osc`, one write, #1280 58aec7d01), and the
   window title AFTER the OSC. `Warp-Stop-ActiveThread` is not reached for a user command (the test at
   `:403` is `-not "$command" -match ...`, see 2.3). Measured about 1 ms in-process. The hex encoder
   is the only per-byte work left; it grows with the command length, not with anything fixed.
4. The command starts. Everything after this point (Precmd 3.7 ms in-process, the prompt 5.8 ms,
   `Warp-Idle-Maintenance` at the next read) is after the command start and does not enter the
   Enter-to-start metric; it enters Enter-to-last-paint for `echo hi`.

Already removed from the Enter path by earlier rounds, so not candidates: housekeeping moved to
`Warp-Idle-Maintenance` (0b365bf8c), PSReadLine `ExtraPromptLineCount` write only on change
(86a77a6a1, ce5ec0d08), lazy runspace pools (1eac12788), cmdlets dropped from Precmd (028ddf2a7),
hand-built JSON (966dc49bb, kill switch `PHOSPHOR_PWSH_HANDJSON=0`), node/git probe caches by PATH
and the in-process `.git/HEAD` read (`pwsh.ps1:913`, `:972`), one console write per hook.

### 2.2 Git Bash (`bash.sh`, `bash_body.sh`)

Between Enter and the command start the shell runs the DEBUG trap, then the bash-preexec dispatcher,
then `__warp_preexec_impl`:

1. DEBUG trap (`bash.sh:465`): a test that skips the dispatcher unless `__bp_preexec_interactive_mode`
   is set or the PROMPT_COMMAND repair is due (#1283). Builtins only. Runs before every simple
   command of the line, but the real work happens once.
2. `__bp_preexec_invoke_exec` is `__warp_run_relaxed` around `_impl` (`bash.sh:437`, `bash_body.sh:94`):
   about 25 builtins to turn `set -e/-u/pipefail/-E/-a` off and restore them (120-250 us on Linux).
3. The command text for the user's preexec functions comes from `history 1`:
   bash 5.3 `${ ...; }` without a fork; bash 5.2 (Git Bash) `__bp_history1_viafile` (`bash.sh:278-296`,
   #1492): `history 1 >| file`, `read -d ''` from it, strip. No fork after the first mktemp, but it is a
   file create/truncate, write and read on every command. On Windows a file open is a Win32
   `CreateFile` through the MSYS layer, and antivirus hooks see it. Not measured.
4. `__warp_preexec_impl` (`bash_body.sh:411`), MSYS2 branch (`:421`): five `printf -v`, one `printf` to
   `/dev/tty` (one ConPTY round trip), reset-grid OSC in the same write (#1492). The remaining
   non-MSYS branch is not used here.
5. Then the generator-pid check: two `[[ -f ]]`, no fork unless generator pids exist.

No fork remains in this chain after `__bp_h1_file` exists. The 21.2 ms hook cost G2 and the 31 ms
gap are therefore NOT explained by forks in steps 1-5; they are most plausibly the sum of (a) the
file I/O in step 3, (b) the `/dev/tty` open/write/close through the MSYS tty layer in step 4, (c)
bash itself running more builtins under MSYS than under Linux (every builtin that touches the
environment copies it on a fork-emulating runtime), and (d) readline ingesting a line typed in one
burst, which the conhost reference does not do (comment of 2026-11-02: "the editor sends the whole
line + `\r` in one burst"). None of the four is separated by the data on hand.
The forking part of the bash hooks is in Precmd, after the command: `$(builtin pwd -P)`, `git
symbolic-ref`, `git rev-parse`, `$(node --version)` (`bash_body.sh:736-990`, non-MSYS and MSYS
branches differ; git runs on MSYS). At 39 ms per process that is the Enter-to-NEXT-PROMPT cost, not
Enter-to-start, and it already has `warp_git`'s result handled per prompt.

### 2.3 One real defect noticed on the way (not a perf item)

`pwsh.ps1:403`: `if (-not "$command" -match '^Warp-Run-GeneratorCommand')`. PowerShell binds `-not`
tighter than `-match`, so this is `(-not "$command") -match ...`, i.e. `$false -match '^Warp-...'`, which
is always false for a non-empty command: `Warp-Stop-ActiveThread` is never called for user commands.
Fixing it would ADD work to Preexec (a call that stops threads) and change behaviour, so it is not a
perf change and is not made here. It needs its own issue before anyone touches it.

## 3. Candidates

Each is listed with what it would save, risk, kill switch and how it is measured. None is
implemented: the saving is unmeasured, and the budget rule is "measured, small, gated, or drop".

| id | change | expected saving | risk | kill switch | status |
|---|---|---|---|---|---|
| C1 | Git Bash: skip `history 1` when the only preexec function is `warp_preexec` (it reads `$BASH_COMMAND`, `bash_body.sh:412-419`). Set `this_command=$BASH_COMMAND`. | step 3 entirely: one file create/write/read per command; unmeasured on Windows, 0.1-0.3 ms on Linux | medium. `history 1` differs from `$BASH_COMMAND` for commands ignored by `HISTCONTROL`/`HISTIGNORE` (Phosphor sets `ignorespace`, and in-band commands start with a space: `this_command` then is the PREVIOUS history line). The `-z` sanity check relies on that to skip non-interactive. A user preexec added later would see the wrong text | `PHOSPHOR_BASH_PREEXEC_NO_HISTORY=0` | DROPPED: changes which commands fire Preexec, cannot be proven without the Windows lab |
| C2 | Git Bash: merge the DEBUG-trap dispatcher into one function so `__warp_run_relaxed` is entered once, not twice | 25 builtins, tens of us | low | none needed | DROPPED: below noise |
| C3 | PowerShell: do not write `ESC 2` when the shell's line is provably empty (hand-off knows the line is empty) | one `ConPTY` message and one PSReadLine key dispatch, about 1-3 ms; unmeasured | high: a wrong "empty" leaves typed-ahead text in front of the command (the `ESC 2` exists to prevent that); touches the typeahead hand-off that has had repeated races (#1996, #1997, #2024) | would need `PHOSPHOR_PWSH_SKIP_KILL=0` | DROPPED: single-line commands already take the chord path with no kill prefix (2.1 step 2); this only affects the typed fallback |
| C4 | PowerShell: skip hex-encoding the Preexec JSON for short commands | encoder is per byte; ~0.1 ms | low | n/a | DROPPED: below noise |
| C5 | Git Bash: send `Preexec` from the PS0 expansion instead of the DEBUG trap (bash >= 5.0), no trap, no history | removes steps 1-3 | high: PS0 runs before the command only in an interactive prompt and skips agent wrappers and `bind -x`; different semantics for `PROMPT_COMMAND` repair | would need a switch | DESIGN ONLY; revisit only if the split below points at the shell hooks |

## 4. What decides it: the measurement that is already built

Do not write C1/C5 before this has run on the Windows box with the split on:

- `APPCI_ENTER_SPLIT=1` (`script/realapp/appci/cases.py:4347`, off by default so runs stay
  comparable) splits Enter-to-start into `t0_to_lf_ms` (delivery + line ingest) and `lf_to_stamp_ms` (the
  hooks) for Phosphor and for the conhost reference, with the same no-fork `PS0+=` record.
  Reading: if `lf_to_stamp_ms` is within ~2 ms of the reference, the hooks are not the cost and
  the gap is delivery/ingest (not fixable here; document it as accepted). If it is 15+ ms over,
  C1 and C5 are justified and step 3/4 are then timed individually with `EPOCHREALTIME` stamps in
  a variant `bash_body.sh` placed next to the exe (as the #1280 finding did for pwsh).
- PowerShell: `PHOSPHOR_PWSH_HOOK_TIMING=<file>` plus `<file>.on` (marks k0..e, `pwsh.ps1:62-92`,
  analysed by `pwsh_timing.py`); the app logs `qpc=` on `pwsh_command_via_pipe` so the marks are on
  the app's timeline. The metric `app_enter_to_pty_write_ms` is the app's share.
- Both rows pass only when the app share is at most 2 ms (it is: 1.79 and 0.93) and the
  Enter-to-start delta against conhost is explained hop by hop. The delta itself has no target the
  repo can hit alone, because the reference has no ConPTY.

## 5. Outcome

No code change. The two issues stay open as measurement items: #1280 and #1492 are not marked
fixed-untested because nothing was changed and their numeric targets (conhost parity) were not
met. The app's share is inside budget. The next action is one Windows run with
`APPCI_ENTER_SPLIT=1` and the pwsh timing file on, then choose from C1/C5 or close both as
"accepted, shell and ConPTY cost".
