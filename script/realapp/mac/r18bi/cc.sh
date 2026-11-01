#!/bin/zsh
# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
cd ~/phosphor-profile/r18bi/lat; source ./h.sh
for i in 1 2 3 4 5; do guard || exit 1
 c0=$(wc -l < ctl/log)
 ./htype2 367 170 1028 699 /dev/null "t:sleep 30:10:25" "e:15" >/dev/null 2>&1; sleep 1.2
 guard || exit 1
 t1=$(python3 -c 'import time;print(time.time())'); cliclick kd:ctrl t:c ku:ctrl; sleep 1
 tail -n +$((c0+1)) ctl/log | grep -a '^end' | awk -v t1=$t1 '{printf "ctrlc: end_status=%s delay_ms=%.0f\n",$3,($2-t1)*1000}'
done
# control: cliclick+python overhead
t1=$(python3 -c 'import time;print(time.time())'); cliclick kd:shift ku:shift; t2=$(python3 -c 'import time;print(time.time())'); python3 -c "print('control cliclick+py ms', ($t2-$t1)*1000)"
