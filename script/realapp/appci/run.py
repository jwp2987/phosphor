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
import backend, cases, deadline, injacct, lab as labmod, profile, sshd, winmap, app as appmod  # noqa: E402


def skip_status(r, why):
    """Status for a case that raised Skip. A failed check recorded before the Skip is NEVER downgraded: the FAIL stands and says it was also skipped (#1498)."""
    if r.fails:
        r.notes.insert(0, "ALSO SKIPPED after the failed check(s) above: %s" % why)
        return "fail"
    if isinstance(why, cases.SettingsMismatch):  # round 16: the app does not run with a setting the case needs: not judged (NOREF), never a pass, never a failure
        r.noref.append("UNVERIFIABLE: " + str(why))
        r.notes.insert(0, "NO-REFERENCE: UNVERIFIABLE: " + str(why))
        return "noref"
    if isinstance(why, cases.HostMissing):      # #2043: a required test host did not start: not judged (NOREF, the trend report lists it), never a plain skip
        r.noref.append(str(why))
        r.notes.insert(0, "NO-REFERENCE: " + str(why))
        return "noref"
    r.notes.insert(0, str(why))
    return "skip"


def unknown_case_names(requested, registry_names):
    """names in a comma list (--cases / --skip) that no registered case has (#1508: a misspelling used to select nothing and exit 0)"""
    import fnmatch      # --cases / --skip also take fnmatch globs (lab_* arms): a glob is unknown only when it matches no registered case
    return [n for n in (requested or "").split(",") if n and not any(fnmatch.fnmatchcase(r, n) for r in registry_names)]


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
    fix = cases.SMOKE + cases.PROVING       # the fix tier runs SMOKE + PROVING (owner rule 2026-11-09)
    fix = fix + winmap.PROVING_WINDOWS      # r15 wave 4 (#2060): plus the older cases whose Windows arm is new (Windows fix tier only)
    n = {}
    for mode in ("terminal", "agent"):
        for pane in ("powershell", "gitbash"):
            names, o = winmap.tier_plan("windows", "smoke", mode, pane, smoke, cases.PROVING)
            ran = [x for x in names if mode in [c for c in cases.CASES if c["name"] == x][0]["modes"] and not winmap.skip_reason(x, pane)]
            n[(mode, pane)] = len(ran)
            assert all(x in fix for x in names), "a fix-tier case outside SMOKE + PROVING"
            assert all(x in names for x in cases.PROVING), "a PROVING case missing from the Windows fix-tier plan (it must run or SKIP with a reason)"
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
    secret_url = ollama_url      # the configured (owner machine-local) endpoint: redacted from every artifact even when a pass uses the fake model instead
    # APPCI_FAKE_MODEL=1 FORCES the fake (owner rule 2026-11-09, lab jobs): the CI machines carry the owner's Ollama settings, which used to win and made every
    # fake-model case SKIP there. L2 fake model (fake_model.py): a local Ollama-protocol server, never a real provider
    if os.environ.get("APPCI_FAKE_MODEL") == "1":
        import fake_model
        os.environ.setdefault("APPCI_FAKE_MODEL_LOG", os.path.join(run, "fake_model_requests.jsonl"))      # #1829: what the model was sent, per request
        fake = fake_model.FakeModel().start()
        ollama_url, ollama_model = fake.url, "fake-model"
        os.environ["APPCI_FAKE_MODEL_ACTIVE"] = "1"
        print("fake model: a local scripted provider is the agent provider for this run (APPCI_FAKE_MODEL=1)", flush=True)
    real_url, real_model = ollama_url, ollama_model      # the provider of the ordinary passes (the fake when forced above); the fake pass below swaps ollama_url per pass and back
    if os.environ.get("APPCI_FAKE_EMBEDDINGS") == "1":      # P9b: the user's BYOP /embeddings provider for the remote-index lab case (loopback, no outgoing connection)
        elog = os.path.join(run, "fake_embeddings.log")
        open(elog, "w").close()
        fe = subprocess.Popen([sys.executable, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "fake_embeddings.py"), "--log", elog],
                              stdout=subprocess.PIPE, text=True)
        port = int(fe.stdout.readline().strip())
        os.environ["APPCI_FAKE_EMBEDDINGS_URL"] = "http://127.0.0.1:%d/v1" % port
        os.environ["APPCI_FAKE_EMBEDDINGS_LOG"] = elog
        os.environ["APPCI_FAKE_EMBEDDINGS_ACTIVE"] = "1"
        os.environ.setdefault("ZAP_UNSTABLE_FEATURES", "full_source_code_embedding")
        print("fake embeddings: a loopback /embeddings provider is configured for this run (APPCI_FAKE_EMBEDDINGS=1)", flush=True)
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
        print(ci_env.redact(row_log_line(row), secret_url or ollama_url), flush=True)
        for e in (row.get("evidence") or []):
            print(ci_env.redact("    evidence: " + e, secret_url or ollama_url), flush=True)
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
    refused = cases.retired_dest_switch()
    if refused is not None:      # #1353 step 8: the legacy / shadow modes are gone; the app says so in its log and runs the table
        print("WARNING: PHOSPHOR_QUEUE_DESTINATION=%s asks for a destination mode the app no longer has; the app ignores it (one log line) and this is a table run" % refused, flush=True)
    print("destination logic of the app under test: %s (the only one, #1353 step 8)" % dest, flush=True)
    if not a.cases and os.environ.get("APPCI_TIER") == "smoke":
        # fix tier = SMOKE + PROVING (owner rule 2026-11-09: a new case runs here until it has passed)
        a.cases = ",".join(winmap.smoke_for(dest, cases.SMOKE + cases.PROVING))
    reg = {c["name"] for c in cases.CASES}
    bad_names = unknown_case_names(a.cases, reg) + unknown_case_names(a.skip, reg)
    if bad_names:       # #1508: an unknown name is an error, never an empty (green) selection
        for m in a.modes.split(","):
            emit(dict(name="case_selection", mode=m, status="fail", seconds=0, detail=["unknown case name(s) %s (not in the registry: renamed or misspelled?)" % ", ".join(bad_names)]))
        return finish(run, a, 2)
    # fake-ssh-lab cases (cases_lab.py, docs/testing.md 2.15) run ONLY when --cases names them (the app-test-lab-* jobs pass 'lab_*'), and only the arms
    # APPCI_LAB_MATRIX selects; the ordinary jobs never start a lab container
    lab_sel = labmod.matrix_selection()
    exact_names = {n for n in a.cases.split(",") if n and not any(ch in n for ch in "*?[")}      # an arm named exactly bypasses the matrix filter; globs are filtered by it
    sel = [c for c in cases.CASES if ((not a.cases and not c.get("lab")) or (a.cases and labmod.name_matches(a.cases, c["name"])))
           and not labmod.name_matches(a.skip, c["name"]) and (not c.get("lab") or c["name"] in exact_names or labmod.arm_selected(c["arm"], lab_sel))
           and (not c["name"].startswith("lab_replay") or "lab_replay" in a.cases)]      # the 15-60 min real-use replay is never swept in by 'lab_*'; only a pattern naming it runs it
    if a.cases and not sel:       # never a silent empty (green) selection: the matrix or --skip removed everything the names matched
        for m in a.modes.split(","):
            emit(dict(name="case_selection", mode=m, status="fail", seconds=0, detail=["--cases %r selected no case (APPCI_LAB_MATRIX=%s and --skip removed them all)" % (a.cases, os.environ.get("APPCI_LAB_MATRIX", "quick"))]))
        return finish(run, a, 2)
    sel.sort(key=lambda c: c["order"])
    exit_fail = False
    crashed_exit = None
    case_timeout = float(os.environ.get("APPCI_CASE_TIMEOUT", "1500"))
    passes = [(m, p, False) for m in a.modes.split(",") for p in panes]
    # The fake pass (owner rule 2026-11-09): in the fix tier the PROVING cases that need the scripted fake model (cases.PROVING_FAKE) run in their OWN app
    # launch ("agent@fake", own profile) whose provider is a local fake_model.py; the other passes keep the configured provider, so the SMOKE agent cases are
    # undisturbed. No fake pass when the whole run already is the fake (APPCI_FAKE_MODEL_ACTIVE), nor when --cases is explicit. A real provider is never sent anything by these cases.
    fake_plan = fake_pass_plan(a.cases if os.environ.get("APPCI_TIER") == "smoke" and not explicit_cases else None, a.modes.split(","),
                              os.environ.get("APPCI_FAKE_MODEL_ACTIVE") == "1")
    fake_server = [None]
    if fake_plan:
        passes += [("agent", p, True) for p in panes]
    clock = deadline.Clock()      # the job's own time budget: no new case starts when too little is left (r18e0: killed by the job timeout, artifacts lost)
    print("[deadline] " + clock.describe(), flush=True)

    def case_limit(name):
        if name.startswith("lab_replay"):         # the real-use replay runs APPCI_REPLAY_MINUTES (15) plus its setup and drain
            return max(case_timeout, float(os.environ.get("APPCI_REPLAY_MINUTES", "15")) * 60 * 1.6 + 900)
        return max(case_timeout, cases.cases_robust.soak_minutes() * 60 + 600) if name == "soak" else case_timeout      # 60 min; 10 in the fix tier (T49)

    for mode, pane, fake in passes:
        # per-mode fresh profile + app: settings differ (default_session_mode). Windows: once per pane shell too (label = terminal | terminal@gitbash)
        # #1534: "pwsh7" / "cmd" are startup_shell_override passes: `variant` names the pass (label, settings, case list), `pane` is the shell FAMILY every
        # pane-aware helper below keys on (pwsh 7 = powershell, cmd.exe = cmd)
        variant = pane if pane in winmap.VARIANT_FAMILY else None
        pane = winmap.VARIANT_FAMILY.get(pane, pane)
        label = mode if (pane in (None, "powershell") and not variant) else "%s@%s" % (mode, variant or pane)
        if fake:
            label += "@fake" if label == mode else "+fake"      # agent@fake (Linux/macOS), agent@gitbash+fake (Windows)
            if fake_server[0] is None:
                import fake_model
                os.environ.setdefault("APPCI_FAKE_MODEL_LOG", os.path.join(run, "fake_model_requests.jsonl"))
                fake_server[0] = fake_model.FakeModel().start()
            ollama_url, ollama_model = fake_server[0].url, "fake-model"
            os.environ["APPCI_FAKE_MODEL_ACTIVE"] = "1"
            print("fake model: the %s pass uses the local scripted provider (fake pass of the fix tier); the configured provider is not used" % label, flush=True)
        else:
            ollama_url, ollama_model = real_url, real_model
            if fake_plan:      # the fake model is on only in its own pass (it is never the global provider when a plan exists)
                os.environ.pop("APPCI_FAKE_MODEL_ACTIVE", None)
        psel = [c for c in sel if fake_pass_ok(c["name"], fake, fake_plan)]
        prof.pane = pane or prof.pane
        prof.variant = variant
        if variant:
            import winapi
            # r15 (T46): a pass none of whose cases is selected (an explicit --cases list, the lab job) starts no app at all
            vnames = [c["name"] for c in psel if c["name"] in winmap.variant_cases(variant) and mode in c["modes"]]
            if not vnames:
                print("[appci] pass %s: none of its cases is selected, not started" % label, flush=True)
                continue
            exe = {"pwsh7": winapi.pwsh7, "cmd": winapi.cmd_exe, "wsl": winapi.wsl_distro}[variant]()
            if not exe:
                emit(dict(name="startup_shell_pass", mode=label, status="skip", seconds=0, metrics={}, shots=[],
                          detail=["%s is not installed on this machine: the %s pass cannot run (the harness installs nothing). %s. Not run: %s" % (
                              {"pwsh7": "PowerShell 7 (pwsh.exe)", "cmd": "cmd.exe", "wsl": "a WSL distribution"}[variant], variant, winmap.VARIANT_NEEDS[variant], ", ".join(vnames))]))
                continue
        ssh_info = None
        ssh_why = None       # why the ssh cases skip when there is no usable ssh test host
        lab_obj, lab_why = None, None      # the fake ssh lab (labmod.Lab) of this pass, and why there is none
        app = None
        instance = 1       # app instance number within this mode: each instance's log is kept as phosphor.<n>.log (#1457)
        if not clock.allows(0)[0]:
            # a pass that would start with (almost) no budget left: no app launch, every case of the pass gets its explicit skip row
            why = clock.allows(0)[1]
            print("[deadline] %s: pass %s not started: %s" % (time.strftime("%H:%M:%S"), label, why), flush=True)
            clock.logged_stop = True
            tn = None
            if win and not explicit_cases:
                tn, _o = winmap.tier_plan("windows", os.environ.get("APPCI_TIER"), mode, pane, cases.SMOKE, cases.PROVING)
            for c in psel:
                if mode in c["modes"] and (tn is None or c["name"] in tn):
                    emit(dict(name=c["name"], mode=label, status="skip", seconds=0, detail=[why], metrics={}, shots=[]))
            continue
        try:
            prof.build(mode, ollama_url, ollama_model or 'gpt-oss:20b')
            ssh_info = None if sim else sshd.start(prof, run, app_bin=a.bin)
            if ssh_info is None and win and sshd.WHY:
                ssh_why = sshd.WHY
                print("no ssh test host on Windows, the ssh cases cannot run: %s" % ssh_why, flush=True)
            if ssh_info:
                prof.build(mode, ollama_url, ollama_model or 'gpt-oss:20b', ssh_port=ssh_info["port"], ssh_ident=ssh_info["ident"], ssh_pw=ssh_info.get("pwhost"))
                ssh_why = prof.ssh_probe()
                if ssh_why:
                    print("ssh probe failed, ssh cases will skip: %s" % ssh_why, flush=True)
                    sshd.stop(ssh_info)
                    ssh_info = None
            ssh_missing = ssh_info is None and not sim and sshd.required()
            if ssh_missing:
                # #2043: rounds 10 and 11 lost every Windows ssh case to a quiet SKIP. Where the host is required its absence is a FAIL row of its
                # own (the job goes red, the reason and the server's stderr are in the row) and the ssh cases of the pass are NOREF.
                row = sshd.host_missing_row(label, ssh_why, os.path.join(run, "ssh", "fakesshd.stderr") if win else None)
                print("SSH TEST HOST MISSING [%s]: %s" % (label, " ;; ".join(row["detail"])), flush=True)
                emit(row)
                exit_fail = True
            if any(c.get("lab") and mode in c["modes"] and (not win or not winmap.skip_reason(c["name"], pane)) for c in psel):
                lab_obj, lab_why = labmod.Lab.start(run)
                if lab_obj:
                    try:
                        lab_obj.lease()
                        lab_obj.write_ssh(prof.home)
                        print("[lab] %s lab up (%s); fault ports only, control port untouched" % (lab_obj.mode, lab_obj.info.get("image")), flush=True)
                    except Exception as e:
                        lab_why = "the lab started but the lease/config failed: %r" % (e,)
                        lab_obj.stop()
                        lab_obj = None
                if lab_obj is None:
                    print("[lab] no lab for this pass, the lab cases will skip: %s" % lab_why, flush=True)
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
            ctx = cases.Ctx(be, prof, app, run, mode, a.seed, ssh_info, bool(ollama_url), log_path, ssh_why, pane=pane, variant=variant)
            ctx.ssh_required = ssh_missing
            ctx.lab, ctx.lab_why = lab_obj, lab_why
            if win:       # #1540/#1541: a case that relaunches the app by itself re-proves the keys and re-sets the pane state exactly as the relaunch branch below does
                ctx.reprove = lambda: prove_keys(ctx, be, pane, label)
            tier_names = None
            if win and not explicit_cases:       # the Windows fix tier is a table (winmap.FIX_TIER); an explicit --cases list always wins, the full tier is untouched
                tier_names, ctx.opts = winmap.tier_plan("windows", os.environ.get("APPCI_TIER"), mode, pane, cases.SMOKE, cases.PROVING)
            if variant:       # #1534: a startup_shell_override pass runs only its own list (winmap.VARIANT_CASES); an explicit --cases list narrows it further
                tier_names = list(winmap.variant_cases(variant))
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
                if win:
                    ctx.log_path = os.environ.get("APPCI_APP_LOG") or app.find_log() or ""      # now that the app has written one
                if variant:
                    # r15 (T46, #2084): did the app start the shell this pass is about? Read from the app's own job object, before any key is typed:
                    # a pane that fell back to the default shell would get cmd.exe / bash lines and the pass would end in "keys not delivered".
                    vbad = variant_pane_problem(variant, app)
                    if vbad:
                        emit(dict(name="startup_shell_pass", mode=label, status="fail", seconds=0, metrics={}, shots=[ctx.shot("startup_shell_pass_%s" % variant) or ""],
                                  detail=[vbad + ". No case of this pass was run: %s" % ", ".join(vnames)]))
                        exit_fail = True
                        continue
                settle_first_run(ctx)      # the first-run overlay: the pane's view_mode hop, then a short margin (mode_proof sends Escape)
                if a.backend == "mac":
                    d = be.system_dialog(ocr=True)       # BEFORE the first key: a Local Network / TCC prompt would take it
                    if d:
                        raise be.needs_owner(d)
                    if app.instance_count() != 1:
                        raise RuntimeError("expected exactly ONE app instance for this job's bundle, found %d: %s" % (app.instance_count(), app.warn))
                be.calibrate()
                if not win:        # Windows records it in prove_keys, after the key proof put the pane in the case dir (audit #1b/B)
                    bad = record_root_pid(ctx, app, "launch")
                    if bad:
                        emit(dict(name="depth_check_off_after_launch", mode=label, status="skip", seconds=0, detail=[bad], metrics={}, shots=[]))
            else:
                ctx.until(lambda: prof.prompts() > 0, 10)
            proved = not win
            for c in psel:
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
                    why = None if variant else winmap.skip_reason(c["name"], pane)
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
                if win and not os.environ.get("APPCI_APP_LOG"):
                    # r18e9: ctx.log_path was resolved ONCE, at the first launch. After a relaunch (the death / pane_not_local / left-agent-mode branches) the new instance
                    # writes a different file, so every later case read a stale or 0-byte log: no app_segments_ms metric on latency_enter, no kh_hold_entry for
                    # ssh_failing_not_local (UNVERIFIABLE), no case log. Re-resolve it before each case and say when it moved.
                    try:
                        nl = app.find_log()
                        if nl and nl != ctx.log_path:
                            print("[appci] app log moved: %s -> %s (relaunched instance)" % (ctx.log_path, nl), flush=True)
                            ctx.log_path = nl
                    except Exception as e:
                        print("[appci] app log re-resolve failed: %r" % (e,), flush=True)
                log_off = ctx.log_size() if (ctx.log_path and not sim) else None      # job-log evidence: the app-side Enter destinations of THIS case
                settings_before = cases.settings_text(ctx)       # round 16: a case that leaves the settings file changed says so in its row (names only)
                status = "pass"
                try:       # r18e9: the app instance this case started on (a case that relaunches the app itself ends in a fresh TERMINAL session)
                    pid_start = app.pid() if app is not None and hasattr(app, "pid") else None
                except Exception:
                    pid_start = None
                # the pane's ROOT shell pid was recorded right after await_shell of this (re)launch (record_root_pid, audit #1b): a probe typed here, before
                # be.guard() and ctx.reset(), once named a previous case's leaked nested shell as the root, and two consistent probes then `exit`ed the real root.
                # Only when a CASE relaunched the app itself (ui_startup_time, remote_session_ext: the pane is a fresh terminal session) is it recorded here, and
                # record_root_pid still refuses unless exactly one shell runs under the app.
                if not sim and not win and pid_start and getattr(ctx, "root_app_pid", None) != pid_start:
                    bad = record_root_pid(ctx, app, "relaunch_by_case")
                    if bad:
                        emit(dict(name="depth_check_off_before_" + c["name"], mode=label, status="skip", seconds=0, detail=[bad], metrics={}, shots=[]))
                depth_check = (not sim) and cases.wants_depth_check(c["name"], cases.PROVING) and not (win and pane in ("cmd", "wsl")) and ctx.root_app_pid == pid_start
                # r18d8: soak runs APPCI_SOAK_MIN (60) minutes, longer than the 1500 s default: it hit CaseTimeout at 25 min. It gets its own length + 10 min.
                disarm = arm_case_timeout(case_limit(c["name"]))
                try:
                    if sim and c["gui"]:
                        raise cases.Skip("needs a GUI backend")
                    cases.requires_declared(ctx, r, c["name"])      # round 16: the pass baseline + the case's declared settings, before its first key
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
                try:
                    drift = cases.settings_left_changed(settings_before, cases.settings_text(ctx))
                    if drift:
                        r.note(drift)
                        print("[appci] %s [%s]: %s" % (c["name"], label, drift), flush=True)
                except Exception as e:
                    r.note("settings comparison failed: %r" % (e,))
                # audit #9 / F (#1916): state a case must have put back before anything else is typed (the screenshot, the probes, the next case)
                leaks = post_case_invariants(ctx, prof, be)
                if leaks:
                    emit(dict(name="harness_state_reset_after_" + c["name"], mode=label, status="fail" if status in ("pass", "noref") else "pass", seconds=0, metrics={}, shots=[],
                              detail=["after %s the harness's own state was not put back and was reset so later cases are not judged against it: %s%s" % (
                                  c["name"], "; ".join(leaks), "" if status not in ("pass", "noref") else " -- the case did not fail, so its cleanup has a hole (TEST defect)")]))
                    if status in ("pass", "noref"):
                        exit_fail = True
                if a.backend == "mac" and not sim:      # #1826: a case that left a harness-started Terminal.app (with windows) running names itself in its row
                    try:
                        import refterm_mac
                        leak = refterm_mac.case_guard(c["name"])
                        if leak:
                            r.note(leak)
                            print("LEAK " + leak, flush=True)
                    except Exception as e:
                        r.note("terminal leak guard failed: %r" % (e,))
                if k_start is not None and not sim:       # every backend that records its calls, Windows included (audit #5)
                    account_case(r, be.calls[k_start:], list(ctx.extra_logs) + [ctx.log_path or log_path] if ctx.extra_logs else (ctx.log_path or log_path), c["name"], time.time(), t0)
                    if status == "pass" and r.noref:
                        status = "noref"
                if log_off is not None:       # r18e9: did the pty write carry '\r'? (an Enter the editor accepted whose write lacked the CR = dropped/held Enter, ssh_* on Windows)
                    try:
                        pw = pty_write_cr(ctx.log_since(log_off))
                        if pw["enters"] or pw["writes"]:
                            r.metrics["pty_write_cr"] = pw
                            if pw["enters"] > pw["with_cr"]:
                                r.note("pty writes: %(writes)d, of which carrying a CR (\\r): %(with_cr)d; Enter(s) the input editor started: %(enters)d (an Enter may ride in a multi-line write; a gap with held/handed-back Enters is the thing to look at)" % pw)
                    except Exception as e:
                        r.note("pty write CR count failed: %r" % (e,))
                tainted = False
                if a.backend == "mac" and not sim:       # r18e9: Returns macOS targeted at the app that its NSApplication never logged taint THIS case's row
                    import taint
                    try:
                        pids = {app.pid()} - {None}
                    except Exception:
                        pids = set()
                    tn, tm = taint.case_taint(run, c["name"], label, t0, time.time(), pids,
                                              ctx.log_since(log_off) if log_off is not None else "", alive=bool(app.alive()) if hasattr(app, "alive") else None)
                    if tm:
                        r.metrics["enter_taint"] = tm
                    if tn:
                        r.note(tn)
                        tainted = True
                for ev in (be.drain_health() if hasattr(be, "drain_health") else []):
                    r.note(ev + " (key loss in this case is the INJECTOR's, not the app's)")
                shot = None if sim else ctx.shot(c["name"] + ("_FAIL" if status == "fail" else ""))
                evidence = case_evidence(c["name"], r, ctx, log_off)
                dead = app.death() if hasattr(app, "death") else (None if app is None or app.alive() else "APP DIED during/after this case (crash): see logs/app.stdout")
                if dead:
                    status = "fail"
                    dead = classify_death(dead, ctx, t0, log_off)
                    r.fails.append(dead)
                    if not sim:        # the dying instance's lines of THIS case, before the relaunch moves the log (audit #10)
                        save_case_log(run, label, c["name"], t0, prof, ctx, app, ollama_url, log_off=log_off, backend_name=a.backend)
                    emit(dict(name=c["name"], mode=label, status=status, seconds=round(time.time() - t0, 1), evidence=evidence, tainted=tainted, wall=[round(t0, 3), round(time.time(), 3)],
                              detail=r.fails + r.notes, metrics=r.metrics, shots=[shot] if shot else []))
                    exit_fail = True
                    instance, proved = relaunch(ctx, app, prof, be, win, a, run, label, ollama_url, instance, emit, "death_of_" + c["name"], stop=False)
                    continue
                # audit #10 (#1917): EVERY platform keeps the case's own app-log slice (it was Windows-only; a long Linux/Mac pass rotated the failing case's
                # lines away before the end-of-pass copy). The slice starts at the case's LogOffset, which follows the log across in-session rotations.
                # r18e9: a PASSING case's slice too (capped to its newest 1 MB), so a validator can check a pass against the app's own hops; a failing or
                # unverifiable case keeps its start (12 MB) plus the ssh server log (r18cj: the evidence was gone).
                if not sim and status not in ("skip",):
                    if status in ("fail", "noref"):
                        save_case_log(run, label, c["name"], t0, prof, ctx, app, ollama_url, log_off=log_off, backend_name=a.backend)
                    else:
                        save_case_log(run, label, c["name"], t0, prof, ctx, app, ollama_url, cap=1_000_000, log_off=log_off, backend_name=a.backend)
                if status == "fail":
                    exit_fail = True
                emit(dict(name=c["name"], mode=label, status=status, seconds=round(time.time() - t0, 1), evidence=evidence, tainted=tainted, wall=[round(t0, 3), round(time.time(), 3)],
                          detail=(r.fails or []) + r.notes, metrics=r.metrics, shots=[os.path.relpath(shot, run)] if shot else []))
                if c["name"].startswith(("ssh_", "remote_")) and cases.post_case_invariants_apply(status):
                    # r18ct: a failed nested-ssh case left the pane inside the remote shell, and the next case (latency_enter) typed a zsh-only
                    # `print` into the remote bash, which agent mode routed to the agent. Every ssh case ends with the pane provably LOCAL.
                    where = ctx.return_to_local()
                    if where != "local":
                        emit(dict(name="pane_not_local_after_" + c["name"], mode=label, status="fail", seconds=0,
                                  shots=[ctx.shot("not_local_after_" + c["name"]) or ""],
                                  detail=["after %s the pane could not be proven back in the local shell (%s); the app is relaunched so later cases start clean" % (c["name"], where)]))
                        instance, proved = relaunch(ctx, app, prof, be, win, a, run, label, ollama_url, instance, emit, "pane_not_local_after_" + c["name"])
                        continue
                if depth_check and ctx.root_pid is not None:
                    try:
                        if pid_start and app.pid() != pid_start:       # the case relaunched the app itself: a fresh pane, its root is recorded at the next case
                            ctx.root_pid = None
                            depth_check = False
                    except Exception:
                        pass
                if depth_check and ctx.root_pid is not None and cases.post_case_invariants_apply(status):
                    # r6 (Mac fix tier): a subshell_ready case left the pane in a LOCAL bash subshell (same HOME, so where_now saw "local") and every later case
                    # typed into it. After a checked case the shell in front must be the pane's ROOT shell (its PID); `exit` only while a probe proves otherwise.
                    nest = ctx.leave_nested(ctx.root_pid)
                    if nest in ("nested", "none"):
                        emit(dict(name="pane_not_local_after_" + c["name"], mode=label, status="fail", seconds=0,
                                  shots=[ctx.shot("not_local_after_" + c["name"]) or ""],
                                  detail=["after %s the pane's shell was not the root shell (%s; root pid %s); the app is relaunched so later cases start clean" % (c["name"], nest, ctx.root_pid)]))
                        instance, proved = relaunch(ctx, app, prof, be, win, a, run, label, ollama_url, instance, emit, "pane_not_root_after_" + c["name"])
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
                        instance, proved = relaunch(ctx, app, prof, be, win, a, run, label, ollama_url, instance, emit, "agent_view_not_restored_after_" + c["name"])
                        continue
                    if seen == "terminal" and ctx.double_ctrlc_since(t0):
                        # r18d2: two Ctrl-C within the 1 s confirmation window of an empty agent conversation leave agent view BY DESIGN
                        # (view.rs should_ctrl_c_exit_agent_view -> exit_agent_view_with_required_confirmation). A visible row, not a failure; the app is
                        # still relaunched so the cases after this one run in agent mode.
                        emit(dict(name="agent_view_left_by_double_ctrl_c_after_" + c["name"], mode=label, status="pass", seconds=0,
                                  shots=[os.path.relpath(p, run) for p in [ctx.shot("left_agent_after_" + c["name"])] if p],
                                  detail=["after %s the pane is a terminal session, and the case sent two Ctrl-C within the 1 s exit-confirmation window: that exits an empty "
                                          "agent conversation by design, so this is not an app finding; the app is relaunched (fresh agent session)" % c["name"]]))
                        instance, proved = relaunch(ctx, app, prof, be, win, a, run, label, ollama_url, instance, emit, "double_ctrl_c_after_" + c["name"])
                        continue
                    if seen == "terminal":
                        emit(dict(name="left_agent_mode_after_" + c["name"], mode=label, status="fail", seconds=0,
                                  shots=[os.path.relpath(p, run) for p in [ctx.shot("left_agent_after_" + c["name"])] if p],
                                  detail=["after %s the pane shows a TERMINAL session, not agent view; the app is relaunched (fresh agent session) so later "
                                          "agent cases measure agent mode. Candidate APP finding if the case typed no Escape." % c["name"]]))
                        exit_fail = True
                        instance, proved = relaunch(ctx, app, prof, be, win, a, run, label, ollama_url, instance, emit, "left_agent_mode_after_" + c["name"])
                        continue
                # audit #12 (#1919): after ANY failing case on every platform (Windows-only before): on POSIX a program left in the foreground that ignores
                # Ctrl-C (vim, less, cat) silently failed every later case. pane_responsive is an effect probe and platform-neutral.
                alive = ctx.pane_responsive() if (not sim and (status == "fail" or (win and c["name"] in ("ctrl_c_d", "cadence")))) else 0
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
                    instance, proved = relaunch(ctx, app, prof, be, win, a, run, label, ollama_url, instance, emit, "pane_wedged_after_" + c["name"])
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
            if lab_obj:
                lab_obj.stop()
                print("teardown[%s]: lab container %s stopped" % (label, lab_obj.cid[:12]), flush=True)
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
                        save_old_rotations(src, logs, "phosphor", ollama_url)
            if dest_log is not None:
                # #1353 step 8: the run-wide oracle of "the table is the only destination path". A plan no executor had a leaf for used to
                # re-run the legacy chain; now the key is not applied, and the app says so at error level. One such line fails the pass
                # (a row is added only then: a clean pass has no row of this name).
                no_leaf = cases.table_no_leaf_lines(dest_log)
                if no_leaf:
                    emit(dict(name="table_no_leaf", mode=label, status="fail", seconds=0,
                              detail=["%d destination plan(s) had no leaf in the app log (a key that was not applied): %s" % (len(no_leaf), no_leaf[0][-200:])],
                              metrics={"queue_destination": dest, "table_no_leaf": len(no_leaf)}, shots=[]))
                    exit_fail = True
            if os.path.exists(run + "/app.stdout"):
                os.remove(run + "/app.stdout")
            if not sim:
                cleanup_scratch(label, prof, run)
        if crashed_exit is not None:
            break
        if not win and not exit_fail:
            # #1508: every selected case that applies to this mode must have a row (pass, fail or skip), else the run did less than asked
            missing = [c["name"] for c in psel if mode in c["modes"] and (label, c["name"]) not in emitted]
            if missing:
                emit(dict(name="cases_not_run", mode=label, status="fail", seconds=0, detail=["selected case(s) produced no row: %s" % ", ".join(missing[:12])]))
                exit_fail = True
    if crashed_exit is not None:
        finish(run, a, 143)
        raise SystemExit(crashed_exit)
    return finish(run, a, 1 if exit_fail else 0)


PTY_WRITE_RX = re.compile(r'hop=pty_write_bytes .*?bytes="((?:[^"\\]|\\.)*)"')


def post_case_invariants(ctx, prof, be):
    """audit #9 / F (#1916): harness state that must be back to its resting value when a case returns, however it returned. A case that raised between
    switching to the reference terminal and closing it left prof.ref_active / be.guard_override set: every later prompts()/commands() then read the
    REFERENCE shell's logs and every injection was guarded against the (closed) reference window, a cascade of false fails to the end of the pass. A
    lab arm that did not reach its cleanup left ctx.remote / prof.remote on the lab user. Each is reset here and named. -> [what was reset] (empty =
    clean). Never raises."""
    import lab as labmod_
    out = []
    try:
        if getattr(prof, "ref_active", False):
            prof.ref_active = False
            out.append("prof.ref_active was still True (shell logs were being read from the reference terminal)")
        if be is not None and getattr(be, "guard_override", None):
            be.guard_override = None
            out.append("be.guard_override was still set (keys were still aimed at the reference terminal)")
        arm = getattr(ctx, "arm", None)
        if arm is not None or not isinstance(getattr(ctx, "remote", None), labmod_.LocalRemote) or getattr(prof, "remote", None) is not None:
            ctx.arm = None
            ctx._lab_prev = None
            ctx.remote = labmod_.LocalRemote(prof)
            prof.remote = None
            who = (arm.get("name") or arm.get("user")) if isinstance(arm, dict) else None
            out.append("the remote side still pointed at a lab arm (%s)" % (who or ("arm set" if arm is not None else "ctx.arm unset, but ctx.remote / prof.remote was not the same-machine remote")))
        # r15 wave 4 (#2060): the Windows pointer. A button or a modifier a case left down is released here and named.
        held = be.release_held() if be is not None and hasattr(be, "release_held") else []
        if held:
            out.append("the injector still held %s (released)" % ", ".join(held))
        if out and be is not None and hasattr(be, "refocus"):
            try:
                be.refocus()
            except Exception as e:
                out.append("refocus after the reset failed: %r" % (e,))
    except Exception as e:
        out.append("post-case invariant check itself failed: %r" % (e,))
    return out


def account_case(r, calls, log_path, name, t_end, t_floor=None):
    """injector accounting for one case (#1509): the note + metric, read over the window that ends at the case's end (`t_end`), and (audit #5,
    #1912) a case with no failure whose injected keys or Enters never reached the app's key trace, outside the loss cases, becomes UNVERIFIABLE
    (noref): it passed on its effect check while keys were lost. A case that already failed keeps its own verdict. Never raises."""
    try:
        txt, acct = injacct.account(calls, log_path, name, t_end, t_floor)
        if txt:
            r.note("injector accounting: " + txt)
            r.metrics["injector"] = acct
        bad = injacct.loss_verdict(acct, name)
        if bad and not r.fails:
            r.unverifiable(bad)
    except Exception as e:
        r.note("injector accounting failed: %r" % e)


def pty_write_cr(text):
    """r18e9: {writes, with_cr, enters} from a case's app-log slice: hop=pty_write_bytes lines, how many carried a CR (`\\r` in the logged bytes), and the fresh
    (not deferred) hop=enter_begin count. Both reports asked for it: an Enter the editor accepted but no pty write carried a CR."""
    writes = with_cr = enters = 0
    for l in text.splitlines():
        m = PTY_WRITE_RX.search(l)
        if m:
            writes += 1
            if "\\r" in m.group(1):
                with_cr += 1
        elif "hop=enter_begin deferred=false" in l:
            enters += 1
    return dict(writes=writes, with_cr=with_cr, enters=enters)


CRASH_RX = re.compile(r"panicked at|\bSIGSEGV\b|\bSIGABRT\b|\bSIGBUS\b|fatal runtime error|stack overflow|Segmentation fault|EXC_BAD_ACCESS", re.I)


LAUNCHD_EXIT_RX = re.compile(r"exited (due to (SIG[A-Z0-9]+)(?: \| sent by ([^,]+))?|with exit code:? -?\d+)")
FATAL_LINE_RX = re.compile(r"^.*\[ERROR\] fatal signal .*$", re.M)


def launchd_exit_record(text, pid):
    """#1782 (r18u26 macOS shell_nu): the app died with no app-log line and no crash report; only launchd's line in the unified log said why:
    `[gui/501/application.dev.phosphor...[94694]:] exited due to SIGBUS | sent by exc handler[94694], ran for 1251994ms`. Returns
    (what, signal) for `pid` from `log show` output, e.g. ("due to SIGBUS | sent by exc handler[94694]", "SIGBUS"), or (None, None)."""
    for l in text.splitlines():
        if "[%d]" % pid not in l:
            continue
        m = LAUNCHD_EXIT_RX.search(l)
        if m:
            return m.group(1).strip(), m.group(2)
    return None, None


def mac_exit_evidence(pid, t0, timeout=120):
    """macOS: what the unified log says about the dead `pid` since the case began: launchd's exit record (the signal and who sent it: a fault is
    `sent by exc handler[<pid>]`, a kill names the sender, jetsam says so) and the app's own last system-log lines (r18u26: AppKit's NSPasteboard
    read 1 ms before the SIGBUS). Read only; bounded; '' when `log` is unavailable."""
    start = time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(t0 - 5))
    pred = 'processIdentifier == %d OR (process == "launchd" AND eventMessage CONTAINS "[%d]") OR (process == "kernel" AND eventMessage CONTAINS "%d")' % (pid, pid, pid)
    try:
        out = subprocess.run(["log", "show", "--style", "compact", "--start", start, "--predicate", pred],
                             capture_output=True, text=True, timeout=timeout).stdout
    except Exception as e:
        return "log show failed: %r" % (e,), None
    what, sig = launchd_exit_record(out, pid)
    own = [l.strip()[:200] for l in out.splitlines() if "[%d:" % pid in l][-4:]
    parts = ["launchd: pid %d exited %s" % (pid, what) if what else "launchd logged no exit record for pid %d" % pid]
    if own:
        parts.append("the app's last system-log lines: " + " | ".join(own))
    return "; ".join(parts), sig


def classify_death(dead, ctx, t0, log_off, exit_evidence=None):
    """r18e9 (macOS remote_session_*, shell_zsh): the case's trailing `exit` ran in the pane's own shell, the pane closed, the app quit, and the harness reported
    "APP DIED". Name a pane closed by the case's own `exit` (no panic/signal/crash report in the case's app-log slice, window-close line present) as that, a
    HARNESS defect of the case, never a crash; keep the crash wording otherwise and say what was checked."""
    try:
        sl = ctx.log_since(log_off) if log_off is not None else ""
        # #1782: the app's own fatal-signal line (warp_logging fatal_signal) first: it names the signal, address and thread
        crash = ([m.group(0) for m in FATAL_LINE_RX.finditer(sl)] + [l for l in sl.splitlines() if CRASH_RX.search(l)])[:3]
        reports = []
        if sys.platform == "darwin":       # a real crash leaves a report; a clean quit does not
            import glob
            reports = [p for p in glob.glob(os.path.expanduser("~/Library/Logs/DiagnosticReports/*hosphor*")) if os.path.getmtime(p) >= t0 - 5]
            pm = re.search(r"pid (\d+)", dead)
            if pm:
                # #1782: the exit status the harness cannot wait for (`open` launches the app under launchd): launchd's record of it
                ev, sig = (exit_evidence or mac_exit_evidence)(int(pm.group(1)), t0)
                if ev:
                    dead = "%s; %s" % (dead, ev)
                if sig:
                    reports.append("launchd: exited due to %s" % sig)
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


def case_log_text(ctx, log_off, src, t0):
    """the app-log lines of ONE case. With the case's LogOffset (ctx.log_size() taken before the case): everything logged after it, across the
    app's in-session rotations (Ctx.log_since), plus the logs of instances the case itself replaced (ctx.extra_logs). Without one (no log path
    when the case began): the old timestamp cut of the whole session log from one second before the case."""
    def ts_cut(path):
        keep = []
        if path and os.path.exists(path):
            cut = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(t0 - 1))
            for l in app_log_text(path).splitlines():
                if l[:20] >= cut or not l[:2] == "20":       # non-timestamped continuation lines ride with their neighbour
                    keep.append(l)
        return "\n".join(keep)
    if log_off is not None and getattr(ctx, "log_path", None):
        parts = []
        for k, p in enumerate(getattr(ctx, "extra_logs", None) or []):
            # a case that relaunched the app: its earlier launches' log copies, oldest first. The first is the instance the case began on, so it is read
            # from the case's offset; the later ones were launched inside the case and are the case's whole.
            try:
                with open(p, "rb") as f:
                    if k == 0 and int(log_off) <= os.path.getsize(p):
                        f.seek(int(log_off))
                    parts.append("=== %s (an earlier launch within this case) ===\n" % os.path.basename(p) + f.read().decode(errors="replace"))
            except OSError:
                pass
        parts.append(ctx.log_since(log_off) or "")
        if src and os.path.exists(src) and os.path.abspath(src) != os.path.abspath(ctx.log_path):
            # Windows: the case's own relaunch writes a NEW log file that ctx.log_path does not name yet (re-resolved before the next case)
            parts.append("=== %s (the instance this case relaunched) ===\n" % os.path.basename(src) + ts_cut(src))
        return "\n".join(parts)
    return ts_cut(src)


def save_case_log(run, label, case, t0, prof, ctx, app, ollama_url=None, cap=12_000_000, log_off=None, backend_name="windows"):
    """Every platform (audit #10; Windows-only before): write the app-log lines of this case (the log rotates and keeps only its newest part, so a
    failing case's key trace was gone by the end of the pass), plus the ssh server's log for a failing case, to logs/<label>/case_<case>.log
    (#1488, r18cj). `log_off`: the case's LogOffset (rotation-aware slice); see case_log_text."""
    import ci_env
    try:
        txt = case_log_text(ctx, log_off, log_src(prof, backend_name, app), t0)
        txt = txt[:cap] if cap >= 12_000_000 else txt[-cap:]       # a failing case keeps its start; a passing one its newest part
        sl = os.path.join(run, "ssh", "sshd.log")
        if os.path.exists(sl) and cap >= 12_000_000:        # the ssh server log rides with FAILING cases only
            txt += "\n=== ssh/sshd.log ===\n" + open(sl, errors="replace").read()[-200_000:]
        logs = os.path.join(run, "logs", label)
        os.makedirs(logs, exist_ok=True)
        with open(os.path.join(logs, "case_%s.log" % case), "w", encoding="utf-8", errors="replace") as fh:      # r18e9: the cp1252 default crashed the whole run on U+FFFD
            fh.write(ci_env.redact(txt, ollama_url))
    except Exception as e:          # evidence capture must never fail the run
        print(("save_case_log: %r" % (e,)).encode("ascii", "backslashreplace").decode("ascii"), flush=True)


def save_old_rotations(src, logs, stem, ollama_url):
    """r18u8: keep EVERY rotation of the app log in the saved artifacts. app_log_text() already folds the `.in_session.N` chunks into phosphor.log, but the
    launch-time rotations (`<log>.old.N`, plain or .gz) were left behind and a relaunch's earlier launches vanished. Each is written next to the log as
    `<stem>.old.N.log` (decompressed and redacted: the endpoint never reaches an artifact). Never raises."""
    import glob, gzip, ci_env
    if not src:
        return []
    saved = []
    for f in sorted(glob.glob(glob.escape(src) + ".old.*")):
        try:
            raw = open(f, "rb").read()
            if f.endswith(".gz"):
                raw = gzip.decompress(raw)
            name = os.path.basename(f)[:-3] if f.endswith(".gz") else os.path.basename(f)
            dst = os.path.join(logs, "%s.old.%s.log" % (stem, name.split(".old.", 1)[1]))
            with open(dst, "w", encoding="utf-8", errors="replace") as fh:
                fh.write(ci_env.redact(raw.decode("utf-8", errors="replace"), ollama_url))
            saved.append(dst)
        except Exception as e:
            print(("save_old_rotations %s: %r" % (f, e)).encode("ascii", "backslashreplace").decode("ascii"), flush=True)
    return saved


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
    the app's main window exists, then the app's own log shows the shell's first prompt report (`hop=precmd_seen`; the pane's PowerShell runs the
    app's hooks) plus a 1 s margin for PSReadLine; the old fixed 12 s settle remains the give-up net when the log shows nothing (audit #11)."""
    if not win:
        return ctx.until(lambda: prof.prompts() > base, timeout, step=0.5)
    if not app.wait_window(60):
        return False
    if prof.local_prompts:
        return ctx.until(lambda: prof.prompts() > base, timeout, step=0.5)
    t0 = time.time()
    lp = os.environ.get("APPCI_APP_LOG") or app.find_log() or ""
    if lp:
        ctx.log_path = lp
    seen = lp and ctx.until(lambda: ctx.precmds_since(t0 - 5) >= 1, min(timeout, 12), step=0.25)
    time.sleep(1.0 if seen else max(0.0, 12 - (time.time() - t0)))
    return app.alive()


def settle_first_run(ctx, timeout=4.0):
    """the first-run settle after a (re)launch: the pane's `hop=view_mode` line (logged when the pane is first shown, #1568) plus a 0.5 s margin for the
    overlay, instead of a fixed 4 s (audit #11); the 4 s stay the give-up net when the log shows none (an older build, no log yet)."""
    if ctx.log_path and ctx.until(lambda: ctx.log_view_mode() is not None, timeout, step=0.2):
        time.sleep(0.5)
        return True
    if not ctx.log_path:
        time.sleep(timeout)
    return False


def record_root_pid(ctx, app, why):
    """audit #1b/B: the pane's ROOT shell pid, ONCE per (re)launch, right after await_shell while nothing else has been typed, behind be.guard(). On
    POSIX the process tree must show exactly ONE shell under the app, else the recorded pid could be a leaked shell's and the depth check would
    `exit` the real root later: the check is then left OFF (root_pid None) and the reason returned for a visible row. None when recorded."""
    try:
        ctx.root_app_pid = app.pid() if app is not None and hasattr(app, "pid") else None
    except Exception:
        ctx.root_app_pid = None
    ctx.root_pid = None
    if ctx.win and ctx.pane in ("cmd", "wsl"):
        return None                      # no PID probe in cmd.exe (nor one the harness can read in a WSL pane): the depth check never runs there
    try:
        ctx.be.guard()
    except RuntimeError as e:
        return "root shell pid not recorded after %s: the injection guard refused (%s); the depth check is off until the next relaunch" % (why, e)
    if not ctx.win:
        n = ctx.shell_count()
        if n is not None and n != 1:
            return "root shell pid not recorded after %s: %d shells under the app where exactly 1 was expected; the depth check is off until the next relaunch" % (why, n)
    pid = ctx.shell_pid()
    if pid is None:
        return "root shell pid not recorded after %s: the PID probe got no answer; the depth check is off until the next relaunch" % why
    ctx.root_pid = pid
    return None


def relaunch(ctx, app, prof, be, win, a, run, label, ollama_url, instance, emit, why, stop=True):
    """EVERY relaunch inside a pass (audit B; run.py used to repeat this six times): keep the dying instance's log, stop, start, await the shell
    (counting only a prompt the NEW instance logs: prompt.log is never cleared, r18cz), settle, then on POSIX record the pane's root shell pid
    while the pane is provably clean (Windows records it in prove_keys). -> (instance + 1, proved): a relaunched Windows pane needs its keys
    proven again (and its cwd set)."""
    keep_instance_log(run, label, prof, a.backend, ollama_url, instance, app)
    if stop:
        app.stop()
    nb = prof.prompts()
    app.start()
    if await_shell(ctx, app, prof, win, a.startup_timeout, base=nb):
        if win and not os.environ.get("APPCI_APP_LOG"):
            try:
                ctx.log_path = app.find_log() or ctx.log_path
            except Exception:
                pass
        settle_first_run(ctx)
    if win:
        be.guard()
        return instance + 1, False
    bad = record_root_pid(ctx, app, why)
    if bad:
        emit(dict(name="depth_check_off_after_" + why, mode=label, status="skip", seconds=0, detail=[bad], metrics={}, shots=[]))
    return instance + 1, True


def set_ps_pane_state(ctx, be):
    """PowerShell pane, once per app launch, after the key proof: cd into the case dir (cases write RELATIVE effect files there) and, when there is an ssh
    test host, define `ssh` as a function that adds `-F <home>/.ssh/config` (the pane has no rc, so no <home>/bin wrapper and System32 ssh.exe reads the real
    user's config: r18co showed `ssh: Could not resolve hostname fake` in every PowerShell ssh case). Returns None, or the reason it could not be set.
    r18co: the first Set-Location was typed while the proof's Add-Content was still returning to the prompt, so the pane took Ctrl-U as text
    (`^USet-Location` -> 'not recognized'), stayed in the real home, and EVERY later PowerShell effect file landed outside the case dir (all PowerShell
    cases failed, then the wedge probe failed too). So: settle, type, and verify by an effect (a relative marker file must appear in the case dir); retry."""
    wd = ctx.W.replace("\\", "/")
    for attempt in range(3):
        # the proof's command must have returned to the prompt first: the app's own `precmd_seen` hop for it (the pane has no rc hook) plus a margin,
        # the old fixed settle when the log shows none (audit #11)
        t_wait = time.time()
        if ctx.log_path and ctx.until(lambda: ctx.precmds_since(t_wait - 2.0) >= 1, 1.5 + attempt, step=0.1):
            time.sleep(0.4)
        else:
            time.sleep(max(0.0, 1.5 + attempt - (time.time() - t_wait)))
        for _ in range(attempt + 1):
            be.key("ctrl+c")
            time.sleep(ctx.EXIT_WINDOW_S + 0.4 if ctx.mode == "agent" else 0.8)      # r18u8 (H16): two Ctrl-C < 1 s apart exit an empty agent view
        be.key("Escape" if ctx.pane == "cmd" else "ctrl+u")
        tok = ctx.tok(8).lower()
        fn = ("; " + ctx.prof.ps_ssh_function()) if (ctx.ssh and ctx.prof.ps_ssh_function()) else ""
        if ctx.pane == "cmd":       # cmd.exe: `cd /d` (also changes drive), no function for ssh (the ssh.cmd wrapper in <home>\\bin is found as `ssh`)
            be.type('cd /d "%s" && echo %s>>cwd_%s.txt' % (wd, tok, tok), 8, enter=True)
        elif ctx.pane == "wsl":     # r15 (T46): the same directory as the distribution sees it (/mnt/<drive>/...)
            import winapi
            be.type("cd '%s' && echo %s >> cwd_%s.txt" % (winapi.wsl_path(wd), tok, tok), 8, enter=True)
        else:
            be.type("Set-Location -LiteralPath '%s'%s; Add-Content cwd_%s.txt %s" % (wd, fn, tok, tok), 8, enter=True)
        if ctx.until(lambda: tok in ctx.lines("cwd_%s.txt" % tok), 10 + 5 * attempt):
            time.sleep(0.8)
            return None
    return "the PowerShell pane could not be put in the case dir (Set-Location typed 3 times, the relative marker file never appeared in %s): every PowerShell effect file would land elsewhere" % ctx.W


def variant_pane_problem(variant, app):
    """r15 (T46): None when the app runs the shell of the pass `variant`, else why not (winmap.variant_shell_problem over the image names of the
    processes in the app's job object). No job object (creation failed) or an unreadable one: None, the pass runs and startup_shell_pane judges."""
    job = getattr(app, "job", None)
    if not job:
        return None
    try:
        import winapi
        me = app.pid()
        names = [winapi.proc_name(p) for p in winapi.job_pids(job) if p != me]
    except Exception as e:
        print("[appci] could not list the app's processes for the %s pass: %r" % (variant, e), flush=True)
        return None
    return winmap.variant_shell_problem(variant, names)


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
            be.key("Escape" if pane == "cmd" else "ctrl+u")
            if pane == "cmd":       # cmd.exe: no single quotes, no space before `>>`
                be.type('echo %s>>"%s"' % (tok, fp), 8, enter=True)
            elif pane == "wsl":     # r15 (T46): the absolute path as the distribution sees it
                be.type("echo %s >> '%s'" % (tok, winapi.wsl_path(fp)), 8, enter=True)
            else:
                be.type(("Add-Content -LiteralPath '%s' %s" if pane == "powershell" else "echo %s >> '%s'") % ((fp, tok) if pane == "powershell" else (tok, fp)), 8, enter=True)
        except Exception as e:
            log.append("%s threw: %s" % (method, e))
            continue
        if ctx.until(lambda: tok in ctx.lines(os.path.basename(f)), 8):
            if pane in ("powershell", "cmd", "wsl"):
                bad = set_ps_pane_state(ctx, be)
                if bad:
                    return bad
            print("keys: delivered via SendInput %s" % method, flush=True)
            off = record_root_pid(ctx, ctx.app, "key_proof")      # audit #1b: the pane is provably at its prompt and in the case dir now
            if off:
                print("[appci] " + off, flush=True)
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
    save_old_rotations(src, logs, "phosphor.%d" % n, ollama_url)


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


SSH_CM_RX = re.compile(r"ControlPath=(\S+)")


def stop_ssh_controlmasters(label, run):
    """r18e9 (macOS): 12 ssh ControlMaster clients naming the run dir outlived the pass (listed by the audit, never stopped; teardown said nothing was left). The app's ssh
    wrapper starts `ssh -o ControlMaster=yes -o ControlPath=<p> ...` per connection. Ask each master to exit through ITS OWN control socket (`ssh -O exit`), and only when
    that fails signal that exact PID, re-verified to still name the run dir. Never by process name. One line per client; never raises."""
    if os.name == "nt":
        return
    try:
        import signal
        out = subprocess.run(["ps", "-axo", "pid=,command="], capture_output=True, text=True, timeout=30).stdout
        for l in out.splitlines():
            l = l.strip()
            if not l or run not in l or "run.py" in l or l.startswith("%d " % os.getpid()):
                continue
            pid, _, cmd = l.partition(" ")
            if not pid.isdigit() or not re.match(r"(\S*/)?ssh\s", cmd) or "ControlMaster" not in cmd:
                continue
            m = SSH_CM_RX.search(cmd)
            how = "no ControlPath in its command line"
            if m:
                try:
                    p = subprocess.run(["ssh", "-O", "exit", "-o", "ControlPath=" + m.group(1), "-o", "BatchMode=yes", "stopcm"], capture_output=True, text=True, timeout=15)
                    how = "ssh -O exit via %s: rc=%d %s" % (m.group(1), p.returncode, (p.stderr or p.stdout).strip()[:80])
                except Exception as e:
                    how = "ssh -O exit failed to run: %r" % (e,)
            time.sleep(0.5)
            alive = subprocess.run(["ps", "-o", "command=", "-p", pid], capture_output=True, text=True).stdout
            if not alive.strip():
                print("teardown[%s]: ssh ControlMaster pid %s stopped (%s)" % (label, pid, how), flush=True)
                continue
            if run in alive:
                try:
                    os.kill(int(pid), signal.SIGTERM)
                    time.sleep(1.0)
                except Exception:
                    pass
                gone = not subprocess.run(["ps", "-o", "command=", "-p", pid], capture_output=True, text=True).stdout.strip()
                print("teardown[%s]: ssh ControlMaster pid %s %s (%s; then SIGTERM to that pid)" % (label, pid, "stopped" if gone else "STILL RUNNING", how), flush=True)
            else:
                print("teardown[%s]: ssh pid %s no longer names the run dir: left alone" % (label, pid), flush=True)
    except Exception as e:
        print("teardown[%s]: ssh ControlMaster cleanup failed: %r" % (label, e), flush=True)


def cleanup_scratch(label, prof, run):
    """end-of-pass cleanup evidence (all platforms; the logs were copied above): remove the scratch HOME and ctl dirs, then audit for leftover
    processes whose command line names this run dir. Prints what it did; never raises."""
    stop_ssh_controlmasters(label, run)
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


def fake_pass_plan(cases_csv, modes, fake_global):
    """True when the fix tier needs the separate fake-model pass: cases_csv is the fix-tier selection (None outside the fix tier / with explicit --cases),
    an agent pass exists, the run is not already the fake globally, and the selection names a PROVING case that needs the scripted model."""
    if cases_csv is None or fake_global or "agent" not in modes:
        return False
    names = set(cases_csv.split(","))
    return any(n in names for n in cases.PROVING_FAKE)


def fake_pass_ok(name, fake, plan):
    """does case `name` run in this pass? Without a fake-pass plan every case runs in every pass (unchanged); with one, the PROVING_FAKE cases run
    only in the fake pass and everything else only in the ordinary passes."""
    return True if not plan else (name in cases.PROVING_FAKE) == bool(fake)


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
            # the release gate (#1495, script/ci/check_release_gate.py) reads the FULL commit this job ran on from results.json
            "sha=" + os.environ.get("CI_COMMIT_SHA", "local"),
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
