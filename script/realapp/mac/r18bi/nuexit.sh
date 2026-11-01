#!/bin/zsh
# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
cd ~/phosphor-profile/r18bi/lat; source ./h.sh; export PATH=/opt/homebrew/bin:$PATH
for i in 1 2 3; do
  kill -0 $(apppid) 2>/dev/null || { echo "APP-DEAD -> relaunch"; launch >/dev/null; sleep 4; }
  guard || exit 1
  c0=$(wc -l < ctl/log)
  ./htype2 367 170 1028 699 /dev/null "t:echo ppid_before=\$\$ >> /tmp/zq-pid.log:10:20" "e:30" >/dev/null 2>&1; sleep 1
  ./htype2 367 170 1028 699 /dev/null "t:nu:12:18" "e:250" >/dev/null 2>&1; sleep 3; guard || exit 1
  if [ $((i % 2)) = 1 ]; then cliclick kd:ctrl t:i ku:ctrl; sleep 3; fi
  ./htype2 367 170 1028 699 /dev/null "t:exit:12:18" "e:60" >/dev/null 2>&1; sleep 4; shot nuexit_$i
  alive=$(kill -0 $(apppid) 2>/dev/null && echo app-alive || echo APP-DEAD)
  guard && ./htype2 367 170 1028 699 /dev/null "t:echo ppid_after=\$\$ >> /tmp/zq-pid.log:10:20" "e:30" >/dev/null 2>&1; sleep 1.5
  echo "nuexit $i phos=$((i%2)) $alive: $(tail -2 /tmp/zq-pid.log | tr '\n' ' ')"
done
