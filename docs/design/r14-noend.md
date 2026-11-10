# Round 14: a reply with no End runs nothing (#2048), and two appci cases that never ran (#1340, #1985)

Branch `fix/r14-noend` (from integ/r14 4bf571aae). Nothing here was compiled or run (rustfmt and
the appci self-test only).

## A. #2048: a reply whose body closes with no `End`

Owner decision 2026-11-10: such a reply must NOT run its in-stream tools.

### Cause (by reading, `app/src/ai/agent_providers/chat_stream.rs`, `generate_byop_output`)

* The stream loop is left the same way by an `End` and by a body that just closes (the stream
  yields `None`, `break`). The commit after the loop goes through every buffered call; the
  interceptors run the tool right there: `todowrite` yields the todo update, `websearch` /
  `webfetch` are awaited (`dispatch_byop_web_tool(..).await`), `search_codebase` is answered,
  `get_relevant_files` awaits a model call of its own. Only after the single `final_messages`
  yield does the `end_count == 0` guard report the reply as incomplete (audit:
  `docs/design/r13-fuzz2.md`, table row "closed with no End").
* Chunks are buffered by call id (`tool_bufs.insert(call_id, call)`): a second call with the id
  of an earlier one replaces it. With an `End` the calls are taken from the captured content,
  where the repeat is renamed `id#2` (`unique_call_ids`). With no `End` there is no captured
  content, the first call is gone and nothing tells the model.

### Fix

1. `reply_incomplete = end_count == 0`, read in the commit loop. A call to a tool the stream
   runs by itself (`runs_inside_the_stream`: todowrite, webfetch, websearch, search_codebase,
   get_relevant_files) is answered with a carrier call and a result that says so
   (`incomplete_reply_result_payload`: `status: not_executed`, `error: reply_incomplete`,
   `executed: false`, a hint) and is not run or awaited. The check sits after the dispatch gate,
   so a tool the request did not offer keeps its own refusal. No loading card, no todo update.
   The owner named websearch, webfetch and todowrite; the two codebase tools are held back by
   the same rule because they are run by the same commit (one of them awaits a model call), so
   the rule is "the stream runs nothing for a reply it is about to call incomplete". Say so if
   the two read-only ones should run after all: it is one line in `runs_inside_the_stream`.
2. The stream loop notes when a chunk is NOT a later chunk of the call buffered under its id
   (`continues_stream_call`: genai sends a call's chunks with the arguments accumulated so far,
   so the same call has the same tool name and arguments that start with the earlier ones)
   and keeps the replaced call's tool name (`replaced_calls`). With an `End` the note is not
   read. With no `End` the call that kept the id is answered by the stream, whatever its tool,
   with `reused_call_id_result_payload`: `error: duplicate_tool_call_id`, the names of the
   replaced calls, "none of them ran, make each call again, one id per call". A card shown
   early under the id becomes the carrier, as for a call whose final parse fails.

What does not change: text and calls of the reply are kept; its commands, reads and edits are
still never queued (only a finished exchange queues actions) and are answered as cancelled on
the resume; the error text, the ERROR log line and the automatic resume.

### The fuzz oracle (I4)

No rule changes. The table in `oracle::check_tool_messages` and `r13-fuzz2.md` stands as it is:
a reply that closed with no `End` and was not cut is in later requests whole, one call per id,
in place. The calls and their ids are the same as before; only their results differ (an in-stream
tool: "not executed" instead of the tool's output; a reused id: the note). `merged_by_id` in the
player is still what the app keeps. Wording only: the `ReplyEnd::ClosedEarly` doc, the table
cell ("the SAME commit" is now "the commit of every call, none run by the stream"), the
`reply_cut` doc (no search keeps such a reply's stream open any more) and the comment on
`a_reply_that_closed_with_no_end_is_committed_whole`.

I6: no new ERROR line (the two new lines are WARN).

### Rejected

* **Rename the repeat `id#2` on a body with no `End`, as an `End` does.** The request builder
  moves every `id#n` call next to `id` (`repeats_beside_their_originals`), so a repeat that is
  not next to its original (`[x, search, x]`) is listed `[x, x#2, search]`, and I4 expects the
  order the model emitted. That is true today for a FINISHED reply with such a repeat (by
  reading; filed, see below); renaming here would have put the existing corpus scenario
  `i4_reply_closed_without_end_is_committed` and the random proptests onto that path. Telling
  the model keeps one call per id and changes no I4 rule.
* **Leave the held-back calls unanswered, for the resume's cancelled placeholder.** It says
  `interrupted_by_user`, which is false here, and it cannot carry the reused-id note.
* **Do not commit a reply with no `End` at all.** Rejected in round 13: it throws away text and
  calls the user has seen.
* **Tell a repeated id apart from a re-sent chunk when both are identical** (same id, name and
  arguments). Not possible from the chunks (an empty argument delta re-sends the same chunk).
  Such a repeat stays one call; nothing of it is lost.

### Risk

* A false "another call" (a provider whose later chunk does not start with the earlier
  arguments) only matters on a body with no `End`, where nothing runs anyway: the call would be
  answered with the reused-id note instead of the incomplete-reply one. With an `End` the note
  is never read.
* One prefix comparison per tool chunk (a `memcmp` of the arguments received so far, next to
  the clone genai already makes of them). Not measured; no allocation is added on the path.
* The model now sees `not_executed` results on the resume. They carry `executed: false` and a
  hint, like the rejected-call results; the `error` values are new strings and are not the
  `invalid_arguments` marker the controller auto-resumes on.

### Tests

* `chat_stream::duplicate_call_id_tests`: `a_later_chunk_carries_on_its_call_and_a_reused_id_is_another_call`,
  `the_tools_the_stream_runs_itself_are_the_ones_an_incomplete_reply_does_not_run`,
  `an_incomplete_reply_answers_say_that_nothing_ran`.
* Corpus `noend_in_stream_tools_do_not_run` (a grep, a websearch, a file_glob with the grep's id,
  a webfetch, a todowrite; `never_end`), replayed with the corpus (I4: every call id once, each
  with one result).
* `corpus_replay_tests::a_reply_that_closes_with_no_end_runs_no_search`: the rig's local search
  counter stands still across that scenario, after a control (the same search in a finished
  reply moves it).

### Seen while reading, not changed

* **A repeated id that is not next to its original is listed out of order in the next request**
  (`repeats_beside_their_originals`, `chat_stream.rs:740`): reply `[x, y, x]` with an `End`
  becomes `[x, x#2, y]`. The move is right when the repeat was only RECORDED late (its
  neighbours had early cards) and wrong when it was emitted late; the stored order is the same
  in both cases. I4 (`uniquify`) expects `[x, y, x#2]`. Filed, not worked.

## B. #1340 `editor_memory_7mb` and #1985 `flood_scrollback_find`

Owner decision 2026-11-10: the opt-in open trim stays, as an experiment that does not fail the
row; both cases are to run in the fix tier.

### Why neither case has a row in rounds 10 to 12: tier placement only

* `script/realapp/appci/cases.py` has two lists. `SMOKE` and `PROVING`; the fix tier
  (`APPCI_TIER=smoke`, `run.py:244-246`) plays `SMOKE + PROVING` and nothing else. A case in
  neither list runs only with no tier set (candidate / release: "full").
* `editor_memory_7mb` (`cases.py`, `@case(..., modes=("terminal",), order=85, gui=True)`) and
  `flood_scrollback_find` (`cases_robust.py`, `order=62`) are registered like every other case
  and were in neither list. They predate the PROVING rule (2026-11-09), which only says where
  NEW cases go.
* Every committed run since is a fix-tier run: `test-results/2026-11-10_build_r10a_*`,
  `_r11a_*`, `_r12a_*` all carry `"tier": "smoke"` in each platform's `results.json` and
  `"tier": "fix"` in the manifest. No candidate run happened.
* Skip conditions, read: the memory case skips when process memory is unreadable or the file
  never opens (not strict); the flood case skips without an OCR engine (tesseract is on the
  Linux box, `script/ci/nodes/linux-box.sh:81`, and on the Mac) and is `winmap.NOT_WINDOWS`
  ("needs OCR (tesseract) and the Linux find flow"). `md_viewer_big` and `soak` skip themselves
  in the smoke tier; these two have no such line. So nothing but the lists kept them out.

### Change (harness only; no app code)

* Both names are appended to `PROVING`. Not to `PROVING_FAKE` (no scripted model). On Windows
  `editor_memory_7mb` is in `winmap.BOTH` (runs in both panes, terminal pass) and the flood case
  SKIPs with its reason. Cost, estimated: about 8 minutes for the memory case (two 75 s
  undo-close waits, four relaunches), about 3 minutes per mode for the flood case.
* RSS against file size: `editor_cost_vs_file` records `<kind>_file_mb` and, per memory figure,
  `<kind>_open_cost_x_file` (MB added by the open / MB of the file; the issue is stated as
  "~120x file size"), plus one note. Reported; the verdict stays the absolute targets.
* The open trim: `trim_experiment` writes a tagged note (`EXPERIMENT (the opt-in open trim ...`,
  then `inside the budget` / `OUTSIDE the budget`) and the results go into the metric
  `trim_open_experiment`. The three places that failed the row for the opt-in trim (it logged no
  `heap_trim why=open`; a UI item over 4 ms and over the control while it ran; the load's layout
  frame over the budget and the control) now report. The trim, its environment switch and its
  A/B variant are all still there. The variant runs last, so losing its relaunch leaves the
  judged close-trim A/B complete. The default close trim is judged exactly as before.
* `flood_scrollback_find` itself is not changed. #1985 (app or harness) stays open until the
  row exists: `find_verdict` already separates the OCR-blind case from a view that did not move.

### Rejected

* A fix-tier-only short form of the memory case (one cycle, no grace wait). It would measure
  something else than the issue's figures; the cost is about 8 minutes once per platform.
* `r.known(...)` for an over-budget experiment. KNOWN is for an accepted shortfall of something
  judged; nothing of the experiment is judged.
* Adding the cases to `SMOKE`. The owner rule sends unproven cases to `PROVING`.

### Risk

Harness only. The fix tier gets longer by the figures above. An `editor_memory_7mb` row can
still FAIL on what is judged: memory held after close, cycle growth, the close trim's UI item
on macOS and Windows, and the open cost against the #1340 targets once the app version passes
0.1.17 (`ACCEPTED_FOR_RELEASE["editor_open_cost"]` reports it as KNOWN while `app/Cargo.toml`
says 0.1.17, which it does today). Those are the issue's subject, not the experiment.
