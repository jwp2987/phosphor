#!/usr/bin/env bash
# script/ci/fetch_build.sh: app-test-*-reuse jobs. Put the dist/ of an EARLIER build job (same project, any pipeline) into ./dist
# instead of rebuilding. FIRST choice: the copy the build job left in this machine's cache ($HOME/.cache/phosphor-ci/builds/
# <platform>/<job id>/, script/ci/cache_build.sh); each platform's app test runs on the machine that built it, so no download.
# Only if that copy is missing does it fall back to downloading the GitLab artifacts (below), and it says so loudly. Inputs (play-time variables from script/ci/ci_seq.py):
#   APPCI_BUILD_JOB_ID (required), APPCI_BUILD_SHA (required, verified), APPCI_BUILD_PIPELINE_ID, APPCI_BUILD_JOB_NAME, APPCI_BUILD_REF.
# IDENTITY (#1501): a cached copy's meta.json (job id, sha) must equal what ci_seq asked for; a downloaded build is checked against GitLab's own
# record of the job (script/ci/verify_build_identity.py, needs read_api: CI_JOB_TOKEN or APPCI_API_TOKEN). A mismatch or an unverifiable build
# is exit 2; reused_build.json only ever carries a verified sha.
# Fails loudly (exit 2) on a missing variable, a failed fetch or a zip without dist/: it never tests "something else".
# Auth: CI_JOB_TOKEN (JOB-TOKEN header, same project). If GitLab refuses it and APPCI_API_TOKEN is set, that PRIVATE-TOKEN is tried.
#
# `fetch_build.sh --this-pipeline` (plain app-test-* jobs): copy THIS pipeline's build, which the build job just made on this
# machine (cache_build.sh leaves a pipeline-<platform>-<id> pointer), from the local cache. No download: the job's `needs` has
# artifacts: false. A missing copy fails loudly (exit 2); it never tests another build.
set -eu
die() { echo "ERROR fetch_build: $*" >&2; exit 2; }
case "$(uname -s)" in Darwin) PLAT=macos;; *) PLAT=linux;; esac
ROOT="${PHOSPHOR_CI_BUILD_CACHE:-$HOME/.cache/phosphor-ci/builds}/$PLAT"
if [ "${1:-}" = --this-pipeline ]; then
  ptr="$ROOT/pipeline-$PLAT-$CI_PIPELINE_ID"
  [ -f "$ptr" ] || ptr="$ROOT/pipeline-$CI_PIPELINE_ID"   # pointer name before #1524
  [ -f "$ptr" ] || die "no local build for pipeline $CI_PIPELINE_ID ($ptr missing): the build job did not run on this machine or cache_build.sh failed. Re-run the build job."
  id=$(cat "$ptr"); src="$ROOT/$id"
  [ -f "$src/SHA256SUMS" ] || die "local build $src is missing or pruned; re-run the build job"
  echo "fetch_build: using this pipeline's build job $id from this machine: $src (no download)"
  python3 "$(dirname "$0")/verify_build_identity.py" meta-pipeline "$src" || die "the local build is not this pipeline's build (identity check above)"
  rm -rf dist; mkdir dist; cp -R "$src"/. dist/; rm -f dist/meta.json
  (cd dist && { sha256sum -c SHA256SUMS 2>/dev/null || shasum -a 256 -c SHA256SUMS; }) || die "dist/SHA256SUMS check failed"
  exit 0
fi
: "${APPCI_BUILD_JOB_ID:?}" 2>/dev/null || die "APPCI_BUILD_JOB_ID is not set. This *-reuse job must be played with the build job id (script/ci/ci_seq.py does that); to test a fresh build play the plain app-test-* job instead."
case "$APPCI_BUILD_JOB_ID" in ''|*[!0-9]*) die "APPCI_BUILD_JOB_ID='$APPCI_BUILD_JOB_ID' is not a job id";; esac
LOCAL="$ROOT/$APPCI_BUILD_JOB_ID"
write_record() {
  # verify the files FIRST, then record their hashes: results.json (record_reuse.py) names the exact binaries tested (#1501)
  (cd dist && ls -l && { sha256sum -c SHA256SUMS 2>/dev/null || shasum -a 256 -c SHA256SUMS; }) || die "dist/SHA256SUMS check failed"
  mkdir -p appci-out
  python3 - "$1" <<'PY' || die "could not write appci-out/reused_build.json"
import json, os, sys
sums = {}
for line in open("dist/SHA256SUMS"):
    parts = line.split(None, 1)
    if len(parts) == 2:
        sums[parts[1].strip().lstrip("*")] = parts[0]
env = os.environ.get
rec = {"job_id": int(env("APPCI_BUILD_JOB_ID")), "job_name": env("APPCI_BUILD_JOB_NAME", ""),
       "pipeline_id": env("APPCI_BUILD_PIPELINE_ID", ""), "commit": env("APPCI_BUILD_SHA", ""),
       "this_pipeline": env("CI_PIPELINE_ID", ""), "this_commit": env("CI_COMMIT_SHA", ""),
       "fetched": sys.argv[1], "dist_sha256": sums}
json.dump({"reused_build": rec}, open("appci-out/reused_build.json", "w"), indent=1)
PY
  echo "fetch_build: OK. Testing build job $APPCI_BUILD_JOB_ID from pipeline ${APPCI_BUILD_PIPELINE_ID:-?} (commit ${APPCI_BUILD_SHA:-?}), NOT a build of this pipeline's commit $CI_COMMIT_SHA (app-identical by ci_seq.py)."
}
if [ -f "$LOCAL/SHA256SUMS" ]; then
  echo "fetch_build: using the build job $APPCI_BUILD_JOB_ID already on this machine: $LOCAL (no download)"
  python3 "$(dirname "$0")/verify_build_identity.py" meta "$LOCAL" || die "the cached build is not the one ci_seq asked for (identity check above)"
  rm -rf dist; mkdir dist; cp -R "$LOCAL"/. dist/; rm -f dist/meta.json
  write_record "local cache ($LOCAL)"
  exit 0
fi
echo "fetch_build: WARNING the local copy of build job $APPCI_BUILD_JOB_ID is MISSING ($LOCAL; pruned, or built on another machine). Falling back to downloading its GitLab artifacts."
URL="$CI_API_V4_URL/projects/$CI_PROJECT_ID/jobs/$APPCI_BUILD_JOB_ID/artifacts"
Z="$(mktemp -t appci-build.XXXXXX)"; trap 'rm -f "$Z"' EXIT
echo "fetch_build: downloading artifacts of build job $APPCI_BUILD_JOB_ID (pipeline ${APPCI_BUILD_PIPELINE_ID:-?}, commit ${APPCI_BUILD_SHA:-?})"
code=$(curl -sS -L -o "$Z" -w '%{http_code}' --header "JOB-TOKEN: $CI_JOB_TOKEN" "$URL") || die "curl failed for $URL"
# GitLab answers 404 to a JOB-TOKEN asking for another pipeline's job by id (first CI run, 2026-11-02: the build had
# artifacts). The job token IS allowed the by-ref endpoint (latest successful pipeline of a ref + job name), so try
# that next, then check below that what arrived is the job we were told to test.
if [ "$code" != 200 ] && [ -n "${APPCI_BUILD_REF:-}" ] && [ -n "${APPCI_BUILD_JOB_NAME:-}" ]; then
  REF_ENC=$(printf '%s' "$APPCI_BUILD_REF" | sed 's#/#%2F#g')
  URL="$CI_API_V4_URL/projects/$CI_PROJECT_ID/jobs/artifacts/$REF_ENC/download?job=$APPCI_BUILD_JOB_NAME"
  echo "fetch_build: by-id fetch refused (HTTP $code); trying latest successful $APPCI_BUILD_JOB_NAME on $APPCI_BUILD_REF"
  code=$(curl -sS -L -o "$Z" -w '%{http_code}' --header "JOB-TOKEN: $CI_JOB_TOKEN" "$URL") || die "curl failed for $URL"
  BY_REF=1
fi
if [ "$code" != 200 ] && [ -n "${APPCI_API_TOKEN:-}" ]; then
  code=$(curl -sS -L -o "$Z" -w '%{http_code}' --header "PRIVATE-TOKEN: $APPCI_API_TOKEN" "$URL") || die "curl failed for $URL"
fi
if [ "$code" != 200 ]; then
  case "$code" in
    404) die "HTTP 404 for $URL: no artifacts reachable with the job token (expired after 3 days, erased, or not readable by CI_JOB_TOKEN; pass APPCI_BUILD_REF + APPCI_BUILD_JOB_NAME for the by-ref fetch). Rebuild: play the plain app-test-* job's build.";;
    401|403) die "HTTP $code: CI_JOB_TOKEN may not read job $APPCI_BUILD_JOB_ID's artifacts (set a masked APPCI_API_TOKEN with read_api, or build instead).";;
    *) die "HTTP $code from $URL";;
  esac
fi
rm -rf dist; unzip -q -o "$Z" || die "downloaded file is not a valid artifacts zip"
[ -d dist ] && [ -n "$(ls -A dist)" ] || die "artifacts of job $APPCI_BUILD_JOB_ID contain no dist/"
# identity BEFORE anything is stamped: GitLab's record of the job (commit, name, status), and for the by-ref download that it IS the latest one
APPCI_VERIFY_LATEST="${BY_REF:+1}" python3 "$(dirname "$0")/verify_build_identity.py" api || die "the downloaded build could not be verified as job $APPCI_BUILD_JOB_ID at commit ${APPCI_BUILD_SHA:-?}; refusing to test it"
write_record "GitLab download (local copy missing): ${BY_REF:+by-ref latest successful on $APPCI_BUILD_REF}${BY_REF:-by job id}"
