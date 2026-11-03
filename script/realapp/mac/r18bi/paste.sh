#!/bin/zsh
# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
# paste.sh TAG : paste+instant Enter x10, paste+Left x3+type+Enter x10, Tab+Enter x10
cd ~/phosphor-profile/r18bi/lat; source ./h.sh; T=$1
chk(){ # name expected_file
  tail -n +$((c0+1)) ctl/log | grep '^start' | sed 's/^start [0-9.]* //' > raw/$1.got
  if diff -q raw/$1.exp raw/$1.got >/dev/null; then echo "$1 PASS $(wc -l < raw/$1.exp)"; else echo "$1 FAIL"; diff raw/$1.exp raw/$1.got | head -8; fi }
[ "$2" = p3 ] || {
# P1
: > raw/P1$T.exp; c0=$(wc -l < ctl/log)
for i in $(seq 1 10); do guard || exit 1; s="echo pasteA${T}_${i}_zzzzzzzzzzzz"; printf %s "$s" | pbcopy; echo $s >> raw/P1$T.exp
  osascript -e 'tell application "System Events"' -e 'keystroke "v" using command down' -e 'key code 36' -e 'end tell'; sleep 1.3; done
sleep 1; chk P1$T; shot paste1_$T
# P2 : paste then 3 Left then type ZZ then Enter -> "echo pasteB..abcZZdef"
: > raw/P2$T.exp; c0=$(wc -l < ctl/log)
for i in $(seq 1 10); do guard || exit 1; printf %s "echo pasteB${T}_${i}_abcdef" | pbcopy; echo "echo pasteB${T}_${i}_abcZZdef" >> raw/P2$T.exp
  osascript -e 'tell application "System Events"' -e 'keystroke "v" using command down' -e 'key code 123' -e 'key code 123' -e 'key code 123' -e 'keystroke "ZZ"' -e 'key code 36' -e 'end tell'; sleep 1.3; done
sleep 1; chk P2$T; shot paste2_$T
}
# P3 Tab + Enter
: > raw/P3$T.exp; c0=$(wc -l < ctl/log)
for i in $(seq 1 10); do guard || exit 1; echo "cd ~/zqdir_unique_name/" >> raw/P3$T.exp
  ./htype2 367 170 1028 699 raw/P3$T.$i.out "t:cd ~/zqdir_un:10:25" "x:20" "e:15" >/dev/null 2>&1; sleep 1.3; done
sleep 1; chk P3$T; shot paste3_$T
