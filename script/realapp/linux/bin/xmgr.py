#!/usr/bin/env python3
# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
# minimal CLIPBOARD_MANAGER: on SAVE_TARGETS fetch CLIPBOARD text and re-own it. logs to stderr.
import sys, time
from Xlib import X, display, Xatom
from Xlib.protocol import event
d = display.Display(); s = d.screen()
w = s.root.create_window(0,0,1,1,0,s.root_depth)
A = d.intern_atom
CM, CLIP, SAVE, UTF8, TARGETS = A('CLIPBOARD_MANAGER'), A('CLIPBOARD'), A('SAVE_TARGETS'), A('UTF8_STRING'), A('TARGETS')
w.set_selection_owner(CM, X.CurrentTime); d.flush()
t0=time.time(); secs=float(sys.argv[1]) if len(sys.argv)>1 else 300
def log(m): print(f"{time.time()-t0:8.3f} {m}", file=sys.stderr, flush=True)
log("managing CLIPBOARD_MANAGER")
saved=None; pending=None
while time.time()-t0 < secs:
    while d.pending_events():
        e = d.next_event()
        if e.type == X.SelectionRequest and e.selection == CM and e.target == SAVE:
            log("SAVE_TARGETS request from window %#x" % e.requestor.id)
            pending = e
            w.convert_selection(CLIP, UTF8, A('MGR_PROP'), X.CurrentTime); d.flush()
        elif e.type == X.SelectionNotify and pending is not None:
            r = w.get_full_property(A('MGR_PROP'), X.AnyPropertyType) if e.property != X.NONE else None
            saved = bytes(r.value) if r else None
            log(f"fetched clipboard from exiting owner: {saved!r}")
            if saved is not None:
                w.set_selection_owner(CLIP, X.CurrentTime)
            n = event.SelectionNotify(time=pending.time, requestor=pending.requestor, selection=CM, target=SAVE, property=pending.property if saved is not None else X.NONE)
            pending.requestor.send_event(n); d.flush(); pending=None
        elif e.type == X.SelectionRequest and e.selection == CLIP:
            tgt = e.target; prop = e.property or tgt
            log(f"serving request target={d.get_atom_name(tgt)}")
            if tgt == TARGETS: e.requestor.change_property(prop, Xatom.ATOM, 32, [TARGETS, UTF8]); ok=True
            elif tgt == UTF8 and saved is not None: e.requestor.change_property(prop, UTF8, 8, saved); ok=True
            else: ok=False
            n = event.SelectionNotify(time=e.time, requestor=e.requestor, selection=e.selection, target=tgt, property=prop if ok else X.NONE)
            e.requestor.send_event(n); d.flush()
    time.sleep(0.002)
