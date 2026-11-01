# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
import os,sys,time
secs=float(sys.argv[1]); seen={}
t0=time.time()
while time.time()-t0<secs:
    for p in os.listdir('/proc'):
        if p.isdigit() and p not in seen:
            try:
                c=open(f'/proc/{p}/comm').read().strip()
                if c=='git':
                    seen[p]="%.2f "%(time.time()-t0)+open(f'/proc/{p}/cmdline').read().replace('\0',' ')[-150:]
            except Exception: pass
    time.sleep(0.002)
print(len(seen)); [print(v) for v in seen.values()]
