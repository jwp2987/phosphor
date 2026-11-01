# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
cd ~/phosphor-profile/r18ba/lat; source h3.sh
osascript -e 'tell application "Terminal" to close (window id '$(cat tterm.wid)') saving no' 2>&1
sleep 1
osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $(apppid)) to true"; sleep 1
guard || exit 1
cmd t; sleep 2; osascript -e 'tell application "System Events" to key code 53'; sleep 1.5
guard || exit 1
# a) @ picker
c0=$(wc -l < ctl/log)
cliclick -w 30 t:"@"; sleep 1.5; shot sm_at1
osascript -e 'tell application "System Events" to key code 53'; sleep .5
# clear input: select all + delete
cmd a; osascript -e 'tell application "System Events" to key code 51'; sleep .5; shot sm_at2
# b) 60k lines
guard || exit 1
cliclick -w 15 t:"seq 1 60000"; sleep .4; ret; sleep 6; shot sm_seq
guard || exit 1
cmd f; sleep 1; cliclick -w 40 t:"1"; sleep 4; shot sm_find1
ps -o %cpu,rss -p $(apppid) > raw/find_ps.txt
osascript -e 'tell application "System Events" to key code 53'; sleep 1
# c) scrolled back + resize
guard || exit 1
for i in 1 2 3 4 5 6; do osascript -e 'tell application "System Events" to key code 116 using {shift down}'; done; sleep 1; shot sm_scroll
osascript -e 'tell application "System Events" to tell process "phosphor-oss" to set size of window 1 to {900, 600}'; sleep 2; shot sm_resize1
osascript -e 'tell application "System Events" to tell process "phosphor-oss" to set size of window 1 to {1120, 760}'; sleep 2; shot sm_resize2
kill -0 $(apppid) && echo alive-after-resize
# d) copy then immediate paste
guard || exit 1
osascript -e 'tell application "System Events" to key code 119 using {shift down}' ; sleep .3
cliclick -w 15 t:"echo cpy7431"; sleep .4
cmd a; osascript -e 'tell application "System Events"' -e 'keystroke "c" using command down' -e 'key code 51' -e 'keystroke "v" using command down' -e 'end tell'
sleep .5; shot sm_copy1
c0=$(wc -l < ctl/log); ret; sleep 1.5; tail -n +$((c0+1)) ctl/log | grep '^start'; shot sm_copy2
echo SMOKE-DONE
