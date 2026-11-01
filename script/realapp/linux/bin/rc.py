# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
import sys,time,json
from Xlib import X, display
from Xlib.ext import xtest
d=display.Display(); root=d.screen().root
def snap(x,y,w,h): return root.get_image(x,y,w,h,X.ZPixmap,0xffffffff).data
res=[]
for i in range(int(sys.argv[1])):
    root.warp_pointer(0,0) if False else None
    xtest.fake_input(d,X.MotionNotify,0,x=700,y=900); d.flush(); time.sleep(0.4)
    base=snap(500,300,500,690)
    t0=time.perf_counter()
    xtest.fake_input(d,X.ButtonPress,3); xtest.fake_input(d,X.ButtonRelease,3); d.flush()
    t1=None
    while time.perf_counter()<t0+2:
        if snap(500,300,500,690)!=base: t1=time.perf_counter(); break
    res.append((t1-t0)*1000 if t1 else None)
    time.sleep(0.3)
    from Xlib import XK
    k=d.keysym_to_keycode(XK.string_to_keysym('Escape')); xtest.fake_input(d,X.KeyPress,k); xtest.fake_input(d,X.KeyRelease,k); d.flush(); time.sleep(0.4)
print(json.dumps(res))
