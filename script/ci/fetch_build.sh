#!/usr/bin/env bash
# script/ci/fetch_build.sh: app-test-*-reuse jobs. Download the dist/ artifacts of an EARLIER build job (same project, any pipeline)
# into ./dist instead of rebuilding. Inputs (play-time variables from script/ci/ci_seq.py):
#   APPCI_BUILD_JOB_ID (required), APPCI_BUILD_PIPELINE_ID, APPCI_BUILD_SHA, APPCI_BUILD_JOB_NAME (recorded only).
# Fails loudly (exit 2) on a missing variable, a failed fetch or a zip without dist/: it never tests "something else".
# Auth: CI_JOB_TOKEN (JOB-TOKEN header, same project). If GitLab refuses it and APPCI_API_TOKEN is set, that PRIVATE-TOKEN is tried.
set -eu
die() { echo "ERROR fetch_build: $*" >&2; exit 2; }
: "${APPCI_BUILD_JOB_ID:?}" 2>/dev/null || die "APPCI_BUILD_JOB_ID is not set. This *-reuse job must be played with the build job id (script/ci/ci_seq.py does that); to test a fresh build play the plain app-test-* job instead."
case "$APPCI_BUILD_JOB_ID" in ''|*[!0-9]*) die "APPCI_BUILD_JOB_ID='$APPCI_BUILD_JOB_ID' is not a job id";; esac
URL="$CI_API_V4_URL/projects/$CI_PROJECT_ID/jobs/$APPCI_BUILD_JOB_ID/artifacts"
Z="$(mktemp -t appci-build.XXXXXX)"; trap 'rm -f "$Z"' EXIT
echo "fetch_build: downloading artifacts of build job $APPCI_BUILD_JOB_ID (pipeline ${APPCI_BUILD_PIPELINE_ID:-?}, commit ${APPCI_BUILD_SHA:-?})"
code=$(curl -sS -L -o "$Z" -w '%{http_code}' --header "JOB-TOKEN: $CI_JOB_TOKEN" "$URL") || die "curl failed for $URL"
if [ "$code" != 200 ] && [ -n "${APPCI_API_TOKEN:-}" ]; then
  code=$(curl -sS -L -o "$Z" -w '%{http_code}' --header "PRIVATE-TOKEN: $APPCI_API_TOKEN" "$URL") || die "curl failed for $URL"
fi
if [ "$code" != 200 ]; then
  case "$code" in
    404) die "HTTP 404: job $APPCI_BUILD_JOB_ID has no artifacts (expired after 3 days, or erased). Rebuild: play the plain app-test-* job's build.";;
    401|403) die "HTTP $code: CI_JOB_TOKEN may not read job $APPCI_BUILD_JOB_ID's artifacts (set a masked APPCI_API_TOKEN with read_api, or build instead).";;
    *) die "HTTP $code from $URL";;
  esac
fi
rm -rf dist; unzip -q -o "$Z" || die "downloaded file is not a valid artifacts zip"
[ -d dist ] && [ -n "$(ls -A dist)" ] || die "artifacts of job $APPCI_BUILD_JOB_ID contain no dist/"
mkdir -p appci-out
cat > appci-out/reused_build.json <<J
{"reused_build": {"job_id": $APPCI_BUILD_JOB_ID, "job_name": "${APPCI_BUILD_JOB_NAME:-}", "pipeline_id": "${APPCI_BUILD_PIPELINE_ID:-}", "commit": "${APPCI_BUILD_SHA:-}", "this_pipeline": "$CI_PIPELINE_ID", "this_commit": "$CI_COMMIT_SHA"}}
J
echo "fetch_build: OK. Testing build job $APPCI_BUILD_JOB_ID from pipeline ${APPCI_BUILD_PIPELINE_ID:-?} (commit ${APPCI_BUILD_SHA:-?}), NOT a build of this pipeline's commit $CI_COMMIT_SHA (app-identical by ci_seq.py)."
(cd dist && ls -l && { sha256sum -c SHA256SUMS 2>/dev/null || shasum -a 256 -c SHA256SUMS; }) || die "dist/SHA256SUMS check failed"
