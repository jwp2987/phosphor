# appci/selftest_r29_labharness2.py: offline self-tests of the r29 lab-harness fixes from the r29a Linux lab run: the Tab -> insert time as a perf band (owner
# 2026-11-13 'Turn it into a band'), lab_chips counting channels per prompt the app ran, the git_read branch evidence (OCR tolerance + the app's chip value), and
# the helper takeover on lan ('creating a remote server executor'). Pure Python.
import json, os, shutil, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


class Ctx:
    label = "terminal"


ARM = {"name": "lab_tab_remote_dir.bash.lan.helper", "lat_ms": 0}
KEY = "wayland/terminal/lab_tab_remote_dir.bash.lan.helper/tab_to_insert"


@check
def r29_tab_to_insert_is_a_band_and_no_band_is_noref():
    import cases, cases_lab as CL
    old = os.environ.get("APPCI_BANDS_FILE"), os.environ.get("APPCI_PLATFORM")
    tmp = tempfile.mkdtemp()
    try:
        os.environ["APPCI_PLATFORM"] = "linux"
        assert CL.tab_band_key(Ctx, ARM) == KEY, CL.tab_band_key(Ctx, ARM)
        os.environ["APPCI_BANDS_FILE"] = os.path.join(tmp, "none.json")              # no band file: NOREF, never fail or pass
        r = cases.Result("t", "terminal")
        assert CL.tab_band_check(r, Ctx, ARM, 0.4) == "noref" and not r.fails and len(r.noref) == 1 and r.metrics["tab_to_insert_ms"] == 400.0, (r.fails, r.noref)
        f = os.path.join(tmp, "b.json")
        json.dump({KEY: {"n": 6, "median": 180, "mad": 5, "upper": 200.0, "lower": 150.0, "values": [180], "runs": ["r"], "provisional": False, "set_by": "data", "kind": "ms"}}, open(f, "w"))
        os.environ["APPCI_BANDS_FILE"] = f
        r = cases.Result("t", "terminal")
        assert CL.tab_band_check(r, Ctx, ARM, 0.19) == "pass" and not r.fails
        r = cases.Result("t", "terminal")
        assert CL.tab_band_check(r, Ctx, ARM, 0.25) == "fail" and len(r.fails) == 1 and "outside its band" in r.fails[0]
    finally:
        for k, v in zip(("APPCI_BANDS_FILE", "APPCI_PLATFORM"), old):
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        shutil.rmtree(tmp)
    src = open(os.path.join(HERE, "cases_lab.py"), encoding="utf-8").read()
    assert "the budget is 1 RTT" not in src and "tab_band_check(r, ctx, arm, took)" in src
    # every correctness check of the case is still there
    for s in ("the remote directory the completed `cd` entered", "def c_tab_remote_dir"):
        assert s in src, s


@check
def r29_perf_bands_extracts_only_the_lab_tab_metric():
    sys.path.insert(0, os.path.join(HERE, "..", "..", "ci"))
    import perf_bands as pb
    row = {"name": "lab_tab_remote_dir.zsh.150.nohelper", "mode": "terminal", "metrics": {"tab_to_insert_ms": 231.4, "tab_insert_s": 0.231}}
    assert pb.extract(row, "wayland") == [("lab_tab_remote_dir.zsh.150.nohelper/tab_to_insert", 231.4, "ms")]
    assert pb.extract({"name": "lab_chips.bash.lan.nohelper", "metrics": {"side_calls": 2}}, "wayland") == []


R29A_BASH150 = """2026-11-13T04:19:32Z [DEBUG] git batch: exec started generation=1 prompt=1 producer=navigate_tree path=/work/u_bash
2026-11-13T04:19:33Z [DEBUG] git batch: exec done generation=1 ok=true producers=["navigate_tree"] path=/work/u_bash
2026-11-13T04:19:35Z [DEBUG] git batch: exec started generation=2 prompt=2 producer=navigate_tree path=/work/u_bash/chipst45prhlws
2026-11-13T04:19:35Z [DEBUG] git batch: producer=git_status joins generation=2 (prompt=2) path=/work/u_bash/chipst45prhlws
"""


@check
def r29_chips_channels_are_judged_per_prompt_the_app_ran():
    import cases_lab as CL
    assert CL.batch_prompts_run(R29A_BASH150) == [1, 2] and CL.batch_prompts_run("") == []
    # r29a bash.150: prompt.log grew by 1, two prompts' batches fell in the window, two exec channels: 2 / 2 passes the <= 1 per prompt target
    prompts = CL.chip_prompt_count(1, R29A_BASH150)
    assert prompts == 2 and 2 <= prompts
    # a prompt with two execs still fails: the target is not loosened
    two_execs = R29A_BASH150 + "2026-11-13T04:19:36Z [DEBUG] git batch: exec started generation=3 prompt=2 producer=refresh_tree path=/x\n"
    assert CL.chip_prompt_count(1, two_execs) == 2 and not (3 <= CL.chip_prompt_count(1, two_execs))
    assert CL.chip_prompt_count(0, "") == 1, "never fewer than one prompt"
    assert CL.chip_prompt_count(3, R29A_BASH150) == 3, "the shell's own larger count stands"
    src = open(os.path.join(HERE, "cases_lab.py"), encoding="utf-8").read()
    assert "r.check(calls <= prompts," in src and "chip_prompt_count(ctx.remote.prompts() - p0, ctx.log_since(off))" in src


@check
def r29_branch_evidence_screen_tolerance_or_the_chip_log():
    import cases_lab as CL
    br = "brtl0ab9x"
    log = "2026-11-13T04:19:35Z [INFO] [chips] update_chip_value ShellGitBranch -> %s\n" % br
    seen, how = CL.branch_evidence("user@h:~ $ cd x\n( brt10ab9x )\n", br, "")       # l/1 and 0/o style misread: one glyph off
    assert seen is True and "OCR" in how, (seen, how)
    assert CL.branch_evidence("... brtl0ab9x ...", br, "")[0] is True
    assert CL.branch_evidence("nothing here", br, log) == (True, "app log chip value"), "the log's chip value is evidence on its own"
    assert CL.branch_evidence("nothing here", br, "")[0] is False, "neither: the row still fails"
    assert CL.branch_evidence("brtXXXXXX far off", br, "")[0] is False
    assert CL.branch_evidence(None, br, "")[0] is None and CL.branch_evidence(None, br, log)[0] is True
    src = open(os.path.join(HERE, "cases_lab_helper.py"), encoding="utf-8").read()
    assert 'fx["branch"] in txt' not in src and "branch %s not on screen after cd into the repo on the remote" in src


@check
def r29_helper_takeover_accepts_the_lan_created_executor():
    import cases_lab as CL
    lan = ("2026-11-13T04:12:48Z [INFO] creating a remote server executor for session SessionId(14340739750824140586)\n"
           "2026-11-13T04:12:48Z [INFO] Remote-server client connected for ssh session SessionId(14340739750824140586)\n")
    assert CL.helper_takeover_state(lan) == (True, 0)
    late = "2026-11-13T04:13:23Z [INFO] Remote-server client connected after session init for ssh session S; command executor swapped from the ControlMaster\n"
    assert CL.helper_takeover_state(late) == (True, 0)
    assert CL.helper_takeover_state("Remote-server client lost for ssh session S; command executor fell back to the ControlMaster\n") == (False, 0)
    assert "executor_swapped=True" in CL.helper_takeover_note(lan)

    class C:
        def __init__(self, t): self.t = t
        def log_since(self, off): return self.t
        def until(self, f, *a, **k): return f()

    class R:
        def note(self, *a): pass
    assert CL.wait_helper_takeover(C(lan), R(), {"helper": "helper"}, 0, quiet=0) is True


# r31b: macOS lab_chips.zsh.150.nohelper counted the session's own connect-time exec channels (two file-tree snapshot execs, logged by the app at the second the window opened) as side calls
@check
def r31b_chips_window_waits_for_the_connect_time_work():
    import cases_lab as CL
    reg = "[INFO] shell mode: ControlMaster registered as the out-of-band channel for SessionId(1)\n"
    snap = "[INFO] hop=file_tree event=snapshot source=shell removed=0 added=2 added_hash=[a,b]\n"
    route = "[INFO] hop=backend_route family=repo_metadata backend=shell reason=caps\n"
    assert CL.bootstrap_state("") == (False, 0)
    assert CL.bootstrap_state(snap + route) == (False, 2), "work lines without the registration: not registered"
    assert CL.bootstrap_state(reg + snap + snap + route) == (True, 3)

    class C:
        """log_since returns the next scripted excerpt each poll; until() polls up to 8 times"""
        def __init__(self, logs): self.logs, self.i = logs, 0
        def log_since(self, off): return self.logs[min(self.i, len(self.logs) - 1)]
        def until(self, fn, t, step=0.1):
            for _ in range(8):
                self.i += 1
                if fn():
                    return True
            return False

    class R:
        def __init__(self): self.notes = []
        def note(self, m): self.notes.append(m)
    r = R()
    # steady: registered, work finished, the log stops growing -> settles
    assert CL.wait_bootstrap_quiet(C([reg + snap + snap]), r, 0, quiet=0) is True
    # still growing on every poll (a snapshot lands each time): never quiet, noted, the case goes on
    grow = [reg + snap * k for k in range(1, 12)]
    r = R()
    assert CL.wait_bootstrap_quiet(C(grow), r, 0, quiet=0) is False and "NOT settled" in r.notes[-1]
    # never registered: not settled
    assert CL.wait_bootstrap_quiet(C([snap]), R(), 0, quiet=0) is False
    # both chips and path_detect open their window after it
    for fn in ("c_chips", "c_path_detect"):
        src = open(os.path.join(HERE, "cases_lab.py"), encoding="utf-8").read()
        body = src[src.index("def %s(" % fn):]
        body = body[:body.index("\n@lab_case", 10)] if "\n@lab_case" in body[10:] else body
        assert "wait_bootstrap_quiet(" in body and body.index("wait_bootstrap_quiet(") < body.index("ChannelWatch("), fn
