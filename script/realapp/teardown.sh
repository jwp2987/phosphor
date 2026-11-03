#!/usr/bin/env bash
# script/realapp/teardown.sh OUT_DIR...: after_script of every app-test-* job (GitLab runs after_script also when the job is
# CANCELLED, when the job script's own EXIT/TERM trap may never run). Kills only processes recorded in OUT_DIR/pids and
# OUT_DIR/run/pids (lines "<kind> <pid> ..." or "<name> <pid>") plus run/ssh/sshd.pid, and only when that PID's command line
# contains OUT_DIR or the app bundle path under it (PID-reuse safe). Never by name or pattern. Idempotent; always exits 0.
# #1517: a recorded process that is its OWN process-group leader (everything the harness starts with setsid / start_new_session:
# sway, Xvfb, the wtype keyboard holder, dbus-run-session + the app under it) is stopped by its group ("kill -- -PGID", PGID == the
# recorded PID, never a group the runner or the job shell is in), so children such as the app under dbus-run-session do not survive a
# cancel. A non-leader gets a plain PID kill. `xvfb <pid>` lines (Xvfb's command line never names OUT_DIR) are ours when the process is
# a group-leading `Xvfb -displayfd` that started after OUT_DIR was created. Still never by name or pattern.
# One extra rule (#1485): a `terminal <pid>` line (the Terminal.app this run launched as the macOS reference) is ours only when the
# command is exactly the Terminal.app executable AND that PID started after OUT_DIR was created (ps lstart, so a reused PID is safe).
for out in "$@"; do
  [ -d "$out" ] || continue
  out=$(cd "$out" && pwd)
  pids=""; termpids=""
  for f in "$out/pids" "$out/run/pids"; do
    [ -f "$f" ] && pids="$pids $(awk '{for(i=1;i<=NF;i++) if ($i ~ /^[0-9]+$/) {print $i; break}}' "$f")"
    [ -f "$f" ] && termpids="$termpids $(awk '$1=="terminal" && $2 ~ /^[0-9]+$/ {print $2}' "$f")"
  done
  [ -f "$out/run/ssh/sshd.pid" ] && pids="$pids $(cat "$out/run/ssh/sshd.pid" 2>/dev/null)"
  ours=""
  for p in $pids; do
    cmd=$(ps -o command= -p "$p" 2>/dev/null) || continue
    case "$cmd" in *"$out"*|*"${CI_PROJECT_DIR:-/nonexistent}/appci-out"*|"caffeinate -dimsu") ours="$ours $p";; esac
  done
  tpath=/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal
  for p in $termpids; do
    [ "$(ps -o command= -p "$p" 2>/dev/null)" = "$tpath" ] || continue
    born=$(stat -f %B "$out" 2>/dev/null || stat -c %W "$out" 2>/dev/null); [ "${born:-0}" -gt 0 ] 2>/dev/null || born=$(stat -f %m "$out" 2>/dev/null || stat -c %Y "$out" 2>/dev/null)
    started=$(LC_ALL=C ps -o lstart= -p "$p" 2>/dev/null | python3 -c 'import sys,time; print(int(time.mktime(time.strptime(sys.stdin.read().strip(), "%a %b %d %H:%M:%S %Y"))))' 2>/dev/null)
    [ -n "$started" ] && [ -n "$born" ] && [ "$started" -gt "$born" ] && case " $ours " in *" $p "*) ;; *) ours="$ours $p";; esac
  done
  xvpids=$(for f in "$out/pids" "$out/run/pids"; do [ -f "$f" ] && awk '$1=="xvfb" && $2 ~ /^[0-9]+$/ {print $2}' "$f"; done)
  born=$(stat -f %B "$out" 2>/dev/null || stat -c %W "$out" 2>/dev/null); [ "${born:-0}" -gt 0 ] 2>/dev/null || born=$(stat -f %m "$out" 2>/dev/null || stat -c %Y "$out" 2>/dev/null)
  for p in $xvpids; do
    case " $ours " in *" $p "*) continue;; esac
    cmd=$(ps -o command= -p "$p" 2>/dev/null) || continue
    case "$cmd" in "Xvfb -displayfd"*|*"/Xvfb -displayfd"*) ;; *) continue;; esac
    [ "$(ps -o pgid= -p "$p" 2>/dev/null | tr -d ' ')" = "$p" ] || continue
    started=$(LC_ALL=C ps -o lstart= -p "$p" 2>/dev/null | python3 -c 'import sys,time; print(int(time.mktime(time.strptime(sys.stdin.read().strip(), "%a %b %d %H:%M:%S %Y"))))' 2>/dev/null)
    [ -n "$started" ] && [ -n "$born" ] && [ "$started" -ge "$born" ] && ours="$ours $p"
  done
  [ -n "$ours" ] || { echo "teardown $out: nothing left running"; continue; }
  echo "teardown $out: stopping$ours"
  pgs=""
  for p in $ours; do   # decide group vs PID BEFORE signalling (a dead leader no longer reports its pgid)
    if [ "$(ps -o pgid= -p "$p" 2>/dev/null | tr -d ' ')" = "$p" ]; then pgs="$pgs $p"; else kill "$p" 2>/dev/null; fi
  done
  for g in $pgs; do kill -- "-$g" 2>/dev/null; done
  sleep 2
  for p in $ours; do kill -0 "$p" 2>/dev/null && kill -9 "$p" 2>/dev/null; done
  for g in $pgs; do kill -0 -- "-$g" 2>/dev/null && kill -9 -- "-$g" 2>/dev/null; done
done
exit 0
