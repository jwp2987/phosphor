# Daily-user annoyance review (Fable 5.1, 2026-11-09)

Read-only review at merge/w0 0fd58815f. Several items were fixed by merges after that commit (lifecycle d2fa9a8f2, subshell 73a82d6d8, privacy b587236b2, #1925 cba31b86b); #1958 and #1933 are in progress.

Evidence base: newest run `test-results/2026-11-09_build_r18w8_3047198675/README.md` + `app/{macos,wayland,windows}/results.json(l)`; ssh lab `test-results/2026-11-05_ssh_r18u28_*/summary.md`; `docs/testing.md`; `docs/audits/2026-11-05-busy-machine-timers.md`; `docs/design/1843-1848-latency.md`; `DECLINED.md`; `glab issue list/view` (open ~230, closed audit). "Verified" = code path, test row or owner-confirmed issue; "Suspected" = inferred from code without a run. Label `fixed-untested` = fix merged on main but no app run has proved it since.

## Ranked list (most annoying to a daily user first; owner setup: agent mode, autodetect+NLD on, Ollama + Claude proxy, vertical tabs, bash)

### 1. Typed keys are DISCARDED (by design) whenever an Enter gate gives up; the "Input was waiting too long" / "N keys typed after Enter were discarded" toasts
- Experience: you type a command and keep typing; a few seconds later a toast says the shell "didn't take the command in time" and everything after the Enter is gone, or "Input was waiting too long, so it was not sent; what you typed is in the input". On macOS at an EMPTY remote prompt this toast kept popping (owner report).
- When: any time a hold/gate times out: route-settle 10 s net, uncovered-report patience 3 s (#1701/#1702), unready-shell 60 s, young-Enter hold, Tab-completion hold 150 ms (`app/src/terminal/input.rs:1482`), paste verdict floor, ssh connect ceilings. On a busy machine or VPN the timers fire on healthy sessions (audit: 33 HIGH + 68 MEDIUM timers).
- Evidence (verified): policy `DECLINED.md:2898` "Keys typed after an Enter are DROPPED when its gate gives up — DECIDED 2026-11-05 (#1661)"; toast strings `app/src/terminal/input/enter_queue.rs:102,1469,1478`; `app/src/terminal/view/paste_queue.rs:62` (`GATE_GIVE_UP_TOAST` = the owner's macOS toast, chosen at `paste_queue.rs:150-156` for every `why` except the subshell net). The empty-prompt macOS case is #1793 (bare Enter on a warpified remote held 3000 ms because `requests_outstanding` was stale, `<Enter>~.` escape keys discarded) and #1844 (after a bash 3.2 subshell the typeahead mode stays `InputMatching`, a `HandoffReport` gate never ends, every key held until the 60 s net). Both `fixed-untested` (stale counts made event-only, `docs/testing.md:3675`; mode-per-session `typeahead_mode_by_session`, #1897 says it leaks per session).
- Issue state: #1661 decided; #1793, #1844 open fixed-untested; #1900, #1862 (synced origin lost behind a gate) open.
- Fix direction: the decision itself is user-hostile relative to any plain terminal (no terminal ever drops typed bytes). Instead of discarding on give-up, write the held bytes to the pty as typeahead in typed order (that is what a plain terminal would have done the whole time) and show no toast; reserve the editor hand-back for the line before the Enter only. Also re-run the Mac ssh lab to prove #1793/#1844 before the next release.

### 2. Agent turn hangs on "Preparing question…" / stuck cards after Stop
- Experience: the agent asks a question, the card never becomes answerable; the conversation is stuck until you start a new one. After Stop, tool-call cards keep spinning.
- When: ask_user_question after approval (#1797), with an earlier pending action in the same turn (#1858), after a steer (#1861), after Stop (#1946); owner hit it again on another device.
- Evidence (verified): r18w8 Windows Git Bash+fake `agent_question_answer`, `agent_question_slow`, `agent_question_nodone`, `agent_question_after_command` all FAIL "the card hangs on 'Preparing question...' (150 s, state blocked, ask actions [])"; macOS/Wayland pass the same cases. `agent_question_steer_at_end` FAIL on all three ("the steer prompt never reached the model"; Mac row TAINTED). Lifecycle fixes merged in d2fa9a8f2 (#1942-#1953) and `docs/testing.md:3106`.
- Issue state: #1797, #1851, #1858-#1861, #1946 open fixed-untested; #1836 (prove the fuzzer catches it on the pre-fix base) open.
- Fix direction: the Windows failure shows a path the lifecycle audit missed (likely the ConPTY/Git Bash pane's agent view); add the invariant "no pending action without a card" as a runtime assertion that self-heals (resolve with an error result and a visible card) instead of a hang, and surface a "turn stuck, click to cancel" affordance after N seconds.

### 3. Key latency over a plain terminal: Linux per-key /proc scan; agent-mode typing while a reply streams; macOS key-to-paint
- Experience: typing in agent mode while a reply streams feels sluggish (Linux); on macOS every key paints ~7 ms later than Terminal.app; on Linux each key costs 4-16 ms before it even reaches the pty.
- Evidence (verified): r18w8 `latency_key_log` key->pty write p50 Linux terminal 4.23 ms, agent 16.50 ms, macOS 0.44 ms (target 1 ms) — #1958: `TypeaheadHandoff::log_route_decision` samples `sample_prompt_fact()` -> `process_group_names` which reads every `/proc/<pid>/stat` per key (`app/src/terminal/local_tty/terminal_attributes.rs`). It runs only when `warp::input_order` debug logging is enabled (`crates/warpui_core/src/input_order.rs:23`), but the owner's own daily logs in #1796/#1800 contain those hops, so it is likely paid daily. `agent_latency_key/streaming` FAIL 7.24 ms over budget (#1843; cause = winit Wayland frame-callback throttle, fix merged 9a64eebe6 behind `PHOSPHOR_KEY_FRAME_NOW`, NOT yet measured). macOS `latency_paint` FAIL 7.45 ± 1.76 ms over Terminal.app (#1848; fix merged, unmeasured). Wayland `latency_enter` 2.30 ms over foot FAIL (ssh lab) and a +14.2 % regression r18w7->r18w8 (32.5 -> 37.1 ms, README "vs previous run").
- Issue state: #1958 open; #1843/#1848 open (fix merged, unproven); #1792 X11 11.7 ms closed.
- Fix direction: move the prompt-fact sample out of the log closure (log the already-known fact), or make `process_group_members` use `/proc/<pgid>/task`/`sched_getpgid` on candidates only; run r18w9 to judge the #1843/#1848 arms; treat the Enter regression as a gate.

### 4. ssh without the remote helper: laggy over VPN, no completions/path detection; and ssh escape/disconnect recovery
- Experience: over a VPN every prompt/Enter seems to wait; no Tab completion, remote paths not recognised; `<Enter>~.` does nothing; after a dropped connection no local prompt comes back.
- Evidence: #1800 owner-confirmed (VPN, no helper, agent mode); #1801 fix merged (`docs/testing.md:3338`, in-band Tab) — r18w8 Wayland `remote_tab_no_helper` moved to FIXED, but macOS SKIPs it ("remote /bin/bash 3.2: no in-band Tab below bash 4.0, by design") and Windows Git Bash FAILs (#1933: wrapper falls back to plain ssh, never managed). The owner's plan items 1-5 in #1800 (cwd prefetch cache, batched per-prompt side work, persistent side channel, Enter never waits on the network) are NOT verified: `latency_enter_ssh` INCONCLUSIVE, netem 150 ms test not run. ssh lab Wayland `ssh_drop_reconnect` FAIL "[escape] no local prompt within 20 s" (#1793). #985 (remote proxy shares the interactive connection, 40-100 ms key delay) GitHub-era, status unknown.
- Issue state: #1800, #1801, #1793 open fixed-untested; #1933 open; #1808-#1810 timers open.
- Fix direction: make the netem ssh lab a release gate; bash<4 remote fallback (zsh/fish paths exist; for bash 3.2 use `compgen` via the prompt hook); the Enter path must never block on the line-report round trip in ssh sessions.

### 5. Long-running silent agent command handed to the user ("Command needs your input"), and the reverse (real prompt never handed over)
- Experience: the agent runs `grep`/`du` over ssh, it goes quiet, then the block says "Command needs your input. User is in control." and you must click "Give control back to agent". Or a real `[y/N]` prompt sits for a minute with nobody told.
- Evidence (verified): #1796 owner report + root cause (silence-plus-shape detector, 3 s/15 s/5 s timers in `interaction_mode.rs`, `terminal_manager.rs`; LRC handoff error "Already in agent mode"); #1811/#1812 timers (3 s/15 s/8 s pager). r18w8 Windows Git Bash `agent_lrc_no_false_takeover` FAIL part 2: "`printf 'Continue? [y/N] '; read` sat 60 s, neither handed over nor given the take-over notice" — the fix over-corrected there. Hand-back button has no keybinding found (`app/i18n/en/warp.ftl:413`, suspected click-only). Windows: all take-over hooks unix-only (#1882).
- Issue state: #1796, #1811, #1812 open fixed-untested.
- Fix direction: hand over only on a proven tty read (termios probe, as #1811 states), show a non-modal "the command seems to wait for input — take over?" notice otherwise, and give the hand-back a key.

### 6. Subshell "Phosphorize" setup fights the user (lines lost, secrets, 60 s hangs, empty prompt in child shells)
- Experience: `bash`/`sh`/`dash`/`sudo -i`/`docker exec`/`nix develop`: a line typed while setup runs vanishes or runs in the wrong shell; a password typed into `read -s` in `.bashrc` is cancelled or ends up as editor text; dash with no prompt waits 60 s; a persistent toast "Phosphor setup is waiting for the shell…" sits there; child shells started from a bash pane show NO prompt when the user exports PS1.
- Evidence (verified): r18w8 `subshell_ready_vim_typeahead` FAIL mac+wayland both modes (agent mode root shell lost = APP #1941), `subshell_ready_secret_after_prompt` FAIL all 4, `subshell_ready_zsh_newuser_install` FAIL wayland, `subshell_sh_probe` FAIL wayland (#1935), `pane_not_local_after_*` relaunches. #1907 (empty PS1 inherited) is a plain-terminal regression. Three overlapping give-ups 7 s/30 s/120 s (#1939). Toast text `app/src/terminal/view.rs:22813`; "Phosphorize failed… continues as a plain shell" `paste_queue.rs:69`.
- Issue state: #1934-#1941, #1905, #1907, #1939 open; #1904/#1840/#1845-#1847 fixed-untested.
- Fix direction: make auto-setup conservative: never write while a line is being typed, never cancel on a secret read, one give-up net, and restore PS1 for children; consider setup opt-in per subshell (a chip "Phosphorize this shell?") rather than eager injection.

### 7. Agent-run interactive commands leak or lose input
- Experience: the agent runs `sudo`/`ssh` and you type the password: it goes into the conversation as a user message (sent to the model); an agent-driven remote `read` is never handed over; vim driven by the agent eats keys.
- Evidence (verified): r18w8 `agent_interactive_password` FAIL on macOS, Wayland, Windows ("request 2 carries the typed text … leaked into the conversation", #1926); `agent_interactive_ssh` FAIL ("remote read never handed to the user within 60 s"); `agent_interactive_vim` FAIL (route unproven). #1957: keys held behind a deferred Enter become visible editor text while the tty reads a secret; #1927 NLD/next-command suggestions ship typed text to the provider while the tty reads a secret.
- Issue state: #1926, #1927, #1957 open; #1855/#1893 fixed-untested.
- Fix direction: a single "tty reads a secret" fact consulted by EVERY release path (gate releases, hand-back, NLD prediction, hand-back-after-handover context).

### 8. Copy/paste
- Verified: (a) OSC 52 is `deny` by default and a denied write is only a log line (`app/src/terminal/view.rs:18413-18450`, `terminal/settings.rs:95`) — tmux/vim/`pbcopy`-style yanks silently do nothing, unlike Windows Terminal/Konsole (allow write) or iTerm2 (asks). (b) Large paste: 1 MB freezes 6-7 s, Enter 1.4-5 s (#1191 fixed-untested, no case in r18w8). (c) macOS pasteboard read on the main thread — a slow provider freezes the window (DECLINED #1782, accepted; owner choice, note it). (d) Multi-line paste + Enter into a shell without bracketed paste shows "Run N lines one at a time?" and discards keys typed meanwhile (DECLINED #1652; bash 5.1+ has bracketed paste so the dialog is rare on the owner's setup; `input.rs:7257`). (e) Paste during an ssh connect decided by the legacy derivation/reordered (#1853 fixed-untested, #1895 open). (f) Keys typed during a paste read lost on ConPTY (#1636, closed). (g) Ctrl-C/Z in table mode cancels a held Enter and discards keys (#1852 fixed-untested).
- Suspected: ssh lab Wayland `ui_mouse_select_copy` FAIL (double-click + Cmd/Ctrl-C left the clipboard unchanged) and `ui_right_click_paste` FAIL — may be harness pointer limits (#1789), unproven. Copy of soft-wrapped lines: no code or test found either way.
- Fix direction: OSC 52 default `write_only` with a one-time toast on first use; verify #1191 with the appci large-paste cases; add a wrapped-line copy case.

### 9. Startup waits and first-run surprises
- Verified: no keyring -> window appears only after ~120 s ("slow model update: 119937 ms (ProxyCredentials)", #1805 fixed-untested; the `keyring_unavailable_startup` case SKIPs on every platform in r18w8, so unproven). macOS relaunch logs ERROR "A default keychain could not be found" twice (r18w8 `agent_restart_kill_*` FAIL on that line). With a model configured the app opens connections to the configured endpoint at idle (r18w8 Wayland `zero_network_startup` FAIL: 4 connects during 120 s idle, the case comment says to the CI Ollama; `script/realapp/appci/cases_netless.py:105-116`) — benign but a surprise, and `GET /api/tags` runs at startup (`openai_compatible.rs:192`). models.dev catalog downloaded at startup unless `agents.warp_agent.models_dev_fetch=false` (#1791, fixed). Every Wayland launch logs ERROR "error creating connection to Xorg server" (#1863, fixed). Startup writable probe held the UI 1-1.8 s (#1772, fixed).
- Fix direction: no credential lookup on the first-frame path; a one-line non-modal notice when storage is unavailable.

### 10. Hidden-tab output forcing full-window frames (#805) — fixed, residue remains
- Verified: fixed 52a379f24 (issue comment: 20 hidden busy tabs 361 % -> 52 % CPU). Remaining: the VISIBLE busy tab still drives ~1 frame per output line (#812) and hidden views are still re-rendered at 60/s per pane. With Claude Code in 10 tabs the main thread is still busier than iTerm2.
- Fix direction: skip the in-place re-render of unmounted views too (the issue's alternative B).

### 11. Autodetection routing (shell <-> agent)
- Verified good: #1560 closed (shell structure beats unknown head, `crates/input_classifier/src/util/routing.rs:360-370`); r18w8 `routing_table` PASS on all platforms, Wayland row explicitly "'find the bug in main.rs': went to shell" in terminal mode; `InstalledHead` rule `routing.rs:442`.
- Suspected annoyances: in an agent conversation an unknown head goes to the agent (`UnknownHeadInConversation`, `routing.rs:398-409`) — a typo like `gti status` becomes an agent prompt instead of "command not found". "make a PR" goes to the agent in terminal mode (row note). NLD prediction fires on a debounce while typing (`input.rs:411`, `predict_am_query` `input.rs:16443`) — every pause sends the buffer to Ollama (#1927 for the secret case), which is network traffic while typing and a VPN cost.
- Fix direction: in a conversation, an unknown head that is one edit away from an installed command should route to the shell; make NLD prediction opt-in in ssh sessions.

### 12. Ctrl-C aftermath: '^[i' echo block and bogus "Session abandoned before bootstrap"
- Verified: #1798/#1672 fix merged (event-driven line-editor activation, #1825 prompt event opens the gate, `docs/testing.md:3636,3833`); r18w8 `ctrl_c_no_esc_i_local` PASS on all three platforms. Not verified for the owner's actual scenario (managed ssh, Ctrl-C pressed several times) — only a LOCAL case exists. The "Session abandoned before bootstrap" warning (`view.rs:40292`) fires on an established session (defect 2 in #1798), log-only, uninvestigated.
- Fix direction: add the ssh twin of the case in the lab; find what re-enters the pre-bootstrap state mid-session.

### 13. Tab then fast Enter / Tab during a pending line report
- Verified: `tab_enter` PASS all platforms; the completion hold's give-up now hands back instead of submitting the uncompleted line (`input.rs:15440-15468`), so the failure mode on a busy machine is "Enter did nothing, press it again" (toast). #1659 open: a Tab while the shell's line report is in flight completes only the editor fragment (Windows seen, mechanism platform-neutral).
- Fix direction: re-run completion on the merged text when the report lands (as #1659 proposes).

### 14. Focus moving unexpectedly
- Verified: macOS Cmd-Shift-F (global search) then Escape leaves focus in the search field; the next line typed goes into the search (#1643 open fixed-untested). Escape in the agent view exits to the terminal (`view.rs:5405`, `agent_view/shortcuts/mod.rs:238`) — collides with vim-mode/readline Escape habits (suspected annoyance; `exit_agent_view` does not touch the buffer). Keys typed while an approval card holds focus move focus to the input (#1028, intended). New tab-group rename editor closed by PaneFocused (#1773 closed).

### 15. Scroll position
- Verified: Find did not scroll to a match in an older block with async find (#1775 fixed; ssh lab r18u28 agent-mode `flood_scrollback_find` still red — app log says it scrolled, OCR could not read; unresolved whether app or harness). Latent: full-screen agent view draws PinnedToBottom while scroll math uses the setting (#1788 closed).

### 16. Notices / toasts / banners
- Verified: toasts for discarded keys (item 1); persistent "setup is waiting" toast; "Phosphorize failed" toast; UI strings showing a literal `\n` (#1842 open, save modal + onboarding callout); Settings text describes removed behaviour (#1816 open); "Show more" on toasts has no keyboard toggle (DECLINED #902); Linux-only banner "did you intend ctrl-a/ctrl-e to move the cursor" on first emacs key (`view.rs:32575`, dismiss is permanent — acceptable). Error toasts are ephemeral with no dedupe (`view.rs:25871` `add_ephemeral_toast`): a repeating give-up shows a new toast every time (the owner's "kept popping").
- Fix direction: dedupe by text within a window; one notice per cause.

### 17. Settings that may not stick
- Verified: settings.toml saved by hand while the app runs could be overwritten by the next in-app change and syntax errors logged as "unknown error" (#1771 fixed, unrun). Preferences flush at quit bounded by 3 s (audit H31) — last change can be lost on a slow disk. Network-gate "Allow" from the toast/log row did not persist to settings.toml in r18w8 Wayland (`agent_gate_deny_allow` FAIL: "settings.toml holds localhost:42879 0 times (expected once)"; also the gate did not refuse an unlisted host — fake-model case, partly OCR-limited).

### 18. Agent file edits change what you did not ask for
- Verified: r18w8 `agent_edit_endings` FAIL mac+wayland: accepting an edit adds a final newline to a file that had none; mixed LF/CRLF files rewritten to the majority ending (#1956 open; #1954/#1955/#1925 fixed-untested). Remote accept has no on-disk precondition (#1837).

### 19. Windows-only (owner's "another device" may be one)
- Verified: Ctrl+Break never reaches the pty (#1906/#1888; r18w8 `ps_ctrl_break` 0/3); Git Bash ssh never managed (#1933); a typed key dropped at ssh hand-over on ConPTY (#1922); PowerShell hooks 18-60 ms/key, `echo hi` Enter->paint 137-152 ms vs conhost 25 ms (#1280); Git Bash Enter +26-31 ms (#1492); a secret typed into a precmd `read -s` goes to the editor and is submitted to the agent (#1923; `gitbash_precmd_secret` FAIL); subshell auto-setup never writes, 60 s then a misleading NotAShell toast (#1886); cancelling a restart still kills Phosphor (#1103).

### 20. Smaller / suspected
- Closed ssh tabs keep the local shell and ControlMaster alive until quit (#980, GitHub-era, "investigated, not fixed", no newer issue found) — suspected still present.
- Closing a pane SIGKILLs the foreground program 300 ms after SIGHUP (audit H25; `process_tree.rs`) — DECLINED.md:2590 says #1685 raised it to 5 s; verify.
- `HANDOFF_GATE_LEAK_NET` 60 s is the one timer that can still write user keys to the pty (audit H22).
- Code editor: 7 MB file costs ~850 MB RSS, layout 15 ms (#1340, `editor_memory_7mb` FAIL).
- Ctrl-R opens Warp command search, so the shell's own reverse-i-search is never reachable while the editor owns input (`input.rs:2485`, by inheritance from Warp; a readline user's muscle memory collides). Alt-. has no binding (passes through). Ctrl-C in the agent view = pause agent.
- Vertical tabs: no issue or test evidence either way.
- Keyring file fallback uses a hardcoded AES key (#1932, parked-owner) — security, not friction.

## Five I would fix first, and why
1. Item 1 (discard-on-give-up + the stale-gate causes #1793/#1844): it is the one behaviour that is strictly worse than every plain terminal, hits the owner on macOS at an EMPTY prompt, and every other timer bug funnels into it. Route late keys to the pty instead of dropping them; prove #1793/#1844 in the Mac ssh lab.
2. Item 2 ("Preparing question…"): still reproducing on Windows in r18w8 after the lifecycle audit; a stuck agent turn blocks the whole workflow. Add the self-healing invariant and the fuzzer proof (#1836).
3. Item 3 (#1958 /proc scan + unmeasured #1843/#1848): the owner's hardest budgets (key->paint ≤2 ms) are currently failing on both Linux and macOS, and #1958 is a cheap, certain win.
4. Item 4 (ssh without helper over VPN): owner-confirmed daily pain; fixes merged but nothing has been measured with latency injected. Run the netem lab, make Enter never wait on the network.
5. Item 6 (subshell Phosphorize): 10+ open issues, four FAIL rows per platform in r18w8, and #1907 breaks plain `bash`/`dash` prompts for anyone who exports PS1. Make the setup conservative or opt-in.

### Critical Files for Implementation
- /home/winters/git/phosphor-merge/app/src/terminal/view/paste_queue.rs (gate give-ups, `GATE_GIVE_UP_TOAST`, discard policy)
- /home/winters/git/phosphor-merge/app/src/terminal/input/enter_queue.rs (Enter gates, discard toasts, give-up nets)
- /home/winters/git/phosphor-merge/app/src/terminal/model/typeahead_handoff.rs (hand-off counts, young Enter, `log_route_decision` /proc sample #1958)
- /home/winters/git/phosphor-merge/app/src/ai/blocklist/controller.rs (agent turn lifecycle, ask_user_question hang paths)
- /home/winters/git/phosphor-merge/app/src/terminal/view.rs (subshell setup, LRC hand-off, OSC 52 gate, toasts)