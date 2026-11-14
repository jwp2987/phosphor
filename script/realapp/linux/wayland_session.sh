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
# APPCI_TABS, APPCI_BIG_MB, APPCI_SSH_TRIALS, APPCI_SEED, APPCI_WL_KEY_MODE=batch|perkey, APPCI_WL_KBD_HOLD=0|1 (default 1: persistent virtual keyboard, #1414), APPCI_WL_PTR_HOLD=0|1 (default 1: persistent virtual pointer, #1789), APPCI_IDLE_CPU_MAX.
# Display matrix (#2078, harness only; default unchanged = scale 1, one output): APPCI_DISPLAY_SCALE=<0.5..4> (e.g. 1.25 1.5 2) runs the WHOLE run at that
# display scale (wayland / xwayland: sway `output HEADLESS-1 ... scale S`; x11: Xvfb -dpi and the Xft.dpi X resource, which needs `xrdb`, else the run exits 2
# rather than silently running at scale 1). APPCI_SECOND_OUTPUT_SCALE=<s> (wayland / xwayland only) adds a second headless output of that scale to the right of
# the first, so windows can sit on, or be moved to, an output of another scale. Both are recorded in results.json meta (display_scale, outputs). NOT the
# same as APPCI_SCALE, which shortens counts for a smoke run. Cases that assume scale 1 are expected to say so, not to be skipped here.
# usage: wayland_session.sh --mode M --dist DIR_WITH_deb_or_AppImage [--bin PATH] [--out DIR] [--modes terminal,agent] [--cases a,b] [--skip a,b]
# APPCI_TIER=smoke runs only the fix-tier case list (appci/cases.py SMOKE + PROVING); the Wayland job skips its xwayland pass then. The x11 job runs in
# every tier since round 15 (#2058), both session modes (the --modes default below).
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
PIDS="$OUT/pids"
# r18d8: reap what an EARLIER killed job left behind BEFORE this job truncates its own pidfile (a job killed by the runner's timeout left
# sway, the app, foot, the wtype holder and the test sshd running; the after_script teardown only knew the CURRENT job's dir). Exact
# recorded PIDs only, command-line checked against the old dir (teardown.sh), never by name.
"$HERE/../teardown.sh" "$OUT" 2>&1 | sed 's/^/[session] stale: /'         # this dir's own pidfiles, if the checkout kept them
"$HERE/../teardown.sh" --stale 2>&1 | sed 's/^/[session] stale: /'          # anything carrying a dead session's APPCI_SESSION marker
: > "$PIDS"
# every process this session starts inherits the marker (the reaper above identifies leftovers by it; script pid:job id:mode)
export APPCI_SESSION="$$:${CI_JOB_ID:-manual}:$MODE"
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

. "$HERE/../reap_leftovers.sh"
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
  # #2150: anything still alive whose command line names this out dir is logged and ended by PID (it could keep the job's output pipe open)
  reap_leftovers "$OUT" | sed 's/^/[session] /'
  [ -n "${RT_TMP:-}" ] && rm -rf "$RT_TMP"
  true
}
# #2123: the logs and screenshots are made ready for upload HERE (scrub, then run_slim/), after the teardown, on every exit path of this script
# (the EXIT trap also runs after on_signal and after the `exit 2` paths), not in the job's after_script: GitLab gives an after_script 5 minutes in
# all, and a run that failed then kept no logs or shots. In CI only (CI_JOB_ID); APPCI_ARTIFACTS=1 does it for a run by hand, =0 never. It cannot
# change this script's exit status. The after_script keeps `appci_artifacts.py --fallback` as the net for a job that was killed before this ran.
artifacts() {
  case "${APPCI_ARTIFACTS:-${CI_JOB_ID:+1}}" in 1) ;; *) return 0;; esac
  python3 "$HERE/../../ci/appci_artifacts.py" "$OUT" 2>&1 | sed 's/^/[session] /'
  true
}
trap 'teardown; artifacts' EXIT
# r18d8: GitLab's job timeout/cancel signals only this bash; python (run.py) used to keep running for 2 more minutes and its rows were
# lost. On TERM/INT/HUP: signal run.py's process group (the exact pgid it was started with, below), give it time to flush results.json
# and stop the app, then exit (the EXIT trap runs teardown over every recorded PID).
RUNPID=""
on_signal() {
  local sig=$1
  trap '' TERM INT HUP
  log "caught $sig: stopping run.py (process group ${RUNPID:-none}), then teardown"
  if [ -n "$RUNPID" ]; then
    kill -TERM -- "-$RUNPID" 2>/dev/null || kill -TERM "$RUNPID" 2>/dev/null
    for _ in $(seq 1 60); do kill -0 "$RUNPID" 2>/dev/null || break; sleep 0.5; done
    kill -0 "$RUNPID" 2>/dev/null && { log "run.py ignored TERM for 30 s: KILL"; kill -KILL -- "-$RUNPID" 2>/dev/null || kill -KILL "$RUNPID" 2>/dev/null; }
  fi
  exit $((128 + ${2:-15}))
}
trap 'on_signal TERM 15' TERM
trap 'on_signal INT 2' INT
trap 'on_signal HUP 1' HUP

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

DSCALE="${APPCI_DISPLAY_SCALE:-1}" SCALE2="${APPCI_SECOND_OUTPUT_SCALE:-}"
scale_ok() { awk -v s="$1" 'BEGIN{ exit !(s ~ /^[0-9]+(\.[0-9]+)?$/ && s+0 >= 0.5 && s+0 <= 4) }'; }
scale_ok "$DSCALE" || { log "APPCI_DISPLAY_SCALE=$DSCALE is not a number between 0.5 and 4"; exit 2; }
if [ -n "$SCALE2" ]; then
  scale_ok "$SCALE2" || { log "APPCI_SECOND_OUTPUT_SCALE=$SCALE2 is not a number between 0.5 and 4"; exit 2; }
  [ "$MODE" = x11 ] && { log "APPCI_SECOND_OUTPUT_SCALE needs a Wayland session (sway outputs); the x11 mode has one screen"; exit 2; }
fi
export APPCI_DISPLAY_SCALE="$DSCALE" APPCI_OUTPUTS=1
[ -n "$SCALE2" ] && export APPCI_OUTPUTS=2

start_sway() { # renderer
  local r=$1 i
  rm -f "$RT"/wayland-* "$RT"/sway-ipc.*
  cat > "$OUT/sway.conf" <<CONF
xwayland enable
default_border none
default_floating_border none
focus_follows_mouse no
output HEADLESS-1 resolution ${APPCI_RES:-1600x1000} position 0 0 scale ${DSCALE}
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
  if [ "$DSCALE" != 1 ] && [ "$DSCALE" != 1.0 ]; then
    command -v xrdb >/dev/null || { log "APPCI_DISPLAY_SCALE=$DSCALE needs xrdb (x11-xserver-utils) on this machine; refusing to run at scale 1 under a scaled name"; exit 2; }
    dpi=$(awk -v s="$DSCALE" 'BEGIN{printf "%d", 96*s + 0.5}')
    printf 'Xft.dpi:\t%s\n' "$dpi" | xrdb -merge || { log "xrdb -merge of Xft.dpi=$dpi failed"; exit 2; }
    log "x11 display scale $DSCALE: Xft.dpi=$dpi"
  fi
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
  if [ -n "$SCALE2" ]; then
    # #2078: a second headless output of another scale, right of the first (logical width = pixel width / the first output's scale)
    res=${APPCI_RES:-1600x1000}; w1=${res%x*}
    x2=$(awk -v w="$w1" -v s="$DSCALE" 'BEGIN{printf "%d", w/s + 0.5}')
    before=$(swaymsg -t get_outputs -r 2>/dev/null | grep -o '"name": *"[^"]*"' | sort)
    swaymsg create_output >/dev/null 2>&1 || { log "swaymsg create_output failed: no second output"; exit 2; }
    sleep 0.5
    out2=$(swaymsg -t get_outputs -r 2>/dev/null | grep -o '"name": *"[^"]*"' | sort | comm -13 <(echo "$before") - | head -1 | sed 's/.*: *"\(.*\)"/\1/')
    [ -n "$out2" ] || { log "create_output succeeded but sway lists no new output"; exit 2; }
    swaymsg "output $out2 resolution $res position $x2 0 scale $SCALE2" >/dev/null 2>&1 || { log "could not configure $out2"; exit 2; }
    log "second output $out2 at scale $SCALE2, position $x2,0 (first output scale $DSCALE)"
  fi
  # #1414: ONE virtual keyboard for the whole session. Every wtype call creates (and on exit destroys) its own virtual keyboard, so with
  # none alive the seat's keyboard capability flips 0->2->0 per call and wtype's first key lands before any client has bound the new
  # wl_keyboard (a lost first character; key-only calls like ctrl+c lose the whole key). A long-lived wtype that only sleeps keeps its
  # keyboard (and so the capability) alive; clients bind wl_keyboard once, before any test key. Recorded in $PIDS, killed by exact PID.
  # APPCI_WL_KBD_HOLD=0 restores the old per-call-keyboard behaviour (a harness property, docs/testing.md "Wayland key injection").
  WL_KBD_HOLD=off
  if [ "${APPCI_WL_KBD_HOLD:-1}" != 0 ]; then
    nsl=(); for _ in $(seq 1 40); do nsl+=(-s 1000000); done     # 40 x 1e6 ms (each under int/usleep limits) = 11 h
    setsid wtype "${nsl[@]}" > "$OUT/kbd-holder.log" 2>&1 &
    KBDPID=$!
    echo "kbd-holder $KBDPID" >> "$PIDS"
    sleep 1
    if kill -0 "$KBDPID" 2>/dev/null; then
      WL_KBD_HOLD=persistent
      kb=$(swaymsg -t get_inputs 2>/dev/null | grep -c '"type": *"keyboard"')
      log "persistent virtual keyboard held by wtype pid $KBDPID (sway sees $kb keyboard input(s))"
    else
      WL_KBD_HOLD=percall
      log "WARNING: the keyboard holder exited ($(head -c 200 "$OUT/kbd-holder.log")); falling back to a keyboard per wtype call"
    fi
  else
    WL_KBD_HOLD=percall
    log "APPCI_WL_KBD_HOLD=0: a virtual keyboard per wtype call (old behaviour)"
  fi
  export APPCI_WL_INJECTOR=$WL_KBD_HOLD
  # #1789: ONE virtual pointer for the whole session, created BEFORE the app starts. Headless sway has no input device, so its seat has no pointer
  # capability and no client ever creates a wl_pointer (`swaymsg seat - cursor` moves sway's own cursor only). appci/wlptr.py speaks
  # zwlr_virtual_pointer_v1 in plain Python (no install); the harness drives it over $RT/wlptr.sock (short path, socket limit 108 bytes).
  # APPCI_WL_PTR_HOLD=0 skips it (the pointer cases then report NOREF on Wayland as before). Recorded in $PIDS, killed by exact PID.
  if [ "${APPCI_WL_PTR_HOLD:-1}" != 0 ]; then
    setsid python3 "$APPCI/wlptr.py" serve --ctl "$RT/wlptr.sock" > "$OUT/ptr-holder.log" 2>&1 &
    PTRPID=$!
    echo "ptr-holder $PTRPID" >> "$PIDS"
    for _ in $(seq 1 20); do [ -S "$RT/wlptr.sock" ] && break; kill -0 "$PTRPID" 2>/dev/null || break; sleep 0.25; done
    if [ -S "$RT/wlptr.sock" ] && python3 "$APPCI/wlptr.py" cmd --ctl "$RT/wlptr.sock" ping >/dev/null 2>&1; then
      export APPCI_WL_PTR="$RT/wlptr.sock"
      log "persistent virtual pointer held by wlptr.py pid $PTRPID (sway sees $(swaymsg -t get_inputs 2>/dev/null | grep -c '"type": *"pointer"') pointer input(s))"
    else
      log "WARNING: the virtual-pointer holder did not start ($(head -c 300 "$OUT/ptr-holder.log")); pointer cases report NOREF on Wayland"
    fi
  else
    log "APPCI_WL_PTR_HOLD=0: no virtual pointer (pointer cases report NOREF on Wayland)"
  fi
  XD=$(cat "$OUT/xdisplay" 2>/dev/null | tr -d '[:space:]'); [ -n "$XD" ] && export DISPLAY="$XD"
  BACKEND=wayland APPDISP=wayland; [ "$MODE" = xwayland ] && APPDISP=x11
  [ "$MODE" = xwayland ] && [ -z "${DISPLAY:-}" ] && { log "XWayland DISPLAY was not published by sway"; exit 2; }
fi
if [ "$MODE" != x11 ] && [ -z "${DISPLAY:-}" ] && [ -d /tmp/.X11-unix ] && [ "$(stat -c %u /tmp/.X11-unix 2>/dev/null)" != 0 ] && [ "$(stat -c %u /tmp/.X11-unix 2>/dev/null)" != "$(id -u)" ]; then
  # #1567: XWayland refuses a socket dir that is not root-owned. Not changed here (box environment, owner only): no DISPLAY means no xterm reference unless foot is installed.
  log "WARNING /tmp/.X11-unix is owned by uid $(stat -c %u /tmp/.X11-unix), not root: XWayland cannot start, so no xterm reference (latency rows become NOREF unless foot is installed). Owner fix: sudo chown root: /tmp/.X11-unix; sudo chmod 1777 /tmp/.X11-unix"
fi
log "session up: renderer=$RENDERER WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-} DISPLAY=${DISPLAY:-} app-display=$APPDISP"
{ echo "== uname"; uname -srm; echo "== dri"; ls -l /dev/dri 2>&1; echo "== vulkan"; vulkaninfo --summary 2>&1 | head -20; } > "$OUT/gpu.txt" 2>&1

RUNDIR="$OUT/run"; mkdir -p "$RUNDIR"
# r15 (T43 installer_smoke): where the build's artefacts are (.deb / .AppImage), for the case that installs one itself; unset with --bin alone
[ -n "$DIST" ] && [ -d "$DIST" ] && export APPCI_DIST="$(cd "$DIST" && pwd)"
# r18d8: run.py in its OWN process group (setsid; in a non-interactive shell the background child is not a group leader, so setsid execs
# in place and $! is the pgid), recorded in $PIDS so the teardown and the next job's stale-reaper find it. `wait` is interruptible by the traps.
setsid python3 "$APPCI/run.py" --run "$RUNDIR" --bin "$BIN" --backend "$BACKEND" --app-display "$APPDISP" --modes "$SMODES" \
  --cases "$CASES" --skip "$SKIP" --platform "linux-$MODE" &
RUNPID=$!
echo "runpy $RUNPID" >> "$PIDS"
wait "$RUNPID"; RC=$?
cp "$OUT/sway."*.log "$OUT/gpu.txt" "$RUNDIR/logs/" 2>/dev/null
log "done rc=$RC; results in $RUNDIR/results.json"
exit $RC
