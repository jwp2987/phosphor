#!/bin/bash
# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
# cpu.sh SECS : process ticks (utime+stime) of the app main pid over SECS
P=$(pgrep -f 'squashfs-root/opt/phosphor/phosphor-oss$' | head -1)
a=$(awk '{print $14+$15}' /proc/$P/stat); sleep $1; b=$(awk '{print $14+$15}' /proc/$P/stat); echo "ticks=$((b-a)) over $1 s (pid $P)"
