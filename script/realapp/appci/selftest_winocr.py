# appci/selftest_winocr.py (#2060): offline self-tests of the Windows OCR reader (winocr.py + windows/lib/ocr.ps1) and of the cases that use it.
# Pure Python: nothing is run on Windows, no PowerShell is started (a fake runner stands in for powershell.exe).
import os, subprocess, sys, types

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


SAMPLE = "L\thello world\nW\t10\t20\t50\t12\thello\nW\t70\t20\t60\t12\tworld\nL\t\nW\tbad\t1\t2\t3\tx\nW\t1\t2\t3\t4\t\nL\tsecond line\nW\t5\t100\t40\t10\tsecond\n"


@check
def winocr_parse_and_screen_math():
    import winocr as W
    lines, words = W.parse(SAMPLE)
    assert lines == ["hello world", "", "second line"], lines
    assert words == [("hello", 10, 20, 50, 12), ("world", 70, 20, 60, 12), ("second", 5, 100, 40, 10)], "malformed and empty words are dropped: %r" % (words,)
    assert W.parse("") == ([], []) and W.parse(None) == ([], [])
    ws = W.to_screen_words(words[:1], 1, (0, 0))
    assert ws == [("hello", 35, 26, 10, 60, 20, 32)], ws
    ws2 = W.to_screen_words([("hello", 20, 40, 100, 24)], 2, (0, 0))          # read from a 2x copy: boxes scaled back
    assert ws2 == [("hello", 35, 26, 10, 60, 20, 32)], ws2
    ws3 = W.to_screen_words(words[:1], 1, (-1920, 0))                          # a monitor left of the primary: the shot starts at x=-1920
    assert ws3[0][1] == 35 - 1920 and ws3[0][3] == 10 - 1920 and ws3[0][2] == 26, ws3
    assert W.shot_to_screen(5, 6, (-1920, -10)) == (-1915, -4)


@check
def winocr_command_scale_and_availability():
    import winocr as W
    c = W.ps_command("C:\\shots\\a.png", "en-US")
    assert c[0] == "powershell.exe" and "-NoProfile" in c and "-File" in c and c[c.index("-File") + 1].endswith(os.path.join("windows", "lib", "ocr.ps1")), c
    assert c[-4:] == ["-Path", "C:\\shots\\a.png", "-Lang", "en-US"], c
    p = W.ps_command("", probe=True)
    assert p[-1] == "-Probe" and "-Path" not in p, p
    assert os.path.exists(W.SCRIPT), "the script the command names exists in the repo"
    assert W.scale_from_env({}) == 2 and W.scale_from_env({W.SCALE_ENV: "1"}) == 1 and W.scale_from_env({W.SCALE_ENV: "9"}) == 4 and W.scale_from_env({W.SCALE_ENV: "x"}) == 2
    assert W.effective_scale(1920, 1080, 2) == 2 and W.effective_scale(6000, 1000, 2) == 1 and W.effective_scale(4000, 4000, 4) == 2

    def runner(code, out=b"", err=b""):
        return lambda argv, t: types.SimpleNamespace(returncode=code, stdout=out, stderr=err)
    assert W.available(runner(0, b"OK\ten-US\n")) is True
    assert W.available(runner(4, b"", b"no OCR engine")) is False
    assert W.run("x.png", runner=runner(4, b"", b"no OCR engine"))[0] == 4
    assert W.run("x.png", runner=lambda a, t: (_ for _ in ()).throw(OSError("no powershell")))[0] == 5, "a runner that cannot start is a failed read, not a crash"


@check
def winocr_read_adds_plain_and_upscaled_reads_and_none_without_engine():
    import winocr as W
    seen = []

    def runner(argv, t):
        seen.append(argv)
        if "-Probe" in argv:
            return types.SimpleNamespace(returncode=0, stdout=b"OK\ten-US\n", stderr=b"")
        return types.SimpleNamespace(returncode=0, stdout=SAMPLE.encode(), stderr=b"")
    env = os.environ.get(W.SCALE_ENV)
    os.environ[W.SCALE_ENV] = "1"
    try:
        text, words = W.read("nofile.png", (100, 0), runner=runner)
    finally:
        os.environ.pop(W.SCALE_ENV, None) if env is None else os.environ.__setitem__(W.SCALE_ENV, env)
    assert "hello world" in text and "second line" in text
    assert words[0] == ("hello", 135, 26, 110, 160, 20, 32), words[0]
    assert sum(1 for a in seen if "-Path" in a) == 1, "scale 1: one read"

    def noeng(argv, t):
        return types.SimpleNamespace(returncode=4, stdout=b"", stderr=b"no OCR engine")
    assert W.read("nofile.png", runner=noeng) is None, "no engine: None (callers report not judged / skip with the reason)"


@check
def winocr_is_wired_into_the_cases_and_the_windows_map():
    import inspect
    import cases as C, cases_ui as U, winmap, cases_r14_w1a as W1, backend
    assert "winocr.read" in inspect.getsource(C.Ctx.ocr)
    src = inspect.getsource(U.words)
    assert "winocr.read" in src and "shot_origin" in src, "words() on Windows returns Windows.Media.Ocr boxes in screen pixels"
    assert "winocr.available" in inspect.getsource(U.have_word_ocr)
    assert hasattr(backend.WindowsBackend, "shot_origin")
    # a Windows ctx is no longer sent to tesseract: need_tesseract passes, words() reaches winocr (the engine probe is faked off here)
    import winocr
    ctx = types.SimpleNamespace(be=types.SimpleNamespace(name="windows"))
    U.need_tesseract(C.Result("x", "terminal"), ctx)
    # scroll_keys_position skipped on Windows only for the missing OCR: it is classified for the Git Bash pane now and its body has no Windows skip
    assert winmap.skip_reason("scroll_keys_position", "gitbash") is None and "Git Bash" in winmap.skip_reason("scroll_keys_position", "powershell")
    body = inspect.getsource(W1.c_scroll_keys_position)
    assert "ctx.win" not in body and "ctx.ocr(" in body
    # the cases that still need more than OCR keep an honest reason
    for n in ("wheel_scroll", "flood_scrollback_find"):
        why = winmap.skip_reason(n, "gitbash")
        assert why and "winocr" in why and "no OCR engine on the VM" not in why, (n, why)


@check
def winocr_script_has_the_contract_the_parser_reads():
    src = open(os.path.join(HERE, "..", "windows", "lib", "ocr.ps1"), encoding="utf-8").read()
    for needle in ("Windows.Media.Ocr.OcrEngine", "TryCreateFromUserProfileLanguages", "RecognizeAsync", "BoundingRect", '"L`t"', '"W`t', "MaxImageDimension", "-Probe", "exit 4"):
        assert needle in src, needle
    assert "Add-WindowsCapability" not in src, "the script names the language pack, it never installs one"
    assert "Windows.Media.Ocr" in open(os.path.join(HERE, "..", "..", "..", "docs", "testing.md"), encoding="utf-8").read(), "docs/testing.md documents the Windows OCR"
