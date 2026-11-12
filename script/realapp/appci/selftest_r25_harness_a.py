# appci/selftest_r25_harness_a.py: offline self-tests of round 25 harness batch A (#2146 leaked tabs, #2170 nested POSIX shell probe order, the lab
# append-line helper for fish and nu arms). Pure Python: no app, no display, no lab. selftest.py appends CHECKS to its own list.
import ast, inspect, os, sys, types

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


class TabRing:
    """a model of the app's tab strip: pids of the shells, the front index, the keys the cases send. Streaming tabs answer no PID probe until Ctrl-C."""
    def __init__(self, own="100"):
        self.tabs = [{"pid": own, "streaming": False}]
        self.front = 0
        self.nxt = 1000
        self.keys = []
        self.closed = []

    def new(self, streaming=False):
        self.nxt += 1
        self.tabs.append({"pid": str(self.nxt), "streaming": streaming})
        self.front = len(self.tabs) - 1

    def key(self, k, *a, **kw):
        self.keys.append(k)
        n = len(self.tabs)
        if k == "NEW":
            self.new()
        elif k == "NEXT":
            self.front = (self.front + 1) % n
        elif k == "PREV":
            self.front = (self.front - 1) % n
        elif k == "ctrl+c":
            self.tabs[self.front]["streaming"] = False
        elif k == "CLOSE" and n > 1:
            self.closed.append(self.tabs.pop(self.front)["pid"])
            self.front = min(self.front, len(self.tabs) - 1)

    def shell_pid(self, timeout=8):
        t = self.tabs[self.front]
        return None if t["streaming"] else t["pid"]

    def ctx(self):
        return types.SimpleNamespace(be=types.SimpleNamespace(key=self.key), shell_pid=self.shell_pid, reset=lambda: None)


def _r(fails, notes):
    return types.SimpleNamespace(check=lambda ok, msg: (fails.append(msg) if not ok else None) or ok, note=notes.append)


@check
def r25_cb_ui_action_latency_closes_only_the_throwaway_tab_among_n_leaked_tabs():
    """#2146 (round 24): 13 leaked tabs stand between the pass's own tab and the throw-away one; next-tab wrapped onto a still-streaming tab and the close
    key closed it. The throw-away tab's pid is read when it is opened and only that pid's tab is closed, with any number of other tabs."""
    import cases_coverage_b as B
    import cases_robust as R
    saved = R.CLOSE_PANE
    R.CLOSE_PANE = "CLOSE"
    try:
        for leaked in (0, 1, 2, 13):
            ring = TabRing()
            for _ in range(leaked):
                ring.new()                                 # leaked tabs; the pass's own tab is the FIRST, the front is the last leaked one
            own = ring.tabs[0]["pid"]
            ring.front = 0
            ring.key("NEW")                                 # cb_ui_action_latency: new tab (in front, last), its pid recorded
            throw = ring.shell_pid()
            ring.key("NEXT")                                # the tab-switch timing: front is now the first tab
            fails, notes = [], []
            res = B.close_throwaway_tab(ring.ctx(), _r(fails, notes), own, throw, "PREV")
            assert res == "closed" and ring.closed == [throw], (leaked, res, ring.closed, ring.keys)
            assert not [f for f in fails if "NOT sent" in f], fails
        # a throw-away tab that cannot be told apart: nothing is closed
        ring = TabRing()
        ring.new()
        fails = []
        assert B.close_throwaway_tab(ring.ctx(), _r(fails, []), "100", None, "PREV") == "refused" and ring.closed == [] and "CLOSE" not in ring.keys
    finally:
        R.CLOSE_PANE = saved


@check
def r25_tabs_streaming_and_two_windows_close_the_tabs_they_opened():
    """#2146: the 12+1 streaming tabs of tabs_streaming (and the window and tabs of tabs_two_windows) are closed in a finally, newest first, each proven
    by pid; the pass's own tab is never closed; a count that does not match fails instead of closing blind."""
    import cases
    import cases_robust as R
    saved = R.CLOSE_PANE
    R.CLOSE_PANE = "CLOSE"
    try:
        for n in (1, 4, 13):
            ring = TabRing()
            for _ in range(n):
                ring.new(streaming=True)
            fails, notes = [], []
            sent = cases.close_opened_tabs(ring.ctx(), _r(fails, notes), "100", n, "t")
            assert sent == n and len(ring.tabs) == 1 and ring.tabs[0]["pid"] == "100" and not fails, (n, sent, fails, ring.keys)
        # fewer tabs open than the case counted (a tab died on its own): stops at the pass's own tab, closes nothing more
        ring = TabRing(); ring.new(streaming=True)
        fails = []
        assert cases.close_opened_tabs(ring.ctx(), _r(fails, []), "100", 5, "t") == 1 and len(ring.tabs) == 1 and not fails
        # more tabs open than counted: the check fails, it does not close the pass's own tab
        ring = TabRing()
        for _ in range(3):
            ring.new(streaming=True)
        fails = []
        cases.close_opened_tabs(ring.ctx(), _r(fails, []), "100", 2, "t")
        assert len(ring.tabs) == 2 and len(fails) == 1 and "still open" in fails[0], (len(ring.tabs), fails)
        # a pane with no PID probe: nothing is closed blind
        ring = TabRing(); ring.new()
        assert cases.close_opened_tabs(ring.ctx(), _r([], []), None, 1, "t") == 0 and ring.closed == []
        # no shell answers: a failure naming it, no close key sent
        ring = TabRing(); ring.new(streaming=True)
        ring.key = lambda k, *a, **kw: ring.keys.append(k)           # Ctrl-C does not stop the stream
        fails = []
        assert cases.close_opened_tabs(ring.ctx(), _r(fails, []), "100", 1, "t") == 0 and len(fails) == 1 and "no shell answered" in fails[0] and "CLOSE" not in ring.keys
    finally:
        R.CLOSE_PANE = saved
    for fn, marker in ((cases.c_tabs_streaming, "_tabs_streaming(ctx, r, st)"), (cases.c_tabs, "_tabs_two_windows(ctx, r, st)")):
        src = inspect.getsource(fn)
        assert src.index("own_pid = ctx.shell_pid()") < src.index("try:") < src.index(marker) < src.index("finally:") < src.index("close_opened_tabs(ctx, r, own_pid, st[\"opened\"]"), fn
    assert inspect.getsource(cases._tabs_streaming).count('st["opened"] += 1') == 2
    two = inspect.getsource(cases._tabs_two_windows)
    assert two.count('st["opened"] += 1') == 2 and two.index("ctx.be.key(newwin)") < two.rindex('st["opened"] += 1')


@check
def r25_mptr_looks_through_only_the_cursor_sprite_window():
    """#2118 follow-up: the pointer's fence skips the Window Server's mouse-cursor sprite (layer at/above the cursor level, under 64x64 pt) like the Dock
    backing window, and nothing else of the Window Server's. A Python mirror of the rule is checked against the constants in the Swift text."""
    src = open(os.path.join(HERE, "..", "mac", "tools", "mptr.swift"), encoding="utf-8").read()
    assert "func isCursorSprite(_ own: String, _ layer: Int, _ r: CGRect) -> Bool {\n  return own == \"Window Server\" && layer >= cursorLayer && r.width < 64 && r.height < 64\n}" in src
    assert "let cursorLayer = Int(CGWindowLevelForKey(.cursorWindow))" in src
    own_at = src[src.index("func ownerAt("):src.index("func describeWindowsAt(")]
    assert "isCursorSprite(w.1, w.2, w.3) { lookedThroughCursor = true; continue }" in own_at
    assert "[mouse-cursor sprite window, looked through]" in src and "NOTE looked through the mouse-cursor sprite window" in src
    CURSOR = 2147483630

    def sprite(own, layer, w, h):                     # the rule, as written in the Swift text
        return own == "Window Server" and layer >= CURSOR and w < 64 and h < 64
    assert sprite("Window Server", 2147483630, 28, 40)                     # the r24a wheel_scroll window
    assert sprite("Window Server", 2147483631, 63, 63)
    assert not sprite("Window Server", 2147483630, 64, 40) and not sprite("Window Server", 2147483630, 28, 64)      # too big
    assert not sprite("Window Server", 25, 28, 40) and not sprite("Window Server", 101, 20, 20)        # a dialog or a menu level: still refused
    assert not sprite("Dock", 2147483630, 28, 40) and not sprite("Phosphor", 2147483630, 28, 40)       # not the window server's
    # every Window Server window that is not the sprite is still an obstacle: the skip is not by owner alone
    assert 'ignoreOwners.contains("Window Server")' not in src and src.count('own == "Window Server"') == 1


@check
def r25_nested_posix_shell_makes_the_post_case_probe_posix_first():
    """#2170 follow-up: after a case opened a nested POSIX shell (Git Bash from the PowerShell pane) the post-case PID probe types the POSIX form first,
    never `Add-Content ...` into bash. Unknown shell + a nested start -> POSIX first; no nested start -> the pane's own form first (as before)."""
    import cases
    K = cases.pid_probe_kinds
    assert K("powershell", None) == ["powershell", "posix"] and K("powershell", None, False) == ["powershell", "posix"]
    assert K("powershell", None, True) == ["posix", "powershell"]
    assert K("powershell", "posix", False) == ["posix", "powershell"] and K("powershell", "powershell", False) == ["powershell", "posix"]
    assert K("cmd", None, True) == [] and K("gitbash", None, True) == ["default"] and K(None, None, True) == ["default"]

    class P:
        pane, shell_kind, nested_posix = "powershell", "powershell", False
        typed = []

        def ptok(self, n=8):
            return "T"

        def probe(self, cmd, token, name, timeout=8):
            self.typed.append(cmd)
            return ("local", ["%s:4242" % token]) if not cmd.startswith("Add-Content") else ("none", [])
    p = P()
    cases.Ctx.note_nested_posix(p)
    assert p.shell_kind is None and p.nested_posix is True
    assert cases.Ctx.shell_pid(p) == "4242" and len(p.typed) == 1 and not p.typed[0].startswith("Add-Content"), p.typed
    assert p.shell_kind == "posix"
    # every case that starts a Git Bash from the PowerShell pane says so; the flag is cleared where the shell is left or the app relaunched
    src = open(os.path.join(HERE, "cases.py"), encoding="utf-8").read()
    gb = inspect.getsource(cases.c_gitbash_subshell)
    assert gb.index("--noprofile --norc -i") < gb.index("ctx.note_nested_posix()")
    assert "ctx.note_nested_posix()" in open(os.path.join(HERE, "cases_caseintl.py"), encoding="utf-8").read()
    assert src.count("self.nested_posix = False") >= 2
    assert "ctx.nested_posix = False" in open(os.path.join(HERE, "run.py"), encoding="utf-8").read().split("def relaunch(")[1].split("def set_ps_pane_state")[0]


@check
def r25_lab_append_lines_are_built_in_one_place_and_right_for_nu():
    """lab cases judge fish and nu fully: every line that appends to a file in the remote shell is built by cases_lab.append_line / pwd_append_line /
    then_line. nu gets `save --append` with the newline spelled out (the same bytes `echo >>` writes), never `>>` (which prints a table there)."""
    import cases
    import cases_lab as CL
    nu, fish, bash = {"shell": "nu"}, {"shell": "fish"}, {"shell": "bash"}
    assert CL.append_line(nu, "tok1", "/work/u/f.txt") == '"tok1\\n" | save --append /work/u/f.txt'
    assert CL.append_line(nu, "tok1", "~/f") == cases.Ctx.append_cmd(None, "nu", "tok1", "~/f"), "the same line Ctx.append_cmd builds for nu"
    assert CL.append_line(fish, "tok1", "/w/f") == "echo tok1 >> /w/f" == CL.append_line(bash, "tok1", "/w/f") == cases.Ctx.append_cmd(None, "bash", "tok1", "/w/f")
    assert CL.append_line("zsh", "a b", "f") == "echo a b >> f" and CL.append_line("nu", "a b", "f") == '"a b\\n" | save --append f'
    for shell in ("bash", "zsh", "fish", "dash", "ash"):
        assert ">>" in CL.append_line(shell, "t", "f") and "save" not in CL.append_line(shell, "t", "f")
    assert ">>" not in CL.append_line("nu", "t", "f") and ">>" not in CL.pwd_append_line("nu", "f") and "&&" not in CL.then_line("nu")
    assert CL.pwd_append_line(nu, "/w/tab.txt") == '$"($env.PWD)\\n" | save --append /w/tab.txt' and CL.pwd_append_line(fish, "/w/t") == "pwd >> /w/t"
    assert CL.then_line(nu) == " ; " and CL.then_line(fish) == " && " == CL.then_line(bash), "fish >= 3.0 runs &&; the lab image is trixie (fish 4.x)"
    try:
        CL.append_line(nu, 'a"b', "f")
        raise AssertionError("a nu line with a quote must be refused, not built")
    except AssertionError as e:
        assert "plain text" in str(e)
    cont = open(os.path.join(HERE, "..", "sshlab", "Containerfile"), encoding="utf-8").read()
    assert "trixie" in cont and "fish" in cont.split("FROM common AS full")[1].split("COPY")[0]


@check
def r25_no_lab_case_builds_a_raw_append_line_for_a_nu_arm():
    """every string constant of the lab case files that holds `>>` (docstrings aside) is either the helper's own line, a line run by `sh` (sh -c, the
    shell-neutral executor), a zsh-only line, a bash/zsh-only stamp, or a nu-specific line. A new raw `echo X >> f` typed into the pane fails here."""
    files = sorted(f for f in os.listdir(HERE) if f.startswith("cases_lab") and f.endswith(".py")) + ["lab.py"]
    bad = []
    for fn in files:
        tree = ast.parse(open(os.path.join(HERE, fn), encoding="utf-8").read())
        docs = set()
        for n in ast.walk(tree):
            if isinstance(n, (ast.Module, ast.FunctionDef, ast.ClassDef)) and n.body and isinstance(n.body[0], ast.Expr) and isinstance(n.body[0].value, ast.Constant):
                docs.add(id(n.body[0].value))
        for n in ast.walk(tree):
            if isinstance(n, ast.Constant) and isinstance(n.value, str) and ">>" in n.value and id(n) not in docs:
                v = n.value
                if v in ("echo %s >> %s", "pwd >> %s") or any(a in v for a in ("sh -c", "^sh -c", "print -l $preexec_functions", "$EPOCHREALTIME")):
                    continue
                if fn == "cases_lab_diff.py" and v.startswith("cd %s && echo two >>"):
                    continue                       # CLG.run: the script goes through `sh -c` / base64 | sh
                if fn == "cases_lab_index.py" and v.startswith("echo delta epsilon >>"):
                    continue                       # wrapped in CLG.sh_c(...) at the call
                if fn == "lab.py" and "$out" in v:
                    continue                       # the hook script, run by the lab user's sh
                bad.append((fn, n.lineno, v[:80]))
    assert not bad, "a lab case builds a raw `>>` line: use cases_lab.append_line / pwd_append_line (nu prints a table for `echo x >> f`): %r" % bad
    # and the cases that type into the pane use the helper
    for func in ("c_tab_remote_dir", "c_lab_drop", "c_sh_login_profile"):
        src = inspect.getsource(getattr(__import__("cases_lab"), func))
        assert "append_line(" in src or "pwd_append_line(" in src, func
    assert "CLG.sh_c(" in open(os.path.join(HERE, "cases_lab_index.py"), encoding="utf-8").read()
