#!/bin/bash
# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
# tapp.sh : Terminal.app control batch (K, F, T) -> raw/q_ctl_*
cd ~/phosphor-profile/r18aq/lat; source h3.sh
export APPNAME=Terminal RECT="110 110 700 400"
tg(){ [ "$(front)" = Terminal ] || { echo "GUARD: frontmost $(front) not Terminal" >&2; return 1; }; }
osascript -e 'tell application "Terminal" to activate' -e 'tell application "Terminal" to do script "exec env ZDOTDIR=$HOME/phosphor-profile/r18aq/lat/zdot zsh -f -c \"source \\$ZDOTDIR/.zshrc; exec zsh -i\"" ' >/dev/null 2>&1
sleep 2
osascript -e 'tell application "Terminal" to set bounds of front window to {110,110,810,510}'
sleep 1
tg || exit 1
echo front-ok; osascript -e 'tell application "Terminal" to get id of front window'
