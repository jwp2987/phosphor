#!/bin/bash
# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
# cpu2.sh PID SECS
P=$1; a=$(awk '{print $14+$15}' /proc/$P/stat); sleep $2; b=$(awk '{print $14+$15}' /proc/$P/stat); echo "$(( (b-a)*100 / ($2*100) ))% of one core ($((b-a)) ticks / $2 s)  rss=$(awk '/VmRSS/{print $2}' /proc/$P/status)kB"
