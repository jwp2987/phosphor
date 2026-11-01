# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
import sys,collections
for f in sys.argv[1:]:
    d=collections.defaultdict(list)
    for l in open(f):
        p=l.strip().split(',')
        if len(p)>=7 and float(p[4])>0: d[p[0]].append(float(p[4])-float(p[3]))
    for k,v in d.items():
        c=collections.Counter(int(round(x/16.67)) for x in v)
        s=sorted(v)
        print("%-18s n=%d med=%.0f mean=%.1f  vsync-steps %s"%(k,len(v),s[len(s)//2],sum(v)/len(v),dict(sorted(c.items()))))
