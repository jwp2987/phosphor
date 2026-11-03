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
import argparse, json, os, shutil, subprocess, sys, time, traceback

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import backend, cases, profile, sshd, winmap, app as appmod  # noqa: E402


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
    ap.add_argument("--report-only", action="store_true", help="only (re)generate results.json / junit.xml / summary.md from results.jsonl")
    a = ap.parse_args()
    if a.app_display is None:
        a.app_display = {"mac": "macos", "windows": "windows"}.get(a.backend, "wayland")
    if not a.report_only and not a.backend:
        ap.error("the following arguments are required: --backend")
    import signal
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))     # cancelled job: unwind through the finally blocks (app.stop, sshd.stop)
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
    if win:
        import winmap
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
        print("[%s] %-24s %s %s" % (row["mode"], row["name"], row["status"].upper(), "; ".join(row["detail"])[:200]), flush=True)

    be = None if sim else backend.make(a.backend)
    if be:
        problems = be.preflight()
        if problems:
            for m in a.modes.split(","):
                emit(dict(name="preflight", mode=m, status="fail", detail=problems, seconds=0))
            return finish(run, a, 2)
    explicit_cases = bool(a.cases)
    if not a.cases and os.environ.get("APPCI_TIER") == "smoke":
        a.cases = ",".join(cases.SMOKE)
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
    for mode, pane in passes:
        # per-mode fresh profile + app: settings differ (default_session_mode). Windows: once per pane shell too (label = terminal | terminal@gitbash)
        label = mode if pane in (None, "powershell") else "%s@%s" % (mode, pane)
        prof.pane = pane or prof.pane
        ssh_info = None
        ssh_why = None       # why the ssh cases skip when there is no usable ssh test host
        app = None
        instance = 1       # app instance number within this mode: each instance's log is kept as phosphor.<n>.log (#1457)
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
                if a.backend in ("mac", "windows"):
                    be.pid_fn = app.pid           # the guard compares the frontmost PID with THIS pid before every injection
            ctx = cases.Ctx(be, prof, app, run, mode, a.seed, ssh_info, bool(ollama_url), log_path, ssh_why, pane=pane)
            tier_names = None
            if win and not explicit_cases:       # the Windows fix tier is a table (winmap.FIX_TIER); an explicit --cases list always wins, the full tier is untouched
                tier_names, ctx.opts = winmap.tier_plan("windows", os.environ.get("APPCI_TIER"), mode, pane, cases.SMOKE)
            if not sim:
                up = await_shell(ctx, app, prof, win, a.startup_timeout)
                if not up:
                    emit(dict(name="launch", mode=label, status="fail", seconds=a.startup_timeout,
                              detail=["no shell prompt within %ds (see logs/app.stdout; GPU/Wayland init or keyring?%s)" % (a.startup_timeout, "; Windows: no main window of pid %s, or the %s pane's shell never started" % (app.pid(), pane) if win else "")]))
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
                status = "pass"
                disarm = arm_case_timeout(case_timeout)
                try:
                    if sim and c["gui"]:
                        raise cases.Skip("needs a GUI backend")
                    if not sim:
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
                shot = None if sim else ctx.shot(c["name"] + ("_FAIL" if status == "fail" else ""))
                dead = app.death() if hasattr(app, "death") else (None if app is None or app.alive() else "APP DIED during/after this case (crash): see logs/app.stdout")
                if dead:
                    status = "fail"
                    r.fails.append(dead)
                    emit(dict(name=c["name"], mode=label, status=status, seconds=round(time.time() - t0, 1),
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
                emit(dict(name=c["name"], mode=label, status=status, seconds=round(time.time() - t0, 1),
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
                app.stop()
            sshd.stop(ssh_info)
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
            for src, dst in ((log_src(prof, a.backend, app), "phosphor.log"), (run + "/app.stdout", "app.stdout"),
                             (run + "/ssh/sshd.log", "sshd.log"), (run + "/ssh/fakesshd.stderr", "fakesshd.stderr")):
                if os.path.exists(src):
                    t = ci_env.redact(app_log_text(src) if dst == "phosphor.log" else open(src, errors="replace").read(), ollama_url)   # the endpoint never reaches an artifact
                    open(os.path.join(logs, dst), "w").write(t)
            if os.path.exists(run + "/app.stdout"):
                os.remove(run + "/app.stdout")
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
    rc = subprocess.run([sys.executable, os.path.join(here, "report.py"), run, "platform=" + a.platform, "backend=" + a.backend,
                         "app_display=" + a.app_display, "seed=%d" % a.seed, "commit=" + os.environ.get("CI_COMMIT_SHORT_SHA", "local"),
                         "owner_settings=" + note, "tier=" + (os.environ.get("APPCI_TIER") or "full"),
                         "scale=" + os.environ.get("APPCI_SCALE", "1")]).returncode
    return max(code, rc)


if __name__ == "__main__":
    sys.exit(main())
