#!/bin/zsh
# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
# fail.sh LABEL "offs": failing ssh (conn refused) + continuous line+Enter; must not run locally
cd ~/phosphor-profile/r18bi/lat; source ./h.sh; L=$1; OFFS=(${=2}); N=${#OFFS}
for i in $(seq 1 $N); do
  guard || { echo GUARD-FAIL; exit 1; }
  off=${OFFS[$i]}; f=/tmp/zq-local-${L}$i.log; rm -f $f; l0=$(wc -l < stamped.log); c0=$(wc -l < ctl/log)
  ./htype2 367 170 1028 699 raw/fail_${L}_$i.out "t:ssh -o BatchMode=yes -o IdentityFile=/dev/null nosuchzq@${PHOSPHOR_BOX_HOST}:12:18" "e:250" "p:$off" "t:echo zqloc${L}$i >> $f:12:18" "e:60" >/dev/null 2>&1
  sleep 1.2; shot fail1_${L}_$i; sleep 4; shot fail_${L}_$i
  ran=no; [ -e $f ] && ran=YES-LOCALLY
  tail -n +$((c0+1)) ctl/log | grep '^start' | cut -c1-120 > raw/fail_${L}_$i.start
  tail -n +$((l0+1)) stamped.log | grep -iE "held back|ssh_drop|can_execute|handover|kh_iq_release|discard" | cut -c1-240 > raw/fail_${L}_$i.log
  echo "fail $L$i off=$off ran_locally=$ran startlines=$(wc -l < raw/fail_${L}_$i.start)"
  rm -f $f
  guard && { ./htype2 367 170 1028 699 raw/fail_clr_$i.out "b:70:5" >/dev/null 2>&1; }
done
