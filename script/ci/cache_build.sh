#!/usr/bin/env bash
# script/ci/cache_build.sh <platform>: last step of a SUCCESSFUL build-* job (linux, macos). Copies ./dist (with SHA256SUMS) into the
# machine-local build cache so the app-test-*-reuse job, which runs on the SAME machine, reuses it without any download:
#   $HOME/.cache/phosphor-ci/builds/<platform>/<CI_JOB_ID>/{dist files, meta.json}
# $HOME is the runner user's home, outside the job's build dir, so `git clean` / "Removing dist/" never touches it.
# Atomic (copy to a tmp dir, then rename); keeps only the newest 3 builds per platform.
set -eu
plat="${1:?platform}"; : "${CI_JOB_ID:?}"
root="${PHOSPHOR_CI_BUILD_CACHE:-$HOME/.cache/phosphor-ci/builds}/$plat"
[ -f dist/SHA256SUMS ] || { echo "cache_build: ERROR dist/SHA256SUMS missing; not caching" >&2; exit 1; }
mkdir -p "$root"; tmp="$root/.tmp-$CI_JOB_ID"; rm -rf "$tmp"; mkdir "$tmp"
cp -R dist/. "$tmp/"
printf '{"job_id": %s, "pipeline_id": "%s", "sha": "%s", "ref": "%s", "created": "%s"}\n' \
  "$CI_JOB_ID" "${CI_PIPELINE_ID:-}" "${CI_COMMIT_SHA:-}" "${CI_COMMIT_REF_NAME:-}" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$tmp/meta.json"
rm -rf "$root/$CI_JOB_ID"; mv "$tmp" "$root/$CI_JOB_ID"
# retention: newest 3 job ids (numeric order, newest first), stale .tmp-* leftovers go too
ls -1 "$root" | grep -E '^[0-9]+$' | sort -rn | tail -n +4 | while read -r old; do rm -rf "${root:?}/$old"; done
find "$root" -maxdepth 1 -name '.tmp-*' -mmin +120 -exec rm -rf {} + 2>/dev/null || true
echo "cache_build: cached dist/ of job $CI_JOB_ID at $root/$CI_JOB_ID (kept: $(ls -1 "$root" | grep -E '^[0-9]+$' | tr '\n' ' '))"
