#!/usr/bin/env bash
# script/ci/clean_mutants_tmp.sh: remove stale cargo-mutants-* scratch directories from the temp dir (#1764).
#
# cargo-mutants copies the source tree to <tmp>/cargo-mutants-<name>-*.tmp and only removes it on a clean exit. A cancelled job
# leaves it behind (one was 18.65 GB and filled the Windows VM disk). Run before every mutants job.
# Safe by construction: does nothing if any cargo-mutants process is running for this user (a concurrent run's tree is live).
# Temp dir: $TMPDIR or /tmp (Linux, macOS); $TEMP / $TMP under Git Bash on Windows. Never fails the job.
set -uo pipefail
tmp="${TMPDIR:-${TEMP:-${TMP:-/tmp}}}"
tmp="${tmp%/}"
[[ -d "$tmp" ]] || { echo "clean_mutants_tmp: no temp dir ($tmp); nothing to do"; exit 0; }

running() {
  if command -v pgrep >/dev/null 2>&1; then
    pgrep -u "$(id -u)" -x 'cargo-mutants' >/dev/null 2>&1 && return 0
    return 1
  fi
  # Git Bash on Windows has no pgrep: ask tasklist (the current user's session; one job per machine).
  if command -v tasklist.exe >/dev/null 2>&1; then
    tasklist.exe 2>/dev/null | grep -qi 'cargo-mutants' && return 0
  fi
  return 1
}

free_space() { df -h "$tmp" 2>/dev/null | awk 'NR==2 {print $4 " free of " $2 " (" $5 " used)"}'; }

echo "clean_mutants_tmp: temp dir $tmp, $(free_space)"
shopt -s nullglob
dirs=("$tmp"/cargo-mutants-*)
if [[ ${#dirs[@]} -eq 0 ]]; then echo "clean_mutants_tmp: no stale cargo-mutants-* directories"; exit 0; fi
if running; then echo "clean_mutants_tmp: a cargo-mutants process is running for this user; leaving ${#dirs[@]} director(ies) alone"; exit 0; fi
for d in "${dirs[@]}"; do
  [[ -d "$d" && ! -L "$d" ]] || continue
  sz="$(du -sh "$d" 2>/dev/null | cut -f1)"
  if rm -rf -- "$d" 2>/dev/null; then echo "clean_mutants_tmp: removed $d (${sz:-?})"; else echo "clean_mutants_tmp: could not remove $d (${sz:-?})"; fi
done
echo "clean_mutants_tmp: after cleanup, $(free_space)"
exit 0
