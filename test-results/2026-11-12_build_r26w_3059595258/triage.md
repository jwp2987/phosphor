# Triage: round 26 Windows-only targeted run, pipeline 3059595258 (build/r26w 943954d29)

## Units
Windows test-windows, warpui-core-windows, workspace-rest-windows: green (units first).

## App (Windows) 215 pass / 43 fail
- #2182 REGRESSION (APP, my fix): ssh_connect_typing_wrapper (both Git Bash passes) — the in-band-only rule let 'i','t' overtake held 'e','x' of 'exit' (ex -> pty, it -> editor -> agent turn); ssh never left; the later Git Bash ssh family + pane_not_local_* cascade from it. #2182 REVERTED (b3c05c6c2); redesign in progress.
- #2192 NEW (APP): the editor keeps a submitted line after a nested/remote command; next text glued ('exitecho ...', 'cd ~cd ...'). Not a dropped Enter; #2116 not involved on this path.
- tabs_two_windows / tabs_streaming Git Bash 1/16, 2/10: #1658 (Windows streaming-tab young-hold reorder) — was ACCEPTED_FOR_RELEASE v0.1.17, now judged. PowerShell tabs_two_windows cleanup 'pid None' and terminal tabs_streaming 1/10: harness fault from f11a79d35 (#2146), fix in progress.
- history_up_recall x4: #2107 (fixed after this build; PASSES in r27a Linux and macOS).
- mode_proof x3: Windows OCR misread (the next build records the OCR text).
- latency_key_log / latency_first_key_after_idle: judged against fixed baselines in this build (pre-bands); the r26w collect regenerated bands.
- Proven here: md_viewer PASS (TEST fix), routing_table_powershell no agent-view leak (TEST fix), the pwsh7 pass runs (TEST fix), #2183 keybinding_rebind NOREF (no prompt log in that pane).
- ui_completions_as_you_type_local: unproven (msys path completion), open.

## Bands
The collect asked to widen 3 Windows bands beyond the 0.5 ms auto limit (held for the owner): windows/agent/latency_enter 35.1 -> 39.54 ms; windows/agent@gitbash/latency_enter#app 1.75 -> 2.37 ms; windows/terminal@gitbash/ctrl_c 27.49 -> 35.04 ms.
