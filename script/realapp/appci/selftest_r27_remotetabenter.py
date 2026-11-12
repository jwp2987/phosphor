# appci/selftest_r27_remotetabenter.py: offline self-tests of the #2188 step of lab_tab_remote_dir (r27): Tab then a FAST Enter in a remote session runs the
# COMPLETED line (the app holds the Enter behind its RemoteTabCompletion queue gate until the remote completer answers). Pure Python, no display.
import inspect, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


def _src(*parts):
    root = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
    return open(os.path.join(root, *parts), encoding="utf-8").read()


@check
def r27_remote_tab_wait_lines_are_parsed_2188():
    """the app's remote_tab_wait lines (released and gave_up) are read with their counts; other lines are not"""
    import cases  # noqa: F401
    import cases_lab as CL
    log = ("x\n[INFO] remote_tab_wait: released why=answered_applied waited_ms=163 held=1 enter_held=1\n"
           "[INFO] remote_tab_wait: released why=answered waited_ms=151 held=0 enter_held=0\n"
           "[WARN] remote_tab_wait: gave_up why=net waited_ms=5001 held=2 enter_held=1 cap_ms=5000; the remote completion did not answer\n"
           "remote_tab_wait: released why=answered waited_ms=abc held=0 enter_held=0\n")
    got = CL.remote_tab_waits(log)
    assert got == [
        {"end": "released", "why": "answered_applied", "waited_ms": 163, "held": 1, "enter_held": True},
        {"end": "released", "why": "answered", "waited_ms": 151, "held": 0, "enter_held": False},
        {"end": "gave_up", "why": "net", "waited_ms": 5001, "held": 2, "enter_held": True},
    ], got
    assert CL.remote_tab_waits("") == [] and CL.remote_tab_waits(None) == []


@check
def r27_remote_tab_wait_note_names_the_wait_or_why_there_is_none_2188():
    """the row note carries the logged wait in ms and how many waits held an Enter; with no line it says why that can be"""
    import cases  # noqa: F401
    import cases_lab as CL
    note = CL.remote_tab_wait_note(CL.remote_tab_waits("remote_tab_wait: released why=answered_applied waited_ms=163 held=1 enter_held=1\n"))
    assert "waited 163 ms" in note and "why=answered_applied" in note and "1 of 1 with an Enter held" in note, note
    none = CL.remote_tab_wait_note([])
    assert "no remote_tab_wait line" in none and CL.REMOTE_TAB_ENTER_WAIT_ENV + "=0" in none and "#1801" in none, none


@check
def r27_tab_enter_received_demands_exactly_the_completed_line_2188():
    """exactly one `cd` line, exactly the completed directory: a trailing `/` or blank is the completer's; the uncompleted prefix, a doubled line (#2177), a
    second cd or a longer path all fail"""
    import cases  # noqa: F401
    import cases_lab as CL
    want = "/work/u/tabfx_ab12/zzqab12_only"
    for ok_line in ("cd " + want, "cd " + want + "/", "cd " + want + "/ ", "cd " + want + " "):
        assert CL.tab_enter_received([ok_line], want)[0], ok_line
    for bad in (["cd /work/u/tabfx_ab12/zzqab12"],                                  # the r25a failure: the uncompleted line ran
                ["cd /wocd " + want + "/"],                                          # #2177: the typed prefix kept in front
                ["cd /work/u/tabfx_ab12/zzqab12", "cd " + want + "/"],               # both ran
                ["cd " + want + "/x"],
                ["cd " + want + "_more"],
                []):
        ok, msg = CL.tab_enter_received(bad, want)
        assert not ok and "want exactly one line" in msg, (bad, msg)


@check
def r27_lab_tab_case_types_a_fast_enter_and_judges_the_received_line_2188():
    """step (c) of lab_tab_remote_dir presses Enter one key pace (10-25 ms, the owner's fast-stress rule) after Tab with no fixed sleep, keeps its exact effect
    verdict, judges the received line exactly, and notes the app's logged wait"""
    import cases  # noqa: F401
    import cases_lab as CL
    body = inspect.getsource(CL.c_tab_remote_dir)
    c = body[body.index("# (c) FAST ENTER"):]
    assert "time.sleep(0.03)" not in body, "the 30 ms Enter was outside the 10-25 ms fast-stress pace"
    assert c.index('ctx.be.key("Tab")') < c.index("enter_ms = type_pace(ctx)") < c.index("time.sleep(enter_ms / 1000.0)") < c.index('ctx.be.key("Return")')
    assert "CA.exact_lines(r," in c and "[want_dir], got2)" in c, "the effect verdict stays exact"
    assert "tab_enter_received(recs, want_dir)" in c and "r.check(ok," in c
    assert "r.note(remote_tab_wait_note(remote_tab_waits(ctx.log_since(off_c))))" in c
    assert c.index("off_c = ctx.log_size()") < c.index('ctx.be.key("Tab")')
    lo, hi = 10, 25

    class Rng:
        def randint(self, a, b):
            assert (a, b) == (lo, hi)
            return b

    class Ctx:
        rng = Rng()

    assert CL.type_pace(Ctx()) == hi


@check
def r27_remote_tab_names_match_the_app_2188():
    """the kill switch, the gate, its net and the log line the case reads are the app's own"""
    import cases  # noqa: F401
    import cases_lab as CL
    assert CL.REMOTE_TAB_ENTER_WAIT_ENV == "PHOSPHOR_REMOTE_TAB_ENTER_WAIT"
    sw = _src("app", "src", "terminal", "input_queue", "switches.rs")
    assert 'pub const REMOTE_TAB_ENTER_WAIT_ENV: &str = "PHOSPHOR_REMOTE_TAB_ENTER_WAIT";' in sw
    assert "| `PHOSPHOR_REMOTE_TAB_ENTER_WAIT` | #2188:" in sw, "the switch is in the doc table of switches.rs"
    gates = _src("app", "src", "terminal", "input_queue", "gates.rs")
    assert "RemoteTabCompletion," in gates and "GateKind::RemoteTabCompletion => Some(Duration::from_secs(5))," in gates
    assert 'GateKind::RemoteTabCompletion => "remote_tab_completion",' in gates
    rt = _src("app", "src", "terminal", "input", "remote_tab.rs")
    for line in ('"remote_tab_wait: released why={why} waited_ms={} held={held} enter_held={}"',
                 '"remote_tab_wait: released why={why} waited_ms={} held=0 enter_held=0"',
                 '"remote_tab_wait: gave_up why=net waited_ms={} held={held} enter_held={} cap_ms={}; \\'):
        assert line in rt, line
    # every why the app can log is a word the regex reads
    inp = _src("app", "src", "terminal", "input.rs")
    for why in ("answered", "answered_applied", "answered_ambiguous", "answered_no_match", "answered_stale", "superseded", "ctrl_c"):
        assert '"%s"' % why in inp, why
        assert CL.remote_tab_waits("remote_tab_wait: released why=%s waited_ms=1 held=0 enter_held=0" % why)[0]["why"] == why
    docs = _src("docs", "testing.md")
    assert "PHOSPHOR_REMOTE_TAB_ENTER_WAIT" in docs and "selftest_r27_remotetabenter.py" in docs and "#2188" in docs
