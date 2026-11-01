#!/bin/bash
# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
# handoff.sh LABEL [ENV=VAL...]  (BIN_OVERRIDE may be set)
. "$(dirname "${BASH_SOURCE[0]}")/../h.sh"
L=$1; shift
(uv run --with python-xlib $REALAPP/bin/xmgr.py 100 > $R/raw/xmgr-$L.log 2>&1 &)
$R/launch.sh "$@"; sleep 11
W=$(xdotool search --name "." | while read w; do xwininfo -id $w 2>/dev/null | grep -q "Width: 1[0-9][0-9][0-9]" && echo $w; done | head -1)
xdotool windowsize $W 1600 1000; xdotool windowmove $W 0 0; xdotool windowfocus $W; sleep 1
xdotool mousemove 900 924 click 1; sleep .5
t "HANDOFF-$L"; xdotool key --delay 0 ctrl+a ctrl+shift+c; sleep .5
echo "before quit: $(uv run --with python-xlib $REALAPP/bin/xget.py UTF8_STRING 2>/dev/null)"
xdotool mousemove 1573 17 click 1; sleep 4
echo "app procs left: $(ps -eo pid,cmd | grep "$(basename $(dirname $(dirname $BIN)))" | grep -v grep | wc -l)"
echo "after quit: $(uv run --with python-xlib $REALAPP/bin/xget.py UTF8_STRING 2>&1 | head -3)"
cat $R/raw/xmgr-$L.log
pkill -f "xmgr.py 100" 2>/dev/null
