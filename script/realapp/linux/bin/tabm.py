# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
import sys,time,subprocess,json
from Xlib import X, display, XK
from Xlib.ext import xtest
d=display.Display(); root=d.screen().root
def kc(n): return d.keysym_to_keycode(XK.string_to_keysym(n))
def snap(x,y,w,h): return root.get_image(x,y,w,h,X.ZPixmap,0xffffffff).data
def key(n, mods=()):
    for m in mods: xtest.fake_input(d,X.KeyPress,kc(m))
    xtest.fake_input(d,X.KeyPress,kc(n)); xtest.fake_input(d,X.KeyRelease,kc(n))
    for m in reversed(mods): xtest.fake_input(d,X.KeyRelease,kc(m))
    d.flush()
mode=sys.argv[1]; N=int(sys.argv[2]); text=sys.argv[3] if len(sys.argv)>3 else ''
reg=(260,500,730,400) if mode=='tab' else (180,100,640,360)
res=[]
for i in range(N):
    if mode=='tab':
        subprocess.run(['xdotool','type','--delay','12',text]); time.sleep(0.6)
    base=snap(*reg); time.sleep(0.05); base=snap(*reg)
    t0=time.perf_counter()
    if mode=='tab': key('Tab')
    else: key('p',('Control_L','Shift_L'))
    t1=None
    while time.perf_counter()<t0+3:
        if snap(*reg)!=base: t1=time.perf_counter(); break
    res.append((t1-t0)*1000 if t1 else None)
    time.sleep(0.5)
    key('Escape'); time.sleep(0.3)
    if mode=='tab': key('u',('Control_L',)); time.sleep(0.4)
print(json.dumps(res))
