# appci/selftest_w4win.py: offline self-tests of round 15 wave 4 (#2060; docs/design/r14-test-gaps.md T31, T33; design docs/design/r15-w4win.md):
# the Windows pointer (winapi.Pointer, backend.WindowsBackend.ptr_*), the window size (SetWindowPos read back), positions from the app's find hook
# only, the settings file the app reads on Windows, and the Windows classification of the cases ported to them. Pure Python: no Windows, app or
# display; every Win32 call is a parameter or a stub.
# selftest.py appends CHECKS to its own list, so `run.py --self-test` and `selftest.py` both run these.
import ctypes, inspect, os, sys, types

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import backend, cases, winapi, winmap  # noqa: E402
import cases_pointer as P  # noqa: E402
import cases_ui as U  # noqa: E402
import cases_window_use as W  # noqa: E402
import cases_wrap_vtabs as V  # noqa: E402

CHECKS = []
ROOT = os.path.join(HERE, "..", "..", "..")
POINTER_CASES = ("ui_mouse_select_copy", "ui_copy_wrapped_drag", "ui_vertical_tabs_panel_click", "selection_modes", "context_menu_actions")
RESIZE_CASES = ("ui_resize_typing", "resize_reflow_output", "ui_vertical_tabs_many")


def check(f):
    CHECKS.append(f)
    return f


def raises(exc, fn, needle=None):
    try:
        fn()
    except exc as e:
        assert needle is None or needle in str(e), (needle, str(e))
        return str(e)
    raise AssertionError("%s not raised" % exc.__name__)


@check
def w4win_mouse_events_pure_parts():
    """the SendInput structures have the Win64 layout, an absolute move names exactly the pixel asked for on any desktop (also one that starts left of
    the primary monitor), a point off the desktop is refused, the buttons and the wheel carry the right flags and sign"""
    if ctypes.sizeof(ctypes.c_void_p) == 8:
        assert ctypes.sizeof(winapi.MOUSEINPUT) == 32 and ctypes.sizeof(winapi.KEYBDINPUT) == 24 and ctypes.sizeof(winapi.INPUT) == 40
        assert winapi.INPUT.u.offset == 8 and winapi.MOUSEINPUT.dwFlags.offset == 12 and winapi.MOUSEINPUT.dwExtraInfo.offset == 24
    for vs in ((0, 0, 1920, 1080), (0, 0, 1024, 768), (-1920, 0, 3840, 1200), (0, -1080, 2560, 2520), (0, 0, 1, 1)):
        vx, vy, vw, vh = vs
        for px in sorted({0, 1, 2, 3, 29, 30, 31, vw // 3, vw // 2, vw - 2, vw - 1} & set(range(vw))):
            for py in sorted({0, 1, 17, vh // 2, vh - 1} & set(range(vh))):
                nx, ny = winapi.mouse_norm(vx + px, vy + py, vs)
                assert 0 <= nx <= 65535 and 0 <= ny <= 65535
                # both ways Windows is described to map the value back: truncation and MulDiv rounding
                assert nx * vw // 65536 == px and ny * vh // 65536 == py, (vs, px, py, nx, ny)
                assert (nx * vw + 32768) // 65536 == px and (ny * vh + 32768) // 65536 == py, (vs, px, py, nx, ny)
    vs = (0, 0, 1920, 1080)
    for off in ((-1, 5), (5, -1), (1920, 5), (5, 1080)):
        assert winapi.mouse_norm(off[0], off[1], vs) is None and winapi.mouse_move_event(off[0], off[1], vs) is None, off
    assert winapi.mouse_norm(-1, 5, (-1920, 0, 3840, 1200)) is not None, "a monitor left of the primary one is on the desktop"
    assert winapi.mouse_move_event(10, 20, vs)[2:] == (0, 0x0001 | 0x8000 | 0x4000), "an absolute move on the VIRTUAL desktop, never a relative one"
    assert winapi.mouse_button_event(1, True) == (0, 0, 0, 0x0002) and winapi.mouse_button_event(1, False) == (0, 0, 0, 0x0004)
    assert winapi.mouse_button_event(3, True) == (0, 0, 0, 0x0008) and winapi.mouse_button_event(3, False) == (0, 0, 0, 0x0010)
    assert winapi.mouse_button_event(2, True) == (0, 0, 0, 0x0020) and winapi.mouse_button_event(2, False) == (0, 0, 0, 0x0040)
    raises(ValueError, lambda: winapi.mouse_button_event(4, True))
    # positive steps = down (later lines), as cases_pointer.wheel; Windows: a NEGATIVE delta scrolls down
    assert winapi.mouse_wheel_event(1) == (0, 0, (-120) & 0xFFFFFFFF, 0x0800) and winapi.mouse_wheel_event(-2) == (0, 0, 240, 0x0800)
    seen = []

    def send(n, arr, size):
        seen.append((n, size, [(arr[i].type, arr[i].u.mi.dx, arr[i].u.mi.dy, arr[i].u.mi.mouseData, arr[i].u.mi.dwFlags) for i in range(n)]))
        return n
    assert winapi.send_mouse([(7, 9, 0, 0xC001), winapi.mouse_wheel_event(1)], send) == 2
    assert seen == [(2, ctypes.sizeof(winapi.INPUT), [(0, 7, 9, 0, 0xC001), (0, 0, 0, 4294967176, 0x0800)])], seen


def _pointer(cursor_script, accept=1, vs=(0, 0, 1920, 1080)):
    """a winapi.Pointer on fakes: cursor_script(sent events so far, polls so far) -> the cursor position"""
    sent, polls, slept = [], [0], []

    def send(evs):
        sent.extend(evs)
        return accept if callable(accept) is False else accept(evs)

    def cursor():
        polls[0] += 1
        return cursor_script(sent, polls[0])
    p = winapi.Pointer(send=send, cursor=cursor, vscreen=lambda: vs, sleep=slept.append, settle=0.05)
    return p, sent, slept


@check
def w4win_pointer_delivery_is_proven():
    """a move is read back from the cursor: an applied move passes (also one applied a few polls late), a move the desktop did not apply raises with
    the NEEDS text, a refused SendInput raises, a point off the screen raises before anything is sent; held buttons are tracked and released"""
    target = (640, 400)
    p, sent, slept = _pointer(lambda s, n: target)
    assert p.move(*target) == target and len(sent) == 1 and not slept
    p, sent, slept = _pointer(lambda s, n: target if n >= 3 else (5, 5))
    assert p.move(*target) == target and len(slept) == 2, "the input thread applies the move a moment later: waited for, not failed"
    p, sent, slept = _pointer(lambda s, n: (target[0] + 1, target[1] - 1))
    assert p.move(*target) == (641, 399), "one pixel off is the same pixel for a click (tol)"
    p, sent, slept = _pointer(lambda s, n: (5, 5))
    msg = raises(winapi.PointerError, lambda: p.move(*target), "NEEDS:")
    assert "(640, 400)" in msg and "(5, 5)" in msg and len(sent) == 1, msg
    p, sent, slept = _pointer(lambda s, n: None)
    assert "unreadable" in raises(winapi.PointerError, lambda: p.move(*target), "NEEDS:")
    p, sent, slept = _pointer(lambda s, n: target, accept=lambda evs: 0)
    raises(winapi.PointerError, lambda: p.move(*target), "refused")
    p, sent, slept = _pointer(lambda s, n: target)
    raises(winapi.PointerError, lambda: p.move(2000, 400), "not on the screen")
    assert sent == [], "nothing is sent for a point off the desktop"
    p.button(1, True)
    p.button(3, True)
    assert p.held == {1, 3}
    p.button(1, False)
    assert p.held == {3} and [e[3] for e in sent] == [0x0002, 0x0008, 0x0004]
    p.release_all()
    assert p.held == set() and sent[-1][3] == 0x0010
    p.wheel(3)
    assert sent[-1] == winapi.mouse_wheel_event(3)
    assert issubclass(winapi.PointerError, RuntimeError)


class _U32:
    """user32 for the window helpers: one window (handle 7) with a client area at (108, 131) of 1264 x 681 inside an outer rectangle"""

    def __init__(self):
        self.outer, self.zoomed, self.calls = [100, 100, 1380, 820], False, []

    def ClientToScreen(self, h, ref):
        ref._obj.x, ref._obj.y = 108, 131
        return 1 if h == 7 else 0

    def GetClientRect(self, h, ref):
        ref._obj.left, ref._obj.top, ref._obj.right, ref._obj.bottom = 0, 0, 1264, 681
        return 1

    def GetWindowRect(self, h, ref):
        ref._obj.left, ref._obj.top, ref._obj.right, ref._obj.bottom = self.outer
        return 1 if h == 7 else 0

    def GetSystemMetrics(self, i):
        return {76: -1920, 77: 0, 78: 3840, 79: 1200}[i]

    def GetCursorPos(self, ref):
        ref._obj.x, ref._obj.y = 321, 123
        return 1

    def WindowFromPoint(self, pt):
        return 9 if (pt.x, pt.y) == (300, 300) else None

    def GetAncestor(self, h, flag):
        assert flag == 2
        return 7

    def GetWindowThreadProcessId(self, h, ref):
        ref._obj.value = 4242 if h == 7 else 1
        return 1

    def IsZoomed(self, h):
        return 1 if self.zoomed else 0

    def ShowWindowAsync(self, h, cmd):
        self.calls.append(("show", h, cmd))
        return 1

    def SetWindowPos(self, h, after, x, y, w, hgt, flags):
        self.calls.append((h, after, x, y, w, hgt, flags))
        return 1


@check
def w4win_window_helpers_and_resize_verdict():
    """the window helpers read what user32 gives (client area in screen pixels, outer rectangle, the window under a point), SetWindowPos is posted with
    no z-order change and no activation, and the resize verdict takes a clamped size as a change and an unchanged window as not taken"""
    u = _U32()
    assert winapi.client_rect_screen(7, u) == (108, 131, 1264, 681) and winapi.client_rect_screen(None, u) is None and winapi.client_rect_screen(8, u) is None
    assert winapi.window_rect(7, u) == (100, 100, 1280, 720) and winapi.window_rect(8, u) is None
    assert winapi.virtual_screen(u) == (-1920, 0, 3840, 1200) and winapi.cursor_pos(u) == (321, 123)
    assert winapi.root_window_at(300, 300, u) == (7, 4242) and winapi.root_window_at(1, 1, u) == (None, 0)
    assert winapi.is_zoomed(7, u) is False
    assert winapi.set_window_rect(7, 900, 600, u32=u) and u.calls[-1] == (7, None, 0, 0, 900, 600, 0x0004 | 0x0010 | 0x4000 | 0x0002), u.calls
    assert winapi.set_window_rect(7, 900, 600, 10, 20, u32=u) and u.calls[-1] == (7, None, 10, 20, 900, 600, 0x0004 | 0x0010 | 0x4000)
    assert winapi.show_window(7, winapi.SW_MAXIMIZE, u) and u.calls[-1] == ("show", 7, 3) and winapi.SW_RESTORE == 9
    rv = winapi.resize_verdict
    assert rv((1280, 720), (900, 600), (900, 600)) is None
    assert rv((1280, 720), (1300, 800), (1296, 776)) is None, "clamped to the desktop: still a change"
    assert "still 1280x720" in rv((1280, 720), (900, 600), (1280, 720))
    assert "could not be read" in rv((1280, 720), (900, 600), None)
    assert rv((900, 600), (900, 600), (900, 600)) is None and "asked to stay" in rv((900, 600), (900, 600), (800, 600))
    # the resize case's desktop precondition
    sizes = W.plan_sizes(1280, 720)
    assert W.desktop_too_small((0, 0, 1920, 1080), sizes) is None and W.desktop_too_small((0, 0, 1300, 800), sizes) is None
    small = W.desktop_too_small((0, 0, 1024, 768), sizes)
    assert small.startswith("NEEDS: a desktop of at least 1180x800") and "1024x768" in small, small       # plan_sizes moves the sizes near 1280 px in
    assert W.WIN_RR_LINES >= 200 and "ctx.win" in inspect.getsource(W._stream_under_resizes)


def _backend(pid=4242):
    """a WindowsBackend with no Windows under it: winapi replaced by a stub namespace, the pointer by a recorder"""
    be = backend.WindowsBackend.__new__(backend.WindowsBackend)
    log = []
    ptr = types.SimpleNamespace(held=set())
    ptr.move = lambda x, y: (log.append(("move", int(x), int(y))), (int(x), int(y)))[1]
    ptr.button = lambda b, d: log.append(("button", b, d))
    ptr.wheel = lambda n: log.append(("wheel", n))
    ptr.release_all = lambda: log.append(("release_all",))
    st = types.SimpleNamespace(fg_pid=pid, under_pid=pid, cursor=(700, 500), rect=[100, 100, 1280, 720], zoomed=False, iconic=False, set_ok=True, clamp=None)

    def set_rect(h, w, hgt, x=None, y=None, u32=None):
        log.append(("set_window_rect", w, hgt, x, y))
        if st.set_ok:
            cw, ch = st.clamp or (w, hgt)
            st.rect = [st.rect[0] if x is None else x, st.rect[1] if y is None else y, min(w, cw), min(hgt, ch)]
        return True

    def show(h, cmd):
        log.append(("ShowWindow", cmd))
        st.zoomed, st.iconic = cmd == 3, False
        if cmd == 9:
            st.rect = [100, 100, 1000, 700]
        return 1
    be.w = types.SimpleNamespace(
        Pointer=lambda: ptr, main_window=lambda p: 7 if p == pid else None, foreground=lambda: (7, st.fg_pid), proc_name=lambda p: "other.exe",
        focus_window=lambda h: log.append(("focus", h)), cursor_pos=lambda: st.cursor, root_window_at=lambda x, y: (7, st.under_pid),
        client_rect_screen=lambda h: (108, 131, 1264, 681) if h == 7 else None, window_rect=lambda h: tuple(st.rect) if h == 7 else None,
        is_zoomed=lambda h: st.zoomed, set_window_rect=set_rect, resize_verdict=winapi.resize_verdict, SW_RESTORE=9, SW_MAXIMIZE=3, show_window=show,
        user32=types.SimpleNamespace(IsIconic=lambda h: st.iconic, MapVirtualKeyW=lambda vk, t: 0x2A))
    be.ct = ctypes
    be.pid_fn = lambda: pid
    be.guard_override, be.inject_log, be._cur, be.fails, be.calls, be.key_method = None, None, None, 0, [], "vk"
    be._ptr = None
    be.RESIZE_WAIT = 0.03
    keys = []
    be._send = lambda evs: (keys.extend(evs), len(evs))[1]
    return be, st, log, keys


@check
def w4win_backend_pointer_guards():
    """the backend's pointer acts only on the Phosphor window: a press is refused over another process's window and while the reference console is the
    target, a release always goes out; the readiness probe names why the pointer cannot be used; a held modifier is one key-down and one key-up"""
    be, st, log, keys = _backend()
    assert be.client_origin() == (108, 131)
    be.ptr_move(300, 400)
    be.ptr_button(1, True)
    be.ptr_button(1, False)
    assert log == [("move", 300, 400), ("button", 1, True), ("button", 1, False)], log
    st.under_pid = 99                                   # another window covers the point
    del log[:]
    raises(RuntimeError, lambda: be.ptr_button(1, True), "refusing to click")
    assert log == [], "nothing is pressed over a window that is not Phosphor's"
    be.ptr_button(1, False)
    assert log == [("button", 1, False)], "a release is never refused (a button must not stay down)"
    st.under_pid = 4242
    be.guard_override = dict(pids=(1,), hwnds=())
    for fn in (lambda: be.ptr_move(1, 1), lambda: be.ptr_button(1, True), lambda: be.ptr_wheel(1)):
        raises(RuntimeError, fn, "reference console")
    be.guard_override = None
    st.fg_pid = 99                                      # Phosphor is not the foreground window and focusing it does not help
    real_sleep, backend.time.sleep = backend.time.sleep, lambda s: None
    try:
        raises(RuntimeError, lambda: be.ptr_move(1, 1), "refusing to send keys")
    finally:
        backend.time.sleep = real_sleep
    st.fg_pid = 4242
    # readiness: a hover on the middle of the client area, the window under it Phosphor's
    del log[:]
    assert be.ptr_ready() is None and log == [("move", 108 + 632, 131 + 340)], log
    st.under_pid = 99
    assert "covered by a window" in be.ptr_ready()
    st.under_pid = 4242
    be.w.Pointer = None
    be._ptr = types.SimpleNamespace(move=lambda x, y: (_ for _ in ()).throw(winapi.PointerError("the pointer was sent to (1, 1) and is at (0, 0). NEEDS: x")))
    assert "NEEDS:" in be.ptr_ready(), "an undelivered move is the reason, with what the machine needs"
    be.w.client_rect_screen = lambda h: None
    assert "client area could not be read" in be.ptr_ready()
    # modifiers
    be.mod_hold("shift", True)
    be.mod_hold("shift", False)
    assert [(e[0], e[2] & 2) for e in keys] == [(0xA0, 0), (0xA0, 2)], keys
    raises(ValueError, lambda: be.mod_hold("win", True))
    # what a case left down is released after it and named (run.post_case_invariants)
    assert be.release_held() == []
    be._ptr = types.SimpleNamespace(held={1}, release_all=lambda: log.append(("release_all",)))
    be.mod_hold("ctrl", True)
    del keys[:], log[:]
    assert be.release_held() == ["mouse button 1", "the ctrl key"] and log == [("release_all",)] and [(e[0], e[2] & 2) for e in keys] == [(0xA2, 2)]
    be._ptr = None
    assert be.release_held() == [], "released once"
    import run as runmod
    prof = types.SimpleNamespace(ref_active=False, remote=None)
    import lab as labmod
    cx = types.SimpleNamespace(arm=None, remote=labmod.LocalRemote(prof))
    fake = types.SimpleNamespace(guard_override=None, release_held=lambda: ["mouse button 1"], refocus=lambda: None)
    assert runmod.post_case_invariants(cx, prof, fake) == ["the injector still held mouse button 1 (released)"]
    fake.release_held = lambda: []
    assert runmod.post_case_invariants(cx, prof, fake) == []
    # the X keysym names the shared cases use for the tab keys
    assert backend.WindowsBackend.NAMED["next"] == 0x22 and backend.WindowsBackend.NAMED["prior"] == 0x21
    dn, up, vk = be._chord("ctrl+Next")
    assert vk == 0x22 and dn[0][0] == 0xA2 and dn[-1][2] & 1, "ctrl + PageDown, an extended key"
    assert be._chord("ctrl+Prior")[2] == 0x21
    assert V.KX(types.SimpleNamespace(be=types.SimpleNamespace(name="windows")), "next_tab").split("+")[-1].lower() in backend.WindowsBackend.NAMED
    ws = open(os.path.join(ROOT, "app", "src", "workspace", "mod.rs"), encoding="utf-8").read()
    assert '.with_linux_or_windows_key_binding("ctrl-pagedown")' in ws and '.with_linux_or_windows_key_binding("ctrl-pageup")' in ws


@check
def w4win_backend_window_size():
    """the window size: SetWindowPos read back. A taken size and a clamped one are no error, a window that kept its size is; a maximised window is
    restored first and maximised again by window_restore, which also puts the place back and never raises"""
    be, st, log, _keys = _backend()
    saved = be.window_save()
    assert saved == dict(hwnd=7, rect=(100, 100, 1280, 720), zoomed=False) and be.window_size() == (1280, 720)
    assert be.window_resize(900, 600) is None and be.window_size() == (900, 600)
    assert be.window_resize(900, 600) is None and log.count(("set_window_rect", 900, 600, None, None)) == 1, "already that size: nothing is sent"
    st.clamp = (1040, 780)
    assert be.window_resize(1300, 800) is None and be.window_size() == (1040, 780), "clamped by the desktop: a change is the proof"
    st.clamp, st.set_ok = None, False
    assert "still 1040x780" in be.window_resize(720, 460), "accepted by SetWindowPos and not applied is an error, after the give-up wait"
    st.set_ok = True
    be.window_restore(saved)
    assert tuple(st.rect) == (100, 100, 1280, 720) and ("focus", 7) in log
    # maximised
    st.zoomed = True
    saved = be.window_save()
    assert saved["zoomed"] is True
    del log[:]
    assert be.window_resize(900, 600) is None and log[0] == ("ShowWindow", 9) and be.window_size() == (900, 600)
    be.window_restore(saved)
    assert st.zoomed is True and log[-2:] == [("ShowWindow", 3), ("focus", 7)], log
    be.window_restore(None)
    be.w.window_rect = lambda h: (_ for _ in ()).throw(OSError("gone"))
    be.window_restore(dict(hwnd=7, rect=(1, 2, 3, 4), zoomed=False))        # never raises
    be2, _st, _log, _k = _backend()
    be2.pid_fn = lambda: None
    assert be2.window_save() is None and be2.window_resize(900, 600) == "the app has no window" and be2.client_origin() is None


class _FakeWinBe:
    name = "windows"

    def __init__(self, fail_move_at=None):
        self.log, self.fail_move_at, self.moves = [], fail_move_at, 0

    def ptr_move(self, x, y):
        self.moves += 1
        if self.fail_move_at == self.moves:
            raise winapi.PointerError("off the screen")
        self.log.append(("move", int(x), int(y)))

    def ptr_button(self, b, d):
        self.log.append(("btn", b, d))

    def mod_hold(self, mod, down):
        self.log.append(("mod", mod, down))


@check
def w4win_mouse_class_drives_the_windows_backend():
    """cases_ui.Mouse on Windows: a click is a move and balanced presses, a drag holds the button over eight moves and releases it even when a move
    raises; cases_pointer.Held presses and releases the modifier even when the body raises"""
    real_sleep, U.time.sleep = U.time.sleep, lambda s: None
    try:
        be = _FakeWinBe()
        m = U.Mouse(be)
        m.click(50, 60, 1, 2)
        assert be.log == [("move", 50, 60), ("btn", 1, True), ("btn", 1, False), ("btn", 1, True), ("btn", 1, False)], be.log
        be = _FakeWinBe()
        U.Mouse(be).click(5, 6, 3)
        assert be.log == [("move", 5, 6), ("btn", 3, True), ("btn", 3, False)]
        be = _FakeWinBe()
        U.Mouse(be).drag(100, 200, 180, 240)
        assert be.log[0] == ("move", 100, 200) and be.log[1] == ("btn", 1, True) and be.log[-1] == ("btn", 1, False)
        assert [e for e in be.log if e[0] == "move"][1:] == [("move", 100 + 10 * i, 200 + 5 * i) for i in range(1, 9)], be.log
        be = _FakeWinBe(fail_move_at=4)
        raises(winapi.PointerError, lambda: U.Mouse(be).drag(100, 200, 180, 240))
        assert be.log.count(("btn", 1, True)) == 1 and be.log[-1] == ("btn", 1, False), "the button is released when a move of the drag raises"
        # Held
        ctx = types.SimpleNamespace(be=_FakeWinBe())
        with P.Held(ctx, "shift"):
            ctx.be.ptr_button(1, True)
        assert ctx.be.log == [("mod", "shift", True), ("btn", 1, True), ("mod", "shift", False)]
        ctx = types.SimpleNamespace(be=_FakeWinBe())

        def body():
            with P.Held(ctx, "ctrl"):
                raise KeyError("x")
        raises(KeyError, body)
        assert ctx.be.log == [("mod", "ctrl", True), ("mod", "ctrl", False)], "the modifier is released when the body raises"
    finally:
        U.time.sleep = real_sleep


def _win_ctx(log_text, origin=(108, 131), ready=None):
    be = types.SimpleNamespace(name="windows", pid_fn=lambda: 4242, client_origin=lambda: origin, ptr_ready=lambda: ready)
    ctx = types.SimpleNamespace(be=be, win=True, shots=[])
    ctx.log_since = lambda off: log_text[off:]
    ctx.until = lambda pred, timeout=10, step=0.05: pred()
    ctx.shot = lambda tag: (ctx.shots.append(tag), "/nowhere/%s.png" % tag)[1]
    return ctx


@check
def w4win_positions_come_from_the_hook_only():
    """on Windows a position is the app's `appci_find` line plus the client area's origin; without a line nothing is OCR'd, the part is UNVERIFIABLE
    (never a pass, never a FAIL of the app), and a pointer that is not delivered makes the case NOT JUDGED with the NEEDS text"""
    log = "old appci_find token=abcd x0=1.0 y0=2.0 x1=3.0 y1=4.0\nnew appci_find token=abcd x0=10.0 y0=20.0 x1=50.0 y1=40.0\n"
    ctx = _win_ctx(log)
    assert U._hook_word(ctx, "abcd", 0) == ("abcd", 108 + 30, 131 + 30, 118, 158, 151, 171), "the newest line, offset by the client area's origin"
    assert U._hook_word(ctx, "nope", 0) is None
    assert U._hook_word(_win_ctx(log, origin=None), "abcd", 0) is None, "no window origin: no position"
    os.environ["APPCI_NO_FIND_HOOK"] = "1"
    try:
        assert U._hook_word(ctx, "abcd", 0) is None
    finally:
        del os.environ["APPCI_NO_FIND_HOOK"]
    real_run = U.subprocess.run
    U.subprocess.run = lambda *a, **k: (_ for _ in ()).throw(AssertionError("no tool may be run for a position on Windows: %r" % (a,)))
    try:
        assert U.find_token(ctx, "t", "abcd", since=0)[0][1:3] == (138, 161)
        assert U.find_token(ctx, "t", "nope", since=0) == (None, "/nowhere/t.png"), "no OCR fallback: not found, with the screenshot as evidence"
        assert U.words(ctx, "w") == ([], "/nowhere/w.png")
        assert P.locate_many(ctx, None, "lm", ("abcd", "nope"), 0) == {"abcd": U._hook_word(ctx, "abcd", 0)}
    finally:
        U.subprocess.run = real_run
    r = cases.Result("x", "terminal")
    assert U.wl_check(ctx, r, False, "token not found", "OCR") is False and not r.fails and r.noref and "appci_find" in r.noref[0] and "UNVERIFIABLE" in r.noref[0]
    r = cases.Result("x", "terminal")
    assert U.wl_check(ctx, r, False, "nothing was copied", "pointer") is False and r.fails == ["nothing was copied"] and not r.noref, "a pointer claim is a plain check on Windows"
    assert U.wl_check(ctx, r, True, "fine", "OCR") is True
    assert P.need_all(ctx, cases.Result("x", "terminal"), {}, ("abcd",), "tag") is False
    # tesseract is not needed on Windows for a ported case, and still is everywhere else
    real_which, U.shutil.which = U.shutil.which, lambda n: None
    try:
        U.need_tesseract(cases.Result("x", "terminal"), ctx)
        raises(cases.Skip, lambda: U.need_tesseract(cases.Result("x", "terminal"), types.SimpleNamespace(be=types.SimpleNamespace(name="x11"))), "tesseract")
        raises(cases.Skip, lambda: U.need_tesseract(cases.Result("x", "terminal")), "tesseract")
    finally:
        U.shutil.which = real_which
    # the pointer itself
    assert isinstance(U.mouse_or_skip(ctx, cases.Result("x", "terminal")), U.Mouse)
    bad = _win_ctx(log, ready="the pointer was sent to (740, 471) and is at (0, 0)")
    msg = raises(cases.HostMissing, lambda: U.mouse_or_skip(bad, cases.Result("x", "terminal")), "NEEDS:")
    assert "was not judged" in msg and "(740, 471)" in msg
    import run as runmod
    r = cases.Result("x", "terminal")
    assert runmod.skip_status(r, cases.HostMissing(msg)) == "noref", "not judged is NOREF in the report, never a skip and never a pass"
    assert W.window_origin(ctx) == (108, 131) and W.hook_rows(ctx, "abcd", 0)[-1][1:3] == (138, 161)


@check
def w4win_panel_rows_from_the_hook():
    """the vertical-tabs panel rows on Windows: of each tab token's sightings the newest one in its leftmost column is the panel's (the same token
    is drawn in the tab's pane, further right); a tab never sighted is left out; the result is judged by rows_stacked like the OCR rows"""
    s = {2: [(60, 120), (420, 300), (430, 340), (60, 120)], 3: [(430, 300), (61, 168)], 4: [(62, 216), (425, 310)]}
    rows = V.hook_panel_rows(s)
    assert rows == [(2, 60, 120), (3, 61, 168), (4, 62, 216)] and V.rows_stacked(rows)[0] is True, rows
    moved = {2: [(60, 120), (60, 150), (400, 90)], 3: [(60, 168), (60, 198)]}
    assert V.hook_panel_rows(moved) == [(2, 60, 150), (3, 60, 198)], "a row that moved: the newest sighting in the panel's column"
    assert V.hook_panel_rows({2: [], 3: [(60, 10)], 4: []}) == [(3, 60, 10)] and V.hook_panel_rows({}) == []
    bar = V.hook_panel_rows({2: [(200, 20)], 3: [(380, 21)], 4: [(560, 20)]})
    assert V.rows_stacked(bar)[0] is False, "a horizontal tab bar is not a panel"
    text = "a appci_find token=vtaaaaa x0=10.0 y0=100.0 x1=70.0 y1=116.0\nb appci_find token=vtbbbbb x0=10.0 y0=140.0 x1=70.0 y1=156.0\n"
    assert [(w[1], w[2]) for w in W.find_rows(text, "vtbbbbb", (100, 50))] == [(140, 198)]
    src = inspect.getsource(V.c_vertical_tabs_panel_click) + inspect.getsource(V._panel_rows_from_hook)
    for needle in ("register=U.is_windows(ctx)", "hook_panel_rows(sightings)", "r.unverifiable(", "rows_stacked(rows)", "m.click(cx, cy, 1)", "got == dirs[tabno - 1]"):
        assert needle in src, needle
    assert "U.register_find(ctx, *toks)" in inspect.getsource(V.open_tabs), "the tokens are registered before any title holds them"
    assert "r.skip(" not in inspect.getsource(V.vt_need), "no platform skips the vertical-tabs cases for the settings rewrite any more"


@check
def w4win_settings_file_the_app_reads():
    """cases_ui.settings_path is the file the running app reads: on Windows WindowsApp.config, not the profile's source file; Linux and macOS as before"""
    prof = types.SimpleNamespace(home=os.path.join("h", "home"))
    winapp = types.SimpleNamespace(config=os.path.join("L", "phosphor", "Phosphor", "config"))
    assert U.settings_path(types.SimpleNamespace(win=True, app=winapp, prof=prof)) == os.path.join("L", "phosphor", "Phosphor", "config", "settings.toml")
    assert U.settings_path(types.SimpleNamespace(win=False, app=types.SimpleNamespace(settings="/m/.phosphor/settings.toml"), prof=prof)) == "/m/.phosphor/settings.toml"
    assert U.settings_path(types.SimpleNamespace(win=False, app=types.SimpleNamespace(), prof=prof)) == os.path.join("h", "home", ".config", "phosphor", "settings.toml")
    assert U.settings_path(types.SimpleNamespace(win=False, app=None, prof=prof)).endswith("settings.toml")
    appsrc = open(os.path.join(HERE, "app.py"), encoding="utf-8").read().split("class WindowsApp")[1]
    assert 'self.config = os.path.join(os.environ["LOCALAPPDATA"], "phosphor", "Phosphor", "config")' in appsrc
    assert 'shutil.copy2(src, os.path.join(self.config, "settings.toml"))' in appsrc, "start() installs the profile's file where settings_path points"


@check
def w4win_registry_winmap_and_docs():
    """the eight cases run in the Git Bash pane and SKIP in the PowerShell pane with a reason; no Windows reason still claims the pointer or the resize is
    missing; the two older cases join the Windows fix tier through PROVING_WINDOWS only; the app's Windows log level shows the lines the cases read;
    docs/testing.md describes all of it"""
    reg = {c["name"]: c for c in cases.CASES}
    for n in POINTER_CASES + RESIZE_CASES:
        assert n in reg and reg[n]["gui"], n
        assert n in winmap.GITBASH_ONLY and n not in winmap.NOT_WINDOWS and winmap.skip_reason(n, "gitbash") is None, n
        why = winmap.skip_reason(n, "powershell")
        assert why and "Git Bash" in why and ("T31" in why if n in POINTER_CASES else "T33" in why), (n, why)
    for n, why in winmap.NOT_WINDOWS.items():
        assert "no pointer synthesis" not in why and "no window resize" not in why and "not wired into cases_ui.settings_path" not in why, (n, why)
    assert "resize only" in winmap.NOT_WINDOWS["minimize_fullscreen"] and "r14-test-gaps T3" in winmap.NOT_WINDOWS["tab_pointer_use"]
    assert W.WIN_STATE_WHY in inspect.getsource(W) and "WIN_STATE_WHY" in inspect.getsource(W.c_minimize_fullscreen)
    assert "ctx.win" not in inspect.getsource(W.need_window), "the Window resizes on Windows: no blanket skip"
    assert "U.mouse_or_skip(ctx, r)" in inspect.getsource(P.c_selection_modes) and "need_tesseract(r, ctx)" in inspect.getsource(P.c_selection_modes)
    for fn in (U.c_mouse_copy, V.c_copy_wrapped_drag, V.c_vertical_tabs_panel_click):
        assert "need_tesseract(r, ctx)" in inspect.getsource(fn), fn.__name__
    assert "need_tesseract" not in inspect.getsource(W.c_context_menu_actions), "placed from the app's position lines only, on every platform"
    # the Windows proving list
    assert winmap.PROVING_WINDOWS == ("ui_mouse_select_copy", "ui_resize_typing")
    for n in winmap.PROVING_WINDOWS:
        assert n in reg and n not in cases.SMOKE and n not in cases.PROVING and reg[n]["modes"] == ("terminal", "agent"), n
    for mode in ("terminal", "agent"):
        for pane in ("powershell", "gitbash"):
            names, _o = winmap.tier_plan("windows", "smoke", mode, pane, cases.SMOKE, cases.PROVING)
            assert set(winmap.PROVING_WINDOWS) <= set(names) and len(names) == len(set(names)), (mode, pane)
    assert winmap.tier_plan("windows", "full", "agent", "gitbash", cases.SMOKE, cases.PROVING)[0] is None, "the full tier is every case, as before"
    for n in ("selection_modes", "context_menu_actions", "resize_reflow_output", "ui_copy_wrapped_drag", "ui_vertical_tabs_many", "ui_vertical_tabs_panel_click"):
        assert n in cases.PROVING, n
    # the lines the cases read are logged on Windows
    import app as appmod
    assert appmod.rust_log_enabled(appmod.WINDOWS_RUST_LOG, "warpui::appci_find", "info"), "the find hook's lines"
    assert appmod.rust_log_enabled(appmod.WINDOWS_RUST_LOG, "warp::input_order", "debug"), "hop=grid_resize, hop=tab_activate, hop=enter_begin"
    assert 'e["PHOSPHOR_APPCI_FIND_FILE"]' in open(os.path.join(HERE, "app.py"), encoding="utf-8").read().split("class WindowsApp")[1]
    hook = open(os.path.join(ROOT, "crates", "warpui_core", "src", "appci_find.rs"), encoding="utf-8").read()
    assert '"appci_find token={token} x0={:.1} y0={:.1} x1={:.1} y1={:.1}"' in hook and "rect[0] * scale" in hook, "device pixels of the window, the shape FIND_RE reads"
    doc = open(os.path.join(ROOT, "docs", "testing.md"), encoding="utf-8").read()
    for needle in ("Round 15 wave 4", "#2060", "selftest_w4win.py", "PROVING_WINDOWS", "winapi.Pointer", "SetWindowPos", "NEEDS:", "release_held") + tuple(
            "`%s`" % n for n in POINTER_CASES + RESIZE_CASES):
        assert needle in doc, "docs/testing.md does not mention %s" % needle
    assert os.path.isfile(os.path.join(ROOT, "docs", "design", "r15-w4win.md"))
