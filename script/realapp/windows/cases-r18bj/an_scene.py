# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
import re,sys,collections,statistics as st
def load(f,t0,t1):
    fr=[];fc=[]
    for l in open(f,errors='replace'):
        m=re.match(r'\d{4}-\d\d-\d\dT(\d\d:\d\d:\d\d)Z',l)
        if not m or not (t0<=m.group(1)<=t1): continue
        if 'fingerprint=' in l:
            fp=re.search(r'fingerprint=(\w+)',l).group(1)
            g=lambda k: float(re.search(k+r': ([\d.]+)',l).group(1))
            fr.append((m.group(1),fp,g('rect_area_px'),g('glyphs'),g('rects')))
        elif 'updated=' in l:
            fc.append((m.group(1),re.search(r'updated=(\[.*?\]) removed=\d+ redraw=(\w+) paint_only=(\w+)',l).groups()))
    return fr,fc
for f,t0,t1 in [('LA_agent_app.log','19:46:30','19:46:48'),('LA_term_app.log','19:49:48','19:50:06')]:
    fr,fc=load(f,t0,t1)
    print('==',f,'idle window frames drawn:',len(fr),'(%.1f/s)'%(len(fr)/18))
    if fr:
        a=[x[2] for x in fr]
        print(' rect_area_px med=%.0f min=%.0f max=%.0f ; glyphs med=%d ; rects med=%d'%(st.median(a),min(a),max(a),st.median([x[3] for x in fr]),st.median([x[4] for x in fr])))
        fps=collections.Counter(x[1] for x in fr); print(' distinct fingerprints',len(fps),'top',fps.most_common(3))
        same=sum(1 for i in range(1,len(fr)) if fr[i][1]==fr[i-1][1]); print(' consecutive frames with IDENTICAL fingerprint (repaint of same content):',same,'of',len(fr)-1)
    cs=collections.Counter((x[1][0][:60],x[1][1],x[1][2]) for x in fc if x[1][0]!='[]' or x[1][1]=='true' or x[1][2]=='true')
    print(' frame causes:',cs.most_common(6))
