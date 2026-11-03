# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
cd ~/phosphor-profile/r18aq/lat; source h3.sh
mkdir -p raw/tab
for i in $(seq 1 20); do
  guard || exit 1
  c0=$(wc -l < ctl/log)
  ./htype2 367 170 1028 699 raw/tab/none_$i.out "t:zzqqxx:12:18" "x:0" "e:15" >/dev/null 2>&1
  sleep 1.3
  tail -n +$((c0+1)) ctl/log | grep -E '^start' > raw/tab/none_$i.ctl
done
for i in $(seq 1 15); do
  guard || exit 1
  c0=$(wc -l < ctl/log)
  ./htype2 367 170 1028 699 raw/tab/real_$i.out "t:git statu:12:18" "x:0" "e:15" >/dev/null 2>&1
  sleep 1.5
  tail -n +$((c0+1)) ctl/log | grep -E '^start' > raw/tab/real_$i.ctl
done
echo TAB-DONE
