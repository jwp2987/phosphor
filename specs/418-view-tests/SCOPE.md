# Scope: closing the `terminal/view_tests.rs` gap (#418 and its real dependencies)

**Measured 2026-08-08** against the pinned oracle `02b53fcd8`, by appending all
17 tests #418 lists and reading what the compiler said: **69 errors across 12
tests**. Every claim below is from that compile, not from the issue text.

## Correcting #418

#418 says the 17 remaining tests are "portable today — every symbol they need
already exists in the fork" and that r4-04 simply ran out of budget.

**That is wrong.** 12 of the 17 do not compile. The issue also mis-frames the
cause: it is not one gap but five, four of which already have their own issues.

It is also less bad than a first read of the errors suggests. Two things that
look like missing subsystems are not:

- `terminal_surface_id` is a **rename**, not a model difference. The fork calls
  it `terminal_view_id`; `BlocklistAIHistoryEvent::AppendedExchange` is
  otherwise field-for-field identical to the pin. Do not "port" anything here.
- `ResponseStream` / `ResponseStreamId` **exist** (6 and 13 files). Those errors
  were missing `use` statements in the test module, nothing more.

## The 17, by what actually blocks them

### A. Compile today — 5 tests

| test | state |
|---|---|
| `ctrl_g_closes_cli_agent_rich_input_when_editor_is_focused` | ported, PASSES |
| `ctrl_g_closes_cli_agent_rich_input_from_terminal_context` | ported, PASSES |
| `ctrl_g_toggles_cli_agent_rich_input_from_terminal_context` | ported, PASSES |
| `cmd_enter_from_terminal_without_selected_block_enters_agent_view` | compiles, unverified |
| `attach_path_as_context_routes_to_open_cli_agent_rich_input` | compiles, unverified |

The three Ctrl-G tests needed only an `open_cli_agent_rich_input_for_agent_with_window_id`
helper split (the pin has the identical split). They confirm the fork's
three-way keymap predicate already matches the pin — it simply had no test
dispatching a real keystroke.

**Work: none beyond verifying the two unverified ones.** ~1h.

### B. Blocked on an existing issue — 5 tests

These are not #418 work. They unblock when their real issue lands, and belong
there as acceptance criteria.

| test | blocked on | missing |
|---|---|---|
| `cmd_enter_..._with_selected_block_enters_agent_view_with_context` | **#423** | `BlockList::transcript_scope` |
| `cli_session_status_updates_single_child_conversation_without_agent_view` | **#399** | `CLIAgentEventSource`, `CLIAgentEvent.source` |
| `paste_raw_image_clipboard_in_cli_agent_sends_correct_bytes` | **#399** | `CLIAgentSession.received_rich_notification` |
| `drag_drop_image_in_cli_agent_long_running_command_pastes_via_clipboard` | **#399** | same field |
| `updated_conversation_metadata_refreshes_selected_conversation_pane_title` | — | `terminal_surface_id` rename only; **fix in place** |

The last one is mis-grouped above on purpose: it is a one-line rename, not a
blocked test. Fix it with group A.

### C. Not portable — 1 test

`clicking_old_banner_for_open_conversation_focuses_current_terminal_surface_without_transferring_blocks`
needs `ActiveAgentViewsModel`, which this fork **deleted with the cloud
management view** (see `app/src/notifications/model.rs` and DECLINED.md). It
cannot be ported without reversing that decision.

**Action: close against DECLINED.md. Do not port.**

### D. Need a test-harness layer nobody has filed — 6 tests

47 of the 69 errors come from three conversation-transfer tests, plus the three
Cmd-K tests. They do not need new product subsystems; they need **8 test
helpers** the fork never ported:

    ai_block_count                              agent_view_entry_count_for_conversation
    command_block_count_for_conversation        append_exchange_with_inputs_and_handle_event
    exchange_with_inputs                        bootstrap_with_long_running_block
    set_active_block_agent_driving              agent_jump_user_query

plus `BlocklistAIController::register_mock_stream_for_test` and the
`RestoreConversationEntryBehavior` type.

This is the single highest-leverage item in the whole list: these helpers are
shared by the pin's view tests generally, so porting them unblocks far more than
these six. Two of the Cmd-K tests also hit an arity drift (fork method takes 4
args where the pin passes 5) that must be checked rather than papered over — it
may be a real behavioural divergence.

## Recommended order

1. **Group A + the rename** — land the 5 compiling tests. ~1h, no dependencies.
2. **Group D harness** — port the 8 helpers + mock-stream registration. This is
   the real unlock; size it before committing, but it is test-only code with no
   product risk. Estimate 1–2 days, mostly mechanical, plus investigation of the
   arity drift.
3. **#399** (`CLIAgentEventSource` + `received_rich_notification`) — 38 refs
   across 8 files at the pin, 0 in the fork. Product change. Unblocks 3 tests.
4. **#423** (`TranscriptScope`) — pin has 95 refs across 13 files; the fork's
   `AgentViewState` has 123 refs across 17. This is a semantic replacement, not
   a rename, and touches the most code of anything here. Unblocks 1 test.
5. **Close the `ActiveAgentViewsModel` test** against DECLINED.md.

Ordering matters: #423 is the largest and unblocks the fewest tests. It should
be scheduled on its own merits (unfiltered transcript scope), **not** as a way
to close #418.

## Tracker changes this implies

- **#418** — retitle to the 5 portable tests; strike "every symbol they need
  already exists" and the budget framing.
- **#399** — add the 3 tests it unblocks as acceptance criteria.
- **#423** — add the 1 test it unblocks.
- **NEW** — "port the pin's view-test harness helpers (8 fns + mock stream
  registration)", covering group D.
- **DECLINED.md** — record the `clicking_old_banner` test alongside the existing
  `ActiveAgentViewsModel` entry.
