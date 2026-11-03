#!/bin/zsh
# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
# ssh2.sh LABEL "offs..." : expected exact lines <id> in box:/tmp/zq-mac.log
cd ~/phosphor-profile/r18bi/lat; source ./h.sh; L=$1; OFFS=(${=2}); N=${#OFFS}
mkdir -p raw/ssh
for i in $(seq 1 $N); do
  guard || { echo "GUARD-FAIL i=$i"; exit 1; }
  off=${OFFS[$i]}; id=$(printf "zq%s%02d" $L $i)
  l0=$(wc -l < stamped.log)
  ./htype2 367 170 1028 699 raw/ssh/${L}_$i.out "t:ssh ${PHOSPHOR_BOX_SSH}:12:18" "e:250" "p:$off" "t:echo $id >> /tmp/zq-mac.log:12:18" "e:60" > /dev/null 2>&1
  sleep ${CONNW:-6}; shot ssh_${L}_$i
  guard || { echo "GUARD-FAIL-after i=$i"; exit 1; }
  ./htype2 367 170 1028 699 raw/ssh/${L}_exit_$i.out "b:45:6" "t:exit:12:18" "e:60" > /dev/null 2>&1
  sleep 2.5
  tail -n +$((l0+1)) stamped.log | grep -E "ssh|hold|drop|can_execute|kh_handover|held back" | cut -c1-240 > raw/ssh/${L}_$i.log
  echo "trial $i off=$off id=$id"
done
echo SSH-DONE $L
