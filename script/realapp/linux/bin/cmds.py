# realapp/linux (round-18 harness r18bi): helper for the Linux real-app run.
# Platform: Linux X11 (Xvfb + xdotool + xsel), private dbus session, scratch HOME.
# Prereqs: source ../config.example.sh values, ../env.sh; extracted release .deb; python3, xdotool, xwininfo.
# Run: see ../h.sh (source it) and ../f.sh; most helpers take positional args documented in their first comment.
# Restores: touches only $PHOSPHOR_LINUX_RUN and /tmp/phosphor-realapp; kills only exact PIDs it started.
import os; RUN=os.environ['PHOSPHOR_LINUX_RUN']
import sqlite3,shutil,sys,glob
src=RUN+'/home/.local/state/phosphor/warp.sqlite'
for f in glob.glob(src+'*'): shutil.copy(f,'/tmp/phosphor-realapp/cp_'+f.split('/')[-1])
c=sqlite3.connect('/tmp/phosphor-realapp/cp_warp.sqlite')
n=int(sys.argv[1]) if len(sys.argv)>1 else 5
for r in c.execute("select id,length(command),substr(command,1,50),substr(command,-70) from commands order by id desc limit ?",(n,)): print(r)
