#!/bin/bash
# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
# relaunch.sh [ENV=val ...] : kill our app (exact pid), relaunch, open terminal-mode tab in out/
cd ${PHOSPHOR_LINUX_RUN}; source raw/app.env; kill $APP 2>/dev/null; sleep 4
./launch.sh "$@"; sleep 14; . ./f.sh
APP=$(ps -eo pid,cmd | grep "linux-r18bi/dist/x/squashfs-root/opt/phosphor/phosphor-oss$" | grep -v grep | awk '{print $1}'); echo "APP=$APP" > raw/app.env
xdotool windowfocus $W; xdotool key ctrl+shift+t; sleep 2.5; xdotool key Escape; sleep 1; xdotool type --delay 12 -- "cd $R/out && pwd"; xdotool key Return; sleep 1.5
