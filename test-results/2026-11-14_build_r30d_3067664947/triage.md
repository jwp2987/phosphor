# Triage: round 30 targeted run d, pipeline 3067664947 (build/r30d 2355e1da0)

The rerun after #2219 (Windows unit test waits) and #2213 (lab reader): Linux and macOS zsh lab arms, the full Windows chain.

**Stopped by the owner** at 11:46Z: the Windows app list was 126 cases (nearly the whole suite, ~6 passes each) plus 124 Windows lab cases, which is not a targeted run (coordinator fault). app-test-windows was cancelled after 221 min in the agent@gitbash pass; app-test-lab-windows was never played. Rule since: a targeted run lists only issue-tied cases (no soak/perf).

## Units
All nine suites green on Linux, macOS and Windows (test-windows green proves #2219).

## Lab
- **Linux lab: 9 pass, 9 fail. macOS lab: 23 pass, 18 fail, 29 NOREF.** Every fail is one of two known causes:
  - lab_chips.{bash,zsh,fish}.*.nohelper, lab_fam_git_read / lab_fam_run_command .{bash,zsh}.*.nohelper (both modes): 2-4 side-call exec channels per prompt. #2216 (APP); the legacy_generator diagnostic in this build named the extra channel, the fix (ff55a2b1d) merged after r30d, proven in r30e.
  - lab_tab_no_helper.zsh.{lan,150}.nohelper (macOS, both modes): no in-band candidates. #2196: TEST (the lab zsh user never runs compinit, so the #2174 bootstrap answers empty by design); fixed in a997653cf (zshc arm). The wasted in-band round trips on zsh without compinit: #2225.
- Proven: lab_connect_typeahead.zsh.* PASS on Linux and macOS (#2213).

## App (Windows, partial: PowerShell, Git Bash, pwsh7 terminal passes; PowerShell agent pass; agent@gitbash cut off)
- routing_table_powershell [terminal] FAIL: TEST #2220 (no installed= for 'make a PR'); fixed bf2fa0000.
- routing_table [terminal@gitbash] FAIL and routing_plain_installed_head [terminal@gitbash] FAIL: #2200 (owner closed as is 2026-11-13; bare `date`/`pwd`/repeated `ls` 'Undecided by model', `date` went to the agent and the pane stayed in the conversation). Duplicate #2221 closed into it.
- expert_fuzz [terminal@gitbash] FAIL: APP #2115 (reopened): Left keys between the hand-off request and the report hit the partial editor buffer and are clamped (fz1ywjnt -> fz1wyjnt). In the #2209 change set.
- ui_completions_as_you_type_local [terminal@gitbash] FAIL: APP #2222 (no completion for a path argument in Git Bash; failing since r24a, filed now). Diagnostics on fix/2222-gitbash-path-completions, held behind #2209.
- selection_modes [terminal@gitbash] FAIL: APP #2223 (WRONG COMMAND: after the Ctrl-C hand-off gives up, declined Enters are dropped and four lines merge into one). In the #2209 change set. #2117: the copy half proven, the interrupt of the msys sh loop was not observed (stays open).
- editor_memory_7mb [terminal@gitbash] FAIL: APP-candidate #2224 (10 MB private held after closing the .rs file vs -11.5 MB in r29a; band n=1).
- Proven: agent_basics_powershell [agent] PASS (#2059).

## Bands
Held for the owner (WOULD LOOSEN from collect, not applied): macos agent latency_first_key_after_idle gap1s/gap5s, macos terminal ctrl_c_d ctrl_c_flood and latency_first_key_after_idle gap2s; windows agent latency_enter, agent@gitbash latency_enter / latency_key_log, agent@pwsh7 ctrl_c_d, terminal@gitbash editor_memory_7mb rs private (#2224), terminal@gitbash subshell_ready_rc_questions.

## Not proven by this run
Windows lab (never played), the rest of the agent@gitbash pass: r30e (trimmed) and the round after #2209.
