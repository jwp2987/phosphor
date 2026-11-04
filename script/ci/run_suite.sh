#!/usr/bin/env bash
# script/ci/run_suite.sh <macos|windows> <suite>: the Linux box's test suites on the Mac and Windows runners.
#
# Owner decision 2026-11-03: "the tests need to be the same on all boxes". Each suite below is the same command as the
# box job of the same name in .gitlab-ci.yml (test-linux, test-warpui-core, test-workspace-rest, test-shuttle-loom,
# fuzz-smoke, bench, mutants, mutants-warpui-core); the *-macos / *-windows jobs call this script so the command lives in one
# place for both platforms. On Windows it runs under Git for Windows' bash (the PowerShell job calls bash.exe).
#
# Suites: test | warpui-core | workspace-rest | shuttle-loom | fuzz-smoke | bench | mutants | mutants-warpui-core
#
# Tools: only into the CI user's OWN home (cargo-binstall and the crates it installs go to $CARGO_HOME/bin = ~/.cargo/bin),
# installed when missing, never system-wide, never sudo. Anything that cannot be installed that way fails the job with the
# reason. cargo-fuzz does not exist on Windows (see fuzz-smoke below): that job fails loudly, it does not skip.
# Env: TEST_FILTER (nextest filterset, test suite only), FUZZ_SECONDS, MUTANTS_* (as the box jobs), PHOSPHOR_PYTHON (python.exe on Windows).
set -euo pipefail
cd "$(dirname "$0")/../.."
plat="${1:?usage: script/ci/run_suite.sh macos|windows <suite>}"
suite="${2:?usage: script/ci/run_suite.sh macos|windows <suite>}"

case "$plat" in
  macos)   export PATH="/opt/homebrew/bin:/usr/local/bin:$HOME/.local/bin:$HOME/.cargo/bin:$PATH"; jobs_default=4 ;;   # 16 GB Mac, as build-macos-arm64
  windows) export PATH="$HOME/.cargo/bin:$PATH"; jobs_default=8 ;;                                                     # as build-windows-x86_64
  *) echo "run_suite.sh: unknown platform '$plat'" >&2; exit 2 ;;
esac
# One job at a time on these machines: no slot arbitration (script/agent-cargo needs /proc/meminfo and flock). The caps still apply.
export PHOSPHOR_GOVERNOR=0
export PHOSPHOR_BUILD_JOBS="${PHOSPHOR_BUILD_JOBS:-$jobs_default}"
export KNOWN_FAILURES_FILE="script/known_test_failures_$plat.txt"
export PHOSPHOR_KNOWN_FAILURES="$KNOWN_FAILURES_FILE"
PY="${PHOSPHOR_PYTHON:-python3}"
command -v "$PY" >/dev/null || { echo "ERROR python ($PY) is missing on this runner; install it by hand (not done automatically)" >&2; exit 1; }

for t in cargo rustc; do
  command -v "$t" >/dev/null || { echo "ERROR $t is missing on this runner; install it by hand (not done automatically)" >&2; exit 1; }
done
[[ "$plat" == macos ]] && { command -v protoc >/dev/null || { echo "ERROR protoc is missing on this runner; install it by hand (not done automatically)" >&2; exit 1; }; }

# Install a cargo tool into the CI user's own ~/.cargo/bin when missing (pinned cargo-binstall: script/install_cargo_binstall).
need_tool() {   # need_tool <binary> <crate>
  command -v "$1" >/dev/null && return 0
  echo "installing $2 into $HOME/.cargo/bin (this user only)"
  ./script/install_cargo_binstall
  cargo binstall --force -y "$2"
  command -v "$1" >/dev/null || { echo "ERROR $1 is still missing after installing $2" >&2; exit 1; }
}

case "$suite" in
  test)
    need_tool cargo-nextest cargo-nextest
    # == test-linux: BOTH crates in ONE invocation (a listed failure that did not run counts as "fixed"). Default nextest profile.
    set -o pipefail
    st=0
    ./script/check_test_failures -p warp -p warp_tui --lib --features warp/gui ${TEST_FILTER:+-E "$TEST_FILTER"} 2>&1 | tee "nextest-$plat.log" || st=$?
    if [[ -n "${TEST_FILTER:-}" ]]; then "$PY" script/ci/check_filter_terms.py "$TEST_FILTER" "nextest-$plat.log" || st=$(( st ? st : 1 )); fi
    exit $st
    ;;
  warpui-core)
    need_tool cargo-nextest cargo-nextest
    # == test-warpui-core: --features tui is not optional (elements/tui/ is cfg(feature = "tui")).
    ./script/check_test_failures -p warpui_core --features tui ${TEST_FILTER:+-E "$TEST_FILTER"} 2>&1 | tee "nextest-warpui-core-$plat.log"
    exit "${PIPESTATUS[0]}"
    ;;
  workspace-rest)
    need_tool cargo-nextest cargo-nextest
    # == test-workspace-rest: one invocation listing every package (do not split).
    ./script/check_test_failures \
        -p sum_tree -p string-offset -p fuzzy_match -p handlebars -p syntax_tree \
        -p languages -p asset_cache -p settings_value -p jsonrpc -p simple_logger \
        -p warp_logging -p channel_versions -p command -p http_client -p warp_features \
        -p voice_input -p warp_isolation_platform -p websocket \
        -p input_classifier -p warp_search_core -p persistence -p settings \
        -p warp_managed_secrets -p markdown_parser -p warp_util -p warp_core \
        -p repo_metadata -p warp_terminal -p warp_files -p local_control -p vim \
        -p warp_completer -p warpui -p warpui_extras -p ai -p remote_server \
        -p usage_suite -p warp_editor -p warp_cli ${TEST_FILTER:+-E "$TEST_FILTER"} 2>&1 | tee "nextest-rest-$plat.log"
    exit "${PIPESTATUS[0]}"
    ;;
  shuttle-loom)
    # == test-shuttle-loom. RUSTFLAGS replaces .cargo/config.toml's flags (fine for tests). One target dir per cfg: --cfg rebuilds every crate.
    CARGO_TARGET_DIR="target/ci-shuttle" RUSTFLAGS="--cfg shuttle" ./script/agent-cargo ci-shuttle test -p warpui_core --lib --release -- model_tests::
    CARGO_TARGET_DIR="target/ci-shuttle" RUSTFLAGS="--cfg shuttle" ./script/agent-cargo ci-shuttle test -p warp --features gui --lib --release -- model_tests::
    CARGO_TARGET_DIR="target/ci-phosphor_loom" RUSTFLAGS="--cfg phosphor_loom" ./script/agent-cargo ci-phosphor_loom test -p warpui_core --lib --release -- model_tests::
    CARGO_TARGET_DIR="target/ci-phosphor_loom" RUSTFLAGS="--cfg phosphor_loom" ./script/agent-cargo ci-phosphor_loom test -p warp --features gui --lib --release -- model_tests::
    ;;
  fuzz-smoke)
    if [[ "$plat" == windows ]]; then
      echo "ERROR fuzz-smoke cannot run on Windows: cargo-fuzz works only on x86-64 and Aarch64 and only on Unix-like systems" >&2
      echo "       (not Windows; https://github.com/rust-fuzz/cargo-fuzz README). Its libFuzzer + sanitizer build has no MSVC path." >&2
      echo "       This is a platform limit, not a pass: the Linux and macOS fuzz-smoke jobs cover these targets." >&2
      exit 1
    fi
    # == fuzz-smoke: the box uses RUSTC_BOOTSTRAP=1 on the pinned stable toolchain (no nightly is installed anywhere).
    export RUSTC_BOOTSTRAP=1
    need_tool cargo-fuzz cargo-fuzz
    cp Cargo.lock fuzz/Cargo.lock
    for target in ansi_parser hook_parsers input_queue key_facts_cell markdown_parser mermaid_render route_decide typeahead_handoff; do
      CARGO_TARGET_DIR="target/ci-fuzz" ./script/agent-cargo ci-fuzz fuzz run --fuzz-dir fuzz "$target" -- -max_total_time="${FUZZ_SECONDS:-180}" -timeout=20
    done
    ;;
  bench)
    # == bench: criterion benches, then the order-of-magnitude gate.
    CARGO_TARGET_DIR="target/ci-bench" ./script/agent-cargo ci-bench bench -p warp --features dev-support --bench key_path_bench --bench ansi_bench --bench routing_bench
    mkdir -p bench-results
    cp -r target/ci-bench/criterion bench-results/ 2>/dev/null || { echo "no criterion output found under target/ci-bench" >&2; exit 1; }
    "$PY" script/bench_gate.py target/ci-bench/criterion
    ;;
  mutants)
    need_tool cargo-mutants cargo-mutants
    FILE_ARGS=()
    for f in ${MUTANTS_FILES:?MUTANTS_FILES is not set}; do FILE_ARGS+=(--file "$f"); done
    st=0
    CARGO_TARGET_DIR="target/ci-mutants" ./script/agent-cargo ci-mutants mutants -p "${MUTANTS_PACKAGE:-warp}" --no-shuffle "${FILE_ARGS[@]}" \
        --cargo-test-arg=-p --cargo-test-arg=warp_tui --cargo-test-arg=--lib --cargo-test-arg=--features --cargo-test-arg=warp/gui \
        ${MUTANTS_SHARD:+--shard "$MUTANTS_SHARD"} ${MUTANTS_TEST_FILTER:+--cargo-test-arg=-E --cargo-test-arg="$MUTANTS_TEST_FILTER"} || st=$?
    [[ $st -eq 0 ]] || echo "cargo-mutants exited $st (missed mutants are reported in mutants.out/)"
    ;;
  mutants-warpui-core)
    need_tool cargo-mutants cargo-mutants
    st=0
    CARGO_TARGET_DIR="target/ci-mutants-core" ./script/agent-cargo ci-mutants-core mutants -p warpui_core --no-shuffle --cargo-test-arg=--features --cargo-test-arg=tui \
        --file crates/warpui_core/src/clipboard.rs --file crates/warpui_core/src/clipboard_utils.rs || st=$?
    [[ $st -eq 0 ]] || echo "cargo-mutants exited $st (missed mutants are reported in mutants.out/)"
    ;;
  *) echo "run_suite.sh: unknown suite '$suite'" >&2; exit 2 ;;
esac
