#!/usr/bin/env python3
# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
import os,time
P=os.path.expanduser("~/phosphor-profile/r18bi/lat/home/Library/Logs/phosphor.log")
O=open(os.path.expanduser("~/phosphor-profile/r18bi/lat/stamped.log"),"a",buffering=1)
f=None;ino=None;buf=b""
while True:
    try:
        st=os.stat(P)
        if f is None or st.st_ino!=ino:
            if f: f.close()
            f=open(P,"rb"); ino=st.st_ino
        d=f.read()
    except FileNotFoundError:
        time.sleep(0.05); continue
    if d:
        now=time.time()*1000
        buf+=d
        while b"\n" in buf:
            l,buf=buf.split(b"\n",1)
            O.write("ARR %.3f %s\n"%(now,l.decode("utf8","replace")))
    else: time.sleep(0.0005)
