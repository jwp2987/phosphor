#!/bin/zsh
# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
cd ~/phosphor-profile/r18bi/lat; source ./h.sh
TAG=$1; APP=${2:-phos}; PID=${3:-$(apppid)}; PN=${4:-phosphor-oss}
shot lat_${TAG}_before
for c in "key 40" "enter 20" "burst 8" "scroll 6"; do set -- ${=c}; python3 lat.py $APP $1 $2 $PID $PN | sed "s/^/$TAG /"; done
shot lat_${TAG}_after
