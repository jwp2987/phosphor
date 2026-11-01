#!/bin/bash
# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
# waits until no CI job running and perf-remote.lock free; writes box-ready flag
for i in $(seq 1 420); do
  RUN=$(glab api "projects/88085499/jobs?scope[]=running" 2>/dev/null | python3 -c "import sys,json; print(len([j for j in json.load(sys.stdin) if 'linux' in j['name']]))" 2>/dev/null)
  LK=$(ssh -o BatchMode=yes -o ConnectTimeout=8 ${PHOSPHOR_BOX_SSH} 'test -d ~/perf-remote.lock && echo held || echo free' 2>/dev/null)
  echo "$(date +%T) run=$RUN lock=$LK" >> ${PHOSPHOR_LINUX_RUN}/raw/waitbox.log
  if [ "$RUN" = "0" ] && [ "$LK" = "free" ]; then echo ready > ${PHOSPHOR_LINUX_RUN}/box-ready; exit 0; fi
  sleep 60
done
