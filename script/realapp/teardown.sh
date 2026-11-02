#!/usr/bin/env bash
# script/realapp/teardown.sh OUT_DIR...: after_script of every app-test-* job (GitLab runs after_script also when the job is
# CANCELLED, when the job script's own EXIT/TERM trap may never run). Kills only processes recorded in OUT_DIR/pids and
# OUT_DIR/run/pids (lines "<kind> <pid> ..." or "<name> <pid>") plus run/ssh/sshd.pid, and only when that PID's command line
# contains OUT_DIR or the app bundle path under it (PID-reuse safe). Never by name or pattern. Idempotent; always exits 0.
for out in "$@"; do
  [ -d "$out" ] || continue
  out=$(cd "$out" && pwd)
  pids=""
  for f in "$out/pids" "$out/run/pids"; do
    [ -f "$f" ] && pids="$pids $(awk '{for(i=1;i<=NF;i++) if ($i ~ /^[0-9]+$/) {print $i; break}}' "$f")"
  done
  [ -f "$out/run/ssh/sshd.pid" ] && pids="$pids $(cat "$out/run/ssh/sshd.pid" 2>/dev/null)"
  ours=""
  for p in $pids; do
    cmd=$(ps -o command= -p "$p" 2>/dev/null) || continue
    case "$cmd" in *"$out"*|*"${CI_PROJECT_DIR:-/nonexistent}/appci-out"*|"caffeinate -dimsu") ours="$ours $p";; esac
  done
  [ -n "$ours" ] || { echo "teardown $out: nothing left running"; continue; }
  echo "teardown $out: stopping$ours"
  kill $ours 2>/dev/null; sleep 2
  for p in $ours; do kill -0 "$p" 2>/dev/null && kill -9 "$p" 2>/dev/null; done
done
exit 0
