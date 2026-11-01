# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
cd ~/phosphor-profile/r18ba/lat; source h3.sh
guard || exit 1
cliclick m:900,500
python3 - <<'PY'
import Quartz,time
for i in range(30):
    e=Quartz.CGEventCreateScrollWheelEvent(None,Quartz.kCGScrollEventUnitLine,1,15); Quartz.CGEventPost(Quartz.kCGHIDEventTap,e); time.sleep(0.02)
PY
sleep 1; shot sc_a
osascript -e 'tell application "System Events" to tell process "phosphor-oss" to set size of window 1 to {900, 600}'; sleep 2; shot sc_b
osascript -e 'tell application "System Events" to tell process "phosphor-oss" to set size of window 1 to {1120, 760}'; sleep 2; shot sc_c
kill -0 $(apppid) && echo alive
echo SC-DONE
