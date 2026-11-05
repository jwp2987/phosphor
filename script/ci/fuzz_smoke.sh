#!/usr/bin/env bash
# script/ci/fuzz_smoke.sh: runs every cargo-fuzz target for the fuzz-smoke jobs (box: .gitlab-ci.yml `fuzz-smoke`; Mac:
# script/ci/run_suite.sh macos fuzz-smoke) with libFuzzer's output kept OUT of the job trace.
#
# Why: libFuzzer prints a progress line per new input and a pulse line per doubling, for each target, plus the cargo build.
# Eight targets of 180 s overran GitLab's 4 MB trace limit (r18u23 job 17761810047): the trace was cut before the crash, so
# the panic message of the one failing target was lost. Now each target's full output (build + libFuzzer, stdout and stderr)
# goes to fuzz/logs/<target>.log, kept as a job artifact, and the trace gets per target: one start line, the final stats
# (`-print_final_stats=1`), and on a failure the crash text (the panic message, libFuzzer's ERROR/SUMMARY lines, the path of
# the crash input) plus the last lines of the log. A crash's panic message always reaches the trace.
#
# Every target runs even after one fails (one crash no longer hides the next target's), and the script exits non-zero if
# any failed, naming them.
#
# Usage: script/ci/fuzz_smoke.sh [extra cargo-fuzz run args, e.g. --target aarch64-apple-darwin]
# Env:   FUZZ_SECONDS (default 180), FUZZ_TARGETS (default: all eight), FUZZ_LOG_DIR (default fuzz/logs),
#        CARGO_TARGET_DIR (as the caller sets it). Runs from the repo root; expects fuzz/Cargo.lock to be in place.
set -uo pipefail
cd "$(dirname "$0")/../.."

targets="${FUZZ_TARGETS:-ansi_parser hook_parsers input_queue key_facts_cell markdown_parser mermaid_render route_decide typeahead_handoff}"
seconds="${FUZZ_SECONDS:-180}"
log_dir="${FUZZ_LOG_DIR:-fuzz/logs}"
mkdir -p "$log_dir"

# The lines of a log that say why a target failed: a Rust panic and the lines after it (the message, which may span lines),
# libFuzzer's / a sanitizer's error and summary, and where the crash input was written. Capped so a panic loop cannot flood
# the trace.
crash_text() {
  grep -n -E -A12 'panicked at|ERROR: libFuzzer|ERROR: AddressSanitizer|ERROR: LeakSanitizer|SUMMARY: |Test unit written to|^error(\[E[0-9]+\])?:' "$1" | head -n 120
}

failed=()
for target in $targets; do
  log="$log_dir/$target.log"
  echo "== fuzz $target: ${seconds}s, full output in $log"
  st=0
  ./script/agent-cargo ci-fuzz fuzz run "$@" --fuzz-dir fuzz "$target" -- \
    -max_total_time="$seconds" -timeout=20 -print_final_stats=1 >"$log" 2>&1 || st=$?
  # The final stats (stat::number_of_executed_units, peak RSS, ...) and libFuzzer's last "Done" line, on success or not.
  grep -E '^stat::|^Done [0-9]+ runs' "$log" | sed "s/^/   [$target] /"
  if [[ $st -ne 0 ]]; then
    failed+=("$target")
    echo "!! fuzz $target FAILED (exit $st). Crash text:"
    crash_text "$log" | sed "s/^/   [$target] /"
    echo "!! last 40 lines of $log:"
    tail -n 40 "$log" | sed "s/^/   [$target] /"
  fi
done

if [[ ${#failed[@]} -gt 0 ]]; then
  echo "fuzz-smoke: ${#failed[@]} target(s) failed: ${failed[*]} (logs in $log_dir/, crash inputs in fuzz/artifacts/)" >&2
  exit 1
fi
echo "fuzz-smoke: every target ran ${seconds}s with no crash"
