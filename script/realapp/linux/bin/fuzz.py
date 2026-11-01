#!/usr/bin/env python3
# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
# fuzz.py TAG N DELAY SEED : expert typist fuzzer; verifies exact command via file
import os; RUN=os.environ['PHOSPHOR_LINUX_RUN']
import sys,random,subprocess,time,os
tag,N,dl,seed=sys.argv[1],int(sys.argv[2]),int(sys.argv[3]),int(sys.argv[4])
rnd=random.Random(seed); R=RUN+'/out'
out=f'{R}/fz_{tag}.txt'; open(out,'w').close()
L='abcdefghijklmnopqrstuvwxyz0123456789'
def kn(c): return {' ':'space','>':'greater','_':'underscore','-':'minus','.':'period'}.get(c,c)
exp=[]
for n in range(N):
    words=[''.join(rnd.choice(L) for _ in range(rnd.randint(3,9))) for _ in range(rnd.randint(3,6))]
    tgt='echo '+' '.join(words)+f' >> fz_{tag}.txt'
    exp.append(' '.join(words))
    keys=[]
    i=0
    body=list('echo '+' '.join(words)+' ')
    # typed prefix up to redirect, with edits
    seq=[]
    for c in body:
        r=rnd.random()
        if r<0.08: seq+= [rnd.choice(L),'BackSpace']
        elif r<0.11: seq+= [rnd.choice(L),rnd.choice(L),'BackSpace','BackSpace']
        seq.append(c)
    # a missing-char fix: omit a char then go back with Left and insert
    if rnd.random()<0.5 and len(words[-1])>3:
        w=words[-1]; k=rnd.randrange(1,len(w)-1); ch=w[k]
        # retype tail without ch: we emit: type up to end w/o ch, then Left*(len(w)-k-1), ch, then End
        pre='echo '+' '.join(words[:-1])+' '+w[:k]+w[k+1:]
        seq=[]
        for c in pre: seq.append(c)
        seq+=['Left']*(len(w)-k-1)+[ch,'End']
        seq+=[' ']
    seq+=['>','>',' ']+list(f'fz_{tag}.txt')
    # a wrong word then ctrl+w at the very end (then retype last token)
    if rnd.random()<0.3:
        seq+=[' ','x','y','z','ctrl+w']
        seq+=['BackSpace']*0
    ks=[kn(c) for c in seq]
    # remove trailing space artifact from ctrl+w: ctrl+w deletes "xyz" leaving trailing space; fine for shell
    subprocess.run(['xdotool','key','--delay',str(dl)]+ks+['Return'])
    time.sleep(rnd.choice([0.0,0.0,0.05,0.3]))
time.sleep(3)
got=[l.rstrip('\n') for l in open(out)]
bad=[(i,a,b) for i,(a,b) in enumerate(zip(exp,got)) if a!=b]
print(f'fuzz {tag} dl={dl} seed={seed}: got {len(got)}/{N} lines, mismatches {len(bad)}')
for b in bad[:5]: print('  exp',b[1],'| got',b[2])
