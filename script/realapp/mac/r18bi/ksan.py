# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
import os
import sys,subprocess,re
L=sys.argv[1]; n=int(sys.argv[2])
log=subprocess.check_output(["ssh",os.environ["PHOSPHOR_MAC_SSH"],"ssh "+os.environ["PHOSPHOR_BOX_SSH"]+" cat /tmp/zq-mac.log"]).decode().split("\n")
from collections import Counter
c=Counter(log)
bad=[i for i in range(1,n+1) if c.get("zq%s%02d"%(L,i),0)!=1]
print(L,"exact",n-len(bad),"/",n,"missing",bad)
