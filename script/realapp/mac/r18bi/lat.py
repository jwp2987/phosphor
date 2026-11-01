#!/usr/bin/env python3
# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
# lat.py APP(phos|term) CASE N PID NAME  ; writes json lines to stdout
import subprocess,sys,time,json,os,statistics as st
app,case,N,pid,pname=sys.argv[1],sys.argv[2],int(sys.argv[3]),sys.argv[4],sys.argv[5]
R=os.path.expanduser("~/phosphor-profile/r18m/lat")
RECT=os.environ.get("RECT","367 170 1028 699").split()
def osa(s): return subprocess.run(["osascript","-e",s],capture_output=True,text=True).stdout.strip()
def front(): return osa('tell application "System Events" to unix id of first process whose frontmost is true')
def guard():
    if front()==pid: return True
    osa('tell application "System Events" to set frontmost of (first process whose unix id is %s) to true'%pid); time.sleep(0.7)
    return front()==pid
kl=subprocess.Popen([R+"/kl3"]+RECT,stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True,bufsize=1,env=dict(os.environ,KSETTLE=os.environ.get("KSETTLE","350")))
time.sleep(3)
def cmd(s): kl.stdin.write(s+"\n"); kl.stdin.flush(); return kl.stdout.readline().strip()
def parse(l):
    d={}
    for tok in l.split():
        if "=" in tok:
            k,v=tok.split("=",1); d[k]=v
    ch=[(float(a.split(":")[0]),int(a.split(":")[1])) for a in d.get("ch","").split(",") if a]
    return float(d["post0"]),float(d["post1"]),ch
def lat(l):
    p0,p1,ch=parse(l)
    ch=[c for c in ch if c[0]>=p1-2]
    return (ch[0][0]-p0, (ch[-1][0]-p0), len(ch)) if ch else (None,None,0)
res=[]
for i in range(N):
    if not guard(): print("GUARD FAIL",front(),flush=True); sys.exit(2)
    if case=="key":
        a=lat(cmd("K 300 a")); d=lat(cmd("K 300 DEL")); res.append(("a",a)); res.append(("del",d))
    elif case=="enter":
        cmd("W echo hello"); time.sleep(1.2); r=lat(cmd("R 700")); res.append(("enter",r)); time.sleep(0.5)
    elif case=="scroll":
        r=lat(cmd("F 2500 seq 1 4000")); res.append(("scroll",r)); time.sleep(3)
        subprocess.run(["cliclick","kd:ctrl","t:l","ku:ctrl"]); time.sleep(0.6)
    elif case=="burst":
        l=cmd("B 20 25,a,DEL")
        posts=[float(x) for x in l.split("posts=")[1].split()[0].split(",")]
        ch=[(float(a.split(":")[0])) for a in l.split("ch=")[1].split(",") if a]
        for p in posts:
            nxt=[c for c in ch if c>=p]
            res.append(("burst",((nxt[0]-p) if nxt else None,None,0)))
        time.sleep(0.8)
    time.sleep(0.3)
kl.stdin.write("Q\n")
byk={}
for k,(f,l,n) in res:
    if f is not None: byk.setdefault(k,[]).append((f,l))
for k,v in byk.items():
    f=sorted(x[0] for x in v)
    print(json.dumps({"app":app,"case":case,"k":k,"n":len(f),"first_median":round(st.median(f),1),"p95":round(f[int(len(f)*0.95)-1],1) if len(f)>1 else f[0],"min":round(f[0],1),"max":round(f[-1],1),"last_median":(round(st.median([x[1] for x in v if x[1] is not None]),1) if any(x[1] is not None for x in v) else None)}),flush=True)
print("DONE",app,case,flush=True)
