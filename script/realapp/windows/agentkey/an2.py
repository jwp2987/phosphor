# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
import re,sys,statistics as st,collections
log=sys.argv[1]; csv=sys.argv[2]
fr=[]
for l in open(log,errors='replace'):
    m=re.search(r'hop=frame_drawn total_ms=([\d.]+) acquire_ms=([\d.]+) t=([\d.]+)',l)
    if m: fr.append((float(m.group(3)),float(m.group(1)),float(m.group(2))))
keys=collections.defaultdict(list)
for l in open(csv):
    p=l.strip().split(',')
    keys[p[0]].append((float(p[3]),float(p[4])))
for lab,ks in keys.items():
    print("==",lab)
    nf=[];first=[];tot=[];acq=[];paint=[]
    for ts,tf in ks:
        f=[x for x in fr if ts-5<=x[0]<=ts+120]
        nf.append(len(f))
        if f:
            first.append(f[0][0]-ts); tot.append(sum(x[1] for x in f)); acq.append(max(x[2] for x in f))
            paint.append(tf-ts)
    def s(a): return "med=%.1f p90=%.1f max=%.1f"%(st.median(a),sorted(a)[int(len(a)*.9)],max(a)) if a else "-"
    print(" frames/key",collections.Counter(nf))
    print(" key->first frame_drawn(log)",s(first))
    print(" sum frame total_ms",s(tot)," max acquire_ms",s(acq))
    print(" harness key->paint",s(paint))
