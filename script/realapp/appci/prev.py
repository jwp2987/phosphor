# appci/prev.py: fetch results.json of the previous SUCCESSFUL run of this CI job (any build/* ref) for build-to-build comparison.
# usage: prev.py OUT_FILE [ARTIFACT_PATH]   (needs CI_API_V4_URL, CI_PROJECT_ID, CI_JOB_NAME, CI_JOB_TOKEN; prints why it found nothing and exits 0)
import json, os, sys, urllib.request


def get(url, tok, raw=False):
    rq = urllib.request.Request(url, headers={"JOB-TOKEN": tok})
    with urllib.request.urlopen(rq, timeout=30) as r:
        b = r.read()
    return b if raw else json.loads(b)


def fetch(out, art_path=None, ref_prefix=os.environ.get("APPCI_PREV_REF_PREFIX", "build/")):
    api, pid, name, tok = (os.environ.get(k) for k in ("CI_API_V4_URL", "CI_PROJECT_ID", "CI_JOB_NAME", "CI_JOB_TOKEN"))
    if not (api and pid and name and tok):
        return "not in CI: no previous-build comparison"
    me = os.environ.get("CI_JOB_ID", "")
    try:
        for page in (1, 2, 3, 4):
            jobs = get("%s/projects/%s/jobs?scope[]=success&per_page=100&page=%d" % (api, pid, page), tok)
            if not jobs:
                break
            for j in jobs:      # newest first
                if j.get("name") == name and str(j.get("id")) != me and str(j.get("ref", "")).startswith(ref_prefix):
                    path = art_path or os.environ.get("APPCI_PREV_ARTIFACT", "")
                    data = get("%s/projects/%s/jobs/%s/artifacts/%s" % (api, pid, j["id"], path), tok, raw=True)
                    json.loads(data)
                    open(out, "wb").write(data)
                    return "previous build: job %s on %s (commit %s)" % (j["id"], j.get("ref"), (j.get("commit") or {}).get("short_id", "?"))
    except Exception as e:
        return "previous-build fetch failed (%s): no build-to-build comparison" % e
    return "no earlier successful %s job on a %s* ref: no build-to-build comparison" % (name, ref_prefix)


if __name__ == "__main__":
    print(fetch(sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else None))
