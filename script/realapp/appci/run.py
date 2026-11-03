#!/usr/bin/env python3
# appci/run.py: runs the real-app case matrix against a Phosphor build, once per session mode (terminal, agent).
# The caller (script/realapp/linux/wayland_session.sh, mac/ci/app_test.sh) prepares the display/compositor and exports
# its environment; this program owns the scratch HOME, the app process (exact PID/process-group teardown), the local
# ssh server and the result files:  RUN/results.jsonl (appended per case), results.json, junit.xml, summary.md,
# shots/*.png, logs/ (app log, stdout, rec logs, sshd log).
# usage: run.py --run DIR --bin PATH --backend wayland|x11|mac|windows|sim [--modes terminal,agent] [--cases a,b] [--app-display wayland|x11]
# Windows (--backend windows --platform windows; windows/ci/app_test.ps1 does the VM safety work and calls this with the private Python):
#   --bin is the exe of the side-by-side TEST build; each session mode runs once per pane shell (APPCI_WIN_SHELLS, default "powershell,gitbash"),
#   cases a pane cannot run SKIP with the reason from winmap.py; `--report-only` regenerates results.json/junit/summary after the wrapper added its rows.
import argparse, json, os, re, shutil, subprocess, sys, time, traceback

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import backend, cases, deadline, injacct, profile, sshd, winmap, app as appmod  # noqa: E402


def skip_status(r, why):
    """Status for a case that raised Skip. A failed check recorded before the Skip is NEVER downgraded: the FAIL stands and says it was also skipped (#1498)."""
    if r.fails:
        r.notes.insert(0, "ALSO SKIPPED after the failed check(s) above: %s" % why)
        return "fail"
    r.notes.insert(0, str(why))
    return "skip"


def unknown_case_names(requested, registry_names):
    """names in a comma list (--cases / --skip) that no registered case has (#1508: a misspelling used to select nothing and exit 0)"""
    return [n for n in (requested or "").split(",") if n and n not in registry_names]


class CaseTimeout(Exception):
    pass


def arm_case_timeout(seconds):
    """per-case wall-clock limit (POSIX SIGALRM; a hung case used to run to the 90 min job timeout with nothing written, #1508).
    Returns a disarm function. No-op where SIGALRM does not exist (Windows)."""
    import signal
    if not hasattr(signal, "SIGALRM") or seconds <= 0:
        return lambda: None

    def boom(*_):
        raise CaseTimeout("case exceeded the %ds per-case limit (APPCI_CASE_TIMEOUT)" % seconds)
    old = signal.signal(signal.SIGALRM, boom)
    signal.alarm(int(seconds))

    def disarm():
        signal.alarm(0)
        signal.signal(signal.SIGALRM, old)
    return disarm


def self_test():
    """`run.py --self-test`: harness-level checks that need no app, display or backend."""
    r = cases.Result("t", "terminal")
    assert skip_status(r, "no card") == "skip" and r.notes == ["no card"], (r.notes, r.fails)
    r = cases.Result("t", "terminal")
    r.check(False, "approve half lost the command")
    assert skip_status(r, "no card within 120 s") == "fail", "a Skip after a failed check must stay FAIL"
    assert r.fails == ["approve half lost the command"] and "ALSO SKIPPED" in r.notes[0], (r.fails, r.notes)
    r = cases.Result("t", "terminal")
    r.check(True, "fine")
    assert skip_status(r, "x") == "skip"
    r = cases.Result("lat", "terminal")
    cases.lat_checks(r, {"budget_verdict": "no-reference", "budget_kind": "overhead", "verdict": "no-reference"}, "Enter")
    assert not r.fails and r.noref and "NO-REFERENCE" in r.notes[0], (r.fails, r.noref)     # not a pass, not a failure (#1499)
    # #1500: the reference-terminal focus guard refuses unless the REFERENCE window has focus
    import backend as _b
    want = dict(pids={4242}, names={"foot"})
    assert _b.focus_matches(want, dict(pid=4242, app_id="foot"))
    assert _b.focus_matches(want, dict(pid=None, app_id="Foot"))
    assert not _b.focus_matches(want, dict(pid=7, app_id="phosphor")) and not _b.focus_matches(want, None)
    be = _b.WaylandBackend()
    be.guard_ref()                       # no override: no-op, never queries sway
    be.guard_override = want
    be._focused = lambda: dict(pid=7, app_id="phosphor")
    _sleep, _b.time.sleep = _b.time.sleep, lambda s: None
    try:
        be.guard()
        raise AssertionError("guard let keys through to the wrong window")
    except RuntimeError as e:
        assert "does not have focus" in str(e)
    finally:
        _b.time.sleep = _sleep
    be._focused = lambda: dict(pid=4242, app_id="foot")
    be.guard_ref()
    # the reference has its own ctl dir: its log is never the app pane's
    import profile as _p, tempfile
    d = tempfile.mkdtemp()
    pr = _p.Profile(d)
    for dd in (pr.ctl, pr.refctl):
        os.makedirs(dd, exist_ok=True)
    open(os.path.join(pr.ctl, "rec.log"), "w").write("C\t1.0\tapp cmd\n")
    open(os.path.join(pr.refctl, "rec.log"), "w").write("C\t2.0\tref cmd\n")
    assert pr.refctl != pr.ctl and [c for _, c in pr.commands()] == ["app cmd"]
    pr.ref_active = True
    assert [c for _, c in pr.commands()] == ["ref cmd"], "while the reference is the target its OWN log is read"
    pr.ref_active = False
    shutil.rmtree(d)
    # the Windows fix tier table: case counts per pass (smoke) and the full tier is untouched
    smoke = cases.SMOKE
    n = {}
    for mode in ("terminal", "agent"):
        for pane in ("powershell", "gitbash"):
            names, o = winmap.tier_plan("windows", "smoke", mode, pane, smoke)
            ran = [x for x in names if mode in [c for c in cases.CASES if c["name"] == x][0]["modes"] and not winmap.skip_reason(x, pane)]
            n[(mode, pane)] = len(ran)
            assert all(x in smoke for x in names), "a fix-tier case outside SMOKE"
            assert {"ssh_connect_typing", "ssh_enter_then_immediate_command", "ssh_nested_typeahead", "fast_typing", "ctrl_c_d"} <= set(names), (mode, pane)
            assert ("key_loss_control" in names) == (mode == "terminal" and pane == "gitbash"), (mode, pane)
            assert (not o["ref"]) == (mode == "agent") and (len(o["ft_gaps"]) == 1) == (mode == "agent")
    assert winmap.tier_plan("windows", None, "agent", "gitbash", smoke)[0] is None and winmap.tier_plan("windows", "full", "agent", "gitbash", smoke)[1]["ref"]
    print("fix-tier cases that run per pass (smoke):", {"%s/%s" % k: v for k, v in n.items()})
    import selftest      # #1522: the offline parser / registry checks
    if selftest.main():
        return 1
    print("self-test ok")
    return 0


def main():
    for st in (sys.stdout, sys.stderr):    # r18e4 (Windows cp1252): an unencodable character in a log line must be escaped, never end the pass
        try:
            st.reconfigure(errors="backslashreplace")
        except (AttributeError, ValueError):
            pass
    if "--self-test" in sys.argv[1:]:
        return self_test()
    ap = argparse.ArgumentParser()
    ap.add_argument("--run", required=True)
    ap.add_argument("--bin", default="")
    ap.add_argument("--backend", default="")      # required unless --report-only (checked below); the wrapper's report-only call used to die on "--backend is required" and the restore row never reached results.json (r18cj)
    ap.add_argument("--modes", default="terminal,agent")
    ap.add_argument("--cases", default="")
    ap.add_argument("--skip", default="")
    ap.add_argument("--app-display", default=None)   # None: derived from the backend (#1526)
    ap.add_argument("--seed", type=int, default=int(os.environ.get("APPCI_SEED", "1")))
    ap.add_argument("--platform", default="linux")
    ap.add_argument("--startup-timeout", type=int, default=90)
    ap.add_argument("--ssh-trials", type=int, default=0, help="#1127: TOTAL ssh_connect_typing trials, spread over its 4 gaps (APPCI_SSH_TOTAL_TRIALS); 40 bounds a 1/40 loss rate only to ~7 %% at 95 %%. Default: APPCI_SSH_TRIALS per gap")
    ap.add_argument("--report-only", action="store_true", help="only (re)generate results.json / junit.xml / summary.md from results.jsonl")
    a = ap.parse_args()
    if a.ssh_trials:
        os.environ["APPCI_SSH_TOTAL_TRIALS"] = str(a.ssh_trials)
    if a.app_display is None:
        a.app_display = {"mac": "macos", "windows": "windows"}.get(a.backend, "wayland")
    if not a.report_only and not a.backend:
        ap.error("the following arguments are required: --backend")
    import signal
    def on_term(signum, _frame):
        # r18d8: a timed-out/cancelled job SIGTERMs us; flush results.json/summary.md/junit from the rows so far BEFORE unwinding (the
        # unwinding itself can take a while: app.stop, sshd.stop), then exit through the finally blocks.
        signal.signal(signum, signal.SIG_IGN)      # a second signal must not interrupt the flush
        print("[appci] signal %d: flushing results, then unwinding" % signum, flush=True)
        write_partial_report(os.path.abspath(a.run), a)
        signal.signal(signum, on_term)
        sys.exit(143)
    signal.signal(signal.SIGTERM, on_term)     # cancelled job: unwind through the finally blocks (app.stop, sshd.stop)
    if hasattr(signal, "SIGHUP"):
        signal.signal(signal.SIGHUP, on_term)
    if hasattr(signal, "SIGBREAK"):
        signal.signal(signal.SIGBREAK, lambda *_: sys.exit(143))    # Windows: how a cancelled runner job stops us
    run = os.path.abspath(a.run)
    os.makedirs(run, exist_ok=True)
    if a.report_only:
        meta = json.load(open(os.path.join(run, "report_meta.json")))
        return finish(run, argparse.Namespace(**meta), 0, write_meta=False)
    resf = os.path.join(run, "results.jsonl")
    open(resf, "w").close()
    win = a.backend == "windows"
    prof = profile.Profile(run)
    panes = [p for p in os.environ.get("APPCI_WIN_SHELLS", "powershell,gitbash").replace(" ", "").split(",") if p] if win else [None]
    # winmap is imported at module level; a local `import winmap` here made it a local of main() and broke every non-Windows run (r18d4)
    if a.backend == "mac":
        import refterm_mac
        refterm_mac.snapshot(run)       # #1485: was Terminal.app already running? (if not, a reference launch is quit by PID at exit)
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

    emitted = set()          # (label, case name) of every row emitted: a selected case with no row never ran (#1508)

    def emit(row):
        with open(resf, "a") as f:
            f.write(json.dumps(row) + "\n")
        emitted.add((row["mode"], row["name"]))
        print(ci_env.redact(row_log_line(row), ollama_url), flush=True)
        for e in (row.get("evidence") or []):
            print(ci_env.redact("    evidence: " + e, ollama_url), flush=True)
        write_partial_report(run, a)      # r18d8: a killed job still leaves every finished row in results.json/summary.md/junit.xml

    be = None if sim else backend.make(a.backend)
    if be:
        problems = be.preflight()
        if problems:
            for m in a.modes.split(","):
                emit(dict(name="preflight", mode=m, status="fail", detail=problems, seconds=0))
            return finish(run, a, 2)
    explicit_cases = bool(a.cases)
    for kv in os.environ.get("APPCI_APP_ENV", "").split(","):       # #1532: extra environment for the app under test, "K=V,K2=V2" (Linux/Windows inherit it, macOS via app.passthrough_env)
        if "=" in kv:
            os.environ[kv.split("=", 1)[0]] = kv.split("=", 1)[1]
    dest = cases.dest_mode()
    print("destination mode of the app under test: PHOSPHOR_QUEUE_DESTINATION=%s (effective %s)" % (os.environ.get("PHOSPHOR_QUEUE_DESTINATION", "<unset>"), dest), flush=True)
    if not a.cases and os.environ.get("APPCI_TIER") == "smoke":
        a.cases = ",".join(winmap.smoke_for(dest, cases.SMOKE))       # legacy (the kill switch): the narrowed list, see winmap.KILL_SWITCH_SMOKE
    reg = {c["name"] for c in cases.CASES}
    bad_names = unknown_case_names(a.cases, reg) + unknown_case_names(a.skip, reg)
    if bad_names:       # #1508: an unknown name is an error, never an empty (green) selection
        for m in a.modes.split(","):
            emit(dict(name="case_selection", mode=m, status="fail", seconds=0, detail=["unknown case name(s) %s (not in the registry: renamed or misspelled?)" % ", ".join(bad_names)]))
        return finish(run, a, 2)
    sel = [c for c in cases.CASES if (not a.cases or c["name"] in a.cases.split(",")) and c["name"] not in a.skip.split(",")]
    sel.sort(key=lambda c: c["order"])
    exit_fail = False
    crashed_exit = None
    case_timeout = float(os.environ.get("APPCI_CASE_TIMEOUT", "1500"))
    passes = [(m, p) for m in a.modes.split(",") for p in panes]
    clock = deadline.Clock()      # the job's own time budget: no new case starts when too little is left (r18e0: killed by the job timeout, artifacts lost)
    print("[deadline] " + clock.describe(), flush=True)

    def case_limit(name):
        return max(case_timeout, float(os.environ.get("APPCI_SOAK_MIN", "60")) * 60 + 600) if name == "soak" else case_timeout

    for mode, pane in passes:
        # per-mode fresh profile + app: settings differ (default_session_mode). Windows: once per pane shell too (label = terminal | terminal@gitbash)
        label = mode if pane in (None, "powershell") else "%s@%s" % (mode, pane)
        prof.pane = pane or prof.pane
        ssh_info = None
        ssh_why = None       # why the ssh cases skip when there is no usable ssh test host
        app = None
        instance = 1       # app instance number within this mode: each instance's log is kept as phosphor.<n>.log (#1457)
        if not clock.allows(0)[0]:
            # a pass that would start with (almost) no budget left: no app launch, every case of the pass gets its explicit skip row
            why = clock.allows(0)[1]
            print("[deadline] %s: pass %s not started: %s" % (time.strftime("%H:%M:%S"), label, why), flush=True)
            clock.logged_stop = True
            tn = None
            if win and not explicit_cases:
                tn, _o = winmap.tier_plan("windows", os.environ.get("APPCI_TIER"), mode, pane, cases.SMOKE)
            for c in sel:
                if mode in c["modes"] and (tn is None or c["name"] in tn):
                    emit(dict(name=c["name"], mode=label, status="skip", seconds=0, detail=[why], metrics={}, shots=[]))
            continue
        try:
            prof.build(mode, ollama_url, ollama_model or 'gpt-oss:20b')
            ssh_info = None if sim else sshd.start(prof, run)
            if ssh_info is None and win and sshd.WHY:
                ssh_why = sshd.WHY
                print("no ssh test host on Windows, ssh cases will skip: %s" % ssh_why, flush=True)
            if ssh_info:
                prof.build(mode, ollama_url, ollama_model or 'gpt-oss:20b', ssh_port=ssh_info["port"], ssh_ident=ssh_info["ident"])
                ssh_why = prof.ssh_probe()
                if ssh_why:
                    print("ssh probe failed, ssh cases will skip: %s" % ssh_why, flush=True)
                    sshd.stop(ssh_info)
                    ssh_info = None
            print("owner settings: %s" % profile.Profile.owner_note, flush=True)
            log_path = os.environ.get("APPCI_APP_LOG") or (prof.home + "/.local/state/phosphor/phosphor.log" if a.backend != "mac" else prof.home + "/Library/Logs/phosphor.log")
            if win:
                log_path = os.environ.get("APPCI_APP_LOG") or None       # the log is under the app's known-folder data dir: found after the launch
            if sim:
                be = backend.make("sim", rcfile=prof.home + "/.bashrc", home=prof.home, env={"HOME": prof.home, "PA_CTL": prof.ctl, "ZDOTDIR": prof.home})
                app = None
            else:
                if a.backend == "mac":
                    bundle = os.environ["APPCI_MAC_BUNDLE"]
                    app = appmod.MacApp(run, prof.home, bundle, a.bin)
                elif win:
                    app = appmod.WindowsApp(run, prof.home, a.bin, pane, prof.ctl)
                    app.prepare()       # wipes %LOCALAPPDATA%\\phosphor and %APPDATA%\\phosphor (backed up + md5-restored by windows/ci/app_test.ps1)
                else:
                    app = appmod.App(run, prof.home, a.bin, a.app_display)
                app.start()
                if a.backend in ("mac", "windows", "wayland"):
                    be.pid_fn = app.pid           # the guard compares the frontmost PID with THIS pid before every injection
            ctx = cases.Ctx(be, prof, app, run, mode, a.seed, ssh_info, bool(ollama_url), log_path, ssh_why, pane=pane)
            tier_names = None
            if win and not explicit_cases:       # the Windows fix tier is a table (winmap.FIX_TIER); an explicit --cases list always wins, the full tier is untouched
                tier_names, ctx.opts = winmap.tier_plan("windows", os.environ.get("APPCI_TIER"), mode, pane, cases.SMOKE)
            if not sim:
                up = await_shell(ctx, app, prof, win, a.startup_timeout)
                if not up:
                    # r18d8 (macOS): the detail named logs/app.stdout (the mac app writes none) and a Wayland cause; the real cause,
                    # 'Bootstrapping failed for shell "zsh"', sat only in phosphor.log. Quote the app's own error lines here.
                    hint = {"windows": "Windows: no main window of pid %s, or the %s pane's shell never started" % (app.pid(), pane),
                            "mac": "macOS: see logs/<mode>/phosphor.log"}.get(a.backend, "Linux: GPU/Wayland init or keyring? see logs/<mode>/app.stdout")
                    emit(dict(name="launch", mode=label, status="fail", seconds=a.startup_timeout,
                              detail=["no shell prompt within %ds (%s)" % (a.startup_timeout, hint)] + launch_errors(log_src(prof, a.backend, app), ollama_url)))
                    ctx.shot("launch_fail")
                    exit_fail = True
                    continue
                time.sleep(4)   # let the first-run overlay settle; mode_proof sends Escape
                if win:
                    ctx.log_path = os.environ.get("APPCI_APP_LOG") or app.find_log() or ""      # now that the app has written one
                if a.backend == "mac":
                    d = be.system_dialog(ocr=True)       # BEFORE the first key: a Local Network / TCC prompt would take it
                    if d:
                        raise be.needs_owner(d)
                    if app.instance_count() != 1:
                        raise RuntimeError("expected exactly ONE app instance for this job's bundle, found %d: %s" % (app.instance_count(), app.warn))
                be.calibrate()
            else:
                ctx.until(lambda: prof.prompts() > 0, 10)
            proved = not win
            for c in sel:
                if mode not in c["modes"]:
                    continue
                if tier_names is not None and c["name"] not in tier_names:
                    continue
                ok_t, why_t = clock.allows(case_limit(c["name"]))
                if not ok_t:
                    if not clock.logged_stop:
                        clock.logged_stop = True
                        print("[deadline] %s: STOP starting cases in %s: %s; every remaining case gets a skip row" % (time.strftime("%H:%M:%S"), label, why_t), flush=True)
                    emit(dict(name=c["name"], mode=label, status="skip", seconds=0, detail=[why_t], metrics={}, shots=[]))
                    continue
                if win:
                    why = winmap.skip_reason(c["name"], pane)
                    if why:          # explicit, per pane: never a silent omission
                        emit(dict(name=c["name"], mode=label, status="skip", seconds=0, detail=[why], metrics={}, shots=[]))
                        continue
                    if not proved and c["name"] != "mode_proof":
                        proved = True
                        bad = prove_keys(ctx, be, pane, label)       # the key-delivery proof of the PowerShell harness (after mode_proof's pre-key screenshot)
                        if bad:
                            emit(dict(name="key_delivery", mode=label, status="fail", seconds=0, detail=[bad], metrics={}, shots=[ctx.shot("keys_not_delivered") or ""]))
                            exit_fail = True
                            break
                        emit(dict(name="key_delivery", mode=label, status="pass", seconds=0, detail=["marker typed and received via SendInput %s" % be.key_method],
                                  metrics={"method": be.key_method}, shots=[]))
                r = cases.Result(c["name"], mode)
                t0 = time.time()
                ctx.t_case0 = t0
                ctx.extra_logs = []
                k_start = len(be.calls) if getattr(be, "calls", None) is not None else None     # injector accounting (#1509)
                log_off = ctx.log_size() if (ctx.log_path and not sim) else None      # job-log evidence: the app-side Enter destinations of THIS case
                status = "pass"
                try:       # r18e9: the app instance this case started on (a case that relaunches the app itself ends in a fresh TERMINAL session)
                    pid_start = app.pid() if app is not None and hasattr(app, "pid") else None
                except Exception:
                    pid_start = None
                # r18d8: soak runs APPCI_SOAK_MIN (60) minutes, longer than the 1500 s default: it hit CaseTimeout at 25 min. It gets its own length + 10 min.
                disarm = arm_case_timeout(max(case_timeout, float(os.environ.get("APPCI_SOAK_MIN", "60")) * 60 + 600) if c["name"] == "soak" else case_timeout)
                try:
                    if sim and c["gui"]:
                        raise cases.Skip("needs a GUI backend")
                    if not sim:
                        if hasattr(be, "check_injector"):
                            be.check_injector()       # r18d8: a dead keyboard holder is recorded and restarted, never read as app key loss
                        be.guard()
                    if c["name"] not in ("mode_proof",):
                        ctx.reset()
                    c["fn"](ctx, r)
                    status = "fail" if r.fails else ("noref" if r.noref else "pass")
                except backend.NeedsOwner:
                    raise
                except cases.Skip as e:
                    status = skip_status(r, e)
                except Exception as e:
                    status = "fail"
                    r.fails.append("harness exception: %r" % e)
                    traceback.print_exc()
                finally:
                    disarm()
                if k_start is not None and a.backend in ("wayland", "x11", "mac") and not sim:
                    try:       # informational (#1509): injector sent N; app saw M; lost-before-app K
                        txt, acct = injacct.account(be.calls[k_start:], list(ctx.extra_logs) + [log_path] if ctx.extra_logs else log_path, c["name"])
                        if txt:
                            r.note("injector accounting: " + txt)
                            r.metrics["injector"] = acct
                    except Exception as e:
                        r.note("injector accounting failed: %r" % e)
                for ev in (be.drain_health() if hasattr(be, "drain_health") else []):
                    r.note(ev + " (key loss in this case is the INJECTOR's, not the app's)")
                shot = None if sim else ctx.shot(c["name"] + ("_FAIL" if status == "fail" else ""))
                evidence = case_evidence(c["name"], r, ctx, log_off)
                dead = app.death() if hasattr(app, "death") else (None if app is None or app.alive() else "APP DIED during/after this case (crash): see logs/app.stdout")
                if dead:
                    status = "fail"
                    dead = classify_death(dead, ctx, t0, log_off)
                    r.fails.append(dead)
                    emit(dict(name=c["name"], mode=label, status=status, seconds=round(time.time() - t0, 1), evidence=evidence,
                              detail=r.fails + r.notes, metrics=r.metrics, shots=[shot] if shot else []))
                    exit_fail = True
                    keep_instance_log(run, label, prof, a.backend, ollama_url, instance, app)   # the relaunch overwrites phosphor.log (#1457)
                    instance += 1
                    nb = prof.prompts()      # prompt.log is never cleared: count only a prompt the NEW instance logs (r18cz: the relaunch awaited an OLD prompt, Ctrl-C hit the bootstrap)
                    app.start()          # relaunch so the remaining cases still run
                    if await_shell(ctx, app, prof, win, a.startup_timeout, base=nb):
                        time.sleep(4)      # the same first-run settle as the first launch
                    if win:
                        be.guard()
                        proved = False       # a relaunched Windows pane needs its keys proven again (and its cwd set)
                    continue
                if status == "fail":
                    exit_fail = True
                    if win:     # the rotating app log keeps only its newest part: save this failing case's lines now, plus the ssh logs (r18cj: the evidence was gone)
                        save_case_log(run, label, c["name"], t0, prof, ctx, app, ollama_url)
                emit(dict(name=c["name"], mode=label, status=status, seconds=round(time.time() - t0, 1), evidence=evidence,
                          detail=(r.fails or []) + r.notes, metrics=r.metrics, shots=[os.path.relpath(shot, run)] if shot else []))
                if c["name"].startswith(("ssh_", "remote_")):
                    # r18ct: a failed nested-ssh case left the pane inside the remote shell, and the next case (latency_enter) typed a zsh-only
                    # `print` into the remote bash, which agent mode routed to the agent. Every ssh case ends with the pane provably LOCAL.
                    where = ctx.return_to_local()
                    if where != "local":
                        emit(dict(name="pane_not_local_after_" + c["name"], mode=label, status="fail", seconds=0,
                                  shots=[ctx.shot("not_local_after_" + c["name"]) or ""],
                                  detail=["after %s the pane could not be proven back in the local shell (%s); the app is relaunched so later cases start clean" % (c["name"], where)]))
                        keep_instance_log(run, label, prof, a.backend, ollama_url, instance, app)
                        instance += 1
                        app.stop()
                        nb = prof.prompts()      # prompt.log is never cleared: count only a prompt the NEW instance logs (r18cz: the relaunch awaited an OLD prompt, Ctrl-C hit the bootstrap)
                        app.start()
                        if await_shell(ctx, app, prof, win, a.startup_timeout, base=nb):
                            time.sleep(4)      # the same first-run settle as the first launch
                        if win:
                            be.guard()
                            proved = False
                        continue
                if ctx.mode == "agent" and c["name"] != "mode_proof":
                    # every agent-pass case must END in agent mode, or the cases after it measure a terminal session under the agent label
                    # (r18cx macOS: mode_proof's Escape left agent view; nothing noticed). Unreadable screen: no verdict, no row.
                    seen = ctx.mode_now("mode_after_" + c["name"])
                    try:
                        pid_now = app.pid() if app is not None and hasattr(app, "pid") else None
                    except Exception:
                        pid_now = None
                    if seen == "terminal" and pid_start and pid_now and pid_now != pid_start:
                        # r18e9 (macOS ui_startup_time, remote_session_ext): the CASE relaunched the app itself (pid %s -> %s) and a relaunch starts in a terminal session,
                        # so "left agent mode" blamed the app for the case's own restart. Visible row, not a failure; the app is relaunched so the next cases run in agent mode.
                        emit(dict(name="agent_view_not_restored_after_" + c["name"], mode=label, status="pass", seconds=0,
                                  shots=[os.path.relpath(p, run) for p in [ctx.shot("left_agent_after_" + c["name"])] if p],
                                  detail=["%s relaunched the app itself (pid %s -> %s); a relaunched app starts in a terminal session, so this is not an app finding. "
                                          "The app is relaunched again (fresh agent session) so the cases after it run in agent mode." % (c["name"], pid_start, pid_now)]))
                        keep_instance_log(run, label, prof, a.backend, ollama_url, instance, app)
                        instance += 1
                        app.stop()
                        nb = prof.prompts()
                        app.start()
                        if await_shell(ctx, app, prof, win, a.startup_timeout, base=nb):
                            time.sleep(4)
                        if win:
                            be.guard()
                            proved = False
                        continue
                    if seen == "terminal" and ctx.double_ctrlc_since(t0):
                        # r18d2: two Ctrl-C within the 1 s confirmation window of an empty agent conversation leave agent view BY DESIGN
                        # (view.rs should_ctrl_c_exit_agent_view -> exit_agent_view_with_required_confirmation). A visible row, not a failure; the app is
                        # still relaunched so the cases after this one run in agent mode.
                        emit(dict(name="agent_view_left_by_double_ctrl_c_after_" + c["name"], mode=label, status="pass", seconds=0,
                                  shots=[os.path.relpath(p, run) for p in [ctx.shot("left_agent_after_" + c["name"])] if p],
                                  detail=["after %s the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation window: that exits an empty "
                                          "agent conversation by design, so this is not an app finding; the app is relaunched (fresh agent session)" % c["name"]]))
                        keep_instance_log(run, label, prof, a.backend, ollama_url, instance, app)
                        instance += 1
                        app.stop()
                        nb = prof.prompts()
                        app.start()
                        if await_shell(ctx, app, prof, win, a.startup_timeout, base=nb):
                            time.sleep(4)
                        if win:
                            be.guard()
                            proved = False
                        continue
                    if seen == "terminal":
                        emit(dict(name="left_agent_mode_after_" + c["name"], mode=label, status="fail", seconds=0,
                                  shots=[os.path.relpath(p, run) for p in [ctx.shot("left_agent_after_" + c["name"])] if p],
                                  detail=["after %s the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later "
                                          "agent cases measure agent mode. Candidate APP finding if the case typed no Escape." % c["name"]]))
                        exit_fail = True
                        keep_instance_log(run, label, prof, a.backend, ollama_url, instance, app)
                        instance += 1
                        app.stop()
                        nb = prof.prompts()      # prompt.log is never cleared: count only a prompt the NEW instance logs (r18cz: the relaunch awaited an OLD prompt, Ctrl-C hit the bootstrap)
                        app.start()
                        if await_shell(ctx, app, prof, win, a.startup_timeout, base=nb):
                            time.sleep(4)      # the same first-run settle as the first launch
                        if win:
                            be.guard()
                            proved = False
                        continue
                alive = ctx.pane_responsive() if (win and (status == "fail" or c["name"] in ("ctrl_c_d", "cadence"))) else 0
                if alive == 1:
                    # typing took effect only after the retry's Ctrl-C x3: not wedged, but the NEXT case would have typed into a pane that eats Enter (r18co Git Bash
                    # pane after cadence: keys written to the pty, Enter taken by the empty input editor, fast_typing 0/30). Visible row, not a failure here.
                    emit(dict(name="pane_needed_ctrl_c_after_" + c["name"], mode=label, status="pass", seconds=0,
                              detail=["the first marker command after %s had no effect; it worked after Ctrl-C x3 (candidate APP finding: input focus/Enter routing after a long-running command, see docs/testing.md Windows triage)" % c["name"]]))
                if alive is None:
                    # a wedged pane makes every later case fail for a reason that is not theirs (r18cj: PowerShell pane stuck after ctrl_c_d, so ssh_* and
                    # latency_enter measured nothing). Wedged = a marker command typed twice (the second after Ctrl-C x3) had NO EFFECT (no marker file). Say so in its own
                    # row, relaunch the app, and prove the keys again.
                    emit(dict(name="pane_wedged_after_" + c["name"], mode=label, status="fail", seconds=0, shots=[ctx.shot("wedged_after_" + c["name"]) or ""],
                              detail=["the %s pane did not run a marker command (no effect file) after %s, twice: the app is relaunched so the remaining cases measure something" % (pane or "default", c["name"])]))
                    exit_fail = True
                    keep_instance_log(run, label, prof, a.backend, ollama_url, instance, app)
                    instance += 1
                    app.stop()
                    nb = prof.prompts()      # prompt.log is never cleared: count only a prompt the NEW instance logs (r18cz: the relaunch awaited an OLD prompt, Ctrl-C hit the bootstrap)
                    app.start()
                    if await_shell(ctx, app, prof, win, a.startup_timeout, base=nb):
                        time.sleep(4)      # the same first-run settle as the first launch
                    be.guard()
                    proved = False
        except backend.NeedsOwner as e:
            msg = "NEEDS OWNER: %s" % e
            print(msg, flush=True)
            emit(dict(name="needs_owner", mode=label, status="fail", seconds=0, detail=[msg]))
            for d in {run, os.path.dirname(os.path.abspath(run))}:
                try:
                    open(os.path.join(d, "NEEDS_OWNER.txt"), "w").write(msg + "\n")
                except OSError:
                    pass
            if app:
                app.stop()
            sshd.stop(ssh_info)
            return finish(run, a, 3)
        except (Exception, SystemExit) as e:
            # prof.build / app.start / be.calibrate / a relaunch raising used to abort with no results.json (#1508): a visible FAIL row, the
            # remaining passes still run (a cancelled job, SystemExit, ends the loop after its row)
            traceback.print_exc()
            emit(dict(name="harness_crash", mode=label, status="fail", seconds=0, detail=["the %s pass crashed outside a case: %r" % (label, e)]))
            exit_fail = True
            if isinstance(e, SystemExit):
                crashed_exit = e.code if e.code is not None else 143
        finally:
            if app:
                pid_before = None
                try:
                    pid_before = app.pid()
                except Exception:
                    pass
                app.stop()
                try:
                    print("teardown[%s]: app pid %s %s" % (label, pid_before, "STILL RUNNING after stop" if app.alive() else "stopped (gone)"), flush=True)
                except Exception as e:
                    print("teardown[%s]: app pid %s stop status unknown: %r" % (label, pid_before, e), flush=True)
            sshd.stop(ssh_info)
            print("teardown[%s]: test sshd %s" % (label, "stopped" if ssh_info else "was not running"), flush=True)
            if sim and be:
                be.close()
            logs = os.path.join(run, "logs", label)
            os.makedirs(logs, exist_ok=True)
            for src in (prof.ctl, prof.rctl):
                if os.path.isdir(src):
                    shutil.copytree(src, os.path.join(logs, os.path.basename(src)), dirs_exist_ok=True)
            if win:      # the app's data + log dirs (known folders) go next to the other artifacts before app_test.ps1 restores the owner's copy
                try:
                    shutil.copytree(os.path.join(os.environ["LOCALAPPDATA"], "phosphor"), os.path.join(run, "data_" + label.replace("@", "_")), dirs_exist_ok=True)
                except Exception:
                    pass
            dest_log = None
            for src, dst in ((log_src(prof, a.backend, app), "phosphor.log"), (run + "/app.stdout", "app.stdout"),
                             (run + "/ssh/sshd.log", "sshd.log"), (run + "/ssh/fakesshd.stderr", "fakesshd.stderr")):
                if os.path.exists(src):
                    t = ci_env.redact(app_log_text(src) if dst == "phosphor.log" else open(src, errors="replace").read(), ollama_url)   # the endpoint never reaches an artifact
                    open(os.path.join(logs, dst), "w").write(t)
                    if dst == "phosphor.log":
                        dest_log = t
            if dest == "shadow" and dest_log is not None:
                # #1528: the shadow comparison's consumer in the real app: every untagged kh_dest_disagree line is a regression
                total, untagged = cases.dest_disagreements(dest_log)
                emit(dict(name="dest_disagree", mode=label, status="fail" if untagged else "pass", seconds=0,
                          detail=["%d untagged kh_dest_disagree line(s) (%d total, tagged ones are known causes) in the app log" % (untagged, total)],
                          metrics={"queue_destination": dest, "dest_disagree": total, "dest_disagree_untagged": untagged}, shots=[]))
                exit_fail = exit_fail or bool(untagged)
            if os.path.exists(run + "/app.stdout"):
                os.remove(run + "/app.stdout")
            if not sim:
                cleanup_scratch(label, prof, run)
        if crashed_exit is not None:
            break
        if not win and not exit_fail:
            # #1508: every selected case that applies to this mode must have a row (pass, fail or skip), else the run did less than asked
            missing = [c["name"] for c in sel if mode in c["modes"] and (label, c["name"]) not in emitted]
            if missing:
                emit(dict(name="cases_not_run", mode=label, status="fail", seconds=0, detail=["selected case(s) produced no row: %s" % ", ".join(missing[:12])]))
                exit_fail = True
    if crashed_exit is not None:
        finish(run, a, 143)
        raise SystemExit(crashed_exit)
    return finish(run, a, 1 if exit_fail else 0)


CRASH_RX = re.compile(r"panicked at|\bSIGSEGV\b|\bSIGABRT\b|\bSIGBUS\b|fatal runtime error|stack overflow|Segmentation fault|EXC_BAD_ACCESS", re.I)


def classify_death(dead, ctx, t0, log_off):
    """r18e9 (macOS remote_session_*, shell_zsh): the case's trailing `exit` ran in the pane's own shell, the pane closed, the app quit, and the harness reported
    "APP DIED". Name a pane closed by the case's own `exit` (no panic/signal/crash report in the case's app-log slice, window-close line present) as that, a
    HARNESS defect of the case, never a crash; keep the crash wording otherwise and say what was checked."""
    try:
        sl = ctx.log_since(log_off) if log_off is not None else ""
        crash = [l for l in sl.splitlines() if CRASH_RX.search(l)][:3]
        reports = []
        if sys.platform == "darwin":       # a real crash leaves a report; a clean quit does not
            import glob
            reports = [p for p in glob.glob(os.path.expanduser("~/Library/Logs/DiagnosticReports/*hosphor*")) if os.path.getmtime(p) >= t0 - 5]
        closing = [l for l in sl.splitlines() if re.search(r"will close|window.*clos|all windows closed", l, re.I)][:1]
        if crash or reports:
            return "%s; crash evidence: %s" % (dead, "; ".join([c[:160] for c in crash] + reports))
        if ctx.last_typed_exit_since(t0) and closing:
            return ("PANE CLOSED BY THE CASE'S OWN `exit` (harness defect, NOT an app crash): the last text the case typed was `exit`, the app logged %r, and the case's app-log slice "
                    "has no panic/SIGSEGV/SIGABRT/crash report. Underlying: %s" % (closing[0][:160], dead))
        return "%s; no panic/signal/crash report found in the case's app-log slice (%d chars) or DiagnosticReports%s" % (
            dead, len(sl), "; the case's last typed text was `exit` but no window-close line was logged" if ctx.last_typed_exit_since(t0) else "")
    except Exception as e:
        return "%s (death classification failed: %r)" % (dead, e)


def save_case_log(run, label, case, t0, prof, ctx, app, ollama_url=None):
    """Windows: after a FAILED case, write the app-log lines since the case began (the log rotates and keeps only its newest part, so the
    failing case's key trace was gone by the end of the pass), plus the ssh server's log, to logs/<label>/case_<case>.log (#1488, r18cj)."""
    import ci_env
    try:
        src = log_src(prof, "windows", app)
        keep = []
        if src and os.path.exists(src):
            cut = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(t0 - 1))
            for l in app_log_text(src).splitlines():
                if l[:20] >= cut or not l[:2] == "20":       # non-timestamped continuation lines ride with their neighbour
                    keep.append(l)
        txt = "\n".join(keep)[:12_000_000]
        sl = os.path.join(run, "ssh", "sshd.log")
        if os.path.exists(sl):
            txt += "\n=== ssh/sshd.log ===\n" + open(sl, errors="replace").read()[-200_000:]
        logs = os.path.join(run, "logs", label)
        os.makedirs(logs, exist_ok=True)
        open(os.path.join(logs, "case_%s.log" % case), "w").write(ci_env.redact(txt, ollama_url))
    except Exception as e:          # evidence capture must never fail the run
        print("save_case_log: %r" % (e,), flush=True)


def app_log_text(src):
    """The whole session's app log: the app rotates it at 10 MB into <log>.in_session.N (#1187, N=0 newest), so the rotated chunks come
    first, oldest first, then the active file. Copying only the active file lost the first part of long modes (#1482 analysis)."""
    import glob
    def idx(f):
        tail = f.rsplit(".", 1)[1]
        return int(tail) if tail.isdigit() else -1
    parts = sorted(glob.glob(glob.escape(src) + ".in_session.*"), key=idx, reverse=True) + [src]
    return "".join(open(f, errors="replace").read() for f in parts if os.path.exists(f))


LAUNCH_ERROR_RX = re.compile(r"Bootstrapping failed|panicked|\bERROR\b|\[ERROR\]|failed to (create|open|spawn)|keyring", re.I)


def launch_errors(src, ollama_url, limit=8):
    """The app log's error lines at a launch failure (newest last, at most `limit`, redacted), so the case detail names the cause."""
    import ci_env
    try:
        lines = [l for l in app_log_text(src).splitlines() if LAUNCH_ERROR_RX.search(l)] if src and os.path.exists(src) else []
    except Exception as e:
        return ["app log unreadable at launch failure: %s" % e]
    if not lines:
        return ["app log (%s): no error line" % (src or "not found")]
    return ["app log: " + ci_env.redact(l.strip(), ollama_url) for l in lines[-limit:]]


def log_src(prof, backend_name, app=None):
    if backend_name == "windows":
        return (app.find_log() if app and hasattr(app, "find_log") else None) or ""
    return prof.home + ("/Library/Logs/phosphor.log" if backend_name == "mac" else "/.local/state/phosphor/phosphor.log")


def await_shell(ctx, app, prof, win, timeout, base=0):
    """True once the pane's shell is up. POSIX / Git Bash pane: the first prompt.log line from the scratch rc. Windows PowerShell pane (no rc hook):
    the app's main window exists, then a fixed settle for PowerShell + the app's bootstrap (12 s, as the PowerShell harness)."""
    if not win:
        return ctx.until(lambda: prof.prompts() > base, timeout, step=0.5)
    if not app.wait_window(60):
        return False
    if prof.local_prompts:
        return ctx.until(lambda: prof.prompts() > base, timeout, step=0.5)
    time.sleep(12)
    return app.alive()


def set_ps_pane_state(ctx, be):
    """PowerShell pane, once per app launch, after the key proof: cd into the case dir (cases write RELATIVE effect files there) and, when there is an ssh
    test host, define `ssh` as a function that adds `-F <home>/.ssh/config` (the pane has no rc, so no <home>/bin wrapper and System32 ssh.exe reads the real
    user's config: r18co showed `ssh: Could not resolve hostname fake` in every PowerShell ssh case). Returns None, or the reason it could not be set.
    r18co: the first Set-Location was typed while the proof's Add-Content was still returning to the prompt, so the pane took Ctrl-U as text
    (`^USet-Location` -> 'not recognized'), stayed in the real home, and EVERY later PowerShell effect file landed outside the case dir (all PowerShell
    cases failed, then the wedge probe failed too). So: settle, type, and verify by an effect (a relative marker file must appear in the case dir); retry."""
    wd = ctx.W.replace("\\", "/")
    for attempt in range(3):
        time.sleep(1.5 + attempt)
        for _ in range(attempt + 1):
            be.key("ctrl+c")
            time.sleep(0.8)
        be.key("ctrl+u")
        tok = ctx.tok(8).lower()
        fn = ("; " + ctx.prof.ps_ssh_function()) if (ctx.ssh and ctx.prof.ps_ssh_function()) else ""
        be.type("Set-Location -LiteralPath '%s'%s; Add-Content cwd_%s.txt %s" % (wd, fn, tok, tok), 8, enter=True)
        if ctx.until(lambda: tok in ctx.lines("cwd_%s.txt" % tok), 10 + 5 * attempt):
            time.sleep(0.8)
            return None
    return "the PowerShell pane could not be put in the case dir (Set-Location typed 3 times, the relative marker file never appeared in %s): every PowerShell effect file would land elsewhere" % ctx.W


def prove_keys(ctx, be, pane, label):
    """Windows key-delivery proof (Prove-Keys of the PowerShell harness): type a marker command with an ABSOLUTE path and confirm the file got it,
    with SendInput virtual keys first, then KEYEVENTF_UNICODE packets. None on success (be.key_method is the method that worked; a PowerShell pane is
    cd'ed into the case dir), else the 'keys not delivered: <reason>' text with the session/foreground diagnostics."""
    import winapi
    log = []
    for method in ("vk", "unicode"):
        be.key_method = method
        tok = ctx.tok(10).lower()
        f = os.path.join(ctx.W, "proof_%s.txt" % label.replace("@", "_"))
        try:
            os.remove(f)
        except OSError:
            pass
        fp = f.replace("\\", "/")
        try:
            be.guard()
            be.key("ctrl+c")
            time.sleep(0.3)
            be.key("ctrl+u")
            be.type(("Add-Content -LiteralPath '%s' %s" if pane == "powershell" else "echo %s >> '%s'") % ((fp, tok) if pane == "powershell" else (tok, fp)), 8, enter=True)
        except Exception as e:
            log.append("%s threw: %s" % (method, e))
            continue
        if ctx.until(lambda: tok in ctx.lines(os.path.basename(f)), 8):
            if pane == "powershell":
                bad = set_ps_pane_state(ctx, be)
                if bad:
                    return bad
            print("keys: delivered via SendInput %s" % method, flush=True)
            return None
        log.append("%s: marker not in the file" % method)
    h, fp_ = winapi.foreground()
    return "keys not delivered: %s; foreground=%s(pid %s) app-pid=%s SendInput-failures=%d" % ("; ".join(log), winapi.proc_name(fp_), fp_, ctx.app.pid(), be.fails)


def keep_instance_log(run, mode, prof, backend_name, ollama_url, n, app=None):
    """Copy the dying instance's phosphor.log to logs/<mode>/phosphor.<n>.log BEFORE a relaunch overwrites it, so the failing cases'
    key traces survive (#1457). The final instance's log is still written as phosphor.log by the cleanup below."""
    import ci_env
    src = log_src(prof, backend_name, app)
    if not src or not os.path.exists(src):
        return
    logs = os.path.join(run, "logs", mode)
    os.makedirs(logs, exist_ok=True)
    t = ci_env.redact(app_log_text(src), ollama_url)
    open(os.path.join(logs, "phosphor.%d.log" % n), "w").write(t)


def row_log_line(row):
    """r18d8: the job-log line of a finished case. The first failing message (or, for a no-reference row, the NO-REFERENCE reason)
    is printed IN FULL (the runner log used to show only 200 characters); the remaining detail stays capped at 200."""
    det = [str(d) for d in (row.get("detail") or [])]
    head = "[%s] %-24s %s" % (row["mode"], row["name"], row["status"].upper())
    if not det:
        return head
    first = det[0]
    if row["status"] == "noref":
        first = next((d for d in det if d.startswith("NO-REFERENCE")), first)
    rest = [d for d in det if d is not first]
    line = head + " " + first
    if rest:
        line += "; " + "; ".join(rest)[:200]
    return line


def cleanup_scratch(label, prof, run):
    """end-of-pass cleanup evidence (all platforms; the logs were copied above): remove the scratch HOME and ctl dirs, then audit for leftover
    processes whose command line names this run dir. Prints what it did; never raises."""
    try:
        gone, kept = [], []
        for d in (prof.home, prof.ctl, prof.rctl, prof.refctl, prof.rhome):
            if not os.path.isdir(d):
                continue
            shutil.rmtree(d, ignore_errors=True)
            (kept if os.path.exists(d) else gone).append(os.path.basename(d))
        print("teardown[%s]: scratch dirs removed: %s%s" % (label, ",".join(gone) or "none", ("; COULD NOT REMOVE: " + ",".join(kept)) if kept else ""), flush=True)
    except Exception as e:
        print("teardown[%s]: scratch removal failed: %r" % (label, e), flush=True)
    try:
        left = leftover_processes(run)
        print("teardown[%s]: leftover processes naming %s: %s" % (label, run, "; ".join(left) if left else "none"), flush=True)
    except Exception as e:
        print("teardown[%s]: leftover-process audit failed: %r" % (label, e), flush=True)


def leftover_processes(run):
    """['pid command'] of processes (other than this harness and the audit itself) whose command line contains the run dir"""
    if os.name == "nt":
        ps = "Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -like '*%s*' } | ForEach-Object { '{0} {1}' -f $_.ProcessId, $_.CommandLine }" % run.replace("'", "''")
        out = subprocess.run(["powershell", "-NoProfile", "-Command", ps], capture_output=True, text=True, timeout=60).stdout
        rows = [l.strip() for l in out.splitlines() if l.strip()]
        return [l[:200] for l in rows if not l.startswith("%d " % os.getpid()) and "Get-CimInstance" not in l and "run.py" not in l]
    out = subprocess.run(["ps", "-axo", "pid=,command="], capture_output=True, text=True, timeout=30).stdout
    res = []
    for l in out.splitlines():
        l = l.strip()
        if run in l and not l.startswith("%d " % os.getpid()) and "run.py" not in l and not l.split(None, 1)[1].startswith("ps "):
            res.append(l[:200])
    return res


def host_load():
    """'loadavg1=1.23 cpus=12' (Windows has no loadavg: cpus only)"""
    try:
        la = "loadavg1=%.2f " % os.getloadavg()[0]
    except (OSError, AttributeError):
        la = "loadavg1=n/a "
    return la + "cpus=%s" % (os.cpu_count() or "?")


def enter_summary(types):
    """'enter_route: 12 Return(s) at the app: Shell=11 AI=1 [Shell,Shell,...]' from the input_type of every hop=enter_begin of a case"""
    if not types:
        return None
    cnt = {}
    for t in types:
        cnt[t] = cnt.get(t, 0) + 1
    seq = ",".join(types[:40]) + (",..." if len(types) > 40 else "")
    return "enter_route: %d Enter(s) reached the input editor: %s [%s]" % (len(types), " ".join("%s=%d" % kv for kv in sorted(cnt.items())), seq)


def case_evidence(name, r, ctx, log_off):
    """r18e0 validators had only the job log: lines printed under the row for mode_proof (logged view mode + OCR marker booleans), for any case
    whose app log shows Enters (the destination summary) and for every latency row (host load). Never raises."""
    ev = []
    try:
        m = r.metrics or {}
        if name == "mode_proof":
            ev.append("mode_proof: log_view_mode=%s ocr_agent_markers=%s ocr_terminal_hint=%s queue_destination=%s" % (
                m.get("log_view_mode"), m.get("ocr_agent_markers"), m.get("ocr_terminal_hint"), m.get("queue_destination")))
        if log_off is not None:
            import cases_caseagent as ca
            es = enter_summary(ca.enter_begin_types(ctx.log_since(log_off)))
            if es:
                ev.append(es)
        if name.startswith("latency") or m.get("lat"):
            ev.append("host load after the case: " + host_load())
            r.metrics["host_load"] = host_load()
    except Exception as e:
        ev.append("evidence capture failed: %r" % (e,))
    return ev


def write_partial_report(run, a):
    """r18d8: regenerate results.json / summary.md / junit.xml from results.jsonl after EVERY case (report.py output and exit status
    are ignored here; finish() still writes the final report and decides the exit code). Never raises."""
    try:
        subprocess.run(report_cmd(run, a, str(profile.Profile.owner_note)) + ["partial=1"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=120)
    except Exception:
        pass


def report_cmd(run, a, note):
    here = os.path.dirname(os.path.abspath(__file__))
    return [sys.executable, os.path.join(here, "report.py"), run, "platform=" + a.platform, "backend=" + a.backend,
            "app_display=" + a.app_display, "seed=%d" % a.seed, "commit=" + os.environ.get("CI_COMMIT_SHORT_SHA", "local"),
            "owner_settings=" + note, "tier=" + (os.environ.get("APPCI_TIER") or "full"),
            "scale=" + os.environ.get("APPCI_SCALE", "1")]


def finish(run, a, code, write_meta=True):
    here = os.path.dirname(os.path.abspath(__file__))
    note = str(profile.Profile.owner_note)
    if write_meta and a.backend == "windows":      # windows/ci/app_test.ps1 adds its restore row afterwards and calls `run.py --report-only`, which needs the same meta
        try:
            json.dump(dict(platform=a.platform, backend=a.backend, app_display=a.app_display, seed=a.seed, owner_note=note, run=run,
                           bin="", report_only=False), open(os.path.join(run, "report_meta.json"), "w"))
        except OSError:
            pass
    else:
        note = getattr(a, "owner_note", note)
    rc = subprocess.run(report_cmd(run, a, note)).returncode
    return max(code, rc)


if __name__ == "__main__":
    sys.exit(main())
