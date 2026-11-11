# Round 18: the turn self-heal after round 14's unit run (#1967, #2103)

Fable 5.2, 2026-11-10, branch `fix/r18-agentheal` from integ/r14 42ef67839 (= build/r14a, pipeline 3052606915).
Nothing was compiled or run. Line numbers are those of the base commit. Evidence: the round 14 unit traces
(`test-linux`, `test-macos`).

## A. The paced agent fuzz fails I6 on `hop=agent_turn_stuck fault=unsettled_calls healed=1`

**Verdict: APP, a false positive of the #1967 heal (#2103). No call was lost, so the oracle is right to fail and is
not changed.**

The failing line (both paced tests on Linux; the first on macOS, with the same `unsettled_ids` hashes as Linux):

    hop=agent_turn_stuck fault=unsettled_calls healed=1 ... facts=TurnFacts { stream_in_flight: false,
    suspended_for_close: false, parked: 0, unsettled: 1, ... }

Scenario (shrunk by proptest until its time ran out): one reply with one `write_to_long_running_shell_command`
naming a command that never ran; user steps `ctrl_c, close_tab, approve, approve, edit, stop, approve`, 10 ms apart.

Replay by reading:

1. The reply ends cleanly. `AfterStreamFinished` schedules a turn check (`controller.rs:6366`, a live step, so the
   watch gets its entry) and queues the call (`:6524`). The block id does not resolve, the profile asks before a pty
   write, so the call waits on its card (`execute/shell_command.rs:400-440`). Check: the call has a status. Clear.
2. `ctrl_c` with nothing running is a stop (`actor.rs`, the view's Ctrl-C). `cancel_conversation_progress` cancels
   the pending call (`controller.rs:5897`); its cancelled result is held in `finished_action_results` to ride on the
   user's next request (#1942). Check: status `Finished`. Clear.
3. `close_tab` is `discard_pending_work_for_close` (`controller.rs:2028`), which ends in
   `clear_finished_action_results` (`action_model.rs:2100`): the results are ERASED, not moved to
   `past_action_results` as a send does (`drain_finished_action_results`, `:2034`). That is #848's intent ("nothing
   stale rides along with the user's next prompt after an undo"). The call now has no action-model status
   (`get_action_status`, `:1020`) and, never sent, no `ToolCallResult` message in the conversation.
4. `approve, approve, edit` find nothing pending. `stop` calls `cancel_conversation_progress` again, which
   schedules a check because the watch entry exists (`:5822`, `:5607`). `turn_invariant::unsettled_actions` reads
   the erased call as "nothing holds it, nothing answered it"; three hops agree; ERROR, and
   `cancel_discarded_actions` cancels the call a second time.

Why the old 2 s window did not see it: the fuzz oracle has had this exception since it got the rule.
`Oracle::tab_closed` (`oracle.rs:102-104`, `:267`): "the rig's close discards the finished results on purpose, so the
'status or result' half of I1 no longer holds". The app's check has only `suspended_for_close` (`turn_invariant.rs`,
`judge`), which is true while the pane is closed. The discard outlasts it. (The rig's close never sets that flag at
all: it calls only the discard, `runner.rs` "the rig's close only discards pending work".)

**It is not only the rig.** In the app the close sets `suspended_for_close`, discards, stops, and the suspended
`FinishedAction` branch erases the stop's cancelled results too (`controller.rs:1351-1361`); no check runs while
suspended (`:5604`). After the undo (`resume_after_close_undone`, `view.rs:10413`) the flag is off, the watch entry
from before the close is still there, and the conversation's last reply still has calls with no status and no
result. The next `cancel_conversation_progress` schedules a check: the user's Stop, or the superseding cancel of
their next message (`cancel_active_conversation_for_follow_up`, `:3212`) when that message's send waits on an
attachment read and no new exchange exists for three hops. The heal then logs the ERROR and puts cancelled results
back for calls whose results the close removed so that nothing stale goes out. With the conversation `Cancelled`
the reason is the stop's, so no request follows; the harm is the false ERROR (the app cases count them) and results
the close had removed riding on the next prompt. By reading, not run.

**Fix.** The controller remembers, per conversation, the reply (last exchange id) a close stopped
(`replies_discarded_for_close`), noted at both places the close erases results (`discard_pending_work_for_close`,
the suspended `FinishedAction` branch). `TurnFacts::reply_discarded_for_close` is true while that reply is still the
conversation's last one, and `judge` then reports no unsettled calls. Parked actions are judged as before. The entry
goes at the first check that finds a newer exchange (or no conversation). It is the oracle's exception, held per
reply instead of for the rest of the conversation.

**Rejected.**
- Allowing the ERROR in the oracle (`error_allowance`), or a `tab_closed` exemption in `check_logs`: the line is a
  false alarm in the app; hiding it in the test leaves it in the app.
- Making the close retire the results into `past_action_results` instead of erasing them (three lines, and the
  calls would keep their `Finished` status, which also answers the oracle's own reading). It changes what every
  card of a restored pane renders (`get_action_status` drives the cards: `block/view_impl/output.rs`; with "hide
  completed tool cards" on, finished cards of a restored pane would disappear), and that cannot be looked at
  without a build. Probably the better end state: see the owner note in the report.
- Ending the watch at the close (as a revert does): the next live step in the same reply (the rig approves a card
  after its close) would start it again and judge the same erased calls.
- Setting `suspended_for_close` in the rig's close: it would make the rig pass and leave the app's after-undo case.

**Risk.** Low. One map, read in one place; the only behaviour change is that a reply a close stopped is no longer
healed for unsettled calls. A real lost call in such a reply would go unhealed, in a conversation the close already
ended `Cancelled`; the user's next message starts a new reply, which is judged again.

## B. Four question view tests fail their "the question waits" precondition

`a_question_shows_its_card_with_no_approval_step_and_a_skip_resolves_it_once`,
`autonomy_max_still_waits_for_the_answer`, `cancelling_a_question_resolves_it_once` (`view_test.rs:33735`) and
`a_question_finishes_only_with_its_cards_answer` (`:33599`).

**Verdict: TEST fault (fixture), caused by #1967's `watch_blocked_question_card` (commit 0d82f9df6). The app does
what #1967 designed.** Not #2029, #2034, #2054 or step 8.

Evidence: each of the four traces has, just before the panic,

    [ERROR] [warp::terminal::view] hop=agent_turn_stuck fault=blocked_question_without_card healed=1 ... id=question

The fixture (`terminal_asking_questions` + `queue_actions_for_test`, from #1797, 57d142aa5) queues an
`ask_user_question` straight into the action model of a pane that has no AI block. The card the tests then use is a
detached `AskUserQuestionView` they build themselves. From the pane's side that is exactly the #1967 fault: a
question blocked at the front of the active conversation with no question card in any AI block of the pane
(`view.rs:12656-12681`). `ActionBlockedOnUserConfirmation` starts the watch (`view.rs:9034`), its three hops are
ready futures that run before the test task is polled again, and the question is cancelled as a stop. So
`pump_until` never sees `Blocked`. `a_question_with_nothing_to_show_resolves_with_an_error` passes because that
question never blocks.

In the app the state cannot be reached by a healthy turn: the only place a question card is made is the reply's
stream update (`block.rs:2294`, `:3880`), which runs when the tool call arrives, before the reply ends and its
calls are queued. The agent fuzz question scenarios (real streams, real blocks) passed in the same run with no such
line.

**Fix (tests only).** The fixture gives the pane what the stream gives it: an AI block of the conversation holding
the question's card (`mount_question_card_in_pane`, through the test-only
`AIBlock::show_question_card_for_test`, which calls the stream's own handler). No assertion of the four tests is
changed or removed. Added: `queue_one_question` now also asserts the question is still blocked after the watch's
hops (a question with its card up is not healed), and the state the old fixture built by accident is a test of its
own, `a_waiting_question_with_no_card_in_the_pane_is_stopped` (the pane's check had none).

**Rejected.** Turning the heal off for these tests (the kill switch is read once per process and would hide the
check); queueing the question in a conversation that is not the pane's active one (it dodges the check instead of
meeting its precondition).

**Risk.** The four tests now have a real AI block in the pane; nothing was run. If the block changes what they
observe, the next run shows it in these four tests only.

## C. Files not touched

`app/src/terminal/input.rs`, `input/enter_queue.rs`, `view/give_up_to_pty.rs`. No timer was added.
