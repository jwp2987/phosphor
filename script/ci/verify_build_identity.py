#!/usr/bin/env python3
# script/ci/verify_build_identity.py: identity checks for fetch_build.sh (#1501). Never trusts the play-time variables: it compares them with
# something the build itself recorded. Exit 0 = verified, 2 = MISMATCH or cannot verify (fetch_build.sh then dies; it never stamps an unverified sha).
#   meta   DIR            cached build: DIR/meta.json (job_id, sha, pipeline_id written by cache_build) vs APPCI_BUILD_JOB_ID / APPCI_BUILD_SHA
#   meta-pipeline DIR     the same for --this-pipeline: job id = the pointer's id, sha = CI_COMMIT_SHA, pipeline = CI_PIPELINE_ID
#   api                   downloaded build: GitLab's own record of job APPCI_BUILD_JOB_ID (commit, name, status) vs the variables; with
#                         APPCI_VERIFY_LATEST=1 (by-ref download = "latest successful pipeline on the ref") the job must also be THAT pipeline's job
import json, os, sys, urllib.request, urllib.error


def die(msg):
    print("ERROR fetch_build identity: " + msg, file=sys.stderr)
    sys.exit(2)


def env(k):
    return os.environ.get(k, "")


def check_meta(d, want_job, want_sha, want_pipeline=None):
    p = os.path.join(d, "meta.json")
    try:
        m = json.load(open(p))
    except Exception as e:
        die("%s unreadable (%s): a build without a recorded identity cannot be verified" % (p, e))
    if str(m.get("job_id")) != str(want_job):
        die("cached build %s is job %s, but job %s was asked for" % (d, m.get("job_id"), want_job))
    if not want_sha:
        die("no expected commit to compare with (APPCI_BUILD_SHA / CI_COMMIT_SHA empty)")
    if m.get("sha") != want_sha:
        die("cached build %s was built from commit %s, but %s was asked for" % (d, m.get("sha"), want_sha))
    if want_pipeline and str(m.get("pipeline_id")) != str(want_pipeline):
        die("cached build %s belongs to pipeline %s, not %s" % (d, m.get("pipeline_id"), want_pipeline))
    print("fetch_build: identity verified from %s: job %s, commit %s" % (p, m["job_id"], m["sha"][:10]))


def get(path):
    base, last = env("CI_API_V4_URL"), None
    for hdr in (("JOB-TOKEN", env("CI_JOB_TOKEN")), ("PRIVATE-TOKEN", env("APPCI_API_TOKEN"))):
        if not hdr[1]:
            continue
        try:
            req = urllib.request.Request(base + path, headers={hdr[0]: hdr[1]})
            return json.load(urllib.request.urlopen(req, timeout=60))
        except urllib.error.HTTPError as e:
            last = "HTTP %s with %s" % (e.code, hdr[0])
        except Exception as e:
            last = str(e)
    die("cannot read %s from the GitLab API to verify the build (%s); set a masked APPCI_API_TOKEN with read_api, or build instead" % (path, last))


def check_api():
    proj, job, sha, name = env("CI_PROJECT_ID"), env("APPCI_BUILD_JOB_ID"), env("APPCI_BUILD_SHA"), env("APPCI_BUILD_JOB_NAME")
    if not sha:
        die("APPCI_BUILD_SHA is empty: nothing to verify the downloaded build against")
    j = get("/projects/%s/jobs/%s" % (proj, job))
    got = (j.get("commit") or {}).get("id")
    if got != sha:
        die("job %s was built from commit %s, but %s was asked for" % (job, got, sha))
    if name and j.get("name") != name:
        die("job %s is %r, but %r was asked for" % (job, j.get("name"), name))
    if j.get("status") != "success":
        die("job %s status is %r, not success" % (job, j.get("status")))
    if env("APPCI_VERIFY_LATEST") == "1":
        ref = env("APPCI_BUILD_REF")
        pl = get("/projects/%s/pipelines?ref=%s&status=success&per_page=1" % (proj, ref.replace("/", "%2F")))
        if not pl:
            die("no successful pipeline on %s to compare the by-ref download with" % ref)
        jobs = get("/projects/%s/pipelines/%s/jobs?per_page=100" % (proj, pl[0]["id"]))
        ids = [x["id"] for x in jobs if x.get("name") == name and x.get("status") == "success"]
        if str(job) not in [str(i) for i in ids]:
            die("the by-ref download is the latest successful %s on %s (pipeline %s, job(s) %s), NOT job %s that was asked for: refusing to test a different build" % (name, ref, pl[0]["id"], ids, job))
    print("fetch_build: identity verified from the GitLab API: job %s, commit %s, %s, success" % (job, sha[:10], name))


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else ""
    if cmd == "meta":
        check_meta(sys.argv[2], env("APPCI_BUILD_JOB_ID"), env("APPCI_BUILD_SHA"))
    elif cmd == "meta-pipeline":
        check_meta(sys.argv[2], os.path.basename(sys.argv[2].rstrip("/")), env("CI_COMMIT_SHA"), env("CI_PIPELINE_ID"))
    elif cmd == "api":
        check_api()
    else:
        die("usage: verify_build_identity.py meta DIR | meta-pipeline DIR | api")
