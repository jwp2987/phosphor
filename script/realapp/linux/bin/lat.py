#!/usr/bin/env python3
# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
# lat.py WHICH MODE N   WHICH=phos|xterm  MODE=key|enter
import sys, time, os, subprocess, json
from Xlib import X, display, XK
from Xlib.ext import xtest
d = display.Display(); root = d.screen().root
def kc(ch): return d.keysym_to_keycode(XK.string_to_keysym(ch))
def fake(typ, code): xtest.fake_input(d, typ, code); 
def key(code):
    fake(X.KeyPress, code); fake(X.KeyRelease, code); d.flush()
def snap(x,y,w,h): return root.get_image(x,y,w,h,X.ZPixmap,0xffffffff).data
which, mode, N = sys.argv[1], sys.argv[2], int(sys.argv[3])
cfg = {'phos': dict(x0=266, y=915, h=22, cw=7.83, wide=(266,880,700,60)),
       'xterm': dict(x0=1010+2+14, y=2, h=16, cw=None, wide=(1010,0,600,40))}[which]
if len(sys.argv)>4: cfg.update(json.loads(sys.argv[4]))
RET = kc('Return'); 
res=[]
def clear():
    # ctrl+u
    cu = d.keysym_to_keycode(XK.string_to_keysym('Control_L')); u = kc('u')
    fake(X.KeyPress,cu); key(u); fake(X.KeyRelease,cu); d.flush()
if mode == 'key':
    cw = cfg['cw']; 
    for i in range(N):
        pos = i % 12
        x = int(cfg['x0'] + pos*cw) + 3; w = max(2,int(cw)-4)
        time.sleep(0.25)
        base = snap(x,cfg['y'],w,cfg['h'])
        base2 = snap(x,cfg['y'],w,cfg['h'])
        t0=time.perf_counter(); key(kc('m'))
        dl = t0+1.0; t1=None
        while time.perf_counter()<dl:
            if snap(x,cfg['y'],w,cfg['h'])!=base: t1=time.perf_counter(); break
        res.append((t1-t0)*1000 if t1 else None)
        if pos==11: clear(); time.sleep(.3)
    clear()
else:
    # enter->command start & first paint
    wx,wy,ww,wh = cfg['wide']
    for i in range(N):
        path=f"/tmp/phosphor-realapp/lat_{which}_{i}"
        try: os.remove(path)
        except: pass
        if which=='xterm': 
            subprocess.run(['xdotool','type','--delay','10','clear']); key(RET); time.sleep(.4)
        cmd = f"date +%s.%N>{path}"
        subprocess.run(['xdotool','type','--delay','12',cmd])
        time.sleep(0.3)
        base = snap(wx,wy,ww,wh)
        t0=time.time(); p0=time.perf_counter(); key(RET)
        t1=None
        while time.perf_counter()<p0+1.0:
            if snap(wx,wy,ww,wh)!=base: t1=time.perf_counter(); break
        tc=None
        for _ in range(200):
            if os.path.exists(path):
                try: tc=float(open(path).read().strip())
                except: tc=None
                if tc: break
            time.sleep(0.002)
        res.append(((t1-p0)*1000 if t1 else None, ((tc-t0)*1000) if tc else None))
        time.sleep(.5)
print(json.dumps(res))
