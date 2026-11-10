# Round 14 review: gaps in app testing for how the app is USED

Read-only review, 2026-11-10, at integ/r14 `4bf571aae`. Nothing was compiled, run, labelled, filed or closed for this note. No code or
test changed. Owner question: "are there any app tests that are missing, or gaps in our app testing, for how the app is USED?"

## 1. Summary

Yes. The gaps fall into three groups, and the largest is not missing cases.

1. **Most daily-use cases exist but have not run on the current code.** 75 of the roughly 185 registered app cases are full-tier only.
   The last committed full-tier run is the v0.1.17 release candidate (`test-results/2026-11-05_ssh_r18u28_*`, commit `3d6d6ed21`).
   Since then: 826 commits, 786 files and about 127,000 inserted lines under `app/` and `crates/`, much of it in the input, hand-over,
   ssh and agent paths. Session restore, crash restore, window resize, split panes, Ctrl-R, ghost-text accept, live settings, the 1 MB
   paste, bracketed paste, the ssh password prompt, the remote session cases, wide characters and the markdown viewer on large files
   are all "worked, unproven" on today's tree. No candidate tier has been played in rounds 9 to 13. The in-process GUI suite
   (`crates/integration`, about 257 scenarios) and `usage-test-*` have no row in any committed result.
2. **Whole platform and pane columns are empty or thin.**
   - Linux X11: no app run since r18u28, and that run was partial: terminal mode only (90 terminal rows, 3 agent rows), 19 failures.
   - Windows PowerShell pane (the default Windows shell): 40 case names judged in r12a. Autodetect routing (`routing_table`,
     `routing_typo_head`), `agent_mode_switch`, `agent_input_exact` and `agent_stream_cancel` all SKIP there; they run in Git Bash only.
   - Windows as a whole: no pointer synthesis, no window resize, no OCR. Every mouse, resize, find, split-pane, settings-page and
     vertical-tabs case SKIPs. 169 of 336 rows in r12a are SKIP.
   - macOS: no pointer tool, no usable quit key, no close key. Mouse selection, right-click, panel click, clean-quit restarts,
     `quit_24_tabs` and `soak` SKIP. The no-network, keyring, plugin and network-gate cases are Linux only.
   - The ssh lab: every helper arm is NOREF (40 rows a run); 18 lab families have never produced a row; lab on macOS and Windows and
     the real-use replay have never been played.
3. **Features a daily user touches that no app case drives at all** (section 4). The ones nobody had named: scrolling (wheel, page
   keys, staying put while output streams), Find outside the one flood case, Up-arrow history recall, tab close / undo-close / rename,
   close-with-running-process confirmation, link and path clicks, switching model or provider mid-conversation, the Anthropic-format
   provider (the owner's Claude proxy), auto-run permission settings, resuming an agent conversation after restart, attention state
   for an agent blocked in a hidden tab, desktop notifications, focus loss and return, minimise / fullscreen, sleep or long freeze,
   reflow of existing output on resize, rendering correctness (colours, wide cells, cursor), DPI scale other than 1, job control
   (Ctrl-Z, fg), Alt/Home/End/word keys, hostile or binary output, a true first run on default settings, MCP (its one case has never
   been judged), keybinding changes, installer and single-instance behaviour.

False coverage is real and listed in section 6: 20 entries that cannot fail today (always SKIP or NOREF on every platform, or an
oracle that accepts any outcome).

Recommended order: first run what exists (section 7.0, no new code), then build the 50 cases of section 7 in five waves.

## 2. Evidence base

| source | what it is | used for |
|---|---|---|
| `test-results/2026-11-10_build_r12a_3050758880` | newest committed run, fix tier (SMOKE + PROVING), build/r12a `501a18469`; Wayland, macOS, Windows, lab on Wayland | "judged now" |
| `test-results/2026-11-05_ssh_r18u28_{linux-wayland,linux-x11,macos,windows}` | last committed FULL-tier run, v0.1.17 RC `3d6d6ed21` | "judged at v0.1.17 only" |
| `test-results/INDEX.md`, every `manifest.json` | which CI jobs ever ran | suites with no run at all |
| `script/realapp/appci/cases*.py`, `winmap.py`, `cases.SMOKE` (21), `cases.PROVING` (88), `PROVING_LAB` | what is registered and in which tier | inventory |
| `crates/integration/src/test/*.rs`, `tests/integration/ui_tests.rs` | in-process GUI scenarios | "in-process only" |
| `app/src` action names (357 binding action names; 144 `workspace:`, 61 `terminal:`, 17 `pane_group:`) | what a user can do | feature inventory |
| `docs/testing.md` 2.9, 3.5, 5, 7; `docs/design/r13-recon.md`; `docs/design/user-annoyance-review.md`; `DECLINED.md` | stated matrix, known gaps, decisions | cross-check |

Limits of this review, stated once:

- Round 13 has no committed results. Everything merged after `501a18469` is worked, unproven; r12a is the newest evidence.
- "Not tested" for a feature means: no app case names it (keyword and docstring read of every case file) and no committed row
  exists. A case could exercise a feature in passing without naming it; where I could not rule that out the row says "none found".
- Rows are counted as the run recorded them. NOREF and SKIP are "not tested" throughout, as the brief requires.
- I did not verify behaviour of the app; this is about test reach, not about defects.

Counts in r12a (rows; one row per case, mode and pane):

| job | pass | fail | skip | noref | judged share |
|---|---|---|---|---|---|
| Linux Wayland | 118 | 2 | 34 | 8 | 74% |
| macOS | 111 | 0 | 44 | 5 | 69% |
| Windows (both panes) | 131 | 29 | 169 | 7 | 48% |
| ssh lab (Wayland) | 29 | 21 | 0 | 50 | 50% |

PROVING, by case name (88 names): judged on Wayland 66, macOS 55, Windows 43; SKIP on every arm: Wayland 17, macOS 29, Windows 42.
Windows PowerShell pane alone: 40 names judged (list in section 3.2).

## 3. What exists

### 3.1 Layers

| layer | size | runs where | last committed evidence |
|---|---|---|---|
| unit / in-process (`test-linux`, `test-warpui-core`, `test-workspace-rest`) | about 16,400 tests | box every fix tier; macOS and Windows twins manual | r12a: 16,423 pass, 4 fail (all four triaged as test faults) |
| agent fuzzer (`agent_fuzz`, L0/L1) | inside `test-linux` | box | r12a: 2 oracle faults (#2010), audited in r13 |
| `crates/integration` | about 257 GUI scenarios, synthetic events inside the app | Linux box, Xvfb, candidate / release tier | none: no manifest shows the job with a result |
| `usage-test-linux`, `usage-test-macos` | NDJSON runner over the integration scenarios | candidate / release | none |
| shell harnesses `script/test_*` | 25 scripts | `precheck` | not archived |
| app cases, fix tier | SMOKE 21 + PROVING 88 | Wayland, macOS, Windows (PowerShell and Git Bash passes), fake-model pass | r12a |
| app cases, full tier only | 75 names | all four desktop jobs | r18u28 (v0.1.17 RC) |
| ssh lab | 28 families x shell x link x helper arm | Linux only in practice | r12a: 5 families, no-helper arms only |
| real-use replay (`lab_replay`) | 1 case per mode | `app-test-replay-linux` | none |
| cargo-fuzz, loom, mutants, bench | release tier | box | r18d8 / r18e4 (2026-11-03) |

### 3.2 What the Windows PowerShell pane judges (r12a)

`mode_proof`, `cadence`, `fast_typing`, `paste_instant_enter`, `tab_enter`, `ctrl_c_d`, `enter_loss_control`, `key_delivery`,
`md_viewer`, the three SMOKE ssh cases (PASS in this pane; the same three FAIL in the Git Bash pane), `giveup_keys_reach_shell`, `output_frames`, `latency_key_views_ab`,
the four `ps_*` cases, and in the fake pass: `agent_fault_*` (6), `agent_question_*` (10), `agent_restart_kill_*` (3),
`agent_interactive_password`, `agent_lrc_no_false_takeover`. Nothing in that list sends a typed line through the autodetect and
proves where it went.

## 4. Matrix: feature area x platform

Codes: **J** judged in r12a (fix tier). **J!** judged in r12a and failing. **j17** last judged at the v0.1.17 RC (full tier only, not
run since). **j17!** same, failing then. **N** ran but only NOREF. **S** SKIP on that platform. **0** no app case. **-** does not
apply. Where the two modes differ: `t` terminal, `a` agent. "In-process" names `crates/integration` or unit tests that touch the
area; none of those has a committed result for integration, so it is context, not proof.

X11 cells are from r18u28, terminal mode only: agent mode has never run on X11 in a committed result.

### 4.1 Typing, paste, keys

| area (cases) | Wayland | X11 | macOS | Win PS | Win Git Bash | in-process |
|---|---|---|---|---|---|---|
| fast typing, instant Enter (`fast_typing`, `cadence`) | J | j17 | J | J | J | typeahead.rs |
| rollover, typo fixing (`rollover`, `expert_fuzz`, `keydown_count`) | j17 | j17! | j17 | j17 | j17 | - |
| paste + Enter (`paste_instant_enter`) | J | j17 | J | J | J | - |
| 1 MB paste (`big_paste`) | j17 | j17 | j17 | S | j17! | paste unit tests |
| multi-line and no-bracket paste (`paste_multiline_prompt`, `paste_bracketed_off`) | j17 t | j17 | j17 t | j17 / S | j17 t | - |
| paste into agent input (`paste_agent_input`) | j17 | 0 | j17 | S | j17 | - |
| Tab then Enter (`tab_enter`) | J | j17 | J | J | J | - |
| Tab menu, ghost accept, Ctrl-R (`ui_tab_menu`, `ui_ghost_accept`, `ui_ctrl_r`) | j17 | j17 | j17 | S | j17 | input.rs, history.rs |
| completions as you type (`ui_completions_as_you_type_local`) | J t | 0 | J t | S | N (no OCR) | yes |
| Up-arrow history recall, local | 0 | 0 | 0 | 0 | 0 | history.rs |
| Ctrl-C / Ctrl-D (`ctrl_c_d`, `ctrl_c_no_esc_i_local`) | J | j17 N | J | J | J / N; `no_esc_i` S | ctrl_d.rs |
| Ctrl-Z, bg, fg (job control) | 0 | 0 | 0 | - | 0 | none found |
| Alt/Meta, Home/End, word movement and kill keys | 0 (only BackSpace, Left, Ctrl-W in `expert_fuzz`, j17) | 0 | 0 | 0 | 0 | editor unit tests |
| kitty keyboard protocol | 0 | 0 | 0 | 0 | 0 | keyboard_protocol.rs |
| wide characters typed, pasted, edited (`intl_wide_*`) | j17 | j17! (3 of 4) | j17 | j17, edit j17! | j17 | - |
| dead keys / compose / AltGr | j17 (`intl_compose_linux`) | j17 | j17 (`intl_deadkey_option`) | j17 | j17 | - |
| IME (CJK) | 0 | 0 | S (needs owner: input sources) | 0 | 0 | - |
| input while the multi-line submit dialog is open (#1652) | none found | - | - | - | - | unit |

### 4.2 Terminal core and screen

| area | Wayland | X11 | macOS | Win PS | Win Git Bash | in-process |
|---|---|---|---|---|---|---|
| alt-screen programs: vim, less, htop, tmux (`alt_screen`, `alt_screen_programs`) | j17 | j17 | j17 | S | j17 / N | - |
| mouse reporting to a program (`alt_screen_mouse_reporting`) | j17! | j17! | S | S | S | context-menu tests |
| scroll: wheel, page keys, scroll-to-top | 0 | 0 | 0 | 0 | 0 | none found |
| scroll position held while output streams | 0 | 0 | 0 | 0 | 0 | snackbar tests only |
| Find (`flood_scrollback_find` only) | j17 t, j17! a | j17 | j17 t, j17! a | S | S | find tests |
| Find in an ordinary scrollback, next / previous, focus back to input | 0 | 0 | 0 | 0 | 0 | find tests |
| output flood while typing (`flood_typed_line`, `flood_typing_latency`) | j17 | j17 | j17 | S | S | - |
| frames per output line (`output_frames`) | N t, J a | 0 | J | N / J | N / J! | - |
| window resize, typing exact (`ui_resize_typing`) | j17 | j17 | j17 | S | S | size unit tests |
| reflow of existing output, SIGWINCH to programs, rapid resizes during output | 0 | 0 | 0 | 0 | 0 | size unit tests |
| rendering correctness (SGR, truecolor, wide cells, emoji, cursor, box drawing) | 0 (screenshots are for people) | 0 | 0 | 0 | 0 | `terminal/ref_tests`, grid renderer unit |
| hostile or binary output (cat a binary, title spam, huge OSC 8) | 0 | 0 | 0 | 0 | 0 | cargo-fuzz (release tier, failed twice) |
| long OSC through msys (`gitbash_long_osc_reaches_app`) | - | - | - | S | J | - |
| OSC 52 clipboard (#1963) | 0 | 0 | 0 | 0 | 0 | unit |
| OSC 8 and URL / path click | 0 | 0 | 0 | 0 | 0 | osc8_hyperlinks.rs |
| bell, window / tab title from OSC 0/2 | 0 | 0 | 0 | 0 | 0 | tab_title unit |
| clear screen / clear blocks | 0 as a subject | 0 | 0 | 0 | 0 | pane_group state tests |

### 4.3 Selection, clipboard, pointer

| area | Wayland | X11 | macOS | Win PS | Win Git Bash | in-process |
|---|---|---|---|---|---|---|
| keyboard block copy, wrapped lines (`ui_copy_wrapped_output`) | J | 0 | J | S | J | - |
| pointer drag copy, wrapped (`ui_copy_wrapped_drag`) | J | 0 | S | S | S | - |
| double-click / drag copy (`ui_mouse_select_copy`) | j17! | j17 | S | S | S | selection tests |
| triple click, shift-click, selection across blocks, selection while streaming | 0 | 0 | 0 | 0 | 0 | selection tests |
| right-click paste (`ui_right_click_paste`) | j17! | j17! | S | S | S | context-menu tests |
| PRIMARY selection, middle-click paste, copy on select | 0 | 0 | - | - | - | yes |
| drag and drop a file (`ui_drag_drop_file`) | S | S | S | S | S | - |
| clipboard after quit (#1693) | none found | - | - | - | - | unit |
| image paste into the agent input | 0 | 0 | 0 | 0 | 0 | - |

### 4.4 Tabs, panes, windows

| area | Wayland | X11 | macOS | Win PS | Win Git Bash | in-process |
|---|---|---|---|---|---|---|
| many streaming tabs (`tabs_streaming`) | j17 | j17 | j17 | j17 | j17 | - |
| vertical tabs, many tabs by number key (`ui_vertical_tabs_many`) | J | 0 | J | S | S | - |
| vertical tabs panel click (`ui_vertical_tabs_panel_click`) | J t, N a | 0 | S | S | S | - |
| tab close, undo close, rename, move, close others | 0 as a subject (soak opens and closes a tab, Linux, j17) | 0 | 0 | 0 | 0 | tab tests, tab_groups.rs |
| close a tab or window with a running process: confirmation | only inside `quit_24_tabs` (j17, Linux) | j17 | S | S | S | yes |
| split panes and focus keys (`ui_split_panes`) | j17 | j17 | j17 | S | S | pane tests |
| pane resize, maximise, move to own tab | 0 | 0 | 0 | 0 | 0 | pane tests |
| two windows (`tabs_two_windows`) | j17 | j17! | j17 | j17 | j17 | workspace.rs |
| drag a tab (reorder, between windows) | 0 | 0 | 0 | 0 | 0 | yes |
| focus loss and return, focus reporting | 0 | 0 | 0 (the #1643 case is macOS only and was NOREF) | 0 | 0 | focus_env unit |
| minimise / restore, fullscreen, occluded window | 0 | 0 | 0 | 0 | 0 | - |
| second monitor, DPI change, scale other than 1 | 0 (every run: scale 1) | 0 | 0 | 0 | 0 | - |
| sleep / wake, long freeze | 0 | 0 | 0 | 0 | 0 | - |

### 4.5 Sessions, shells, ssh

| area | Wayland | X11 | macOS | Win PS | Win Git Bash | in-process |
|---|---|---|---|---|---|---|
| fish, nu as subshell (`shell_fish`, `shell_nu`) | J | j17 | J | S | S (not installed) | - |
| zsh, pwsh as subshell (`shell_zsh`, `shell_pwsh`) | j17 / S | j17 / S | j17 | S | S | - |
| pane shell override (`startup_shell_pane`) | S | S | S | S | S | preferred-shell test |
| cmd, pwsh 7, WSL as the pane | - | - | - | 0 | 0 | wsl unit |
| subshell setup (`subshell_ready_*`, `subshell_sh_probe*`, 13 cases) | J (9), S (3) | 0 | J (8), S (3) | S | J (2; `rc_clear` J! agent) | subshell.rs |
| real sudo / su boundary, docker (`sudo_su_real_shell`, `*_relay_shape`, `*_docker_t_without_i`) | S | S | S | S | S | - |
| precmd secret (`precmd_secret_honor_ps1`, `gitbash_precmd_secret`, `ps_read_host_secure`) | J! | 0 | N | J | J | view unit |
| ssh connect typing, nested (`ssh_connect_typing`, `ssh_enter_then_immediate_command`, `ssh_nested_typeahead`) | J | j17 | J | J | J! (all three, both modes) | ssh.rs |
| ssh failing / hanging host, password prompt | j17 | j17 | j17 | j17 / S | j17 | - |
| remote session with and without the extension (`remote_session_*`) | j17 | j17 / j17! | j17 | S | N | - |
| remote Tab without the helper (`remote_tab_*_no_helper`) | J | 0 | J | S | J! | - |
| remote here-doc paste (`remote_heredoc_paste`) | j17 | j17 | j17 | S | j17! | - |
| ssh drop and reconnect (`ssh_drop_reconnect`) | j17! | j17! | j17 | S | S | - |
| Git Bash managed ssh (`ssh_gitbash_*`) | - | - | - | S | J! (2), J (1), 1 no row yet | - |
| ssh wrapper pinned on (`ssh_*_wrapper`) | S | S | S | S | S | - |
| lab, no helper: Enter at 150 ms, Ctrl-C, agent remote, remote edit conflicts | J / J! (bash, zsh mostly J; fish, nu J! or N) | - | 0 (never played) | 0 | 0 | - |
| lab, helper arms | N (all 40 rows) | - | 0 | 0 | 0 | helper unit |
| lab families: chips, path detect, remote Tab dir, file tree, diff panel, git write, codebase index, skills, read files, remote edit, helper install, password prompt, drop reconnect, replay | 0 (never a row) | - | 0 | 0 | 0 | - |

### 4.6 Agent

| area | Wayland | X11 | macOS | Win PS | Win Git Bash | in-process |
|---|---|---|---|---|---|---|
| routing of typed lines (`routing_table`, `routing_typo_head`) | J | j17 t | J | S | J | classifier unit |
| mode switch, exact agent input (`agent_mode_switch`, `agent_input_exact`) | J | j17 t | J | S | J | - |
| stream and cancel, real provider (`agent_stream_cancel`) | J | 0 | J | S | J | - |
| approve / reject a command, real provider (`agent_approval`) | j17! | 0 | j17 | S | j17! | fuzzer |
| approval card after a pause (`approval_card_pause`) | j17 | 0 | j17 | S | S | - |
| auto-run permission settings (allow / deny lists, autonomy) | 0 | 0 | 0 | 0 | 0 | fuzzer with the allowlist on |
| question card (`agent_question_*`, 10) | J | 0 | J | J | J | fuzzer corpus |
| model faults and restarts (`agent_fault_*`, `agent_restart_kill_*`) | J | 0 | J | J | J | - |
| clean-quit restarts (`agent_restart_quit_*`) | J | 0 | S | S | S | - |
| file edits (`agent_edit_*`, 5) | J | 0 | J | S | S | unit |
| interactive programs under hand-over (`agent_interactive_*`) | J (password, vim, less), N (ssh) | 0 | J, N (ssh) | J (password only) | S | - |
| latency and steer (`agent_latency_*`, `agent_steer_fast`) | J | 0 | J | S | J | - |
| many agent tabs (`agent_multitab_streaming`) | J | 0 | J | S | S | - |
| MCP (`agent_mcp_faults`) | S | 0 | S | S | S | mcp unit |
| network gate (`agent_gate_*`, `network_gate_*`) | J (3), N (2) | 0 | S | S | S | unit |
| slash and @ pickers, file mention (`cb_agent_pickers`, `cb_agent_file_mention`) | S / j17 | 0 | j17 | S | j17 | - |
| model or provider switch mid-conversation | 0 | 0 | 0 | 0 | 0 | model-selector tests |
| Anthropic-format provider (the Claude proxy) | 0 (none found) | 0 | 0 | 0 | 0 | provider unit |
| resume a finished conversation after restart; conversation list | 0 | 0 | 0 | 0 | 0 | restored-block tests |
| ask about a block / attach terminal output as context | 0 | 0 | 0 | 0 | 0 | - |
| attention state for an agent blocked in a hidden tab | 0 | 0 | 0 | 0 | 0 | - |
| long conversation (hundreds of turns) cost | 0 judged (P3 is measured-only) | 0 | 0 | 0 | 0 | - |
| markdown answer rendering (`md_viewer_ai_answer`) | j17 (fails only on a crash) | 0 | j17 | S | S | - |
| bundled CLI-agent plugins offline (`claude_plugin_offline`, `agent_plugins_offline`) | J | 0 | S | S | S | - |

### 4.7 Settings, state, start, stop

| area | Wayland | X11 | macOS | Win PS | Win Git Bash | in-process |
|---|---|---|---|---|---|---|
| startup time (`ui_startup_time`; budget: warm median over 20 s fails) | j17 | j17! | j17 | S | S | - |
| true first run on default settings (no harness profile toggles) | 0 | 0 | 0 | 0 | 0 | - |
| no network at start (`zero_network_startup`) | J | 0 | S | S | S | - |
| no keyring (`keyring_unavailable_startup`); macOS keychain (#1977) | J | 0 | S | S | S | unit |
| settings page opens (`cb_settings_ui_open`) | S (Pillow missing) | S | j17 | S | S | settings_navigation.rs |
| change a setting in the UI and it sticks | Network page only (`network_gate_switch` N, `agent_gate_deny_allow` N) | 0 | 0 | 0 | 0 | page unit tests |
| live settings file: theme, font size, vertical tabs (`ui_settings_live`) | j17 | j17 | j17 | S | S | hot_reload.rs |
| font zoom keys | 0 | 0 | 0 | 0 | 0 | yes |
| settings file from another version, truncated (`cb_settings_migration`) | j17 | j17! | j17 | S | S | yes |
| settings write failure (read-only, disk full) | 0 | 0 | 0 | 0 | 0 | unit |
| keybinding change takes effect | 0 (only inside the upgrade fixture, never run) | 0 | 0 | 0 | 0 | - |
| session restore: tabs and cwd (`session_restore`) | j17 | j17 | j17 | S | j17 | session_restoration.rs |
| restore shows earlier output and agent conversations | 0 | 0 | 0 | 0 | 0 | yes |
| history survives restart (`cb_state_relaunch`) | j17 | j17! | j17 | S | S | - |
| kill -9 and corrupt database (`crash_kill9_restore`, `crash_corrupt_db`) | j17 | j17 | j17 | j17 | j17 | - |
| quit with 24 tabs (`quit_24_tabs`) | j17 | j17 | S | S | S | - |
| window close button, SIGTERM, logout | 0 | 0 | 0 | 0 | 0 | close-policy unit |
| upgrade from v0.1.17 (`upgrade_from_v0_1_17`, `upgrade_over_previous`) | no row / S | S | S | S | S | fixtures |
| installer, uninstall, single instance, `phosphor <path>`, URI handler, default terminal | only `editor_edit_save` opens a path (j17) | j17 | 0 | j17 | j17 | uri unit |
| report a problem (`cb_report_problem`) | J | 0 | J | S | S | unit |
| no auto-update (`cb_no_autoupdate`) | j17 | j17 | j17 | j17 | j17 | guard |

### 4.8 Editor, viewer, panels

| area | Wayland | X11 | macOS | Win PS | Win Git Bash | in-process |
|---|---|---|---|---|---|---|
| markdown viewer, small (`md_viewer`) | J | j17! | J | J t | J t | - |
| markdown viewer, 2 MB (`md_viewer_big`) | j17 | j17 | j17 | S | S | - |
| code editor edit / save / paste (`editor_edit_save`, `editor_paste`, `cb_editor_mac`) | j17 | j17 | j17 | j17 | j17 | - |
| editor memory, 7 MB file (`editor_memory_7mb`) | j17! | j17! | j17 | j17! | j17! | - |
| editor: find, go to line, unsaved-close prompt, external change | 0 | 0 | 0 | 0 | 0 | goto_line.rs |
| file tree, global search, code review panel (local) | 0 | 0 | 0 | 0 | 0 | file_tree.rs, code_review.rs |
| command palette (`ui_palette`: one action) | j17! | j17 | j17 | S | j17 | palette tests |
| workflows, launch configs, env vars, rules, notebooks | 0 | 0 | 0 | 0 | 0 | yes (several files) |
| secret redaction on / off (owner: regexes set, redaction off) | 0 | 0 | 0 | 0 | 0 | secrets.rs |
| notifications (command finished, agent needs input) | 0 | 0 | 0 | 0 | 0 | 1 mention |
| toasts: text, dedupe | only the network-gate toast (J agent, Wayland) | 0 | 0 | 0 | 0 | unit |
| accessibility tree | 0 | 0 | 0 | 0 | 0 | - |

### 4.9 Performance

| area | Wayland | X11 | macOS | Win PS | Win Git Bash |
|---|---|---|---|---|---|
| Enter latency (`latency_enter`) | N t, J a | j17 N | J | N | N |
| key to frame from the log (`latency_key_log`) | N | N | N | S | N |
| key to pixel (`latency_paint`, `latency_paint_ab`) | S | S | J | S | S |
| Ctrl-C under flood A/B (`ctrl_c_flood_ab`) | S | S | J | S | S |
| per-key view re-render (`latency_key_views_ab`) | J | 0 | J | J | J |
| first key after idle (`latency_first_key_after_idle`) | j17 t, j17! a | j17! | N | S | j17 / N |
| idle CPU (`idle_cpu`) | j17 | j17 | j17 | j17 | j17 |
| soak, memory growth (`soak`) | j17 | j17 | S | S | S |
| memory per tab, throughput numbers | 0 judged | 0 | 0 | 0 | 0 |
| real GPU rendering | 0 (llvmpipe or the box render node only) | 0 | real | VM | VM |

## 5. Ranked gap list

Impact = how often a daily user (owner setup: agent mode default, autodetect and NLD on, Ollama and the Claude proxy, vertical tabs,
bash; Linux Wayland / X11, macOS, Windows PowerShell and Git Bash) meets the path, times how bad a defect there is. H / M / L.
"Named" = already named by `r13-recon.md` or `docs/testing.md` 7. "T n" points at the test in section 7; "run" means a case exists
and only has to be played.

### 5.1 Structural gaps (reach, not cases)

| # | gap | impact | named | evidence | proposal |
|---|---|---|---|---|---|
| G1 | 75 full-tier cases unrun since v0.1.17 RC; no candidate tier in rounds 9 to 13 | H | no | INDEX.md; r18u28 is the last full run | 7.0 a, b |
| G2 | Linux X11 has no current app evidence; agent mode never ran there | H | partly (testing.md 7 says "reduced pass") | r18u28 x11: 90 terminal rows, 3 agent rows, 19 fail; no x11 job since | 7.0 c |
| G3 | Windows PowerShell pane: no routing, mode-switch or agent-input proof | H | no | r12a skip reasons "runs in the Git Bash pane" | T5, T6 |
| G4 | Windows: no pointer, no resize, no OCR in the backend | H | partly ("vertical tabs on Windows") | winmap reasons in r12a and r18u28 | T31, T33, 5.3 |
| G5 | macOS: no pointer tool, no quit key, no close key | M | no | skip reasons in r12a | T32, 5.3 |
| G6 | lab helper arms all NOREF; 18 lab families and the replay never run; no lab on macOS or Windows | H (owner pain order starts with remote) | partly (helper arms, Mac and Windows lab) | lab-wayland results r10a to r12a; manifest: `lab-helper`, `app-test-replay-linux`, `app-test-lab-macos`, `-windows` never played | 7.0 d, e |
| G7 | `crates/integration` and `usage-test-*` have no committed result | M | partly (testing.md 2.9 says "never ran" before #1502) | every manifest.json | 7.0 f |
| G8 | Linux-only safety cases: no-network start, keyring, offline plugins, network gate | M | partly (#1977) | "(this platform: macos)" skips | T41, T42 |
| G9 | every committed run uses scale 1, one output, llvmpipe | M | partly (GPU) | `meta.scale`, testing.md 7 | T35, T48 |
| G10 | agent cases with a real provider are few and the tool-call one fails | M | no | `agent_approval` j17! on Wayland and Git Bash | 7.0 a; T12 |

### 5.2 Feature gaps, existing harness can see them

| # | gap | impact | named | proposal |
|---|---|---|---|---|
| G11 | Up-arrow history recall in a local shell | H | no | T1 |
| G12 | scrolling and scroll position while output streams | H | scroll position only (#1985) | T2, T24 |
| G13 | Find in ordinary use; focus back to the input; agent-mode Find failed at v0.1.17 RC | H | no | T3 |
| G14 | tab close, undo close, close with a running process | H | no | T4 |
| G15 | job control: Ctrl-Z, bg, fg | M | no (matrix 3.5 item 7 lists it, no case) | T7 |
| G16 | binary or hostile output leaves the pane usable | M | no (matrix item 9 lists it, no case) | T8 |
| G17 | Alt/Meta, Home/End, word keys, kill keys | H | no | T9 |
| G18 | true first run on default settings | M | no | T10 |
| G19 | model / provider switch mid-conversation | H (owner runs two providers) | no | T11 |
| G20 | Anthropic-format provider | H | no | T12 |
| G21 | auto-run permission settings | H (safety) | no | T13 |
| G22 | resume a conversation after restart | M | no | T14 |
| G23 | block output as agent context; redaction off / on in what the model receives | M | no | T15 |
| G24 | agent blocked in a hidden tab: attention state | H (owner: many vertical tabs, agent default) | no | T16 |
| G25 | a setting changed in the UI sticks and applies; #2026 hand edit seen | M | partly (#1987 settings that may not stick) | T17 |
| G26 | OSC 52 write allowed, read denied | M | yes | T18 |
| G27 | history and suggestions after restart and after kill -9 | M | no | T19 |
| G28 | restore shows earlier output and conversations | M | no | T20 |
| G29 | link and path click, OSC 8 | M | no | T21 |
| G30 | selection modes beyond one word and one wrapped drag | M | no | T22 |
| G31 | PRIMARY, middle click, copy on select (Linux) | M | no | T23 |
| G32 | wheel in scrollback, pagers and mouse-reporting programs; `alt_screen_mouse_reporting` never passed | M | no | T24 |
| G33 | pane resize, maximise | L | no | T25 |
| G34 | reflow and SIGWINCH on resize, rapid resizes during output | H | no (matrix item 9 lists "60 rapid resizes", no case) | T26 |
| G35 | context-menu actions; `ui_right_click_paste` never passed | L | no | T27 |
| G36 | tab click, drag reorder, panel pointer use | M | partly (vertical tabs) | T28 |
| G37 | focus loss and return; focus reporting | H | "focus" (#1643) only for one macOS key | T29 |
| G38 | minimise, restore, fullscreen | M | no | T30 |
| G39 | keybinding change takes effect | L | no | T45 |
| G40 | editor daily use beyond edit and save | L | "editor memory" only | T50 |
| G41 | Tab during a pending line report (#1659) | M | yes | named; a case of its own in PROVING |
| G42 | toast text and dedupe (#1842) | L | yes | named |
| G43 | 1 MB paste on current code | M | yes | run `big_paste` (7.0 a) |
| G44 | zsh, fish as the PANE shell with routing (macOS default shell is zsh) | L for the owner | no | add to 7.0 a as a pass, not a case |
| G45 | secret redaction on in copy and on screen | L (owner: off) | no | folded into T15 |
| G46 | workflows, launch configs, env vars, rules, notebooks through the real window | L | no | none proposed; in-process suite (7.0 f) |
| G47 | throughput and memory-per-tab budgets | L | partly | owner budget question (section 9) |

### 5.3 Gaps that need new harness capability (listed separately, as asked)

| # | gap | impact | capability missing | proposal |
|---|---|---|---|---|
| H1 | every pointer case on Windows | H | mouse `SendInput` in `winapi.py` | T31 |
| H2 | every pointer case on macOS | M | a CGEvent pointer tool with Retina mapping (as `htype2` is for keys) | T32 |
| H3 | resize and reflow on Windows | M | `SetWindowPos` in the Windows backend | T33 |
| H4 | rendering correctness on all platforms | M | an app hop or debug action that dumps the visible grid (cell, colours, attributes, cursor) | T34 |
| H5 | DPI scale and monitor change | M | a scale knob per backend; a second sway output | T35 |
| H6 | sleep / long freeze | M | a freeze step (SIGSTOP / SIGCONT of the app) plus the lab's stall fault | T36 |
| H7 | desktop notifications | M | a recording `org.freedesktop.Notifications` on the launcher's private bus; a log hop elsewhere | T37 |
| H8 | drag and drop | L | a drag-source client per display system | T38 |
| H9 | IME | L for the owner | ibus or fcitx in the job session; owner enables macOS input sources | T39 |
| H10 | MCP | H | the case's precondition never holds: the app under test does not load the harness's MCP config | T40 |
| H11 | no-network proof on macOS and Windows | M | a network trace or deny sandbox per platform | T41 |
| H12 | macOS keychain at start (#1977) | M | a test keychain swap | T42 |
| H13 | installer, single instance | M | the build artefact and an install sandbox in the app-test job | T43 |
| H14 | upgrade over the previous release | M | where CI gets the previous artefact (owner, #1546) | T44 |
| H15 | cmd, pwsh 7, WSL panes | L | a tier that sets `APPCI_WIN_SHELLS`; WSL in the VM | T46 |
| H16 | real sudo / su / docker boundary | M | passwordless sudo for a test user, a local image (machine change: owner) | T47 |
| H17 | real GPU | L | a GPU runner | T48 |
| H18 | soak on macOS and Windows | M | process sampling without `/proc`; a close key on macOS | T49 |
| H19 | OCR-judged cases on Windows | M | an OCR engine on the VM, or log-hop twins | section 9 |
| H20 | accessibility tree | L | an AT-SPI / UIA client | none in the 50 |

## 6. False coverage: registered, cannot fail today

| # | case(s) | why it cannot fail | fix |
|---|---|---|---|
| F1 | `agent_mcp_faults` (PROVING) | SKIP on every platform in every committed run: "the app did not load" the harness MCP config; Windows SKIP by winmap. Never judged once | T40 |
| F2 | `ui_drag_drop_file` | SKIP on all four jobs: no drag source | T38, or delete the case until one exists |
| F3 | `startup_shell_pane` | SKIP everywhere: "runs only in a startup_shell_override pass", and no tier makes that pass | T46 |
| F4 | `sudo_su_real_shell`, `subshell_ready_relay_shape`, `subshell_ready_docker_t_without_i` | SKIP everywhere: no passwordless sudo, no local image | T47 |
| F5 | `upgrade_over_previous`; `upgrade_from_v0_1_17` | SKIP everywhere (no previous-release data); no row ever | T44 |
| F6 | `intl_ime_japanese`, `intl_ime_pinyin` | SKIP everywhere (macOS: NEEDS_OWNER) | T39 |
| F7 | `ssh_connect_typing_wrapper`, `ssh_nested_typeahead_wrapper` (PROVING) | SKIP in every pane on every platform: need `APPCI_SSH_WRAPPER=1`, which no tier sets | set it in the Git Bash pass, or remove from PROVING |
| F8 | lab helper arms (40 rows a run) | NOREF every run: no built helper for the checkout | 7.0 d |
| F9 | `lab_fam_agent_context` | "no scripted app driver ... the case is registered so the guard passes": the coverage guard is satisfied by a case that cannot judge | write the driver, or let the guard fail |
| F10 | `agent_code_recycle` | SKIP / NOREF / fail on all platforms at r18u28: depends on a real model's wording | move to the fake model |
| F11 | `latency_key_log` | NOREF on every platform in r12a and r18u28 for key to frame ("no reference terminal measured"); its 1 ms key-to-pty target is a note | judge the key-to-pty target, or pair a reference |
| F12 | `alt_screen_mouse_reporting` | never passed: fails on both Linux backends (program received `b'k'`), SKIP elsewhere; cause unclassified (test or app) | T24; first a TEST-vs-APP verdict |
| F13 | `ui_right_click_paste` | never passed: OCR cannot find the probe to place the pointer | T27 (use the app's position hop) |
| F14 | `ui_mouse_select_copy` | passed on X11 only; fails on Wayland ("SENTINEL_NOT_COPIED"); SKIP macOS, Windows; unclassified | verdict, then T22 |
| F15 | `cb_settings_ui_open`, `cb_ui_action_latency`, `cb_agent_pickers` on Linux | SKIP: "Pillow not installed on this runner (owner: install?)" | owner decision (section 9) |
| F16 | `agent_interactive_ssh` | NOREF on Wayland and macOS in r12a, SKIP on Windows; last judged r18w8 | r13 changed this path; needs a run |
| F17 | `ui_startup_time` | fails only above a 20 s warm median or 90 s to a prompt; a start five times slower than today passes | owner budget (section 9) |
| F18 | `md_viewer_ai_answer`, `cb_ui_action_latency`, replay latency | by design "never a failure" except a crash; they are measurements | list them as measurements, not coverage |
| F19 | OCR checks on Windows (`mode_proof` OCR fields `None`, `ui_completions_as_you_type_local` NOREF) | no OCR engine on the VM | H19 |
| F20 | `shell_fish`, `shell_nu` on Windows; `subshell_ready_unsupported_tcsh` on Wayland | SKIP: shell not installed on that machine | install (owner: machine change) or accept |

Not false coverage, but easy to misread: `latency_paint`, `latency_paint_ab` and `ctrl_c_flood_ab` are macOS-only by design, so key to
pixel is unmeasured on Linux and Windows; `agent_restart_quit_*` is Linux only.

## 7. Recommended order

### 7.0 Before any new case: run what exists (no code)

These are tier and machine actions for the coordinator and the owner; none is decided here.

| | action | closes |
|---|---|---|
| a | one candidate-tier (full) app run per platform on the current tree, results committed | G1, G10, G43; turns about 75 "j17" cells into evidence |
| b | pick a daily-use slice of the full-tier cases for the fix tier (owner decides which): `session_restore`, `crash_kill9_restore`, `ui_resize_typing`, `ui_split_panes`, `ui_ctrl_r`, `ui_ghost_accept`, `ui_tab_menu`, `big_paste`, `paste_multiline_prompt`, `ssh_password_prompt`, `remote_session_legacy`, `alt_screen_programs`, `agent_approval` | G1 staying closed |
| c | play `app-test-linux-x11` with both modes | G2 |
| d | play `lab-helper` before the lab so the helper arms judge | G6, F8 |
| e | play the unplayed lab families and `app-test-replay-linux`; then the lab on macOS and Windows | G6 |
| f | play `integration-linux`, `-pwsh`, `usage-test-*` once and commit the result | G7 |
| g | TEST-vs-APP verdicts for the never-passed cases F12, F13, F14 and for `agent_approval` | F12 to F14, G10 |

### 7.1 The 50 cases to build

Every case: tier PROVING (fix tier until it has passed; graduation is the owner's), both modes unless noted, typing at 10 to 25 ms per
key with instant Enter, exact received line checked. Cost is an estimate in fix-tier minutes per platform pass, both modes together;
none is measured. "Existing" means the harness already has the injector and the oracle named.

**Wave 1: highest impact, existing harness, keyboard in and files / app log out**

| T | case | platforms | injects / observes | oracle (how a defect shows) | min | harness |
|---|---|---|---|---|---|---|
| 1 | `history_up_recall` | all five | three commands; Up, Up, Enter; Up, edit, Enter; Up then Down to an empty line and a new command | `rec.log` (PSReadLine history file in PowerShell) holds the recalled line exactly once and the edited line exactly; a wrong, doubled or empty line fails | 2 | existing |
| 2 | `scroll_keys_position` | Wayland, X11, macOS; Windows once a scroll-position hop exists (T34's grid hop would serve) | 500 numbered lines, then a slow stream (one line per 100 ms); page-up and scroll-to-top by key; wait 3 s; type one key | OCR: the early line is still on screen after 3 s of new output (no snap to the bottom); after the key the last line is on screen; the typed command is exact | 3 | existing (OCR) |
| 3 | `find_basic` | Wayland, X11, macOS (OCR); Git Bash by log only | token in block 3 of 10; open Find, type, next, previous, Escape; then type a command | the app's find hop reports the expected match count; the match is on screen; after Escape the command lands in the shell exactly (focus returned) | 2 | existing (find hops, OCR) |
| 4 | `tab_lifecycle_keys` | Wayland, X11, both Windows panes; macOS after T32's close key | new tab, a command, close tab, undo close; a tab running `sleep 300`: close, cancel the dialog, close again, confirm | the tab returns with its cwd and nothing re-runs (`rec.log`, effect file); cancel keeps the process (pid alive); confirm ends it (pid gone, no orphan) | 3 | existing |
| 5 | `routing_table_powershell` | Windows PowerShell | the routing corpus (commands, prose, typo heads, shell syntax) | PSReadLine history plus `hop=enter_begin input_type`: each line went to the shell or the agent as the table says | 3 | existing (`ps_history_lines`) |
| 6 | `agent_basics_powershell` | Windows PowerShell | prose into the agent input; Escape; a shell line; back to the agent | agent text byte-exact in the app log; the shell line exact in PSReadLine history; nothing reaches the shell from the prose | 2 | existing |
| 7 | `job_control` | Wayland, X11, macOS | `sleep 100`, Ctrl-Z, `bg`, `jobs > f`, `fg`, Ctrl-C; `cat`, Ctrl-Z, a command, `fg`, text, Ctrl-D | the `jobs` file shows the expected states; every typed line exact; the prompt returns after each step; no key goes to the wrong program | 2 | existing |
| 8 | `hostile_output_recovers` | all but PowerShell | 2 MB of random bytes; a script that prints title spam, enters the alt screen and never leaves, turns mouse reporting on and never off, a 1 MB OSC 8 | afterwards: a typed command is exact, no panic or ERROR in the app log, the largest main-thread item is under the warn bound, Find still matches | 3 | existing |
| 9 | `meta_and_word_keys` | all five | Alt-B, Alt-F, Alt-D, Alt-dot, Ctrl-A, E, K, U, W, Y, Home, End, Delete, Ctrl-Left / Right at the prompt; the same keys into a raw reader | the resulting command line in `rec.log` is the expected text; `bytes_reader.py` shows the expected bytes for each key in the raw program | 3 | existing |
| 10 | `first_run_defaults` | all five | every state and config directory removed; default settings (only the logging the oracle needs) | a prompt within the startup bound; the first command typed in each mode is exact; keys sent equals keys the app saw (no dialog ate them); no ERROR | 3 | existing |

**Wave 2: existing harness, agent and state**

| T | case | platforms | injects / observes | oracle | min | harness |
|---|---|---|---|---|---|---|
| 11 | `agent_model_switch` | Wayland, macOS, Git Bash (fake pass) | two fake providers on two ports; switch model by keyboard mid-conversation; prompt; switch back | request logs: the prompt reaches provider B only, with the earlier turns; provider A's key header never reaches B | 3 | existing (two `fake_model.py`) |
| 12 | `agent_anthropic_wire` | same | a fake provider speaking the Anthropic messages stream: text, a tool call with approval, a question, a cut stream | the same oracles as `agent_question_answer`, `agent_approval`, `agent_fault_disconnect`, on that wire format | 4 | existing plus a wire format in `fake_model.py` if it has none (not checked in depth) |
| 13 | `agent_autorun_permissions` | Wayland, macOS, Git Bash | auto-run on with an allow list: `ls`; a denied `rm`; `ls; rm x`; `$(rm x)`; a pipe to `sh` | allowed: runs with no card. Each of the others: a card, and the effect file does not exist before Enter (`pty_write_bytes` hop shows no write) | 4 | existing |
| 14 | `agent_conversation_resume` | Wayland, macOS, Git Bash | finish a conversation; quit (kill on macOS and Windows); relaunch; reopen it; a follow-up | the earlier text is shown (OCR or hop); the follow-up request carries the earlier turns exactly once | 3 | existing |
| 15 | `agent_block_context` | Wayland, macOS, Git Bash | a command printing a token and a string matching a secret regex; ask about that block; redaction off, then on | off (owner setting): the request holds the output unchanged. On: the secret is absent from the request and from a copy | 3 | existing |
| 16 | `agent_hidden_tab_attention` | Wayland, macOS (OCR); Windows by hop | tab 2: a reply that ends in a question card; tab 1 shown, typing | the panel row of tab 2 shows the waiting state; the typing in tab 1 is exact; jump to tab 2, Enter answers | 3 | existing (vertical-tabs OCR) |
| 17 | `settings_ui_toggle_persists` | Wayland, X11, macOS | open settings by key; toggle vertical tabs and font size by keyboard; relaunch; hand-edit the file back to its last written bytes (#2026) | exactly those keys change in the settings file; the screen changes; the value survives relaunch; the hand edit is seen | 3 | existing (as `network_gate_switch`) |
| 18 | `osc52_write_read` | all but PowerShell; plus over ssh | a program writes OSC 52 with a token; then sends an OSC 52 read query | the clipboard holds the token; the reader receives no reply to the query | 2 | existing |
| 19 | `history_survives_restart` | all five | commands typed; quit; relaunch; Up and Ctrl-R; repeat with kill -9; an agent prompt in between | each command is recalled exactly once; the agent prompt is not in shell history | 3 | existing |
| 20 | `restore_shows_history` | Wayland, macOS, Git Bash | a block with a token and an agent turn; quit; relaunch | Find matches the token in the restored view; the conversation text is shown; nothing re-runs | 3 | existing |

**Wave 3: existing pointer and OCR harness on Linux, log-hop twins elsewhere**

| T | case | platforms | injects / observes | oracle | min | harness |
|---|---|---|---|---|---|---|
| 21 | `link_and_path_click` | Wayland, X11 | print a URL, an OSC 8 link, a `file:line` path; modified click on each | a recording opener first on PATH logs exactly that URL; the path opens a file pane (`root_view:add_file_pane` in the log); no other outbound connection | 3 | existing (pointer, OCR) |
| 22 | `selection_modes` | Wayland, X11 | double click, triple click, shift-click extend, drag across two blocks, drag while output streams | the clipboard equals the expected text each time (sentinel pre-set) | 3 | existing |
| 23 | `primary_and_middle_click` | Wayland, X11 | select by drag; middle click in the input; with copy-on-select off and on | PRIMARY holds the selection; the paste is exact; CLIPBOARD is unchanged unless the setting says so | 2 | existing |
| 24 | `wheel_scroll` | Wayland, X11 | wheel over the scrollback, inside `less`, inside a reader that enabled SGR mouse reporting | scrollback moves (OCR); `less` moves; the reader gets SGR wheel and press / release bytes (`bytes_reader.py`), which also settles F12 | 3 | existing |
| 25 | `pane_resize_maximize` | Wayland, X11, macOS (keys) | split; maximise and restore by key; resize by key; on Linux drag the divider | `stty size` in each pane changes as drawn; typing lands in the focused pane (`rec.log` per pane) | 3 | existing |
| 26 | `resize_reflow_output` | Wayland, X11, macOS | wrapped known lines; a SIGWINCH trap writing rows and columns; 60 resizes in 10 s during a stream; vim open during a resize | final `stty size` equals the app's grid; the block's copied output is byte-identical after reflow; no line lost (count); vim reports the new size | 4 | existing |
| 27 | `context_menu_actions` | Wayland, X11 | right click on a block: copy output, copy command; on the input: paste. Pointer placed from the app's position hop, not OCR | the clipboard, then the input, hold the exact text | 3 | existing (`hook_find`) |
| 28 | `tab_pointer_use` | Wayland, X11 | click a tab; drag to reorder; middle-click close; the same in the vertical panel | tab order proven by number keys and per-tab tokens; the closed tab's shell is gone | 3 | existing |
| 29 | `focus_cycle` | Wayland, X11, macOS, Windows | focus the reference terminal, type there, return; a reader that enabled focus reporting | the first key after return is not lost; nothing typed elsewhere reaches Phosphor; the reader gets one focus-out and one focus-in | 3 | existing (reference-terminal switching) |
| 30 | `minimize_fullscreen` | Wayland, X11, macOS | minimise for 30 s under a stream; restore; toggle fullscreen twice | output complete (line count); first frame after restore within the UI budget; `stty size` changes and returns; typing exact | 3 | existing on Linux; a window-state call on Windows |

**Wave 4: new harness capability, high reach**

| T | case | platforms | what must be built first | oracle | min | harness |
|---|---|---|---|---|---|---|
| 31 | pointer on Windows: `ui_mouse_select_copy`, `ui_copy_wrapped_drag`, `ui_vertical_tabs_panel_click`, T22, T27 | Windows | mouse `SendInput` in `winapi.py`; positions from the app's hop (no OCR) | as the Linux cases | 6 | NEW |
| 32 | pointer and close / quit keys on macOS: the same cases, plus `quit_24_tabs`, `agent_restart_quit_*`, T4 | macOS | a CGEvent pointer tool; an allowed way to send the close and quit chords | as the Linux cases | 8 | NEW |
| 33 | resize on Windows: `ui_resize_typing`, T26, `ui_vertical_tabs_many` | Windows | `SetWindowPos`; the Windows config path in the settings-rewrite helper | as the Linux cases | 5 | NEW |
| 34 | `grid_snapshot_render` | all | a debug action or hop that writes the visible grid (text, colours, attributes, cursor) | a fixed escape script yields the expected grid: SGR, truecolor, wide and combining cells, emoji width, box drawing, cursor shape and place | 3 | NEW (app-side hop) |
| 35 | `scale_matrix` | Wayland, X11, Windows | a scale knob per backend (sway output scale, X resources, a VM DPI setting); a second sway output | at 1.25, 1.5 and 2: `stty size` matches the window; a click on a word copies that word; moving the window to the other output keeps both | 6 | NEW |
| 36 | `freeze_and_wake` | Wayland, macOS, lab | SIGSTOP the app for 60 s, SIGCONT; with an ssh session and a streaming agent turn open; the lab link stalled for 90 s | local typing exact at once; the ssh tab is alive or says it dropped and returns a local prompt; the agent turn ends or errors; no burst of held keys goes to the wrong place | 5 | NEW (freeze step) |
| 37 | `desktop_notification` | Wayland, X11; macOS and Windows by hop | a recording notification service on the launcher's private bus | a long command ends while unfocused: one notification with the command; none while focused; an agent question in the background: one | 3 | NEW |
| 38 | `drag_drop_file` (make F2 real) | X11 first, then Wayland, Windows, macOS | a drag-source client | the dropped path, with a space, arrives quoted in the input and `ls` on it works | 3 | NEW |
| 39 | IME: Linux ibus, Windows IME, macOS (F6) | Linux, Windows, macOS | ibus in the job session; owner enables the macOS input sources | composed text reaches the shell and a raw reader exactly; nothing leaks before the commit | 5 | NEW (packages: owner) |
| 40 | `mcp_basic` and a judged `agent_mcp_faults` | Wayland, macOS, Git Bash | the harness places the MCP config where the app under test reads it, per platform | a healthy server's tool is offered, called after approval, its result returned; then the six fault servers as the case already states | 5 | NEW (harness fix) |

**Wave 5: new capability, narrower or machine-bound**

| T | case | platforms | what must be built first | oracle | min | harness |
|---|---|---|---|---|---|---|
| 41 | `zero_network_startup` on macOS and Windows | macOS, Windows | a per-platform network trace or deny sandbox | no non-loopback destination from launch through idle | 4 | NEW |
| 42 | `keychain_startup_macos` (#1977) | macOS | a test keychain the harness may lock and swap | start time within the bound; no repeated prompt; the keychain line logged once | 3 | NEW |
| 43 | `installer_smoke` | all | the build artefact and an install sandbox in the job | install, launch to a prompt, a second launch joins the first instance, `phosphor <path>` opens the file, uninstall keeps user data | 6 | NEW |
| 44 | `upgrade_from_previous_release` (run F5) | all | the previous artefact's source (owner, #1546) | `upgrade_table.py` rows hold; no ERROR; second launch changes nothing | 6 | existing case, NEW input |
| 45 | `keybinding_rebind` | all five | none | a rebound chord triggers its action (prompt log +1) and the old chord does not; a chord that shadows Ctrl-R follows the user's file | 2 | existing |
| 46 | pane shells: cmd, pwsh 7, WSL (run F3) | Windows | a tier pass with `APPCI_WIN_SHELLS`; WSL in the VM (owner) | `startup_shell_pane` as written, plus T1 and T5 in each | 6 | NEW (tier pass) |
| 47 | real privilege boundary (run F4) | Linux, macOS | a test user with passwordless sudo, a local image (owner: machine change) | the three cases as written | 5 | NEW (machine) |
| 48 | `gpu_real_render` | Linux | a GPU-backed runner or the box render node in the job | frame times under budget; screenshot equals the software one within tolerance; a lost device recovers | 4 | NEW (machine) |
| 49 | `soak` on macOS and Windows | macOS, Windows | process sampling without `/proc`; T32's close key | the soak's memory, CPU and exact-command verdicts | 60 (release tier after proving at 10) | NEW |
| 50 | `editor_daily` | Wayland, X11, Git Bash; macOS via the open handler | none for keys; T21's click for open-by-click | find and replace changes exactly the expected bytes; closing with unsaved changes asks; a file changed on disk is reloaded or flagged, never silently overwritten | 3 | existing |

Estimated fix-tier cost if all of waves 1 to 3 sit in PROVING at once: about 85 minutes per Linux pass, about 60 on macOS, about 35
on Windows (estimates; the first run records them). That argues for building a wave, proving it, and asking the owner about
graduation before the next.

## 8. Proposed issues (none filed)

1. Full tier has not run since v0.1.17 RC; play a candidate tier and commit results (G1).
2. Fix tier carries no daily-use slice of the full cases; owner to pick (G1, 7.0 b).
3. Linux X11 app tests: no current run, agent mode never run (G2).
4. Windows PowerShell pane: routing and agent basics unproven (G3; T5, T6).
5. Windows backend: pointer, resize, OCR or hop twins (G4; T31, T33, H19).
6. macOS backend: pointer, close and quit keys (G5; T32).
7. Lab: helper arms NOREF every run; play `lab-helper` first (G6, F8).
8. Lab families and replay never played; lab on macOS and Windows never played (G6).
9. `integration-linux` and `usage-test-*`: no committed result (G7).
10. `agent_mcp_faults` has never been judged; MCP has no app coverage (F1, H10; T40).
11. Always-SKIP cases in PROVING: `ssh_*_wrapper`, `subshell_ready_relay_shape`, `subshell_ready_docker_t_without_i` (F4, F7).
12. Always-SKIP full-tier cases: `ui_drag_drop_file`, `startup_shell_pane`, `sudo_su_real_shell`, `upgrade_over_previous`, IME (F2 to F6).
13. `lab_fam_agent_context` satisfies the coverage guard without a driver (F9).
14. Never-passed cases need a TEST-vs-APP verdict: `alt_screen_mouse_reporting`, `ui_right_click_paste`, `ui_mouse_select_copy` on Wayland, `agent_approval` on Wayland and Git Bash (F12 to F14, G10).
15. `latency_key_log` is NOREF on every platform (F11).
16. `agent_code_recycle` depends on a real model's wording (F10).
17. New cases, wave 1 (T1 to T10), one issue per case or one for the wave.
18. New cases, wave 2 (T11 to T20).
19. New cases, wave 3 (T21 to T30).
20. Harness capabilities, wave 4 (T31 to T40), one issue per capability.
21. Harness capabilities, wave 5 (T41 to T50).
22. Safety cases are Linux only: no-network start, keyring, offline plugins, network gate (G8).
23. Every run is at scale 1 on one output with software rendering (G9).

## 9. Owner decisions this review ran into (not decided here)

- Which full-tier cases, if any, join the fix tier as a daily-use slice (7.0 b), and whether X11 joins the fix tier.
- Pillow on the Linux box (three cases skip for it), an OCR engine on the Windows VM, fish and nu on the VM, tcsh on the box:
  machine changes.
- Passwordless sudo for a test user and a local container image on the test machines (F4).
- Where CI gets the previous release for the upgrade cases (#1546, already open).
- macOS input sources for the runner user (IME), already flagged NEEDS_OWNER by the case.
- Budgets that are loose enough to be vacuous: startup (20 s warm), and whether throughput and memory per tab get a budget.
- Whether always-SKIP cases stay registered (they read as coverage) or are removed until their capability exists.
