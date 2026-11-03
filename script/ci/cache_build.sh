#!/usr/bin/env bash
# script/ci/cache_build.sh <platform>: last step of a SUCCESSFUL build-* job (linux, macos). Copies ./dist (with SHA256SUMS) into the
# machine-local build cache so the app-test-* jobs, which run on the SAME machine, use it without any download:
#   $HOME/.cache/phosphor-ci/builds/<platform>/<CI_JOB_ID>/{dist files, meta.json}
#   $HOME/.cache/phosphor-ci/builds/<platform>/pipeline-<platform>-<CI_PIPELINE_ID>   (holds <CI_JOB_ID>: the same-pipeline app test's lookup;
#   keyed by platform AND pipeline, written atomically under a lock, #1524; fetch_build.sh still reads the old pipeline-<id> name as a fallback)
# $HOME is the runner user's home, outside the job's build dir, so `git clean` / "Removing dist/" never touches it.
# Atomic (copy to a tmp dir, then rename); keeps only the newest 3 builds per platform.
set -eu
plat="${1:?platform}"; : "${CI_JOB_ID:?}"
root="${PHOSPHOR_CI_BUILD_CACHE:-$HOME/.cache/phosphor-ci/builds}/$plat"
[ -f dist/SHA256SUMS ] || { echo "cache_build: ERROR dist/SHA256SUMS missing; not caching" >&2; exit 1; }
mkdir -p "$root"
# One writer at a time per platform (two builds finishing together, a retried job): mkdir is the portable lock (macOS has no flock).
# A lock older than 10 minutes belongs to a dead job and is taken over.
lock="$root/.lock"
for _ in $(seq 1 120); do
  if mkdir "$lock" 2>/dev/null; then break; fi
  [ -n "$(find "$lock" -maxdepth 0 -mmin +10 2>/dev/null)" ] && { rm -rf "$lock"; continue; }
  sleep 1
done
[ -d "$lock" ] && [ -z "$(find "$lock" -maxdepth 0 -mmin +10 2>/dev/null)" ] || { echo "cache_build: ERROR could not take $lock" >&2; exit 1; }
trap 'rmdir "$lock" 2>/dev/null || true' EXIT
tmp="$root/.tmp-$CI_JOB_ID"; rm -rf "$tmp"; mkdir "$tmp"
cp -R dist/. "$tmp/"
printf '{"job_id": %s, "pipeline_id": "%s", "sha": "%s", "ref": "%s", "created": "%s"}\n' \
  "$CI_JOB_ID" "${CI_PIPELINE_ID:-}" "${CI_COMMIT_SHA:-}" "${CI_COMMIT_REF_NAME:-}" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$tmp/meta.json"
rm -rf "$root/$CI_JOB_ID"; mv "$tmp" "$root/$CI_JOB_ID"
if [ -n "${CI_PIPELINE_ID:-}" ]; then   # write-then-rename: a reader never sees an empty or half-written pointer
  echo "$CI_JOB_ID" > "$root/.ptr-$CI_JOB_ID" && mv -f "$root/.ptr-$CI_JOB_ID" "$root/pipeline-$plat-$CI_PIPELINE_ID"
fi
# retention: newest 3 job ids (numeric order, newest first), stale .tmp-* leftovers go too
ls -1 "$root" | grep -E '^[0-9]+$' | sort -rn | tail -n +4 | while read -r old; do rm -rf "${root:?}/$old"; done
find "$root" -maxdepth 1 \( -name '.tmp-*' -o -name '.ptr-*' \) -mmin +120 -exec rm -rf {} + 2>/dev/null || true
for p in "$root"/pipeline-*; do [ -f "$p" ] && [ ! -d "$root/$(cat "$p")" ] && rm -f "$p"; done; true
echo "cache_build: cached dist/ of job $CI_JOB_ID at $root/$CI_JOB_ID (kept: $(ls -1 "$root" | grep -E '^[0-9]+$' | tr '\n' ' '))"
