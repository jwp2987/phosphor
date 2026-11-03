#!/bin/bash
# realapp/linux/torture: seeded expert-typist fuzzer and XTest key sequencer (kseq.c, rec.c) with exact-received-command verification, run against Phosphor and a reference terminal.
# Platform: Linux X11. Prereqs: cc -O2 kseq.c rec.c -lX11 -lXtst; PHOSPHOR_DEB_DIR extracted deb; config.example.sh sourced.
# Run: run.sh / all.sh (scenarios in scen.py); lib.sh builds the scratch profile (owner-config copy, agent mode, LAN provider).
# Restores: scratch profile only; the owner's config is only read (copied) and never printed.
# scen.sh <binary> <group|all> [reps]   (groups: ghost tab interleave hist enter)  fresh app per group; OWNER=1/AI=1 as in lib.sh
HERE="$(cd "$(dirname "$0")" && pwd)"
export BIN="$(readlink -f "$1")"; G=${2:-ghost}; REPS=${3:-12}
export D=${D:-:97}; export T=${T:-$HERE/work}; source $HERE/lib.sh
ensure_xvfb
for g in $( [ "$G" = all ] && echo "ghost tab interleave hist enter" || echo $G ); do
  stop_app 2>/dev/null
  HL=0; [ "$g" = enter ] && [ -n "$BIGHIST" ] && HL=100000
  setup_profile $HL; launch_app; M=$(cat $P/mainpid)
  echo "##### group=$g bin=$BIN owner=${OWNER:-0} histlines=$HL"
  KSEQ=$KSEQ REC_LOG=$LOG PAINT=268,712,700,26 PID=$M OUT=$T/scen-${LABEL:-x}-$g python3 $HERE/scen.py $g $REPS
  stop_app
done
