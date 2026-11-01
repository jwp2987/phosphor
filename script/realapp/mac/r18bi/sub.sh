#!/bin/zsh
# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
# sub.sh SH LABEL "offs" : start subshell SH, type marker line at offset after Enter, then exit
cd ~/phosphor-profile/r18bi/lat; source ./h.sh; SH=$1; L=$2; OFFS=(${=3}); N=${#OFFS}
F=/tmp/zq-sub.log
for i in $(seq 1 $N); do
  guard || { echo GUARD-FAIL; exit 1; }
  off=${OFFS[$i]}; id=$(printf "zqS%s%02d" $L $i)
  if [ "$SH" = nu ]; then line="\"$id\\n\" | save -a $F"; else line="echo $id >> $F"; fi
  l0=$(wc -l < stamped.log)
  if [ -n "$PHOS" ]; then
    ./htype2 367 170 1028 699 /dev/null "t:$SH:12:18" "e:250" >/dev/null 2>&1; sleep ${PRE:-2}; guard || exit 1
    cliclick kd:ctrl t:i ku:ctrl
    ./htype2 367 170 1028 699 raw/sub_${L}_$i.out "p:$off" "t:$line:12:18" "e:60" >/dev/null 2>&1
  else
    ./htype2 367 170 1028 699 raw/sub_${L}_$i.out "t:$SH:12:18" "e:250" "p:$off" "t:$line:12:18" "e:60" >/dev/null 2>&1
  fi
  sleep ${SUBW:-5}; shot sub_${L}_$i
  guard || { echo GUARD-FAIL-after; exit 1; }
  ./htype2 367 170 1028 699 /dev/null "t:exit:12:18" "e:60" >/dev/null 2>&1; sleep 2
  echo "sub $L$i off=$off"
done
echo "got: $(grep -c "^zqS$L" $F 2>/dev/null) / $N"
