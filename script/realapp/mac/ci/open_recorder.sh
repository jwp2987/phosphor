#!/bin/bash
# mac/ci/open_recorder.sh: the ONE-TIME machine change link_and_path_click needs on the test Mac (#2061, r14-test-gaps T32 / T21). Run BY THE OWNER, in
# the runner user's GUI session; the CI job never runs it and never changes a default handler.
#   open_recorder.sh status     which app opens https URLs now, and whether it is the recorder
#   open_recorder.sh install    build ~/Applications/PhosphorAppciOpenRecorder.app from script/realapp/mac/tools/openrec.swift, register it, and ask macOS
#                               to make it the default handler for http and https. macOS shows a confirmation dialog: the owner clicks "Use ...".
#   open_recorder.sh remove     delete the bundle and unregister it (then choose the default browser again in System Settings > Desktop & Dock)
# While it is the default handler, EVERY http/https link opened by this user is recorded to ~/Library/Caches/phosphor-appci/open-recorder.log and opens
# nothing. That is the point on a dedicated test machine and wrong on anyone's own Mac.
set -eu
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; SRC="$HERE/../tools/openrec.swift"
APP="$HOME/Applications/PhosphorAppciOpenRecorder.app"; BIN="$APP/Contents/MacOS/openrec"
LSREG=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
case "${1:-status}" in
  status)
    if [ -x "$BIN" ]; then cur=$("$BIN" --handler https://192.0.2.7/ || true); echo "https opens with: ${cur:-nothing}"
      [ "$cur" = "$APP" ] && echo "the recorder IS the default handler" || echo "the recorder is installed but NOT the default handler"
    else echo "not installed ($APP does not exist)"; fi;;
  install)
    command -v swiftc >/dev/null || { echo "swiftc is missing (xcode-select --install)"; exit 1; }
    mkdir -p "$APP/Contents/MacOS"
    swiftc -O "$SRC" -o "$BIN"
    cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>dev.phosphor.appci.open-recorder</string>
  <key>CFBundleName</key><string>PhosphorAppciOpenRecorder</string>
  <key>CFBundleExecutable</key><string>openrec</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSUIElement</key><true/>
  <key>CFBundleURLTypes</key><array><dict>
    <key>CFBundleURLName</key><string>appci recorded links</string>
    <key>CFBundleURLSchemes</key><array><string>http</string><string>https</string></array>
  </dict></array>
</dict></plist>
PLIST
    codesign --force --sign - "$APP" >/dev/null 2>&1 || true
    "$LSREG" -f "$APP"
    # NSWorkspace.setDefaultApplication (macOS 12+): the system asks the user to confirm for http / https
    /usr/bin/swift - "$APP" <<'SW'
import AppKit
let app = URL(fileURLWithPath: CommandLine.arguments[1])
let g = DispatchGroup()
for s in ["http", "https"] { g.enter(); NSWorkspace.shared.setDefaultApplication(at: app, toOpenURLsWithScheme: s) { e in if let e = e { print("\(s): \(e.localizedDescription)") }; g.leave() } }
g.notify(queue: .main) { exit(0) }
RunLoop.main.run()
SW
    "$0" status;;
  remove)
    [ -d "$APP" ] && { "$LSREG" -u "$APP" || true; rm -rf "$APP"; echo "removed $APP; choose the default browser again in System Settings"; } || echo "not installed";;
  *) echo "usage: $0 status|install|remove"; exit 2;;
esac
