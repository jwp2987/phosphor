#!/usr/bin/env python3
# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
# usage: xowner.py MODE TEXT [secs]   MODE=normal|stall ; owns CLIPBOARD; logs requests to stderr
import sys, time, threading
from Xlib import X, display, Xatom
from Xlib.protocol import event
mode, text = sys.argv[1], sys.argv[2]
if text.startswith('@'): text = open(text[1:]).read()
secs = float(sys.argv[3]) if len(sys.argv) > 3 else 60
d = display.Display()
s = d.screen()
w = s.root.create_window(0,0,1,1,0,s.root_depth)
A = lambda n: d.intern_atom(n)
import os
CLIP, TARGETS, UTF8, HTML, PLAIN, STR = A(os.environ.get('SEL','CLIPBOARD')), A('TARGETS'), A('UTF8_STRING'), A('text/html'), A('text/plain'), A('STRING')
w.set_selection_owner(CLIP, X.CurrentTime); d.flush()
t0 = time.time()
def log(m): print(f"{time.time()-t0:8.3f} {m}", file=sys.stderr, flush=True)
log(f"owning CLIPBOARD mode={mode} text={text!r}")
html = ('<b>'+text+'</b>').encode()
PNG = A('image/png')
png_bytes = open(text,'rb').read() if mode=='png' else b''
MODE=mode
def reply(e, prop, typ, fmt, data):
    e.requestor.change_property(prop, typ, fmt, data)
    n = event.SelectionNotify(time=e.time, requestor=e.requestor, selection=e.selection, target=e.target, property=prop)
    e.requestor.send_event(n); d.flush()
def refuse(e):
    n = event.SelectionNotify(time=e.time, requestor=e.requestor, selection=e.selection, target=e.target, property=X.NONE)
    e.requestor.send_event(n); d.flush()
def handle(e):
    tgt = d.get_atom_name(e.target)
    prop = e.property or e.target
    log(f"request target={tgt}")
    if e.target == TARGETS:
        reply(e, prop, Xatom.ATOM, 32, [TARGETS, PNG] if MODE=='png' else ([TARGETS, HTML, UTF8, PLAIN, STR]))
    elif e.target == PNG and MODE=='png':
        reply(e, prop, PNG, 8, png_bytes)
    elif MODE=='blackhole':
        log('blackhole: no reply'); return
    elif e.target == HTML:
        if mode == 'stall':
            log("html: STALLING (no reply)"); return
        reply(e, prop, HTML, 8, html)
    elif e.target in (UTF8, PLAIN):
        reply(e, prop, e.target, 8, text.encode())
    elif e.target == STR:
        reply(e, prop, STR, 8, text.encode())
    else:
        refuse(e)
end = t0 + secs
while time.time() < end:
    while d.pending_events():
        e = d.next_event()
        if e.type == X.SelectionRequest:
            threading.Thread(target=handle, args=(e,), daemon=True).start()
        elif e.type == X.SelectionClear:
            log("SelectionClear (lost ownership)"); sys.exit(0)
    time.sleep(0.002)
