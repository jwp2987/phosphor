#!/bin/zsh
# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
cd ~/phosphor-profile/r18bi/lat; source ./h.sh
guard || exit 1
./htype2 367 170 1028 699 /dev/null 't:printf "\033[?1049h\033[2J\033[H abs= %s\r\n rel= ./zqdir_unique_name/zqfile.txt\r\n" "$HOME/zqdir_unique_name/zqfile.txt"; sleep 25; printf "\033[?1049l":12:18' "e:30" >/dev/null 2>&1
sleep 2; shot alt1
