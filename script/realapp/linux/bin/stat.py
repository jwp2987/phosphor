# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
import os; RUN=os.environ['PHOSPHOR_LINUX_RUN']
import json,statistics as s,sys
for f in sys.argv[1:]:
    r=json.load(open(f'{RUN}/raw/lat_{f}.json'))
    if r and isinstance(r[0],list):
        for j,name in ((0,'Enter->first paint'),(1,'Enter->cmd start')):
            a=sorted(x[j] for x in r if x[j] is not None)
            print(f,name,len(a),'/',len(r),'med %.2f p95 %.2f max %.2f'%(s.median(a),a[max(0,int(.95*len(a))-1)],a[-1]) if a else 'none')
    else:
        a=sorted(x for x in r if x is not None); print(f,len(a),'/',len(r),'med %.2f p95 %.2f max %.2f'%(s.median(a),a[max(0,int(.95*len(a))-1)],a[-1]))
