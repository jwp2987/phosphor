# r17-privacy2: ten more log lines that can carry typed or user text (#2100)

Branch `fix/r17-privacy2`, from integ/r14 3e095eaad. Nothing here was compiled or run (laptop rule).

## Cause

The #2040 audit (`docs/design/r10-r17-privacy.md`, last table) found ten kinds of log line, outside the
`warp::input_order` target, that print user text with plain `{:?}` / `{}`: no opt-in, no secret guard. Three are
info or warn lines, so they are in every default log (and in a diagnostics bundle). Read again for this task; the
call sites as they were at 3e095eaad:

| # | line | where | level |
|---|---|---|---|
| 1 | `dispatching typed action: {}::{action:?}`, and the same dump in `Dispatched action has no handlers` and `Action .. was dispatched, but no view handled it` | `crates/warpui_core/src/core/app.rs:1842`, `:1853`, `:1922` | caller's level; warn; debug |
| 2 | `Creating new History model with persisted commands {..:?}` | `app/src/terminal/history.rs:629` | debug |
| 3 | `[byop-diag] LRC running_command filled .. command={:?}` | `app/src/ai/blocklist/controller.rs:4123` | info |
| 4 | find / search queries | `app/src/terminal/find/model.rs:565` (trace), `app/src/ai/blocklist/block/find.rs:179` (warn), `app/src/code_review/find_model.rs:336` (info, with the regex error, which prints the pattern), `app/src/workspace/view.rs:9898` (info), `crates/editor/src/search.rs:344` (warn, the error) | as listed |
| 5 | next-command lines | `app/src/ai/predict/next_command_model.rs:104`, `:138`, `:609`, `:802`, `:971`, `:1008` | debug / warn |
| 6 | `parsing internal command {lite_cmd:?}` | `crates/warp_completer/src/parsers/legacy.rs:83` | debug |
| 7 | `Transcribed voice input: {..:?}` | `app/src/editor/view/voice.rs:508` | debug |
| 8 | rich-text editor apply lines | `crates/editor/src/content/core.rs:272`, `:450`, `:452`, `:473`; `content/buffer.rs:1763`; `content/find.rs:151`, `:297`; `render/model/mod.rs:3285` | debug / trace |
| 9 | typeahead and hook JSON | `app/src/terminal/model/early_output.rs:198`, `:206`, `:274`, `:456`, `:464`, `:485`; `app/src/terminal/model/ansi/mod.rs:775`, `:827`, `:838`, `:865`, `:1005`, `:1377`, `:1393` | Dev / Local `full:` arms; Integration channel info |
| 10 | `interactive prompt {:?} unchanged ..` and the two `prompt-shaped line {:?} ..` lines | `app/src/terminal/model/block/interaction_mode.rs:993`, `:1007`, `:1021` | info |

All ten are app defects (a log holds text the user did not agree to log). None is a harness fault.

Wider than the issue's wording, found on the same read and treated the same way: the two twins of line 1; the
regex error next to the query (4); all six next-command lines, not only the two prefix ones (5); the decode-error
`full:` arms next to the hook JSON (9: a serde error quotes the value it rejects); the two `prompt-shaped line`
lines next to line 10.

## Fix

One rule, the one #2040 made: a length unless `WARP_INPUT_ORDER_LOG_TEXT=1`, and a length whenever the secret
guard says a secret may be typed.

1. **`input_order::TextShown`** (`crates/warpui_core/src/input_order.rs`): the answer to "may this line show
   text" taken once per line. `TextShown::sample()` is the same decision `describe_text` makes (same `TextPolicy`,
   same probes, same `hop=secret_text_withheld` edge line). Forms: `text` (`"x"` / `<N chars>`), `opt_text`
   (`Some("x")` / `Some(<N chars>)` / `None`, the shape `{:?}` gave), `display` (unquoted / `<N chars>`), `debug`
   (the dump, or the type / variant name it starts with and the length of the rest: `Insert<20 chars>`,
   `Backspace`). `describe_debug`, `describe_opt_text`, `describe_display` are the sampled shorthands.
2. **A builder per line.** Each line is a small function that takes its inputs and a `TextShown` and returns the
   string; the call site is `log::x!("{}", builder(.., TextShown::sample()))`. The builder call stays inside the
   macro's arguments, so nothing is formatted or sampled when the level is off (the trace lines dump the whole
   editor buffer).
3. **`TextShown::fixed(text_opted_in, secret_active)` and `assert_line_withholds`** for the tests: a line is built
   under each policy and must hold none of the given texts by default and during a secret read (opt-in on or
   off), and all of them with the opt-in and no secret. The last part is the control: a test of a line that never
   had the text cannot pass.

The name kept by `debug` is safe by construction: it must start with an ASCII capital, hold only identifier
characters and `::`, and be followed by the end, `(`, ` {` or `{`. A quoted string, a list, or raw text gives no
name. Residue: a hand-written `Debug` that prints raw user text shaped like `Word(..)`; none of the dumped types
has one.

Wording changes: the next-command lines quoted their text in backticks and now use the `describe_text` form
(`"x"` or `<N chars>`); the history line gains the count of commands; the "no handlers" / "no view handled it"
lines gain the action's type name.

## Harness

`grep` over `script/` and `crates/integration` for each line's words: only the prompt detector's lines are read,
by `cases_caseagent.lrc_takeover_verdict` and `cases_agent_interactive.handover_seen` / `notice_seen`, and those
match words outside the text (`handing the command to the user`, `showing a take-over notice`, `leaving the
command with the agent`). No parser reads text from any of the ten, so no parser changed and no oracle is lost.
The launchers already set the text opt-in, so a harness log shows these lines as before. Self-test
`agent_lrc_takeover_log_parse` now also feeds the count form of each fixture line and expects the same verdict.

## Rejected

- **Deleting the lines.** They are useful when reproducing with synthetic text; the opt-in keeps them.
- **The `[byop-diag]` content switch for line 3** (`snippet_or_shape_for_log`, `chat_stream.rs`). It has no secret
  guard, and the issue asks for one rule; one switch for "typed text in the log" is easier to reason about.
- **A redacting `Debug` on every action type.** Hundreds of types; a new one would leak again. The dump is
  redacted at the one place it is printed.
- **Dropping the dump's leading name.** `Insert<20 chars>` against `<26 chars>`: the name is what makes the line
  worth keeping by default.
- **Calling `describe_text` at each site with no builder.** Shorter, but then no test can build the line under
  "secret read" (the policy is read from the environment once per process).

## Not changed

- `core.rs` `match_step` (`action={action:?}` at an Enter): a keymap-bound action, payload defined by the binding,
  not typed; an input-order line the harness may read.
- `crates/editor` `Describe for BlockItem` prints an image's alt text (`Image: {alt_text}`) in the trace-level
  render tree dumps: document content, one field, left; recorded here.
- `input.rs`, `enter_queue.rs`, `give_up_to_pty.rs`: none of the ten lines lives there; not edited.

## Risk

- Logging only. Call sites keep their level and module (the Windows launcher's `RUST_LOG` filters by module).
- A person reading a default log loses the text of these lines; `WARP_INPUT_ORDER_LOG_TEXT=1` brings it back.
- Cost: with the opt-in on, one more probe sample per such line (a mutex and one `tcgetattr` per local pane);
  without it, nothing is sampled. `debug` formats the dump to count it, as the old line formatted it to print it.
- `rustfmt` now formats two blocks of `next_command_model.rs` it used to skip (a long string literal made it give
  up on the enclosing expression); those hunks are formatting only (line breaks, trailing commas).
- Compile doubts are listed in the report (`f52/r17-privacy2.report.md`).
