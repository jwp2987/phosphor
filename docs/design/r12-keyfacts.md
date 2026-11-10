# Round 12: the `key_facts` unit failure of round 11 (#1923)

Branch `fix/r12-keyfacts`, from `integ/r12` d1ec60c7b. Nothing here was compiled or run; Rust is
`rustfmt --check` only. Wording: "worked, unproven" until a run shows it.

Failure (build/r11a, unit): `terminal::model::key_facts::tests::a_question_printed_after_the_precmd_is_seen_on_the_grid_the_block_writes_to`,
`key_facts_tests.rs:129`, `assertion failed: active_block_tail_asks_for_input(block)`. The two
assertions before it passed: the block is before execution and the output grid is empty.

## Verdict: APP. The test is right, the function was wrong for one prompt mode

The question asked: do bytes processed after `simulate_block` on the mock land in the
prompt-and-command grid, as they do in the real model after a Precmd? They do. The grid they land
in is not started, and that is also what the real model has in one of its two prompt modes.

Where the bytes go (same in the mock and the app):

- `Block::apply_precmd` sets `BlockState::BeforeExecution` (`block.rs:3452`); the block's
  `delegate!` sends every handler call to the header grid in that state (`block.rs:3326-3330`),
  and the header grid's `delegate!` sends it to `prompt_and_command_grid` outside a prompt
  (`header_grid.rs:63-65`). The mock is bootstrapped (`terminal_model.rs:1218`), so
  `BlockList::input` does not start the block on the first char (`blocks.rs:5202`).

What the grid's length says:

- `BlockGrid::len()` is 0 until the grid is started (`blockgrid.rs:184-192`), and
  `contents_to_string` returns an empty string for a grid of length 0 (`blockgrid.rs:504`). The
  cells and the cursor are written either way.
- The prompt-and-command grid is started in three places: a Precmd with a non-empty `ps1`
  (`header_grid.rs:1282-1298`), the prompt's start marker (`header_grid.rs:1190`), and the
  command's start (`header_grid.rs:695`).

What the shell sends (bash, `app/assets/bundled/bootstrap/bash_body.sh`):

- App prompt (honor PS1 off): `ps1` is the rendered PS1, for the prompt preview (`:697-732`,
  `:916-942`). The Precmd starts the grid. A precmd function's `pin: ` is then in a started grid
  of one row: the round 11 function reads it. This is the Windows CI setup, and the screenshot of
  round 10 shows `pin: ` drawn, which needs a started grid.
- The shell's own prompt (honor PS1 on): `deref_ps1` stays empty (`:691-696`), the Precmd carries
  `"ps1": ""`, read as `None` (`dcs_hooks.rs:862`), and `prompt_only_precmd` does not start the
  grid. The start marker rides in PS1 (`:2031-2034`), which bash prints after ALL precmd
  functions. So a precmd function's `read -s -p "pin: "` asks into an unstarted grid. That is
  exactly the mock's state (`finish_block` sends a Precmd with no `ps1`).

So for a user with honor PS1 on, at the prompt hold's cap `screen_tail_asks_for_input()` was false
with the question in the grid, and the PIN stayed held (`prompt_hold_kept_overdue`): the round 10
symptom of #1923, unchanged. The round 11 change is correct only with the app's prompt.

## Fix

`key_facts::active_block_tail_asks_for_input`: a started grid is read as before
(`contents_to_string`, last rows of the grid); an unstarted grid is read from its cells, the rows
ending at the cursor's row (`BlockGrid::tail_at_cursor_to_string`, new, the same
`bounds_to_string` call as `contents_through_row_to_string` without the length gate). The
column-0 rule (a finished line asks nothing) is unchanged and applies to both.

Tests (`key_facts_tests.rs`): the failing test keeps every assertion, gains one that states its
premise (the grid is unstarted) and three more checks on the unstarted grid (a question under
other text, an ended line, plain text under an answered question). A new test covers the
app-prompt Precmd (a started grid) through a new helper, `TerminalModel::finish_block_with_ps1`
(`test_utils.rs`; `finish_block_with_exit_code` now shares its body).

## Rejected

- Changing the test to send a Precmd with a `ps1` only: it would pass, and hide the honor-PS1
  defect. That is weakening the test.
- Starting the grid at every Precmd: it changes what is drawn and measured for every block
  (`len`, heights, `prompt_number_of_rows`), far from this fact.
- Reading every grid through the cursor's row, started or not: probably the better rule (#1934
  did that for the subshell look), but it changes what the fact says for a running command whose
  cursor moved up, with no failing evidence. Left alone.

## Risk

Low. The fact is read in one place, at the prompt hold's cap. The new path is reached only for a
grid that is unstarted with the cursor off column 0, where the fact was always false before. A
false "asks for input" there writes the held keys to the pty in typed order, which is what a
plain terminal does.

## Open, not decided here

- With honor PS1 on, the question of a precmd function is in a grid of length 0, so it is probably
  NOT DRAWN until the prompt arrives (read from `len_displayed`, not checked in the app). The keys
  reach the `read` after this change; whether the user sees `pin: ` is a separate display question.
- No appci case runs with honor PS1 on. `gitbash_precmd_secret` proves the app-prompt mode only.
- zsh and fish were not traced; the fact is shell-neutral and the unstarted read covers any shell
  whose Precmd carries no `ps1`.
