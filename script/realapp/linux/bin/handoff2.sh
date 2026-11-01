#!/bin/bash
# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
. "$(dirname "${BASH_SOURCE[0]}")/../h.sh"
L=$1; shift
$R/venv/bin/python $REALAPP/bin/xmgr.py 100 > $R/raw/xmgr-$L.log 2>&1 &
XM=$!
$R/launch.sh "$@"; sleep 11
W=$(xdotool search --name "." | while read w; do xwininfo -id $w 2>/dev/null | grep -q "Width: 1[0-9][0-9][0-9]" && echo $w; done | head -1)
xdotool windowsize $W 1600 1000; xdotool windowmove $W 0 0; xdotool windowfocus $W; sleep 1
xdotool key Escape; sleep 1
xdotool mousemove 900 924 click 1; sleep .5
t "HANDOFF-$L"; xdotool key --delay 0 ctrl+a ctrl+shift+c; sleep .5
echo "before quit: $($R/venv/bin/python $REALAPP/bin/xget.py UTF8_STRING 2>/dev/null)"
P=$(pgrep -f 'linux-r18bi/dist/x/squashfs-root/opt/phosphor/phosphor-oss$' | head -1)
T0=$(date +%s.%N)
xdotool mousemove 1573 17 click 1
while kill -0 $P 2>/dev/null; do sleep 0.02; done
echo "quit time: $(echo "$(date +%s.%N) - $T0" | bc) s"
sleep 1
echo "after quit: $($R/venv/bin/python $REALAPP/bin/xget.py UTF8_STRING 2>&1 | head -3 | tr '\n' '|')"
cat $R/raw/xmgr-$L.log
kill $XM
