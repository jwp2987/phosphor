#!/usr/bin/env bash
# Counts a process's threads by name (Linux). Usage: script/thread_census.sh <pid> [--watch SECONDS]
#
# Prints "<count> <thread name>" sorted by count, then the total. Run it on an idle app, again
# after opening an editor, and compare builds with PHOSPHOR_RAYON_THREADS / PHOSPHOR_SHARED_WATCHER
# set to 0 against the defaults. See docs/testing.md, "Thread census".
set -euo pipefail

usage() {
    echo "usage: $0 <pid> [--watch SECONDS]" >&2
    exit 2
}

[ $# -ge 1 ] || usage
pid="$1"
[[ "$pid" =~ ^[0-9]+$ ]] || usage
[ -d "/proc/$pid" ] || {
    echo "no such process: $pid" >&2
    exit 1
}

census() {
    # comm is the 15-byte thread name; numbered workers (warp-rayon-3) are folded to "name-N".
    ps -T -p "$pid" -o comm= |
        sed -E 's/[-_ ]?[0-9]+$/-N/' |
        sort | uniq -c | sort -rn
    echo "$(ps -T -p "$pid" -o tid= | wc -l) total"
}

if [ "${2:-}" = "--watch" ]; then
    [[ "${3:-}" =~ ^[0-9]+$ ]] || usage
    while [ -d "/proc/$pid" ]; do
        date +%T
        census
        echo
        sleep "$3"
    done
else
    census
fi
