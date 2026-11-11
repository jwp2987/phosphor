# Round 21, Wayland: editor reload, pane divider, and four rows that were test faults

Source: round 15 targeted run, build/r15a `b06800e98`, `app-test-linux-wayland` job 17936839261
(58 PASS, 16 FAIL, 20 NOREF). Evidence is the job's `run_slim` logs and shots. Nothing here was
compiled or run; see "Risk".

| case | mode | verdict | issue |
|---|---|---|---|
| `editor_daily` | both | APP | #2143 |
| `pane_resize_maximize` | both | APP | #2144 |
| `tab_lifecycle_keys`, `pane_wedged_after_tab_lifecycle_keys` | terminal | TEST (follow-on of `network_gate_switch`) | #2146 |
| `scroll_keys_position` | agent | TEST, plus an owner question | #2146, #2145 |
| `pane_not_local_after_meta_and_word_keys` | terminal | TEST (follow-on of `cb_ui_action_latency`; the runner typed `exit` in a root shell) | #2146 |
| `meta_and_word_keys` | both, NOREF | TEST: NO-REFERENCE was wrong, the accounting dropped a key the app logged | #2146 |

## 1. editor_daily: a file replaced by rename is not reloaded (APP, #2143)

New arm on Linux (round 14 stopped at the close question).

Cause. The app log of the run has no `editor_disk outcome=` line at all, with the target on.
`crates/warp_files/src/lib.rs` `FileModel::handle_watcher_event` reloads
`event.added_or_updated_set()`: `added` and `modified` only. inotify reports a rename as
`Name(From)`, `Name(To)`, `Name(Both)`; `crates/watcher/src/lib.rs`
`deduplicate_and_merge_raw_notifier_events` turns that into `moved {target <- source}`. The case
replaces the file the way git and editors do (new file, rename over). The target is in neither
set, so nothing is reloaded. FSEvents reports `Name(Any)`, which the merge turns into `added`:
macOS passes the same arm. The editor's own save is temp file + rename (`atomic_write.rs`), which
is why not even its echo is logged. The repository path (`handle_repository_update`) already
reloads `update.moved` targets.

Second gap, same merge: a `Name(To)` with no `From` before it (a file moved in from another
directory) matches no arm and is dropped.

Fix.
- `FileModel::handle_watcher_event` reloads `paths_to_reload(event)`: added, modified and the
  targets of renames. `reload_file_paths` already keeps only tracked files.
- The merge records an unpaired `Name(To)` as an added path (it exists now, nothing says where
  it came from).
- Kill switch `PHOSPHOR_FILE_RELOAD_ON_RENAME=0`: the old set.

Effect beyond the defect: on Linux and Windows the editor now hears the echo of its own save, as
macOS always has. #2087 tells it apart by the hash of the written text
(`is_external_disk_text`); a clean buffer takes an empty diff.

Rejected: adding rename targets to `added_or_updated_iter` itself. The code index and the repo
metadata model read it too and handle `moved` their own way; they would see a path twice.

## 2. pane_resize_maximize: keyboard resize pairs the focused pane with a hidden one (APP, #2144)

Changed by no round 19 / 21 agent; first judged at this step in round 15.

Cause. `app/src/pane_group/tree.rs`. A closed pane stays in its branch's `nodes` for the
undo-close window (60 s); `editor_daily`, the case before, closed four editor splits.
`adjust_pane_size_by_id` takes the divider at the focused pane's index in `nodes`;
`adjust_pane_size` moves flex between `nodes[idx]` and `nodes[idx + 1]`. With
`[left, right, closed...]` and the right pane focused the pair is (right, closed): the right pane
gives flex to a pane that is not drawn and shrinks. Measured: 80/80 columns became 81/79 after
two steps; the pointer drag right after gave 66/94. `render` hands divider k to the k-th DRAWN
node, so the two disagree whenever a hidden node sits among drawn ones. `render` also draws a
divider after the last drawn node when hidden nodes follow.

Fix. One rule for both paths, the rule `render` uses: the nodes that are drawn
(`has_visible_children || has_children_hidden_for_move`).
- by pane: the focused child and the next drawn one; the last drawn child pairs with the one
  before it (as before, among drawn nodes);
- by divider: divider k moves the k-th and (k+1)-th drawn nodes; no such pair, no change;
- `render` adds a divider only between two drawn nodes.
- Kill switch `PHOSPHOR_PANE_RESIZE_DRAWN_PAIR=0`: pairs by raw index, as before.

With nothing hidden all three are what they were.

Rejected: dropping closed panes on a keyboard resize, as the pointer drag does. It ends undo
for the closed pane and still leaves panes hidden for a job or a child agent in the pair.

## 3. tab_lifecycle_keys [terminal]: Settings was in front (TEST, #2146)

Not changed by round 19 / 21. `network_gate_switch` (terminal only) closes its page with Escape
twice, which does not close the Settings tab, and proved the shell in front only off Linux
(round 15 added the proof for macOS and Windows). Shot: the next case's probe line is in the
"Allowed origins" field. The runner's rows after it are right. Fix: the proof
(`shell_back_in_front`, which closes the tab with the close key) runs on Linux too, also when the
page half ends early.

## 4. scroll_keys_position [agent] (TEST #2146; owner question #2145)

Now with shots and the app log.
- "block-up then Home did not move the view": in the agent view a block selected by keyboard
  leaves the keyboard in the input (`select_most_recent_blocks`: "we don't want to divert the
  focus away from the input box"; transcript navigation never focuses the terminal). App log:
  Home is an `EditorAction` there, a `TerminalAction` in terminal mode. By design. The key that
  scrolls to the top at a prompt in both modes is Shift+Home (#2080, round 16). The agent pass
  uses it; the terminal pass keeps block-up then Home.
- "none of the job's last 8 lines is on screen": the agent view does not draw a background job's
  output (a background block belongs to no conversation, `Block::should_hide_block`). The view
  was at the end of what the conversation shows. Whether that is wanted is #2145; not decided
  here.

Test change. A control after the job starts reads whether its lines are drawn. Drawn: every
check as before. Not drawn in terminal mode: FAIL. Not drawn in agent mode: the two facts that
need drawn output ("the view holds against new output", "the newest output is back after a key")
are NOT judged and the row says so with #2145 (NOREF); still judged there: PageUp moves the
view, the view does not move during 3 s, Shift+Home reaches the top, one key brings the end of
the numbered lines back, the command is exact. When the owner's answer makes the agent view draw
the output, the full checks apply by themselves.

## 5. pane_not_local_after_meta_and_word_keys [terminal] (TEST, #2146)

`meta_and_word_keys` passed every check. `cb_ui_action_latency` before it opens a tab, presses
next-tab (with two tabs: the ORIGINAL tab comes to the front) and closes the tab in front: the
pass's own. The depth check after `meta_and_word_keys` found a shell that is not the recorded
root, called it nested and typed `exit` (app log), closing the only tab and the window; the shot
is black. New in round 15 because both cases failed earlier in round 14 and the check did not
run.

Fix.
- `cb_ui_action_latency` reads the shell pid before and proves the tab in front is NOT that
  shell before the close key, and IS that shell after. It goes back to the throw-away tab with
  next-tab when needed.
- `leave_nested` types `exit` only in a shell whose ancestors include the root shell
  (`ps -o ppid=`; POSIX). Otherwise it returns `"other"` and the row says another tab or pane is
  in front. Where ancestry cannot be read (Windows) nothing changes.

## 6. meta_and_word_keys NOREF, both modes (TEST, #2146)

"1 of 761 injected key(s) never reached the app's key trace". The key is Alt-. into the raw
reader; the app logged `keydown key="." ... alt=true ... seq=1206`. `injacct._LINE` reads the
name as `(\S+)\b`: no word boundary after `"."`, no match, and a chord that types nothing has no
`typed_characters` line. Counted per step against the case's own key list: every key is in the
trace. Same single key in round 14. Fix: the name is everything up to ` ctrl=`.
`ui_readline_keys` [terminal] (1 lost) is expected to clear too; `intl_compose_linux` (8) and
`upgrade_over_previous` (112) are other causes and are not touched.

## Risk

- Nothing was compiled. Rust: `rustfmt --check` only.
- #2143: a save on Linux and Windows is now followed by a reload event for the saved file. The
  path is the one macOS has always taken.
- #2144: `render`'s divider change alters which element follows the last drawn pane only when
  hidden nodes trail it.
- The agent row of `scroll_keys_position` will read NOREF until #2145 is answered.
