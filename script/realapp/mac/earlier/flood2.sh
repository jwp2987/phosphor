#!/bin/bash
# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
# flood2.sh LABEL : fast output cases, durations from ctl log, shots mid-flood
cd ~/phosphor-profile/r18aq/lat; source h3.sh; L=$1
CS=("seq 1 2000000" "sh y50.sh" "sh bigcat.sh"); [ -n "$ONLY" ] && CS=("$ONLY")
echo 'yes | head -c 52428800' > $HOME/phosphor-profile/repo/y50.sh
echo 'cat $HOME/phosphor-profile/data/long.txt $HOME/phosphor-profile/data/long.txt $HOME/phosphor-profile/data/long.txt' > $HOME/phosphor-profile/repo/bigcat.sh
ls -la $HOME/phosphor-profile/data/long.txt
for c in "${CS[@]}"; do
  guard || { echo GUARD-FAIL; exit 1; }
  n0=$(wc -l < ctl/log); l0=$(wc -l < stamped.log)
  REPOCHK=0 MODE=T python3 drv.py $L fl "$c" 1 50 >/dev/null
  t0=$(python3 -c 'import time;print(time.time())')
  sleep 1; shot ${L}_flood_$(echo $c|tr -c a-z0-9 _)_1
  sleep 1; shot ${L}_flood_$(echo $c|tr -c a-z0-9 _)_2
  for i in $(seq 1 120); do tail -n +$((n0+1)) ctl/log | grep -q "^end" && break; sleep 1; done
  tail -n +$((n0+1)) ctl/log | grep -E "^(start|end)" | tr '\n' ' '; echo
  echo "CASE [$c] slow-warns: $(tail -n +$((l0+1)) stamped.log | grep -c -E 'Slow frames|slow frame')"
  tail -n +$((l0+1)) stamped.log | grep -E 'Slow frames|slow frame|slow model' | cut -c1-220 | tail -5
  sleep 4
done
echo FLOOD2-DONE
