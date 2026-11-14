# Round 13: questions asked by precmd functions (#2053 implemented, #2052 design only)

Branch `fix/r13-secretui`, from `integ/r13` 574f83868. Nothing here was compiled or run; Rust is
`rustfmt --check` only, the appci wiring is `run.py --self-test` only. Wording: "worked, unproven"
until a run shows it. Owner decisions 2026-11-10: #2053 "show the question in an agent
conversation only while something is reading the tty"; #2052 "the bash-side marker, design first".

## 1. #2053: an agent conversation shows the question while its reader waits (implemented)

### Cause (APP, by design until the owner decided)

A precmd function's `read -s -p "vault pin: "` prints between the Precmd and the command's start.
That is early output (`BlockList::is_early_output`, `blocks.rs`), written to a background block
(`EarlyOutput::handler`). `create_pending_background_block` passes no conversation id, so the
block is `AgentViewVisibility::Terminal` with no conversation attached, and
`Block::should_hide_block` returns `true` for it in `TranscriptScope::Conversation` (`block.rs`,
the `visible_in_conversation` match). Height 0 in the conversation, so nothing is drawn: round 12
agent pass, `question_on_screen_hp0 = false`, `_hp1 = false`. With #2050 the keys reach the
`read`; the user answers a question they cannot see.

### Fix

One fact, already kept by the hand-off: a reader ahead of the line editor is noted
(`TypeaheadHandoff::prompt_reader_asks`). It is set from the tty on Unix (#2050,
`take_input_for_secret_reader`: canonical, echo off) and from the screen on ConPTY (#2038), and it
ends on events only (the line editor's signal, a Preexec, the next Precmd, a new session).

- `Block::shown_to_tty_reader` (transient, never serialized). In a conversation scope a block is
  hidden only if it is not attached to the conversation AND this is not set. No other scope reads
  it: the terminal scope shows a background block either way.
- `BlockList::show_question_of_tty_reader(waiting)`: while `waiting`, flags the background block
  receiving early output (once it is in the list) and refreshes its cached height
  (`refresh_block_height`, the #810 rule for visibility flips); when not waiting, clears the flag
  and refreshes again. Idempotent; returns whether anything changed. `finish_background_block`
  clears the flag before its emptiness test, so whether a block no conversation owns is reported
  as completed (and saved for restore) is exactly as before.
- `TerminalView::show_question_of_tty_reader(site, ctx)` (`view/paste_queue.rs`): reads the
  hand-off's fact and tells the block list. Called
  - at every terminal wakeup, after the #2050 look (`handle_terminal_wakeup`): the question is
    output, and so is every event that ends a reader, so the wakeup after that event hides the
    block again;
  - where ConPTY notes its reader: on the write path after `note_prompt_reader_if_the_screen_asks`
    and at the prompt hold's cap (`prompt_hold_screen_asks`). A `read -s` prints nothing after the
    key, so no wakeup would come;
  - at a key the pane's sink gives a secret reader, only while no block is shown yet (the
    wakeup's look found the model busy).
  With no reader noted and none at the last look it is one read of the hand-off and returns. The
  model is taken with `try_lock` only; a busy model is parsing and its wakeup asks again. No clock.
- Kill switch `PHOSPHOR_READER_QUESTION_IN_AGENT_VIEW=0` (read once per pane): the look never
  says "waiting", the block stays hidden as in round 12. The keys still go to the reader.

"Only while": the block is shown from the event that notes the reader to the wakeup after the
event that ends it. For bash that end is the line editor's marker, milliseconds after the answer's
Enter; the question then leaves the conversation (it stays in the terminal view's transcript).

### Rejected

- **Attaching the block to the conversation** (`add_attached_conversation_id`): permanent, it is
  persisted, and it makes shell output no command produced part of what the conversation shows
  and may send. The owner asked for "only while".
- **A notice or a card instead of the block:** a second rendering of text the block already
  draws, and it needs its own redaction story.
- **Asking the tty again from the view each frame:** the hand-off already holds the answer and
  ends it on events.
- **Clearing only in the model at the reader's end events:** the block list would need its own
  copy of the hand-off's event list. The view's wakeup look reads one fact instead.

### Risk

Low. A hidden block becomes visible, in agent view only, only while a reader is noted.

- A shell that sends no line-editor marker keeps its reader noted until the next Preexec or
  Precmd; the question stays on screen in the conversation until then.
- The whole background block is shown, not the question's line: output a precmd function printed
  before its question in the same stage is shown with it (it is in the terminal view already).
- ConPTY: the reader is noted at the first key or at the hold's cap, not at the question's
  output, so the question appears in the conversation when the first key is typed (or the cap
  passes). The marker of section 2 would close that gap for the bash builtin.
- Cost: one hand-off read per wakeup and per pty write of a key; a `try_lock` and a block-list
  call only while a reader is noted.

### Tests

- `model/blocks_test.rs`: `a_tty_readers_question_is_shown_in_a_conversation_with_or_without_the_reader`,
  `a_tty_readers_question_changes_nothing_in_the_terminal_scope`,
  `a_finished_question_block_stays_shown_in_the_conversation_and_is_reported`.
- `terminal/view_test.rs`: `a_precmd_question_is_shown_in_an_agent_conversation_while_its_reader_has_the_keys`
  (the #2050 fixture in a conversation scope; shown at the look after the reader is noted, kept
  while the PIN is typed, hidden at the look after the line editor's signal; control arm = kill
  switch; the pty writes are the PIN's keys in both arms).
- appci `precmd_secret_honor_ps1` (PROVING): the agent arm now JUDGES the question. Where the tty
  is sampled (Linux, macOS) the look before any key is judged in both modes
  (`question_on_screen_<arm>`). In the agent pass a second look after the first half of the PIN is
  judged on every platform (`question_on_screen_typing_<arm>`); on ConPTY it is the only judged
  look of the agent pass (see Risk). This is a stricter expectation, following the owner decision;
  nothing was weakened.

### What the next run should show

Agent pass, both arms, Linux and macOS: `question_on_screen_hp0/_hp1 = true`,
`question_on_screen_typing_* = true`, and in the app log after `touch pp_<arm>_go.txt`:

    hop=prompt_reader_asks by=tty_secret took_from_editor=true
    hop=reader_question_in_conversation site=output shown=true
    ... hop=prompt_reader_end why=line_editor_signal
    hop=reader_question_in_conversation site=output shown=false

`site=key` on the first line is still a pass (the question's wakeup found the model busy).
Windows Git Bash, agent pass: `shown=true` with `site=key` or `site=cap`;
`question_on_screen_typing_* = true`; `question_on_screen_*` may be false (recorded). No
`shown=true` line with `prompt_reader_asks` present means the block was not found (not early
output, or still pending). Terminal pass: no change expected on any platform.

## 2. #2052: an echoing question from a precmd function on Unix (DESIGN ONLY, not implemented)

### The gap

`read -p "continue? " x` in a precmd function that runs after the app's Precmd. On Linux and
macOS the Precmd has handed input to the editor; the answer is typed into the editor and its Enter
runs it as a command or sends it to the agent. #2050's OS fact cannot see it: an echoing read
leaves the tty canonical with echo on, which is also the tty of a shell between its Precmd and its
line editor.

Which functions are after the Precmd (read from the bootstraps): `warp_precmd` is appended to
`precmd_functions` when the bootstrap runs (`bash_body.sh:3373`, `zsh_body.sh:2119`) and
`warp_update_prompt_vars` re-appends itself last at every prompt. So functions the rc files
registered run BEFORE the Precmd (the app still sees the command running, the pty owns input, a
question there already works like a plain terminal). After the Precmd run: the user's
`PROMPT_COMMAND` (bash: `user_prompt_command`), and every function registered after the bootstrap
(typed, sourced later, or installed lazily by a tool). fish: `--on-event fish_prompt` handlers
defined after `warp_precmd`.

### The marker: the shell says "I am asking now"

A stage bracket alone ("user precmd code runs now") adds nothing: the app already knows that
window (`ShellActivity::Prompt`, Precmd parsed, no line-editor marker yet). Acting on the window
alone means sending every key typed in it to the pty, which is arming the prompt hold on Unix,
rejected in `r13-wlps1.md` section 4. What is missing is the moment of the read. The shell knows
it exactly, for its own builtin:

**During the user stage of the precmd, `read` is a function that announces the read and then
runs the builtin.** The bracket is the app's own two functions: `warp_precmd` opens the stage as
its last step, `warp_update_prompt_vars` closes it as its first.

bash (sketch, builtins only, no fork):

    __warp_reader_stage_open () {      # last step of warp_precmd
      [[ ${PHOSPHOR_READER_MARKER:-1} != 0 && -n ${WARP_SESSION_ID:-} ]] || return 0
      [[ :$SHELLOPTS: == *:posix:* ]] && return 0          # see Risks
      declare -F read >/dev/null && return 0               # the user's own `read`: never replaced
      read () {
        if [[ -t 0 ]]; then
          __warp_emit '\e]9278;f;{"hook": "TtyReaderAsks", "value": {"session_id": %s, "seq": %s}}\a' \
            "$WARP_SESSION_ID" "${__warp_seq:-0}"
        fi
        builtin read "$@"
      }
      __warp_reader_stage=1
    }
    __warp_reader_stage_close () {     # first step of warp_update_prompt_vars
      [[ -n ${__warp_reader_stage-} ]] && { unset -f read; __warp_reader_stage=; }
    }

- Same OSC 9278 JSON form and the same `session_id` / `seq` as `LineEditorReading`
  (`bash_body.sh:1960`): parsed in stream order, so the marker is applied before the question's
  bytes are drawn; a marker of an older prompt is ignored by `seq`.
- Outside the stage `read` is the builtin: nothing changes for completion functions, loops and
  scripts. Inside the stage a read from a file or pipe (`-t 0` false) sends nothing. Measured
  here (bash 5.3): the function adds about 2.7 us per call.
- No locals in the wrapper, so `read -r a b`, `local` variables of the caller, `REPLY`, `-a`,
  `IFS=: read`, the EOF status and `-t` behave as with the builtin (checked with a prototype
  outside the repo, bash 5.3; bash 3.2 not checked).
- `-u N` and `read <&3` are not announced unless fd 0 is the terminal: accepted for a first cut.

zsh: the same pair in `warp_precmd` / `warp_update_prompt_vars` (`functions[read]` defined and
`unfunction read`; `builtin read "$@"`; skipped when `$+functions[read]`). `read -q`, `read -k`
and `vared` use the same announcement; `vared` starts ZLE, which runs `zle-line-init` and sends
`LineEditorReading` today (not traced further; see Open).

fish: `function read --no-scope-shadowing` (so `read -l` still creates the caller's local)
defined by `warp_precmd`'s handler and erased (`functions -e read`) where `__warp_lr_print` runs.
fish's `read` runs fish's own reader (non-canonical), so #2050 does not see even `read -s` there;
the marker would cover both. Highest uncertainty of the three; a later step.

### App side: a third source for the one reader fact

Today the reader of a prompt is noted by `tty_secret` (#2050, Unix) or by the screen (#2038,
ConPTY). The marker is a third source, and the rest of the path is unchanged:

- Parser: hook `TtyReaderAsks` -> `TerminalModel::tty_reader_asks`, next to `line_editor_reading`
  (`terminal_model.rs:4217`), on the parser thread in stream order:
  `TypeaheadHandoff::note_reader_by_marker(session_id, seq)`. Allowed only in
  `ShellActivity::Prompt` for the current seq. Where the editor owns input it does what
  `take_input_for_secret_reader` does (pty owns input, `PtyOwned { false, 1, 1 }`, reader noted,
  keys-follow-editor ended); where a prompt hold is queued (ConPTY) it does what
  `note_prompt_reader_asks` does. Log `hop=prompt_reader_asks by=shell_marker`.
- View: the wakeup's look (`hand_keys_to_a_secret_reader`) already returns early when a reader has
  the keys; it gains "a reader was noted since the last look" (an edge the hand-off keeps) to
  clear the queue pin and move focus to the terminal once. `secret_reader_has_the_keys` becomes
  "a reader noted by the tty or by the marker", so the pane's sink routes the keys to the
  terminal whatever view has focus, as for #2050.
- From there nothing new: `route_prompt_reader_bytes` writes each key at once and in order, the
  Enter is the read's, the reader ends at the line editor's signal / Preexec / next Precmd, keys
  typed after the read's Enter are the shell's line and are requested into the editor (the
  unread-keys rule of #2038). No `ReaderDone` marker is needed for a shell that sends
  `LineEditorReading`; a second `read` in the same stage finds the reader still noted.
- #2053 follows for free: `prompt_reader_asks` is what shows the block in a conversation, so an
  echoing question is visible in agent view, and on ConPTY from the question's output instead of
  the first key.
- ConPTY keeps the screen fact for what the marker cannot announce (an external program); the
  marker makes the bash builtin exact there too. #2050's tty fact stays for `sudo`, `ssh-add` and
  any program that turns echo off.

### What it does not cover

- An external program asking with echo on from a precmd function (`python -c 'input()'`),
  `select`, `read` from `-u N`: still the editor's on Unix. Linux could add the `/proc` read
  probe of #1641 (`foreground_reads_tty`) as a fourth source at a key while activity is `Prompt`;
  macOS has no such probe. Not proposed now.
- Text typed before the question is drawn went to the editor and stays there (as #2050).

### Risks

- **Every pane loads these files.** A wrong edit breaks every prompt. Mitigations: the function
  exists only between the app's two precmd functions; it is never defined over a user's `read`;
  it is removed by a function that already runs last at every prompt; and with the switch off the
  files behave byte for byte as today (the open/close helpers return first).
- **bash POSIX mode:** on older bash an assignment before a function call persists after it in
  POSIX mode (`IFS= read` would leave `IFS` empty). bash 5.3 here does not do that; bash 3.2
  (macOS) is the one to check. The sketch does not install the wrapper in POSIX mode at all.
- **A precmd function that fails between open and close** cannot leave the wrapper behind:
  bash-preexec runs each function of the array regardless of the previous one's status, and
  `warp_update_prompt_vars` is forced last. A user who removes `warp_update_prompt_vars` from the
  array (zsh repair paths exist for that, `zsh_body.sh:2138-2201`) would keep the wrapper for the
  session until the next open (see the next point).
- **In-band generator commands** unregister the user's precmd functions and run only
  `warp_precmd` (`bash_body.sh:395-398`), so `warp_update_prompt_vars` does not run in that
  cycle: `warp_precmd` must not open the stage on its in-band path (it knows it:
  `is_after_in_band_command`), and the open helper closes a stage left open before it opens one.
- **`set -x` users** see the wrapper's lines in their trace during the stage.
- **App side:** a marker taking input from the editor is the #2050 transition with a different
  trigger; the same unit tests apply. A stale or forged marker (a program printing the OSC) can
  only move input to the pty for the current prompt, which the next line-editor signal undoes;
  it is honoured only in `ShellActivity::Prompt` with the current `seq`.

### Kill switches

- Shell side `PHOSPHOR_READER_MARKER=0`: no wrapper is defined, no marker is sent.
- App side `PHOSPHOR_READER_MARKER_APP=0` (or one name read by both): the hook is parsed and
  ignored.
- Existing: `PHOSPHOR_SECRET_READER=0` (#2050), `PHOSPHOR_READER_QUESTION_IN_AGENT_VIEW=0`.

### Tests (for the implementation round)

- Shell, no app (`script/test_bash_bootstrap_*` style, bash 3.2 and 5.x, zsh): inside the stage
  `read -p` emits exactly one marker before the question; a read from a file emits none; `IFS=:
  read -r a b`, caller locals, `REPLY`, `-a`, EOF status and `-t 0` equal the builtin's; after the
  stage `type -t read` is `builtin`; a user-defined `read` is untouched; POSIX mode installs
  nothing; `set -u` / `set -e` do not abort the helpers.
- Unit, `typeahead_handoff_tests.rs`: the marker at a prompt the editor owns takes input for the
  pty and every key is the reader's; ignored outside `Prompt`, for an old seq, with the switch
  off; ends on the same events; keys after the read's Enter are requested.
- View: the #2050 pair of tests with the marker as the trigger (focus in the editor, Legacy and
  Table; no character of the answer in the editor, one pty write per key).
- appci (PROVING): `precmd_question_echo`, both modes, bash on Linux / macOS (3.2) / Git Bash and
  zsh: a precmd function's `read -p "continue? " x` writes `x` to a file. The answer reaches the
  read whole, is never in the editor at an Enter (`pin_in_editor_enters`), is not a command the
  shell received, is not sent to the agent, the question is on screen in both modes, and the next
  line runs. A second arm asks from `PROMPT_COMMAND`.

### Rejected for #2052

- **The #2038 screen fact on Unix** (the tail of the early-output block is an unterminated
  question): a guess, and it would run on every key at every prompt of the owner's main platform.
- **Sending the Precmd after the user's precmd functions** (re-append `warp_precmd` late, as
  `warp_update_prompt_vars` does): every question would be asked while the app still sees the
  command running, with no marker at all. But precmd output would land in the previous command's
  block, and a slow `PROMPT_COMMAND` would delay the app's prompt and the editor taking input at
  every prompt. Too wide a change of the common path for this case.
- **A permanent `read` function:** every completion function and loop of the interactive shell
  would pay for it and be exposed to its edge cases.
- **`/proc` only:** Linux only, and a sample per key instead of an event.

### Open, for the owner (not decided here)

- Go / no-go on replacing `read` during the precmd stage, and whether fish is in the first cut.
- zsh `vared` in a precmd function sends `LineEditorReading` (read from the bootstrap, not run):
  the app would treat it as the shell's line editor. Not filed; needs a run first.
