# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
cd ~/phosphor-profile/r18aq/lat; source h3.sh
sz(){ osascript -e "tell application \"System Events\" to tell process \"phosphor-oss\" to set size of window 1 to {$1,$2}"; }
guard || exit 1; cliclick c:255,239; sleep 1.5
guard || exit 1; cliclick kd:cmd,shift kp:arrow-up ku:cmd,shift; sleep 1; shot rs_a
sz 800 700; sleep 1.5; shot rs_b
sz 1278 799; sleep 1.5; shot rs_c
sz 1000 600; sleep 0.3; shot rs_d_fast; sleep 1.5; shot rs_d
sz 1278 799; sleep 1.5
kill -0 $(apppid) && echo ALIVE; grep -c -E "panic" $LOG
