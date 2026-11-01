# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
import sys,time,os,subprocess,json
from Xlib import X, display, XK
from Xlib.ext import xtest
d=display.Display()
which=sys.argv[1]; N=int(sys.argv[2])
def kc(n): return d.keysym_to_keycode(XK.string_to_keysym(n))
res=[]
for i in range(N):
    try: os.remove('/tmp/phosphor-realapp/sigint_ts')
    except: pass
    subprocess.run(['xdotool','type','--delay','10','bash /tmp/phosphor-realapp/flood.sh']); subprocess.run(['xdotool','key','Return'])
    time.sleep(2.5)
    t0=time.time()
    xtest.fake_input(d,X.KeyPress,kc('Control_L')); xtest.fake_input(d,X.KeyPress,kc('c')); xtest.fake_input(d,X.KeyRelease,kc('c')); xtest.fake_input(d,X.KeyRelease,kc('Control_L')); d.flush()
    for _ in range(3000):
        if os.path.exists('/tmp/phosphor-realapp/sigint_ts'):
            try: ts=float(open('/tmp/phosphor-realapp/sigint_ts').read()); res.append((ts-t0)*1000); break
            except: pass
        time.sleep(0.001)
    else: res.append(None)
    time.sleep(1.5)
print(json.dumps(res))
