#!/usr/bin/env python3
# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
# cmp.py CASE ROUNDS BLOCK   (apps interleaved block by block; windows stacked at same rect)
import subprocess,sys,time,json,os
R=os.path.expanduser("~/phosphor-profile/warpcmp"); ENV=dict(os.environ,PATH="/opt/homebrew/bin:"+os.environ["PATH"])
APPS={"warp":(open(R+"/ctl/warp.pid").read().strip(),"stable"),
      "phos":(open(R+"/ctl/phos.pid").read().strip(),"phosphor-oss"),
      "term":(open(R+"/ctl/term.pid").read().strip(),"Terminal")}
case,rounds,block=sys.argv[1],int(sys.argv[2]),int(sys.argv[3])
order=sys.argv[4].split(",") if len(sys.argv)>4 else ["warp","phos","term"]
RECT="100 60 996 694"
def osa(s): return subprocess.run(["osascript","-e",s],capture_output=True,text=True).stdout.strip()
def front(): return osa('tell application "System Events" to unix id of first process whose frontmost is true')
def activate(app):
    pid=APPS[app][0]
    for _ in range(3):
        if front()==pid: return True
        osa('tell application "System Events" to set frontmost of (first process whose unix id is %s) to true'%pid); time.sleep(0.7)
    return front()==pid
def guard(app): return front()==APPS[app][0]
os.environ["KSETTLE"]="1500"
kl=subprocess.Popen([R+"/kl3"]+RECT.split(),stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True,bufsize=1,env=dict(ENV,KSETTLE="1500"))
time.sleep(3)
def kl_cmd(s):
    kl.stdin.write(s+"\n"); kl.stdin.flush(); return kl.stdout.readline().strip()
def nlines(p):
    try: return sum(1 for _ in open(p,errors="replace"))
    except: return 0
def tail(p,n): return open(p,errors="replace").read().splitlines()[n:]
out=open("%s/raw/%s.jsonl"%(R,case),"a")
def clear(): subprocess.run(["cliclick","kd:ctrl","t:u","ku:ctrl"],env=ENV); time.sleep(0.4)
for r in range(rounds):
  for app in order:
    if not activate(app): print("GUARD FAIL activate",app,flush=True); sys.exit(2)
    time.sleep(1.0)
    log=R+"/ctl/log."+app
    for b in range(block):
        if not guard(app): print("GUARD FAIL",app,front(),flush=True); sys.exit(2)
        clear(); c0=nlines(log)
        rec={"app":app,"round":r,"b":b,"case":case}
        if case=="key":
            rec["a"]=kl_cmd("K 300 a"); rec["del"]=kl_cmd("K 300 DEL")
        elif case=="pal":
            rec["m"]=kl_cmd("M 500 p"); time.sleep(0.5); subprocess.run(["screencapture","-x",R+"/shots/pal_%s_%d_%d.png"%(app,r,b)]) if r==0 and b==0 else None; rec["esc"]=kl_cmd("K 400 ESC")
        elif case=="F":
            rec["resp"]=kl_cmd("F 700 echo hello")
        elif case=="P":
            kl_cmd("W echo hello"); time.sleep(1.5); rec["resp"]=kl_cmd("R 700")
        elif case=="ctl":
            rec["resp"]=kl_cmd("C 1000")
        time.sleep(0.9)
        rec["ctl"]=tail(log,c0)
        out.write(json.dumps(rec)+"\n"); out.flush()
        if case in("F","P"): time.sleep(0.4)
print("DONE",case,flush=True)
kl.stdin.write("Q\n")
