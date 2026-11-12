# appci/selftest_pointer.py: offline self-tests of cases_pointer.py (round 14 wave 3a, T21 to T25). Pure Python plus `sh`: no app, display or
# network. selftest.py appends CHECKS to its own list, so `run.py --self-test` and `selftest.py` both run these.
import os, random, subprocess, sys, tempfile, types

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import cases, winmap  # noqa: E402
import cases_pointer as P  # noqa: E402

CHECKS = []
NAMES = ("link_and_path_click", "selection_modes", "primary_and_middle_click", "wheel_scroll", "pane_resize_maximize")


def check(f):
    CHECKS.append(f)
    return f


def _rows(words):
    """cases_ui.words() rows for `words`, one per screen row"""
    return [(w, 100, 20 * i, 80, 120, 20 * i - 8, 20 * i + 8) for i, w in enumerate(words)]


@check
def pointer_cases_registry_r14_w3a():
    """the five cases are registered once, PROVING only (never SMOKE, never the fake pass), both modes, gui, SKIP on every Windows pane with a
    reason that names the missing capability, and docs/testing.md describes each with its cost"""
    reg = {c["name"]: c for c in cases.CASES}
    for n in NAMES:
        assert n in reg and n in cases.PROVING and n not in cases.SMOKE and n not in cases.PROVING_FAKE, n
        assert reg[n]["gui"] and reg[n]["modes"] == ("terminal", "agent"), n
        if n == "selection_modes":       # r15 wave 4 (#2060): the Windows pointer exists (SendInput), so it runs in the Git Bash pane
            assert winmap.skip_reason(n, "gitbash") is None and "Git Bash" in winmap.skip_reason(n, "powershell"), n
        else:
            assert n in winmap.NOT_WINDOWS and all(winmap.skip_reason(n, pane) for pane in ("powershell", "gitbash")), n
        assert cases.wants_depth_check(n, cases.PROVING), n
    doc = open(os.path.join(HERE, "..", "..", "..", "docs", "testing.md"), encoding="utf-8").read()
    assert all(("`%s`" % n) in doc for n in NAMES), "docs/testing.md does not describe every wave 3a case"
    assert "Round 14 wave 3a" in doc and "selftest_pointer.py" in doc
    # the keys: the macOS backend takes the maximise chord (it refuses every Ctrl+Cmd chord, which is why the resize is the palette entry)
    import backend
    mac_, lin = types.SimpleNamespace(be=types.SimpleNamespace(name="mac")), types.SimpleNamespace(be=types.SimpleNamespace(name="wayland"))
    assert P.KP(lin, "maximize") == "ctrl+alt+m" and P.KP(mac_, "maximize") == "cmd+shift+Return"
    assert backend.MacBackend()._kspec(P.KP(mac_, "maximize"))
    try:
        backend.MacBackend()._kspec("cmd+ctrl+Left")
        raise AssertionError("the macOS backend took cmd+ctrl+Left: the palette route of pane_resize_maximize is no longer needed, use the key")
    except ValueError:
        pass
    ftl = open(os.path.join(HERE, "..", "..", "..", "app", "i18n", "en", "warp.ftl"), encoding="utf-8").read()
    assert "keybinding-desc-pane-group-resize-left = Resize pane > Move divider left\n" in ftl and P.PALETTE_DIVIDER_LEFT == "move divider left"
    src = open(os.path.join(HERE, "..", "..", "..", "app", "src", "util", "bindings.rs"), encoding="utf-8").read()
    assert 'Keystroke::parse("ctrl-alt-m")' in src and 'Keystroke::parse("cmd-shift-enter")' in src


@check
def pointer_fixture_and_screen_read_parts():
    rng = random.Random(7)
    ws = P.distinct_words(rng, 240)
    assert len(set(ws)) == 240 and all(len(w) == 7 and set(w) <= set(P.U.OCR_SAFE) for w in ws)
    assert all(sum(a != b for a, b in zip(ws[i], ws[j])) >= 2 for i in range(0, 240, 17) for j in range(240) if i != j)
    assert P.word_spans("ab cde f") == {"ab": (0, 2), "cde": (3, 6), "f": (7, 8)}
    # the screen read: exact words, one misread glyph, one dropped glyph, OCR noise around a word; a word two glyphs off names nothing
    wrong = ws[5][:3] + ("2" if ws[5][3] != "2" else "3") + ws[5][4:]
    two_off = ("x" if ws[9][0] != "x" else "y") + ("x" if ws[9][1] != "x" else "y") + ws[9][2:]
    got = P.visible_indexes(_rows([ws[3], wrong, ws[7][1:], "'%s:" % ws[8], two_off, "prompt$", ""]), ws)
    assert got == {3, 5, 7, 8}, got
    assert P.visible_indexes([], ws) == set() and P.visible_indexes(_rows(["nothing"]), ws) == set()
    # the scroll judge: moved and came back passes; not moved, not back and an unread screen do not
    bottom, up, top, down = set(range(200, 240)), set(range(176, 216)), set(range(0, 40)), set(range(24, 64))
    assert P.scroll_verdict(bottom, up, bottom, True)[0] == "ok" and P.scroll_verdict(top, down, top, False)[0] == "ok"
    assert P.scroll_verdict(bottom, bottom, bottom, True)[0] == "fail" and "earlier" in P.scroll_verdict(bottom, bottom, bottom, True)[1]
    assert P.scroll_verdict(bottom, set(range(199, 239)), bottom, True)[0] == "fail"          # one row is not a wheel movement
    assert P.scroll_verdict(bottom, up, up, True)[0] == "fail" and "return" in P.scroll_verdict(bottom, up, up, True)[1]
    assert P.scroll_verdict(top, top, top, False)[0] == "fail" and P.scroll_verdict(top, down, down, False)[0] == "fail"
    assert P.scroll_verdict(top, up, top, False)[0] == "ok" and P.scroll_verdict(bottom, down, bottom, True)[0] == "ok"
    for trio in ((set(), up, bottom), (bottom, set(), bottom), (bottom, up, set())):
        assert P.scroll_verdict(*trio, True)[0] == "unread"
    assert P.aim_at(_rows(ws[:5]), ws, (1, 2)) == (100, 40) and P.aim_at(_rows(["zzz"]), ws, (1, 2)) == (1, 2)


@check
def pointer_sgr_and_reporting_parts():
    press, rel = b"\x1b[<0;12;3M", b"\x1b[<0;12;3m"
    assert P.sgr_events(press + b"k" + rel) == ([(0, 12, 3, "M"), (0, 12, 3, "m")], b"k")
    assert P.sgr_click_verdict(press + rel) is None
    assert "no SGR mouse report" in P.sgr_click_verdict(b"k") and "no SGR mouse report" in P.sgr_click_verdict(b"")      # F12's symptom: only the key arrived
    assert "not a mouse report" in P.sgr_click_verdict(press + rel + b"k")
    assert "expected one press" in P.sgr_click_verdict(press) and "expected one press" in P.sgr_click_verdict(rel + press)
    assert "expected one press" in P.sgr_click_verdict(b"\x1b[<2;12;3M\x1b[<2;12;3m") and "different" in P.sgr_click_verdict(press + b"\x1b[<0;13;3m")
    up, down = b"\x1b[<64;5;9M", b"\x1b[<65;5;9M"
    assert P.sgr_wheel_verdict(up, True) is None and P.sgr_wheel_verdict(up * 3, True) is None and P.sgr_wheel_verdict(down * 2, False) is None
    assert "expected only" in P.sgr_wheel_verdict(down, True) and "expected only" in P.sgr_wheel_verdict(up + down, True) and "expected only" in P.sgr_wheel_verdict(up, False)
    assert "no SGR wheel" in P.sgr_wheel_verdict(b"", True) and "no SGR wheel" in P.sgr_wheel_verdict(b"\x1b[A\x1b[A", True)     # arrow keys are not reports
    assert "not a mouse report" in P.sgr_wheel_verdict(up + b"\x1b[A", True)
    assert P.reporting_expected("") == (True, True) and P.reporting_expected("[terminal]\nmouse_reporting_enabled = true\n") == (True, True)
    assert P.reporting_expected("[terminal]\nscroll_reporting_enabled = false\n") == (True, False)
    assert P.reporting_expected("[terminal]\nmouse_reporting_enabled = false\n") == (False, False)
    # the scripts are valid sh and do what the case needs
    with tempfile.TemporaryDirectory() as d:
        for name, text in (("sgr.sh", P.sgr_script("aim wmabc here", "out.txt")), ("stream.sh", P.stream_script("aa bb cc dd", "tick.txt", 3))):
            path = os.path.join(d, name)
            open(path, "w").write(text)
            assert subprocess.run(["sh", "-n", path], capture_output=True).returncode == 0, name
        sgr = P.sgr_script("aim wmabc here", "out.txt")
        assert sgr.index("?1000h") < sgr.index("bytes_reader.py") < sgr.index("?1000l") and "?1006h" in sgr and "echo 'aim wmabc here'" in sgr
        assert os.path.isfile(os.path.join(HERE, "bytes_reader.py"))
        run = subprocess.run(["sh", os.path.join(d, "stream.sh")], capture_output=True, cwd=d, timeout=20)
        assert run.returncode == 0 and run.stdout.startswith(b"aa bb cc dd\n") and run.stdout.count(b"\n") == 2 and run.stdout.count(b"\r") == 3, run.stdout
        assert open(os.path.join(d, "tick.txt")).read().strip() == "2"
    compile(P.WINCH_PROG, "winch", "exec")


@check
def pointer_selection_and_clipboard_parts():
    la, lb = "aaaaaaa ccccccc ddddddd eeeeeee", "fffffff hhhhhhh kkkkkkk mmmmmmm"
    a0, b1 = P.word_spans(la)["ddddddd"][0], P.word_spans(lb)["hhhhhhh"][1]
    mid = "$ cat sm_b.txt"
    good = "\n".join([la[a0:], mid, lb[:b1]])
    assert P.judge_cross_block(good, la, a0, lb, b1) is None and P.judge_cross_block(good + "\n", la, a0, lb, b1) is None
    assert P.judge_cross_block("\n".join([la[a0 + 2:], "", mid, lb[:b1 - 1]]), la, a0, lb, b1) is None                 # a cell or two either way
    assert P.judge_cross_block("\n".join([la[a0:] + "   ", lb[:b1]]), la, a0, lb, b1) is None                          # nothing between is the caller's to report
    assert "nothing was copied" in P.judge_cross_block(P.SENTINEL, la, a0, lb, b1) and "nothing was copied" in P.judge_cross_block("", la, a0, lb, b1)
    assert "one line" in P.judge_cross_block(la[a0:], la, a0, lb, b1)
    assert "first line" in P.judge_cross_block("\n".join([la, lb[:b1]]), la, a0, lb, b1)                                # the whole first line: too much
    assert "first line" in P.judge_cross_block("\n".join(["dddXddd eeeeeee", lb[:b1]]), la, a0, lb, b1)                 # a changed character
    assert "last line" in P.judge_cross_block("\n".join([la[a0:], lb]), la, a0, lb, b1)
    assert "last line" in P.judge_cross_block("\n".join([la[a0:], "x" + lb[:b1]]), la, a0, lb, b1)
    assert "first line" in P.judge_cross_block("\n".join([lb[:b1], la[a0:]]), la, a0, lb, b1)                           # the two parts swapped
    # the profile gate of primary_and_middle_click
    assert P.cos_profile_block("") is None and P.cos_profile_block("[terminal]\ncopy_on_select = true\n") is None
    assert P.cos_profile_block("[terminal.input]\nhonor_ps1 = true\n\n[general]\nx = 1\n") is None
    assert "linux_selection_clipboard" in P.cos_profile_block("[system]\nlinux_selection_clipboard = false\n")
    assert "middle_click_paste_enabled" in P.cos_profile_block("[terminal.input]\nmiddle_click_paste_enabled = false\n")
    assert P.cos_profile_block("[system]\nlinux_selection_clipboard = true\n") is None
    for odd in ("terminal.copy_on_select = false\n", "terminal.input.honor_ps1 = true\n", "terminal = { copy_on_select = true }\n", "[terminal.x]\ncopy_on_select = 1\n"):
        assert "cannot be edited" in P.cos_profile_block(odd), odd
    # the edit the case makes is one toml_text can make and undo by restoring the bytes
    t = P.T.set_key("[terminal.input]\nhonor_ps1 = true\n", "terminal", "copy_on_select", "false")
    assert P.T.get_key(t, "terminal", "copy_on_select") == "copy_on_select = false" and "[terminal.input]\nhonor_ps1 = true" in t
    # the tools
    assert P.primary_argv("x11", True) == ["xsel", "-p", "-i"] and P.primary_argv("x11", False) == ["xsel", "-p", "-o"]
    assert P.primary_argv("wayland", True) == ["wl-copy", "--primary"] and P.primary_argv("wayland", False) == ["wl-paste", "--primary", "-n"]
    assert P.hold_argv("x11", "ctrl", 900) == (["xdotool", "keydown", "ctrl"], ["xdotool", "keyup", "ctrl"])
    down, up = P.hold_argv("wayland", "shift", 900)
    assert up is None and down == ["wtype", "-k", "Shift_L", "-s", "20", "-M", "shift", "-s", "900", "-m", "shift"]
    import backend
    assert backend.parse_injector(down) == dict(chars=0, keys=[]) and backend.parse_injector(P.hold_argv("x11", "ctrl", 1)[0]) == dict(chars=0, keys=[])   # a held modifier is no key in the accounting


@check
def pointer_link_opener_parts():
    with tempfile.TemporaryDirectory() as d:
        log = os.path.join(d, "open.log")
        for n in ("xdg-open", "gio"):
            open(os.path.join(d, n), "w").write(P.opener_script(log))
            os.chmod(os.path.join(d, n), 0o755)
        url = "https://192.0.2.7/uaabc?x=1&y=%20z"
        assert subprocess.run([os.path.join(d, "xdg-open"), url], capture_output=True).returncode == 0
        assert subprocess.run([os.path.join(d, "gio"), "open", "/tmp/a b/c.rs"], capture_output=True).returncode == 0
        calls = P.opener_calls(open(log).read())
        assert calls == [("xdg-open", [url]), ("gio", ["/tmp/a b/c.rs"])], calls
    assert "xdg-open" in P.OPENER_NAMES and P.opener_calls("") == [] and P.opener_calls("noise\ncall\txdg-open\n") == [("xdg-open", [])]
    one = [("xdg-open", ["https://192.0.2.7/uaabc"])]
    assert P.url_open_verdict(one, "https://192.0.2.7/uaabc") is None
    assert "nothing" in P.url_open_verdict([], "https://192.0.2.7/uaabc")
    assert "2 times" in P.url_open_verdict(one * 2, "https://192.0.2.7/uaabc")
    assert "expected exactly" in P.url_open_verdict(one, "https://192.0.2.9/htabc")           # the OSC 8 text opened instead of its target, or another link
    assert "expected exactly" in P.url_open_verdict([("xdg-open", ["https://192.0.2.7/uaabc/"])], "https://192.0.2.7/uaabc")
    path = "/home/x/w/paabc.rs"
    assert P.path_open_verdict([("xdg-open", [path])], path, False, False) == ("system", None)
    assert P.path_open_verdict([("xdg-open", ["file://" + path])], path, False, False) == ("system", None)
    assert P.path_open_verdict([("xdg-open", ["vscode://file" + path + ":140"])], path, False, False) == ("external", None)
    assert P.path_open_verdict([], path, True, False) == ("pane", None) and P.path_open_verdict([], path, True, True) == ("pane", None)
    how, err = P.path_open_verdict([], path, False, True)
    assert how == "pane" and ":line" in err
    assert P.path_open_verdict([], path, False, False)[0] is None and "nothing opened" in P.path_open_verdict([], path, False, False)[1]
    assert "does not name" in P.path_open_verdict([("xdg-open", ["/home/x/w/other.rs"])], path, True, True)[1]
    assert "2 times" in P.path_open_verdict([("xdg-open", [path])] * 2, path, False, False)[1]
    # the trace judge the case reuses: a connect to the printed link's host is an outbound attempt, loopback is not
    import cases_netless as NL
    line = 'connect(5, {sa_family=AF_INET, sin_port=htons(443), sin_addr=inet_addr("%s")}, 16) = -1'
    assert NL.outbound_attempts(line % "192.0.2.7", ()) == [("connect", "192.0.2.7", 443)] and NL.outbound_attempts(line % "127.0.0.1", ()) == []


@check
def pointer_pane_probe_parts():
    assert P.pane_probe_cmd("a0") == "sh -c 'echo $PPID $(stty size)' >> pr_a0.txt"
    assert P.parse_pane_probe(["4711 40 164"]) == (4711, 40, 164) and P.parse_pane_probe([" 9 1 2 "]) == (9, 1, 2)
    for bad in ([], [""], ["4711"], ["4711 40"], ["x 40 164"], ["4711 40 164 9"]):
        assert P.parse_pane_probe(bad) is None, bad
    with tempfile.TemporaryDirectory() as d:        # the probe line itself: stty has no tty here, so the shell's pid comes alone and is NOT a probe answer
        subprocess.run(["sh", "-c", P.pane_probe_cmd("t")], cwd=d, stdin=subprocess.DEVNULL, capture_output=True)
        got = open(os.path.join(d, "pr_t.txt")).read().split()
        assert len(got) == 1 and got[0].isdigit() and P.parse_pane_probe([" ".join(got)]) is None
    a0, a1, b1 = (10, 40, 164), (10, 40, 81), (20, 40, 81)
    assert P.split_verdict(a0, a1, b1) == []
    assert "OLD pane" in P.split_verdict(a0, a1, (10, 40, 81))[0]
    assert "not the first pane" in P.split_verdict(a0, (20, 40, 81), b1)[0]
    assert "did not shrink" in P.split_verdict(a0, (10, 40, 164), b1)[0] and "did not shrink" in P.split_verdict(a0, a1, (20, 40, 164))[0]
    assert "more than" in P.split_verdict(a0, (10, 40, 60), (20, 40, 60))[0]
    assert P.maximize_verdict(a0, b1, (20, 40, 164), b1) == [] and P.maximize_verdict(a0, b1, (20, 40, 163), b1) == []
    assert "maximised" in P.maximize_verdict(a0, b1, (20, 40, 81), b1)[0]                    # the key did nothing
    assert "restored" in P.maximize_verdict(a0, b1, (20, 40, 164), (20, 40, 164))[0]          # never restored
    assert "not the focused" in P.maximize_verdict(a0, b1, (10, 40, 164), b1)[0]
    assert P.divider_verdict(a1, b1, (10, 40, 79), (20, 40, 83)) == [] and P.divider_verdict(a1, b1, (10, 40, 79), (20, 40, 84)) == []
    assert P.divider_verdict(a1, b1, (10, 40, 67), (20, 40, 95), want_cols=13.3) == []
    assert "expected the right one wider" in P.divider_verdict(a1, b1, a1, b1)[0]             # nothing moved
    assert "expected the right one wider" in P.divider_verdict(a1, b1, (10, 40, 83), (20, 40, 79))[0]      # moved the other way
    assert "total changed" in P.divider_verdict(a1, b1, (10, 40, 80), (20, 40, 90))[0]
    assert "for a divider moved about" in P.divider_verdict(a1, b1, (10, 40, 79), (20, 40, 83), want_cols=13.3)[0]


@check
def pointer_link_click_follows_a_moved_block_r21():
    """r15a macOS (job 17936839263): a prompt-suggestions banner quoting the URL was laid out under the block 15 s after the `cat`, the block moved
    up 78 px and the path click went to the old place, which was the banner. The numbers below are that run's hook lines plus the window origin."""
    import cases_ui as U
    ox, oy = 232, 102
    def row(tok, x0, y0, x1, y1):
        l, t, rgt, b = int(ox + x0), int(oy + y0), int(ox + x1), int(oy + y1)
        return (tok, (l + rgt) // 2, (t + b) // 2, l, rgt, t, b)
    path0, path1 = row("pan2yw2r4", 612.0, 1276.0, 756.0, 1308.0), row("pan2yw2r4", 612.0, 1198.0, 756.0, 1230.0)
    assert path0[1:3] == (916, 1394), "the place the case clicked in that run"
    assert P.current_row(path0, [path0]) == path0 and P.current_row(path0, []) == path0
    assert P.current_row(path0, [path0, path1]) == path1 and P.current_row(path0, [path0, path1])[2] == 1316, "the block moved up 78 px: the click follows it"
    # another copy of the word (the banner's text: another left edge, width and height) is never the target, wherever it is in the log
    url0, url1, banner = row("uanuardcf", 884.0, 1148.0, 1028.0, 1180.0), row("uanuardcf", 884.0, 1070.0, 1028.0, 1102.0), row("uanuardcf", 852.7, 1296.2, 969.8, 1326.7)
    assert P.current_row(url0, [url0, url1, banner]) == url1 and P.current_row(url0, [url0, banner, url1, banner]) == url1
    assert P.current_row(url0, [banner]) == url0, "only a foreign copy was logged since: the place found first stands"
    # hook_find: every sighting, oldest first; the single answer is still the last line
    log = ("a appci_find token=pan2yw2r4 x0=612.0 y0=1276.0 x1=756.0 y1=1308.0\nb appci_find token=other x0=1.0 y0=1.0 x1=2.0 y1=2.0\n"
           "c appci_find token=pan2yw2r4 x0=612.0 y0=1198.0 x1=756.0 y1=1230.0\n")
    fctx = types.SimpleNamespace(log_since=lambda off: log)
    assert U.hook_find(fctx, "pan2yw2r4", origin=(ox, oy), since=0, all_hits=True) == [path0, path1]
    assert U.hook_find(fctx, "pan2yw2r4", origin=(ox, oy), since=0) == path1 and U.hook_find(fctx, "absent", since=0) is None
    assert U.hook_find(fctx, "absent", since=0, all_hits=True) == [] and U._hook_word(types.SimpleNamespace(be=types.SimpleNamespace(name="tui")), "x", all_hits=True) == []
    # aim: hovers where the text is now; a move during the hover gets another hover; text that never rests gets no click
    real_hook, real_hover = U._hook_word, P.hover
    hovers, script = [], []
    U._hook_word = lambda ctx, token, since=None, all_hits=False: list(script[0]) if script else []
    def fake_hover(m, r_):
        hovers.append(tuple(r_[1:3]))
        if len(script) > 1:
            script.pop(0)
    P.hover = fake_hover
    try:
        script[:] = [[path0, path1]]                                    # moved before the hover
        assert P.aim(None, None, "pan2yw2r4", path0, 0) == (path1, None) and hovers == [(916, 1316)]
        hovers[:] = []
        script[:] = [[path0], [path0, path1]]                           # moved DURING the first hover
        assert P.aim(None, None, "pan2yw2r4", path0, 0) == (path1, None) and hovers == [(916, 1394), (916, 1316)]
        hovers[:] = []
        script[:] = [[path0], [path0, path1], [path0, path1, path0], [path0, path1, path0, path1], [path0, path1, path0, path1, path0]]
        got, why = P.aim(None, None, "pan2yw2r4", path0, 0)
        assert got is None and "kept moving" in why and "no click was sent" in why and len(hovers) == 4
        script[:] = [[path0, path1]]
        assert P.moved_since(None, "pan2yw2r4", path0, 0) == (0, -78) and P.moved_since(None, "pan2yw2r4", path1, 0) is None
    finally:
        U._hook_word, P.hover = real_hook, real_hover
    # what a click that opened nothing did instead
    went = P.click_went_elsewhere("2026-11-12T19:35:37Z [DEBUG] input_order hop=view_mode mode=agent pane=1496 t=1\n", "terminal")
    assert went and "agent view" in went
    assert P.click_went_elsewhere("2026-11-12T19:35:37Z [DEBUG] hop=view_mode mode=agent pane=1 t=1\n", "agent") is None and P.click_went_elsewhere("nothing\n", "terminal") is None
    import inspect
    src = inspect.getsource(P.links_play)
    assert src.count("aim(ctx, m, ") == 2 and "m.click(found[" not in src, "every link click goes where the text is now, never where it was first found"
    assert src.count("moved_since(ctx, ") == 2 and "click_went_elsewhere(ctx.log_since(off), ctx.mode)" in src
