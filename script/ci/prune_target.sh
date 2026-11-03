#!/bin/sh
# Prune the persistent cargo target dirs on the CI build box so they cannot fill the disk
# (2026-11: ci-test reached 219 GB and ci-check-linux 119 GB; jobs then failed).
#
# Usage: script/ci/prune_target.sh            # every dir under $PHOSPHOR_TARGET_ROOT
#        script/ci/prune_target.sh <label>... # only those labels
# Plain POSIX sh + find/du/df; does not need cargo-sweep. Never touches CARGO_HOME/registry.
# Never fails the job: all errors are logged and ignored.
#
# Env (defaults in parens):
#   PHOSPHOR_TARGET_ROOT   base dir of the per-label target dirs
#   PRUNE_MIN_FREE_GB (80) below this much free space on the filesystem, prune harder
#   PRUNE_STALE_DAYS  (7)  files untouched this long are "stale"
#   PRUNE_CAP_GB_<label>   per-dir cap, label upper-cased with - as _ (default PRUNE_CAP_GB_DEFAULT=60;
#                          ci-test=120, ci-check-linux=60)
#   PRUNE_DRY_RUN=1        only report
#
# In-use markers (#1523): script/agent-cargo writes $ROOT/.inuse/<label>.<pid> (containing its pid) while a cargo process
# compiles into <label>. A label with a LIVE marker is never pruned or wiped (it is logged and skipped); markers of dead pids are
# removed. The check is repeated right before the stale-file pass and before the whole-dir wipe, because a job can start
# between the first check and the delete; the window that remains is the few ms between the last check and `rm -rf` (the
# `resource_group`s that serialise the box jobs make a concurrent cargo on the same label rare in the first place).
#
# Per dir: (1) always drop incremental/ of release/LTO profiles; (2) if free space is low or the
# dir is over its cap: drop all incremental/, then stale files, then (still over) wipe the dir;
# (3) log what was freed. nextest output (nextest/, junit) is also capped by age.

ROOT="${PHOSPHOR_TARGET_ROOT:-/home/gitlab-runner/.phosphor-build/targets}"
MIN_FREE_GB="${PRUNE_MIN_FREE_GB:-80}"
STALE_DAYS="${PRUNE_STALE_DAYS:-7}"
DEFAULT_CAP="${PRUNE_CAP_GB_DEFAULT:-60}"

log() { echo "[prune_target] $*" >&2; }
rm_tree() { if [ -n "$PRUNE_DRY_RUN" ]; then log "dry-run: rm -rf $1"; else rm -rf "$1" 2>/dev/null; fi; }
# Sizes are kept in MB internally, thresholds are GB.
used_mb() { du -sk "$1" 2>/dev/null | awk '{n=$1} END{printf "%d", n/1024}'; }
free_mb() { df -Pk "$ROOT" 2>/dev/null | awk 'NR==2{n=$4} END{printf "%d", n/1024}'; }
cap_for() {
  case "$1" in
    ci-test) c=120 ;;
    ci-check-linux) c=60 ;;
    *) c="$DEFAULT_CAP" ;;
  esac
  v=$(echo "$1" | tr 'a-z-' 'A-Z_')
  eval "o=\${PRUNE_CAP_GB_$v:-}"
  echo "${o:-$c}"
}

# in_use <label>: true when a live agent-cargo marker exists for it; stale markers are deleted on the way.
in_use() {
  for m in "$ROOT/.inuse/$1".*; do
    [ -f "$m" ] || continue
    pid=$(cat "$m" 2>/dev/null)
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then return 0; fi
    rm -f "$m" 2>/dev/null
  done
  return 1
}

[ -d "$ROOT" ] || { log "no $ROOT; nothing to prune"; exit 0; }
if [ $# -gt 0 ]; then labels="$*"; else labels=$(ls "$ROOT" 2>/dev/null); fi

for label in $labels; do
  d="$ROOT/$label"
  [ -d "$d" ] || continue
  case "$label" in tmp|locks|logs|.inuse) continue ;; esac
  if in_use "$label"; then log "$label: a cargo process is using this target dir right now (.inuse marker); not pruning"; continue; fi
  start=$(used_mb "$d"); free0=$(free_mb); cap=$(cap_for "$label")
  # (1) incremental caches of release/LTO-style profiles never help across branches.
  for p in "$d"/release* "$d"/*/release* "$d"/dist "$d"/*/dist "$d"/*lto* "$d"/*/*lto*; do
    [ -d "$p/incremental" ] && rm_tree "$p/incremental"
  done
  # nextest/junit output is only useful for the current run.
  find "$d" -maxdepth 3 \( -name nextest -o -name junit.xml \) -mtime +1 2>/dev/null | while read -r n; do rm_tree "$n"; done
  now=$(used_mb "$d"); free1=$(free_mb)
  if [ "$free1" -lt $((MIN_FREE_GB * 1024)) ] || [ "$now" -gt $((cap * 1024)) ]; then
    log "$label: used ${now}M (cap ${cap}G), free ${free1}M (min ${MIN_FREE_GB}G): pruning"
    find "$d" -type d -name incremental -prune 2>/dev/null | while read -r n; do rm_tree "$n"; done
    if in_use "$label"; then log "$label: became in use; stopping"; continue; fi
    [ -z "$PRUNE_DRY_RUN" ] && find "$d" -type f -mtime +"$STALE_DAYS" -atime +"$STALE_DAYS" -delete 2>/dev/null
    [ -z "$PRUNE_DRY_RUN" ] && find "$d" -depth -type d -empty -delete 2>/dev/null
    now=$(used_mb "$d"); free1=$(free_mb)
    if [ "$free1" -lt $((MIN_FREE_GB * 1024)) ] || [ "$now" -gt $((cap * 1024)) ]; then
      if in_use "$label"; then log "$label: still over its limit but now in use; NOT wiping"; continue; fi
      log "$label: still ${now}M used, ${free1}M free after stale prune: wiping target dir"
      rm_tree "$d"
    fi
  fi
  end=$(used_mb "$d"); free2=$(free_mb)
  log "$label: ${start}M -> ${end}M (freed $((start - end))M), disk free ${free0}M -> ${free2}M"
done
exit 0
