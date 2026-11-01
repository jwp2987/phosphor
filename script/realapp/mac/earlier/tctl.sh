# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
cd ~/phosphor-profile/r18ba/lat; source h3.sh
WID=$(osascript -e 'tell application "Terminal"
 do script ""
 set b to {300,120,1328,820}
 set bounds of front window to b
 activate
 return id of front window
end tell')
echo $WID > tterm.wid; sleep 2
osascript -e 'tell application "Terminal" to do script "clear" in front window' >/dev/null
sleep 1; shot tapp_pre
f=$(front); echo "front=$f wid=$WID"; [ "$f" = Terminal ] || { echo ABORT; exit 1; }
RECT="305 150 1020 660"
export APPNAME=Terminal REPOCHK=0
RECT="$RECT" KSETTLE=1500 PAUSE=0.2 MODE=K python3 drvm.py ba_def K_termapp "a|DEL" 40 400
RECT="$RECT" PAUSE=0.9 MODE=F python3 drvm.py ba_def F_termapp "echo hello world" 40 600
RECT="$RECT" PAUSE=0.9 MODE=T python3 drvm.py ba_def T_termapp "echo hello world" 40 600
echo TAPP-DONE
