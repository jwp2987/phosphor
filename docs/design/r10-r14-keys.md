# Round 14: readline habits (#1984 Escape with vi keys, #1988 Ctrl-R, #1989 Alt-.)

Owner decisions 2026-11-10 on three questions of the daily-user review
(`docs/design/user-annoyance-review.md` items 14 and 20). Branch `fix/r14-keys`, from `integ/r14`
4bf571aae. Nothing was compiled or run when this was written: the code is worked, unproven.

Not touched, by instruction: `terminal/input.rs`, `terminal/input/enter_queue.rs`,
`terminal/view/give_up_to_pty.rs`.

## 1. #1984: Escape in the agent view, in a pane with vi keys

**Decision.** Escape leaves the agent view for the terminal: kept. A pane whose editor or shell is
in vim / vi mode receives the Escape instead. A running command or full-screen program: unchanged.

**How an Escape gets to the exit (read, not run).**

- The editor takes Escape first (`editor/view/mod.rs` `escape`, about :6990): with vim keybindings
  on and something pending (insert, visual or replace mode, a pending operator) it is the vim
  state machine's and nothing is emitted.
- Otherwise `EditorEvent::Escape` reaches `Input::editor_escape` (`terminal/input.rs`, about
  :9478), which closes whatever is open, in order: a history menu in vim insert mode, the AI
  context menu, slash commands, an inline menu, the suggestions, a workflow, a vim mode other than
  normal, attached context. Only with none of those does it emit `Event::Escape`.
- `TerminalView::handle_input_event` (`terminal/view.rs`, `InputEvent::Escape`, about :33475)
  then, in the agent view: walks up to a parent conversation, or, where the view cannot be left
  (the agent's own command runs), stops the turn (#850), or leaves the view (asking first when
  the input holds text).

So the Escape that throws a vi user out is exactly one: vim normal mode with nothing pending and
nothing open, i.e. the "am I in normal mode?" press. A user whose shell is in vi mode has the same
habit with the editor's vim keys off.

**How vi keys are known.** Two facts the app already holds:

- the input editor's `vim_mode_enabled` (`text_editing.vim_mode_enabled` and the editor supporting
  it);
- the shell's bootstrap report: `vi_mode_enabled` from bash (`set -o vi`), fish and PSReadLine
  becomes the `vi_mode` shell option (`terminal/model/session.rs` about :1010), a zsh vi plugin is
  in `plugins`. This is what the "enable vim keybindings?" banner is offered on
  (`should_display_vim_banner`); the test is now one function, `shell_reports_vi_mode`, used by
  both.

**Fix.** `terminal/view/agent_escape.rs`, a pure decision: with the rule on, vi keys in the pane
and a view that could be left now (it can be exited and no command is long-running, read from the
published key snapshot), the Escape `StaysWithViKeys`; otherwise `Leave`, the existing block,
byte for byte. The gate is one extra condition on the existing `if` in `InputEvent::Escape`. What
follows that block (clearing prompt suggestions, `Event::Escape` to the pane group) still runs.
Kill switch `PHOSPHOR_VI_ESCAPE_STAYS=0`. One log hop per kept Escape,
`agent_view_escape_stays_vi`.

"Receives the Escape instead" for a shell in vi mode means the view stays; no byte is written to
the shell. In the agent view the input editor owns the line, the shell's line is not shown, and an
ESC written into it would put a hidden readline into command mode under the editor.

Ways out of the view in a vi pane, all existing: the back button, the zero state's "go back to
terminal" row (both dispatch `ExitAgentView`, not gated), Ctrl-C twice on an empty input. The back
button no longer shows the Escape key in such a pane (`update_agent_view_back_button_state`).

**Rejected.** (a) Gating in `editor_escape`: forbidden file, and the view owns the exit. (b) Only
when the buffer is empty: the habit press happens with text in the buffer too, and with text the
old code already asks before leaving, so the two rules would differ for no reason the user could
name. (c) Sending VimEscape to the editor from the view: the editor already had the key and found
nothing pending.

**Known gaps.** The shortcut list (`agent_view/shortcuts/mod.rs`) and the zero-state row still
name Escape in a vi pane (the row is clickable and works; the list is a hint). The back button's
hint follows the setting at the next state update (entering the view, a command starting or
ending), not at the moment the setting is switched.

**Risk.** Low. One condition, evaluated only in the agent view on an Escape that reached the view;
false for every pane without vi keys. A vi user who relied on Escape to leave must use Ctrl-C or
the button: that is the decision.

## 2. #1989: Alt-. (last argument); Ctrl-C in the agent view

**Decision.** Alt-. (Option-. on macOS) is the shell's last argument; keep readline's own
behaviour; Ctrl-C in the agent view pauses the agent, unchanged (nothing was changed there).

**What the code did.** The issue says Alt-. "has no editor binding". It has one,
`editor:insert_last_word_previous_command` (`editor/view/mod.rs` about :910), with readline's
repeat (a second press takes the command before, `update_last_word_insertion_state`). Its default
was `meta-.` on every platform. A key event carries `meta` only when an Alt/Option key is set to
act as Meta (`apply_extra_meta_keys`, `lib.rs` about :668; the setting's default is off), and winit
never sets it (`key_events.rs` about :165). So on Linux and Windows with default settings Alt-.
arrived as `alt-.` and matched nothing. The pty encoder treats the platforms the other way round:
Alt is Meta off macOS, Option is Meta on macOS only with the setting
(`meta_keystroke_to_escape_sequence`, `warp_terminal/.../escape_sequences.rs` about :618). So with
a program in front Alt-. already was `ESC .`.

**Fix.** Make the editor agree with the encoder:

- Linux and Windows: the editable binding's default is `alt-.`; `meta-.` (an Alt key set to act
  as Meta) stays as a fixed binding, so nobody loses the key they had.
- macOS: `meta-.`, as before: Option-. with Option as Meta. An Option key that is not Meta types
  the layout's character (`≥` US, `…` German) and keeps doing so, as in Terminal.app and iTerm2.
- The defaults live in the registry (`util/bindings.rs`), next to the quit key's.

With a program in front (a nested shell, an ssh without integration, a REPL) nothing changed:
`ESC .` reaches it and readline does its own yank-last-arg.

**Rejected: writing `ESC .` to the shell while the input editor is in front.** The instruction
reads "send ESC . to the pty in terminal mode/when a shell is in front". At the app's own prompt
the shell's line is hidden and the hand-off counts it empty (`typeahead_handoff.rs`: no user byte
is written after a request). `ESC .` there would insert the argument into the hidden line; it
would come back, if at all, with the next line report, which lands in FRONT of the editor's text
(`report_pending_before_editor_text`), not at the cursor: `cat ` + Alt-. would give `<arg>cat `.
Making it land at the cursor means giving the editor's text to the shell first, which is the
give-up path of other agents' files. The editor's action inserts at the cursor at once, from the
same history. Not provable safe by reading; dropped.

**Risk.** Low. One default keystroke per platform; `alt-.` was unbound everywhere (test).

## 3. #1988: Ctrl-R and the shell's reverse-i-search

**Decision.** Ctrl-R opens command search: kept. Make the shell's reverse-i-search reachable on
another key (Alt-R proposed), "a binding that sends the shell's ^R byte to the pty when a shell is
in front".

**Evidence.**

- Alt-R (`alt-r`, and `meta-r` for Option as Meta) is free in every binding table on the three
  platforms; a test now pins it.
- Where a shell without integration has the terminal (a nested shell, a plain ssh, a pane with no
  prompt hook) the terminal view has focus and Ctrl-R already reaches that shell as `0x12`: the
  command-search binding lives in the `Input` context (`terminal/input.rs` about :2623) and the
  macOS menu item is disabled when no binding matches (`app_menus.rs` `custom_action_updater`). So
  in the one state where a `^R` write is deliverable, Ctrl-R delivers it. An Alt-R binding there
  would add nothing and would take `ESC r` (readline's revert-line) from that program.
- At the app's own prompt, which is where the issue is ("while the editor owns input"), a `^R`
  write is not a fix. Reverse-i-search is a MODE of the shell's line editor: it draws
  `(reverse-i-search)` on the prompt line, which the input editor covers, and it reads every
  following key. The keys the user types next go to the editor, not the shell; the next command
  the editor writes would be read by the hidden search as search text. That breaks "keys are
  never misrouted".

**What it would take (not built).** Lend the prompt to the shell: on Alt-R with an empty editor,
make the hand-off pty-owned without a written command, hide the input box and show the shell's
prompt line, focus the terminal, write `^R`; hand back on the next Preexec (the accepted command
runs as a normal block) or, after an abort, on the line report. That is a new hand-off state
(`typeahead_handoff.rs`), a new block-list display state and changes in `input.rs`: a design
round with its own tests, not a binding.

**Done here.** No app change. The manual says where Ctrl-R is the shell's
(`docs/manual/02-terminal-basics.md`); the encoder test and the app case show `ESC r` and `0x12`
reach a program in front; Alt-R stays unbound.

**For the owner.** Either (A) the lent prompt above, or (B) Alt-R opens the app's history-only
search (`input:search_command_history`, ships unbound; one line in `custom_tag_to_keystroke`):
the same job as reverse-i-search over the same history, but the app's list, not the shell's
prompt. Not decided here.

## Tests

- `terminal/view/agent_escape_tests.rs`: the decision table and the kill switch.
- `terminal/view_test.rs`: `escape_exits_local_agent_view_without_vi_keys` (control),
  `escape_stays_in_local_agent_view_with_editor_vim_keys` (two Escapes stay; Ctrl-C twice
  leaves), `escape_with_vim_keys_keeps_its_meaning_for_a_long_running_command`.
- `util/bindings_tests.rs`: the per-platform default, the registered binding, no other claimant
  of `alt-.`, exactly the fixed fallback on `meta-.` off macOS, `alt-r` / `meta-r` unbound.
- `warp_terminal` `escape_sequences_test.rs`: `ESC .` and `ESC r` for the Meta key.
- appci `ui_readline_keys` (PROVING; Linux backends): Alt-. + Enter runs the line with the last
  word of the previous command; with a raw reader in front Alt-. / Alt-R / Ctrl-R arrive as
  `1b2e` / `1b72` / `12`.
- Not covered: the shell-vi-mode fact at view level (the session lookup is read, not run);
  #1984 in the app (needs the vim setting switched in the profile); macOS and Windows in appci.
