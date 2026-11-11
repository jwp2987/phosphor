// openrec: a recording URL handler for link_and_path_click on macOS (#2061, r14-test-gaps T32 / T21). Phosphor opens a clicked link with
// -[NSWorkspace openURL:], so no PATH stand-in can record it (the Linux case puts a recording `xdg-open` first on the app's PATH). This is the stand-in:
// an app that, once the OWNER has made it the default handler for http and https on the test Mac (script/realapp/mac/ci/open_recorder.sh, a machine
// change that macOS confirms in a dialog), appends every URL it is asked to open to a log and opens NOTHING. Compiled by the job like the other tools.
//
//   openrec --handler URL      print the path of the application macOS would open URL with (exit 1 when none); used by the case to decide
//                              between running and SKIP "NEEDS: ..."
//   openrec --log              print the log path
//   openrec                    run as the handler (launched by LaunchServices from its bundle; LSUIElement, no window, no Dock icon)
// Log: ~/Library/Caches/phosphor-appci/open-recorder.log, one line "open<TAB><url>" per URL (appci/macptr.py recorder_calls).
import AppKit

let logURL = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Caches/phosphor-appci/open-recorder.log")
let a = CommandLine.arguments
if a.count >= 2 && a[1] == "--log" { print(logURL.path); exit(0) }
if a.count >= 3 && a[1] == "--handler" {
  guard let u = URL(string: a[2]), let app = NSWorkspace.shared.urlForApplication(toOpen: u) else { exit(1) }
  print(app.path); exit(0)
}

final class Recorder: NSObject, NSApplicationDelegate {
  func record(_ s: String) {
    try? FileManager.default.createDirectory(at: logURL.deletingLastPathComponent(), withIntermediateDirectories: true)
    let line = ("open\t" + s.replacingOccurrences(of: "\n", with: " ") + "\n").data(using: .utf8)!
    if let h = try? FileHandle(forWritingTo: logURL) { h.seekToEndOfFile(); h.write(line); h.closeFile() } else { try? line.write(to: logURL) }
  }
  func application(_ application: NSApplication, open urls: [URL]) { for u in urls { record(u.absoluteString) } }
  func applicationDidFinishLaunching(_ n: Notification) {
    // stay up for a short while so a burst of opens lands in one process, then leave: nothing of the harness stays running
    DispatchQueue.main.asyncAfter(deadline: .now() + 20) { NSApp.terminate(nil) }
  }
}
let app = NSApplication.shared
let delegate = Recorder()
app.delegate = delegate
app.setActivationPolicy(.accessory)
app.run()
