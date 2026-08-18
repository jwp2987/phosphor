#!/usr/bin/env bash
set -uo pipefail
# Reproduce generate_repin_queue:209 exactly: a big captured blob piped into grep -q.
big="$(head -c 400000 /dev/zero | tr '\0' 'x')"
diff_text="MARKER_AT_TOP
$big"
for i in 1 2 3 4 5; do
  if printf '%s' "$diff_text" | grep -qF "MARKER_AT_TOP"; then
    echo "run $i: MATCHED (correct)"
  else
    echo "run $i: NOT MATCHED (WRONG) -- pipeline status $?"
  fi
done
echo "--- raw statuses ---"
for i in 1 2 3; do
  printf '%s' "$diff_text" | grep -qF "MARKER_AT_TOP"; echo "status=$? pipestatus=${PIPESTATUS[*]}"
done
