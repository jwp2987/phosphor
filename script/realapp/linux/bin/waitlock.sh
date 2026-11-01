#!/bin/bash
# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
for i in $(seq 1 500); do
  LK=$(ssh -F ${PHOSPHOR_LINUX_RUN}/home/.ssh/config -o BatchMode=yes -o ConnectTimeout=8 ${PHOSPHOR_BOX_HOST} 'test -e ~/perf-remote.lock && cat ~/perf-remote.lock || echo free' 2>/dev/null)
  echo "$(date +%T) $LK" >> ${PHOSPHOR_LINUX_RUN}/raw/waitlock.log
  if [ "$LK" = "free" ]; then echo free > ${PHOSPHOR_LINUX_RUN}/lock-free; exit 0; fi
  sleep 30
done
