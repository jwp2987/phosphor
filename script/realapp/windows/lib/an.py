# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
import sys,statistics as st
def pc(a,q): a=sorted(a); return a[min(len(a)-1,int(round(q*(len(a)-1))))]
def summ(a): return "n=%d med=%.0f p95=%.0f min=%.0f max=%.0f"%(len(a),st.median(a),pc(a,.95),min(a),max(a)) if a else "n=0"
for f in sys.argv[1:]:
    keys={};echo={};starts={};stamps={}
    for l in open(f):
        p=l.strip().split(',')
        if '-stamps' in p[0]: stamps[p[0]]=[int(float(x)*1000) if '.' in x else int(x) for x in p[1].split(';') if x]; continue
        lab=p[0]
        try:
            if 'key' in lab and len(p)>=7:
                ts,tf=float(p[3]),float(p[4]); 
                if tf>0: keys.setdefault(lab,[]).append(tf-ts)
            elif 'echo-hi' in lab:
                tfs,tls,tff,tfl,n,mj=[float(x) for x in p[2:8]]
                echo.setdefault(lab,[]).append(tfl-tls)
            elif 'start-stamp' in lab:
                tfs,tls=[float(x) for x in p[2:4]]; starts.setdefault(lab,[]).append(tls)
        except Exception as e: pass
    print("##",f)
    for k,v in keys.items(): print(k,"key->first paint",summ(v))
    for k,v in echo.items(): print(k,"Enter->last paint",summ(v))
    for k,v in starts.items():
        s=stamps.get(k.replace('-stamp','-stamps'),[])
        d=[a-b for a,b in zip(s,v)]
        print(k,"Enter->command start",summ(d),[round(x) for x in d])
