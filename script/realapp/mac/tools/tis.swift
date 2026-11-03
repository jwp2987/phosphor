// tis: Text Input Source helper for the IME cases (appci/cases_caseintl.py). Source of truth is this file; app_test.sh compiles it
// at job start with the other tools. NEVER commit binaries.
//   tis list            enabled input sources of THIS user: id TAB selectable TAB selected TAB name
//   tis current         id of the current keyboard input source
//   tis select ID       TISSelectInputSource on an ENABLED source (exit 2 = not enabled for this user, 3 = refused, 4 = did not take effect)
// It never enables or installs a source (that is a change to the account's settings: the owner does it, once).
import Foundation
import Carbon

func str(_ s: TISInputSource, _ key: CFString) -> String {
  guard let p = TISGetInputSourceProperty(s, key) else { return "" }
  return Unmanaged<CFString>.fromOpaque(p).takeUnretainedValue() as String
}
func flag(_ s: TISInputSource, _ key: CFString) -> Bool {
  guard let p = TISGetInputSourceProperty(s, key) else { return false }
  return CFBooleanGetValue(Unmanaged<CFBoolean>.fromOpaque(p).takeUnretainedValue())
}
func enabled() -> [TISInputSource] {
  return (TISCreateInputSourceList(nil, false).takeRetainedValue() as! [TISInputSource])
}
func currentID() -> String {
  return str(TISCopyCurrentKeyboardInputSource().takeRetainedValue(), kTISPropertyInputSourceID)
}
let a = CommandLine.arguments
guard a.count >= 2 else { print("usage: tis list|current|select ID"); exit(1) }
switch a[1] {
case "list":
  for s in enabled() {
    print("\(str(s, kTISPropertyInputSourceID))\t\(flag(s, kTISPropertyInputSourceIsSelectCapable))\t\(flag(s, kTISPropertyInputSourceIsSelected))\t\(str(s, kTISPropertyLocalizedName))")
  }
case "current":
  print(currentID())
case "select":
  guard a.count >= 3 else { exit(1) }
  guard let s = enabled().first(where: { str($0, kTISPropertyInputSourceID) == a[2] }) else { print("NOT ENABLED \(a[2])"); exit(2) }
  if TISSelectInputSource(s) != 0 { print("REFUSED \(a[2])"); exit(3) }
  for _ in 0..<40 {
    if currentID() == a[2] { print("OK \(a[2])"); exit(0) }
    Thread.sleep(forTimeInterval: 0.05)
  }
  print("NOT CURRENT \(a[2]) (current is \(currentID()))"); exit(4)
default:
  print("usage: tis list|current|select ID"); exit(1)
}
