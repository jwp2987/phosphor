**Tested a REUSED build**: job 17667172373 (build-macos-arm64) of pipeline 3018144637, commit e01e2afad5f0ec958a271cfe949eed8bdc56fe8b; this pipeline 3018380432, commit 1159a084f849ea8dc489348bdd165974557dbb81 (app-identical).

# App test results (platform=macos, backend=mac, app_display=wayland, seed=1, commit=1159a084, owner_settings=sha256 a0613b641a27)

pass 14, fail 14, skip 0

| mode | case | status | detail |
|---|---|---|---|
| terminal | mode_proof | PASS |  |
| terminal | cadence | PASS |  |
| terminal | key_loss_control | FAIL | harness exception: RuntimeError("frontmost app is 'Phosphor', not Terminal: refusing to send keys"); reference windows left open: Terminal was already running ( |
| terminal | enter_loss_control | PASS | reference windows left open: Terminal was already running (pid 66832); reference Terminal.app (.command via open) gap 0.00s: 0/30 lines missing (merged/lost Ent |
| terminal | fast_typing | PASS |  |
| terminal | paste_instant_enter | PASS |  |
| terminal | tab_enter | PASS |  |
| terminal | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; Ctrl-D in cat: file ['helloecho done1 >> cd.txt']; nested bash not entered (SHLVL lines []): Ctrl-D |
| terminal | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T163_c71lmo never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phos |
| terminal | latency_enter | FAIL | only 22/30 Enter timings captured; Enter to command start: Phosphor overhead 10.33 ms over Terminal.app (.command via open) exceeds the 2 ms budget; reference w |
| terminal | latency_key_log | PASS | key to frame: no reference terminal measured in this session, overhead verdict 'no-reference' (absolute numbers recorded); key to frame: Phosphor-side keydown t |
| terminal | latency_paint | FAIL | Key to paint: Phosphor overhead 7.33 ms over Terminal.app (.command via open) exceeds the 2 ms budget; Key to paint: kl3 measures post-to-first-changed-pixel at |
| terminal | shell_fish | FAIL | fish: autosuggestion (a history line sharing the typed prefix) did not change the executed line: expected ['AUTOSUGG_alpha_t180_dmdhfosq', 'AUTOSUGG_beta_t181_7 |
| terminal | shell_nu | FAIL | nu: 12 typed lines: expected ['t185_z46bxitkti', 't186_k3jikfqpwu', 't187_r4te1j9w2g', 't188_uel48ccmwx', 't189_6xvhlycrnd', 't190_tuzpxdosam', 't191_iox6rjkoet |
| terminal | md_viewer | PASS |  |
| agent | mode_proof | PASS |  |
| agent | cadence | FAIL | ts_reader produced no output (python3 missing or app did not run it) |
| agent | fast_typing | PASS |  |
| agent | paste_instant_enter | PASS |  |
| agent | tab_enter | FAIL | Tab+Enter: completed file has 1/12 expected lines; Tab+Enter ran the UNCOMPLETED line, stray files: ['zzcompl_alpha_u'] |
| agent | ctrl_c_d | FAIL | Ctrl-C / command-after-Ctrl-C delivered fewer than 33 times; `cat > cd.txt` never started (its Enter was lost?): Ctrl-D NOT sent (it would hit the top-level she |
| agent | ssh_connect_typing | FAIL | ssh_connect_typing (gap 0.0 s): T79_1ereql never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phosp |
| agent | latency_enter | FAIL | only 20/30 Enter timings captured; Enter to command start: Phosphor overhead 8.88 ms over Terminal.app (.command via open) exceeds the 2 ms budget; reference wi |
| agent | latency_key_log | PASS | key to frame: no reference terminal measured in this session, overhead verdict 'no-reference' (absolute numbers recorded); key to frame: Phosphor-side keydown t |
| agent | latency_paint | PASS | Key to paint: kl3 measures post-to-first-changed-pixel at 240 Hz in each terminal's own window; reference measured after Phosphor in the same session |
| agent | shell_fish | FAIL | fish: after exit the pane's bash ran the next command: expected ['t100_q08s58nv'], got []; fish_rec.log empty: the fish_postexec hook from config.fish did not r |
| agent | shell_nu | FAIL | nu: 12 typed lines: expected ['t101_5gawrd82tg', 't102_6rrp0jiqm0', 't103_86j0rr4tr5', 't104_5x4pvll28j', 't105_6u7inu54vh', 't106_iqof8dlhom', 't107_1uabtoforv |
| agent | md_viewer | PASS |  |

## Latency (macos): median / p95 ms, warm-up discarded

| mode | case/measure | Phosphor | reference | overhead ms | budget | previous build | baseline | verdict | n |
|---|---|---|---|---|---|---|---|---|---|
| terminal | ctrl_c_d/ctrl_c | 199.0 / 206.5 | 193.5 / 205.8 (Terminal.app (.command via open)) | 5.53 | overhead <= 20 ms | - | pending | PASS | n=28 |
| terminal | latency_enter/enter_to_start | 201.2 / 213.5 | 190.8 / 205.8 (Terminal.app (.command via open)) | 10.33 | overhead <= 2 ms | - | pending | FAIL | n=22 |
| terminal | latency_key_log/key_to_frame | 7.8 / 9.4 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=52 |
| terminal | latency_key_log/ui_work | worst 0 ms | - | - | <= 4 ms (warn 16) | - | - | PASS | n=0 |
| terminal | latency_paint/key_to_paint | 34.8 / 46.1 | 27.4 / 69.8 (Terminal.app (.command via open)) | 7.33 | overhead <= 2 ms | - | pending | FAIL | n=30 |
| agent | ctrl_c_d/ctrl_c | 201.0 / 212.2 | 192.6 / 209.9 (Terminal.app (.command via open)) | 8.33 | overhead <= 20 ms | - | pending | PASS | n=28 |
| agent | latency_enter/enter_to_start | 198.9 / 212.9 | 190.1 / 205.9 (Terminal.app (.command via open)) | 8.88 | overhead <= 2 ms | - | pending | FAIL | n=20 |
| agent | latency_key_log/key_to_frame | 8.4 / 9.3 | - | - | overhead <= 2 ms | - | pending | NO-REFERENCE | n=55 |
| agent | latency_key_log/ui_work | worst 0 ms | - | - | <= 4 ms (warn 16) | - | - | PASS | n=0 |
| agent | latency_paint/key_to_paint | 34.4 / 41.8 | 33.9 / 74.8 (Terminal.app (.command via open)) | 0.48 | overhead <= 2 ms | - | pending | PASS | n=30 |

## Enter-loss control (#1482): reference-terminal lines missing after a fast burst with instant Enter

| mode | reference | gap s | missing | n |
|---|---|---|---|---|
| terminal | Terminal.app (.command via open) | 0.0 | 0 | 30 |
| terminal | Terminal.app (.command via open) | 0.25 | 0 | 30 |
