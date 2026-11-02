#!/bin/bash
# mac/ci/app_test.sh: CI real-app test on macOS (job app-test-macos). Usage: app_test.sh DIST_DIR OUT_DIR
# Installs the DMG artifact of build-macos-arm64 into OUT/Phosphor.app (never over an installed app), then runs the
# shared case matrix (script/realapp/appci/run.py --backend mac) in BOTH session modes.
#
# THIS NEEDS A GUI SESSION. The Mac runner jobs run as user `gitrunner`; keystroke injection (osascript System Events,
# cliclick) and screen capture only work when that user has a logged-in Aqua session AND the process chain that runs
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
CAFF=""
cleanup() { [ -n "$CAFF" ] && kill "$CAFF" 2>/dev/null; hdiutil detach "$OUT/mnt" -quiet 2>/dev/null; true; }
trap cleanup EXIT
fail3() { echo "NEEDS OWNER: $*" | tee "$OUT/NEEDS_OWNER.txt"; 
  python3 - "$OUT" "$*" <<'PY'
import json,sys
open(sys.argv[1]+"/results.jsonl","w").write(json.dumps(dict(name="preflight",mode="-",status="fail",seconds=0,detail=[sys.argv[2]]))+"\n")
PY
  python3 "$APPCI/report.py" "$OUT" platform=macos backend=mac >/dev/null; exit 3; }

for t in python3 hdiutil osascript screencapture; do command -v $t >/dev/null || fail3 "$t missing"; done
command -v cliclick >/dev/null || fail3 "cliclick is missing for user $(whoami) (brew install cliclick)"
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
[ -z "$PROBE" ] || fail3 "$PROBE. Grant Accessibility and Screen Recording (System Settings > Privacy & Security) to the runner's process chain (gitlab-runner binary, /bin/bash, python3, cliclick, osascript, screencapture) for user $(whoami), and keep the session unlocked."

# latency: run.py fetches the previous successful app-test-macos results.json (build/* refs) and compares against appci/baselines/macos.json;
# the reference terminal is Terminal.app (cases.ref_terminal), driven by the same cliclick/osascript path under the frontmost-app guard.
export APPCI_PREV_ARTIFACT="appci-out/macos/run/results.json"
export APPCI_MAC_BUNDLE="$OUT/Phosphor.app" APPCI_MAC_PROCESS="$(basename "$BIN")"
python3 "$APPCI/run.py" --run "$OUT/run" --bin "$BIN" --backend mac --modes "${APPCI_MODES:-terminal,agent}" \
  --cases "${APPCI_CASES:-}" --skip "${APPCI_SKIP:-}" --platform macos --startup-timeout 120
exit $?
