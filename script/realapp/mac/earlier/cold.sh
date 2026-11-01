# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
cd ~/phosphor-profile/r18ba/lat; source h3.sh
for k in 1 2 3; do
  guard && cmd q; sleep 3; kill -0 $(apppid) 2>/dev/null && kill $(apppid); sleep 1
  RL="warp::input_order=debug,phosphor::routing=debug" launch; sleep 3
  guard && osascript -e 'tell application "System Events" to key code 53'; sleep 1.5
  shot cold_pre_$k
  zsh ssh2.sh c$k "100"
done
echo COLD-DONE
