// mptr: CGEvent POINTER injector for the macOS app-test (#2061, r14-test-gaps T32). The keyboard twin is htype2. Source of truth is this file; the
// CI job compiles it with swiftc at job start (script/realapp/mac/ci/app_test.sh, cached by source hash). NEVER commit binaries. Nothing is installed:
// CoreGraphics ships with macOS (the runner has no PyObjC, checked 2026-11-11: `import Quartz` fails for /usr/bin/python3).
//
//   mptr info                       "display X Y W H PW PH" (main display: bounds in points, mode size in pixels) and "cursor X Y" (points)
//   mptr owner X Y                  "pid<TAB>owner<TAB>layer" of the topmost on-screen window at the point (what a press there would hit)
//   mptr PID OUTFILE op...          pointer ops, in order. All coordinates are POINTS in the global display space (top-left origin), the space
//                                   CGWindowList bounds use (winrect) and a screenshot's pixels divided by the display scale.
// ops:  m:X:Y            move
//       k:X:Y:B:N        N clicks of button B at X,Y (B: 1 left, 2 middle, 3 right, the X11 numbering the harness uses; click state 1..N)
//       g:X0:Y0:X1:Y1    left-button drag in 8 steps
//       w:X:Y:STEPS      wheel, STEPS lines: positive = down (later lines), negative = up
//       f:BITS           hold modifiers for the ops that follow (shift=1 ctrl=2 alt=4 cmd=8; 0 releases). Always released before exit.
//       p:MS             pause
// SAFETY (docs/testing.md 3.3; the caller, appci/backend.py MacBackend.ptr, also runs the frontmost-app guard before every call):
//   - every press, drag end and wheel is refused (exit 4, nothing posted for that op, modifiers released) unless the TOPMOST on-screen window at
//     that point is owned by PID, the app under test. A click can therefore never land on another app, the Dock, the menu bar or a system dialog.
//   - a point outside the main display is refused before anything is posted (exit 2).
// OUTFILE: "T0 mach_ms= epoch=", one "P <op> <x> <y> <mach_ms>" per posted action, "LOC x y" (the cursor read back from the window server after the
// last op: the proof that the events were accepted), or "ERR <why>".
import Foundation
import CoreGraphics

let args = CommandLine.arguments
func err(_ s: String) { FileHandle.standardError.write((s + "\n").data(using: .utf8)!) }
var tb = mach_timebase_info_data_t(); mach_timebase_info(&tb)
func nowMs() -> Double { Double(mach_absolute_time()) * Double(tb.numer) / Double(tb.denom) / 1e6 }
func sl(_ ms: Double) { if ms > 0 { Thread.sleep(forTimeInterval: ms / 1000) } }
func cursor() -> CGPoint { CGEvent(source: nil)?.location ?? .zero }
let mainBounds = CGDisplayBounds(CGMainDisplayID())

let ignoreOwners = Set((ProcessInfo.processInfo.environment["APPCI_MAC_PTR_IGNORE_OWNERS"] ?? "").split(separator: ",").map { String($0) })
/// (pid, owner name, layer) of the topmost on-screen window that contains `p`, front to back; nil when no window does
func ownerAt(_ p: CGPoint) -> (Int, String, Int)? {
  guard let arr = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else { return nil }
  for w in arr {
    guard let b = w[kCGWindowBounds as String] as? [String: Any] else { continue }
    func d(_ k: String) -> Double { (b[k] as? NSNumber)?.doubleValue ?? 0 }
    let r = CGRect(x: d("X"), y: d("Y"), width: d("Width"), height: d("Height"))
    let alpha = (w[kCGWindowAlpha as String] as? NSNumber)?.doubleValue ?? 1
    let own = (w[kCGWindowOwnerName as String] as? String) ?? ""
    if alpha <= 0 || !r.contains(p) || ignoreOwners.contains(own) { continue }
    return ((w[kCGWindowOwnerPID as String] as? NSNumber)?.intValue ?? -1, own, (w[kCGWindowLayer as String] as? NSNumber)?.intValue ?? 0)
  }
  return nil
}

if args.count >= 2 && args[1] == "info" {
  var pw = Int(mainBounds.width), ph = Int(mainBounds.height)
  if let m = CGDisplayCopyDisplayMode(CGMainDisplayID()) { pw = m.pixelWidth; ph = m.pixelHeight }
  print("display \(Int(mainBounds.minX)) \(Int(mainBounds.minY)) \(Int(mainBounds.width)) \(Int(mainBounds.height)) \(pw) \(ph)")
  let c = cursor(); print("cursor \(c.x) \(c.y)")
  exit(0)
}
if args.count >= 4 && args[1] == "owner" {
  guard let x = Double(args[2]), let y = Double(args[3]) else { err("usage: mptr owner X Y"); exit(2) }
  if let o = ownerAt(CGPoint(x: x, y: y)) { print("\(o.0)\t\(o.1)\t\(o.2)"); exit(0) }
  exit(1)
}
let pid: Int = args.count >= 3 ? (Int(args[1]) ?? -1) : -1          // a plain global: mustOwn() below reads it
if pid <= 1 { err("usage: mptr info | mptr owner X Y | mptr PID OUTFILE op..."); exit(2) }
let outPath = args[2]
let ops = Array(args[3...])

// ---- parse and validate EVERYTHING before the first event is posted
enum Op { case move(CGPoint), click(CGPoint, Int, Int), drag(CGPoint, CGPoint), wheel(CGPoint, Int), mods(Int), pause(Double) }
var plan: [Op] = []
func pt(_ a: String, _ b: String) -> CGPoint? {
  guard let x = Double(a), let y = Double(b) else { return nil }
  let p = CGPoint(x: x, y: y)
  return mainBounds.contains(p) ? p : nil
}
for op in ops {
  let p = op.components(separatedBy: ":")
  var parsed: Op? = nil
  switch p[0] {
  case "m": if p.count == 3, let a = pt(p[1], p[2]) { parsed = .move(a) }
  case "k": if p.count == 5, let a = pt(p[1], p[2]), let b = Int(p[3]), let n = Int(p[4]), (1...3).contains(b), (1...3).contains(n) { parsed = .click(a, b, n) }
  case "g": if p.count == 5, let a = pt(p[1], p[2]), let b = pt(p[3], p[4]) { parsed = .drag(a, b) }
  case "w": if p.count == 4, let a = pt(p[1], p[2]), let n = Int(p[3]), n != 0, abs(n) <= 200 { parsed = .wheel(a, n) }
  case "f": if p.count == 2, let m = Int(p[1]), (0...15).contains(m) { parsed = .mods(m) }
  case "p": if p.count == 2, let ms = Double(p[1]), ms >= 0, ms <= 10000 { parsed = .pause(ms) }
  default: break
  }
  guard let o = parsed else { err("bad op \(op) (unknown, malformed, or a point outside the main display \(mainBounds))"); exit(2) }
  plan.append(o)
}

let src = CGEventSource(stateID: .hidSystemState)
var flags = CGEventFlags(rawValue: 0)
var held: [(CGKeyCode, CGEventFlags)] = []
var out = "T0 mach_ms=\(nowMs()) epoch=\(Date().timeIntervalSince1970)\n"
func plog(_ what: String, _ p: CGPoint) { out += "P \(what) \(p.x) \(p.y) \(nowMs())\n" }

func mouse(_ type: CGEventType, _ p: CGPoint, _ button: CGMouseButton, _ clicks: Int64 = 0) {
  guard let e = CGEvent(mouseEventSource: src, mouseType: type, mouseCursorPosition: p, mouseButton: button) else { return }
  e.flags = flags
  if clicks > 0 { e.setIntegerValueField(.mouseEventClickState, value: clicks) }
  e.post(tap: .cghidEventTap)
}
func modKey(_ code: CGKeyCode, _ down: Bool) {
  guard let e = CGEvent(keyboardEventSource: src, virtualKey: code, keyDown: down) else { return }
  e.type = .flagsChanged
  e.flags = flags
  e.post(tap: .cghidEventTap)
}
func setMods(_ m: Int) {
  for (code, fl) in held.reversed() { flags.remove(fl); modKey(code, false) }
  held = []
  let table: [(Int, CGKeyCode, CGEventFlags)] = [(1, 56, .maskShift), (2, 59, .maskControl), (4, 58, .maskAlternate), (8, 55, .maskCommand)]
  for (bit, code, fl) in table where m & bit != 0 { flags.insert(fl); modKey(code, true); held.append((code, fl)) }
  sl(40)
}
func finish(_ code: Int32, _ why: String?) -> Never {
  setMods(0)
  sl(60)                                   // as htype2 (#1482): the hand-off to the window server is asynchronous
  let c = cursor()
  out += "LOC \(c.x) \(c.y)\n"
  if let w = why { out += "ERR \(w)\n"; err(w) }
  try? out.write(toFile: outPath, atomically: true, encoding: .utf8)
  exit(code)
}
/// refuse unless the topmost window at `p` is the app's
func mustOwn(_ p: CGPoint, _ what: String) {
  let o = ownerAt(p)
  if o?.0 != pid {
    var whose = "no window"
    if let w = o { whose = w.1 + " (pid " + String(w.0) + ", layer " + String(w.2) + ")" }
    finish(4, "refused \(what) at (\(p.x), \(p.y)): the topmost window there belongs to \(whose), not to pid \(pid)")
  }
}
func moveTo(_ p: CGPoint) { mouse(.mouseMoved, p, .left); sl(20) }
let buttons: [Int: (CGEventType, CGEventType, CGMouseButton)] = [1: (.leftMouseDown, .leftMouseUp, .left), 2: (.otherMouseDown, .otherMouseUp, .center),
                                                                 3: (.rightMouseDown, .rightMouseUp, .right)]

for op in plan {
  switch op {
  case .move(let p): moveTo(p); plog("move", p)
  case .pause(let ms): sl(ms)
  case .mods(let m): setMods(m)
  case .click(let p, let b, let n):
    moveTo(p); sl(130)
    mustOwn(p, "a click")
    let (down, up, btn) = buttons[b]!
    for i in 1...n {
      mouse(down, p, btn, Int64(i)); sl(30)
      mouse(up, p, btn, Int64(i)); sl(70)
      plog("click\(b)", p)
    }
    sl(150)
  case .drag(let a, let b):
    moveTo(a); sl(130)
    mustOwn(a, "a drag start"); mustOwn(b, "a drag end")
    mouse(.leftMouseDown, a, .left, 1); plog("down", a); sl(150)
    for i in 1...8 {
      let t = CGFloat(i) / 8
      mouse(.leftMouseDragged, CGPoint(x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t), .left, 1); sl(30)
    }
    mouse(.leftMouseDragged, b, .left, 1); sl(60)
    mouse(.leftMouseUp, b, .left, 1); plog("up", b)
    sl(250)
  case .wheel(let p, let n):
    moveTo(p); sl(130)
    mustOwn(p, "a wheel step")
    for _ in 0..<abs(n) {
      if let e = CGEvent(scrollWheelEvent2Source: src, units: .line, wheelCount: 1, wheel1: n > 0 ? -1 : 1, wheel2: 0, wheel3: 0) { e.flags = flags; e.post(tap: .cghidEventTap) }
      sl(40)
    }
    plog("wheel", p)
    sl(300)
  }
}
finish(0, nil)
