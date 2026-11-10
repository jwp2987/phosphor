# Round 13: `precmd_secret_honor_ps1` on Linux Wayland, round 12 (#2050)

Branch `fix/r13-wlps1`, from `integ/r13` 18978908c. Nothing here was compiled or run; Rust is
`rustfmt --check` only. Wording: "worked, unproven" until a run shows it.

Failure (build/r12a, Linux Wayland, the only two failing rows; the case is new in round 12):

- terminal: `[hp0] pp_hp0_pin.txt holds ['\x10pint42792p31d'], want ['pint42792p31d']`, and
  `the PIN (or half of it) was in the input editor when an Enter was pressed, input_type Shell`.
  `[hp1]` the same.
- agent: `[hp0] pp_hp0_pin.txt holds [], want [...]`, `... input_type AI`, `the line typed after
  the read did not run`. `[hp1]` the same.

## 1. Verdict: APP, a design gap. The case is right

A precmd function's `read -s` in the root pane is not handled on Linux at all (nor, read from the
code, on macOS). The #1923 / #2038 work is ConPTY only. The harness typed exactly the PIN and one
Enter (injector accounting: sent = seen, lost 0).

Slice `run_slim/logs/terminal/case_precmd_secret_honor_ps1.log`, after `touch pp_hp0_go.txt`:

    hop=precmd_seen consumed=false seq_mode=true enters=1 precmds=0
    hop=handoff_commit_at_prompt committed=true still_pty_owned=false      <- the editor owns input
    hop=kh_focus_input ... hop=kh_focus_editor
    (1.3 s later) keydown key="p" ... hop=kh_editor_action kind=insert ... buffer="" / "p" / "pi" ...
    hop=kh_editor_action kind=enter ... buffer="pint42792p31d"
    hop=enter_begin deferred=false buffer="pint42792p31d" input_type=Shell
    hop=prompt_fact_sample what=prompt_fact answer=NotAtPrompt             <- sampled AFTER the decision
    hop=write_command
    pty_write bytes=15                                                     <- 13 + 2
    hop=decset_2004 on=true                                                <- the read returned

- No `hop=hand_route` and no `prompt_hold_*` for the PIN: the keys never reached the hand-off.
  They were editor inserts. The screenshot `terminal_pp_hp0_after_pause` shows `pint42` in the
  input editor under `vault pin: `, in clear text.
- **Where the 0x10 comes from: the app.** A command the editor submits is written as the shell's
  kill-line bytes, the text and `\r` (`bytes_to_execute_command`, `pty_controller.rs`; for bash
  the kill byte is Ctrl-P, which the bootstrap binds to `kill-whole-line`, `bash_body.sh:741`).
  `read -s` is not readline: it takes the byte literally. 15 bytes = `\x10` + 13 + `\r`. Not the
  harness, not history-up, not a zsh binding.
- **Why the agent pass differs:** same cause, other Enter. The buffer is submitted to the agent
  (`hop=enter_begin buffer="pint303x3qgsi" input_type=AI`, then a chat request with
  `query_len=13`): the secret is sent to the model, nothing is written to the pty, the `read`
  keeps waiting, and the line typed next goes to it.
- The prompt hold does not run on Linux: `prompt_hold_armed()` needs `conpty_input`
  (`typeahead_handoff.rs`), so `on_precmd` starts no wait and `try_commit_at_prompt` hands input
  to the editor at the Precmd, before the precmd functions have run.

`hp1` (honor PS1 on): the same failure in both modes, same cause. The prompt mode changes where
the line-editor marker rides (in PS1 instead of the last precmd function), not who owns input
after the Precmd. Not an honor-PS1 finding. The fix below does not depend on the prompt mode.

`subshell_ready_zsh_newuser_install` passes in this run (#2016).

## 2. What the platform knows

`read -s` leaves the tty canonical with echo off. `terminal_attributes::tty_is_reading_secret`
(one `tcgetattr`) is that fact; it is already what the ssh typeahead release and the young Enter
use. A shell at its line editor is never in that state (readline, zle, fish run it non-canonical)
and a shell between commands has echo on. Nothing asked it on this path: the editor owned input,
so no key reached the hand-off.

The shell's own account says when the question is allowed: `ShellActivity::Prompt` = a Precmd was
parsed and neither the line editor's marker (`LineEditorReading`, sent by bash after the last
precmd function, in both prompt modes) nor a Preexec since.

## 3. Fix (#2050)

"A secret read ahead of the line editor takes input back from the editor."

- `ForegroundProbe::with_secret_read` (`typeahead_handoff.rs`), installed on Unix with
  `tty_is_reading_secret(fd)` (`terminal_manager.rs`). Kill switch `PHOSPHOR_SECRET_READER=0`:
  the probe is not installed and nothing below runs.
- `TypeaheadHandoff::secret_reader_look_due()`: a state fact (no clock, no sample): the probe is
  installed, activity is `Prompt`, no reader noted, no hold queued, the editor or the pty owns
  input outright.
- `TypeaheadHandoff::take_input_for_secret_reader()`: when due and the tty reads a secret, the
  pty takes input (`PtyOwned { false, 1, 1 }`, the state a Precmd leaves on ConPTY where #2038
  notes its reader) and the reader of #2038 is noted (`prompt_reader_asks`). From there the
  existing reader path runs unchanged: every user write is the reader's
  (`route_prompt_reader_bytes`: written at once, not line content, not an Enter for the shell, no
  young Enter), `try_commit_at_prompt` and `on_line_editor_active` leave input with it, and it
  ends on events only: the line editor's signal, a Preexec, the next Precmd, a new session.
- The view asks (`TerminalView::hand_keys_to_a_secret_reader`, `view/paste_queue.rs`) at two
  events:
  - **pty output** (`handle_terminal_wakeup`): `read -s -p` prints its question after it has set
    the tty, so this is the usual first event; focus moves to the terminal before a key is typed.
  - **a key at the pane's sink** (`on_pane_key_event`), before the destination is decided: a read
    that prints nothing, or a key that beats the wakeup. The key is routed by the terminal
    whatever view has focus (a text field of the pane and a card keep their keys). While a secret
    reader is noted the sink keeps doing that, so a focus move that had to wait for the parser
    (`terminal_focus_redetermine_deferred`) loses no key to the editor. The model is never
    waited for on the key.
- `pty_controller.rs`, `on_line_editor_reading_marker`: when the marker ended a reader, the
  hand-off decides again. A session that enables bracketed paste decides at that signal (the retry
  does nothing while the decision awaits it). One that does not (bash 3.2, the macOS arms of the
  case) has no later event; without the retry the pty would keep input until the next command.

Result for the case, both arms and both modes: the question's output takes input back, the PIN
and its Enter go to the `read` as typed (no kill byte: nothing is submitted by the editor), the
editor never holds a character, nothing is sent to the agent; the marker and the bracketed-paste
enable end the reader, the decision is `Committed`, and the next line is typed into the editor.

## 4. Rejected

- **Arming the prompt hold on Unix** (the Precmd does not commit until the line editor signals,
  as on ConPTY): it puts every prompt of the owner's main platform on the hold, the enter window
  and the cap, for a case the tty answers exactly. Too wide for a change nobody can compile here.
- **Reading the screen** (#2038's fact): a guess where the OS says it. Kept for ConPTY only.
- **`NotAtPrompt` instead of the secret fact:** it is also true while a precmd function runs any
  plain program (`git status` for a prompt). Keys typed then are typeahead for the line, and the
  editor is where they belong today.
- **A hook in `input.rs`** (the editor's Enter): not needed once focus has left the editor, and
  that file is another agent's.

## 5. Risk

Low-medium. Unix only, and only between a Precmd and the line editor's marker with the tty
canonical and echo off.

- Cost: one read of the hand-off per key and per output wakeup; one `tcgetattr` (about a
  microsecond) only while activity is `Prompt`. For bash that is the few milliseconds of the
  precmd stage. A shell that sends no marker stays `Prompt` for the whole prompt: one `tcgetattr`
  per key and wakeup there.
- A user who ran `stty -echo` by hand: the tty is canonical with echo off between each Precmd and
  readline's start. Input goes to the pty for those milliseconds and comes back at the signal
  (a focus round trip per prompt; keys typed in it are requested into the editor by the unread-keys
  rule of #2038).
- Text already in the editor when the read starts stays there, unsent; the keys typed next go to
  the read. A plain terminal would have given the read all of it.
- The first key being the Enter (an empty answer) while the editor still has focus: the editor's
  key binding takes an Enter before the pane's sink sees it. Only when no output event came first
  (a `read -s` with no `-p`, or a program that prints before it sets the tty) or focus could not
  move. Not closed here; see section 7.
- pwsh on Unix and fish were not traced. fish's `read -s` runs its own reader (non-canonical):
  not seen by this fact, unchanged.

## 6. Tests

- `typeahead_handoff_tests.rs`, five tests under "#2050": the round 12 sequence
  (`a_secret_read_at_a_prompt_takes_input_back_from_the_editor_and_gets_every_key`), the read
  seen before the Precmd commits, where the tty is not asked, the events that end the reader and
  the unread-keys rule, and #1658's keys-follow-editor.
- `view_test.rs`: `a_secret_read_at_a_prompt_takes_the_keys_from_the_focused_editor` (the pane's
  sink with focus reported in the editor; Legacy and Table; with a control arm where nothing reads
  a secret and the editor gets the keys) and
  `a_secret_question_at_a_prompt_takes_input_back_at_its_output`.
- No case change. `precmd_secret_honor_ps1` already checks every effect and was not weakened.

## 7. Open, not decided here

- **An echoing question from a precmd function on Unix** (`read -p "continue? " x`): the tty is
  canonical with echo on, the same as a shell between its Precmd and its line editor, so the OS
  fact does not tell. The answer goes to the editor today. Filed as #2052.
- **Agent mode hides the question.** `question_on_screen_hp0/_hp1 = false` in the agent pass (the
  screenshot `agent_pp_hp0_asked` ends at the `touch` block): the background block that holds
  `vault pin: ` is not shown in an agent conversation. With this fix the keys reach the read, but
  the user answers a question they cannot see. An owner question (what an agent conversation
  shows of shell output no command produced). Filed as #2053.
- The Enter-first edge of section 5.

## 8. What the next run should show

Linux Wayland and X11, `precmd_secret_honor_ps1`, both modes, both arms:

    hop=prompt_reader_asks by=tty_secret took_from_editor=true
    hop=secret_reader_takes_input site=output took_from_editor=true
    hop=hand_route reason=prompt_reader state=pty_owned bytes_len=1 ...     (14 of them)
    hop=prompt_reader_end why=line_editor_signal line_dirty=false
    hop=handoff_decision decision=Committed

- No `hop=enter_begin` with the PIN, no `kh_editor_action` for its keys; the effect file holds
  the PIN alone; the next line runs.
- `site=key` instead of `site=output` means the tty was not yet set when the question's wakeup
  ran: still a pass, worth a note.
- If the PIN file holds the PIN but the next line does not run: the reader did not end or the
  decision did not commit; look for `prompt_reader_end` and the `handoff_decision` after it.
- macOS (bash 3.2 forced by the case): `hop=prompt_reader_end_retry` after the marker, then
  `handoff_decision decision=Committed`.
- Windows: unchanged (no probe is installed; `secret_reader_look_due` is false).
