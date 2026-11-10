# r10 focus-gap-reorder: a typed line split between the editor and the shell, merged in the wrong order (#2012)

Round 9 Windows (pipeline 3047937983), PowerShell pane, terminal and agent mode:
`giveup_keys_reach_shell` 2/20. Typed `Add-Content gup_T249_6wm1.txt t252_dxfpnf`, the editor
submitted `d-AdContent gup_T249_6wm1.txt t252_dxfpnf`. Nothing here was compiled or run.

## Verdict

APP bug. Not a focus gap (#1552): every key of the slice has `gap_leaf=-`, and focus did not move
at the block completion. It moved 7 ms earlier, at a **Preexec the shell sent for an empty line**.

## Cause (line numbers are the tree before this change)

The case types `line`, Enter, a bare Enter, the next line. Per line, in the app log:

1. The bare Enter is written to the pty as typeahead while the line's command runs (correct: a
   plain terminal does the same). The hand-off now counts `enters=2`.
2. The command's Precmd is counted (`precmds=1`), then PowerShell's `LineEditorReading` marker
   arrives. `on_line_editor_active` asks `counts_provably_stale`
   (`model/typeahead_handoff.rs:3127`, proof at `:4039-4067`): on ConPTY nothing samples the shell,
   so the marker alone is the proof that "the lines the counts still queue were consumed by the
   command that ended". Log: `handoff_stale_counts_reset by=line_editor_reading_drained enters=2
   precmds=1`, then `handoff_decision decision=Committed`. **The proof is wrong here**: the wrapper
   sends the marker BEFORE PSReadLine reads (`pwsh.ps1:1432-1435`), so an Enter typed ahead during a
   command that reads nothing is still in the console's input when the marker is sent. The editor
   is given input with a line still queued in the shell.
3. The editor takes `A`, `d` (seq 570/571).
4. PSReadLine reads the queued `\r`. PowerShell's wrapper calls `Warp-Preexec "$line"` for EVERY
   line it returns, the empty one too (`pwsh.ps1:1442`). `apply_preexec` calls it shell-started
   (`terminal_model.rs:3516-3527`: a block nobody started), and `on_preexec(true)` in
   `EditorOwned` hands input to the pty (`typeahead_handoff.rs:2968-2975`: `PtyOwned { enters 1,
   precmds 0 }`). Nothing looks at what the editor holds.
5. The view's Preexec handler (`view.rs:20393-20399`) calls
   `focus_terminal_if_typeahead_handoff_wants_it` (`view.rs:15148`); `wants_terminal_focus` is
   `routes_keys_to_pty` (`typeahead_handoff.rs:2081`, `:2132`), so focus goes to the terminal view
   (`kh_focus_terminal_view seq=571`). `d` and `-` (572/573) are decided by row D6
   (`input_queue/table.rs:803-847`; `pty_typed` at `:468` is true because the hand-off routes) and
   written to the pty. The line pin that keeps a line in the editor (`apply_pin`, `table.rs:1059`)
   is not valid while the hand-off routes to the pty, and live decisions pass no pin anyway.
6. The empty command's Precmd and marker come; the decision is `SendRequest`; focus returns to the
   editor, which takes `C`, `o` (`AdCo`).
7. The report `d-` arrives. `insert_typeahead_from_handoff` (`view.rs:15558`) gets
   `(typeahead, 0)` from `advance_typeahead` (`model/early_output.rs:226`) and
   `Input::insert_handoff_typeahead_text` -> `replace_typeahead_prefix` (`input.rs:8921`, `:9019`)
   replaces the first 0 characters: the report always goes to the FRONT. That rule assumes the
   editor holds only text typed after the request. Here it held `Ad` from before the pty episode.
   `handoff_insert replaced_prefix_chars=0 report="d-" before="AdCo" after="d-AdCo"`.

Two defects: (A) step 2, the editor is handed input while a typed-ahead Enter is unread; (B)
steps 4-7, once the shell starts a command by itself while the editor holds unsent text, the
report of the keys typed meanwhile is put in front of that text.

## Why macOS, Linux and Git Bash do not show it

zsh, bash and fish send no Preexec for an empty line (only a Precmd), so step 4 never happens: the
hand-off stays editor-owned and every key stays in the editor. Git Bash is bash, and the marker
proof of step 2 is PowerShell-only (`pty_controller.rs:1020-1031`), so its hand-off stays
pty-owned until the second Precmd and the keys travel together through the shell line. The
`NOREF` rows of Git Bash are not this defect. (B) is platform-neutral in code: any shell-started
Preexec while the editor holds text does it (a non-empty typeahead line behind a wrong stale-count
commit); on Unix that needs the probe to sample the shell inside the few microseconds between zle
starting to read and reading the queued line.

## Fix (B): the report goes where its keys were typed

- `TypeaheadHandoff::on_preexec` records the transition "the shell started a command while the
  editor owned input" (`shell_took_input_from_editor`, taken once by the view).
- The view's Preexec handler takes it BEFORE focus moves and tells the input to remember the text
  in front of the editor's cursor (`Input::anchor_handoff_report_at_cursor`; nothing is recorded
  for an empty prefix, a selection or several cursors).
- `insert_handoff_typeahead_text` places a fresh report behind that prefix when the buffer still
  starts with it, and a later report of the same line replaces the earlier one at the same place
  (the #1569 "must extend" check is made at that place). The cursor keeps its place in the text
  typed after the report. Otherwise (no anchor, or the prefix was edited) the report goes to the
  front exactly as before, and the miss is logged.
- The anchor ends on events only: used by a report; the editor empties; the editor owns input
  again with no report on its way (`handle_typeahead_handoff_changed`).

Routing is unchanged: the keys still go to the pty while the shell-started command runs, exactly
as before, so a program started from typeahead still gets its keys, Enter and control keys take
the hand-off's existing paths, and no timer is added. With the evidence sequence: `Ad` | `d-` to
the pty | `Co` | report `d-` -> `Add-Co`.

## Rejected

- **Keep the keys in the editor (`keys_follow_editor`, #1658) at a shell-started Preexec.** It is
  ten lines, but while the block is long-running the input box is hidden and `pty_control` still
  sends Enter and control keys to the pty (`table.rs:482-493`): typed text would go to a hidden
  editor and its Enter to the shell. On ConPTY nothing samples whether the command reads the tty,
  so a REPL started from typeahead (`python`, Enter, then typing) would have its text swallowed
  until a click or Ctrl-C. #1658 only ever sets the fact while no command runs; setting it under a
  running block is a new, unproven combination. Not provable by reading, so dropped.
- **Fix (A) in this change.** Removing the marker proof brings back the wedge #1614 fixed (after
  a native program consumed an Enter the counts never agree again and the editor never gets input
  back). The sound proof needs a fact from the shell (for example the wrapper reporting whether
  console input is pending when it sends the marker), which is a bootstrap protocol change that
  cannot be tried here. Filed as #2018 for a decision; with (B) in place a wrong commit no
  longer reorders the line.
- **Insert at the cursor the editor has when the report arrives.** Wrong: keys typed in the
  editor after focus returned (`Co`) were typed after the pty keys.

## Risk

- The anchor is created only at the one transition above; every other report is inserted by the
  unchanged code path.
- An anchor that outlives its report (a program consumed the keys) is dropped when the editor is
  the owner again with no report pending, and is checked against the buffer before use.
- Not covered: an Enter typed inside the ~40 ms pty window is still handled by the young-Enter
  rules (held and run with the merged line, or written as typeahead when the hold's evidence says
  so). That is the hand-off's behaviour for any typeahead Enter and is not changed here.
- Defect (A) stays (#2018): a key typed in the few ms between the wrong commit and the empty line's
  Preexec still takes the editor/pty/editor path; (B) makes that path keep typed order.
