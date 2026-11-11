# appci/selftest_r21_wlinput.py: offline self-tests of round 21, Wayland input rows of the round 15 targeted run (docs/design/r10-r21-wlinput.md;
# #2139, #2140, #2141): the find hook under a wrapper launch, places read again between the arms of primary_and_middle_click, fcitx5's toggle key
# and the injector's priming tap in ime_compose. Pure Python: no app, no display.
# selftest.py appends CHECKS to its own list (two lines at its end), so `run.py --self-test` runs them.
# Every judge is fed a right input AND the wrong ones it exists to catch: an oracle that cannot fail is not a test.
import inspect, os, shutil, sys, tempfile, types

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.join(HERE, "..", "..", "..")
sys.path.insert(0, HERE)

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


def _src(*parts):
    return open(os.path.join(REPO, *parts), encoding="utf-8").read()


@check
def r21_window_of_a_wrapped_launch_is_found_through_its_descendants():
    """#2141: a launch through `exec strace -f ... <app>` leaves the wrapper's PID in app.pid; the window is its child's. descendant_pids walks
    the process table nearest generation first; app_window_origin asks the launcher's PID first and then each descendant; a launch without a
    wrapper is unchanged; no window anywhere is None"""
    import cases  # noqa: F401
    import cases_ui as UI
    kids = {10: [11], 11: [30, 20], 20: [21], 99: [100]}
    assert UI.descendant_pids(10, kids) == [11, 20, 30, 21], UI.descendant_pids(10, kids)
    assert UI.descendant_pids(21, kids) == [] and UI.descendant_pids(7, {}) == []
    assert UI.descendant_pids(1, {1: [2], 2: [1]}) == [2], "a loop in the table (a PID reused between two reads) ends"
    asked = []

    def by_pid(owner):
        def f(p):
            asked.append(p)
            return (5, 6) if p == owner else None
        return f
    assert UI.app_window_origin(10, by_pid(10), kids) == (5, 6) and asked == [10], "the launcher's own window: no descendant is asked"
    del asked[:]
    assert UI.app_window_origin(10, by_pid(11), kids) == (5, 6) and asked == [10, 11], "the wrapper's child owns the window"
    assert UI.app_window_origin(10, by_pid(100), kids) is None, "another process tree's window is never taken"
    assert UI.app_window_origin(10, by_pid(12345), {}) is None
    # the real table: this process is a descendant of its parent, and the parent field is read behind the LAST ')'
    if os.path.isdir("/proc/self"):
        assert os.getpid() in UI.descendant_pids(os.getppid()), "child_map reads /proc/<pid>/stat"
    tmp = tempfile.mkdtemp()
    try:
        os.makedirs(os.path.join(tmp, "42"))
        open(os.path.join(tmp, "42", "stat"), "w").write("42 (a b) c) S 7 42 42 0 -1\n")
        os.makedirs(os.path.join(tmp, "43"))
        open(os.path.join(tmp, "43", "stat"), "w").write("garbage")
        os.makedirs(os.path.join(tmp, "self"))
        assert UI.child_map(tmp) == {7: [42]}, UI.child_map(tmp)
        assert UI.child_map(os.path.join(tmp, "absent")) == {}
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


@check
def r21_find_hook_gives_positions_under_a_wrapper():
    """#2141: _hook_word on X11 and Wayland with the app launched through a wrapper: the origin of the child's window is used, so hook_find's row
    is the hook's position (it was None: every word by OCR, and cases_pointer.current_row then had no later sighting to follow)"""
    import cases  # noqa: F401
    import cases_ui as UI
    tmp = tempfile.mkdtemp()
    orig = UI.x11_window_origin, UI.sway_window_rect, UI.child_map
    try:
        log = os.path.join(tmp, "app.log")
        open(log, "w").write("appci_find token=tokw x0=10 y0=20 x1=30 y1=40\nappci_find token=tokw x0=10 y0=5 x1=30 y1=25\n")

        class Be:
            name = "x11"
            pid_fn = staticmethod(lambda: 500)
        c = types.SimpleNamespace(log_path=log, log_since=lambda o: open(log).read()[o:], be=Be())
        UI.child_map = lambda proc="/proc": {500: [501]}
        UI.x11_window_origin = lambda pid: (100, 200) if pid == 501 else None
        row = UI._hook_word(c, "tokw")
        assert row and row[3:] == (110, 130, 205, 225), row
        assert [h[5] for h in UI._hook_word(c, "tokw", all_hits=True)] == [220, 205], "every sighting, oldest first, for current_row"
        UI.x11_window_origin = lambda pid: None
        assert UI._hook_word(c, "tokw") is None and UI._hook_word(c, "tokw", all_hits=True) == []
        Be.name = "wayland"
        UI.sway_window_rect = lambda pid: (7, 9, 800, 600) if pid == 501 else None
        row = UI._hook_word(c, "tokw")
        assert row and row[3:] == (17, 37, 14, 34), row
        UI.sway_window_rect = lambda pid: (1, 2, 800, 600) if pid == 500 else None
        assert UI._hook_word(c, "tokw")[3:5] == (11, 31), "a launch without a wrapper: the launcher's own window"
    finally:
        UI.x11_window_origin, UI.sway_window_rect, UI.child_map = orig
        shutil.rmtree(tmp, ignore_errors=True)
    lc = inspect.getsource(sys.modules["cases_pointer"].c_link_and_path_click)
    assert 'ctx.app.wrap = "strace' in lc, "the case this is for still launches through the wrapper"


@check
def r21_primary_case_reads_the_places_again_in_every_arm():
    """#2141: relocate takes the newest sighting of the same run of text from the find hook, places a word the hook never saw by ONE screen read,
    and leaves out a word found by neither (need_all then says so); primary_and_middle_click calls it inside its loop, before the drag"""
    import cases  # noqa: F401
    import cases_pointer as P
    import cases_ui as UI
    row = lambda t, y, l=100, rr=170: (t, (l + rr) // 2, y + 8, l, rr, y, y + 16)      # noqa: E731
    found = {"aaa": row("aaa", 500), "bbb": row("bbb", 500, 200, 270), "ccc": row("ccc", 500, 300, 370), "ddd": row("ddd", 500, 400, 470)}
    hits = {"aaa": [row("aaa", 500), row("aaa", 440)],                     # the block moved up 60 px
            "bbb": [row("bbb", 500, 200, 270), row("bbb", 300, 40, 110)]}  # another copy (a command line quoting the word): never taken
    orig = UI._hook_word, UI.words
    reads = []
    try:
        UI._hook_word = lambda ctx, t, since=None, all_hits=False: hits.get(t, [])
        UI.words = lambda ctx, tag: (reads.append(tag) or [("ccc", 335, 448, 300, 370, 440, 456)], None)
        out = P.relocate(None, "pm_find_on", ["aaa", "bbb", "ccc", "ddd"], found, 0)
    finally:
        UI._hook_word, UI.words = orig
    assert out["aaa"][2] == 448 and out["bbb"] == found["bbb"] and out["ccc"][2] == 448 and "ddd" not in out, out
    assert reads == ["pm_find_on"], "one screen read for the words the hook has no sighting of"
    body = inspect.getsource(P.c_primary_and_middle_click)
    loop = body.index("for on, word in")
    assert loop < body.index("found = relocate(ctx,") < body.index("m.drag("), "the places are read again in each arm, before its first pointer action"
    assert body.index("found = relocate(ctx,") < body.index("need_all(ctx, r, found, words,", loop)


@check
def r21_ime_case_takes_the_toggle_key_away_and_proves_it():
    """#2141: fcitx5's `AltTriggerKeys` (a lone Left Shift by default) is the injector's priming tap; the case writes a config that empties the
    list before the daemon starts, sends the tap alone with the engine selected and reads the daemon back BEFORE anything is typed through the
    input method; a daemon the tap switched is UNVERIFIABLE, never a 'leak'"""
    import cases  # noqa: F401
    import backend
    import cases_desk as D
    cfg = D.fcitx_config()
    assert cfg.startswith("[Hotkey]\n") and "\nAltTriggerKeys=\n" in cfg and "Shift" not in cfg.replace("Shift_L tap", "")
    assert D.prime_tap_verdict("pinyin", "pinyin") is None
    for after in ("keyboard-us", ""):
        why = D.prime_tap_verdict("pinyin", after)
        assert why and "AltTriggerKeys" in why and "not judged" in why and repr(after or "nothing") in why, why
    start = inspect.getsource(D.LinuxIme.start)
    assert start.index("fcitx_config()") < start.index("arm_session_pre("), "the config is on disk before the daemon is started"
    assert '"config"' in start and '"profile"' in start
    body = inspect.getsource(D.c_ime_compose)
    assert body.index("ime_engaged_verdict(") < body.index("ctx.be.keys([])") < body.index("prime_tap_verdict(") < body.index("INTL.compose_raw(")
    assert "ime_after_prime_tap" in body and "ime_after_keys" in body and body.count("r.unverifiable(why)") == 2
    # the premise: the Wayland backend leads a primed call with the lone Shift_L tap, and keys([]) is that tap and nothing else
    calls = []
    be = backend.WaylandBackend.__new__(backend.WaylandBackend)
    be.prime_ms, be.run_recorded = 20, lambda cmds: calls.extend(cmds)
    be.keys([])
    assert calls == [["wtype", "-k", "Shift_L", "-s", "20"]], calls
    assert backend.parse_injector(calls[0]) == dict(chars=0, keys=[]), "the tap is no key in the injector accounting"


@check
def r21_wlinput_documented():
    """docs/testing.md names what changed in this round and this file"""
    doc = _src("docs", "testing.md")
    sec = doc[doc.index("## Round 21, Wayland input"):]
    for word in ("#2139", "#2140", "#2141", "#2142", "`app_window_origin`", "`relocate`", "`fcitx_config`", "`prime_tap_verdict`", "`ime_after_prime_tap`",
                 "`after_empty_enter`", "`PHOSPHOR_PRIMARY_READS_CLIPBOARD_TEXT=1`", "`PHOSPHOR_AGENT_NOTIFY_EVERY_EVENT=1`", "selftest_r21_wlinput.py",
                 "17936839261"):
        assert word in sec, word
