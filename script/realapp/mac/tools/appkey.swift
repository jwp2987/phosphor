// appkey: the ONLY way the macOS app-test sends Cmd-W, Cmd-Shift-W and Cmd-Q (#2061, r14-test-gaps T32). htype2 never carries them: appci/backend.py
// MacBackend refuses those chords for every other path (docs/testing.md 3.3: sent to the wrong app they close its window or quit it).
// Source of truth is this file; compiled by the job like htype2 (script/realapp/mac/ci/app_test.sh). NEVER commit binaries.
//
//   appkey PID BUNDLE CODE MODS VIA OUTFILE
//     PID     the app under test (the PID the harness started)
//     BUNDLE  the job's private copy of the app (<out>/Phosphor.app): PID's executable must live inside it
//     CODE    virtual key code: 12 (Q) or 13 (W), nothing else
//     MODS    8 (Cmd) for either; 9 (Cmd+Shift) for W only
//     VIA     pid = CGEvent.postToPid(PID): the event is put on that one process's queue, so no other app can receive it whatever happens to the
//                   focus between the check and the post (the default)
//             hid = the HID event tap, as htype2 posts every other key (APPCI_MAC_CHORD_VIA=hid; the frontmost check below is then the only fence)
// Refused with exit 4, nothing posted, unless ALL hold: the chord is in the list above; PID is running and its executable is inside BUNDLE; PID is
// the frontmost application right now (asked here, in this process, immediately before the post).
// OUTFILE has htype2's lines ("T0 mach_ms= epoch=", "K KEY<code> 1 <mach_ms>"), so MacBackend._record_call accounts for the key like any k: op.
import Foundation
import AppKit
import CoreGraphics

func die(_ code: Int32, _ s: String) -> Never { FileHandle.standardError.write((s + "\n").data(using: .utf8)!); exit(code) }
let a = CommandLine.arguments
guard a.count == 7, let pid = Int32(a[1]), pid > 1, let code = UInt16(a[3]), let mods = Int(a[4]) else { die(2, "usage: appkey PID BUNDLE CODE MODS pid|hid OUTFILE") }
let bundle = a[2].hasSuffix("/") ? String(a[2].dropLast()) : a[2]
let via = a[5], outPath = a[6]
guard (code == 12 && mods == 8) || (code == 13 && (mods == 8 || mods == 9)) else { die(4, "refused: appkey sends only Cmd-Q, Cmd-W and Cmd-Shift-W (got code \(code) mods \(mods))") }
guard via == "pid" || via == "hid" else { die(2, "VIA must be pid or hid") }
guard bundle.hasSuffix(".app"), bundle.hasPrefix("/") else { die(4, "refused: BUNDLE must be the absolute path of the job's .app (got \(bundle))") }
guard let app = NSRunningApplication(processIdentifier: pid), !app.isTerminated else { die(4, "refused: pid \(pid) is not a running application") }
let exe = app.executableURL?.resolvingSymlinksInPath().path ?? ""
let want = URL(fileURLWithPath: bundle).resolvingSymlinksInPath().path + "/Contents/MacOS/"
guard exe.hasPrefix(want) else { die(4, "refused: pid \(pid) runs \(exe), which is not inside the job's bundle \(bundle)") }
let front = NSWorkspace.shared.frontmostApplication
guard front?.processIdentifier == pid else {
  let fname = front?.localizedName ?? "unknown", fpid = front?.processIdentifier ?? -1
  die(4, "refused: the frontmost application is \(fname) (pid \(fpid)), not the app under test (pid \(pid))")
}

var tb = mach_timebase_info_data_t(); mach_timebase_info(&tb)
func nowMs() -> Double { Double(mach_absolute_time()) * Double(tb.numer) / Double(tb.denom) / 1e6 }
var out = "T0 mach_ms=\(nowMs()) epoch=\(Date().timeIntervalSince1970)\nVIA \(via)\n"
let src = CGEventSource(stateID: .hidSystemState)
var fl = CGEventFlags.maskCommand
if mods & 1 != 0 { fl.insert(.maskShift) }
out += "K KEY\(code) 1 \(nowMs())\n"
for down in [true, false] {
  guard let e = CGEvent(keyboardEventSource: src, virtualKey: CGKeyCode(code), keyDown: down) else { die(1, "CGEvent could not be created") }
  e.flags = fl
  if via == "pid" { e.postToPid(pid) } else { e.post(tap: .cghidEventTap) }
  Thread.sleep(forTimeInterval: 0.012)
}
try? out.write(toFile: outPath, atomically: true, encoding: .utf8)
Thread.sleep(forTimeInterval: 0.06)        // as htype2 (#1482): the hand-off is asynchronous
exit(0)
