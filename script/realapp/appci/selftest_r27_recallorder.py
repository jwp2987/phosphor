# appci/selftest_r27_recallorder.py: offline self-tests of the #2107 arm of history_up_recall (r27): the Linux / macOS pane's history is made longer than the
# app lists inline, so the Up arrow's list is built on a background thread while step (b)'s fast keys are typed. Pure Python, no display.
import inspect, os, sys, tempfile, types

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
def r27_recall_seed_is_above_the_inline_limit_and_distinct_2107():
    """the seed alone is above the app's inline limit, every line is distinct and inert, and the limit the case names is the app's own"""
    import cases_r14_w1a as W
    lines = W.hist_seed_lines()
    assert len(lines) == W.HIST_SEED_LINES == 6000
    assert len(lines) > W.HIST_INLINE_MAX == 5000
    assert len(set(lines)) == len(lines), "distinct lines: a dedupe must not shrink the history below the limit"
    assert all(l.startswith("# ") and "\n" not in l for l in lines), "inert, one line each"
    ds = _src("app", "src", "terminal", "input", "inline_history", "data_source.rs")
    assert "pub(crate) const INLINE_BUILD_MAX_ENTRIES: usize = 5_000;" in ds


@check
def r27_recall_seed_names_the_kill_switch_2107():
    """the kill switch the case documents is the one the app reads, and the gate it guards is in the app"""
    import cases_r14_w1a as W
    assert W.HOLD_KEYS_BEHIND_RECALL_ENV == "PHOSPHOR_HOLD_KEYS_BEHIND_RECALL"
    sw = _src("app", "src", "terminal", "input_queue", "switches.rs")
    assert 'pub const HOLD_KEYS_BEHIND_RECALL_ENV: &str = "PHOSPHOR_HOLD_KEYS_BEHIND_RECALL";' in sw
    gates = _src("app", "src", "terminal", "input_queue", "gates.rs")
    assert "HistoryRecall" in gates and "Duration::from_secs(125)" in gates


@check
def r27_recall_case_seeds_before_it_types_and_still_demands_exact_lines_2107():
    """the case seeds on Linux / macOS first, asserts the seeded count is above the limit, and keeps its exact-lines verdict"""
    import cases_r14_w1a as W
    body = inspect.getsource(W.c_history_up_recall)
    assert body.index("hist_seed_long_history(ctx, r)") < body.index("hist_edit_plan(shell, fname, toks[1])")
    assert "seeded_n > HIST_INLINE_MAX" in body and "got == want" in body and "hist_file_lines(ctx, rec)" in body
    assert 'fast_keys(ctx, ["Up"] + ["BackSpace"] * plan["backspaces"])' in body


@check
def r27_recall_seed_appends_after_the_app_stopped_2107():
    """hist_seed_long_history stops the app, appends the seed to the shell's history file, then starts it; Windows panes and the sim backend are left alone"""
    import cases_r14_w1a as W
    events = []

    class App:
        def stop(self): events.append("stop")
        def start(self): events.append("start")
        def alive(self): return True

    class Be:
        def guard(self): events.append("guard")

    home = tempfile.mkdtemp()
    hist = os.path.join(home, ".bash_history")
    with open(hist, "w") as f:
        f.write("ls\npwd\n")

    class Ctx:
        win = False
        app = App()
        be = Be()
        prof = types.SimpleNamespace(home=home)

        def snapshot_log(self): events.append("snapshot")
        def prompts(self): return 1 if "start" not in events else 3
        def until(self, fn, timeout, step=0.5): return fn()

    class R:
        def __init__(self): self.notes, self.checks = [], []
        def note(self, t): self.notes.append(t)
        def check(self, ok, t): self.checks.append((ok, t)); return ok

    real_sleep = W.time.sleep
    W.time.sleep = lambda s: None
    try:
        r = R()
        n = W.hist_seed_long_history(Ctx(), r)
    finally:
        W.time.sleep = real_sleep
    assert events[:3] == ["snapshot", "stop", "start"], events
    assert n == 6002 > W.HIST_INLINE_MAX, n
    lines = open(hist).read().splitlines()
    assert lines[:2] == ["ls", "pwd"] and len(lines) == 6002 and lines[2] == W.hist_seed_lines()[0]
    # a Windows pane or the sim backend: nothing is touched
    events.clear()
    win = Ctx(); win.win = True
    assert W.hist_seed_long_history(win, R()) is None and events == []
    sim = Ctx(); sim.app = None
    assert W.hist_seed_long_history(sim, R()) is None and events == []
    # no history file: nothing is stopped, and the case says so
    empty = Ctx(); empty.prof = types.SimpleNamespace(home=tempfile.mkdtemp())
    rr = R()
    assert W.hist_seed_long_history(empty, rr) is None and events == [] and rr.notes
