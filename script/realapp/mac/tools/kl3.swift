// kl3: ScreenCaptureKit key->paint latency tool (stdin-driven). Source of truth is this file; the CI job compiles it with swiftc at job start
// (script/realapp/mac/ci/app_test.sh, cached by source hash). NEVER commit binaries.
// Origin: /Users/winters/phosphor-profile/r18m/lat/kl3.swift (the proven round-18 Mac tool), copied verbatim 2026-11-01. Used by the
// latency_paint case via appci/backend.py MacBackend.kl_*; the backend runs the frontmost-app + PID guard before every command it sends.
// kl2 x y w h  : stdin-driven latency tool. Commands (one per line):
//   T <capms> <text>   type text (no timing), settle, then Return (timed) and capture frames for capms
//   R <capms>          Return only (timed)
//   K <capms> <ch>     one key (timed)
//   W <text>           type text only, settle
//   C <capms>          capture only
// Output: "<CMD> post0=<epoch ms> post1=<epoch ms> n=<changes> ch=<epoch_ms>:<npix>,..."   (frame display times as epoch ms)
import Foundation
import ScreenCaptureKit
import CoreGraphics
import CoreMedia
import CoreVideo
let args = CommandLine.arguments
let rx = Double(args[1])!, ry = Double(args[2])!, rw = Double(args[3])!, rh = Double(args[4])!
var tb = mach_timebase_info_data_t(); mach_timebase_info(&tb)
func t2ms(_ t: UInt64) -> Double { Double(t) * Double(tb.numer) / Double(tb.denom) / 1e6 }
func epochNow() -> Double { Date().timeIntervalSince1970 * 1000 }
func machNow() -> Double { t2ms(mach_absolute_time()) }
let kc: [Character: CGKeyCode] = ["a":0,"s":1,"d":2,"f":3,"h":4,"g":5,"z":6,"x":7,"c":8,"v":9,"b":11,"q":12,"w":13,"e":14,"r":15,"y":16,"t":17,"1":18,"2":19,"3":20,"4":21,"6":22,"5":23,"=":24,"9":25,"7":26,"-":27,"8":28,"0":29,"]":30,"o":31,"u":32,"[":33,"i":34,"p":35,"l":37,"j":38,"\u{27}":39,"k":40,";":41,"\\":42,",":43,"/":44,"n":45,"m":46,".":47," ":49]
final class G: NSObject, SCStreamOutput {
  let lock = NSLock()
  var last: [UInt8] = []
  var armed = true
  var changes: [(Double, Int)] = []   // mach ms of frame, changed pixels
  var frames = 0
  func stream(_ s: SCStream, didOutputSampleBuffer sb: CMSampleBuffer, of type: SCStreamOutputType) {
    guard type == .screen else { return }
    guard let att = CMSampleBufferGetSampleAttachmentsArray(sb, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]], let a = att.first else { return }
    guard let raw = a[.status] as? Int, let st = SCFrameStatus(rawValue: raw), st == .complete else { return }
    guard let pb = CMSampleBufferGetImageBuffer(sb) else { return }
    let dt = (a[.displayTime] as? UInt64) ?? mach_absolute_time()
    CVPixelBufferLockBaseAddress(pb, .readOnly)
    let h = CVPixelBufferGetHeight(pb), bpr = CVPixelBufferGetBytesPerRow(pb), w = CVPixelBufferGetWidth(pb)
    let base = CVPixelBufferGetBaseAddress(pb)!.assumingMemoryBound(to: UInt8.self)
    var buf = [UInt8](repeating: 0, count: w * 4 * h)
    for y in 0..<h { memcpy(&buf[y * w * 4], base + y * bpr, w * 4) }
    CVPixelBufferUnlockBaseAddress(pb, .readOnly)
    lock.lock(); defer { lock.unlock() }
    frames += 1
    if armed && last.count == buf.count {
      var diff = 0; var i = 0
      while i < buf.count { if abs(Int(buf[i]) - Int(last[i])) > 24 || abs(Int(buf[i+1]) - Int(last[i+1])) > 24 || abs(Int(buf[i+2]) - Int(last[i+2])) > 24 { diff += 1 }; i += 4 }
      if diff >= 4 { changes.append((t2ms(dt), diff)) }
    }
    last = buf
  }
}
var shiftNext = false
var cmdNext = false
func post(_ k: CGKeyCode, _ down: Bool, _ src: CGEventSource?) { let e = CGEvent(keyboardEventSource: src, virtualKey: k, keyDown: down)!; e.flags = cmdNext ? .maskCommand : (shiftNext ? .maskShift : CGEventFlags(rawValue: 0)); e.post(tap: .cghidEventTap) }
func keyFor(_ s: String) -> CGKeyCode { cmdNext = false; if s == "TAB" { shiftNext = false; return 48 }; if s == "CMDP" { shiftNext = false; cmdNext = true; return 35 }; if s == "CMDK" { shiftNext = false; cmdNext = true; return 40 }; if s == "DEL" { shiftNext = false; return 51 }; if s == "ESC" { shiftNext = false; return 53 }; if s == "AT" { shiftNext = true; return 19 }; shiftNext = false; return kc[Array(s)[0]] ?? 0 }
func sl(_ m: Double) { if m > 0 { Thread.sleep(forTimeInterval: m/1000) } }
let g = G(); let sem = DispatchSemaphore(value: 0)
var ref: SCStream?
Task {
  do {
    let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
    let filter = SCContentFilter(display: content.displays.first!, excludingWindows: [])
    let cfg = SCStreamConfiguration()
    cfg.sourceRect = CGRect(x: rx, y: ry, width: rw, height: rh)
    cfg.width = Int(rw * 2); cfg.height = Int(rh * 2)
    cfg.minimumFrameInterval = CMTime(value: 1, timescale: 240)
    cfg.queueDepth = 6; cfg.showsCursor = false; cfg.pixelFormat = kCVPixelFormatType_32BGRA
    let s = SCStream(filter: filter, configuration: cfg, delegate: nil)
    try s.addStreamOutput(g, type: .screen, sampleHandlerQueue: DispatchQueue(label: "cap", qos: .userInteractive))
    try await s.startCapture(); ref = s; sem.signal()
  } catch { print("ERR \(error)"); exit(1) }
}
sem.wait(); sleep(1)
let src = CGEventSource(stateID: .hidSystemState)
func typeText(_ t: [Character]) {
  for (i, c) in t.enumerated() { guard let k = kc[c] else { print("BADCHAR"); return }; post(k, true, src); if i > 0, let p = kc[t[i-1]] { post(p, false, src) }; sl(20) }
  if let l = t.last, let k = kc[l] { sl(10); post(k, false, src) }
}
func settle(_ ms: Double) {
  // wait until no change for ms
  g.lock.lock(); g.armed = true; g.changes = []; g.lock.unlock()
  let start = machNow()
  while true {
    sl(20)
    g.lock.lock(); let lc = g.changes.last?.0 ?? 0; g.lock.unlock()
    let n = machNow()
    if n - max(lc, start) >= ms { break }
    if n - start > 6000 { break }
  }
}
func timed(_ cap: Double, _ key: CGKeyCode, _ st: Double = 350) -> String {
  if st > 0 { settle(st) }
  g.lock.lock(); g.changes = []; g.lock.unlock()
  let off = epochNow() - machNow()
  let m0 = machNow(); let e0 = epochNow()
  post(key, true, src); let e1 = epochNow()
  post(key, false, src)
  sl(cap)
  g.lock.lock(); let ch = g.changes; g.lock.unlock()
  _ = m0
  let s = ch.map { String(format: "%.1f:%d", $0.0 + off, $0.1) }.joined(separator: ",")
  return String(format: "post0=%.3f post1=%.3f n=%d ch=%@", e0, e1, ch.count, s)
}
while let line = readLine() {
  let p = line.split(separator: " ", maxSplits: 2, omittingEmptySubsequences: false).map(String.init)
  switch p[0] {
  case "T": typeText(Array(p[2])); print("T " + timed(Double(p[1])!, 36))
  case "F": typeText(Array(p[2])); sl(15); print("F " + timed(Double(p[1])!, 36, 0))
  case "R": print("R " + timed(Double(p[1])!, 36))
  case "K": print("K " + timed(Double(p[1])!, keyFor(p[2]), Double(ProcessInfo.processInfo.environment["KSETTLE"] ?? "350")!))
  case "W": typeText(Array(p[1] + (p.count > 2 ? " " + p[2] : ""))); print("W done")
  case "C": settle(1); sl(Double(p[1])!); g.lock.lock(); let ch = g.changes; g.lock.unlock(); let off = epochNow() - machNow(); print("C n=\(ch.count) ch=" + ch.map { String(format: "%.1f:%d", $0.0 + off, $0.1) }.joined(separator: ","))
  case "B":
    // B <n> <gapms> <chars comma list>: burst of n keys without settling; per-key post times + all changes
    let n = Int(p[1])!; let gap = Double(p[2].split(separator: ",")[0])!
    let keys = p[2].split(separator: ",").dropFirst().map(String.init)
    settle(350)
    g.lock.lock(); g.changes = []; g.lock.unlock()
    let off = epochNow() - machNow()
    var posts: [Double] = []
    for i in 0..<n { let k = keys[i % keys.count]; let kk: CGKeyCode = k == "DEL" ? 51 : (kc[Array(k)[0]] ?? 0); posts.append(epochNow()); post(kk, true, src); post(kk, false, src); sl(gap) }
    sl(300)
    g.lock.lock(); let ch = g.changes; g.lock.unlock()
    print("B posts=" + posts.map { String(format: "%.3f", $0) }.joined(separator: ",") + " ch=" + ch.map { String(format: "%.1f:%d", $0.0 + off, $0.1) }.joined(separator: ","))
  case "Q": exit(0)
  default: print("?")
  }
  fflush(stdout)
}
