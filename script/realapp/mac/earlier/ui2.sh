# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
cd ~/phosphor-profile/r18aq/lat; source h3.sh
for p in "431,432 appearance" "423,494 keyboard" "420,584 privacy" "423,615 network"; do
  guard || exit 1; cliclick c:${p%% *}; sleep 1.2; shot set_${p##* }
done
kill -0 $(apppid) && echo ALIVE
