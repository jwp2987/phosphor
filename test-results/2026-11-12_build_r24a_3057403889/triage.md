# Triage: round 24 targeted run, pipeline 3057403889 (build/r24a 443495ed3)

## Unit suites
All green on Linux, macOS and Windows (units ran FIRST, before each build). Windows test-windows ended by itself (#2148 verified and closed).

## App and lab (TEST/APP verdicts with evidence on the issues)
- Wayland 117 pass / 4 fail: #2146 tab leak (TEST, fixed r25), #2175 agent-view scroll on typing (APP, fixed r25). Latency table agent Enter overhead 4.1 ms informational (app share 1.85 ms passed).
- macOS 98 pass / 9 fail: #2176 Cmd-click line lost (APP, fixed r25), #2175, #2146, #2118 cursor-window refusal (TEST, fixed r25), editor_memory_7mb 36 MB (owner raised the limit to 40 MB). 6 agent rows ran_mode unknown: #2173 (fixed r25).
- Linux lab 78 / 99 / 75 noref and macOS lab: #2156 side-call counts (APP, batching fixed r25), #2174 zsh no compinit (APP, fixed r25), #2177 fish doubling + raw prompt (APP, prompt fixed r25), #2180 nu login shell gets POSIX side commands (APP, fixed r25), #2181 held '~.' (APP, fixed r25), #2155 exec zsh -l (APP, open), #2162 zsh recorder (TEST, hardened r25), nu harness syntax (TEST, fixed r25), fish/nu Tab 150 ms (owner question).
- Windows 180 pass / 37 fail: triage in progress at commit time; mode_proof OCR (TEST, fixed cb54e0961), history_up_recall (#2107).
