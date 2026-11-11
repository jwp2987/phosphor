#!/usr/bin/env bash
# script/realapp/reap_leftovers.sh: source it, then `reap_leftovers OUT_DIR` (#2150).
# Ends every process whose command line names OUT_DIR (the job's own appci-out directory), by exact PID, after logging it: TERM, up to
# ~4 s of grace, then KILL for survivors. Never the calling script, its ancestors (the runner, the job shell), or anything whose command
# line does not name OUT_DIR. Why: a leftover Python child of the run held the job's output pipe after cleanup had merely LISTED it, and
# the macOS lab job sat for 6 h. Bounded: never waits more than ~6 s in all. Prints "reap:" lines; always returns 0.
reap_leftovers() {
  local out="${1:-}" p anc="" snap pid cmd todo="" n
  [ -n "$out" ] || return 0
  p=$$; n=0
  while [ -n "$p" ] && [ "$p" -gt 1 ] 2>/dev/null && [ $n -lt 40 ]; do anc="$anc $p"; p=$(ps -o ppid= -p "$p" 2>/dev/null | tr -d ' '); n=$((n+1)); done
  snap=$(ps -axo pid=,command= 2>/dev/null | grep -F -- "$out" | grep -v -F "grep -F" | cut -c1-200)
  while read -r pid cmd; do
    [ -n "${pid:-}" ] || continue
    case " $anc " in *" $pid "*) continue;; esac
    kill -0 "$pid" 2>/dev/null || continue            # a subshell of this very pipeline, already gone
    echo "reap: leftover pid $pid: $(printf '%s' "$cmd" | cut -c1-140)"
    todo="$todo $pid"
  done <<EOS
$snap
EOS
  if [ -z "$todo" ]; then echo "reap: no leftover process names $out"; return 0; fi
  for pid in $todo; do kill "$pid" 2>/dev/null; done
  for _ in 1 2 3 4 5 6 7 8; do
    n=0; for pid in $todo; do kill -0 "$pid" 2>/dev/null && n=1; done
    [ $n = 0 ] && break; sleep 0.5
  done
  for pid in $todo; do
    if kill -0 "$pid" 2>/dev/null; then kill -9 "$pid" 2>/dev/null; echo "reap: pid $pid needed SIGKILL"; else echo "reap: pid $pid ended"; fi
  done
  return 0
}
