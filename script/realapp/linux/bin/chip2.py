# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
import sys,time
from Xlib import X, display, XK
from Xlib.ext import xtest
d=display.Display(); root=d.screen().root
def snap(x,y,w,h): return root.get_image(x,y,w,h,X.ZPixmap,0xffffffff).data
kc=d.keysym_to_keycode(XK.string_to_keysym('Return'))
reg=(266,878,560,26); cur=snap(*reg); ev=[]
t0=time.perf_counter(); xtest.fake_input(d,X.KeyPress,kc); xtest.fake_input(d,X.KeyRelease,kc); d.flush()
while time.perf_counter()<t0+5:
    s=snap(*reg)
    if s!=cur: ev.append(round((time.perf_counter()-t0)*1000)); cur=s
    time.sleep(0.005)
print('chip region changes at ms:', ev)
