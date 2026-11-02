#!/usr/bin/env python3
# appci/run.py: runs the real-app case matrix against a Phosphor build, once per session mode (terminal, agent).
# The caller (script/realapp/linux/wayland_session.sh, mac/ci/app_test.sh) prepares the display/compositor and exports
# its environment; this program owns the scratch HOME, the app process (exact PID/process-group teardown), the local
# ssh server and the result files:  RUN/results.jsonl (appended per case), results.json, junit.xml, summary.md,
# shots/*.png, logs/ (app log, stdout, rec logs, sshd log).
# usage: run.py --run DIR --bin PATH --backend wayland|x11|mac|sim [--modes terminal,agent] [--cases a,b] [--app-display wayland|x11]
import argparse, json, os, shutil, subprocess, sys, time, traceback

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import backend, cases, profile, sshd, app as appmod  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--run", required=True)
    ap.add_argument("--bin", default="")
    ap.add_argument("--backend", required=True)
    ap.add_argument("--modes", default="terminal,agent")
    ap.add_argument("--cases", default="")
    ap.add_argument("--skip", default="")
    ap.add_argument("--app-display", default="wayland")
    ap.add_argument("--seed", type=int, default=int(os.environ.get("APPCI_SEED", "1")))
    ap.add_argument("--platform", default="linux")
    ap.add_argument("--startup-timeout", type=int, default=90)
    a = ap.parse_args()
    import signal
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))     # cancelled job: unwind through the finally blocks (app.stop, sshd.stop)
    run = os.path.abspath(a.run)
    os.makedirs(run, exist_ok=True)
    resf = os.path.join(run, "results.jsonl")
    open(resf, "w").close()
    prof = profile.Profile(run)
    import ci_env
    ollama_url, ollama_model = ci_env.ollama()      # env, else ~/.config/phosphor-ci/ci.env; the value is never printed
    sim = a.backend == "sim"
    os.environ["APPCI_PLATFORM"] = a.platform      # linux-wayland | linux-x11 | macos | windows (baselines/<platform>.json)
    if not os.environ.get("APPCI_PREV_RESULTS"):
        import prev
        pf = os.path.join(run, "prev_results.json")
        msg = prev.fetch(pf, os.environ.get("APPCI_PREV_ARTIFACT") or "appci-out/%s/run/results.json" % a.platform.replace("linux-", ""))
        print("[latency] " + msg, flush=True)
        if os.path.exists(pf):
            os.environ["APPCI_PREV_RESULTS"] = pf
    elif not os.path.exists(os.environ["APPCI_PREV_RESULTS"]):
        print("[latency] APPCI_PREV_RESULTS=%s does not exist: no previous-build comparison" % os.environ["APPCI_PREV_RESULTS"], flush=True)

    def emit(row):
        with open(resf, "a") as f:
            f.write(json.dumps(row) + "\n")
        print("[%s] %-24s %s %s" % (row["mode"], row["name"], row["status"].upper(), "; ".join(row["detail"])[:200]), flush=True)

    be = None if sim else backend.make(a.backend)
    if be:
        problems = be.preflight()
        if problems:
            for m in a.modes.split(","):
                emit(dict(name="preflight", mode=m, status="fail", detail=problems, seconds=0))
            return finish(run, a, 2)
    if not a.cases and os.environ.get("APPCI_TIER") == "smoke":
        a.cases = ",".join(cases.SMOKE)
    sel = [c for c in cases.CASES if (not a.cases or c["name"] in a.cases.split(",")) and c["name"] not in a.skip.split(",")]
    sel.sort(key=lambda c: c["order"])
    exit_fail = False
    for mode in a.modes.split(","):
        # per-mode fresh profile + app: settings differ (default_session_mode)
        ssh_info = None
        ssh_why = None       # why the ssh cases skip when there is no usable ssh test host
        app = None
        instance = 1       # app instance number within this mode: each instance's log is kept as phosphor.<n>.log (#1457)
        try:
            prof.build(mode, ollama_url, ollama_model or 'gpt-oss:20b')
            ssh_info = None if sim else sshd.start(prof, run)
            if ssh_info:
                prof.build(mode, ollama_url, ollama_model or 'gpt-oss:20b', ssh_port=ssh_info["port"], ssh_ident=ssh_info["ident"])
                ssh_why = prof.ssh_probe()
                if ssh_why:
                    print("ssh probe failed, ssh cases will skip: %s" % ssh_why, flush=True)
                    sshd.stop(ssh_info)
                    ssh_info = None
            print("owner settings: %s" % profile.Profile.owner_note, flush=True)
            log_path = os.environ.get("APPCI_APP_LOG") or (prof.home + "/.local/state/phosphor/phosphor.log" if a.backend != "mac" else prof.home + "/Library/Logs/phosphor.log")
            if sim:
                be = backend.make("sim", rcfile=prof.home + "/.bashrc", home=prof.home, env={"HOME": prof.home, "PA_CTL": prof.ctl, "ZDOTDIR": prof.home})
                app = None
            else:
                if a.backend == "mac":
                    bundle = os.environ["APPCI_MAC_BUNDLE"]
                    app = appmod.MacApp(run, prof.home, bundle, a.bin)
                else:
                    app = appmod.App(run, prof.home, a.bin, a.app_display)
                app.start()
                if a.backend == "mac":
                    be.pid_fn = app.pid           # the guard compares the frontmost PID with THIS pid before every injection
            ctx = cases.Ctx(be, prof, app, run, mode, a.seed, ssh_info, bool(ollama_url), log_path, ssh_why)
            if not sim:
                up = ctx.until(lambda: prof.prompts() > 0, a.startup_timeout, step=0.5)
                if not up:
                    emit(dict(name="launch", mode=mode, status="fail", seconds=a.startup_timeout,
                              detail=["no shell prompt within %ds (see logs/app.stdout; GPU/Wayland init or keyring?)" % a.startup_timeout]))
                    ctx.shot("launch_fail")
                    exit_fail = True
                    continue
                time.sleep(4)   # let the first-run overlay settle; mode_proof sends Escape
                if a.backend == "mac":
                    d = be.system_dialog(ocr=True)       # BEFORE the first key: a Local Network / TCC prompt would take it
                    if d:
                        raise be.needs_owner(d)
                    if app.instance_count() != 1:
                        raise RuntimeError("expected exactly ONE app instance for this job's bundle, found %d: %s" % (app.instance_count(), app.warn))
                be.calibrate()
            else:
                ctx.until(lambda: prof.prompts() > 0, 10)
            for c in sel:
                if mode not in c["modes"]:
                    continue
                r = cases.Result(c["name"], mode)
                t0 = time.time()
                status = "pass"
                try:
                    if sim and c["gui"]:
                        raise cases.Skip("needs a GUI backend")
                    if not sim:
                        be.guard()
                    if c["name"] not in ("mode_proof",):
                        ctx.reset()
                    c["fn"](ctx, r)
                    status = "fail" if r.fails else "pass"
                except backend.NeedsOwner:
                    raise
                except cases.Skip as e:
                    status = "skip"
                    r.notes.insert(0, str(e))
                except Exception as e:
                    status = "fail"
                    r.fails.append("harness exception: %r" % e)
                    traceback.print_exc()
                shot = None if sim else ctx.shot(c["name"] + ("_FAIL" if status == "fail" else ""))
                dead = app.death() if hasattr(app, "death") else (None if app is None or app.alive() else "APP DIED during/after this case (crash): see logs/app.stdout")
                if dead:
                    status = "fail"
                    r.fails.append(dead)
                    emit(dict(name=c["name"], mode=mode, status=status, seconds=round(time.time() - t0, 1),
                              detail=r.fails + r.notes, metrics=r.metrics, shots=[shot] if shot else []))
                    exit_fail = True
                    keep_instance_log(run, mode, prof, a.backend, ollama_url, instance)   # the relaunch overwrites phosphor.log (#1457)
                    instance += 1
                    app.start()          # relaunch so the remaining cases still run
                    ctx.until(lambda: prof.prompts() > 0, a.startup_timeout, step=0.5)
                    continue
                if status == "fail":
                    exit_fail = True
                emit(dict(name=c["name"], mode=mode, status=status, seconds=round(time.time() - t0, 1),
                          detail=(r.fails or []) + r.notes, metrics=r.metrics, shots=[os.path.relpath(shot, run)] if shot else []))
        except backend.NeedsOwner as e:
            msg = "NEEDS OWNER: %s" % e
            print(msg, flush=True)
            emit(dict(name="needs_owner", mode=mode, status="fail", seconds=0, detail=[msg]))
            for d in {run, os.path.dirname(os.path.abspath(run))}:
                try:
                    open(os.path.join(d, "NEEDS_OWNER.txt"), "w").write(msg + "\n")
                except OSError:
                    pass
            if app:
                app.stop()
            sshd.stop(ssh_info)
            return finish(run, a, 3)
        finally:
            if app:
                app.stop()
            sshd.stop(ssh_info)
            if sim and be:
                be.close()
            logs = os.path.join(run, "logs", mode)
            os.makedirs(logs, exist_ok=True)
            for src in (prof.ctl, prof.rctl):
                if os.path.isdir(src):
                    shutil.copytree(src, os.path.join(logs, os.path.basename(src)), dirs_exist_ok=True)
            for src, dst in (((prof.home + ("/Library/Logs/phosphor.log" if a.backend == "mac" else "/.local/state/phosphor/phosphor.log")), "phosphor.log"), (run + "/app.stdout", "app.stdout"),
                             (run + "/ssh/sshd.log", "sshd.log")):
                if os.path.exists(src):
                    t = ci_env.redact(open(src, errors="replace").read(), ollama_url)   # the endpoint never reaches an artifact
                    open(os.path.join(logs, dst), "w").write(t)
            if os.path.exists(run + "/app.stdout"):
                os.remove(run + "/app.stdout")
    return finish(run, a, 1 if exit_fail else 0)


def keep_instance_log(run, mode, prof, backend_name, ollama_url, n):
    """Copy the dying instance's phosphor.log to logs/<mode>/phosphor.<n>.log BEFORE a relaunch overwrites it, so the failing cases'
    key traces survive (#1457). The final instance's log is still written as phosphor.log by the cleanup below."""
    import ci_env
    src = prof.home + ("/Library/Logs/phosphor.log" if backend_name == "mac" else "/.local/state/phosphor/phosphor.log")
    if not os.path.exists(src):
        return
    logs = os.path.join(run, "logs", mode)
    os.makedirs(logs, exist_ok=True)
    t = ci_env.redact(open(src, errors="replace").read(), ollama_url)
    open(os.path.join(logs, "phosphor.%d.log" % n), "w").write(t)


def finish(run, a, code):
    here = os.path.dirname(os.path.abspath(__file__))
    rc = subprocess.run([sys.executable, os.path.join(here, "report.py"), run, "platform=" + a.platform, "backend=" + a.backend,
                         "app_display=" + a.app_display, "seed=%d" % a.seed, "commit=" + os.environ.get("CI_COMMIT_SHORT_SHA", "local"),
                         "owner_settings=" + str(profile.Profile.owner_note)]).returncode
    return max(code, rc)


if __name__ == "__main__":
    sys.exit(main())
