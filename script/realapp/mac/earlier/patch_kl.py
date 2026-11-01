# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
import sys
s=open(sys.argv[1]).read()
s=s.replace("var shiftNext = false","var shiftNext = false\nvar cmdNext = false",1)
s=s.replace("e.flags = shiftNext ? .maskShift : CGEventFlags(rawValue: 0)","e.flags = cmdNext ? .maskCommand : (shiftNext ? .maskShift : CGEventFlags(rawValue: 0))",1)
s=s.replace('  case "Q": exit(0)','  case "M": cmdNext = true; let kk: CGKeyCode = p[2]=="ESC" ? 53 : (kc[Array(p[2])[0]] ?? 0); print("M " + timed(Double(p[1])!, kk, 1500)); cmdNext = false\n  case "Q": exit(0)',1)
open(sys.argv[2],'w').write(s)
