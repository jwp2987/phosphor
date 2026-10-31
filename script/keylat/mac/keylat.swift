// keylat: key-post -> first changed captured frame latency, via ScreenCaptureKit.
// usage: keylat x y w h samples mode [settle_ms]
//   rect in points (global display coords). mode: "ab" = type 'a' then backspace alternately;
//   "append" = type successive letters (no deletes).
import Foundation
import ScreenCaptureKit
import CoreGraphics
import CoreMedia
import CoreVideo

let args = CommandLine.arguments
guard args.count >= 7 else { print("usage: keylat x y w h samples mode [settle_ms]"); exit(2) }
let rx = Double(args[1])!, ry = Double(args[2])!, rw = Double(args[3])!, rh = Double(args[4])!
let samples = Int(args[5])!
let mode = args[6]
let settleMs = args.count > 7 ? Double(args[7])! : 250

var tb = mach_timebase_info_data_t(); mach_timebase_info(&tb)
func ticksToMs(_ t: UInt64) -> Double { Double(t) * Double(tb.numer) / Double(tb.denom) / 1e6 }

final class Grabber: NSObject, SCStreamOutput {
  let lock = NSLock()
  var lastFrame: [UInt8] = []
  var lastFrameTime: UInt64 = 0
  var lastChangeTime: UInt64 = 0   // time of last frame that differed from its predecessor
  var frameCount = 0
  var completeCount = 0
  var baseline: [UInt8]? = nil
  var armedAt: UInt64 = 0
  var hitTime: UInt64 = 0
  var hitDiff = 0
  var minDiff = 4
  func stream(_ stream: SCStream, didOutputSampleBuffer sb: CMSampleBuffer, of type: SCStreamOutputType) {
    guard type == .screen else { return }
    guard let att = CMSampleBufferGetSampleAttachmentsArray(sb, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
          let a = att.first else { return }
    frameCount += 1
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
    completeCount += 1
    if buf != lastFrame { lastChangeTime = dt }
    lastFrame = buf; lastFrameTime = dt
    if let b = baseline, hitTime == 0, dt > armedAt {
      var diff = 0
      var i = 0
      while i < buf.count { if abs(Int(buf[i]) - Int(b[i])) > 24 || abs(Int(buf[i+1]) - Int(b[i+1])) > 24 || abs(Int(buf[i+2]) - Int(b[i+2])) > 24 { diff += 1 }; i += 4 }
      if diff >= minDiff { hitTime = dt; hitDiff = diff }
    }
  }
}

func post(_ key: CGKeyCode) {
  let src = CGEventSource(stateID: .hidSystemState)
  let d = CGEvent(keyboardEventSource: src, virtualKey: key, keyDown: true)!
  let u = CGEvent(keyboardEventSource: src, virtualKey: key, keyDown: false)!
  d.post(tap: .cghidEventTap); u.post(tap: .cghidEventTap)
}
// ANSI keycodes for a..z
let letters: [CGKeyCode] = [0,11,8,2,14,3,5,4,34,38,40,37,46,45,31,35,12,15,1,17,32,9,13,7,16,6]
let kA: CGKeyCode = 0, kDel: CGKeyCode = 51

let g = Grabber()
let sem = DispatchSemaphore(value: 0)
var streamRef: SCStream?
Task {
  do {
    let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
    let disp = content.displays.first!
    let filter = SCContentFilter(display: disp, excludingWindows: [])
    let cfg = SCStreamConfiguration()
    cfg.sourceRect = CGRect(x: rx, y: ry, width: rw, height: rh)
    cfg.width = Int(rw * 2); cfg.height = Int(rh * 2)
    cfg.minimumFrameInterval = CMTime(value: 1, timescale: 240)
    cfg.queueDepth = 6
    cfg.showsCursor = false
    cfg.pixelFormat = kCVPixelFormatType_32BGRA
    let s = SCStream(filter: filter, configuration: cfg, delegate: nil)
    try s.addStreamOutput(g, type: .screen, sampleHandlerQueue: DispatchQueue(label: "cap", qos: .userInteractive))
    try await s.startCapture()
    streamRef = s
    let mode = CGDisplayCopyDisplayMode(CGMainDisplayID())
    FileHandle.standardError.write("refresh=\(mode?.refreshRate ?? -1)\n".data(using: .utf8)!)
    sem.signal()
  } catch { print("ERR \(error)"); exit(1) }
}
sem.wait()
Thread.sleep(forTimeInterval: 0.5)

func waitStable() -> Bool {
  // wait until no change for settleMs (max 5s)
  let start = mach_absolute_time()
  while true {
    g.lock.lock(); let lc = g.lastChangeTime; g.lock.unlock()
    let now = mach_absolute_time()
    if ticksToMs(now &- max(lc, start)) >= settleMs && ticksToMs(now - start) >= settleMs { return true }
    if ticksToMs(now - start) > 5000 { return false }
    Thread.sleep(forTimeInterval: 0.005)
  }
}

var lat: [Double] = []
var seq: [String] = []
var misses = 0
var li = 0
var unstable = 0
for i in 0..<(samples + 2) {
  if !waitStable() { unstable += 1 }
  // jitter so we don't phase-lock to vsync
  Thread.sleep(forTimeInterval: Double.random(in: 0...0.017))
  g.lock.lock()
  g.baseline = g.lastFrame; g.hitTime = 0
  let t0 = mach_absolute_time(); g.armedAt = t0
  g.lock.unlock()
  let key: CGKeyCode
  if mode == "ab" { key = (i % 2 == 0) ? kA : kDel } else { key = letters[li % 26]; li += 1 }
  post(key)
  var got = false
  let ws = mach_absolute_time()
  while ticksToMs(mach_absolute_time() - ws) < 1000 {
    g.lock.lock(); let ht = g.hitTime; g.lock.unlock()
    if ht != 0 { if i >= 2 { lat.append(ticksToMs(ht - t0)); seq.append(String(format: "%@%.0f", key == kDel ? "d" : "k", ticksToMs(ht - t0))) }; got = true; break }
    Thread.sleep(forTimeInterval: 0.001)
  }
  g.lock.lock(); g.baseline = nil; g.lock.unlock()
  if !got && i >= 2 { misses += 1; seq.append("MISS") }
}
if mode == "ab" && (samples + 2) % 2 == 1 { post(kDel) }
lat.sort()
func pct(_ p: Double) -> Double { lat.isEmpty ? .nan : lat[min(lat.count - 1, Int(Double(lat.count - 1) * p + 0.5))] }
print(String(format: "n=%d misses=%d unstable=%d median=%.1f p95=%.1f max=%.1f min=%.1f frames=%d complete=%d", lat.count, misses, unstable, pct(0.5), pct(0.95), lat.last ?? .nan, lat.first ?? .nan, g.frameCount, g.completeCount))
print("seq " + seq.joined(separator: " "))
print("raw " + lat.map { String(format: "%.1f", $0) }.joined(separator: " "))
exit(0)
