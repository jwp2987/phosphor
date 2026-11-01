#!/usr/bin/env python3
# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
# xget.py TARGET : request CLIPBOARD target, print text, time it (acts as 'another X client')
import sys, time
from Xlib import X, display
d = display.Display(); s = d.screen()
w = s.root.create_window(0,0,1,1,0,s.root_depth)
A = d.intern_atom
tgt = A(sys.argv[1] if len(sys.argv)>1 else 'UTF8_STRING'); prop = A('XGET_PROP')
t0=time.time()
w.convert_selection(A('CLIPBOARD'), tgt, prop, X.CurrentTime); d.flush()
while time.time()-t0 < 5:
    if d.pending_events():
        e = d.next_event()
        if e.type == X.SelectionNotify:
            if e.property == X.NONE: print("NONE (no owner / refused)"); sys.exit(1)
            r = w.get_full_property(prop, X.AnyPropertyType)
            print(bytes(r.value).decode(errors='replace'), end=''); print(f"\n[{time.time()-t0:.3f}s]", file=sys.stderr); sys.exit(0)
    time.sleep(0.002)
print("TIMEOUT"); sys.exit(2)
