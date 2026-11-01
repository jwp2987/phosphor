#!/bin/bash
# realapp/linux/torture: seeded expert-typist fuzzer and XTest key sequencer (kseq.c, rec.c) with exact-received-command verification, run against Phosphor and a reference terminal.
# Platform: Linux X11. Prereqs: cc -O2 kseq.c rec.c -lX11 -lXtst; PHOSPHOR_DEB_DIR extracted deb; config.example.sh sourced.
# Run: run.sh / all.sh (scenarios in scen.py); lib.sh builds the scratch profile (owner-config copy, agent mode, LAN provider).
# Restores: scratch profile only; the owner's config is only read (copied) and never printed.
# all.sh <binary> [label]  : the whole torture-suggest suite (≈45 min).  Output: work/all-<label>.log
HERE="$(cd "$(dirname "$0")" && pwd)"; B="$(readlink -f "$1")"; L=${2:-run}
{
 echo "### bin=$B  $(date)"
 "$HERE/run.sh" $B local 11 300 bare-local-s11
 "$HERE/run.sh" $B xterm 11 300 bare-xterm-s11
 OWNER=1 "$HERE/run.sh" $B local 12 300 owner-local-s12
 OWNER=1 "$HERE/run.sh" $B xterm 12 300 owner-xterm-s12
 LABEL=$L "$HERE/scen.sh" $B all 12
 BIGHIST=1 LABEL=${L}-bighist "$HERE/scen.sh" $B enter 30
} 2>&1 | grep -v '^\[[0-9]\]' > "$HERE/work/all-$L.log"
