#!/bin/bash
# realapp/linux/torture: seeded expert-typist fuzzer and XTest key sequencer (kseq.c, rec.c) with exact-received-command verification, run against Phosphor and a reference terminal.
# Platform: Linux X11. Prereqs: cc -O2 kseq.c rec.c -lX11 -lXtst; PHOSPHOR_DEB_DIR extracted deb; config.example.sh sourced.
# Run: run.sh / all.sh (scenarios in scen.py); lib.sh builds the scratch profile (owner-config copy, agent mode, LAN provider).
# Restores: scratch profile only; the owner's config is only read (copied) and never printed.
# run.sh <binary> [ctx=local|xterm|konsole] [seed=1] [secs=300] [label]
# Seeded expert-typist fuzzer against a fresh Phosphor (or xterm/konsole reference) on private Xvfb :97.
# Results: work/fz-<label>/*.jsonl + stdout summary.  Re-run on another binary with the same seed to compare.
HERE="$(cd "$(dirname "$0")" && pwd)"
export BIN="$(readlink -f "$1")"; CTX=${2:-local}; SEED=${3:-1}; SECS=${4:-300}; LABEL=${5:-$CTX-s$SEED}
export D=${D:-:97}; export T=${T:-$HERE/work}; source $HERE/lib.sh
ensure_xvfb
stop_app 2>/dev/null
setup_profile ${HISTLINES:-0}
case $CTX in
 local) launch_app; PAINT=268,712,700,26; [ "${OWNER:-0}" = 1 ] && PAINT=268,900,700,28; EXTRA=; ;;
 xterm) launch_xterm; PAINT=; EXTRA="--noghost --wordmode ws"; ;;
 konsole) launch_konsole; PAINT=; EXTRA="--noghost --wordmode ws"; ;;
esac
M=$(cat $P/mainpid)
echo "[$LABEL] ctx=$CTX app pid=$M bin=$BIN seed=$SEED secs=$SECS"
KSEQ=$KSEQ REC_LOG=$LOG PAINT=$PAINT PID=$M python3 $HERE/fuzz.py --seed $SEED --duration $SECS --hist $H/.bash_history --out $T/fz-$LABEL --label $LABEL $EXTRA ${FUZZ_ARGS}
# app CPU over the run is in the jsonl (cpu_ticks, 100/s); also report RSS
echo "[$LABEL] main RSS kB: $(awk '/VmRSS/{print $2}' /proc/$M/status 2>/dev/null)"
stop_app
