#!/bin/zsh
# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
cd ~/phosphor-profile/r18bi/lat; source ./h.sh
cpu(){ top -l 7 -s 4 -pid $(apppid) -stats pid,cpu,threads 2>/dev/null | grep "^$(apppid)" | tail -5 | awk '{s+=$2; printf "%s ",$2} END{printf "=> avg %.1f\n", s/NR}'; }
guard || exit 1
shot idle_term_prompt; sleep 3
echo "idle quiet prompt ($1): $(cpu)"
./htype2 367 170 1028 699 /dev/null "t:vim -u NONE -N:12:18" "e:30" >/dev/null 2>&1; sleep 3; shot idle_vim
echo "idle vim, mouse parked ($1): $(cpu)"
./htype2 367 170 1028 699 /dev/null "t\:q!:12:18" >/dev/null 2>&1
guard && cliclick kp:esc; sleep .3; ./htype2 367 170 1028 699 /dev/null "t::q!:12:18" "e:30" >/dev/null 2>&1; sleep 2; shot idle_after_vim
echo "idle quiet prompt after vim ($1): $(cpu)"
