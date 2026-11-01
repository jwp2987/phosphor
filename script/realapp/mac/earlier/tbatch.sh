#!/bin/bash
# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
cd ~/phosphor-profile/r18aq/lat; source h3.sh
export APPNAME=Terminal RECT="110 110 700 400" REPOCHK=0
tg(){ [ "$(front)" = Terminal ] || { echo "GUARD: frontmost $(front) not Terminal" >&2; return 1; }; }
tg && MODE=T PAUSE=0.5 python3 drvm.py gate clrT "clear" 1 300 >/dev/null; sleep 1
tg && KSETTLE=1500 PAUSE=0.2 MODE=K python3 drvm.py q_ctl K_term "a|DEL" 40 400
tg && PAUSE=0.9 MODE=F python3 drvm.py q_ctl F_term "echo hello world" 40 600
tg && PAUSE=0.9 MODE=T python3 drvm.py q_ctl T_term "echo hello world" 40 600
echo TBATCH-DONE
