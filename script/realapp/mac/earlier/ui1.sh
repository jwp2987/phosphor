# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
cd ~/phosphor-profile/r18aq/lat; source h3.sh
guard || exit 1
esc(){ osascript -e 'tell application "System Events" to key code 53'; }
cliclick rc:700,500; sleep 1; shot ui_rightclick; esc; sleep .5
guard || exit 1
cliclick kd:cmd t:f ku:cmd; sleep 1; cliclick -w 30 t:"echo"; sleep 1.2; shot ui_find; esc; sleep .5; esc; sleep .5
guard || exit 1
cliclick kd:cmd t:, ku:cmd; sleep 2; shot ui_settings1
echo front=$(front); kill -0 $(apppid) && echo ALIVE
