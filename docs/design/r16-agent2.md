# Round 16, agent 2: emitted order for a repeated call id (#2054), completion-result notifies (#1995), suggestions in ssh, unreadable secure storage

Branch `fix/r16-agent2` (from integ/r14 fae4cecfd). Owner decisions 2026-11-11 (item 21 of the
decision list). Nothing here was compiled or run (rustfmt only).

## 1. #2054: a repeated call id is sent back where it was emitted

### Cause (by reading, `app/src/ai/agent_providers/chat_stream.rs`)

* A finished reply's calls are taken from the `End`'s captured content, where a repeated id is
  renamed `id#2` (`unique_call_ids`). The renamed call gets its message only then, after the
  messages of the calls whose cards the stream had already shown.
* So reply `[x, x, y]` (y had its card) is recorded `[x, y, x#2]`. The request builder
  (`AssistantBuffer::flush_into_with_group`) moved every `id#n` next to `id`
  (`repeats_beside_their_originals`, #1819) to undo that.
* Reply `[x, y, x]` is recorded `[x, y, x#2]` as well, and the same move sent it back as
  `[x, x#2, y]`: not the order the model emitted, and not what I4's `uniquify` expects. The
  two cases are the same stored messages; the builder cannot tell them apart.

### Fix

Make the recorded order the emitted order, so the builder has nothing to undo.

* The stream loop sets `repeat_may_be_pending` at the first chunk that may be another call
  under an id that is already buffered (`may_repeat_call_id`): a chunk that does not carry on
  the buffered call (`continues_stream_call`, #2048), or one equal to a buffered call that is
  whole (its card is shown, or its arguments are complete JSON). From then on no NEW early card
  is shown; those calls get their messages at the commit, in captured order, after the repeat.
  This is the wait a call behind a `websearch` or a call that failed its first parse already
  has (`tool_order ... all(has a card)`).
* The builder sends the calls as recorded (`request_call_order`).
* Kill switch `PHOSPHOR_AGENT_REPEAT_ID_EMITTED_ORDER=0` (`off`, `false`, `no`): no card is
  held and the builder moves the repeat as before.

Why recorded = emitted then: cards are a leading run of the stream-time order (existing rule).
A renamed repeat never has a card. It sits before a carded call in the captured order only if
that call got its card after the repeat's chunk arrived, which the hold rules out.

### Rejected

* **A marker on the late message that says where it was emitted** (in `server_message_data`,
  or in the message id). `server_message_data` is already the carrier format of a call that
  failed to parse (`<name>\n<args>`), which a repeat can be; every branch of the commit loop
  would have to write it; readers in the controller, the converter and compaction look at the
  field.
* **Swap at the `End`** (put the repeat into the later call's shown card and append that
  call). A card would change the call it stands for under the user's eyes.
* **Only hold after a DETECTED other call, keep or drop the move for the rest.** A repeat that
  equals its original (`dup_call_ids`: grep, the same grep, file_glob) cannot be told from a
  chunk sent twice, and either choice lists one of `[x, x, y]` / `[x, y, x]` out of order.
* **State the builder's rule in I4.** The owner decided on the emitted order.

### Risk

* A false "possible repeat" costs the early cards of the calls after it in that reply (they
  appear when the reply ends). By reading genai's streamers, every adapter emits one chunk per
  delta and none re-sends a finished call, so an equal chunk needs an empty delta. The one
  adapter that sends one for every call is Anthropic's (`""` at `content_block_start`, `""`
  again as the first `input_json_delta`; Vertex uses it for Claude). A tool that takes empty
  arguments (every MCP tool, `mcp.rs:573`; the transfer and new-conversation markers) shows
  its card for `""`, so that pair would have held the cards of every later call of the reply.
  `adapter_opens_calls_with_an_empty_chunk(api_type)` exempts exactly that pair on those two
  api types. What is given up: on Anthropic or Vertex, a reply that repeats an id with two
  empty-argument calls and then has a call with an early card is still listed `[x, y, x#2]`
  (Anthropic's ids are unique per call, so this needs a proxy that rewrites them).
* A reply that breaks after a possible repeat shows fewer early cards than before (I4 accepts
  any leading part of the early list).
* A conversation saved before this change with a late-RECORDED repeat (`[x, y, x#2]` stored
  for `[x, x, y]`) is now sent as stored. Every call still has its one result.
* One comparison of the buffered arguments per repeated-id chunk, and one JSON validity scan
  when they are equal. Not measured.

### Tests

Unit (`chat_stream::duplicate_call_id_tests`):
`a_chunk_that_may_repeat_its_id_is_told_from_a_growing_call`,
`a_repeat_that_is_not_next_to_its_original_keeps_its_emitted_place`,
`the_request_lists_a_late_repeat_where_it_was_emitted`,
`the_emitted_order_kill_switch_reads_the_usual_off_words`;
`oracle::tests::a_repeat_stays_where_it_was_emitted`.
Corpus: `i4_repeat_id_emitted_late_same_call`, `i4_repeat_id_emitted_late_other_tool`,
`i4_repeat_id_adjacent_then_later_calls`; `dup_call_ids` unchanged.

Not a pin divergence: the BYOP stream is this fork's code, no DECLINED.md row.

## 2. #1995: a completion result notifies `Input` only when render-read state changed

### Cause (named in `docs/design/r14-echoframe.md` section 5 from the round 12 logs)

`Input::handle_completion_suggestions_results` (`app/src/terminal/input.rs`) ended in an
unconditional `ctx.notify()` (cause `handle_completion_suggestions_results_3`). With
completions as you type it runs for every typed character whose result answers the current
buffer. When the menu is closed and stays closed, `set_mode(Closed)` returns early, nothing
else is notified and `Input` is re-rendered alone beside the editor: 16 % (macOS) to 31 %
(Wayland) of terminal-mode key frames.

### Fix (owner decision 2026-11-11: yes, change input.rs)

What the handler can change that `render` or its children read:

| outcome | changes | notify |
|---|---|---|
| result for another buffer | nothing (early return, as before) | no |
| no result / no suggestions, menu closed, not stale, as you type | nothing | **no (new)** |
| no result / no suggestions, a menu up | the menu closes | yes |
| no result / no suggestions, stale flag set | the flag (accept gating) | yes |
| no result / no suggestions, answers a Tab | maybe nothing | yes, kept (once per Tab) |
| suggestions (open, insert, insert prefix + open, no action) | menu, buffer, prepared completions | yes, as before |

`completion_result_needs_render(answers_a_tab, menu_was_up, menu_was_stale, skip_redundant)` is
the decision; `handle_completion_suggestions_results` now returns whether it notified (its one
caller ignores it; the tests read it). Kill switch
`PHOSPHOR_SKIP_REDUNDANT_COMPLETION_NOTIFY=0` (read once).

`completion_menu_stale` is not read by `render` today (it gates Tab/Enter/click accepts), but
the task names it and a set flag implies a menu that was up, so it is kept as a reason.

### Rejected

* **Drop the final notify altogether and rely on `ModeChanged` (`new_10`).** The open branch
  also sets the prepared completions and can clear the editor ghost; nothing proves by
  reading that every such path emits a mode change (an open menu refilled with an equal mode
  does not).
* **Skip the notify for an answered Tab too.** Not per-key, so nothing is gained, and a Tab is
  where a missed frame would be seen.
* **Observe the notify through the window invalidation in the test.** A notify is an effect
  flushed at the end of the outermost update, where the unit-test window builds its scene and
  consumes the invalidation; the return value is exact.

### Risk

A view state this reading missed that only this notify drew. The cases kept (menu up, stale,
Tab, any suggestions) are every branch that writes state; the skipped case writes nothing
(`set_mode` on an equal mode returns before any write). Kill switch if a run shows otherwise.

### Proof a run should give

`latency_key_log`: `input_notify_causes` no longer led by
`Input:handle_completion_suggestions_results_3`; `hop=key_frame_build views=1` for a character
typed into a non-empty buffer in terminal mode.

## 3. #2094: next-command and prompt suggestions in ssh sessions follow the #1982 rule

### Today (by reading)

* **Next command** (`app/src/ai/predict/next_command_model.rs`,
  `generate_next_command_suggestion_with_prefix`): after a block completes and while a command
  is typed, when history is not confident enough, `byop_generate_input_suggestions` sends
  recent commands, similar history and the typed prefix to the model. The config is resolved
  with no pane (`byop_next_command::resolve(ctx, None)`); nothing looks at the session.
* **Prompt suggestions** (`app/src/ai/blocklist/passive_suggestions/legacy.rs` ->
  `active_ai::prompt_suggestions::dispatch`): after a block completes, the finished command
  and a summary of its output go to the model. Nothing looks at the session.

#1982 left both out on purpose (`r14-secure.md`, Risk). Owner decision 2026-11-11: gate them
the same way.

### Fix

* The rule moves from `nld_predict` to the `active_ai` module so the three features share it:
  `allowed_for_session(in_ssh_session: Option<bool>, allowed_in_ssh)` (unknown = ssh),
  `is_ssh_session`, `pane_is_in_ssh_session`, `allowed_for_pane`. `nld_predict::dispatch`
  behaves as before.
* Two settings in the same family, default false, not synced, a switch under each parent on
  the AI settings page (greyed while the parent is off), strings in en, ja, zh-CN:
  `agents.warp_agent.active_ai.intelligent_autosuggestions_in_ssh_enabled`,
  `agents.warp_agent.active_ai.prompt_suggestions_in_ssh_enabled`. They are the kill
  switches: on restores today's behaviour.
* Prompt suggestions: the gate is at the top of `prompt_suggestions::dispatch` (the one place
  the request is built; it has the pane id).
* Next command: `next_command::resolve_for_session(app, None, in_ssh_session)` returns no
  config when not allowed, and no config is already the silent no-op of the model call. The
  session is the one the suggestion is for: `completer_data.active_block_session_id()` looked
  up in the model's `Sessions`. History-based and completer suggestions run as before (they
  send nothing).

### Rejected

* **One setting for both.** #1982 has its own; a user may want next commands on a trusted
  host and no output summaries sent.
* **Gate the passive-suggestions model at its trigger** (skip the whole
  `maybe_generate_prompt_suggestions`). The static suggestions are local and stay; only the
  request is the concern.
* **Treat an unknown session as local for next command** (as completions-as-you-type does,
  `is_completions_while_typing_turned_on`). The decision says unknown = ssh: not knowing must
  not send.

### Risk

* A local pane whose active block has no session id yet (the first prompt before bootstrap
  finishes) gets no model next command until it has one. History suggestions still show.
* Not compiled. `self.sessions.read(ctx, |sessions, _| sessions.get(id))` and the new settings
  macros are the compile doubts.
* Not gated here, same class, for the owner to rule on: passive code diffs
  (`maybe_generate_passive_code_diff`, `passive_suggestions/maa.rs`), which a static prompt
  suggestion can still start in an ssh session; command explanation and commit messages are
  user-triggered.

### Tests

Unit: `active_ai/mod_test.rs` `every_active_ai_request_shares_the_ssh_rule`;
`settings/ai_tests.rs` `next_command_and_prompt_suggestions_in_ssh_default_off`.
App: `lab_agent_suggestions_ssh.bash.lan.nohelper` (agent mode, fake model, `PROVING_LAB`):
per-purpose local control, then silence on the ssh pane.

## 4. #2095: an unreadable fallback store is told to the user; release-notes line for #1932

### Today (by reading, `crates/warpui_extras/src/secure_storage/linux.rs`)

* `read_fallback_value` returns an error that is not `NotFound` for a blob that does not
  decrypt (lost key file, `K` blob with the keyring away, damage, an unknown header). But
  `fallback_entry` logs a warning and answers `Absent`, and `read_value` then asks the keyring
  and maps every failure to `NotFound`. Every caller sees "never saved".
* A blob that exists but cannot be OPENED or READ (permission denied, I/O error) was mapped to
  `NotFound` at the first line (`File::open(..).map_err(|_| NotFound)`), so it was not even
  logged.

### Fix

* `secure_storage::unreadable` (new, all platforms, no dependencies beyond the crate's
  `async-channel`): a process-wide count with `record()` (true and a message to subscribers for
  the first of the run), `seen()`, `count()`, `subscribe()`. It holds a number: no key, no path,
  no value can reach the notice through it.
* `fallback_entry` calls `record()` in its unreadable branch. The read result is unchanged
  (the keyring copy if there is one, else `NotFound`): callers and their error handling are not
  touched, the blob stays in place.
* `read_fallback_value`: only `ErrorKind::NotFound` / `NotADirectory` on open means "no blob"
  (`fallback_blob_is_absent`); any other open or read failure is an error naming the file and
  the error kind.
* App: `app/src/secure_storage_notice.rs`, singleton `UnreadableSecureStorageNotice`, the twin
  of `KeyringNotice` (#1805): subscribes, waits for a window through the window manager's
  events (no timer), one toast (`common-secure-storage-unreadable`, en / ja / zh-CN, 15 s),
  counted as shown only when a toast stack confirms it. Once per launch (`OncePerLaunch`).
  The text is a fixed sentence with no arguments.
* Manual (`05-ai-agent.md`, "Where credentials are stored"): the notice, and the downgrade
  limit.
* Release notes: there is no drafting file in the repo (the release body is `git-cliff` output,
  `.gitlab-ci.yml` `release-meta`; known issues have been added by hand at release time), so
  `docs/release-notes-draft.md` is new, with the downgrade line under v0.1.18 and one line each
  for this notice and the #2094 switches.

### Rejected

* **Return a distinct error (`Error::Unreadable`) from `read_value`.** Every caller matches on
  `NotFound` to mean "nothing stored, carry on" (provider secrets, MCP tokens, the proxy
  password); a new variant would turn start-up reads into hard errors in code this task does
  not own. The notice needs the fact, not a new control path.
* **Name the affected credential in the notice.** The key names are internal
  (`AgentProviderSecrets`), and the rule is no secret text; the log has the key and the cause.
* **Offer "delete and start over" in the toast.** The blob reads again when its key is back;
  deleting is the one thing that makes the loss permanent.
* **Record only when the keyring had no copy either.** The fallback holds a value only when
  the keyring could not take it, so a keyring copy is older: the user should still hear that
  the newer value could not be opened ("missing or out of date").
* **A kill switch.** The change adds a toast and a log line; no read returns something else
  than before, except a blob the process cannot open, which was `NotFound` at the fallback
  layer and is still `NotFound` from `read_value`.

### Risk

* With the keyring down and a `K` blob present, two notices appear in one launch (keyring
  unavailable; stored credentials could not be opened). Both are true; they have different ids.
* Windows and macOS never record (their stores have no such fallback); the model is idle there.
* Not compiled: `spawn_stream_local` with a `()` item, `ErrorKind::NotADirectory`, the
  `super::super::unreadable` paths in `linux_test.rs`.
