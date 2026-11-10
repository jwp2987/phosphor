# r14-routed: 'make a PR' in the plain terminal (#1981), mixed line endings on save (#1956), the nu agent-mode lab rows (#2019)

Branch `fix/r14-routed`, from integ/r14 `4bf571aae`. Owner decisions of 2026-11-10 for all three. Nothing here was
compiled or run against a build (the appci self-test was run).

## 1. #1981: 'make a PR' typed in terminal mode. Already the shell's; the row was typed inside a conversation (TEST)

**Decision.** A line that looks like a command with an installed first word, typed in TERMINAL mode, runs in the
shell. Request shape still goes to the agent (owner, 2026-11-04); inside an agent conversation the #1183 evidence rule
still stands.

**What the code does.** `crates/input_classifier/src/util/routing.rs` `route`: for `make a PR` with `make` installed
on the plain terminal no earlier rule takes the line (`is_unambiguous_shell_command` is vetoed by the #831 article
rule, `reads_as_request_to_agent` needs a request word and `a` is none, `util.rs:1137`), and
`if installed || head_is_keyword || head_is_path { return Decision::shell(Rule::InstalledHead) }` (`routing.rs:442`)
sends it to the shell. `installed` is the completer's description at the keystroke and the session's own answer at
Enter (`input_model.rs:1202`, `:1763`). The corpus already holds
`Case::cmd("make a PR", "#831 / #964 (installed make, ordinary arguments)")` (`routing_corpus.rs:677`), run at both
stages by `corpus_against_the_routing_procedure`.

**Where the row note came from.** The committed results of every round since r18d2 (`routing_rows` in
`test-results/*/app/*/results.json`): in the terminal-mode pass on macOS and Windows the line was typed with
`surface=AgentConversation`, rule `no-evidence-in-conversation (#1183)`. The table's own prose lines, typed just
before, open a conversation in the terminal-mode pane, and `Ctx.reset()` does not leave it. So the row never tested
the plain terminal. On Wayland the row has no surface at all (no `enter:` line: the keystroke verdict had settled),
and the destination changed between runs (shell in r18d2, w5, w7, r10a, r11a, r12a; agent in w3, w8), which is what
a line typed sometimes inside the left-over conversation and sometimes outside it looks like. `AMBIGUOUS_EXPECT`
pinned nothing for this line on the plain terminal (`"make a PR": (None, "agent")`), so either outcome passed.

**Verdict.** Not an app bug: the plain terminal already runs it in the shell. The gap is the test.

**Change.**
- Unit test `an_installed_head_with_english_arguments_runs_in_the_shell_on_the_plain_terminal` (routing corpus
  tests): the owner's lines at both stages, by `Rule::InstalledHead`; the same line in a conversation is the agent's;
  a request-shaped line with an installed head is the agent's on the plain terminal too.
- appci case `routing_plain_installed_head` (both modes, in `cases.PROVING`): leaves the agent view first (Escape,
  proven by `hop=view_mode mode=terminal`), types the lines at 10-25 ms/key with an instant Enter, and judges only
  lines whose routing row says `PlainTerminal`: the shell's rec.log must hold the exact line. A line whose first word
  is not a program on the machine has no verdict. Ends in the view it started in.
- `routing_table`: `make a PR` is pinned `shell` on the plain terminal, and a row with no `enter:` line now takes
  surface and rule from the line's `keystroke:` row, so the Wayland rows get a verdict instead of a note.

**Rejected.** Making a no-evidence line with an installed head run in the shell inside a conversation whenever the
pane "started" in terminal mode: that is #1183 reversed, which the owner kept.

**Risk.** None in the app (no app change). The new case can only fail where the app really sends an installed-head
line to the agent on the plain terminal.

## 2. #1956: a file that mixes LF and CRLF keeps each line's own ending (APP)

**Decision.** A save from the editor, and an accepted agent edit, keep EACH LINE'S OWN ending.

**Cause.** The editor buffer holds text LF-normalised and serialises it with one ending, the majority one
(`crates/editor/src/content/buffer.rs:996` `infer_line_ending`, `:2258` `text_with_line_ending`). Two paths still
wrote that serialisation over a mixed file:
- the code editor save: `app/src/code/local_code_editor.rs:603-628` and `:1793-1797` pass `editor.text(ctx)` to
  `GlobalBufferModel::save` / `rename_and_save` (`global_buffer_model.rs:991`, `:1005`);
- an accepted agent edit the user changed in the diff view: `accepted_update_text`
  (`crates/ai/src/diff_validation/apply.rs:138`) returns the buffer text whenever the deltas applied to the raw
  original are not the buffer's text.
The untouched-proposal accept is already byte-exact (#1925, `apply_deltas_to_text`); the remote accept is #2020 and
is not touched here.

**Fix.** One pure function, `ai::diff_validation::text_with_line_endings_of(original, text)`
(`crates/ai/src/diff_validation/endings.rs`): when `original` mixes line endings, `text` is re-ended line by line.
Lines are aligned by content (common prefix and suffix, then a longest-common-subsequence of the middle; a greedy
forward match when the middle is too large for the table). A line that is in the original keeps the original's
ending. A changed line takes the ending of the line it replaces (by position inside the changed run, the last
replaced line for the surplus); an inserted line takes the ending of the line before it, else after it. The final
newline state is the text's own. A file with one line ending is returned untouched, so nothing changes for it.
The function checks its own result: if it is not the same text as the input, line endings aside, the input is
returned.
- `accepted_update_text` uses it for the fallback (the user changed the proposal).
- `GlobalBufferModel` remembers the raw text of a mixed-ending file as the disk last had it (`FileLoaded`,
  `FileUpdated`, discard-and-reload, and what a save wrote) and `save` / `rename_and_save` pass the editor's text
  through the function. Only mixed files are remembered, so the memory cost is one copy of such a file.

**Rejected.**
- Per-line endings inside the buffer: the right model, but it touches every edit path of the editor (6 000 lines) and
  cannot be proven safe by reading.
- Re-reading the file at save time: a blocking read on the save path, and the guarded write already owns that read.
- A new `similar` dependency in the `ai` crate: `Cargo.lock` cannot be regenerated here.

**Risk.** Contained: the function returns its input unless the original is mixed, and can never change text (the
self-check). The greedy fallback (a changed middle of more than about a million line pairs) may give an unchanged
line a neighbour's ending; that is still no worse than today's majority rewrite. Lone-CR endings: a `\r` line
followed by an empty LF line would read back as one CRLF; the self-check catches it and the buffer text is written
as before. Not covered: the remote editor save (`save_remote_buffer`, `save_server_local`), which is #2020's side.
`warp_files` `line_endings_preserved` keeps `Mixed` compatible with everything; that blind spot is unchanged.

## 3. #2019: the four nu agent-mode lab rows are a decided limitation, reported by name (TEST + DECLINED)

**Decision.** A host whose login shell is fish or nu gets a plain ssh session (#2019, round 10). The agent tools need
a managed session, so the agent-mode arms on `nu` cannot be judged: a known, owner-decided limitation.

**Change.** `DECLINED.md` gets the entry (reason, what the user sees, condition to revisit). `cases_lab.py`
`plain_session_limit(shell, log_text)`: the premise holds only when the arm's login shell is fish or nu AND the app
log of this connect has `Received PlainSSHFallback hook` AND no managed-session line. Then the row is UNVERIFIABLE
with the DECLINED reference, the pane is left with `exit` (still checked), and the agent steps are not typed (in a
plain session they would be typed into the remote shell). In every other case the body runs as before: a bash arm
that got a plain session still fails on its own checks, and a nu arm that gets a managed session one day is judged
in full. Used by the four rows named in #2019 and by the two other agent-mode cases with a nu arm
(`lab_read_files_remote`, `lab_diff_apply_remote`), which have the same premise.

**Rejected.** Dropping `nu` from the cases' shell lists: the row would vanish, and with it the reminder and the
automatic full judgment once such hosts get a managed session.

**Risk.** A real regression on a nu host that still ends in a plain session is not seen by these rows; the
terminal-mode nu arms and `lab_ctrl_c_no_esc_i.nu` cover the plain session itself.
