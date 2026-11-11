# r17-privacy: typed secrets in the input-order debug log (#2040)

Branch `fix/r17-privacy`, from integ/r14 04e5193a2. Nothing here was compiled or run (laptop rule).

## Cause

The input-order log (`RUST_LOG=warp::input_order=debug`) redacts text to lengths unless
`WARP_INPUT_ORDER_LOG_TEXT=1` is set. The appci launchers set it on every platform
(`script/realapp/appci/app.py`), and a person reproducing an input bug may set it too. With it on, nothing
told a secret from a command:

- `crates/warpui_core/src/input_order.rs` `describe_text` / `describe_bytes` / `describe_bytes_escaped`
  asked only `text_opted_in()`.
- `app/src/terminal/local_tty/event_loop.rs` (`hop=pty_write_bytes`, Windows only) passed the accepted bytes
  of every class but `SetupChunked` to `describe_bytes_escaped`, so each key of a PIN typed at a `read -s`
  was a `class=UserKey ... bytes="4"` line, and a coalesced or released hold was the PIN in one line.
- `app/src/terminal/model/typeahead_handoff.rs` `log_route_decision` (`hop=hand_route ... bytes=`) did the
  same on every platform. (`route_prompt_reader_bytes` already logged `bytes_len=` only.)
- `crates/warpui_core/src/core/app.rs` logs every key twice before it is routed (`keydown key=`,
  `typed_characters text=`), and the editor, the Enter path, the shell's input report and the queued agent
  prompt log buffers (`kh_editor_action`, `enter_begin`, `submit`, `input_report`, `queued_row_restored`,
  the typeahead insert lines), all through `describe_text`.

The app already knows when a secret is read: the tty is canonical with echo off (`tty_is_reading_secret`,
#2050 / #1611), a command announced a secret read (`TtyRead` marker, #1882), a reader ahead of the line
editor has the keys (`prompt_reader_asks`, #2038 / #2052). None of it reached the log.

## Fix

1. **Secret guard, in the one place every text passes** (`input_order.rs`). A pane registers lock-free
   probes (`register_secret_input_probe`): `TerminalModel` registers the `TtyRead` secret marker together
   with the hand-off's noted reader (a new lock-free copy, `prompt_reader_noted_flag`, written only by
   `set_prompt_reader_asks`), and the termios probe when the pty installs it (`set_secret_read_probe`).
   `describe_text`, `describe_bytes`, `describe_bytes_escaped` and the new
   `describe_user_key_bytes_escaped` show text only when the tester opted in AND no probe says yes. The
   probes are sampled at the log line, and only for a line that would otherwise show text: no stored
   state that can stick, no cost without the opt-in. `hop=secret_text_withheld active=<bool>` marks each
   change. Kill switch `PHOSPHOR_LOG_SECRET_GUARD=0`.
2. **User-key bytes need a second opt-in.** `hop=pty_write_bytes` for `UserKey` and `Paste` writes and
   `hop=hand_route bytes=` go through `describe_user_key_bytes_escaped`: `<N bytes>` unless BOTH
   `WARP_INPUT_ORDER_LOG_TEXT=1` and `WARP_INPUT_ORDER_LOG_KEY_BYTES_MAY_INCLUDE_PASSWORDS=1` are set. The
   second variable is the switch back to the old shape.
3. **Harness.** The launchers set the second opt-in (synthetic text only), so `line_evidence`, the
   Ctrl-Break sequence check and `agent_basics_powershell`'s "no prose in a pty write" keep their bytes.
   `precmd_secret_honor_ps1` (already in `cases.PROVING`) gains the oracle of this issue
   (`secret_log_verdict`): from the first PIN key on, no log line holds the PIN or its random half, the
   guard's hop is there, and the PIN's key lines are count-only; the keys of the line typed just before
   are the control. The "PIN in the editor at an Enter" oracle read buffer text, which the guard now
   withholds exactly in the failing state; it is kept and joined by a count form (`withheld_enters`: an
   Enter with a count-only buffer above 0). The key-count and key-latency parsers accept `key=<1 chars>`.

The guard is global on purpose: a key is logged before the app knows which pane takes it, so "this pane
is not the one reading" cannot be known at that line.

## Rejected

- **A stored "secret active" flag set and cleared by events.** A missed clear (a pane closed mid-read)
  would blind the harness for the rest of the run. Sampling the facts at the line cannot stick.
- **Tagging each queued pty write as secret at enqueue.** It needs a field through the write messages and
  the queue (`event_loop.rs`, `write_queue.rs`) for one debug line; the sample at write time plus the
  length-only default covers it. Residual: a key queued while the reader ends for another reason.
- **Lengths only for user keys with no way back.** Three harness oracles read those bytes; removing them
  loses coverage (owner rule: no oracle lost).
- **Moving the harness off key bytes entirely.** The Ctrl-Break sequence and "what did the app write"
  evidence have no effect file that says the same thing.
- **A new shape for withheld text** (`<N chars secret>`). Every parser of the count form would need it;
  the edge hop says the same without touching them.

## Audit of other lines that can carry typed text

A pass over every log call (`log::*!`, `safe_*!` full arms, input-order hops, prints) in the terminal,
editor, agent input and UI-core code. Named in the issue and all through `describe_text` / `describe_bytes*`,
so covered by the guard with no edit at the call site: `keydown`, `keydown_keybound`, `typed_characters`,
`submit`, `kh_editor_action`, `enter_begin`, the typeahead insert lines of `input.rs`, `input_report`,
`queued_row_restored` (agent prompt), `early_output`'s clear line, the two "ignoring control seq" lines.
`pwsh_command_via_pipe` logs a byte count only. `input.rs`, `enter_queue.rs` and `give_up_to_pty.rs` were
not edited.

Changed here because a key typed at a secret read can reach them:

| line | was | now |
|---|---|---|
| `kitty_keyboard_protocol.rs` "Generated CSI u sequence" (plain `debug`, no opt-in) | the key and its whole sequence, for every key while a program has the kitty protocol on | `describe_key` / `describe_user_key_bytes_escaped` |
| `input_model.rs` `log_routing_decision` (`phosphor::routing`) | the buffer's first word, always | `"<withheld>"` while the guard withholds (keys that went to the editor instead of the reader make the first word the secret) |

Found, NOT changed (not on the path of a key typed at a secret read; listed on the issue for a decision):

| line | carries | fires |
|---|---|---|
| `warpui_core/src/core/app.rs` "dispatching typed action" | `{action:?}`: `String` payloads of search / tab-name / free-text actions in every build; editor inserts only in debug builds (`UserInput`); terminal key actions have a redacting `Debug` | debug level |
| `terminal/history.rs` "Creating new History model" | every persisted command line | debug, at start |
| `ai/blocklist/controller.rs` `[byop-diag] LRC running_command filled` | the running command's line | info |
| `terminal/find/model.rs`, `ai/blocklist/block/find.rs`, `code_review/find_model.rs`, `workspace/view.rs` (worktree sidecar) | a typed search query | trace / on a bad regex / info |
| `ai/predict/next_command_model.rs` (prefix mismatch and validation lines) | the text typed so far in the input | debug / warn |
| `warp_completer/src/parsers/legacy.rs` "parsing internal command" | the words of the buffer being parsed | debug |
| `editor/view/voice.rs` | dictated text | debug |
| `crates/editor` buffer / core apply lines | inserted text of the rich-text editors (not the terminal input) | debug / trace |
| `terminal/model/early_output.rs`, `terminal/model/ansi/mod.rs` `safe_debug!` full arms; `early_output.rs` info lines | typeahead, the shell's hook JSON (the input report) | Dev / Local channel only; Integration channel only |
| `block/interaction_mode.rs` "interactive prompt ... unchanged" | the cursor line of an agent-driven command's output (echoed input included) | info; not reached with echo off |

Ruled out (counts, enums or ids only): the input-queue / paste-queue / give-up lines, `dest_table.rs`
(`Replayed::Signal` only; its derived `Debug` would print text for other variants, which no caller
passes), `session.rs`, the Tab-completion lines, the ssh escape lines.

## Limits (accepted; recorded in DECLINED.md)

- ConPTY: a precmd reader is noted at the first key or the prompt hold's cap, so the keys before that are
  logged as text (one character per line). A command's own `read -s` in Git Bash gives ConPTY no fact.
- Any platform: keys typed before the reader turned echo off.
- A secret asked for on a remote host: the local tty is raw under ssh.
- ssh.exe's password prompt on ConPTY.
- Over-redaction: while any pane reads a secret, text typed in another pane is logged by length too.

## Risk

- Logging only; no key path changes. The hand-off change is a setter around an existing bool.
- Harness: a case that types while a secret is read and reads TEXT from the log sees counts now. The cases
  found (`precmd_secret_honor_ps1`, `precmd_question_echo`) are adapted; `PHOSPHOR_LOG_SECRET_GUARD=0`
  tells a guard effect from an app defect if another one shows up.
- Cost in a harness run: per text field one mutex and one `tcgetattr` per local pane (microseconds).
