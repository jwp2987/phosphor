# appci/selftest_r28_tabscleanup.py: offline self-tests of the r28 tab-cleanup fix (r26w Windows: tabs_two_windows cleanup left 5 streaming tabs open after
# 13 close keys; the tabs_streaming / tabs_two_windows Windows reorder rows name #1658). Pure Python; selftest.py appends CHECKS to its own list.
import os, sys, types

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import selftest_r25_harness_a as M  # noqa: E402  (the tab-ring model)

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


class _NoSleep:
    """cases.time.sleep -> nothing, so the 13-tab loops run in milliseconds"""
    def __enter__(self):
        import cases
        self.c, self.old = cases, cases.time.sleep
        cases.time.sleep = lambda *_a, **_k: None
        import cases_robust as R
        self.R, self.saved = R, R.CLOSE_PANE
        R.CLOSE_PANE = "CLOSE"

    def __exit__(self, *a):
        self.c.time.sleep = self.old
        self.R.CLOSE_PANE = self.saved


class LossyRing(M.TabRing):
    """a ring whose first `lose` CLOSE keys are swallowed (the app is busy with the streams): the r26w Windows shape"""
    def __init__(self, lose, own="100"):
        super().__init__(own)
        self.lose = lose

    def key(self, k, *a, **kw):
        if k == "CLOSE" and self.lose > 0 and len(self.tabs) > 1:
            self.lose -= 1
            self.keys.append(k)
            return
        super().key(k, *a, **kw)


@check
def r28_a_lost_close_key_is_resent_after_the_same_shell_is_probed_twice():
    """13 close keys were sent in r26w and 5 tabs stayed open: the loop was bounded by the count of tabs opened. Lost keys are retried now, one per
    proven re-probe of the same shell, and the pass's own tab is still never closed."""
    import cases
    with _NoSleep():
        for lose in (1, 5, 8):
            ring = LossyRing(lose)
            for _ in range(13):
                ring.new(streaming=True)
            fails = []
            sent = cases.close_opened_tabs(ring.ctx(), M._r(fails, []), "100", 13, "tabs_two_windows")
            assert len(ring.tabs) == 1 and ring.tabs[0]["pid"] == "100" and not fails and sent == 13 + lose, (lose, sent, len(ring.tabs), fails)


@check
def r28_a_tab_that_really_stays_open_still_fails_and_never_closes_the_own_tab():
    """a tab that ignores every close key: the check fails after the bound (4n+4 rounds, a bounded number of keys), naming the pid found"""
    import cases
    with _NoSleep():
        ring = LossyRing(10 ** 6)
        for _ in range(3):
            ring.new(streaming=True)
        fails = []
        sent = cases.close_opened_tabs(ring.ctx(), M._r(fails, []), "100", 3, "t")
        assert len(fails) == 1 and "still open" in fails[0] and "pid 10" in fails[0], fails
        assert ring.tabs[0]["pid"] == "100" and len(ring.tabs) == 4 and sent <= 4 * 3 + 4, (sent, len(ring.tabs))


@check
def r28_final_probe_ends_a_running_stream_first_and_names_what_it_found():
    """the r26w final probe sent no Ctrl-C: a front tab still streaming answers None, which the row called 'pid None'. A stream is ended before the last probe
    (a tab that is merely still streaming is not a failure), and a tab that is still open is reported by pid."""
    import cases
    with _NoSleep():
        ring = M.TabRing()
        ring.new(streaming=True)
        ring.front = 1
        fails = []
        cases.close_opened_tabs(ring.ctx(), M._r(fails, []), "100", 1, "t")
        assert not fails and ring.keys.count("ctrl+c") >= 2, (fails, ring.keys)


@check
def r28_a_second_window_left_open_fails_the_cleanup():
    """windows_before is compared with the app's windows after the closes; unknown (None / []) judges nothing"""
    import cases
    with _NoSleep():
        for now, before, bad in ((2, 1, True), (1, 1, False), (0, 1, False), (2, None, False)):
            ring = M.TabRing()
            ring.new(streaming=True)
            ctx = ring.ctx()
            ctx.be.windows = lambda n=now: [object()] * n
            fails = []
            cases.close_opened_tabs(ctx, M._r(fails, []), "100", 1, "t", windows_before=before)
            assert bool([f for f in fails if "window" in f]) == bad, (now, before, fails)
    import inspect
    assert "windows_before=wins0" in inspect.getsource(cases.c_tabs) and "wins0 = len(ctx.be.windows())" in inspect.getsource(cases.c_tabs)


@check
def r28_windows_reorder_row_names_1658_only_without_a_discard():
    """the Git Bash 1/16 and 2/10 rows: on Windows with no key discarded in the app log the failure text names #1658; other platforms and discards do not"""
    import cases
    saved = os.environ.get("APPCI_PLATFORM")
    try:
        for plat, log, named in (("windows", "ok", True), ("windows", "typed after Enter were discarded", False), ("linux-x11", "ok", False)):
            os.environ["APPCI_PLATFORM"] = plat
            fails = []
            ctx = types.SimpleNamespace(log_path="x", log_since=lambda off, l=log: l)
            r = types.SimpleNamespace(check=lambda ok, m: fails.append(m) if not ok else None, known=lambda m: None, note=lambda m: None)
            cases.streaming_exact_lines(ctx, r, ["a"], ["a", "b"], "typing", 0)
            assert len(fails) == 1 and ("#1658" in fails[0]) == named, (plat, fails)
    finally:
        if saved is None:
            os.environ.pop("APPCI_PLATFORM", None)
        else:
            os.environ["APPCI_PLATFORM"] = saved
