# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
import os; RUN=os.environ['PHOSPHOR_LINUX_RUN']
import sqlite3,shutil,sys,glob,re
src=RUN+'/home/.local/state/phosphor/warp.sqlite'
for f in glob.glob(src+'*'): shutil.copy(f,'/tmp/phosphor-realapp/cp_'+f.split('/')[-1])
c=sqlite3.connect('/tmp/phosphor-realapp/cp_warp.sqlite')
tag=sys.argv[1]
rows=list(c.execute("select id,command from commands where command like ? order by id",('%rp_'+tag+'.txt%',)))
rows+= [r for r in c.execute("select id,command from commands where id>(select max(id) from commands where command like ?) order by id",('%rp_'+tag+'.txt%',))][:0]
ok=0
for i,cmd in rows:
    m=re.fullmatch(r'echo (P\w+?q*)xzy \| md5sum >> rp_%s\.txt'%tag,cmd)
    if m: ok+=1
    else: print('BAD',i,len(cmd),repr(cmd[:25]),'...',repr(cmd[-45:]))
print(tag,'cmds',len(rows),'exact',ok)
