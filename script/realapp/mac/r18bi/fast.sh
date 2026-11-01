#!/bin/zsh
# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
# fast.sh TAG N : N commands, each typed 10-25ms/key then Enter after 15ms, next typed immediately (one htype2 run)
cd ~/phosphor-profile/r18bi/lat; source ./h.sh; T=$1; N=${2:-20}
guard || { echo GUARD-FAIL; exit 1; }
ops=(); for i in $(seq 1 $N); do ops+=("t:echo fx${T}_${i}_the_quick_brown_fox:10:25" "e:15"); done
c0=$(wc -l < ctl/log)
./htype2 367 170 1028 699 raw/fast_$T.out $ops >/dev/null 2>&1
sleep 4
tail -n +$((c0+1)) ctl/log | grep '^start' | grep "fx${T}_" | sed 's/^start [0-9.]* //' > raw/fast_$T.got
for i in $(seq 1 $N); do echo "echo fx${T}_${i}_the_quick_brown_fox"; done > raw/fast_$T.exp
if diff -q raw/fast_$T.exp raw/fast_$T.got >/dev/null; then echo "FAST $T PASS $N/$N"; else echo "FAST $T FAIL"; diff raw/fast_$T.exp raw/fast_$T.got | head -10; fi
shot fast_$T
