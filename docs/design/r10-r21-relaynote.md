# Round 21, task `r21-relaynote`: a Phosphorize click behind a relay says what it waits for (#2142)

Owner decision 2026-11-11 on #2142: option 2. The rules stay; the person is told. Nothing here was compiled or run.

## What happens today (cause, with evidence)

- `app/src/terminal/view/subshell_ready.rs`, `subshell_ready()`, arm `SubshellTtyKind::Relay`: with the local tty raw and no prompt
  drawn since the arming the answer is `Decision::Wait(WaitWhy::RelayNoPrompt)` (R4: the relay hides the inner shell's tty, a prompt
  already on screen proves nothing).
- `on_user_line_ended()` (same file): a non-empty line at a shell prompt is `LineEndVerdict::Cancel` (R2); the view's key path
  (`TerminalView::note_user_bytes_for_subshell_readiness`, `view.rs`) sets the state Idle and logs `subshell_setup_cancelled
  why=UserDrove`. It has no `ViewContext`, so it could show nothing; `cancel_subshell_setup` is silent for `UserDrove` too.
- `arm_subshell_setup_from_banner` (`view.rs`) removes the Phosphorize button (`dismiss_warpify_banner`: the in-block banner and, with
  `WarpifyFooter`, the footer button) and, while the attempt is armed, shows the 4 s toast "Phosphor setup starts when the shell shows
  its prompt." Behind a relay the prompt IS on screen, so that sentence tells the person nothing they can act on (#2142 calls it
  "shows nothing"; round 15 log: `subshell_setup_armed`, `Wait(RelayNoPrompt)`, 79 `Wait(LineDirty)`, `verdict=Cancel`).
- After the cancel no button is on screen and Ctrl-I has no context (`terminal:warpify_subshell` needs `SubshellBanner`, which is the
  banner or the footer's mode): there is no way back short of leaving the subshell and starting it again.

## Chosen fix

`app/src/terminal/view/subshell_relay_note.rs` (pure: texts, two rules, the switch) and the wiring in `view.rs`.

1. **Wait note.** `arm_subshell_setup_from_banner`: when the look at the arming said `Wait(RelayNoPrompt)` (new field
   `ArmedSubshellSetup::last_wait`, set by the judge's look) the toast is "Phosphor will set up this shell at the next empty prompt:
   press Enter on an empty line. Running a command first cancels the setup." instead of the old sentence. It is a persistent toast with
   an object id (the form of the #1014 "setup is waiting" notice): a state, shown until `write_armed_subshell_setup` (written),
   `cancel_subshell_setup` (ceiling, block gone, unsupported) or the key path's `UserDrove` cancel removes it. A second click while
   it shows adds nothing. Every other wait keeps the old toast (an Enter on an empty line does not end a secret read or vim).
2. **Cancel note.** `note_user_bytes_for_subshell_readiness` returns the facts of a `UserDrove` cancel (`SubshellUserDroveCancel`);
   its caller (`write_user_bytes_to_pty_after_paste_stage`, which has the context) calls `on_subshell_setup_user_drove`: the wait note
   goes, and for a banner attempt that waited behind a relay (the wait note showed, or the tty fact at the Enter was `Relay`) an
   ordinary toast says "Phosphor setup cancelled: you ran a command before it started. To try again, click Phosphorize subshell or
   press <key>." Only toast events are emitted there: no model lock, nothing written, the key goes on to the pty as before.
3. **The way back.** The note may only name a retry that exists, so the button is offered again: the command it was offered for is
   kept with the attempt (`banner_command`, read at the click before the button is removed), and the first look of the pane after the
   cancel (`judge_subshell_setup_on_event`: the output event the user's own Enter causes) calls `reoffer_subshell_setup_if_pending`:
   the footer button (`WarpifyFooter`, the default) or the in-block banner, for the same command, only while the block the attempt was
   for is still the running one and no new attempt is under way. Not on the key path: offering it takes the model lock. When the
   command is not known the note says "start the subshell again" and names no button.
4. **Kill switch** `PHOSPHOR_SUBSHELL_RELAY_NOTE=0` (`off`, `false`, `no`): the old toast, the silent cancel, no button offered again.
5. **Trace.** `hop=subshell_relay_note note=armed_wait|cancelled event=shown|gone ...`, `hop=subshell_setup_reoffered offered=<bool>`.

No timer of the input path is added (`script/check_input_path_timers` unchanged; the cancel note's 4 s is the toast stack's own display
time, as for every other setup toast). Nothing is typed for the user. Nothing is read off the screen: both notes follow the judge's
decision and the tty fact at the key.

## Rejected

- Treating the prompt on screen as proof behind a relay (option 3 of #2142): the owner kept R4.
- Writing the setup at the user's command's own prompt instead of cancelling: changes R2; not asked for.
- Pressing Enter for the user after the click: typed for the user; an Enter into an unknown reader behind a relay.
- A modal or an in-block banner for the wait: the decision says a short non-modal note in the existing style.
- An ordinary 4 s toast for the wait note: the wait lasts up to 60 s and the instruction is the only way forward; a longer display
  time would be a new duration. A persistent toast needs none and is removed by the events that end the wait.
- Showing the cancel note without offering the button again: "try again" with nothing to click.
- Offering the button on the key path: it locks the model (`show_warpify_footer` / `show_warpify_banner`); the key path does not.
- The cancel note for auto-setup and rc-file attempts, or for a click that never waited behind a relay: out of #2142's scope (nobody
  clicked, or the wait was another one); they stay silent as before.

## Risk

- Low for keys: the key path gains one `Option` return and toast events; no lock, no write, no hold. The judge is untouched.
- The wait note is a persistent toast: if the pane is closed while an attempt is armed, nothing removes it (no event reaches a
  dropped view); the person closes it with its X. The #1014 "setup is waiting" toast has the same property.
- The button is offered again only after an output event; a relay that prints nothing after the user's Enter delays it until it does.
- A re-armed attempt after an E-SECRET hand-back no longer knows the banner's command: its cancel note says "start the subshell
  again".
- Not compiled. The view tests rest on the existing fake relay probe (`sudo_relay`) and on `FeatureFlag::WarpifyFooter` overridden on.
