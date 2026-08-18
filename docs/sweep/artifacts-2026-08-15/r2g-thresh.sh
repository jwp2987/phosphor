#!/usr/bin/env bash
set -uo pipefail
for n in 4096 32768 65536 70000 131072 262144; do
  big="$(head -c $n /dev/zero | tr '\0' 'x')"
  d="MARKER
$big"
  printf '%s' "$d" | grep -qF MARKER; s=$?
  # also: marker at the END (no early exit possible)
  d2="$big
MARKER"
  printf '%s' "$d2" | grep -qF MARKER; s2=$?
  echo "size=$n  marker-at-top status=$s   marker-at-end status=$s2"
done
