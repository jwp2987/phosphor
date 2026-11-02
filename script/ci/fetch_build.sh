#!/usr/bin/env bash
# script/ci/fetch_build.sh: app-test-*-reuse jobs. Put the dist/ of an EARLIER build job (same project, any pipeline) into ./dist
# instead of rebuilding. FIRST choice: the copy the build job left in this machine's cache ($HOME/.cache/phosphor-ci/builds/
# <platform>/<job id>/, script/ci/cache_build.sh); each platform's app test runs on the machine that built it, so no download.
# Only if that copy is missing does it fall back to downloading the GitLab artifacts (below), and it says so loudly. Inputs (play-time variables from script/ci/ci_seq.py):
#   APPCI_BUILD_JOB_ID (required), APPCI_BUILD_PIPELINE_ID, APPCI_BUILD_SHA, APPCI_BUILD_JOB_NAME (recorded only).
# Fails loudly (exit 2) on a missing variable, a failed fetch or a zip without dist/: it never tests "something else".
# Auth: CI_JOB_TOKEN (JOB-TOKEN header, same project). If GitLab refuses it and APPCI_API_TOKEN is set, that PRIVATE-TOKEN is tried.
set -eu
die() { echo "ERROR fetch_build: $*" >&2; exit 2; }
: "${APPCI_BUILD_JOB_ID:?}" 2>/dev/null || die "APPCI_BUILD_JOB_ID is not set. This *-reuse job must be played with the build job id (script/ci/ci_seq.py does that); to test a fresh build play the plain app-test-* job instead."
case "$APPCI_BUILD_JOB_ID" in ''|*[!0-9]*) die "APPCI_BUILD_JOB_ID='$APPCI_BUILD_JOB_ID' is not a job id";; esac
case "$(uname -s)" in Darwin) PLAT=macos;; *) PLAT=linux;; esac
LOCAL="${PHOSPHOR_CI_BUILD_CACHE:-$HOME/.cache/phosphor-ci/builds}/$PLAT/$APPCI_BUILD_JOB_ID"
write_record() {
  mkdir -p appci-out
  cat > appci-out/reused_build.json <<J
{"reused_build": {"job_id": $APPCI_BUILD_JOB_ID, "job_name": "${APPCI_BUILD_JOB_NAME:-}", "pipeline_id": "${APPCI_BUILD_PIPELINE_ID:-}", "commit": "${APPCI_BUILD_SHA:-}", "this_pipeline": "$CI_PIPELINE_ID", "this_commit": "$CI_COMMIT_SHA", "fetched": "$1"}}
J
  echo "fetch_build: OK. Testing build job $APPCI_BUILD_JOB_ID from pipeline ${APPCI_BUILD_PIPELINE_ID:-?} (commit ${APPCI_BUILD_SHA:-?}), NOT a build of this pipeline's commit $CI_COMMIT_SHA (app-identical by ci_seq.py)."
  (cd dist && ls -l && { sha256sum -c SHA256SUMS 2>/dev/null || shasum -a 256 -c SHA256SUMS; }) || die "dist/SHA256SUMS check failed"
}
if [ -f "$LOCAL/SHA256SUMS" ]; then
  echo "fetch_build: using the build job $APPCI_BUILD_JOB_ID already on this machine: $LOCAL (no download)"
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
write_record "GitLab download (local copy missing): ${BY_REF:+by-ref latest successful on $APPCI_BUILD_REF}${BY_REF:-by job id}"
