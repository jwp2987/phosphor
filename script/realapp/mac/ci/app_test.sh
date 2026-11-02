#!/bin/bash
# mac/ci/app_test.sh: CI real-app test on macOS (job app-test-macos). Usage: app_test.sh DIST_DIR OUT_DIR
# Installs the DMG artifact of build-macos-arm64 into OUT/Phosphor.app (never over an installed app), then runs the
# shared case matrix (script/realapp/appci/run.py --backend mac) in BOTH session modes.
#
# THIS NEEDS A GUI SESSION. The Mac runner jobs run as user `gitrunner`; keystroke injection (CGEvent via the htype2 tool,
# osascript System Events) and screen capture only work when that user has a logged-in Aqua session AND the process chain that runs
# the job holds Accessibility + Screen Recording (TCC). The script PROBES this first and, when it is missing, fails
# with exit 3 and the exact list of what the owner has to grant. It does not try to work around TCC.
# Owner rules baked in (docs/testing.md 3.3): caffeinate for the whole run (recorded PID, killed at the end),
# frontmost-app guard before every key burst (backend.MacBackend.guard), no forbidden chords, exact-PID kills only,
# settings backup+sha256 restore, no run while a latency measurement is going (the job is manual + resource_group).
set -u
DIST=${1:?dist dir}; OUT=${2:?out dir}
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; APPCI="$HERE/../../appci"
export PATH="/opt/homebrew/bin:/usr/local/bin:$PATH"
mkdir -p "$OUT"; OUT=$(cd "$OUT" && pwd); : > "$OUT/pids"
CAFF=""; PYPID=""; DONE=0
# Exact-PID cleanup, also on SIGTERM/INT/HUP (a cancelled GitLab job): python runner first (its SIGTERM handler unwinds app.stop /
# sshd.stop), then anything still recorded in the pids files whose command line is verified to be OURS (PID reuse safe), caffeinate, DMG.
killif() {   # killif PID SUBSTRING: signal PID only when its command line contains SUBSTRING
  local p="$1" pat="$2" cmd; [ -n "$p" ] || return 0
  cmd=$(ps -o command= -p "$p" 2>/dev/null) || return 0
  case "$cmd" in *"$pat"*) kill "$p" 2>/dev/null; sleep 1; kill -0 "$p" 2>/dev/null && kill -9 "$p" 2>/dev/null;; esac; return 0; }
cleanup() {
  [ "$DONE" = 1 ] && return; DONE=1
  if [ -n "$PYPID" ] && kill -0 "$PYPID" 2>/dev/null; then kill "$PYPID" 2>/dev/null; for _ in 1 2 3 4 5 6 7 8 9 10 11 12; do kill -0 "$PYPID" 2>/dev/null || break; sleep 1; done; kill -9 "$PYPID" 2>/dev/null; fi
  for f in "$OUT/pids" "$OUT/run/pids"; do [ -f "$f" ] && while read -r kind p _; do [ "$kind" = app ] && killif "$p" "${BIN:-/nonexistent}"; done < "$f"; done
  [ -f "$OUT/run/ssh/sshd.pid" ] && killif "$(cat "$OUT/run/ssh/sshd.pid" 2>/dev/null)" "$OUT/run/ssh/sshd_config"
  [ -n "$CAFF" ] && kill "$CAFF" 2>/dev/null
  hdiutil detach "$OUT/mnt" -quiet 2>/dev/null; true; }
trap cleanup EXIT
trap 'cleanup; exit 143' TERM INT HUP
fail3() { echo "NEEDS OWNER: $*" | tee "$OUT/NEEDS_OWNER.txt"; 
  python3 - "$OUT" "$*" <<'PY'
import json,sys
open(sys.argv[1]+"/results.jsonl","w").write(json.dumps(dict(name="preflight",mode="-",status="fail",seconds=0,detail=[sys.argv[2]]))+"\n")
PY
  python3 "$APPCI/report.py" "$OUT" platform=macos backend=mac >/dev/null; exit 3; }

for t in python3 hdiutil osascript screencapture; do command -v $t >/dev/null || fail3 "$t missing"; done
command -v swiftc >/dev/null || fail3 "swiftc is missing for user $(whoami) (xcode-select --install): the key-injection tools are compiled at job start"
# compile the injection / latency / OCR tools from source (script/realapp/mac/tools/*.swift; NO binaries in git). Cached by source hash
# in the runner's own dir, so a build only happens when a source changed.
TSRC="$HERE/../tools"; TCACHE="$HOME/.cache/phosphor-appci-tools"; mkdir -p "$TCACHE"; TOOLS="$OUT/tools"; mkdir -p "$TOOLS"
for t in htype2 kl3 ocr; do
  h=$(shasum -a 256 "$TSRC/$t.swift" | cut -c1-16); b="$TCACHE/$t-$h"
  if [ ! -x "$b" ]; then swiftc -O "$TSRC/$t.swift" -o "$b.tmp.$$" 2> "$OUT/swiftc-$t.log" && mv "$b.tmp.$$" "$b" || { rm -f "$b.tmp.$$"; fail3 "swiftc failed for $t.swift (see swiftc-$t.log in the job artifacts)"; }; fi
  cp "$b" "$TOOLS/$t"
done
export APPCI_MAC_TOOLS="$TOOLS"
[ "$(launchctl managername 2>/dev/null)" = Aqua ] || fail3 "the job is not in a GUI (Aqua) session (managername=$(launchctl managername 2>&1)). The gitlab-runner must run as a LaunchAgent of a logged-in, unlocked user, not a LaunchDaemon."

DMG=$(ls "$DIST"/*.dmg 2>/dev/null | head -1); [ -n "$DMG" ] || fail3 "no .dmg in $DIST (needs build-macos-arm64 artifacts)"
mkdir -p "$OUT/mnt"; hdiutil attach "$DMG" -nobrowse -readonly -mountpoint "$OUT/mnt" -quiet || fail3 "cannot mount $DMG"
APPSRC=$(ls -d "$OUT/mnt"/*.app | head -1); rm -rf "$OUT/Phosphor.app"; cp -R "$APPSRC" "$OUT/Phosphor.app"
hdiutil detach "$OUT/mnt" -quiet; xattr -dr com.apple.quarantine "$OUT/Phosphor.app" 2>/dev/null
BIN=$(ls "$OUT"/Phosphor.app/Contents/MacOS/* | head -1)
echo "app: $BIN ($("$BIN" --version 2>&1 | head -1))"

# keep the screen awake and prove the display is usable BEFORE any key is sent (a locked screen takes keys into the password field)
caffeinate -dimsu & CAFF=$!; echo "caffeinate $CAFF" >> "$OUT/pids"
PROBE=$(python3 -c "
import sys; sys.path.insert(0,'$APPCI')
import backend
p=backend.make('mac').preflight(); print('\n'.join(p))")
[ -z "$PROBE" ] || fail3 "$PROBE. Grant Accessibility and Screen Recording (System Settings > Privacy & Security) to the runner's process chain (gitlab-runner binary, /bin/bash, python3, the compiled htype2/kl3/ocr tools, osascript, screencapture) for user $(whoami), and keep the session unlocked."

# latency: run.py fetches the previous successful app-test-macos results.json (build/* refs) and compares against appci/baselines/macos.json;
# the reference terminal is Terminal.app (cases.ref_terminal), driven by the same htype2/osascript path under the frontmost-app guard.
export APPCI_PREV_ARTIFACT="appci-out/macos/run/results.json"
export APPCI_MAC_BUNDLE="$OUT/Phosphor.app" APPCI_MAC_PROCESS="$(basename "$BIN")"
export APPCI_KEY_NEWTAB="cmd+t" APPCI_KEY_NEWWIN="cmd+n"
python3 "$APPCI/run.py" --run "$OUT/run" --bin "$BIN" --backend mac --modes "${APPCI_MODES:-terminal,agent}" \
  --cases "${APPCI_CASES:-}" --skip "${APPCI_SKIP:-}" --platform macos --startup-timeout 120 &
PYPID=$!
wait "$PYPID"; RC=$?
PYPID=""
exit $RC
