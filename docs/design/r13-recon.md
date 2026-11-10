# Round 13 reconciliation: the annoyance list and the unlabelled v0.1.18 issues

Read-only audit, 2026-11-10, at integ/r13 `574f83868`. Nothing was compiled, run, labelled or closed for this note. No code changed.

## What the evidence is, and what it is not

- **Last committed run: round 11**, `test-results/2026-11-10_build_r11a_3049602519` (build/r11a `c61b6a3b8`), and round 10 before it
  (`2026-11-10_build_r10a_3048440714`, `5dd50d49c`). Every round 10 shard commit named below is an ancestor of both builds.
- **Round 12 has no results in this tree.** build/r12a is `501a18469`; nothing under `test-results/` is from it. Everything merged after
  `c61b6a3b8` (the round 12 and 13 fixes: #2038, #2041, #2043, #2044, #2045, #2050 and others) is worked, unproven.
- **macOS proves almost nothing in rounds 10 and 11**: 99 of 158 rows are NOREF in r11a (injector accounting, #2041; agent rows NOREF as a
  group). A macOS "pass" is named where one exists; otherwise macOS is unproven.
- **Windows ssh proves nothing**: every Windows ssh case has been a SKIP since round 10 (`fakesshd_win.py did not listen`, #2043).
- **The lab's helper arms prove nothing**: all NOREF (`APPCI_LAB_HELPER_BIN does not name a built phosphor-remote binary`; #2021 is after r11a).
- `editor_memory_7mb` and `flood_scrollback_find` are not in the fix tier and did not run in round 10 or 11.

States: FIXED-IN-CODE (an app change is merged), PARTLY, NOT WORKED, HARNESS-ONLY (only the tests changed; the verdict was a test gap or
test fault), MEASUREMENT-ONLY (only logging or a measuring case was added). "Proven" means a PASS row in a committed run. "Recommended
label" is for the coordinator; none was applied.

## Part 1: the 27 open v0.1.18 issues with no status label

| issue | title (short) | state | evidence | recommended label | next work |
|---|---|---|---|---|---|
| #985 | ssh keys to raw-input programs 40-100 ms late (proxy shares the connection) | NOT WORKED | No commit since it was reopened. The only related code is the opt-in `ObscureKeystrokeTiming=no` of #1243 (`app/assets/bundled/bootstrap/bash_body.sh:2366`, default leaves ssh's setting alone). No lab case times a raw key's arrival at the remote. | none | One measurement in the netem lab: key arrival in `cat` at the remote, managed vs plain ssh, with and without the #1243 setting. |
| #1280 | PowerShell 5.1 hooks, Enter to paint vs conhost | PARTLY, MEASUREMENT | `966dc49bb` (hand-built JSON) is in r10a and r11a. r11a Windows `latency_enter` terminal: 39.90 +/- 0.61 ms over conhost (62 on r18bj, 82 on r18ct), app share 1.79 ms; row NOREF (no ConPTY reference). | none | Owner decision already asked in the row: install Windows Terminal as the ConPTY reference. Then judge. |
| #1340 | Code editor 7 MB file memory | MEASUREMENT-ONLY (rounds 10-13) | `2da080eac`, `3e64e7114`, `37089d6af` add measurement. `editor_memory_7mb` is candidate tier: no row in r10a or r11a. Last figures are r18u28 (Linux inside target; Windows unjudged). | none | Run the case once (candidate tier). Owner decision on the opt-in open trim (issue note 2026-11-10). |
| #1393 | Git Bash 25 ms/key typeahead loses lines | NOT WORKED | Diagnostics only (2026-11-01). Nearest evidence: r11a Windows `fast_typing` PASS in both Git Bash arms (60 and 30 Enters, 0 lost before the app), and `enter_loss_control` PASS. That is not the 24-rep, 10-line protocol of the issue. | none | Rerun the issue's protocol (24 reps, hold on and off) on a current build with the text log on. If 0 losses, close with the run. |
| #1492 | Git Bash Enter +26-31 ms over conhost | MEASUREMENT-ONLY | r11a Windows `latency_enter` terminal@gitbash: 30.98 +/- 2.19 ms over conhost, app share 0.93 ms; G2: the bootstrap's hooks cost 21.2 ms outside the app; fork+exec 39 ms per process on the host; W1: burst ingest does not explain it. Row NOREF (no ConPTY reference). | none | The cost is the forking hooks of the bash bootstrap under msys. Either cut forks there or accept and document. |
| #1842 | Literal `\n` in UI strings | FIXED-IN-CODE | `73f64756a` (in r10a, r11a). Guard `script/check_ftl_escapes` runs in the gates job (`.gitlab-ci.yml:91`); `guards` succeeded in r10a and r11a. No screen check of the save modal or the callout. | fixed-untested | A look at the two screens in three languages, or accept the guard as the proof and mark verified. |
| #1900 | Synced origin lost behind an Enter | NOT WORKED | `queue_behind_enter_with` still offers with `Origin::Keyboard` (`app/src/terminal/input/enter_queue.rs:1377`); `Follow::Offer` still becomes `Origin::Replay(from)` (`app/src/terminal/input_queue/mod.rs:1709`). No commit names the issue. | none | Carry the origin in `AfterEnterInput` and in `Follow::Offer`. Touches enter_queue.rs (another agent's file in round 10). |
| #1907 | Child shells inherit an empty PS1 | FIXED-IN-CODE | `952fd184a` (in r10a, r11a). Seen working by accident in r10a Wayland: the dash drew `$ ` (issue note). `script/test_child_ps1` exists and is listed in `docs/testing.md:211`, but it is in neither `script/precheck` nor `.gitlab-ci.yml`, so no run executes it. No app case asserts it. | fixed-untested | Wire `script/test_child_ps1` into precheck. One app case: bash pane with an exported PS1, start dash, a prompt is drawn. |
| #1938 | Setup prefix lands in a dash `read x` (accepted, D7) | By design, documented | `725b77482`, `116ee6368`: comments at the write site and in subshell_ready.rs. No behaviour change was planned. | none; close as documented | None. |
| #1966 | Windows Git Bash question card "hangs" | HARNESS-ONLY, proven | `325b2e97e` (Windows RUST_LOG). r10a and r11a Windows: all ten `agent_question_*` cases PASS in agent@fake and agent@gitbash+fake. The hang was the log filter hiding the hops. | verified; close | None. |
| #1967 | No self-healing for a stuck agent turn | NOT WORKED | No commit, no code (`turn stuck` appears nowhere under `app/src`). `fix/r10-agentstuck` holds no commits of its own. | none | Design note first (enhancement). |
| #1969 | Remote bash 3.2: no in-band Tab | FIXED-IN-CODE | `e6f50afdc` (in r10a, r11a). `script/test_native_completions` is in precheck. macOS `remote_tab_no_helper` and `remote_tab_enter_no_helper`: NOREF in r10a and r11a, so no app proof. Follow-up #2002 open. | fixed-untested | The two macOS rows after #2041. |
| #1970 | ssh over a slow link unverified | HARNESS-ONLY, partly proven | `9654d5b6f`. r10a and r11a Linux lab: `lab_enter_no_network_wait` bash and zsh, 150 ms, no helper, PASS in both modes. `lab_enter_start` bash and zsh FAIL in r10a (37.5 and 16.5 ms over), PASS in r11a. fish NOREF (no Enter samples), nu FAIL (`exit` did not return), helper arms NOREF, macOS and Windows not run, 300 ms not in the quick matrix. | fixed-untested | fish and nu arms, the helper arms (#2021), a macOS client run (the owner's report was macOS). |
| #1971 | No keybinding for "Give control back to agent" | NOT WORKED | The button exists (`app/src/terminal/view/use_agent_footer/mod.rs:1270`); no binding, no commit. | none | Small: a default binding, shown on the button. |
| #1972 | Agent-run ssh: remote read never handed over | NOT WORKED | No commit. r10a and r11a Wayland `agent_interactive_ssh`: NOREF, "no hand-over (take-over notice only, #1811: the local OS cannot see the remote read)". So the user gets the notice, not the hand-over; the case cannot judge more. | none | Design: what proves a remote tty read without the helper. |
| #1976 | Copy of soft-wrapped lines untested | HARNESS-ONLY, proven (no app bug) | `09f7b00e7`, `25ccf61fd`. r10a and r11a Wayland: `ui_copy_wrapped_output` and `ui_copy_wrapped_drag` PASS in both modes. Windows Git Bash `ui_copy_wrapped_output` PASS in both modes. macOS NOREF or SKIP (no pointer tool). | verified; close | macOS row after #2041. |
| #1977 | macOS relaunch logs the keychain ERROR twice | FIXED-IN-CODE | `7e935fb15` (in r10a, r11a; unit test macOS only). macOS `agent_restart_kill_*`: NOREF in both rounds. | fixed-untested | A macOS relaunch row; expect the 10 s keyring toast on the test machine. |
| #1979 | Output residue: frames per line, hidden re-renders | MEASUREMENT-ONLY | `8ee398db4`. r11a `output_frames` agent arm, Wayland: shown tab 37.7 frames/s for 33.5 wakeups/s, 4 hidden tabs 2.0 frames/s, 0 in-place re-renders of hidden views. macOS (NOREF rows): 32 frames/s for 29 wakeups/s, hidden 2.5 frames/s, 3 re-renders. Windows PowerShell: 48.7 for 46.8, 0 re-renders. So the hidden-view claim is refuted and the visible one (about one frame per wakeup) holds. Terminal arm NOREF on Wayland and Windows (no `wakeup:` line). Windows Git Bash agent arm NOREF: hidden streams were not running, 140 in-place re-renders. | none | Fix the terminal arm's logging and the Git Bash arm. Then close the hidden half; the visible half is renderer work (retained target, damage) and needs its own decision. |
| #1980 | Typo in a conversation goes to the agent | FIXED-IN-CODE, proven | `0fe139887`, `b17e4c85b`. `routing_typo_head` PASS: Wayland agent (r10a, r11a), Windows agent@gitbash (r10a, r11a). macOS NOREF. | verified | Known gap in the issue note (session command set on the Enter-before-parse path). |
| #1983 | Ctrl-C `^[i` and "Session abandoned" tested only locally | PARTLY | Twin in the fix tier (`9654d5b6f`). r11a lab `lab_ctrl_c_no_esc_i`: bash and zsh PASS at LAN and 150 ms, both modes (bash 150 failed in r10a). fish and nu FAIL in r11a: that is an app defect, #2044 (Ctrl-C in a plain ssh session was SIGINT to the local ssh), `5f9de29a3`, after r11a, unproven. Log-line change `ececd9a2f`: unit tests only. | fixed-untested | Lab rerun for fish and nu with `5f9de29a3`. |
| #1985 | `flood_scrollback_find` red in agent mode | HARNESS-ONLY, unrun | `8512f0891` (find_scroll geometry when OCR is blind). The case has no row in r10a or r11a on any platform. App or harness is still undecided. | fixed-untested (keep unconfirmed-harness) | Name the case in one run. |
| #1987 | Network gate Allow did not persist | HARNESS-ONLY (test fault) | `e3eafad95`. r10a Wayland log: gate refused, keyboard Allow persisted once in settings.toml (issue note); the row then failed on a real defect, #2026. r11a `agent_gate_deny_allow`: NOREF (pointer Allow not found by OCR). No PASS row yet. Page keyboard and pointer hops (`2ef3d35ef`) are after r11a. | fixed-untested; drop unconfirmed-harness | A PASS row for the pointer Allow. |
| #1990 | Vertical tabs untested | HARNESS-ONLY, proven | `25ccf61fd`. r11a `ui_vertical_tabs_many` PASS in both modes on Wayland and on macOS (r10a failures were test faults, #2027). `ui_vertical_tabs_panel_click`: Wayland terminal PASS, agent NOREF (OCR). Windows not ported. | verified; close | Windows port if wanted. |
| #1991 | Line-clear prefix echo under a line editor (unconfirmed) | NOT WORKED (owner list) | Filed by the subshell merge `a0ca0729c`; nothing since. The issue names the log facts that would settle it (`subshell_written_judged echo_ended=`). Left alone per the round rules. | none | Read `echo_ended` on a bash or zsh setup in any existing run log. |
| #1995 | Input re-rendered in 16-31 % of terminal key frames | MEASUREMENT-ONLY (owner list) | `6d886d649` (in r10a, r11a). r11a: `latency_key_views_ab` terminal-mode burst keys re-render 1 view p50 on Wayland and Windows, 2 on macOS. The `input_notify_causes` metric is present only on agent cases and on macOS `latency_paint` (terminal: `close_voltron` 5, `on_focus` 5 in 5 frames); the terminal `latency_key_log` rows carry none. Cause still not named. Left alone per the round rules. | none | Report the metric on terminal `latency_key_log`, or state that zero causes were logged. |
| #1836 | Fuzzer proof on the pre-fix base | NOT WORKED | `docs/testing.md:3656`: "to run on the box, not done". | none | The box run on `ffdaa4536`. |
| #1353 | Seven key-holding queues | PARTLY | One PaneQueue holds everything. Step 8 is left: 20 `destination_mode()` call sites under `app/src`, and the legacy bodies at `app/src/terminal/view.rs:11260`, `:12632`, `:13760`. | none (owner 2026-11-05: stays open until step 8) | Retitle to "step 8 only" as its last note suggests. |

Counts: 27 issues. Proven by a run: 4 (#1966, #1976, #1980, #1990). App change merged, no proving row: 4 (#1842, #1907, #1969, #1977).
Tests only, part proven or unrun: 4 (#1970, #1983, #1985, #1987). Measurement only: 5 (#1280, #1340, #1492, #1979, #1995). Not worked: 8
(#985, #1393, #1900, #1967, #1971, #1972, #1991, #1836). Partly: 1 (#1353). By design: 1 (#1938).

The Sonnet group A set (#1907, #1938, #1966, #1985, #1987): #1966 proven, #1907 merged without a test that runs, #1938 comment only,
#1985 unrun, #1987 was not in group A's note (it came from fix/r10-wl-gate) and has no PASS row.

## Part 2: the 20 annoyance items

"Still hit?" is about the owner's setup (agent mode, autodetect on, vertical tabs, bash) and uses round 11 rows. Labels are as GitLab
shows them today.

| # | item | issues | state | does the daily user still hit it? |
|---|---|---|---|---|
| 1 | Typed keys discarded when an Enter gate gives up | #1996 fixed-untested (reverses #1661), #1793, #1844 fixed-untested, #1964 fixed-untested, #1997 open, #1900 not worked, #1862 fixed-untested | FIXED-IN-CODE, part proven | Probably not on Linux and Windows: `giveup_keys_reach_shell` PASS in every arm in r11a. The give-up write itself was not exercised by a run (the macOS row says so; unit tests cover it). The owner's macOS empty-remote-prompt case (#1793) has no proving row. #1997 is an open ordering hole. |
| 2 | Agent turn hangs on "Preparing question" | #1797, #1851, #1858-#1861, #1946 fixed-untested; #1966 proven test fault; #1967, #1836 not worked | FIXED-IN-CODE, proven on two platforms | No for the known paths: all ten `agent_question_*` PASS on Wayland and Windows (both panes) in r11a. macOS NOREF. A new lost-action path would still hang with no way out (#1967). |
| 3 | Key latency | #1958, #1843, #1848, #1962, #2031 fixed-untested; #1995 open | PARTLY proven | Linux: no. Key to pty write 0.64 ms in r11a (was 4.2 and 16.5); `agent_latency_key` PASS (streaming p50 2.91 ms against 3.13 ms in the terminal editor). macOS: unknown; `latency_paint` NOREF, Enter 3.73 ms over the 2 ms budget, `latency_key_log` failed on a gate fault (#2037). |
| 4 | ssh without the helper | #1800, #1801, #1793 fixed-untested; #1969, #1970 above; #985 not worked; #1933 fixed-untested; #2051, #2046 open; #2044 fixed-untested | PARTLY | Less on a Linux client: Enter does not wait on the network for bash and zsh at 150 ms; Tab works without the helper. The owner's macOS client and bash 3.2 remotes are unproven. Windows Git Bash managed ssh does not bootstrap in time (#2051). Raw-key lag (#985) was never measured. |
| 5 | Silent agent command handed to the user | #1796, #1811, #1812, #1882 fixed-untested; #1971 not worked | FIXED-IN-CODE, proven on two platforms | No false take-over on Linux and Windows (`agent_lrc_no_false_takeover` PASS in r11a, both Windows panes; `agent_noninteractive_no_takeover` PASS on Wayland). The hand-back is still a click. |
| 6 | Subshell setup fights the user | #1934-#1941, #1904, #1905, #1939, #1840, #1845-#1847 fixed-untested; #1907 above; #1938 by design; #1991 open; #2016, #2050 after r11a | PARTLY proven | Mostly not on Linux: `subshell_ready_*` and both `subshell_sh_probe` cases PASS on Wayland in r11a. Still failing there: `subshell_ready_zsh_newuser_install` (app, #2016; `2437a20c4` is after r11a, and `docs/design/r13-wlps1.md:53` reports a pass in a later run whose results are not in this tree). macOS NOREF. |
| 7 | Agent-run interactive commands leak or lose input | #1926, #1927, #1957, #1855, #1893 fixed-untested; #1972 not worked | PARTLY proven | Password no longer goes to the model on Linux and Windows PowerShell (`agent_interactive_password` PASS); vim PASS on Wayland. An agent-run ssh with a remote read is still not handed over (#1972). |
| 8 | Copy and paste | #1963, #1191, #1853, #1895, #1852 fixed-untested; #1976 proven; #2004 open | PARTLY | Wrapped-line copy is proven correct. `paste_instant_enter` PASS on Wayland and Windows. OSC 52 (#1963) and the 1 MB paste (#1191) have no row in round 10 or 11. |
| 9 | Startup waits | #1805, #1863, #1772, #1791 fixed-untested; #1977 above | PARTLY proven | Linux: no. `keyring_unavailable_startup` and `zero_network_startup` PASS on Wayland. macOS keychain log unproven. |
| 10 | Hidden-tab output frames | #805, #812 closed; #1979 above | MEASUREMENT-ONLY | Hidden tabs: no (0-3 in-place re-renders, about 2 frames/s). A visible busy tab still costs one frame per wakeup, by design. |
| 11 | Autodetection routing | #1980 proven; #1981, #1982 parked-owner | FIXED-IN-CODE, proven | A typo like `gti status` now runs in the shell (Linux, Windows Git Bash). The two questions wait for the owner. |
| 12 | Ctrl-C aftermath | #1798, #1672 fixed-untested; #1983 above; #2044 fixed-untested | PARTLY proven | Local: no (`ctrl_c_no_esc_i_local` PASS on Wayland). Managed ssh to bash or zsh: no (lab PASS). In an ssh session Phosphor does not manage (fish, nu), Ctrl-C ended the connection in r11a; the change is unproven. |
| 13 | Tab then fast Enter | #1659 fixed-untested | Proven for `tab_enter` | No: `tab_enter` PASS on Wayland and in all four Windows arms. Tab during a pending line report (#1659) has no case of its own. |
| 14 | Focus moving | #1643 fixed-untested; #1984 parked-owner | Unproven | Unknown: the Cmd-Shift-F case is macOS only and macOS rows are NOREF. |
| 15 | Scroll position | #1775 closed; #1985 above | Unrun | Unknown: the case did not run in round 10 or 11. |
| 16 | Notices and toasts | #1842 above; #1964, #1816 fixed-untested; #1986 closed | FIXED-IN-CODE, unproven | Probably not for the discard toasts (item 1). Toast dedupe and the settings text have no row. |
| 17 | Settings that may not stick | #1987 above; #2026 fixed-untested | PARTLY | Allow persists (seen in the r10a log). A real defect was found instead: a hand edit that restores the last written bytes is ignored (#2026), unproven. |
| 18 | Agent edits change more than asked | #1925, #1954, #1955 fixed-untested; #1956 open; #1837 fixed-untested | PARTLY proven | Local: no for final newline and CRLF (`agent_edit_endings`, `_accept`, `_conflict` PASS on Wayland). Mixed-ending files (#1956) not worked. Remote: `lab_fam_agent_edit_conflict` FAILED in r11a for bash and nu; the guarded-write changes are after r11a. |
| 19 | Windows-only | #1906, #1888, #1886, #1922, #1923, #1933, #1103 fixed-untested; #1280, #1492 above; #2038, #2051 | PARTLY proven | Ctrl+Break: no (PASS in the PowerShell pane). Subshell setup in Git Bash: no (`rc_questions`, `rc_clear` PASS). A secret typed into a precmd `read -s` still went wrong in r11a (`gitbash_precmd_secret` FAIL; #2038 after r11a). All ssh rows SKIP. Enter cost over conhost is the shell's hooks, not the app. |
| 20 | Smaller | #980 closed; #1340 above; #1990 proven; #1932, #1988, #1989 parked-owner | Mixed | Vertical tabs work in the tests. Editor memory has no current row. The rest are owner questions. |

## Labels outside the 27 that this pass suggests reviewing

Round 11 PASS rows exist for issues still labelled fixed-untested: #1958 (Linux key to pty 0.64 ms), #1805 (`keyring_unavailable_startup`),
#1906/#1888 (`ps_ctrl_break`, PowerShell pane), #1925 (`agent_edit_endings`, Wayland), #1796/#1811 (`agent_lrc_no_false_takeover`), #1926
(`agent_interactive_password`), the #1797 family (`agent_question_*`), #1996 (`giveup_keys_reach_shell`, with the caveat in item 1). Each
is proven on Linux and Windows only.

## Owner decisions this pass ran into (not decided here)

- #1280, #1492: install Windows Terminal as the ConPTY reference, or accept conhost as the only reference.
- #1340: delete the opt-in open trim, or keep it as an experiment that does not fail the row.
- #1979: is one frame per wakeup in a visible busy tab acceptable, or is renderer work wanted.
- #1938: close as documented.
- #1991, #1995: on the round's owner list, left alone.
