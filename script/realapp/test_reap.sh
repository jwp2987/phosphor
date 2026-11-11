#!/usr/bin/env bash
# Self-test for reap_leftovers (#2150): a child that names the job's out dir and holds the job's stdout must not keep the step open.
# Offline, needs only bash + python3. Run: script/realapp/test_reap.sh
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$HERE/reap_leftovers.sh"
W=$(mktemp -d "${TMPDIR:-.}/reap-test.XXXXXX"); W=$(cd "$W" && pwd); OUT="$W/appci-out"; mkdir -p "$OUT"
fail() { echo "FAIL: $*"; pkill -f -- "$OUT" 2>/dev/null; rm -rf "$W"; exit 1; }
# the "step": starts the leaker (which keeps this subshell's stdout open) and exits; the reader plays the runner waiting for EOF
( python3 -c 'import time; time.sleep(600)' "$OUT/leftover" & echo $! > "$W/leaker.pid"; sleep 0.3 ) | cat > "$W/reader.out" &
RP=$!
sleep 1.5
kill -0 "$RP" 2>/dev/null || fail "control: the leaker should hold the pipe open (reader already ended), the test proves nothing"
LP=$(cat "$W/leaker.pid")
reap_leftovers "$OUT" | tee "$W/reap.log"
for _ in $(seq 1 30); do kill -0 "$RP" 2>/dev/null || break; sleep 0.5; done
kill -0 "$RP" 2>/dev/null && fail "the pipe is still open 15 s after reap_leftovers: a leftover child keeps the step open"
kill -0 "$LP" 2>/dev/null && fail "leaker pid $LP is still alive"
grep -q "leftover pid $LP" "$W/reap.log" || fail "the reaped PID was not logged"
# nothing to reap: says so, returns 0, does not touch this script
reap_leftovers "$OUT" | grep -q "no leftover" || fail "empty case"
rm -rf "$W"; echo "reap self-test ok"
