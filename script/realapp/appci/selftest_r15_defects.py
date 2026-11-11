# appci/selftest_r15_defects.py: offline self-tests of round 15's defect work (#2079, #2080, #2081, #2082): the pure parts of
# cases_r15_defects.py with the input a defect would produce, the foreground script on a real pty, the registry rows, and the app texts the
# cases read (a rename there must fail here). No app, display or network. selftest.py appends CHECKS to its own list.
import os, shutil, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, HERE)
import cases, winmap  # noqa: E402
import cases_r15_defects as X  # noqa: E402

CHECKS = []
NAMES = ("scroll_keys_foreground", "primary_absent_clipboard_kept")


def check(f):
    CHECKS.append(f)
    return f


def _src(*rel):
    return open(os.path.join(ROOT, *rel), encoding="utf-8").read()


@check
def r15_defect_cases_registry():
    """both cases are registered once, PROVING only (never SMOKE, never the fake pass), both modes, gui; Windows: the scroll case runs in the Git
    Bash pane and skips with a reason in the PowerShell pane, the primary case is not a Windows case; docs/testing.md describes each"""
    reg = {c["name"]: c for c in cases.CASES}
    for n in NAMES:
        assert n in reg and n in cases.PROVING and n not in cases.SMOKE and n not in cases.PROVING_FAKE, n
        assert reg[n]["gui"] and reg[n]["modes"] == ("terminal", "agent"), n
        assert cases.wants_depth_check(n, cases.PROVING), n
    assert winmap.skip_reason("scroll_keys_foreground", "gitbash") is None and "#2080" in winmap.skip_reason("scroll_keys_foreground", "powershell")
    assert all("#2081" in winmap.skip_reason("primary_absent_clipboard_kept", pane) for pane in ("powershell", "gitbash"))
    assert not winmap.unclassified(NAMES)
    assert reg["scroll_keys_foreground"]["order"] > reg["scroll_keys_position"]["order"], "the foreground scroll case follows the prompt one"
    doc = _src("docs", "testing.md")
    assert all(("`%s`" % n) in doc for n in NAMES), "docs/testing.md does not describe every round 15 defect case"
    assert "selftest_r15_defects.py" in doc and "hop=scrollback_key" in doc and "PHOSPHOR_SCROLLBACK_KEYS" in doc
    assert "PHOSPHOR_ALT_WORD_KEYS" in doc and "PHOSPHOR_PRIMARY_FALLS_BACK_TO_CLIPBOARD" in doc
    assert X.NEEDS_NO_PRIMARY.startswith("NEEDS: ") and "primary_and_middle_click" in X.NEEDS_NO_PRIMARY


@check
def r15_scroll_keys_pure_parts():
    """#2080: the hop parser reads the app's own line; a key the app wrote to the pty, a key that never reached the binding and a key the app
    scrolled on although it was the program's are each a failure; the view verdict tells the bottom from a scrolled-up view"""
    log = ("12:00:01 [DEBUG] input_order hop=scrollback_key key=page_up t=1\n"
           "12:00:01 [DEBUG] input_order hop=find_state event=next count=3 focused=1 scanning=false\n"
           "12:00:02 [DEBUG] input_order hop=scrollback_key key=top t=2\nhop=scrollback_key key=page_down\nhop=scrollback_key key=bottom t=9\n"
           "hop=scrollback_key key=sideways\n")
    assert X.scrollback_hops(log) == ["page_up", "top", "page_down", "bottom"] and X.scrollback_hops("") == [] and X.scrollback_hops(None) == []
    # the Rust side: the detail format, the four names and the four keystrokes (app/src/terminal/view/scrollback_keys.rs; its unit test pins them too)
    rs = _src("app", "src", "terminal", "view", "scrollback_keys.rs")
    assert 'format!("key={}", key.hop_name())' in rs and 'log_hop("scrollback_key"' in _src("app", "src", "terminal", "view.rs")
    assert all(('"%s"' % n) in rs for n in ("page_up", "page_down", "top", "bottom"))
    assert all(('"%s"' % k) in rs for k in ("shift-pageup", "shift-pagedown", "shift-home", "shift-end"))
    assert 'id!("LongRunningCommand")' in rs and '!id!("AltScreen")' in rs and '!id!("EditorFocused")' in rs and '"PHOSPHOR_SCROLLBACK_KEYS"' in rs
    # keys per injector
    assert X.nav_key("wayland", "pageup") == "Prior" and X.nav_key("x11", "pagedown", True) == "shift+Next" and X.nav_key("x11", "home", True) == "shift+Home"
    assert X.nav_key("mac", "pageup", True) == "shift+pageup" and X.nav_key("windows", "pagedown") == "pagedown" and X.nav_key("mac", "end", True) == "shift+End"
    import backend
    for be in ("mac", "windows"):
        table = dict(backend.MacBackend.SPECIAL) if be == "mac" else dict(backend.WindowsBackend.NAMED)
        for name in ("pageup", "pagedown", "home", "end"):
            assert X.nav_key(be, name, True).lower().split("+")[-1] in table, (be, name)
    assert backend.split_key("shift+Prior")[0] == ["shift"], "the Linux injectors take shift as a modifier"
    # a key that must scroll
    assert X.sfg_scroll_verdict("page_up", 3, ["page_up"] * 3, b"") is None
    assert "written to the pty" in X.sfg_scroll_verdict("page_up", 1, [], b"\x1b[5~"), "the defect of #2080: the shifted key went to the program"
    assert "written to the pty" in X.sfg_scroll_verdict("page_up", 1, ["page_up"], b"\x1b[5~"), "scrolled AND written is still a key the program got"
    assert "did not reach the scrollback binding" in X.sfg_scroll_verdict("page_up", 1, [], b""), "a key that did nothing at all"
    assert X.sfg_scroll_verdict("page_up", 3, ["page_up"] * 2, b"") and X.sfg_scroll_verdict("top", 1, ["bottom"], b"")
    # a key that is the program's
    assert X.sfg_program_verdict(b"\x1b[5", [], b"\x1b[5~", exact=b"\x1b[5~") is None and X.sfg_program_verdict(b"\x1b[5", [], b"\x1b[5;2~") is None
    assert "instead of leaving the key" in X.sfg_program_verdict(b"\x1b[5", ["page_up"], b""), "a full-screen program lost its Shift+PageUp to the scrollback"
    assert "received nothing" in X.sfg_program_verdict(b"\x1b[5", [], b"")
    assert X.sfg_program_verdict(b"\x1b[", [], b"\x1b[1;2F", exact=b"\x1b[1;2H") and X.sfg_program_verdict(b"\x1b[5", [], b"x")
    # the view
    assert X.sfg_view_verdict(98, 100, True)[0] == "pass" and X.sfg_view_verdict(40, 100, True)[0] == "fail" and X.sfg_view_verdict(None, 100, True)[0] == "noref"
    assert X.sfg_view_verdict(40, 100, False)[0] == "pass" and X.sfg_view_verdict(None, 100, False)[0] == "pass"
    assert X.sfg_view_verdict(99, 100, False)[0] == "fail", "a view that followed the output is not scrolled up"


@check
def r15_scroll_foreground_script_on_a_pty():
    """#2080: the script the case types really runs a foreground raw reader with a job printing next to it: on a pty, bytes sent while it runs
    are logged exactly, the job's lines arrive with CR LF, and after Ctrl-C the script stops the job and returns; the `alt` arm enters and leaves
    the alternate screen around the reader and starts no job"""
    if os.name != "posix" or not shutil.which("bash"):
        return
    import pty
    d = tempfile.mkdtemp()
    try:
        with open(os.path.join(d, "st.txt"), "w") as f:
            f.write("".join("line%03d\n" % i for i in range(1, 400)))
        with open(os.path.join(d, "run.sh"), "w") as f:
            f.write(X.sfg_script('python3 "%s"' % os.path.join(HERE, "bytes_reader.py"), "st.txt", "seen.txt", "stop", "rd.txt", 20))
        assert subprocess.run(["bash", "-n", os.path.join(d, "run.sh")]).returncode == 0, "the script must parse"
        for arm, send in (("main", [b"\x1b[5~", b"x"]), ("alt", [b"\x1b[1;2H"])):
            for n in ("rd.txt", "rd.txt.ready", "stop", "seen.txt"):
                if os.path.exists(os.path.join(d, n)):
                    os.remove(os.path.join(d, n))
            pid, fd = pty.fork()
            if pid == 0:
                os.chdir(d)
                os.execvp("bash", ["bash", "run.sh", arm])
            out = b""
            try:
                t0 = time.time()
                while not os.path.exists(os.path.join(d, "rd.txt.ready")) and time.time() - t0 < 8:
                    time.sleep(0.05)
                assert os.path.exists(os.path.join(d, "rd.txt.ready")), "[%s] the reader never became ready" % arm
                time.sleep(1.6 if arm == "main" else 0.2)        # the job starts after `sleep 1`
                for chunk in send:
                    os.write(fd, chunk)
                    time.sleep(0.15)
                os.write(fd, b"\x03")
                t0 = time.time()
                while time.time() - t0 < 8:
                    try:
                        c = os.read(fd, 65536)
                    except OSError:
                        break
                    if not c:
                        break
                    out += c
            finally:
                _, status = os.waitpid(pid, 0)
                os.close(fd)
            got = b"".join(bytes.fromhex(l.split()[1]) for l in open(os.path.join(d, "rd.txt")) if len(l.split()) == 2)
            assert got == b"".join(send) + b"\x03", (arm, got)
            assert os.path.exists(os.path.join(d, "stop")), "[%s] the script did not write the stop file after the reader ended" % arm
            if arm == "main":
                seen = open(os.path.join(d, "seen.txt")).read().split()
                assert seen[:3] == ["line001", "line002", "line003"] and len(seen) < 100, seen[:5]
                assert b"line001\r\n" in out and b"\x1b[?1049h" not in out, out[:200]
            else:
                assert not os.path.exists(os.path.join(d, "seen.txt")), "the alt arm starts no job"
                assert out.index(b"\x1b[?1049h") < out.index(b"\x1b[?1049l"), out[:200]
    finally:
        shutil.rmtree(d, ignore_errors=True)


@check
def r15_primary_absent_verdict_and_source():
    """#2081: with copy on select off, a CLIPBOARD that no longer holds its sentinel is the defect; with it on, a CLIPBOARD that still holds the
    sentinel is a drag that selected nothing (the control), and the selection itself passes. The Rust side drops the write and has its switch."""
    line = "alpha bravo charlie delta"
    a0, b1 = line.index("bravo"), line.index("charlie") + len("charlie")
    assert X.no_primary_verdict(False, "SENT", "SENT", line, a0, b1) is None
    bad = X.no_primary_verdict(False, "bravo charlie", "SENT", line, a0, b1)
    assert bad and "#2081" in bad and "copy on select is off" in bad, "the fallback of #2081: the selection replaced the clipboard"
    assert X.no_primary_verdict(True, "bravo charlie", "SENT", line, a0, b1) is None
    assert "selected nothing" in X.no_primary_verdict(True, "SENT", "SENT", line, a0, b1)
    assert X.no_primary_verdict(True, "something else entirely", "SENT", line, a0, b1)
    rs = _src("crates", "warpui", "src", "windowing", "winit", "linux", "clipboard.rs")
    assert '"PHOSPHOR_PRIMARY_FALLS_BACK_TO_CLIPBOARD"' in rs and "PrimaryWrite::Dropped => note_primary_write_dropped()" in rs
    body = rs[rs.index("fn write_to_primary_clipboard"):rs.index("fn take_read_failure")]
    assert "primary_write_outcome(result, primary_falls_back_to_clipboard())" in body, "the trait's primary write goes through the one decision"
    bg = rs[rs.index("fn write_for_target"):rs.index("fn parse_valid_filepaths_from_text")]
    assert "primary_write_outcome(result, primary_falls_back_to_clipboard())" in bg, "so does the background writer's"
    # primary_and_middle_click names this case where it cannot judge
    assert "primary_absent_clipboard_kept" in open(os.path.join(HERE, "cases_pointer.py"), encoding="utf-8").read()


@check
def r15_alt_word_keys_source_2079():
    """#2079: the three editors take their Linux / Windows word-motion defaults from one place, Alt-Backspace is bound off macOS, the Meta forms
    stay as fixed bindings, and the kill switch exists (the case meta_and_word_keys expects the keys to work)"""
    b = _src("app", "src", "util", "bindings.rs")
    assert '"PHOSPHOR_ALT_WORD_KEYS"' in b and '= "alt-f";' in b and '= "alt-b";' in b and '= "alt-backspace";' in b
    ed = _src("app", "src", "editor", "view", "mod.rs")
    assert ".with_linux_or_windows_key_binding(word_forward_key)" in ed and ".with_linux_or_windows_key_binding(word_backward_key)" in ed
    assert "LINUX_OR_WINDOWS_CUT_WORD_LEFT_KEYSTROKE,\n            EditorAction::CutWordLeft," in ed
    assert '"meta-f",\n            EditorAction::MoveForwardOneWord,' in ed and '"meta-b",\n            EditorAction::MoveBackwardOneWord,' in ed
    for rel in (("app", "src", "code", "editor", "view", "actions.rs"), ("app", "src", "notebooks", "editor", "view.rs")):
        t = _src(*rel)
        assert ".with_linux_or_windows_key_binding(word_forward_key)" in t and "META_MOVE_BACKWARD_ONE_WORD_KEYSTROKE" in t, rel
    import cases_daily_b as D
    for family in ("mac", "other"):
        steps = {s["name"]: s for s in D.prompt_steps("k7m", family, False)}
        assert steps["alt_b"]["issue"] is None and steps["alt_f"]["issue"] is None and "alt_backspace" in steps
        assert steps["alt_backspace"]["ops"][1] == ("k", "alt+BackSpace", 1)


@check
def r15_find_hook_armed_on_every_launcher_2082():
    """#2082: all three launchers arm the app's find hook: Linux in INNER (round 14), Windows in env(), macOS through find_hook_env; the kill
    switch APPCI_NO_FIND_HOOK=1 leaves the macOS launch without it, as it puts the Linux positions back on OCR"""
    import app as A
    src = open(os.path.join(HERE, "app.py"), encoding="utf-8").read()
    assert 'export PHOSPHOR_APPCI_FIND_FILE="%(home)s/appci_find.txt"' in A.INNER
    assert 'e["PHOSPHOR_APPCI_FIND_FILE"]' in src, "the Windows launcher"
    mac = src[src.index("class MacApp"):src.index("class WindowsApp")]
    assert "find_hook_env(self.home).items()" in mac and "warpui::appci_find=info" in mac, "the macOS launcher sets the variable and enables the log target"
    old = os.environ.pop("APPCI_NO_FIND_HOOK", None)
    try:
        assert A.find_hook_env("/h") == {"PHOSPHOR_APPCI_FIND_FILE": os.path.join("/h", "appci_find.txt")}
        os.environ["APPCI_NO_FIND_HOOK"] = "1"
        assert A.find_hook_env("/h") == {}
        os.environ["APPCI_NO_FIND_HOOK"] = "0"
        assert A.find_hook_env("/h"), "only 1 is the kill switch"
    finally:
        os.environ.pop("APPCI_NO_FIND_HOOK", None)
        if old is not None:
            os.environ["APPCI_NO_FIND_HOOK"] = old
    # the file the launcher names is the file register_find appends to
    assert 'os.path.join(ctx.prof.home, "appci_find.txt")' in open(os.path.join(HERE, "cases_ui.py"), encoding="utf-8").read()
