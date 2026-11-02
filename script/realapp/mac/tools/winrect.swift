// winrect: window geometry from the WindowServer WITHOUT System Events / Apple Events (no Automation or Accessibility grant needed).
// Compiled at job start like htype2/kl3/ocr (script/realapp/mac/ci/app_test.sh); no binaries in git.
//   winrect rect PID   : "x,y,w,h" (points) of the largest on-screen normal (layer 0) window owned by PID; exit 1 when none
//   winrect list       : one line per on-screen window, front to back: "pid<TAB>owner<TAB>layer<TAB>x,y,w,h<TAB>name"
// Used by appci/backend.py (MacBackend.window_rect, MacBackend.system_dialog).
import Foundation
import CoreGraphics
let a = CommandLine.arguments
guard a.count >= 2, let arr = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as? [[String: Any]] else { exit(2) }
func rect(_ w: [String: Any]) -> (Double, Double, Double, Double)? {
  guard let b = w[kCGWindowBounds as String] as? [String: Any] else { return nil }
  func d(_ k: String) -> Double { (b[k] as? NSNumber)?.doubleValue ?? 0 }
  return (d("X"), d("Y"), d("Width"), d("Height"))
}
if a[1] == "list" {
  for w in arr {
    guard let r = rect(w) else { continue }
    let pid = (w[kCGWindowOwnerPID as String] as? NSNumber)?.intValue ?? -1
    let own = (w[kCGWindowOwnerName as String] as? String) ?? ""
    let layer = (w[kCGWindowLayer as String] as? NSNumber)?.intValue ?? 0
    let name = ((w[kCGWindowName as String] as? String) ?? "").replacingOccurrences(of: "\t", with: " ")
    print("\(pid)\t\(own)\t\(layer)\t\(Int(r.0)),\(Int(r.1)),\(Int(r.2)),\(Int(r.3))\t\(name)")
  }
  exit(0)
}
if a[1] == "rect", a.count >= 3, let want = Int(a[2]) {
  var best: (Double, Double, Double, Double)? = nil
  for w in arr {
    guard (w[kCGWindowOwnerPID as String] as? NSNumber)?.intValue == want,
          ((w[kCGWindowLayer as String] as? NSNumber)?.intValue ?? 0) == 0, let r = rect(w), r.2 > 100, r.3 > 100 else { continue }
    if best == nil || r.2 * r.3 > best!.2 * best!.3 { best = r }
  }
  if let b = best { print("\(Int(b.0)),\(Int(b.1)),\(Int(b.2)),\(Int(b.3))"); exit(0) }
  exit(1)
}
exit(2)
