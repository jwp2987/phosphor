# Round 12: honor PS1 and the precmd secret (#1923)

Branch `fix/r12-ps1case`, from `integ/r12` c7c606d3a. Nothing here was compiled or run; Rust is
`rustfmt --check` only, the appci wiring is `run.py --self-test` only. Wording: "worked, unproven"
until a run shows it.

Task: no appci case runs with "honor PS1" on, the mode in which the round 11 fix of #1923 was said
to be broken (`docs/design/r12-keyfacts.md`). Add one. Check the claim of that note that with honor
PS1 on a precmd function's question may not be drawn until the prompt arrives.

## 1. Where the question is written (read from the code)

Scenario: a bash precmd function runs `read -s -p "pin: "`. Order in bash: `warp_precmd` sends the
Precmd, the user's function prints `pin: ` and reads, `warp_update_prompt_vars` runs last (it
re-appends itself, `bash_body.sh:2046-2067`), then bash prints PS1.

The Precmd leaves a fresh active block: `BeforeExecution`, not started. From there every printed
byte goes through the block list's handler, and its `delegate!` (`blocks.rs:832-840`) asks
`is_early_output()` first (`blocks.rs:4908-4914`): bootstrap done, active block not started, no
prompt being received, state `BeforeExecution`. All four hold after a Precmd, in both prompt modes
(the prompt's start marker rides in PS1, after the function; with the app's prompt bash sends no
marker before it either). So the bytes go to `EarlyOutput::handler` (`early_output.rs:392-407`),
which starts a BACKGROUND block on the first printed character and writes to its output grid. The
active block gets nothing.

Consequences:

- **The question is drawn, with or without honor PS1.** A background block is an ordinary block of
  the list, its output grid started by `start_background` (`block.rs:1296-1308`). This is the
  `pin: ` of the round 10 Windows screenshot; an unstarted active block has height 0
  (`header_grid.rs:896-902`, `is_command_empty`) and could not have shown it. The claim of
  `r12-keyfacts.md` ("probably not drawn until the prompt arrives, grid length 0") is about a grid
  the question never enters. **Not a defect; no issue filed.**
- **The screen fact of #1923 read the wrong block in rounds 10, 11 and 12.**
  `screen_tail_asks_for_input` (`view/paste_queue.rs`) passed `block_list().active_block()`.
  Round 10 read its output grid, round 11 its prompt-and-command grid, round 12 that grid's cells
  when unstarted. None holds the question. At the prompt hold's cap the fact is false and the keys
  stay held (`prompt_hold_kept_overdue`), which is the round 10 symptom, unchanged, in both prompt
  modes.
- **The round 11 unit failure has the same cause.** `TerminalModel::mock` runs
  `bootstrapped`, `command_finished`, Precmd (`terminal_model.rs:1218-1245`), the same sequence
  `early_output_tests.rs` uses to reach `PostBootstrapPrecmd`. So `simulate_block` + `process_bytes("pin: ")`
  writes to a background block in the test as well. The first two assertions passed (the active
  block is before execution, its output grid is empty), the third failed because the active block
  is empty. `r12-keyfacts.md` read `Block`'s `delegate!` and missed the block list's. Its new
  assertion (`!prompt_and_command_grid().started()`) is true, and its fix would fail the same last
  assertion again: an unstarted, EMPTY grid has its cursor at column 0.

Not traced: zsh and fish (the rule is shell-neutral: no shell prints into a block that has not
started). An agent conversation: a background block belongs to no command of the conversation
(`create_pending_background_block` passes no conversation id, `blocks.rs:4027-4043`), and
`should_hide_block` hides terminal blocks that are not attached to the active conversation
(`block.rs:1496-1530`). Whether the question is visible in agent view depends on the view's
transcript scope and was not settled by reading; the new case records it (metric, not a verdict).

## 2. Fix

- `BlockList::block_receiving_output()` (`blocks.rs`): the block the next printed character is
  written to, by the handler's own rule: in early output the background block (in the list, or
  still pending), otherwise the active block.
- `key_facts::block_tail_asks_for_input(block)` (renamed from `active_block_tail_asks_for_input`,
  body unchanged). The view passes `block_receiving_output()`. For a background block
  `active_grid_type()` is `Output` and the grid is started, so the existing started-grid read
  applies. The unstarted-grid arm of round 12 stays (a block written before its grid starts).

Unit tests (`key_facts_tests.rs`), see `docs/testing.md` "Round 12: honor PS1 and the precmd
secret". The round 11 test keeps its scenario and its expectation (the question is seen); the block
the last assertion reads changes, because the old subject encoded the wrong premise. It now also
asserts that premise's opposite: the active block holds nothing of the question.

Rejected:

- Making early output go to the active block's grid when a question is printed: it changes what
  is drawn and how typeahead echo is separated from background output, for every prompt.
- Searching every unfinished block for a question: the handler already names one block.
- Leaving the app alone and only adding the case: the case would fail in both arms for a cause
  already readable in the code, and the two uncompiled round 12 tests would fail the next unit run.

Risk: low. One caller, at the prompt hold's cap (ConPTY only). New behaviour only when the tail of
the early-output block is an unterminated question; the keys then go to the pty in typed order,
which is what a plain terminal does. A background job that prints `Continue? ` at an idle prompt
within the hold (between the Precmd and the line editor's signal) has the same effect; that window
is milliseconds in a shell that signals.

This is the fourth change on the same fact. Per the owner's "design after the second patch" rule
this note is the audit: sections 1 and 2 are the whole path, and the case in section 3 is the
real-app proof that was missing in every round.

## 3. The case: `precmd_secret_honor_ps1`

`script/realapp/appci/cases_precmd_secret.py`, in `cases.PROVING`, both session modes.

- The setting (`terminal.input.honor_ps1`) reaches a pane as `WARP_HONOR_PS1` in its shell's
  environment (`local_tty/unix.rs:480-484`, `windows/environment.rs:60-63`), so it is fixed when the
  shell starts. The case writes the key into the profile's `settings.toml` and relaunches the app
  with the helpers every restart case uses (`cases_robust.stop_for_restart` / `start_app`), then
  asks the PANE what it got (`echo "hp=${WARP_HONOR_PS1:-unset} sh=${BASH_VERSION:+bash}" > file`).
  An arm whose pane is not a bash in the wanted mode is UNVERIFIABLE.
- Two arms, `hp0` (app prompt) then `hp1` (honor PS1). The control arm is there because on Linux
  and macOS no root-pane precmd `read -s` was ever tested: the prompt hold is ConPTY-only, and a
  failure there in both arms is not an honor-PS1 finding.
- The function is sourced from a file (`source ./pp_<arm>.sh 2> err`): the long definition line is
  not typed, and in an agent conversation the redirect makes the line shell syntax (#1560). The file
  writes a marker when sourced and the function writes one right before its `read`; the PIN is typed
  only after both exist. (The existing `gitbash_precmd_secret` waits for its trigger file, which the
  function removes within milliseconds; a poll can miss it. Not changed here.)
- The PIN goes in two halves around a pause of 2.7 s, as in `gitbash_precmd_secret`.
- Oracles: the effect file holds the PIN; no `hop=enter_begin` shows the PIN or a half of it in the
  editor's buffer; no `rec.log` command holds it; the next line runs; the question is on screen (OCR
  for `vault`: a check in terminal mode, a metric in agent mode, see section 1).
- macOS: the pane is zsh. Both arms relaunch with `session.startup_shell_override = "bash"` (the
  system bash 3.2). Windows: the Git Bash pane only.
- The original `settings.toml` text is written back and the app relaunched in a `finally`.

Rejected: switching the mode in the running shell (`warp_change_prompt_modes_to_ps1`): the model
follows the shell's `honor_ps1` (`header_grid.rs:1260-1278`) but the view keeps reading the
setting, a state no user has. A variant arm inside `gitbash_precmd_secret`: that case is terminal
mode and Windows only, and it is the open proof of #1923; it is left as it is.

## 4. What the next run should show

- Unit: the four `key_facts` tests of section 2 pass. If the first fails at
  `written.is_background()`, section 1 is wrong and `r12-keyfacts.md` stands.
- Windows Git Bash, `gitbash_precmd_secret` and both arms of `precmd_secret_honor_ps1`: at the cap
  after the first half, `hop=prompt_hold_expired precmd_wait=true` and the release warning, no
  `prompt_hold_kept_overdue`; the PIN in the effect file; the next line runs.
- Linux and macOS: unknown. `hp0` and `hp1` should agree; a difference between them is an honor-PS1
  finding, a failure in both is the precmd read on that platform.
- Agent mode: the metric `question_on_screen_hp0` / `_hp1`. False in both arms means an agent
  conversation hides the question of a precmd function: an owner question, not decided here.
