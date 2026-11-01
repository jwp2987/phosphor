#!/bin/zsh
# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
cd ~/phosphor-profile/r18bi/lat; source ./h.sh; L=$1
for i in 1 2 3; do
  guard || { echo GUARD-FAIL; exit 1; }
  c0=$(wc -l < ctl/log); l0=$(wc -l < stamped.log)
  ./htype2 367 170 1028 699 /dev/null "t:ssh -o PreferredAuthentications=password -o PubkeyAuthentication=no -o NumberOfPasswordPrompts=1 ${PHOSPHOR_BOX_SSH}:12:18" "e:250" >/dev/null 2>&1
  sleep 3; shot pw1_${L}_$i
  guard || exit 1
  ./htype2 367 170 1028 699 /dev/null "t:notthepassword$i:12:25" "e:30" >/dev/null 2>&1
  sleep 3; shot pw2_${L}_$i
  ./htype2 367 170 1028 699 /dev/null "t:echo zqpwok${L}$i:12:18" "e:40" >/dev/null 2>&1
  sleep 2; shot pw3_${L}_$i
  echo "pw $L$i: starts=$(tail -n +$((c0+1)) ctl/log | grep '^start' | cut -c1-100 | sed 's/^start [0-9.]* //' | tr '\n' '|') pwleak_in_log=$(tail -n +$((l0+1)) stamped.log | grep -c notthepassword)"
done
