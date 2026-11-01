#!/usr/bin/env python3
# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
# rollover.py TAG N OVERLAP_MS : each next keydown arrives before previous keyup
import os; RUN=os.environ['PHOSPHOR_LINUX_RUN']
import sys,random,subprocess,time
tag,N,ov=sys.argv[1],int(sys.argv[2]),float(sys.argv[3])
rnd=random.Random(7); R=RUN+'/out'
import shutil,os; shutil.rmtree(R+'/rof',ignore_errors=True); os.makedirs(R+'/rof')
L='abcdefghijklmnopqrstuvwxyz0123456789'
kn=lambda c:{' ':'space','>':'greater','_':'underscore','.':'period','/':'slash'}.get(c,c)
exp=[]
for n in range(N):
    w=''
    while len(w)<10:
        c=rnd.choice(L)
        if not w or w[-1]!=c: w+=c
    exp.append(w)
    s='touch rof/'+w
    cmd=['xdotool']; prev=None
    for c in s:
        k=kn(c)
        cmd+=['keydown',k,'sleep',str(ov/1000)]
        if prev: cmd+=['keyup',prev]
        cmd+=['sleep',str(ov/1000)]
        prev=k
    cmd+=['keyup',prev,'key','Return']
    subprocess.run(cmd)
    time.sleep(0.25)
time.sleep(3)
import os
have=set(os.listdir(R+'/rof'))
miss=[w for w in exp if w not in have]
extra=[h for h in have if h not in exp]
print(f'rollover {tag} overlap={ov}ms: created {len(exp)-len(miss)}/{N}; missing {miss[:3]} extra {extra[:3]}')
