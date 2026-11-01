# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
import sys,json,statistics
a=[x for x in json.loads(sys.stdin.read().strip().splitlines()[-1]) if x is not None]
a.sort(); print('n=%d median %.1f p95 %.1f max %.1f'%(len(a),statistics.median(a),a[int(len(a)*.95)-1],a[-1]))
