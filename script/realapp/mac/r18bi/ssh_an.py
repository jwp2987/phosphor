# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
import os
import sys,re,subprocess
log=subprocess.check_output(["ssh",os.environ["PHOSPHOR_MAC_SSH"],"ssh "+os.environ["PHOSPHOR_BOX_SSH"]+" cat /tmp/zq-mac.log"]).decode().split("\n")
from collections import Counter
c=Counter(log)
import itertools
for L,n in [("A3",30),("A0",10),("A1",10),("A2",10),("T3",30),("T0",10),("T1",10),("T2",10)]:
    ok=0;bad=[]
    for i in range(1,n+1):
        idn="zq%s%02d"%(L,i)
        if c.get(idn,0)==(2 if (L=="A0" and i<=2) else 1): ok+=1
        else: bad.append((idn,c.get(idn,0)))
    print(L,ok,"/",n,bad)
junk=[l for l in c if l and not re.fullmatch(r"zq[AT][0-3]\d\d",l)]
print("malformed:",junk)
