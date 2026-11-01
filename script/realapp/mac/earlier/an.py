# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
import sys,json,re,statistics as st,collections
def pct(a,p):
    a=sorted(a); return a[min(len(a)-1,int(round((len(a)-1)*p)))]
def f(a): return "med %.1f / p95 %.1f / max %.1f (n=%d)"%(st.median(a),pct(a,.95),max(a),len(a)) if a else "-"
def first(resp,key="post0"):
    p0=float(re.search(key+r'=([\d.]+)',resp).group(1))
    m=re.search(r'ch=(\S+)',resp)
    ch=[float(c.split(':')[0]) for c in m.group(1).split(',')] if m else []
    ch=[c for c in ch if c>=p0]
    return p0,(ch[0]-p0 if ch else None)
D=collections.defaultdict(lambda:collections.defaultdict(list)); miss=collections.Counter(); ex=collections.Counter(); tot=collections.Counter()
for case in sys.argv[1:]:
    for l in open("raw/%s.jsonl"%case):
        r=json.loads(l); app=r["app"]
        if case=="key":
            for k in("a","del"):
                p0,d=first(r[k]); tot[(case,k,app)]+=1
                if d is None: miss[(case,k,app)]+=1
                else: D[(case,k)][app].append(d)
        else:
            p0,d=first(r["resp"]); tot[(case,"enter",app)]+=1
            if d is None: miss[(case,"enter",app)]+=1
            else: D[(case,"first-paint")][app].append(d)
            sts=[float(c.split()[1])*1000 for c in r["ctl"] if c.startswith("start ")]
            cm=[c.split(' ',2)[2] for c in r["ctl"] if c.startswith("start ")]
            if sts:
                D[(case,"preexec")][app].append(sts[0]-p0)
            tot[(case,"cmd",app)]+=1
            if cm and cm[0]=="echo hello": ex[(case,app)]+=1
for k,v in D.items():
    print(k)
    for app in("warp","phos","term"): print("   %-5s %s"%(app,f(v[app])))
print("nochange:",dict(miss)); print("exact cmd:",dict(ex)); print("tot",dict(tot))
