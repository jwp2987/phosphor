#!/usr/bin/env python3
"""script/ci/appci_artifacts.py: make an app-test job's logs and screenshots ready to upload (scrub, then slim), from the JOB SCRIPT.

#2123. Round 14's app-test-windows ran 7 h, failed, and uploaded no run_slim logs, no sshd.log and no screenshots: the scrub and the slim
were after_script steps, GitLab gives an after_script 5 minutes in all, and after_job.ps1 had used them up (one WMI query for each of 1067
recorded PIDs) before either had started. A step that decides whether a failing run can be diagnosed must not sit behind a 5 minute limit
that other steps share. So:

  appci_artifacts.py OUT...             the job script's own pass, on every exit path it controls (wayland_session.sh and mac/ci/app_test.sh:
                                        EXIT trap; windows/ci/app_test.ps1: after its try/catch/finally). Only the job timeout applies.
                                        results_scrub.scrub_tree(OUT), then slim_appci_run.slim(OUT/run) with no budget: nothing is left out.
  appci_artifacts.py --fallback [--budget-s N] --root appci-out
                                        the after_script net, for a job whose script never got there (cancelled, killed, crashed). Per OUT under
                                        the root: a FINISHED pass that still matches the run (slim_appci_run.is_current) is kept, and only the few
                                        files directly in the root, OUT and OUT/run are scrubbed again (record_reuse.py rewrites results.json /
                                        summary.md there after the job script). Otherwise the pass is done here within the budget: scrub of what is
                                        uploaded (those files and OUT/run/logs), then the slim with what is left of N seconds (logs, then the
                                        shots of non-passing cases, then the rest; see slim_appci_run.py).

Always exits 0 and never raises: it must not change the job's result. Every step prints how long it took, so the next trace that runs out of
time says where. `--self-test` runs the offline checks (also run by `run.py --self-test`: appci/selftest_r19_artifacts.py).
"""
import os, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)          # the Windows VM's embeddable Python does not put the script's own directory on sys.path
import results_scrub, slim_appci_run  # noqa: E402


def _step(label, fn, clock):
    t = clock()
    try:
        r = fn()
    except Exception as e:          # a full disk, a file held open: say so and go on to the next step
        print("artifacts: %s FAILED after %.0f s: %r" % (label, clock() - t, e))
        return None
    print("artifacts: %s: %s (%.0f s)" % (label, r, clock() - t))
    return r


def prepare(out, clock=time.monotonic):
    """the job script's pass over one OUT dir (appci-out/<platform>): whole-tree scrub, then an unbudgeted slim"""
    if not os.path.isdir(out):
        print("artifacts: no %s (nothing to do)" % out)
        return
    _step("scrub %s" % out, lambda: "%d file(s) scrubbed" % results_scrub.scrub_tree(out), clock)
    _step("slim %s" % os.path.join(out, "run"), lambda: slim_appci_run.slim(os.path.join(out, "run")), clock)


def fallback(root, budget_s, clock=time.monotonic):
    """the after_script net over every OUT dir under root; returns {out: 'kept' | 'made' | 'incomplete' | 'no run'}"""
    t0 = clock()
    verdict = {}
    if not os.path.isdir(root):
        print("artifacts: no %s (nothing to do)" % root)
        return verdict
    _step("scrub the files in %s" % root, lambda: "%d file(s) scrubbed" % results_scrub.scrub_files(root), clock)
    for name in sorted(os.listdir(root)):
        out = os.path.join(root, name)
        run = os.path.join(out, "run")
        if not os.path.isdir(out):
            continue
        small = lambda: "%d file(s) scrubbed" % (results_scrub.scrub_files(out) + results_scrub.scrub_files(run))  # noqa: E731
        if not os.path.isdir(run):
            _step("scrub the files in %s" % out, small, clock)
            verdict[out] = "no run"
            continue
        if slim_appci_run.is_current(run):
            _step("scrub the files in %s and its run dir" % out, small, clock)
            print("artifacts: %s_slim was finished by the job script and still matches the run: kept" % run)
            verdict[out] = "kept"
            continue
        print("artifacts: %s has no finished run_slim (the job script did not get there, or the run changed since): making it here" % out)
        _step("scrub the files in %s and its run dir" % out, small, clock)
        _step("scrub %s" % os.path.join(run, "logs"), lambda: "%d file(s) scrubbed" % results_scrub.scrub_tree(os.path.join(run, "logs")), clock)
        left = max(0.0, budget_s - (clock() - t0))
        r = _step("slim %s (budget %.0f s)" % (run, left), lambda: slim_appci_run.slim(run, budget_s=left, clock=clock), clock)
        verdict[out] = "made" if (r and r.get("complete")) else "incomplete"
    print("artifacts: after_script pass done in %.0f s: %s" % (clock() - t0, ", ".join("%s %s" % (k, v) for k, v in sorted(verdict.items())) or "nothing"))
    return verdict


def self_test():
    import json, shutil, tempfile
    d = tempfile.mkdtemp(prefix="appci-artifacts-")
    bad = []

    def ok(cond, what):
        print("%s %s" % ("ok  " if cond else "FAIL", what))
        if not cond:
            bad.append(what)

    def png(p):
        try:
            from PIL import Image
            Image.new("RGB", (8, 8), (10, 20, 30)).save(p, "PNG")
        except ImportError:
            open(p, "wb").write(b"\x89PNG\r\n\x1a\n" + b"\0" * 32)

    def make(name):
        out = os.path.join(d, "appci-out", name)
        run = os.path.join(out, "run")
        os.makedirs(os.path.join(run, "logs", "ssh"))
        os.makedirs(os.path.join(run, "shots"))
        open(os.path.join(run, "logs", "app.log"), "w").write("connect to 10.1.2.3 port 22\n")
        open(os.path.join(run, "logs", "ssh", "sshd.log"), "w").write("Starting session: shell\n")
        for n in ("a_pass.png", "b_fail.png", "c_pass.png"):
            png(os.path.join(run, "shots", n))
        json.dump(dict(cases=[dict(name="a", status="pass", shots=["shots/a_pass.png"]), dict(name="b", status="fail", shots=["shots/b_fail.png"]),
                              dict(name="c", status="pass", shots=["shots/c_pass.png"])]), open(os.path.join(run, "results.json"), "w"))
        open(os.path.join(run, "summary.md"), "w").write("host 10.1.2.3\n")
        return out, run

    def shots(run):
        return sorted(os.path.splitext(f)[0] for f in os.listdir(run + "_slim/shots") if f != "OMITTED.txt")

    try:
        root = os.path.join(d, "appci-out")
        # 1. the job script's pass: everything kept, logs scrubbed before they are copied, stamp written, and the net then keeps it
        out, run = make("linux")
        prepare(out)
        ok(shots(run) == ["a_pass", "b_fail", "c_pass"] and not os.path.exists(run + "_slim/shots/OMITTED.txt"), "the job script's pass keeps every shot")
        ok(open(run + "_slim/logs/app.log").read() == "connect to <ip> port 22\n" and os.path.exists(run + "_slim/logs/ssh/sshd.log"),
           "logs (sshd.log too) are in run_slim, scrubbed")
        ok(slim_appci_run.is_current(run), "a finished pass is current")
        before = os.path.getmtime(run + "_slim/logs/app.log"), os.stat(run + "_slim/" + slim_appci_run.STAMP).st_mtime_ns
        open(os.path.join(run, "summary.md"), "w").write("**Tested a REUSED build** on 10.1.2.3\n")      # what record_reuse.py does afterwards
        open(os.path.join(root, "reused_build.json"), "w").write('{"host": "10.1.2.3"}\n')
        v = fallback(root, 480)
        ok(v == {out: "kept"} and os.stat(run + "_slim/" + slim_appci_run.STAMP).st_mtime_ns == before[1], "the net keeps a finished pass (no second slim)")
        ok("10.1.2.3" not in open(os.path.join(run, "summary.md")).read() + open(os.path.join(root, "reused_build.json")).read(),
           "the net still scrubs the small files written after the job script")
        # 2. the run changed after the pass: no longer current, the net remakes it and keeps what was already there
        open(os.path.join(run, "logs", "late.log"), "w").write("late\n")
        ok(not slim_appci_run.is_current(run), "a run that changed is not current")
        v = fallback(root, 480)
        ok(v == {out: "made"} and os.path.exists(run + "_slim/logs/late.log") and shots(run) == ["a_pass", "b_fail", "c_pass"], "the net adds what is missing")
        shutil.rmtree(out)
        os.remove(os.path.join(root, "reused_build.json"))
        # 3. a job whose script never got there (cancelled): the net makes run_slim itself
        out, run = make("windows")
        v = fallback(root, 480)
        ok(v == {out: "made"} and shots(run) == ["a_pass", "b_fail", "c_pass"] and slim_appci_run.is_current(run), "the net makes run_slim for a killed job")
        ok(open(run + "_slim/logs/app.log").read() == "connect to <ip> port 22\n", "and scrubs the logs before copying them")
        # 4. the budget: logs always, the failing case's shot first, the rest listed; not 'complete', so a later pass does not trust it
        shutil.rmtree(run + "_slim")
        ticks = iter(range(0, 100000, 10))                                   # every look at the clock is 10 s later
        r = slim_appci_run.slim(run, budget_s=25, clock=lambda: next(ticks))
        om = open(run + "_slim/shots/OMITTED.txt").read() if os.path.exists(run + "_slim/shots/OMITTED.txt") else ""
        ok(r["complete"] is False and r["omitted"] >= 1 and "b_fail" in shots(run) and "b_fail" not in om and "budget" in om,
           "out of budget: the failing case's shot is kept, what is left out is listed (%s omitted)" % r.get("omitted"))
        ok(os.path.exists(run + "_slim/logs/app.log") and not slim_appci_run.is_current(run), "out of budget: logs are there, the pass is not 'finished'")
        r = slim_appci_run.slim(run, budget_s=10**6)                          # a later pass with time finishes it and drops the list
        ok(r["complete"] and shots(run) == ["a_pass", "b_fail", "c_pass"] and not os.path.exists(run + "_slim/shots/OMITTED.txt") and slim_appci_run.is_current(run),
           "a later pass with time completes it")
        # 5. nothing there is not an error
        ok(fallback(os.path.join(d, "none"), 10) == {} and slim_appci_run.slim(os.path.join(d, "none", "run"))["complete"], "a missing tree is not an error")
    finally:
        shutil.rmtree(d, ignore_errors=True)
    print("self-test: %s" % ("FAILED (%d)" % len(bad) if bad else "all passed"))
    return bad


def main(argv=None):
    argv = sys.argv[1:] if argv is None else argv
    if "--self-test" in argv:
        return 1 if self_test() else 0
    try:
        if "--fallback" in argv:
            budget = float(argv[argv.index("--budget-s") + 1]) if "--budget-s" in argv else 480.0
            root = argv[argv.index("--root") + 1] if "--root" in argv else "appci-out"
            fallback(root, budget)
        else:
            outs = [a for a in argv if not a.startswith("--")]
            if not outs:
                print(__doc__)
            for out in outs:
                prepare(out)
    except Exception as e:
        print("artifacts: FAILED: %r (the job's result is not changed by this step)" % e)
    return 0


if __name__ == "__main__":
    sys.exit(main())
