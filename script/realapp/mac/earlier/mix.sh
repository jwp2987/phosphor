# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
cd ~/phosphor-profile/r18aq/lat; source h3.sh; L=$1; M=$2
MIX="ls -la|git status|git log -1 --oneline|echo hi|cat about.toml"
guard || exit 1
./gate.sh ${L}_$M 2>&1 | tail -1
guard || exit 1
RECT="367 170 1028 699" PAUSE=1.0 MODE=T REPOCHK=1 python3 drvm.py $L A_T_$M "$MIX" 40 1000
guard || exit 1
RECT="367 170 1028 699" PAUSE=1.0 MODE=F REPOCHK=1 python3 drvm.py $L A_F_$M "$MIX" 40 1000
echo MIX-DONE
