#!/bin/bash
# realapp/linux/torture: seeded expert-typist fuzzer and XTest key sequencer (kseq.c, rec.c) with exact-received-command verification, run against Phosphor and a reference terminal.
# Platform: Linux X11. Prereqs: cc -O2 kseq.c rec.c -lX11 -lXtst; PHOSPHOR_DEB_DIR extracted deb; config.example.sh sourced.
# Run: run.sh / all.sh (scenarios in scen.py); lib.sh builds the scratch profile (owner-config copy, agent mode, LAN provider).
# Restores: scratch profile only; the owner's config is only read (copied) and never printed.
# probe.sh <tokens...> : send tokens then Return, print the shell-received line
source "$(dirname $0)/lib.sh"
n0=$(grep -c '^H ' $LOG)
$KSEQ "$@" d30 '<Return>' >/dev/null
for i in $(seq 1 60); do [ "$(grep -c '^H ' $LOG)" -gt "$n0" ] && break; sleep 0.1; done
sleep 0.2; tail -1 $LOG | sed 's/^ *[0-9]* *//'
