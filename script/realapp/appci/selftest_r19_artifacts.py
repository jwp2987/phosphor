# appci/selftest_r19_artifacts.py: offline self-tests for #2123 (docs/design/r19-winartifacts.md): a failing app-test run keeps its run_slim logs,
# sshd.log and screenshots because the JOB SCRIPT makes them ready (script/ci/appci_artifacts.py), not an after_script that GitLab stops after 5 min.
# selftest.py appends CHECKS to its own list (two lines at its end), so `run.py --self-test` runs them. Pure Python: no app, no display.
import os, re, sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.join(HERE, "..", "..", "..")
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(REPO, "script", "ci"))

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


def _src(*parts):
    return open(os.path.join(REPO, *parts), encoding="utf-8").read()


def _job(yml, name):
    """the text of one top-level job of .gitlab-ci.yml"""
    m = re.search(r"(?m)^%s:\n((?:[ \t#].*\n|\n)*)" % re.escape(name), yml)
    assert m, "no job %s in .gitlab-ci.yml" % name
    return m.group(1)


def _after_script(block):
    m = re.search(r"(?m)^  after_script:\n((?:    .*\n)+)", block)
    return m.group(1) if m else ""


@check
def r19_artifacts_pass_keeps_everything_and_the_net_is_bounded_2123():
    """the behaviour, on a made-up run tree: the job script's pass leaves nothing out and scrubs before it copies; the after_script net keeps a
    finished pass, makes a missing one, and under a budget keeps the logs and the failing case's shot and lists what it left out"""
    import contextlib, io
    import appci_artifacts
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        bad = appci_artifacts.self_test()
    assert not bad, "appci_artifacts.py --self-test: %s" % bad
    assert buf.getvalue().count("\nok  ") + buf.getvalue().startswith("ok  ") >= 12, "the self-test ran fewer checks than it holds"


@check
def r19_job_scripts_make_run_slim_themselves_2123():
    """no app-test job leaves the scrub and the slim to its after_script: each job script calls appci_artifacts.py on its exit path, each after_script
    holds only the --fallback net (as its LAST step), and the after_script limit is set explicitly on the three parents"""
    yml = _src(".gitlab-ci.yml")
    jobs = ["app-test-linux-wayland", "app-test-linux-x11", "app-test-macos", "app-test-windows", "app-test-lab-linux", "app-test-linux-wayland-reuse",
            "app-test-linux-x11-reuse", "app-test-lab-linux-reuse", "app-test-macos-reuse", "app-test-windows-reuse"]
    for j in jobs:
        a = _after_script(_job(yml, j))
        steps = [l for l in a.splitlines() if l.strip()]
        assert steps, "%s has no after_script" % j
        assert "results_scrub.py" not in a and "slim_appci_run.py" not in a, "%s: the scrub / slim are after_script steps again (#2123)" % j
        assert "appci_artifacts.py --fallback --root appci-out" in steps[-1], "%s: the after_script does not end with the artifacts net" % j
    # the limit is stated on the three parents; Windows keeps the 45m of #2114 (r19-winmisc), the other two get 15m. Either is above the net's 8 min.
    for parent, val in ((".appci_linux", "15m"), ("app-test-macos", "15m"), ("app-test-windows", '"45m"')):
        assert re.search(r"(?m)^    RUNNER_AFTER_SCRIPT_TIMEOUT: %s" % re.escape(val), _job(yml, parent)), "%s does not set RUNNER_AFTER_SCRIPT_TIMEOUT: %s" % (parent, val)
    # every job that uploads run_slim is one of the jobs above or one of the two collect jobs (which slim on purpose, by hand)
    for m in re.finditer(r"(?m)^([A-Za-z.][\w.-]*):\n((?:[ \t#].*\n|\n)*)", yml):
        if "run_slim/" in m.group(2) and "extends: .collect_run" not in m.group(2) and "extends: [.windows_job, .collect_run]" not in m.group(2):
            assert m.group(1) in jobs + [".appci_linux"], "%s uploads run_slim but is not covered by this check" % m.group(1)
    lin = _src("script", "realapp", "linux", "wayland_session.sh")
    assert "trap 'teardown; artifacts' EXIT" in lin and 'appci_artifacts.py" "$OUT"' in lin
    mac = _src("script", "realapp", "mac", "ci", "app_test.sh")
    assert "trap 'cleanup; artifacts' EXIT" in mac and 'appci_artifacts.py" "$OUT"' in mac
    win = _src("script", "realapp", "windows", "ci", "app_test.ps1")
    i_rep, i_art, i_exit = win.index("--report-only"), win.index("appci_artifacts.py"), win.rindex("exit 1 } else { exit 0 }")
    assert win.index("finally {") < i_rep < i_art < i_exit, "app_test.ps1: the artifacts step must follow the final report and precede the exit"
    for rel in (("script", "realapp", "linux", "wayland_session.sh"), ("script", "realapp", "mac", "ci", "app_test.sh"), ("script", "realapp", "windows", "ci", "app_test.ps1")):
        d = os.path.dirname(os.path.join(REPO, *rel))
        m = re.search(r'"(?:\$HERE/|)([./\\]+)ci[/\\]appci_artifacts\.py"', _src(*rel))
        assert m and os.path.isfile(os.path.join(d, m.group(1).replace("\\", "/"), "ci", "appci_artifacts.py")), "%s: the path to appci_artifacts.py is wrong" % rel[-1]


@check
def r19_scrub_is_linear_on_a_long_word_run_2123():
    """round 14, app-test-linux-wayland: `results_scrub.py --tree appci-out` was still running when the runner stopped the after_script 15 min later.
    The `url with credentials` rule retried `\\w+` from every character of a word run (40 kB of hex: 3 s; a megabyte: hours). It now starts at a
    word's start only, with the same matches"""
    import time
    import results_scrub as rs
    old = re.compile(r"\w+://[^/\s:@]+:[^/\s@]+@")
    new = dict(rs.SECRET)["url with credentials"]
    for t in ("x https://u:p@h/ y", "abchttps://user:pw@host", "a://b:c@ d://e:f@", "no creds http://host/path", "__a1://x:y@z ssh://git@h:22/x", "://a:b@c"):
        assert old.sub("<R>", t) == new.sub("<R>", t), t
    assert new.sub("<R>", "git clone https://me:tok@example.org/r.git") == "git clone <R>example.org/r.git"
    saved, rs._VALS = rs._VALS, []
    try:
        t0 = time.monotonic()
        out = rs.scrub("ab12" * 250_000 + "\n" + "QUJD" * 250_000 + " 10.1.2.3\n")        # 2 MB in two unbroken runs: the old rule needed hours
        took = time.monotonic() - t0
    finally:
        rs._VALS = saved
    assert out.endswith(" <ip>\n") and took < 20, "scrub of a 2 MB word run took %.1f s (quadratic again?)" % took


@check
def r19_after_job_takes_one_process_snapshot_2123():
    """after_job.ps1 no longer asks WMI once per recorded PID before it knows the PID is alive (1067 such queries used up round 14's after_script):
    one snapshot first, a per-PID query only for a PID that snapshot holds, one summary line for the ones that are gone"""
    ps1 = _src("script", "realapp", "windows", "ci", "after_job.ps1")
    snap = ps1.index("foreach ($q in @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue))")
    loop = ps1.index("foreach ($l in Get-Content $f)")
    gate = ps1.index("if ($snap.Count -and -not $snap[")
    per = ps1.index('Get-CimInstance Win32_Process -Filter "ProcessId = $($w[1])"')
    assert snap < loop < gate < per, "the snapshot must come before the loop and gate the per-PID query"
    assert "already gone ({2:N1} s)" in ps1 and 'pid $($w[1]) already gone"' not in ps1, "one line per gone PID is back (1061 lines in round 14)"
    assert "ToFileTimeUtc" in ps1 and "taskkill.exe /PID" in ps1, "the kill rule (creation time or out-dir path, exact PID) is unchanged"


@check
def r19_docs_say_where_run_slim_is_made_2123():
    doc = _src("docs", "testing.md")
    sec = doc[doc.index("**App-test artifacts are slimmed before upload"):]
    sec = sec[:sec.index("\n### ")]
    for word in ("#2123", "appci_artifacts.py", "--fallback", "RUNNER_AFTER_SCRIPT_TIMEOUT", "SLIM_DONE.json", "OMITTED.txt", "selftest_r19_artifacts.py", "after_job.ps1", "url with credentials",
                 "app-test-linux-wayland"):
        assert word in sec, "docs/testing.md (app-test artifacts) does not mention %s" % word
