# appci/selftest_macptr.py: offline self-tests of round 15 wave 4, the macOS harness (#2061, r14-test-gaps T32): macptr.py, cases_macptr.py, the
# MacBackend chord rule and pointer call, and the gates of the cases enabled on macOS. Pure Python: no app, no display, no Mac. selftest.py appends
# CHECKS to its own list, so `run.py --self-test` and `selftest.py` both run these.
import inspect, os, struct, sys, types

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import backend, cases, winmap  # noqa: E402
import cases_agent_resilience as AR  # noqa: E402
import cases_macptr as CM  # noqa: E402
import cases_pointer as P  # noqa: E402
import cases_r14_w1a as W1  # noqa: E402
import cases_robust as R  # noqa: E402
import cases_ui as U  # noqa: E402
import cases_window_use as WU  # noqa: E402
import cases_wrap_vtabs as WV  # noqa: E402
import macptr as MP  # noqa: E402

CHECKS = []
ROOT = os.path.join(HERE, "..", "..", "..")
TOOLS_DIR = os.path.join(HERE, "..", "mac", "tools")


def check(f):
    CHECKS.append(f)
    return f


class Skipped(Exception):
    pass


class Rec:
    """a result stand-in: skip raises, checks and notes are kept"""

    def __init__(self):
        self.checks, self.notes = [], []

    def skip(self, msg):
        raise Skipped(msg)

    def check(self, ok, msg=None):
        self.checks.append((bool(ok), msg))
        return bool(ok)

    def note(self, msg):
        self.notes.append(msg)

    def metric(self, k, v):
        pass


def _mac_be(tools=""):
    be = backend.MacBackend()
    be.TOOLS = tools
    return be


@check
def macptr_registry_and_docs_2061():
    """the proof case is registered once, PROVING only (never SMOKE, never the fake pass), both modes, gui, ordered right after mode_proof and before
    every case that uses the pointer or the chords; Windows SKIPs it with a reason; docs/testing.md describes the tools, the rule and every case
    enabled on macOS; the tools the job compiles exist as sources and the job compiles the new ones without failing the run on a compile error"""
    reg = {c["name"]: c for c in cases.CASES}
    n = "mac_pointer_chord_proof"
    assert n in reg and n in cases.PROVING and n not in cases.SMOKE and n not in cases.PROVING_FAKE
    assert reg[n]["gui"] and reg[n]["modes"] == ("terminal", "agent") and reg[n]["order"] == 2 and reg["mode_proof"]["order"] < 2
    enabled = ("ui_mouse_select_copy", "ui_copy_wrapped_drag", "ui_vertical_tabs_panel_click", "selection_modes", "context_menu_actions", "tab_pointer_use",
               "link_and_path_click", "quit_24_tabs", "agent_restart_quit_stream", "agent_restart_quit_queued", "agent_restart_quit_blocked", "tab_lifecycle_keys")
    for e in enabled:
        assert e in reg and reg[e]["order"] > 2, e
    assert n in winmap.NOT_WINDOWS and all(winmap.skip_reason(n, pane) for pane in ("powershell", "gitbash"))
    assert "macOS only" in CM.MAC_ONLY
    doc = open(os.path.join(ROOT, "docs", "testing.md"), encoding="utf-8").read()
    for needle in ["`%s`" % n, "selftest_macptr.py", "mptr.swift", "appkey.swift", "ocrw.swift", "openrec.swift", "open_recorder.sh", "APPCI_MAC_CHORD_VIA",
                   "#2061"] + ["`%s`" % e for e in enabled if not e.startswith("agent_restart_quit_")] + ["agent_restart_quit_"]:
        assert needle in doc, "docs/testing.md does not mention %s" % needle
    for t in ("mptr", "appkey", "ocrw", "openrec"):
        assert os.path.isfile(os.path.join(TOOLS_DIR, t + ".swift")), t
    job = open(os.path.join(HERE, "..", "mac", "ci", "app_test.sh"), encoding="utf-8").read()
    assert "for t in mptr appkey ocrw openrec; do" in job and 'APPCI_KEY_CLOSE=""' in job
    opt = job[job.index("for t in mptr appkey ocrw openrec; do"):]
    assert "fail3" not in opt[:opt.index("done")], "a compile error in a round-15 tool must not stop the whole macOS run (the proof case reports it)"
    rec = open(os.path.join(HERE, "..", "mac", "ci", "open_recorder.sh"), encoding="utf-8").read()
    assert MP.RECORDER_APP in rec and "open_recorder.sh install" in MP.RECORDER_NEEDS and MP.RECORDER_NEEDS.startswith("NEEDS: ")
    # the tools' own fences are in their sources (nothing was compiled here: these pin the text the design rests on)
    mptr = open(os.path.join(TOOLS_DIR, "mptr.swift"), encoding="utf-8").read()
    assert 'mustOwn(p, "a click")' in mptr and 'mustOwn(a, "a drag start"); mustOwn(b, "a drag end")' in mptr and 'mustOwn(p, "a wheel step")' in mptr
    assert "finish(4," in mptr and "setMods(0)" in mptr[mptr.index("func finish"):mptr.index("func mustOwn")]
    key = open(os.path.join(TOOLS_DIR, "appkey.swift"), encoding="utf-8").read()
    assert "(code == 12 && mods == 8) || (code == 13 && (mods == 8 || mods == 9))" in key and "front?.processIdentifier == pid" in key
    assert 'exe.hasPrefix(want)' in key and "e.postToPid(pid)" in key


@check
def macptr_chord_rule():
    """the chord rule: exactly Cmd-W, Cmd-Shift-W and Cmd-Q are app chords; htype2 never carries them; everything that was refused stays refused; an
    app chord is refused for the reference terminal, an unknown PID, a bundle that is not the job's, a missing tool; keys() keeps the order"""
    M = backend.MacBackend
    for spec, kind in (("cmd+q", "app"), ("Cmd+W", "app"), ("cmd+shift+w", "app"), ("meta+w", "app"), ("super+q", "app"),
                       ("cmd+shift+q", "refused"), ("cmd+alt+w", "refused"), ("cmd+alt+q", "refused"), ("ctrl+cmd+q", "refused"), ("ctrl+cmd+f", "refused"),
                       ("cmd+h", "refused"), ("cmd+m", "refused"), ("cmd+tab", "refused"), ("cmd+space", "refused"), ("globe", "refused"), ("fn", "refused"),
                       ("cmd+t", "plain"), ("cmd+shift+t", "plain"), ("ctrl+w", "plain"), ("ctrl+c", "plain"), ("Return", "plain"), ("cmd+v", "plain"), ("w", "plain")):
        assert M.chord_kind(spec) == kind, (spec, M.chord_kind(spec))
    assert M.APP_CHORDS == {(8, "q"): 12, (8, "w"): 13, (9, "w"): 13} and M.LETTERS["q"] == 12 and M.LETTERS["w"] == 13
    be = _mac_be()
    for spec in ("cmd+q", "cmd+w", "cmd+shift+w", "cmd+h", "cmd+shift+q"):        # htype2 carries none of them
        try:
            be._kspec(spec)
            raise AssertionError("htype2 would carry " + spec)
        except ValueError as e:
            assert "refusing forbidden chord" in str(e)
    assert be._kspec("cmd+t") and be._kspec("cmd+shift+Return")
    try:
        M.chord_kind("hyper+w")
        raise AssertionError("an unknown modifier was accepted")
    except ValueError:
        pass
    # the refusal reasons, each alone
    be.BUNDLE, be.CHORD_VIA, be.guard_override = "/job/out/Phosphor.app", "pid", None
    assert be.app_chord_refusal("cmd+q", 4242) is None and be.app_chord_refusal("cmd+shift+w", 4242) is None
    assert "not one of the app chords" in be.app_chord_refusal("cmd+t", 4242) and "not one of the app chords" in be.app_chord_refusal("cmd+h", 4242)
    assert "PID unknown" in be.app_chord_refusal("cmd+q", None)
    be.guard_override = "Terminal"
    assert "reference terminal" in be.app_chord_refusal("cmd+w", 4242)
    be.guard_override = None
    for bad in ("", "Phosphor.app", "/Applications/Phosphor", "relative/Phosphor.app"):
        be.BUNDLE = bad
        assert "APPCI_MAC_BUNDLE" in be.app_chord_refusal("cmd+q", 4242), bad
    be.BUNDLE, be.CHORD_VIA = "/job/out/Phosphor.app", "applescript"
    assert "APPCI_MAC_CHORD_VIA" in be.app_chord_refusal("cmd+q", 4242)
    be.CHORD_VIA = "pid"
    # app_chord: refusal and a missing tool raise BEFORE the guard or any tool runs
    ran = []
    be.guard = lambda: ran.append("guard")
    be.pid_fn = lambda: 4242
    be.guard_override = "Terminal"
    try:
        be.app_chord("cmd+q")
        raise AssertionError("sent to the reference terminal")
    except RuntimeError as e:
        assert "reference terminal" in str(e) and ran == []
    be.guard_override = None
    try:
        be.app_chord("cmd+q")
        raise AssertionError("no tool, yet no error")
    except RuntimeError as e:
        assert "appkey tool is missing" in str(e) and ran == []
    # keys(): [Return, cmd+q, Tab, Return] = an htype2 run, the app chord, an htype2 run; a refused chord anywhere sends nothing after it
    sent = []
    be._ht = lambda ops, guard=True: sent.append(("ht", list(ops)))
    be.app_chord = lambda spec: sent.append(("app", spec))
    be.keys(["Return", "cmd+q", "Tab", "Return"], delay_ms=0)
    assert sent == [("ht", ["k:36:0:0"]), ("app", "cmd+q"), ("ht", ["k:48:0:0", "k:36:0:0"])], sent
    del sent[:]
    be.key("cmd+w")
    assert sent == [("app", "cmd+w")]
    del sent[:]
    try:
        be.keys(["cmd+h", "Return"], delay_ms=0)
        raise AssertionError("cmd+h was not refused")
    except ValueError:
        assert sent == []
    # the injector accounting sees the app chord as one Cmd key-down, exactly like an htype2 k: op
    assert M.cmd_chords(["k:12:8:0"]) == 1 and M.cmd_chords(["k:13:9:0"]) == 1 and M.cmd_chords(["k:36:0:0"]) == 0


@check
def macptr_pointer_math_and_ops():
    """screenshot pixels -> points -> mptr ops; the scale is a whole number or nothing; modifiers wrap the ops; the read-back proof"""
    png = b"\x89PNG\r\n\x1a\n" + struct.pack(">I", 13) + b"IHDR" + struct.pack(">II", 3024, 1964) + b"\x08\x06\x00\x00\x00"
    assert MP.png_size(png) == (3024, 1964) and MP.png_size(png[:20]) is None and MP.png_size(b"GIF89a" + png) is None
    assert MP.display_scale(3024, 1512) == 2 and MP.display_scale(1512, 1512) == 1 and MP.display_scale(4536, 1512) == 3
    assert MP.display_scale(3024, 1800) is None and MP.display_scale(0, 1512) is None and MP.display_scale(3024, 0) is None      # 1.68: not this display
    assert MP.display_scale(3020, 1512) == 2 and MP.display_scale(2900, 1512) is None
    assert MP.to_points(301, 99, 2) == (150.5, 49.5) and MP.to_points(300, 100, 1) == (300.0, 100.0)
    assert MP.move_op(300, 100, 2) == "m:150:50" and MP.click_op(301, 99, 1, 2, 2) == "k:150.5:49.5:1:2"
    assert MP.click_op(10, 10, 3, 1, 1) == "k:10:10:3:1" and MP.drag_op(100, 40, 300, 80, 2) == "g:50:20:150:40" and MP.wheel_op(100, 40, -3, 2) == "w:50:20:-3"
    for bad in ((10, 10, 4, 1), (10, 10, 0, 1), (10, 10, 1, 4), (10, 10, 1, 0)):
        try:
            MP.click_op(*bad, 2)
            raise AssertionError(bad)
        except ValueError:
            pass
    assert MP.mods_bits(["shift"]) == 1 and MP.mods_bits(["ctrl"]) == 8 and MP.mods_bits(["cmd", "shift"]) == 9 and MP.mods_bits(["alt"]) == 4
    try:
        MP.mods_bits(["hyper"])
        raise AssertionError("unknown modifier")
    except ValueError:
        pass
    assert MP.with_mods(["k:1:1:1:1"], 0) == ["k:1:1:1:1"] and MP.with_mods(["k:1:1:1:1"], 8) == ["f:8", "k:1:1:1:1", "f:0"]
    assert MP.arrived((150.4, 49.6), (150.5, 49.5)) and not MP.arrived((10.0, 10.0), (150.5, 49.5)) and not MP.arrived(None, (1, 1))
    assert MP.origin_px((100.0, 50.0, 1200.0, 800.0), 2) == (200, 100) and MP.origin_px(None, 2) is None and MP.origin_px((1, 1, 1, 1), None) is None
    # every op MacMouse can build is one mptr.swift parses (the op letters and their field counts)
    src = open(os.path.join(TOOLS_DIR, "mptr.swift"), encoding="utf-8").read()
    for letter, count in (("m", 3), ("k", 5), ("g", 5), ("w", 4), ("f", 2), ("p", 2)):
        assert 'case "%s": if p.count == %d' % (letter, count) in src, letter
    assert len(MP.click_op(1, 1, 1, 1, 2).split(":")) == 5 and len(MP.drag_op(1, 1, 2, 2, 2).split(":")) == 5 and len(MP.wheel_op(1, 1, 1, 2).split(":")) == 4

    # MacMouse: the same methods and unit as cases_ui.Mouse; Held's modifier wraps the click; a failed scale is an error, a missing tool a NEEDS skip
    calls = []
    be = types.SimpleNamespace(name="mac", TOOLS="", ptr=lambda ops: calls.append(list(ops)) or (150.5, 49.5), ptr_info=lambda: {}, shot=lambda p: False)
    m = MP.MacMouse(be)
    assert m.scale is None and "display line" in m.scale_how
    m.scale = 2
    m.click(301, 99, 1, 2)
    assert calls[-1] == ["k:150.5:49.5:1:2"] and m.delivered() is True
    m.mods = MP.mods_bits(["ctrl"])
    m.click(301, 99)
    assert calls[-1] == ["f:8", "k:150.5:49.5:1:1", "f:0"]
    m.mods = 0
    m.drag(100, 40, 300, 80)
    assert calls[-1] == ["g:50:20:150:40"] and m.delivered() is False          # the stand-in cursor is not at the drag's end
    for name in ("move", "click", "drag"):
        assert inspect.signature(getattr(MP.MacMouse, name)).parameters.keys() == inspect.signature(getattr(U.Mouse, name)).parameters.keys(), name
    ctx = types.SimpleNamespace(be=_mac_be())
    for kw, want in ((dict(), "not enabled"), (dict(mac=True), "NEEDS: the mptr tool")):
        try:
            U.mouse_or_skip(ctx, Rec(), **kw)
            raise AssertionError("no skip")
        except Skipped as e:
            assert want in str(e), str(e)
    ctx.be._appci_mouse = types.SimpleNamespace(scale=None, scale_how="why")
    real_tool = MP.tool
    MP.tool = lambda be_, name: "/x/" + name
    try:
        try:
            U.mouse_or_skip(ctx, Rec(), mac=True)
            raise AssertionError("an unmeasured scale was accepted")
        except RuntimeError as e:
            assert "display scale could not be measured" in str(e)
        ctx.be._appci_mouse = types.SimpleNamespace(scale=2, scale_how="ok")
        assert U.mouse_or_skip(ctx, Rec(), mac=True) is ctx.be._appci_mouse
        ctx.be.window_rect = lambda pid=None: (100.0, 50.0, 1200.0, 800.0)
        assert U.mac_window_origin(ctx) == (200, 100) and WU.window_origin(types.SimpleNamespace(be=ctx.be, app=None)) is None      # no pid, no origin
        ctx.be.pid_fn = lambda: 4242
        assert WU.window_origin(types.SimpleNamespace(be=ctx.be, app=None)) == (200, 100)
    finally:
        MP.tool = real_tool


@check
def macptr_vision_words_lsof_and_recorder():
    tsv = "alpha\t100\t40\t60\t20\nbe ta\t1\t2\t3\t4\n\t5\t5\t5\t5\nbad\tx\t1\t1\t1\nzero\t1\t1\t0\t9\nshort\t1\t2\t3\n"
    rows = MP.vision_words(tsv)
    assert rows == [("alpha", 130, 50, 100, 160, 40, 60), ("be ta", 2, 4, 1, 4, 2, 6)], rows
    assert U.token_hits(rows, "alpha")[0][1:3] == (130, 50)                  # the rows are what token_hits and the cases read
    src = open(os.path.join(TOOLS_DIR, "ocrw.swift"), encoding="utf-8").read()
    assert r'print("\(s[i..<j])\t\(Int(x.rounded()))\t\(Int(y.rounded()))\t\(Int(w.rounded()))\t\(Int(hh.rounded()))")' in src and "(1 - b.maxY) * H" in src
    lsof = "p101\nfcwd\nn/Users/r/job/home/w\np102\nfcwd\nn/Users/r/job/home\np103\nfcwd\nn/Users/r/job/homework\np104\nfcwd\nn/\n"
    assert MP.cwd_pids(lsof, "/Users/r/job/home") == {101, 102} and MP.cwd_pids(lsof, "/Users/r/job/home/") == {101, 102} and MP.cwd_pids("", "/x") == set()
    assert "macptr.pids_cwd_under(prefix)" in inspect.getsource(R.descendants_cwd_under)
    log = "open\thttps://192.0.2.7/ua1\nnoise\nopen\t\nopen\thttps://192.0.2.9/ht2\n"
    calls = MP.recorder_calls(log)
    assert calls == [("openrec", ["https://192.0.2.7/ua1"]), ("openrec", ["https://192.0.2.9/ht2"])]
    assert P.url_open_verdict(calls[:1], "https://192.0.2.7/ua1") is None and "expected exactly" in P.url_open_verdict(calls[:1], "https://192.0.2.9/ht2")
    assert "2 times" in P.url_open_verdict(calls, "https://192.0.2.7/ua1") and "nothing" in P.url_open_verdict([], "x")
    assert MP.recorder_is_default("/Users/r/Applications/PhosphorAppciOpenRecorder.app\n") and MP.recorder_is_default("/a/PhosphorAppciOpenRecorder.app/")
    assert not MP.recorder_is_default("/Applications/Safari.app") and not MP.recorder_is_default("") and not MP.recorder_is_default(None)
    rec = open(os.path.join(TOOLS_DIR, "openrec.swift"), encoding="utf-8").read()
    assert '"open\\t"' in rec and "urlForApplication(toOpen: u)" in rec
    # link_and_path_click on macOS: without the recorder as the default handler it SKIPs with NEEDS (never runs a click that would open a browser)
    body = inspect.getsource(P.mac_recorder_or_skip)
    assert "MP.RECORDER_NEEDS" in body and "recorder_is_default" in body
    assert "link_click_mac(ctx, r, m)" in inspect.getsource(P.c_link_and_path_click) and P.link_mod(types.SimpleNamespace(be=types.SimpleNamespace(name="mac"))) == "Cmd"
    assert P.link_mod(types.SimpleNamespace(be=types.SimpleNamespace(name="x11"))) == "Ctrl"


@check
def macptr_case_gates_on_macos():
    """which cases ask for the Mac pointer and the chords, and which still skip there with a stated reason"""
    for fn in (U.c_mouse_copy, WV.c_copy_wrapped_drag, WV.c_vertical_tabs_panel_click, WU.c_context_menu_actions, WU.c_tab_pointer_use):
        assert "mouse_or_skip(ctx, r, mac=True)" in inspect.getsource(fn), fn.__name__
    assert P.MAC_POINTER_CASES == ("link_and_path_click", "selection_modes")
    mac_ctx = types.SimpleNamespace(be=_mac_be(), win=False)
    for name, want in (("primary_and_middle_click", "no PRIMARY selection"), ("wheel_scroll", "not played on macOS yet"), ("some_new_case", "not enabled")):
        try:
            P.linux_pointer_or_skip(mac_ctx, Rec(), name)
            raise AssertionError(name)
        except Skipped as e:
            assert want in str(e), str(e)
    for name in P.MAC_POINTER_CASES:                       # these reach the Mac pointer (here: its missing-tool skip)
        try:
            P.linux_pointer_or_skip(mac_ctx, Rec(), name)
            raise AssertionError(name)
        except Skipped as e:
            assert "NEEDS: the mptr tool" in str(e), str(e)
    assert "the divider drag was not run" in inspect.getsource(P.c_pane_resize_maximize)
    # Held on macOS sets the modifier on the run's mouse and clears it, and starts no Linux tool
    mouse = types.SimpleNamespace(mods=0)
    mac_ctx.be._appci_mouse = mouse
    real_tool = MP.tool
    MP.tool = lambda be_, name: "/x/" + name
    try:
        with P.Held(mac_ctx, "ctrl"):
            assert mouse.mods == 8
        assert mouse.mods == 0
        with P.Held(mac_ctx, "shift"):
            assert mouse.mods == 1
        assert mouse.mods == 0
    finally:
        MP.tool = real_tool
    # the word reader: Apple Vision counts on macOS for a case that passes its ctx; a case that does not keeps the tesseract rule
    import shutil
    if not shutil.which("tesseract"):
        try:
            U.need_tesseract(Rec(), mac_ctx)
            raise AssertionError("no reader, no skip")
        except Skipped as e:
            assert "NEEDS: the ocrw tool" in str(e)
        assert U.have_word_ocr(mac_ctx) is False

    # chords: quit and close
    lin = types.SimpleNamespace(be=types.SimpleNamespace(name="wayland"), win=False)
    win = types.SimpleNamespace(be=types.SimpleNamespace(name="windows"), win=True)
    assert R.quit_key(lin) == R.QUIT_KEY and R.quit_key(win) == "" and R.close_key(lin) == R.CLOSE_PANE
    be = _mac_be()
    be.BUNDLE, be.pid_fn = "/job/out/Phosphor.app", (lambda: 4242)
    mc = types.SimpleNamespace(be=be, win=False)
    assert "NEEDS: the appkey tool" in R.mac_chords_why_not(mc) and R.quit_key(mc) == "" and (R.close_key(mc) == R.CLOSE_PANE)
    real_access = os.access
    os.access = lambda p, mode: True if str(p).endswith("appkey") else real_access(p, mode)
    try:
        assert R.mac_chords_why_not(mc) is None and R.quit_key(mc) == "cmd+q" and R.close_key(mc) == (R.CLOSE_PANE or "cmd+w")
        R.need_quit_app(mc, "x")                                         # no skip
        be.app_chords_proven = False                                     # the proof failed earlier in the run: the chord is not sent again
        assert "FAILED their proof" in R.mac_chords_why_not(mc) and R.quit_key(mc) == ""
        try:
            R.need_quit_app(mc, "quit_24_tabs")
            raise AssertionError("no skip after a failed proof")
        except cases.Skip as e:
            assert "FAILED their proof" in str(e)
        be.app_chords_proven = True
        be.BUNDLE = ""
        assert "APPCI_MAC_BUNDLE" in R.mac_chords_why_not(mc)
    finally:
        os.access = real_access
    assert "need_quit_app(ctx, SKIP_QUIT_WHY" in inspect.getsource(R.c_quit_24)
    src = inspect.getsource(AR.restart_body)
    assert "R.need_quit_app(ctx" in src and "mac_quit=True" in src and src.count("mac_quit=True") == 1       # only the stop that IS the subject quits by key
    assert "mac_quit=False" in inspect.getsource(R.stop_for_restart)
    quit_src = inspect.getsource(R.quit_app)
    assert '(["Return"], ["Return"]) if ctx.be.name == "mac"' in quit_src and "qk = quit_key(ctx) or QUIT_KEY" in quit_src
    tl = inspect.getsource(W1.c_tab_lifecycle_keys)
    assert "R.close_key(ctx)" in tl and "U.mac(ctx):\n        r.skip" not in tl and "ctx.be.key(close)" in tl and "ctx.be.key(undo)" in tl
    binds = open(os.path.join(ROOT, "app", "src", "util", "bindings.rs"), encoding="utf-8").read()
    assert 'Keystroke::parse("cmd-shift-T").ok()' in binds and W1.MAC_REOPEN_KEY == "cmd+shift+t"
    assert 'CustomAction::CloseCurrentSession => Keystroke::parse(cmd_or_ctrl_shift("w")).ok()' in binds and 'format!("cmd-{key}")' in binds
    assert 'Keystroke::parse("cmd-q").ok()' in binds
    macx = types.SimpleNamespace(be=types.SimpleNamespace(name="mac"))
    if not os.environ.get("APPCI_KEY_REOPEN"):
        assert W1.reopen_key(macx) == "cmd+shift+t" and W1.reopen_key(lin) == W1.REOPEN_KEY
    # the native alert's answers: the first button added is the confirm one (Return), Cancel is last (Escape)
    qw = open(os.path.join(ROOT, "app", "src", "quit_warning", "mod.rs"), encoding="utf-8").read()
    assert qw.index('QuitScope::App => "Yes, quit",') < qw.index('crate::t!("common-cancel")') and "ctx.show_native_platform_modal(dialog);" in qw
    # the Mac launcher arms the find hook, as the Linux and Windows launchers do
    import app as appmod
    # merged with #2082 (app.find_hook_env): this asserted a literal ("PHOSPHOR_APPCI_FIND_FILE", ...) pair in MacApp.start. The integrated launcher sets
    # the same variable through find_hook_env, which also honours the APPCI_NO_FIND_HOOK=1 kill switch; a second, unconditional copy would defeat that
    # switch, so there is one statement and the assertion names it, with the value it gives and that the literal copy is not back
    start_src = inspect.getsource(appmod.MacApp.start)
    assert "find_hook_env(self.home).items()" in start_src and '("PHOSPHOR_APPCI_FIND_FILE"' not in start_src
    keep_sw = os.environ.pop("APPCI_NO_FIND_HOOK", None)
    try:
        assert appmod.find_hook_env("/h") == {"PHOSPHOR_APPCI_FIND_FILE": os.path.join("/h", "appci_find.txt")}
    finally:
        if keep_sw is not None:
            os.environ["APPCI_NO_FIND_HOOK"] = keep_sw
    assert '"mac"' in inspect.getsource(U._hook_word) and "mac_window_origin(ctx)" in inspect.getsource(U._hook_word)


@check
def macptr_proof_case_parts():
    hook, ocr = ("t", 400, 300, 380, 420, 290, 310), ("t", 410, 296, 380, 440, 286, 306)
    assert CM.hook_ocr_verdict(hook, ocr) is None and CM.hook_ocr_verdict(hook, None) is None and CM.hook_ocr_verdict(None, ocr) is None
    far = ("t", 400, 356, 380, 420, 346, 366)                       # a title bar's height off (28 points at scale 2)
    assert "APPCI_NO_FIND_HOOK=1" in CM.hook_ocr_verdict(hook, far) and "0 / -56 px" in CM.hook_ocr_verdict(hook, far)
    assert CM.menu_bar_point((0, 0, 1512, 982, 3024, 1964)) == (756.0, 6.0)
    listing = "4242\tphosphor-oss\t0\t100,50,1200,800\tw1\n4242\tphosphor-oss\t0\t0,0,50,20\ttiny\n4242\tphosphor-oss\t25\t0,0,1512,30\tmenu\n77\tDock\t0\t0,0,1512,982\t\n4242\tphosphor-oss\t0\t300,90,900,700\n"
    wins = backend.mac_windows_of(listing, 4242)
    assert [w["rect"] for w in wins] == [(100, 50, 1200, 800), (300, 90, 900, 700)] and wins[0]["name"] == "w1" and backend.mac_windows_of(listing, 1) == []
    body = inspect.getsource(CM._chord_half)
    assert "be.app_chords_proven = ok" in body and 'be.key("cmd+w")' in body and "t.where()" in body
    assert 'in ("mptr", "appkey", "ocrw")' in inspect.getsource(CM.c_mac_pointer_chord_proof)
    # ptr(): refused for the reference terminal and without a PID, before the guard; a missing tool is named
    be = _mac_be()
    ran = []
    be.guard = lambda: ran.append("guard")
    be.guard_override = "Terminal"
    for setup, want in ((lambda: None, "reference terminal"), (lambda: setattr(be, "guard_override", None), "PID unknown"),
                        (lambda: setattr(be, "pid_fn", lambda: 4242), "mptr tool is missing")):
        setup()
        try:
            be.ptr(["m:1:1"])
            raise AssertionError(want)
        except RuntimeError as e:
            assert want in str(e) and ran == [], (want, str(e))


@check
def macptr_menu_bar_refusal_is_judged_on_the_tool_not_the_message_r21():
    """r15a macOS (job 17936839263): mptr refused the menu-bar click (injector-stderr.log has the sentence below) and the proof failed, because the
    error message kept only the last 240 characters of it and the word looked for was cut off."""
    said = ("refused a click at (756.0, 6.0): the topmost window there belongs to Window Server (pid 547, layer 24), not to pid 84331; windows at the "
            "point, front to back: Window Server (pid 547, layer 24, 0,0,1512,33) > Dock (pid 90039, layer 20, 0,0,1512,982); area clear of the Dock bar "
            "and the menu bar: 0,33,1512,874\n")
    assert "refused" not in backend._tail(said), "the old message: the tool's verdict was in the part cut off"
    e = backend.PtrError(4, said)
    assert isinstance(e, RuntimeError) and e.rc == 4 and e.stderr == said and e.refused() and e.posted == []
    assert str(e).startswith("pointer injector failed: mptr exit 4 (refused a click at (756.0, 6.0)") and "area clear of the Dock bar" in str(e)
    assert CM.menu_bar_refusal(e) is None
    # every other outcome is a failed proof, each with its own sentence
    assert "was NOT refused" in CM.menu_bar_refusal(None)
    assert "another reason" in CM.menu_bar_refusal(RuntimeError("refusing pointer input: the injection target is the reference terminal (Terminal)"))
    assert "without refusing" in CM.menu_bar_refusal(backend.PtrError(2, "bad op k:1:1 (unknown, malformed, or a point outside the main display)"))
    assert "without refusing" in CM.menu_bar_refusal(backend.PtrError(4, "")) and "without refusing" in CM.menu_bar_refusal(backend.PtrError(1, said))
    assert "posted ['click1']" in CM.menu_bar_refusal(backend.PtrError(4, said, ["click1"]))
    assert backend.ptr_posted(["T0 mach_ms=1 epoch=2", "P move 756.0 6.0 3", "LOC 756 6", "ERR refused"]) == []
    assert backend.ptr_posted(["P move 1 1 3", "P click1 1 1 4", "P down 1 1 5", "P up 2 2 6", "P wheel 1 1 7"]) == ["click1", "down", "up", "wheel"]
    assert backend.head_tail("short | text") == "short | text" and backend.head_tail("a\nb") == "a | b" and backend.head_tail(None) == ""
    long = "H" * 300 + "M" * 300 + "T" * 300
    assert backend.head_tail(long) == "H" * 200 + " ... " + "T" * 240
    assert "raise PtrError(r.returncode" in inspect.getsource(backend.MacBackend.ptr) and "menu_bar_refusal(got)" in inspect.getsource(CM._pointer_half)
    # the tool itself: the refusal starts with the word the proof reads, exits 4, and posts nothing for the refused op
    swift = open(os.path.join(TOOLS_DIR, "mptr.swift"), encoding="utf-8").read()
    assert 'finish(4, "refused \\(what) at' in swift and swift.index('mustOwn(p, "a click")') < swift.index("mouse(down, p, btn, Int64(i))")


@check
def r21_mac_documented():
    """docs/testing.md and the design note name what round 21 changed for macOS"""
    doc = open(os.path.join(ROOT, "docs", "testing.md"), encoding="utf-8").read()
    sec = doc[doc.index("## Round 21, macOS"):]
    for word in ("#2137", "#2138", "`backend.PtrError`", "`cases_macptr.menu_bar_refusal`", "`cases_pointer.current_row`", "`aim()`", "all_hits=True",
                 "`cases_window_use.ax_fs_script`", "`Window.covers_display()`", "`wait_fullscreen_left()`", "`path_click_at`", "`windows_at_request`",
                 "selftest_macptr.py", "selftest_pointer.py", "docs/design/r10-r21-mac.md", "meta_and_word_keys"):
        assert word in sec, "docs/testing.md (Round 21, macOS) does not mention %s" % word
    note = open(os.path.join(ROOT, "docs", "design", "r10-r21-mac.md"), encoding="utf-8").read()
    for word in ("17936839263", "TEST", "#2138", "window 1", "set_prompt_suggestions_banner_state", "injector-stderr.log"):
        assert word in note, word
