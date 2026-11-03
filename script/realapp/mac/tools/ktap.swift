// ktap: pass-through observer of Return/Enter key events at the ANNOTATED session tap (#1482), where macOS has already chosen
// the target process. For every keyCode 36/76 keyDown/keyUp it logs one line and returns the event UNCHANGED: it never drops,
// delays or edits an event. No typed text is logged (key codes only, never characters).
// Line: E <down|up> keycode=<n> ts_ns=<CGEvent timestamp> target_pid=<n> source_pid=<n> wall=<epoch s>     (Return/Enter, down and up)
//       K down keycode=<n> ts_ns=<CGEvent timestamp> target_pid=<n> source_pid=<n> wall=<epoch s>          (every OTHER key-down, #1509: so
//       enter_route.py can say how many of the keys the injector posted reached which process; no key-ups, no characters)
// Started by script/realapp/mac/ci/app_test.sh for the whole run (exact PID in the run's pids file, stopped by its trap / teardown.sh);
// script/realapp/mac/ci/enter_route.py matches each Enter to Phosphor's own `hop=mac_enter stage=app_send_event ts=` line by timestamp.
// Needs the Accessibility permission the job's process chain already has for htype2 (an active tap); if the tap cannot be created
// it prints ERR and exits 2 without touching anything.
// Usage: ktap OUTFILE   (runs until SIGTERM/SIGINT)
import CoreGraphics
import Foundation

let args = CommandLine.arguments
guard args.count == 2 else { FileHandle.standardError.write("usage: ktap OUTFILE\n".data(using: .utf8)!); exit(2) }
FileManager.default.createFile(atPath: args[1], contents: nil)
guard let out = FileHandle(forWritingAtPath: args[1]) else { print("ERR cannot open \(args[1])"); exit(2) }

var tapRef: CFMachPort?
let lock = NSLock()
var pending: [String] = []

let callback: CGEventTapCallBack = { _, type, event, _ in
  if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
    if let t = tapRef { CGEvent.tapEnable(tap: t, enable: true) }
    lock.lock(); pending.append("W tap re-enabled after \(type == .tapDisabledByTimeout ? "timeout" : "user input") wall=\(Date().timeIntervalSince1970)\n"); lock.unlock()
    return Unmanaged.passUnretained(event)
  }
  if type == .keyDown || type == .keyUp {
    let code = event.getIntegerValueField(.keyboardEventKeycode)
    if code == 36 || code == 76 {
      let line = "E \(type == .keyDown ? "down" : "up") keycode=\(code) ts_ns=\(event.timestamp) target_pid=\(event.getIntegerValueField(.eventTargetUnixProcessID)) source_pid=\(event.getIntegerValueField(.eventSourceUnixProcessID)) wall=\(String(format: "%.6f", Date().timeIntervalSince1970))\n"
      lock.lock(); pending.append(line); lock.unlock()
    } else if type == .keyDown {
      let line = "K down keycode=\(code) ts_ns=\(event.timestamp) target_pid=\(event.getIntegerValueField(.eventTargetUnixProcessID)) source_pid=\(event.getIntegerValueField(.eventSourceUnixProcessID)) wall=\(String(format: "%.6f", Date().timeIntervalSince1970))\n"
      lock.lock(); pending.append(line); lock.unlock()
    }
  }
  return Unmanaged.passUnretained(event)
}

let mask = (1 << CGEventType.keyDown.rawValue) | (1 << CGEventType.keyUp.rawValue)
guard let tap = CGEvent.tapCreate(tap: .cgAnnotatedSessionEventTap, place: .tailAppendEventTap, options: .defaultTap,
                                  eventsOfInterest: CGEventMask(mask), callback: callback, userInfo: nil) else {
  print("ERR CGEvent.tapCreate failed (no Accessibility permission for this process chain?)"); exit(2)
}
tapRef = tap
let src = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0)
CFRunLoopAddSource(CFRunLoopGetCurrent(), src, .commonModes)
CGEvent.tapEnable(tap: tap, enable: true)

// the callback only appends to memory; a timer flushes to the file, so the tap never waits on disk
func flush() {
  lock.lock(); let lines = pending; pending.removeAll(); lock.unlock()
  if !lines.isEmpty { out.write(lines.joined().data(using: .utf8)!) }
}
let timer = Timer(timeInterval: 0.2, repeats: true) { _ in flush() }
RunLoop.current.add(timer, forMode: .common)
for s in [SIGTERM, SIGINT] {
  signal(s, SIG_IGN)
  let ds = DispatchSource.makeSignalSource(signal: s, queue: .main)
  ds.setEventHandler { CGEvent.tapEnable(tap: tap, enable: false); flush(); out.closeFile(); exit(0) }
  ds.resume()
  _ = Unmanaged.passRetained(ds)
}
print("OK ktap running")
fflush(stdout)
CFRunLoopRun()
