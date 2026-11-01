#!/usr/bin/env python3
# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
# flood.py ROUNDS : per app interleaved: seq / yes|head / cat240 ; wall time from preexec/precmd; RSS tree before/after; kl2 frame capture
import subprocess,sys,time,json,os,re
R=os.path.expanduser("~/phosphor-profile/warpcmp"); ENV=dict(os.environ,PATH="/opt/homebrew/bin:"+os.environ["PATH"])
APPS={a:open(R+"/ctl/%s.pid"%a).read().strip() for a in("warp","phos","term")}
rounds=int(sys.argv[1]); mode=sys.argv[2] if len(sys.argv)>2 else "flood"
def osa(s): return subprocess.run(["osascript","-e",s],capture_output=True,text=True).stdout.strip()
def front(): return osa('tell application "System Events" to unix id of first process whose frontmost is true')
def activate(app):
    for _ in range(3):
        if front()==APPS[app]: return True
        osa('tell application "System Events" to set frontmost of (first process whose unix id is %s) to true'%APPS[app]); time.sleep(0.7)
    return front()==APPS[app]
def tree(root):
    ps=subprocess.run(["ps","-axo","pid=,ppid=,rss=,time="],capture_output=True,text=True).stdout.split("\n")
    rows={}
    for l in ps:
        p=l.split()
        if len(p)==4:
            t=p[3]; sec=0.0
            for x in t.split(":"): sec=sec*60+float(x)
            rows[p[0]]=(p[1],int(p[2]),sec)
    s={root}; ch=True
    while ch:
        ch=False
        for k,(pp,_,_) in rows.items():
            if pp in s and k not in s: s.add(k); ch=True
    return sum(rows[k][1] for k in s if k in rows)/1024.0, sum(rows[k][2] for k in s if k in rows)
RECT="100 60 996 694"
kl=subprocess.Popen([R+"/kl2"]+RECT.split(),stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True,bufsize=1,env=ENV)
time.sleep(3)
def kl_cmd(s): kl.stdin.write(s+"\n"); kl.stdin.flush(); return kl.stdout.readline().strip()
def tail(p,n): return open(p,errors="replace").read().splitlines()[n:]
def nl(p): return len(open(p,errors="replace").read().splitlines())
out=open(R+"/raw/%s.jsonl"%mode,"a")
CMDS={"yes":"yes | head -c 52428800","cat":"cat "+os.path.expanduser("~")+"/phosphor-profile/warpcmp/data/big240"}
if mode=="idle":
    for app in sys.argv[3].split(","):
        activate(app); time.sleep(3)
        r0,c0=tree(APPS[app]); time.sleep(60); r1,c1=tree(APPS[app])
        rec={"app":app,"idle_cpu_pct":(c1-c0)/60*100,"rss_mb":r1}; print(rec,flush=True); out.write(json.dumps(rec)+"\n"); out.flush()
    sys.exit()
for r in range(rounds):
  for name,cmd in CMDS.items():
    for app in ("warp","phos","term"):
        if not activate(app): print("GUARD FAIL",app); sys.exit(2)
        time.sleep(1.0)
        log=R+"/ctl/log."+app; c0=nl(log)
        subprocess.run(["cliclick","kd:ctrl","t:u","ku:ctrl"],env=ENV); time.sleep(0.4)
        rss0,_=tree(APPS[app])
        subprocess.run(["cliclick","-w","10","t:"+cmd],env=ENV); time.sleep(0.3)
        kl.stdin.write("C 400\n"); 
        # kl2 C settles first and blocks; so instead capture via a separate approach: just run command, capture afterwards
        kl.stdin.write("Q\n") if False else None
        # (C mode reads stdin sequentially; send Return via cliclick then wait)
        kl.stdin.flush()
        kl.stdout.readline()
        subprocess.run(["osascript","-e",'tell application "System Events" to key code 36'])
        t_end=time.time()+150; peak=rss0
        while time.time()<t_end:
            time.sleep(1.0)
            peak=max(peak,tree(APPS[app])[0])
            L=tail(log,c0)
            if any(x.startswith("end ") for x in L[1:]) or (len(L)>=2 and L[-1].startswith("end ")): break
        L=tail(log,c0)
        if not any(x.startswith("end ") for x in L[1:]): subprocess.run(["cliclick","kd:ctrl","t:c","ku:ctrl"],env=ENV); time.sleep(3); L=tail(log,c0); print("TIMEOUT-CTRLC",app,name,flush=True)
        st=[float(x.split()[1]) for x in L if x.startswith("start ")]; en=[float(x.split()[1]) for x in L if x.startswith("end ")]
        time.sleep(2); rss1,_=tree(APPS[app])
        rec={"app":app,"round":r,"cmd":name,"wall":(en[-1]-st[0]) if st and en and en[-1]>st[0] else None,"rss_before":rss0,"rss_peak":peak,"rss_after":rss1}
        print(rec,flush=True); out.write(json.dumps(rec)+"\n"); out.flush()
        subprocess.run(["cliclick","kd:ctrl","t:l","ku:ctrl"],env=ENV); time.sleep(1.5)
kl.stdin.write("Q\n")
