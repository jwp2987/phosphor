# appci/selftest_r23_modeguard.py: offline self-tests of #2170 (view-mode guard before every case, shell-aware PID probe) and the harness half of #2171
# (intl byte comparison). Pure Python, a fake app: no display, no backend.
import os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


class FakeBe:
    name = "x11"

    def __init__(self, app):
        self.app = app
        self.keys = []

    def key(self, spec, repeat=1, delay_ms=15):
        self.keys.append(spec)
        self.app.on_key(spec)


class FakeApp:
    """a pane with a view mode; Escape leaves agent view, the agent chord enters it; `stuck` ignores both; a relaunch resets to `relaunch_mode`"""
    def __init__(self, mode, stuck=False, relaunch_mode="terminal", log=True, ocr=None):
        self.mode, self.stuck, self.relaunch_mode, self.log, self.ocr_mode = mode, stuck, relaunch_mode, log, ocr
        self.relaunches = 0

    def on_key(self, spec):
        if self.stuck:
            return
        if spec == "Escape" and self.mode == "agent":
            self.mode = "terminal"
        elif spec in ("ctrl+shift+Return", "cmd+Return") and self.mode == "terminal":
            self.mode = "agent"

    def relaunch(self):
        self.relaunches += 1
        self.mode = self.relaunch_mode


class FakeCtx:
    def __init__(self, app):
        self.app = app
        self.be = FakeBe(app)

    def log_view_mode(self):
        return self.app.mode if self.app.log else None

    def mode_now(self, tag):
        return self.log_view_mode() or self.app.ocr_mode

    def ocr(self, tag):
        m = self.app.ocr_mode
        return None if m is None else ("ESC for terminal" if m == "agent" else "new terminal session")

    def until(self, fn, timeout, step=0.2):
        return fn()


def _ensure(app, want, **kw):
    import modeguard
    ctx = FakeCtx(app)
    kw.setdefault("relaunch", app.relaunch)
    return modeguard.ensure(ctx, want, "some_case", **kw), ctx


@check
def r23_modeguard_matching_mode_runs_untouched():
    for m in ("terminal", "agent"):
        g, ctx = _ensure(FakeApp(m), m)
        assert g["ok"] and g["ran_mode"] == m and g["recovered"] == [] and ctx.be.keys == [], g


@check
def r23_modeguard_terminal_case_finding_agent_view_recovers_with_escape():
    g, ctx = _ensure(FakeApp("agent"), "terminal")
    assert g["ok"] and g["ran_mode"] == "terminal" and g["recovered"] == ["escape"] and ctx.be.keys == ["Escape"], g
    assert any("put back" in n and "agent" in n for n in g["notes"]), g["notes"]


@check
def r23_modeguard_agent_case_finding_terminal_recovers_with_the_entry_chord():
    g, ctx = _ensure(FakeApp("terminal"), "agent")
    assert g["ok"] and g["ran_mode"] == "agent" and g["recovered"] == ["enter_agent"], g
    assert ctx.be.keys == ["ctrl+shift+Return"], ctx.be.keys


@check
def r23_modeguard_relaunch_is_the_last_resort():
    app = FakeApp("agent", stuck=True, relaunch_mode="terminal")
    g, ctx = _ensure(app, "terminal")
    assert g["ok"] and g["recovered"] == ["escape", "escape", "relaunch"] and app.relaunches == 1, g
    app = FakeApp("terminal", stuck=True, relaunch_mode="agent")
    g, _ = _ensure(app, "agent")
    assert g["ok"] and g["recovered"][-1] == "relaunch" and app.relaunches == 1, g


@check
def r23_modeguard_unrecoverable_is_a_wrong_mode_fail_never_a_pass():
    for want, have in (("terminal", "agent"), ("agent", "terminal")):
        app = FakeApp(have, stuck=True, relaunch_mode=have)
        g, _ = _ensure(app, want)
        assert not g["ok"] and g["ran_mode"] == have, g
        assert g["fail"].startswith("wrong mode: ran in %s, case is %s" % (have, want)), g["fail"]
        assert "relaunch" in g["fail"]
    g, _ = _ensure(FakeApp("agent", stuck=True), "terminal", relaunch=None)     # no relaunch available: still a FAIL
    assert not g["ok"] and "wrong mode" in g["fail"], g


@check
def r23_modeguard_mode_proof_is_observed_and_recorded_not_recovered():
    app = FakeApp("agent")
    g, ctx = _ensure(app, "terminal", exempt=True)
    assert g["ok"] and g["ran_mode"] == "agent" and ctx.be.keys == [] and app.mode == "agent", g


@check
def r23_modeguard_unreadable_mode_is_a_fail_mode_unproven_not_a_pass():
    """#2173: nothing could say (no log line, OCR gave None) -> ran_mode unknown AND the row fails; mode_proof (exempt) records it instead"""
    g, _ = _ensure(FakeApp("agent", log=False), "terminal")
    assert not g["ok"] and g["ran_mode"] == "unknown" and g["fail"].startswith("mode unproven"), g
    assert any("no OCR engine" in n for n in g["notes"]), g["notes"]
    g, _ = _ensure(FakeApp("agent", log=False), "terminal", exempt=True)
    assert g["ok"] and g["ran_mode"] == "unknown" and g["fail"] is None, g


class OcrCtx(FakeCtx):
    """a relaunched app: the new log has no view_mode line (mode_now finds nothing), the screen text is `text` (None = no engine)"""
    def __init__(self, app, text):
        super().__init__(app)
        self.text = text

    def mode_now(self, tag):
        return None

    def ocr(self, tag):
        return self.text


@check
def r25_modeguard_silent_log_falls_back_to_the_ocr_of_ctx_ocr_with_the_shared_markers():
    import modeguard
    # the screen text the Mac backend's Vision read gives, through ctx.ocr(): agent header + ESC bar -> agent; terminal hint -> terminal
    agent = "new agent conversation esc for terminal ci ollama / gpt-oss:20b"
    term = "run commands, or ask the agent a question ctrl-shift-enter new /agent conversation"
    assert modeguard.observe(OcrCtx(FakeApp("agent"), agent), "t")[0] == "agent"
    m, note = modeguard.observe(OcrCtx(FakeApp("terminal"), term), "t")
    assert m == "terminal" and "read from the screen" in note, (m, note)
    # the slash-less read of the terminal hint (Windows OCR) is a terminal screen, never an agent one
    assert modeguard.observe(OcrCtx(FakeApp("terminal"), "ctrl shift enter new agent conversation"), "t")[0] == "terminal"
    # no engine, and text with no marker, are told apart in the note
    m, note = modeguard.observe(OcrCtx(FakeApp("terminal"), None), "t")
    assert m is None and "no OCR engine" in note, (m, note)
    m, note = modeguard.observe(OcrCtx(FakeApp("terminal"), "hello world"), "t")
    assert m is None and "no marker" in note, (m, note)


@check
def r25_modeguard_ensure_uses_the_screen_when_the_log_is_silent_and_fails_unproven_when_that_is_blank():
    import modeguard
    g = modeguard.ensure(OcrCtx(FakeApp("terminal"), "new terminal session"), "terminal", "c")
    assert g["ok"] and g["ran_mode"] == "terminal", g
    g = modeguard.ensure(OcrCtx(FakeApp("terminal"), "hello world"), "terminal", "c")
    assert not g["ok"] and g["ran_mode"] == "unknown" and g["fail"].startswith("mode unproven") and "no marker" in g["fail"], g
    g = modeguard.ensure(OcrCtx(FakeApp("terminal"), None), "agent", "c")
    assert not g["ok"] and "mode unproven" in g["fail"] and "no OCR engine" in g["fail"], g


@check
def r25_ctx_ocr_is_the_one_ocr_path_and_modeguard_reads_through_it():
    src = open(os.path.join(HERE, "modeguard.py"), encoding="utf-8").read()
    assert "ctx.ocr(tag)" in src and "cases.ocr_agent_markers" in src
    assert "subprocess" not in src and "tesseract" not in src.replace("tesseract elsewhere", "")


@check
def r25_the_app_logs_view_mode_for_every_pane_whenever_its_log_is_opened():
    root = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
    ctrl = open(os.path.join(root, "app/src/ai/blocklist/agent_view/controller.rs"), encoding="utf-8").read()
    assert "pub fn relog_view_modes" in ctrl and "impl Drop for AgentViewController" in ctrl
    lib = open(os.path.join(root, "app/src/lib.rs"), encoding="utf-8").read()
    assert "set_on_log_reopened(crate::ai::blocklist::agent_view::relog_view_modes)" in lib
    rot = open(os.path.join(root, "crates/warp_logging/src/rotation.rs"), encoding="utf-8").read()
    assert rot.count("notify_log_reopened();") == 2, "both rotate paths tell the app the file is fresh"


@check
def r23_modeguard_screen_is_used_when_the_log_is_silent_and_a_disagreement_is_named():
    g, _ = _ensure(FakeApp("agent", log=False, ocr="agent"), "terminal")        # log silent: the screen decides; the fake screen never changes -> FAIL
    assert not g["ok"] and g["ran_mode"] == "agent" and "wrong mode" in g["fail"], g
    app = FakeApp("terminal", ocr="agent")
    g, _ = _ensure(app, "terminal")
    assert g["ok"] and g["ran_mode"] == "terminal" and any("screen reads as agent" in n for n in g["notes"]), g


@check
def r23_run_py_guards_every_case_and_records_ran_mode():
    src = open(os.path.join(HERE, "run.py"), encoding="utf-8").read()
    assert "modeguard.ensure(ctx, mode, c[\"name\"]" in src and "if not sim:" in src.split("modeguard.ensure")[0][-900:], "guard missing before the case"
    guard_at = src.index("modeguard.ensure(")
    assert guard_at < src.index("c[\"fn\"](ctx, r)"), "the guard must run before the case function"
    assert src.count("ran_mode=ran_mode") >= 3, "every case row (pass/fail, death, wrong mode) records ran_mode"
    assert "ctx.mode == \"agent\" and c[\"name\"] != \"mode_proof\"" in src      # the post-case agent check is kept
    assert "exempt=(c[\"name\"] == \"mode_proof\")" in src


@check
def r23_pid_probe_follows_the_observed_shell():
    import cases
    assert cases.pid_probe_kinds("cmd", None) == [] and cases.pid_probe_kinds("wsl", None) == []
    assert cases.pid_probe_kinds(None, None) == ["default"] and cases.pid_probe_kinds("gitbash", "posix") == ["default"]
    assert cases.pid_probe_kinds("powershell", None) == ["powershell", "posix"]
    assert cases.pid_probe_kinds("powershell", "powershell") == ["powershell", "posix"]
    assert cases.pid_probe_kinds("powershell", "posix") == ["posix", "powershell"]
    assert cases.pid_probe_line("powershell", "powershell", "t").startswith("Add-Content ")
    assert cases.pid_probe_line("powershell", "posix", "t").startswith("[ -n ") and "Add-Content" not in cases.pid_probe_line("powershell", "posix", "t")
    assert cases.pid_probe_line(None, "default", "t").startswith("sh -c ")


class ProbeSelf:
    """stands in for Ctx in shell_pid: records the typed lines; `front` is the shell that answers ('powershell' | 'posix')"""
    def __init__(self, pane, front, kind=None):
        self.pane, self.front, self.shell_kind, self.typed, self.n = pane, front, kind, [], 0

    def ptok(self, n=8):
        self.n += 1
        return "T%d" % self.n

    def probe(self, cmd, token, name, timeout=8):
        self.typed.append(cmd)
        runs_in = "powershell" if cmd.startswith("Add-Content") else "posix"
        if self.pane != "powershell" or runs_in == self.front:
            return "local", ["%s:4242" % token]
        return "none", []


@check
def r23_shell_pid_probe_never_types_powershell_into_a_known_bash_and_remembers_the_answer():
    import cases
    # a Git Bash nested in the PowerShell pane, observed once: every later probe is POSIX syntax, never Add-Content
    p = ProbeSelf("powershell", "posix", kind="posix")
    assert cases.Ctx.shell_pid(p) == "4242" and len(p.typed) == 1 and not p.typed[0].startswith("Add-Content"), p.typed
    # unknown shell in front, a bash: PowerShell syntax first, then the POSIX one answers and is remembered
    p = ProbeSelf("powershell", "posix")
    assert cases.Ctx.shell_pid(p) == "4242" and p.shell_kind == "posix" and p.typed[0].startswith("Add-Content") and not p.typed[1].startswith("Add-Content"), p.typed
    # the pane's own PowerShell: one probe, in PowerShell syntax
    p = ProbeSelf("powershell", "powershell")
    assert cases.Ctx.shell_pid(p) == "4242" and p.shell_kind == "powershell" and len(p.typed) == 1
    # cmd.exe has no probe at all
    p = ProbeSelf("cmd", "posix")
    assert cases.Ctx.shell_pid(p) is None and p.typed == []
    # a Git Bash pane / POSIX pane: the shell-agnostic `sh -c` only
    p = ProbeSelf("gitbash", "posix")
    assert cases.Ctx.shell_pid(p) == "4242" and p.typed[0].startswith("sh -c ") and len(p.typed) == 1


@check
def r23_leave_nested_and_relaunch_forget_the_remembered_shell():
    src = open(os.path.join(HERE, "cases.py"), encoding="utf-8").read()
    seg = src[src.index("    def leave_nested("):src.index('        return "nested"', src.index("    def leave_nested("))]
    assert seg.count("self.shell_kind = None") >= 2, "both `exit` sites must forget the remembered shell"
    assert "ctx.shell_kind = None" in open(os.path.join(HERE, "run.py"), encoding="utf-8").read().split("def relaunch(")[1].split("def set_ps_pane_state")[0]


@check
def r23_intl_effect_file_is_compared_to_the_exact_utf8_and_question_marks_fail():
    import cases_caseintl as I
    for tag, text in I.WIDE:
        ok, msg = I.intl_file_verdict((text + "\n").encode("utf-8"), text)
        assert ok, (tag, msg)
        bad, msg = I.intl_file_verdict(("".join(c if ord(c) < 128 else "?" for c in text) + "\n").encode(), text)
        if any(ord(c) >= 128 for c in text):
            assert not bad and "'?' SUBSTITUTION" in msg, (tag, msg)
    # any other difference fails too: dropped ZWJ, UTF-16, missing newline, no file
    zwj = dict(I.WIDE)["zwj"]
    assert not I.intl_file_verdict((zwj.replace("‍", "") + "\n").encode(), zwj)[0]
    assert not I.intl_file_verdict((zwj + "\n").encode("utf-16"), zwj)[0]
    assert not I.intl_file_verdict(zwj.encode(), zwj)[0]
    ok, msg = I.intl_file_verdict(None, "x")
    assert not ok and "no effect file" in msg


class FakeR:
    def __init__(self):
        self.metrics, self.fails, self.notes = {}, [], []

    def check(self, ok, msg):
        if not ok:
            self.fails.append(msg)
        return ok

    def note(self, m):
        self.notes.append(m)


class FakeIntlCtx:
    def __init__(self, commands, cmds):
        self.W = None
        self.pane = "powershell"
        self._cmds = cmds

    def await_commands(self, mark, pred):
        return self._cmds


@check
def r23_intl_row_says_what_it_verified_when_rec_log_is_empty():
    import cases_caseintl as I
    r = FakeR()
    I.check_cmd(FakeIntlCtx(None, []), r, 0, "echo x", "typed cjk")
    assert not r.fails and "NOT verified" in r.notes[0] and r.metrics["intl_unchecked_received_command_line"] == 1, (r.fails, r.notes, r.metrics)
    r.metrics["intl_checked_file_bytes"] = 5
    s = I.verified_summary(r)
    assert "5 effect-file byte" in s and "NOT verified 1" in s, s
    r2 = FakeR()
    I.check_cmd(FakeIntlCtx(None, ["echo a?b"]), r2, 0, "echo a日b", "typed")
    assert r2.fails and "shell received" in r2.fails[0], r2.fails
    r3 = FakeR()
    I.check_cmd(FakeIntlCtx(None, ["echo a日b"]), r3, 0, "echo a日b", "typed")
    assert not r3.fails and r3.metrics.get("intl_checked_received_command_line") == 1, r3.metrics


@check
def r24_a_terminal_case_that_ends_in_agent_view_is_blamed():
    """owner 2026-11-12: after a TERMINAL case the pane must be in terminal mode; ending in agent view is a FAIL row naming that case, unless the case
    declares enters_agent=True"""
    import cases
    src = open(os.path.join(HERE, "run.py"), encoding="utf-8").read()
    assert 'ctx.mode == "terminal" and c["name"] != "mode_proof" and not c.get("enters_agent")' in src
    assert '"entered_agent_view_in_" + c["name"]' in src and 'seen_t == "agent"' in src
    i_guard = src.index('"entered_agent_view_in_"')
    i_agent_check = src.index('"left_agent_mode_after_"')
    assert i_agent_check < i_guard, "both post-case mode checks exist (agent pass and terminal pass)"
    import inspect
    assert "enters_agent" in inspect.signature(cases.case).parameters, "cases.case() takes enters_agent (cases registered another way read it as False via c.get)"


@check
def r24_mode_proof_ocr_hint_without_slash_is_not_agent():
    """r24a Windows mode_proof false FAIL: Windows OCR dropped the '/' of the terminal hint 'ctrl-shift-<enter> new /agent conversation', which then
    matched the agent header. The hint (with or without the slash) is never the header; a real header or ESC bar still counts."""
    import cases
    f = lambda t: cases.ocr_agent_markers(" ".join(t.lower().split()))[0]
    assert not f("run commands, or ask the agent a question ctrl-shift-↵ new /agent conversation")
    assert not f("ctrl shift enter new agent conversation ctrl-shift-↵ new /agent conversation")
    assert f("new agent conversation esc for terminal ci ollama / gpt-oss:20b")
    assert f("make a pr esc for terminal ctrl alt r to resume conversation")
    assert f("new agent conversation ... ctrl-shift-↵ new /agent conversation")
