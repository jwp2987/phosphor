#!/bin/bash
# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
# own.sh MODE TEXT SECS LOG : start xowner, record pid
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
export DISPLAY=$D
[ -f $R/owner.pid ] && kill $(cat $R/owner.pid) 2>/dev/null
SEL=${SEL:-CLIPBOARD} nohup $R/venv/bin/python $REALAPP/bin/xowner.py "$1" "$2" "$3" > $4 2>&1 &
echo $! > $R/owner.pid
