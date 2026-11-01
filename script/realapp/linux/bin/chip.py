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
reg=(266,878,560,26); base=snap(*reg)
t0=time.perf_counter(); xtest.fake_input(d,X.KeyPress,kc); xtest.fake_input(d,X.KeyRelease,kc); d.flush()
t1=None
while time.perf_counter()<t0+10:
    if snap(*reg)!=base: t1=time.perf_counter(); break
print('chip changed after ms:', (t1-t0)*1000 if t1 else None)
