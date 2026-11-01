#!/bin/bash
# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
P=$(pgrep -f 'linux-r18bi/dist/x/squashfs-root/opt/phosphor/phosphor-oss$' | head -1)
[ -n "$P" ] && kill $P; sleep 2; pkill -f 'linux-r18bi/dist/x/squashfs-root.*dbus' 2>/dev/null; true
