// htype2: CGEvent key injector (+ optional ScreenCaptureKit change log). Source of truth is this file; the CI job compiles it
// with swiftc at job start (script/realapp/mac/ci/app_test.sh, cached by source hash). NEVER commit binaries.
// Origin: /Users/winters/phosphor-profile/r18m/htype2.swift (the proven round-18 Mac tool), copied 2026-11-01 with three additions, all
// marked "CI:" below: (1) w<=0 or h<=0 skips the screen capture entirely (no SCK start, no 0.6 s lead-in, no 1.5 s tail), so a
// keystroke burst costs only the tool's own process start; (2) the t: op takes its text up to the last two :min:max fields, so ':' can be
// typed; (3) op k:CODE:MODS:GAP posts one key with modifier flags (MODS bitmask shift=1 ctrl=2 alt=4 cmd=8) for ctrl+c, cmd+v, ...
// The caller (appci/backend.py MacBackend) checks frontmost app + Phosphor PID before EVERY invocation; this tool sends to whatever is focused.
// htype2 x y w h outfile op...   ops: t:TEXT:min:max  p:ms  e:gap  a:gap (right arrow) r:TEXT:gap (rollover) b:n:gap (backspace) x:gap(tab)
import Foundation
import ScreenCaptureKit
import CoreGraphics
import CoreMedia
import CoreVideo
let args = CommandLine.arguments
let rx = Double(args[1])!, ry = Double(args[2])!, rw = Double(args[3])!, rh = Double(args[4])!
let outPath = args[5]; let ops = Array(args[6...])
var tb = mach_timebase_info_data_t(); mach_timebase_info(&tb)
func toMs(_ t: UInt64) -> Double { Double(t) * Double(tb.numer) / Double(tb.denom) / 1e6 }
final class G: NSObject, SCStreamOutput {
  let lock = NSLock(); var last: [UInt8] = []; var changes: [Double] = []; var frames = 0
  func stream(_ s: SCStream, didOutputSampleBuffer sb: CMSampleBuffer, of type: SCStreamOutputType) {
    guard type == .screen else { return }
    guard let att = CMSampleBufferGetSampleAttachmentsArray(sb, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]], let a = att.first,
          let raw = a[.status] as? Int, let st = SCFrameStatus(rawValue: raw), st == .complete, let pb = CMSampleBufferGetImageBuffer(sb) else { return }
    let dt = (a[.displayTime] as? UInt64) ?? mach_absolute_time()
    CVPixelBufferLockBaseAddress(pb, .readOnly)
    let h = CVPixelBufferGetHeight(pb), bpr = CVPixelBufferGetBytesPerRow(pb), w = CVPixelBufferGetWidth(pb)
    let base = CVPixelBufferGetBaseAddress(pb)!.assumingMemoryBound(to: UInt8.self)
    var buf = [UInt8](repeating: 0, count: w*4*h)
    for y in 0..<h { memcpy(&buf[y*w*4], base + y*bpr, w*4) }
    CVPixelBufferUnlockBaseAddress(pb, .readOnly)
    lock.lock(); defer { lock.unlock() }
    frames += 1
    if !last.isEmpty {
      var diff = 0; var i = 0
      while i < buf.count { if abs(Int(buf[i])-Int(last[i])) > 24 || abs(Int(buf[i+1])-Int(last[i+1])) > 24 || abs(Int(buf[i+2])-Int(last[i+2])) > 24 { diff += 1 }; i += 4 }
      if diff >= 4 { changes.append(toMs(dt)) }
    }
    last = buf
  }
}
let src = CGEventSource(stateID: .hidSystemState)
func ev(_ key: CGKeyCode, _ s: String?, _ down: Bool) {
  let e = CGEvent(keyboardEventSource: src, virtualKey: key, keyDown: down)!
  e.flags = []   // CI: explicit empty modifier set, so a Control flag from an earlier k: op can never leak into typed text
  if let s = s { var u = Array(s.utf16); e.keyboardSetUnicodeString(stringLength: u.count, unicodeString: &u) }
  e.post(tap: .cghidEventTap)
}
let g = G(); let sem = DispatchSemaphore(value: 0); var ref: SCStream?
let capture = rw > 0 && rh > 0   // CI: no capture when the rect is empty
if capture { Task {
  do {
    let c = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
    let f = SCContentFilter(display: c.displays.first!, excludingWindows: [])
    let cfg = SCStreamConfiguration()
    cfg.sourceRect = CGRect(x: rx, y: ry, width: rw, height: rh); cfg.width = Int(rw*2); cfg.height = Int(rh*2)
    cfg.minimumFrameInterval = CMTime(value: 1, timescale: 240); cfg.queueDepth = 6; cfg.showsCursor = false
    cfg.pixelFormat = kCVPixelFormatType_32BGRA
    let s = SCStream(filter: f, configuration: cfg, delegate: nil)
    try s.addStreamOutput(g, type: .screen, sampleHandlerQueue: DispatchQueue(label: "cap", qos: .userInteractive))
    try await s.startCapture(); ref = s; sem.signal()
  } catch { print("ERR \(error)"); exit(1) }
} }
if capture { sem.wait(); Thread.sleep(forTimeInterval: 0.6) }
var out = ""
let m0 = mach_absolute_time()
out += "T0 mach_ms=\(toMs(m0)) epoch=\(Date().timeIntervalSince1970)\n"
func sl(_ ms: Double) { if ms > 0 { Thread.sleep(forTimeInterval: ms/1000) } }
func rnd(_ a: Double, _ b: Double) -> Double { a >= b ? a : Double.random(in: a...b) }
func logk(_ tag: String, _ ch: String) { out += "K \(tag) \(ch.unicodeScalars.first!.value) \(toMs(mach_absolute_time()))\n" }
var nk = 0
for op in ops {
  var p = op.components(separatedBy: ":")
  if p[0] == "t" && p.count > 4 { let tail = Array(p.suffix(2)); p = ["t", p[1..<(p.count-2)].joined(separator: ":")] + tail }   // CI: ':' inside the text
  switch p[0] {
  case "t": for ch in p[1] { sl(rnd(Double(p[2])!, Double(p[3])!)); logk(String(nk), String(ch)); nk += 1; ev(0, String(ch), true); ev(0, String(ch), false) }
  case "r":
    let chars = Array(p[1]); let gap = Double(p[2])!
    for (i, ch) in chars.enumerated() {
      sl(gap); logk(String(nk), String(ch)); nk += 1; ev(0, String(ch), true)
      if i > 0 { ev(0, String(chars[i-1]), false) }
    }
    sl(gap/2); ev(0, String(chars.last!), false)
  case "k":   // CI: k:CODE:MODS:GAP
    sl(Double(p[3])!); let code = CGKeyCode(UInt16(p[1])!); let m = Int(p[2])!
    var fl = CGEventFlags(rawValue: 0)
    if m & 1 != 0 { fl.insert(.maskShift) }; if m & 2 != 0 { fl.insert(.maskControl) }; if m & 4 != 0 { fl.insert(.maskAlternate) }; if m & 8 != 0 { fl.insert(.maskCommand) }
    logk("KEY\(code)", "\u{1}")
    for d in [true, false] { let e = CGEvent(keyboardEventSource: src, virtualKey: code, keyDown: d)!; e.flags = fl; e.post(tap: .cghidEventTap) }
  case "p": sl(Double(p[1])!)
  case "e": sl(Double(p[1])!); logk("ENTER", "\r"); ev(36, nil, true); ev(36, nil, false)
  case "a": sl(Double(p[1])!); logk("RIGHT", "\u{1}"); ev(124, nil, true); ev(124, nil, false)
  case "x": sl(Double(p[1])!); logk("TAB", "\t"); ev(48, nil, true); ev(48, nil, false)
  case "b": for _ in 0..<Int(p[1])! { sl(Double(p[2])!); logk("BS", "\u{7f}"); ev(51, nil, true); ev(51, nil, false) }
  default: break
  }
}
if capture { sl(1500) }
g.lock.lock(); for c in g.changes { out += "C \(c)\n" }; out += "FRAMES \(g.frames)\n"; g.lock.unlock()
try! out.write(toFile: outPath, atomically: true, encoding: .utf8)
exit(0)
