#!/bin/zsh
# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
cd ~/phosphor-profile/r18bi/lat; source ./h.sh; L=$1; OFFS=(${=2}); N=${#OFFS}; F=/tmp/zq-sub.log
for i in $(seq 1 $N); do
  kill -0 $(apppid) 2>/dev/null || { echo "APP-DEAD before trial $i -> relaunch"; launch >/dev/null; sleep 3; }
  guard || { echo GUARD-FAIL; exit 1; }
  off=${OFFS[$i]}; id=$(printf "zqS%s%02d" $L $i); line="\"$id\\n\" | save -a $F"
  l0=$(wc -l < stamped.log)
  ./htype2 367 170 1028 699 /dev/null "t:nu:12:18" "e:250" >/dev/null 2>&1; sleep 3; guard || exit 1
  cliclick kd:ctrl t:i ku:ctrl
  ./htype2 367 170 1028 699 raw/nu_${L}_$i.out "p:$off" "t:$line:12:18" "e:60" >/dev/null 2>&1
  sleep 6; shot nu_${L}_$i
  ok=NO; grep -q "^$id\$" $F 2>/dev/null && ok=yes
  skipped=$(tail -n +$((l0+1)) stamped.log | grep -c "setup skipped\|young hold expired")
  echo "nu $L$i off=$off file_line=$ok young_hold_or_skipped_log=$skipped"
  guard && ./htype2 367 170 1028 699 /dev/null "t:exit:12:18" "e:60" >/dev/null 2>&1; sleep 2.5
done
