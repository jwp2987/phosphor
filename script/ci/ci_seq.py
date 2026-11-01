# script/ci/ci_seq.py: drive one GitLab pipeline for a build/* ref in order (check-linux, then plays the manual build/test jobs and waits).
# Prereqs: glab authenticated (run 'glab api user' first). Usage: python3 script/ci/ci_seq.py build/<name> [job-to-skip ...]. Never re-plays a job that already ran. Heavy jobs: ask the owner first.
import json,subprocess,sys,time
ref=sys.argv[1]
SKIP=sys.argv[2:]
def api(p,m="GET"):
    a=["glab","api",p] + (["-X",m] if m!="GET" else [])
    return json.loads(subprocess.run(a,capture_output=True,text=True).stdout or "null")
pid=None
while not pid:
    for p in api("projects/88085499/pipelines?ref="+ref.replace("/","%2F")) or []: pid=p["id"]; break
    time.sleep(20)
def jobs(): return {j["name"]:j for j in api(f"projects/88085499/pipelines/{pid}/jobs?per_page=50")}
def wait(name):
    while True:
        s=jobs()[name]["status"]
        if s in("success","failed","canceled"): return s
        time.sleep(60)
def play(name):
    if jobs()[name]["status"]!="manual": return  # already played/run: never re-run
    api(f"projects/88085499/jobs/{jobs()[name]['id']}/play","POST")
print("pipeline",pid,flush=True)
s=wait("check-linux"); print("check-linux",s,flush=True)
if s!="success": sys.exit(1)
for n in [x for x in ["build-macos-arm64","build-windows-x86_64"] if x not in SKIP]: play(n)
for n in [x for x in ["test-linux","test-warpui-core","test-workspace-rest","build-linux-x86_64"] if x not in SKIP]:
    play(n); print(n,wait(n),flush=True)
for n in [x for x in ["build-macos-arm64","build-windows-x86_64"] if x not in SKIP]: print(n,wait(n),flush=True)
