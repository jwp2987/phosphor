#!/bin/zsh
# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
cd ~/phosphor-profile/r18bi/lat; source ./h.sh; T=$1
guard || exit 1
python3 -c "import sys;sys.stdout.write('echo bigpaste${T} '+'a'*1300000)" | pbcopy
c0=$(wc -l < ctl/log); t0=$(python3 -c 'import time;print(time.time())')
osascript -e 'tell application "System Events"' -e 'keystroke "v" using command down' -e 'key code 36' -e 'end tell'
for s in 1 2 3 4 5 6 7 8 9 10 12 15 20; do sleep 1; grep -aq '^end' <(tail -n +$((c0+1)) ctl/log) && break; done
shot big_$T
tail -n +$((c0+1)) ctl/log | grep -a '^start' | awk '{print "bigstart len="length($0)" head="substr($0,1,40)}'
tail -n +$((c0+1)) ctl/log | grep -a '^end' | head -2
ps -o %cpu,rss -p $(apppid)
# Ctrl-C mid command
for i in 1 2 3; do guard || exit 1
 c0=$(wc -l < ctl/log)
 ./htype2 367 170 1028 699 /dev/null "t:sleep 30:10:25" "e:15" >/dev/null 2>&1; sleep 1.2
 t1=$(python3 -c 'import time;print(time.time())'); guard && cliclick kd:ctrl t:c ku:ctrl; sleep 1
 tail -n +$((c0+1)) ctl/log | grep -a '^end' | awk -v t1=$t1 '{printf "ctrlc%s: end_status=%s delay_ms=%.0f\n","'$i'",$3,($2-t1)*1000}'
done
