#!/bin/zsh
# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
# ks.sh ENVVAR LABEL : relaunch with ENVVAR=0 and run 40 ssh trials (300/100 alternating), terminal mode
cd ~/phosphor-profile/r18bi/lat; source ./h.sh
killapp; sleep 1
: > ctl/log
open -n -g --env HOME=$H --env ZDOTDIR=$R/zdot --env SHELL=/bin/zsh --env RUST_LOG=warp::input_order=debug,phosphor::routing=debug --env $1=0 $APP; sleep 6
pgrep -n -f "$APP/Contents/MacOS/phosphor-oss$" > $R/app.pid; echo "app pid $(apppid) with $1=0"
osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $(apppid)) to true"; sleep 2; shot ks_$2
CONNW=6 ./ssh2.sh $2 "$(printf '300 100 %.0s' {1..20})"
