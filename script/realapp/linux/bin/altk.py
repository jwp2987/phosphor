# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
import sys,time,json
from Xlib import X, display, XK
from Xlib.ext import xtest
d=display.Display(); root=d.screen().root
def kc(n): return d.keysym_to_keycode(XK.string_to_keysym(n))
def snap(x,y,w,h): return root.get_image(x,y,w,h,X.ZPixmap,0xffffffff).data
reg=json.loads(sys.argv[1]); N=int(sys.argv[2]); res=[]
for i in range(N):
    time.sleep(0.25); base=snap(*reg)
    t0=time.perf_counter(); xtest.fake_input(d,X.KeyPress,kc('space')); xtest.fake_input(d,X.KeyRelease,kc('space')); d.flush()
    t1=None
    while time.perf_counter()<t0+1:
        if snap(*reg)!=base: t1=time.perf_counter(); break
    res.append((t1-t0)*1000 if t1 else None)
print(json.dumps(res))
