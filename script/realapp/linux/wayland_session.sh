#!/bin/bash
# wayland_session.sh: run the real-app test matrix against a built Phosphor in a private graphical session (CI or by hand).
#
#   --mode wayland   headless sway (WLR_BACKENDS=headless, gles2 on /dev/dri/renderD128, pixman fallback), app runs as a
#                    native Wayland client; keys via wtype, screenshots via grim, clipboard via wl-copy/wl-paste.
#   --mode xwayland  the same sway session, but the app is forced onto X11 (XWayland; WAYLAND_DISPLAY unset for it) for X11
#                    parity; keys still go through the compositor (wtype), so it exercises XWayland's input path.
#   --mode x11       a private Xvfb (software GL) with xdotool/xsel, the pre-existing X11 path (see linux/f.sh for the old trials).
#
# Everything lives under --out (default $CI_PROJECT_DIR/appci-out/<mode>): its own XDG_RUNTIME_DIR (short path, socket limit 108
# bytes), a private HOME with an owner-like scratch config and NO credentials, a private dbus session for the app, a local
# ssh server on 127.0.0.1. It starts only processes it records in $OUT/pids and tears them down by those exact PIDs
# (process groups it created); it never pkill/pgrep-by-pattern and never touches another session.
#
# Prerequisites (checked, never installed): bash python3 dbus-run-session gnome-keyring-daemon; wayland modes: sway wtype grim
# wl-copy wl-paste (+ Xwayland for xwayland/the reference terminal); x11 mode: Xvfb xdotool xsel ffmpeg|import. Optional:
# tesseract (mode proof / approval card), xterm (reference terminal), zsh fish nu (subshell cases), vim less,
# /usr/sbin/sshd or python3-paramiko (ssh cases), vulkan drivers for the GPU (mesa-vulkan-drivers).
# Environment: APPCI_OLLAMA_URL (optional provider endpoint; without it provider-dependent agent cases skip), APPCI_SCALE,
# APPCI_TABS, APPCI_BIG_MB, APPCI_SSH_TRIALS, APPCI_SEED, APPCI_WL_KEY_MODE=batch|perkey, APPCI_IDLE_CPU_MAX.
# usage: wayland_session.sh --mode M --dist DIR_WITH_deb_or_AppImage [--bin PATH] [--out DIR] [--modes terminal,agent] [--cases a,b] [--skip a,b]
# APPCI_TIER=smoke runs only the smoke case list (appci/cases.py SMOKE); the xwayland and x11 CI passes are skipped by the jobs.
# Exit: 0 all pass/skip, 1 a case failed, 2 prerequisites/session could not start.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APPCI="$HERE/../appci"
MODE=wayland DIST="" BIN="" OUT="" SMODES="${APPCI_MODES:-terminal,agent}" CASES="${APPCI_CASES:-}" SKIP=""   # APPCI_MODES / APPCI_CASES: play-time narrowing for a diagnostic run (#1414)
while [ $# -gt 0 ]; do case "$1" in
  --mode) MODE=$2; shift 2;; --dist) DIST=$2; shift 2;; --bin) BIN=$2; shift 2;; --out) OUT=$2; shift 2;;
  --modes) SMODES=$2; shift 2;; --cases) CASES=$2; shift 2;; --skip) SKIP=$2; shift 2;;
  *) echo "unknown arg $1" >&2; exit 2;; esac; done
case "$MODE" in wayland|xwayland|x11) ;; *) echo "bad --mode $MODE" >&2; exit 2;; esac
OUT=${OUT:-${CI_PROJECT_DIR:-$PWD}/appci-out/$MODE}
mkdir -p "$OUT"; OUT=$(cd "$OUT" && pwd)
PIDS="$OUT/pids"; : > "$PIDS"
log() { echo "[session $MODE] $*" | tee -a "$OUT/session.log"; }
need=()
for t in python3 dbus-run-session; do command -v $t >/dev/null || need+=("$t"); done
command -v gnome-keyring-daemon >/dev/null || log "WARNING gnome-keyring-daemon missing: the app may hang after 'Running app with windowing system'"
if [ "$MODE" = x11 ]; then
  for t in Xvfb xdotool xsel; do command -v $t >/dev/null || need+=("$t"); done
  command -v ffmpeg >/dev/null || command -v import >/dev/null || need+=("ffmpeg-or-imagemagick")
else
  for t in sway swaymsg wtype grim wl-copy wl-paste; do command -v $t >/dev/null || need+=("$t"); done
  command -v Xwayland >/dev/null || { [ "$MODE" = xwayland ] && need+=("Xwayland") || log "note: Xwayland missing (no reference terminal)"; }
fi

teardown() {
  # exact PIDs/process groups recorded in $PIDS, newest first
  local pid name
  while read -r name pid; do
    [ -n "${pid:-}" ] || continue
    kill -TERM -- "-$pid" 2>/dev/null || kill -TERM "$pid" 2>/dev/null
  done < <(tac "$PIDS")
  sleep 1.5
  while read -r name pid; do
    [ -n "${pid:-}" ] || continue
    kill -KILL -- "-$pid" 2>/dev/null || kill -KILL "$pid" 2>/dev/null
  done < <(tac "$PIDS")
  [ -n "${RT_TMP:-}" ] && rm -rf "$RT_TMP"
  true
}
trap teardown EXIT

# ---- the build under test
if [ -z "$BIN" ] && [ -n "$DIST" ]; then
  X="$OUT/app"; rm -rf "$X"; mkdir -p "$X"
  deb=$(ls "$DIST"/*.deb 2>/dev/null | head -1); img=$(ls "$DIST"/*.AppImage 2>/dev/null | head -1)
  if [ -n "$deb" ] && command -v dpkg-deb >/dev/null; then dpkg-deb -x "$deb" "$X" || need+=("working-deb");
  elif [ -n "$img" ]; then (cd "$X" && cp "$img" ./pho.AppImage && chmod +x pho.AppImage && ./pho.AppImage --appimage-extract >/dev/null 2>&1);
  else need+=("a .deb or .AppImage in $DIST (needs build-linux-x86_64 artifacts)"); fi
  BIN=$(find "$X" -type f -name phosphor-oss -perm -u+x 2>/dev/null | head -1)
fi
[ -n "$BIN" ] && [ -x "$BIN" ] || need+=("an executable phosphor-oss binary (--bin or --dist)")
if [ ${#need[@]} -gt 0 ]; then
  log "MISSING PREREQUISITES: ${need[*]}"
  python3 - "$OUT" "${need[*]}" <<'PY'
import json,sys
open(sys.argv[1]+"/results.jsonl","w").write(json.dumps(dict(name="preflight",mode="-",status="fail",seconds=0,detail=["missing: "+sys.argv[2]]))+"\n")
PY
  python3 "$APPCI/report.py" "$OUT" "platform=linux" "backend=$MODE" >/dev/null
  exit 2
fi
log "binary: $BIN ($($BIN --version 2>&1 | head -1))"

# ---- short runtime dir (unix socket limit)
RT="$OUT/rt"
if [ ${#RT} -gt 60 ]; then RT_TMP=$(mktemp -d /tmp/pa-XXXXXX); RT=$RT_TMP; fi
mkdir -p "$RT"; chmod 700 "$RT"
export XDG_RUNTIME_DIR="$RT"
unset WAYLAND_DISPLAY DISPLAY SWAYSOCK WAYLAND_SOCKET

start_sway() { # renderer
  local r=$1 i
  rm -f "$RT"/wayland-* "$RT"/sway-ipc.*
  cat > "$OUT/sway.conf" <<CONF
xwayland enable
default_border none
default_floating_border none
focus_follows_mouse no
output HEADLESS-1 resolution ${APPCI_RES:-1600x1000} position 0 0
exec_always sh -c 'echo "\$DISPLAY" > "$OUT/xdisplay"'
CONF
  local rdev=()
  [ "$r" = gles2 ] && [ -e /dev/dri/renderD128 ] && rdev=(WLR_RENDER_DRM_DEVICE=/dev/dri/renderD128)
  env WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 WLR_RENDERER=$r "${rdev[@]}" XDG_RUNTIME_DIR="$RT" \
    setsid sway -c "$OUT/sway.conf" > "$OUT/sway.$r.log" 2>&1 &
  SWAYPID=$!
  echo "sway $SWAYPID" >> "$PIDS"
  for i in $(seq 1 60); do
    kill -0 $SWAYPID 2>/dev/null || return 1
    if ls "$RT"/wayland-[0-9]* >/dev/null 2>&1 && ls "$RT"/sway-ipc.*.sock >/dev/null 2>&1; then return 0; fi
    sleep 0.25
  done
  return 1
}

if [ "$MODE" = x11 ]; then
  setsid Xvfb -displayfd 3 -screen 0 ${APPCI_RES:-1600x1000}x24 3> "$OUT/xdisp" > "$OUT/xvfb.log" 2>&1 &
  echo "xvfb $!" >> "$PIDS"
  for i in $(seq 1 40); do [ -s "$OUT/xdisp" ] && break; sleep 0.25; done
  [ -s "$OUT/xdisp" ] || { log "Xvfb did not start"; cat "$OUT/xvfb.log"; exit 2; }
  export DISPLAY=":$(head -1 "$OUT/xdisp" | tr -d '[:space:]')"
  BACKEND=x11 APPDISP=x11 RENDERER=llvmpipe
else
  RENDERER=""
  for r in gles2 pixman; do
    if start_sway $r; then RENDERER=$r; break; fi
    log "sway with renderer $r failed; tail of its log:"; tail -5 "$OUT/sway.$r.log" | sed 's/^/    /'
    kill -TERM "$SWAYPID" 2>/dev/null; sleep 1
  done
  [ -n "$RENDERER" ] || { log "no sway renderer worked (gles2, pixman). Is the runner in video+render groups with access to /dev/dri/renderD128?"; exit 2; }
  W=$(basename "$(ls "$RT"/wayland-[0-9]* | grep -v '\.lock$' | head -1)")
  export WAYLAND_DISPLAY="$W" SWAYSOCK="$(ls "$RT"/sway-ipc.*.sock | head -1)"
  sleep 1
  XD=$(cat "$OUT/xdisplay" 2>/dev/null | tr -d '[:space:]'); [ -n "$XD" ] && export DISPLAY="$XD"
  BACKEND=wayland APPDISP=wayland; [ "$MODE" = xwayland ] && APPDISP=x11
  [ "$MODE" = xwayland ] && [ -z "${DISPLAY:-}" ] && { log "XWayland DISPLAY was not published by sway"; exit 2; }
fi
log "session up: renderer=$RENDERER WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-} DISPLAY=${DISPLAY:-} app-display=$APPDISP"
{ echo "== uname"; uname -srm; echo "== dri"; ls -l /dev/dri 2>&1; echo "== vulkan"; vulkaninfo --summary 2>&1 | head -20; } > "$OUT/gpu.txt" 2>&1

RUNDIR="$OUT/run"; mkdir -p "$RUNDIR"
python3 "$APPCI/run.py" --run "$RUNDIR" --bin "$BIN" --backend "$BACKEND" --app-display "$APPDISP" --modes "$SMODES" \
  --cases "$CASES" --skip "$SKIP" --platform "linux-$MODE"
RC=$?
cp "$OUT/sway."*.log "$OUT/gpu.txt" "$RUNDIR/logs/" 2>/dev/null
log "done rc=$RC; results in $RUNDIR/results.json"
exit $RC
