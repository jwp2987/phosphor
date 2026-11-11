# appci/selftest_desk.py: offline self-tests of cases_desk.py, notify_recorder.py and dragsrc.py (round 15 wave 4, r14-test-gaps T35 / T37 / T38 /
# T39). Pure Python plus `sh -n` and one unix socket in a temp dir: no app, display, D-Bus daemon or network. selftest.py appends CHECKS to its own
# list, so `run.py --self-test` and `selftest.py` both run these.
import json, os, shutil, socket, struct, subprocess, sys, tempfile, threading, types

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import cases, winmap  # noqa: E402
import app as appmod  # noqa: E402
import cases_desk as D  # noqa: E402
import dragsrc  # noqa: E402
import notify_recorder as NR  # noqa: E402

CHECKS = []
NAMES = ("scale_matrix", "desktop_notification", "desktop_notification_agent_question", "drag_drop_file", "ime_compose")
REPO = os.path.abspath(os.path.join(HERE, "..", "..", ".."))


def check(f):
    CHECKS.append(f)
    return f


def _skips(fn, ctx):
    """the Skip message `fn(ctx, r)` raises, or None when it does not skip"""
    try:
        fn(ctx, cases.Result("x", "terminal"))
    except cases.Skip as e:
        return str(e)
    return None


@check
def desk_cases_registry_r15_w4():
    """the five cases are registered once, in PROVING and not in SMOKE; only the agent-question half is in the fake pass and it is agent-only; the
    additions to the shared tuples are statements of their own (a sibling branch's merge keeps both); Windows has a class for each; the docs
    describe each; the launcher has the session hook and its template still formats"""
    reg = {c["name"]: c for c in cases.CASES}
    for n in NAMES:
        assert n in reg and [c["name"] for c in cases.CASES].count(n) == 1, n
        assert n in cases.PROVING and n not in cases.SMOKE and reg[n]["gui"], n
        assert (n in cases.PROVING_FAKE) == (n == "desktop_notification_agent_question"), n
        assert reg[n]["modes"] == (("agent",) if n in cases.PROVING_FAKE else ("terminal", "agent")), n
    assert len(set(cases.PROVING)) == len(cases.PROVING), "a case is in PROVING twice"
    src = open(os.path.join(HERE, "cases.py"), encoding="utf-8").read()
    assert '\nPROVING += ("scale_matrix"' in src and '\nPROVING_FAKE += ("desktop_notification_agent_question",)' in src
    assert winmap.unclassified(sorted(reg)) == []
    for n in ("desktop_notification", "desktop_notification_agent_question"):
        assert winmap.skip_reason(n, "powershell") is None and winmap.skip_reason(n, "gitbash") is None, n
    assert winmap.skip_reason("ime_compose", "gitbash") is None and "UTF-16" in winmap.skip_reason("ime_compose", "powershell")
    for n in ("scale_matrix", "drag_drop_file"):
        assert "NEEDS:" in winmap.NOT_WINDOWS[n] and all(winmap.skip_reason(n, p) for p in ("powershell", "gitbash")), n
    wsrc = open(os.path.join(HERE, "winmap.py"), encoding="utf-8").read()
    assert "\nBOTH |= {" in wsrc and "\nNOT_WINDOWS.update({" in wsrc and "\nGITBASH_ONLY.update({" in wsrc
    doc = open(os.path.join(REPO, "docs", "testing.md"), encoding="utf-8").read()
    assert all(("`%s`" % n) in doc for n in NAMES), "docs/testing.md does not describe every wave 4 desktop case"
    assert "Round 15 wave 4" in doc and "selftest_desk.py" in doc and "notify_recorder.py" in doc and "dragsrc.py" in doc and D.SESSION_PRE_ENV in doc
    inner = appmod.INNER % {"home": "/h", "run": "/r", "bin": "/b", "wrap": ""}
    hook = 'if [ -n "$%s" ] && [ -r "$%s" ]; then . "$%s"; fi' % ((D.SESSION_PRE_ENV,) * 3)
    assert hook in inner and inner.index(hook) < inner.index('echo $$ > "/r/app.pid"'), "the session hook must run before the app's pid is published"
    if shutil.which("bash"):
        p = subprocess.run(["bash", "-n"], input=inner.encode(), capture_output=True)
        assert p.returncode == 0, p.stderr.decode()


@check
def desk_cases_skip_with_the_exact_need():
    """off its platforms every case SKIPS and the text says `NEEDS:` and what; the macOS notification arm is gated on the owner's answer to the
    permission prompt (an unanswered prompt belongs to UserNotificationCenter, a dialog owner that stops the pass)"""
    win = types.SimpleNamespace(win=True, be=types.SimpleNamespace(name="windows"), app=object(), log_path="x")
    mac = types.SimpleNamespace(win=False, be=types.SimpleNamespace(name="mac"), app=types.SimpleNamespace(start=1, stop=1), log_path="x")
    for fn, c, words in ((D.c_scale_matrix, win, ("NEEDS:", "T31")), (D.c_scale_matrix, mac, ("NEEDS:", "pointer tool")),
                         (D.c_drag_drop_file, win, ("NEEDS:", "OLE drag source")), (D.c_drag_drop_file, mac, ("NEEDS:", "pasteboard drag source")),
                         (D.c_ime_compose, mac, ("NEEDS:", "intl_ime_pinyin"))):
        msg = _skips(fn, c)
        assert msg and all(w in msg for w in words), (fn.__name__, c.be.name, msg)
    old = os.environ.pop(D.MAC_NOTIFY_ENV, None)
    try:
        msg = _skips(D.c_desktop_notification, mac)
        assert msg and "NEEDS:" in msg and D.MAC_NOTIFY_ENV in msg and "UserNotificationCenter" in msg, msg
    finally:
        if old is not None:
            os.environ[D.MAC_NOTIFY_ENV] = old
    import backend
    assert "usernotificationcenter" in backend.MacBackend.DIALOG_OWNERS, "the reason the macOS arm is gated"
    for text in (D.DRAG_NEEDS, D.LINUX_IME_NEEDS, D.WIN_IME_NEEDS, D.WIN_SCALE_NEEDS):
        assert text.startswith("NEEDS:"), text
    assert "python3-gi" in D.DRAG_NEEDS and "dragon-drop" in D.DRAG_NEEDS and "fcitx5-chinese-addons" in D.LINUX_IME_NEEDS and "Microsoft Pinyin" in D.WIN_IME_NEEDS
    sim = types.SimpleNamespace(win=False, be=types.SimpleNamespace(name="sim"), app=None)
    assert "Linux App launcher" in _skips(D.c_scale_matrix, sim) and "Linux App launcher" in _skips(D.c_drag_drop_file, sim)


# ---------------------------------------------------------------- notify_recorder.py
def _notify_call(serial=7, summary="sum", body="bo dy", sender=":1.9", replaces=0, flags=0):
    return NR.build_message(NR.METHOD_CALL, serial, {NR.F_PATH: NR.PATH, NR.F_IFACE: NR.NAME, NR.F_MEMBER: "Notify", NR.F_DEST: NR.NAME, NR.F_SENDER: sender},
                            "susssasa{sv}i", ("phosphor", replaces, "", summary, body, ["default", "Open"],
                                              {"urgency": ("y", 1), "desktop-entry": ("s", "phosphor"), "x": ("ai", [1, 2, 3])}, 5000), flags=flags)


@check
def notify_recorder_wire_roundtrip():
    """the D-Bus marshalling: signatures split into complete types, values survive a round trip with their alignment (a byte before an int, a
    variant inside a dict inside an array), a message's size is known from its fixed header, and a non-ASCII summary is kept"""
    assert NR.split_sig("susssasa{sv}i") == ["s", "u", "s", "s", "s", "as", "a{sv}", "i"]
    assert NR.split_sig("a(yv)(ii)aai") == ["a(yv)", "(ii)", "aai"]
    for bad in ("a{sv", "z"):
        try:
            NR.split_sig(bad)
            raise AssertionError(bad)
        except ValueError:
            pass
    for sig, val in (("y", 7), ("u", 4000000000), ("i", -5), ("x", -(1 << 40)), ("s", "hé 日"), ("as", ["a", "", "ccc"]), ("(yu)", (1, 2)),
                     ("a{sv}", [("k", ("u", 9)), ("s", ("s", "v")), ("l", ("ai", [1, 2]))]), ("aay", [[1, 2], [], [3]]), ("d", 1.5), ("b", 1)):
        buf = bytearray(b"\x01")                 # an odd start: every type must pad to its own alignment
        NR.marshal(buf, sig, val)
        got, off = NR.unmarshal(bytes(buf), 1, sig)
        assert off == len(buf), (sig, off, len(buf))
        assert got == val or (sig == "(yu)" and got == (1, 2)), (sig, got)
    msg = _notify_call(summary="sleep 12 finished ✓")
    assert NR.message_size(msg) == len(msg) and NR.message_size(msg[:15]) is None and NR.message_size(msg + b"extra") == len(msg)
    assert struct.unpack_from("<I", msg, 4)[0] > 0, "the body length is in the fixed header"
    m = NR.parse_message(msg)
    assert m["type"] == NR.METHOD_CALL and m["serial"] == 7 and m["fields"][NR.F_MEMBER] == "Notify" and m["fields"][NR.F_SIG] == "susssasa{sv}i"
    assert m["args"][0] == "phosphor" and m["args"][3] == "sleep 12 finished ✓" and m["args"][5] == ["default", "Open"] and m["args"][7] == 5000
    assert dict(m["args"][6])["urgency"] == ("y", 1) and dict(m["args"][6])["x"] == ("ai", [1, 2, 3])
    try:
        NR.message_size(b"B" + msg[1:])
        raise AssertionError("a big-endian message must be refused, not misread")
    except ValueError:
        pass
    assert NR.parse_address("unix:path=/tmp/dbus-x,guid=ab") == "/tmp/dbus-x" and NR.parse_address("tcp:host=h;unix:abstract=/tmp/q,guid=1") == b"\0/tmp/q"
    try:
        NR.parse_address("tcp:host=localhost")
        raise AssertionError
    except ValueError:
        pass


@check
def notify_recorder_service_records_each_notify_once():
    """the service: one record and one reply (the id) per Notify; a replaces_id keeps its id; a NO_REPLY call is recorded and not answered; the
    daemon questions notify-rust may ask are answered; an unknown method is an error reply, never silence; two seconds later one
    NotificationClosed(id, expired) per notification, once"""
    clock = [100.0]
    svc = NR.Service(now=lambda: clock[0])
    out = svc.handle(NR.parse_message(_notify_call(serial=7, summary="a")))
    assert len(out) == 1 and len(svc.records) == 1
    rep = NR.parse_message(out[0])
    assert rep["type"] == NR.METHOD_RETURN and rep["fields"][NR.F_REPLY] == 7 and rep["fields"][NR.F_DEST] == ":1.9" and rep["args"] == [1]
    rec = svc.records[0]
    assert rec["summary"] == "a" and rec["body"] == "bo dy" and rec["app"] == "phosphor" and rec["id"] == 1 and rec["sender"] == ":1.9" and rec["actions"] == ["default", "Open"]
    assert NR.parse_message(svc.handle(NR.parse_message(_notify_call(serial=8, summary="b")))[0])["args"] == [2]
    assert NR.parse_message(svc.handle(NR.parse_message(_notify_call(serial=9, summary="c", replaces=1)))[0])["args"] == [1]
    assert svc.handle(NR.parse_message(_notify_call(serial=10, summary="d", flags=1))) == [] and [x["summary"] for x in svc.records] == ["a", "b", "c", "d"]

    def call(member, iface=NR.NAME, sig="", args=()):
        return svc.handle(NR.parse_message(NR.build_message(NR.METHOD_CALL, 20, {NR.F_PATH: NR.PATH, NR.F_IFACE: iface, NR.F_MEMBER: member, NR.F_SENDER: ":1.9"}, sig, args)))
    assert NR.parse_message(call("GetCapabilities")[0])["args"] == [["body"]]
    assert len(NR.parse_message(call("GetServerInformation")[0])["args"]) == 4
    assert NR.parse_message(call("CloseNotification", sig="u", args=(1,))[0])["type"] == NR.METHOD_RETURN
    assert NR.parse_message(call("Ping", iface="org.freedesktop.DBus.Peer")[0])["type"] == NR.METHOD_RETURN
    err = NR.parse_message(call("Bogus")[0])
    assert err["type"] == NR.ERROR and err["fields"][NR.F_ERROR].endswith("UnknownMethod") and err["fields"][NR.F_REPLY] == 20
    assert len(svc.records) == 4, "only Notify records"
    assert svc.handle(NR.parse_message(NR.build_message(NR.SIGNAL, 3, {NR.F_PATH: "/", NR.F_IFACE: "a.b", NR.F_MEMBER: "NameAcquired"}, "s", ("x",)))) == []
    assert svc.due_signals() == []
    clock[0] += NR.CLOSE_AFTER_S + 0.1
    sigs = [NR.parse_message(s) for s in svc.due_signals()]
    assert [s["args"] for s in sigs] == [[1, 1], [2, 1], [1, 1], [3, 1]], sigs
    assert all(s["type"] == NR.SIGNAL and s["fields"][NR.F_MEMBER] == "NotificationClosed" and s["fields"][NR.F_PATH] == NR.PATH for s in sigs)
    assert svc.due_signals() == []


def _fake_bus(path, script, seen):
    """a one-connection stand-in for dbus-daemon on the unix socket `path`: EXTERNAL auth, Hello, RequestName (answered with script['request_name']),
    then the Notify calls of script['notify'], reading the reply of each; what the client sent is appended to `seen`"""
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(path)
    srv.listen(1)

    def run():
        conn, _ = srv.accept()
        conn.settimeout(10)
        buf = b""
        while b"BEGIN\r\n" not in buf:
            chunk = conn.recv(4096)
            if not chunk:
                return
            buf += chunk
            if buf.startswith(b"\0AUTH EXTERNAL ") and b"\r\n" in buf and not seen:
                seen.append(("auth", buf.split(b"\r\n")[0]))
                conn.sendall(b"OK 1234deadbeef\r\n")
        buf = buf.split(b"BEGIN\r\n", 1)[1]

        def read():
            nonlocal buf
            while True:
                n = NR.message_size(buf)
                if n is not None and len(buf) >= n:
                    data, buf = buf[:n], buf[n:]
                    return NR.parse_message(data)
                buf += conn.recv(65536)
        serial = 0
        for _ in range(2):
            m = read()
            seen.append((m["fields"][NR.F_MEMBER], m["args"]))
            serial += 1
            if m["fields"][NR.F_MEMBER] == "Hello":
                conn.sendall(NR.build_message(NR.METHOD_RETURN, serial, {NR.F_REPLY: m["serial"]}, "s", (":1.7",)))
            else:
                conn.sendall(NR.build_message(NR.METHOD_RETURN, serial, {NR.F_REPLY: m["serial"]}, "u", (script["request_name"],)))
        if script["request_name"] == 1:
            for i, summary in enumerate(script["notify"]):
                conn.sendall(_notify_call(serial=50 + i, summary=summary))
                m = read()
                seen.append(("reply", m["fields"].get(NR.F_REPLY), m["args"]))
        else:
            m = read()                                   # GetNameOwner
            conn.sendall(NR.build_message(NR.METHOD_RETURN, 9, {NR.F_REPLY: m["serial"]}, "s", (":1.2",)))
        conn.close()
        srv.close()
    t = threading.Thread(target=run, daemon=True)
    t.start()
    return t


@check
def notify_recorder_serves_a_bus_end_to_end():
    """serve() against a stand-in bus on a unix socket: auth as this uid, Hello, RequestName with REPLACE_EXISTING | DO_NOT_QUEUE, the ready line
    with the bus address, one log line and one reply per Notify, a clean exit when the bus goes away. A bus where another daemon keeps the name
    gives a ready line that says so (ok false) and nothing is recorded: the case then reports UNVERIFIABLE, never a pass on an empty log."""
    tmp = tempfile.mkdtemp(prefix="appci_nr_")
    try:
        for name, rn, notes in (("own", 1, ["one 日", "two"]), ("taken", 3, [])):
            sock, log, ready, seen = os.path.join(tmp, name + ".sock"), os.path.join(tmp, name + ".jsonl"), os.path.join(tmp, name + ".ready"), []
            t = _fake_bus(sock, dict(request_name=rn, notify=notes), seen)
            rc = NR.serve(log, ready, "unix:path=%s,guid=00" % sock)
            t.join(10)
            assert not t.is_alive(), "the stand-in bus did not finish (%s)" % name
            info = json.loads(open(ready, encoding="utf-8").read())
            assert seen[0] == ("auth", b"\0AUTH EXTERNAL " + str(os.getuid()).encode().hex().encode()), seen[0]
            assert seen[1][0] == "Hello" and seen[2] == ("RequestName", [NR.NAME, 6]), seen[1:3]
            if rn == 1:
                assert rc == 0 and info["ok"] and info["address"].startswith("unix:path=") and info["name"] == ":1.7" and info["pid"] == os.getpid(), info
                recs = NR.read_log(log)
                assert [x["summary"] for x in recs] == notes and [x["id"] for x in recs] == [1, 2], recs
                assert [s for s in seen if s[0] == "reply"] == [("reply", 50, [1]), ("reply", 51, [2])], seen
            else:
                assert rc == 1 and info["ok"] is False and ":1.2" in info["why"] and "RequestName returned 3" in info["why"], info
                assert NR.read_log(log) == []
        open(os.path.join(tmp, "torn.jsonl"), "w").write('{"summary": "a"}\n{"summ')
        assert NR.read_log(os.path.join(tmp, "torn.jsonl")) == [{"summary": "a"}] and NR.read_log(os.path.join(tmp, "missing")) == []
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


@check
def notification_verdicts_and_settings():
    """judge_notifications: the service and the app's hop must both show exactly the wanted number; one too many, none, a notification that does not
    name the command, and a hop that disagrees with the service each fail; without a recorder the hop alone judges. The harness's own control
    never counts. The settings edit enables notifications in the file format the app reads (a plain integer of seconds) and refuses a file it
    cannot edit. The hop's format is the one the Rust unit test pins."""
    rec = lambda s, b="Latest output: x", app="phosphor": dict(summary=s, body=b, app=app)       # noqa: E731
    ok = [rec("sleep 12; echo tok9 >> f finished after 12s")]
    assert D.judge_notifications(ok, [(40, 16)], 1, needle="tok9") == []
    assert D.judge_notifications([], [], 0) == []
    assert "received 0 notification(s), expected 1" in D.judge_notifications([], [], 1, needle="tok9")[0]
    assert "received 2 notification(s), expected 1" in D.judge_notifications(ok * 2, [(1, 1), (1, 1)], 1)[0]
    assert "received 1 notification(s), expected 0" in D.judge_notifications(ok, [(1, 1)], 0)[0]
    assert "does not name the command" in D.judge_notifications([rec("Notification")], [(1, 1)], 1, needle="tok9")[0]
    p = D.judge_notifications(ok, [], 1, needle="tok9")
    assert len(p) == 1 and "logged 0 hop=desktop_notification" in p[0] and "disagree" in p[0]
    assert D.judge_notifications([], [(3, 4)], 1, recorded=False) == [] and D.judge_notifications([], [], 1, recorded=False) != []
    assert D.judge_notifications([], [(3, 4), (3, 4)], 1, recorded=False) != [] and D.judge_notifications([], [(3, 4)], 0, recorded=False) != []
    assert D.own_records([rec("control x", app=D.CONTROL_APP), ok[0]]) == ok
    line = "2026-11-10 [DEBUG] input_order hop=desktop_notification title_chars=25 body_chars=17 t=1234.567"
    assert D.notify_hops(line + "\n" + line.replace("25", "7") + "\nhop=desktop_notification_other x") == [(25, 17), (7, 17)]
    rs = open(os.path.join(REPO, "crates", "warpui_core", "src", "input_order.rs"), encoding="utf-8").read()
    assert '"desktop_notification"' in rs and '"title_chars={} body_chars={}"' in rs and "fn desktop_notification_lines_carry_counts_and_no_text" in rs
    core = open(os.path.join(REPO, "crates", "warpui_core", "src", "core", "app.rs"), encoding="utf-8").read()
    assert "crate::input_order::log_desktop_notification(content.title(), content.body());" in core, "every desktop notification passes this one call"
    # settings
    base = '[general]\nrestore_session = false\n\n[appearance]\nfont_size = 13\n'
    new, why = D.notification_settings(base, 3)
    assert why is None and new.startswith(base.rstrip("\n")) and "[notifications.preferences]" in new
    for k, v in (("mode", '"enabled"'), ("long_running_threshold", "3"), ("is_long_running_enabled", "true"), ("is_needs_attention_enabled", "true"),
                 ("play_notification_sound", "false")):
        assert D.T.get_key(new, D.NOTIFY_TABLE, k) == "%s = %s" % (k, v), (k, D.T.get_key(new, D.NOTIFY_TABLE, k))
    again, _ = D.notification_settings(new, 3)
    assert again == new, "the edit is idempotent"
    had = base + '\n[notifications.preferences]\nmode = "disabled"\nlong_running_threshold = 30\n'
    new2, _ = D.notification_settings(had, 3)
    assert new2.count("[notifications.preferences]") == 1 and 'mode = "enabled"' in new2 and "long_running_threshold = 3\n" in new2 and "disabled" not in new2
    for inline in ('[notifications]\npreferences = { mode = "unset" }\n', 'notifications.preferences = { mode = "unset" }\n'):
        t, why = D.notification_settings(inline, 3)
        assert t is None and "cannot edit" in why, inline
    rs = open(os.path.join(REPO, "app", "src", "terminal", "session_settings.rs"), encoding="utf-8").read()
    assert 'toml_path: "notifications.preferences"' in rs and "pub long_running_threshold: Duration" in rs
    assert D.SHORT_S < D.THRESHOLD_S < D.LONG_S - 2
    # the launcher lines
    pre = D.recorder_pre_script("/a b/notify_recorder.py", "/r/n.jsonl", "/r/n.ready", "/r/n.err")
    assert "'/a b/notify_recorder.py' serve --log /r/n.jsonl --ready /r/n.ready" in pre and pre.rstrip().splitlines()[1].endswith("&")
    assert subprocess.run(["sh", "-n"], input=pre.encode(), capture_output=True).returncode == 0
    assert D.long_cmd("bash", 12, "echo t >> f") == "sleep 12; echo t >> f" and D.long_cmd("pwsh", 12, "Add-Content f t") == "Start-Sleep -Seconds 12; Add-Content f t"


@check
def away_gives_and_takes_the_focus():
    """Away: with a switching reference (Linux, macOS) leave / back switch sides and a refused switch is reported, not swallowed; with a
    non-switching one (Windows) leave opens the reference and back closes it, so the next leave opens a new one"""
    calls = []

    def ref(switching, refuse=False):
        def switch(side):
            calls.append(side)
            if refuse and side == "reference":
                raise RuntimeError("focus stayed on Phosphor")
        return types.SimpleNamespace(switch=switch if switching else None, close=lambda: calls.append("close"), name="ref")
    ctx = types.SimpleNamespace(be=types.SimpleNamespace(refocus=lambda **kw: calls.append("refocus")), prof=types.SimpleNamespace(ref_active=True), ref_reason=None)
    old_ref, old_close, old_sleep = D.C.ref_terminal, D.C.close_ref, D.time.sleep
    D.time.sleep = lambda s: None
    try:
        D.C.ref_terminal = lambda c: ref(True)
        D.C.close_ref = lambda rf: calls.append("close_ref")
        a = D.Away(ctx)
        assert a.open() is None and a.can_switch and a.back() is None and a.leave() is None and a.back() is None
        assert calls == ["phosphor", "reference", "phosphor"], calls
        a.close()
        assert calls[-2:] == ["close_ref", "refocus"] and a.ref is None and ctx.prof.ref_active is False
        D.C.ref_terminal = lambda c: ref(True, refuse=True)
        a = D.Away(ctx)
        assert a.open() is None and a.leave() == "focus stayed on Phosphor"
        del calls[:]
        D.C.ref_terminal = lambda c: ref(False)
        a = D.Away(ctx)
        assert a.leave() is None and not a.can_switch and a.back() is None and a.ref is None and calls == ["close", "refocus"], calls
        assert a.leave() is None and a.ref is not None
        D.C.ref_terminal = lambda c: None
        ctx.ref_reason = "no foot and no xterm"
        assert D.Away(ctx).leave() == "no foot and no xterm"
    finally:
        D.C.ref_terminal, D.C.close_ref, D.time.sleep = old_ref, old_close, old_sleep


# ---------------------------------------------------------------- scale
@check
def scale_grid_verdict_trips_on_a_wrong_scale():
    """grid_scale_verdict: the same logical window at scale s keeps the scale-1 grid (+- 8 % or two cells for device-pixel snapping); a grid laid
    out in device pixels (x s) fails, one that ignored the scale (/ s) fails, at every scale of the matrix; a pty that disagrees with the app's
    own grid fails; a dimension too small to tell is reported as weak, not judged; a missing answer is a failure, not a pass"""
    for s in D.SCALES:
        c = (40, 150)
        assert D.grid_scale_verdict(c, c, c, s) == ([], []), s
        assert D.grid_scale_verdict(c, (41, 147), (41, 147), s) == ([], []), "snapping tolerance at %s" % s
        for wrong in ((round(40 * s), round(150 * s)), (round(40 / s), round(150 / s))):
            probs, weak = D.grid_scale_verdict(c, wrong, wrong, s)
            assert len(probs) == 2 and not weak and "rows" in probs[0] and "columns" in probs[1], (s, wrong, probs)
        probs, _ = D.grid_scale_verdict(c, c, (39, 150), s)
        assert len(probs) == 1 and "did not reach the pty" in probs[0]
        assert D.grid_scale_verdict(c, c, None, s) == ([], [])
    probs, weak = D.grid_scale_verdict((6, 150), (6, 150), None, 1.25)
    assert probs == [] and weak == ["rows"]
    probs, weak = D.grid_scale_verdict((6, 150), (6, 188), None, 1.25)
    assert weak == ["rows"] and len(probs) == 1 and "columns: 188 at scale 1.25, 150 at scale 1" in probs[0] and "about 188" in probs[0] and "about 120" in probs[0]
    assert "control" in D.grid_scale_verdict(None, (1, 1), None, 2.0)[0][0] and "at scale 2" in D.grid_scale_verdict((40, 150), None, None, 2.0)[0][0]
    assert D.logical_size(1600, 1000, 1.25) == (1280, 800) and D.logical_size(1600, 1000, 1.5) == (1066, 666) and D.logical_size(1600, 1000, 2.0) == (800, 500)
    assert [D.scale_tag(s) for s in D.SCALES] == ["125", "150", "200"] and [D.fmt_scale(s) for s in D.SCALES] == ["1.25", "1.5", "2"]
    assert [D.xft_dpi(s) for s in (1.0,) + D.SCALES] == [96, 120, 144, 192]
    log = "x input_order hop=grid_resize rows=24 cols=80 pane=7 t=1.0\ny hop=grid_resize rows=30 cols=101 pane=7 t=2.0\n"
    assert D.last_grid(log) == (30, 101) and D.last_grid("nothing") is None and D.last_grid("") is None


@check
def scale_outputs_and_pointer_mapping():
    """the sway output list as the case reads it; the box a virtual pointer's absolute motion spans (one output at a scale, then two outputs);
    which output holds a window; the find hook's device pixels mapped to pointer pixels through the output's scale; Xft.dpi edits keep every
    other X resource and removing it restores the original text"""
    raw = json.dumps([
        {"name": "HEADLESS-1", "active": True, "scale": 2.0, "rect": {"x": 0, "y": 0, "width": 800, "height": 500}, "current_mode": {"width": 1600, "height": 1000}},
        {"name": "HEADLESS-2", "active": True, "scale": 1.5, "rect": {"x": 1600, "y": 0, "width": 1066, "height": 666}, "current_mode": {"width": 1600, "height": 1000}},
        {"name": "OFF", "active": False, "scale": 1.0, "rect": {"x": 0, "y": 0, "width": 0, "height": 0}},
    ])
    outs = D.sway_outputs(raw)
    assert [o["name"] for o in outs] == ["HEADLESS-1", "HEADLESS-2"] and outs[0]["mode"] == (1600, 1000) and outs[0]["rect"] == (0, 0, 800, 500) and outs[1]["scale"] == 1.5
    assert D.sway_outputs(b"not json") == [] and D.sway_outputs("{}") == [] and D.sway_outputs(None) == []
    assert D.layout_bbox(outs[:1]) == (0, 0, 800, 500) and D.layout_bbox(outs) == (0, 0, 2666, 666) and D.layout_bbox([]) is None
    assert D.output_at(outs, (0, 0, 800, 500))["name"] == "HEADLESS-1" and D.output_at(outs, (1600, 0, 800, 500))["name"] == "HEADLESS-2"
    assert D.output_at(outs, (900, 0, 100, 100)) is None and D.output_at(outs, None) is None
    # a word the app drew at device pixels 1000..1100 x 400..440 of a window at layout (1600, 0) on a scale-2 output is at layout 2100..2150 x 200..220
    tmp = tempfile.mkdtemp(prefix="appci_desk_")
    try:
        logp = os.path.join(tmp, "phosphor.log")
        open(logp, "w").write("appci_find token=wordabc x0=1000.0 y0=400.0 x1=1100.0 y1=440.0\n")
        ctx = types.SimpleNamespace(log_path=logp)
        ctx.log_since = lambda off: open(logp).read()
        row = D.U.hook_find(ctx, "wordabc", origin=(1600, 0), scale=1.0 / 2.0, since=0)
        assert row == ("wordabc", 2125, 210, 2100, 2150, 200, 220), row
        assert D.U.hook_find(ctx, "wordabc", origin=(10, 20), scale=1.0, since=0)[1:3] == (1060, 440), "X11: device pixels are pointer pixels"
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    res = "Xcursor.size:\t24\nXft.dpi:\t96\n*foreground: white\n"
    assert D.xrdb_with_dpi(res, 144) == "Xcursor.size:\t24\n*foreground: white\nXft.dpi:\t144\n"
    assert D.xrdb_with_dpi("", 120) == "Xft.dpi:\t120\n" and D.xrdb_with_dpi("Xft.dpi: 192\n", None) == "" and D.xrdb_with_dpi("Xft*dpi: 1\na: b\n", None) == "a: b\n"
    # cases_ui.Mouse caches the first output's size; the scale case must not
    assert "_out" not in D.LayoutMouse._wl_extent.__code__.co_names and issubclass(D.LayoutMouse, D.U.Mouse)


# ---------------------------------------------------------------- drag and drop
@check
def drag_source_and_drop_verdict():
    """dragsrc.py's payload is a uri-list with the space percent-encoded; the source is dragsrc.py when GTK loads, else dragon-drop, else none (the
    case then SKIPs with the packages named); it is pinned to the app's display system; the drop verdict accepts the path quoted or
    backslash-escaped as ONE argument and rejects a split path, a line with nothing inserted, and more than one line"""
    assert dragsrc.file_uri("/w/dnd file a.txt") == "file:///w/dnd%20file%20a.txt"
    assert dragsrc.uri_list(["/w/a b.txt", "/w/c.txt"]) == "file:///w/a%20b.txt\r\nfile:///w/c.txt\r\n"
    assert dragsrc.parse_at("12,34") == (12, 34) and dragsrc.parse_at("x") == (0, 0) and dragsrc.parse_at(None) == (0, 0)
    assert dragsrc.APP_ID == "appci_dragsrc" and dragsrc.W > 100 and dragsrc.H > 60
    argv, kind = D.drag_source_argv(True, "/usr/bin/dragon-drop", "/r/x.ready", (30, 40), ["/w/dnd file a.txt"])
    assert argv[1].endswith("dragsrc.py") and argv[2:6] == ["--ready", "/r/x.ready", "--at", "30,40"] and argv[-1] == "/w/dnd file a.txt" and kind.startswith("dragsrc")
    assert D.drag_source_argv(False, "/usr/bin/dragon-drop", "r", (0, 0), ["p"]) == (["/usr/bin/dragon-drop", "p"], "dragon-drop")
    assert D.drag_source_argv(False, None, "r", (0, 0), ["p"]) == (None, None)
    assert D.gdk_backend("x11") == "x11" and D.gdk_backend("wayland") == "wayland"
    text = '{"pid": 5, "w": 220, "h": 120, "backend": "X11Display", "at": [20, 20]}\n{"event": "begin"}\n{"event": "data"}\n{"event": "end"}\n{"ev'
    first, events = D.drag_events(text)
    assert first["pid"] == 5 and events == ["begin", "data", "end"] and D.drag_events("") == (None, []) and D.drag_events(None) == (None, [])
    path, out = "/h/w/dnd file ab12.txt", "dnd_out_ab12.txt"
    for line in ("ls -d '/h/w/dnd file ab12.txt' > dnd_out_ab12.txt", "ls -d /h/w/dnd\\ file\\ ab12.txt  > dnd_out_ab12.txt", 'ls -d "/h/w/dnd file ab12.txt" > dnd_out_ab12.txt'):
        assert D.drop_line_verdict([line], path, out) is None, line
    assert "ONE argument" in D.drop_line_verdict(["ls -d /h/w/dnd file ab12.txt > dnd_out_ab12.txt"], path, out)
    assert "nothing was inserted" in D.drop_line_verdict(["ls -d  > dnd_out_ab12.txt"], path, out)
    assert "0 line(s)" in D.drop_line_verdict([], path, out) and "2 line(s)" in D.drop_line_verdict(["a", "b"], path, out)
    assert "does not parse" in D.drop_line_verdict(["ls -d '/h/w/dnd file ab12.txt > x"], path, out)
    assert D.drop_line_verdict(["ls -d file:///h/w/dnd%20file%20ab12.txt > dnd_out_ab12.txt"], path, out), "a URI is not the path"
    src = open(os.path.join(HERE, "cases_desk.py"), encoding="utf-8").read()
    assert D.WAYLAND_DROP_ISSUE == "#2085" and "m.move(tx, ty)" in src, "the pointer is parked on the target BEFORE the drag: the app drops at its last pointer position"


# ---------------------------------------------------------------- IME
@check
def ime_plan_and_launcher_lines():
    """linux_ime_plan: fcitx5 with its Pinyin engine serves both display systems; ibus only X11 (and only with an engine installed); nothing usable
    gives the NEEDS text with the package names. The session lines export the input method's environment for the app, publish the bus address
    and start the daemon NOT daemonized (it must stay in the launcher's process group). The Windows precondition reads the user's language list."""
    have = lambda *names: (lambda n: "/usr/bin/" + n if n in names else None)       # noqa: E731
    files = lambda *paths: (lambda p: p in paths)                                    # noqa: E731
    fc = D.linux_ime_plan("wayland", have("fcitx5", "fcitx5-remote"), files(D.FCITX_PINYIN_FILES[0]))
    assert fc == dict(framework="fcitx5", engine="pinyin", plain="keyboard-us", xmodifiers="@im=fcitx")
    assert D.linux_ime_plan("x11", have("fcitx5", "fcitx5-remote", "ibus", "ibus-daemon"), files(D.FCITX_PINYIN_FILES[0]))["framework"] == "fcitx5"
    assert D.linux_ime_plan("wayland", have("fcitx5", "fcitx5-remote"), files()) == D.LINUX_IME_NEEDS, "fcitx5 without the Pinyin addon"
    ib = D.linux_ime_plan("x11", have("ibus", "ibus-daemon"), files(D.IBUS_ENGINES[0][1]))
    assert ib["framework"] == "ibus" and ib["engine"] == "libpinyin" and ib["xmodifiers"] == "@im=ibus"
    assert D.linux_ime_plan("x11", have("ibus", "ibus-daemon"), files(D.IBUS_ENGINES[1][1]))["engine"] == "pinyin"
    assert D.linux_ime_plan("wayland", have("ibus", "ibus-daemon"), files(D.IBUS_ENGINES[0][1])) == D.LINUX_IME_NEEDS, "ibus is not offered on Wayland"
    assert D.linux_ime_plan("x11", have("ibus", "ibus-daemon"), files()) == D.LINUX_IME_NEEDS and D.linux_ime_plan("x11", have(), files()) == D.LINUX_IME_NEEDS
    prof = D.fcitx_profile("pinyin")
    assert "DefaultIM=pinyin" in prof and prof.index("Name=keyboard-us") < prof.index("[Groups/0/Items/1]\nName=pinyin")
    for plan, daemon, env in ((fc, "fcitx5 >> /r/ime.log 2>&1 &", "GTK_IM_MODULE=fcitx"), (ib, "ibus-daemon -rx >> /r/ime.log 2>&1 &", "GTK_IM_MODULE=ibus")):
        pre = D.ime_pre_script(plan, "/r/addr", "/r/ime.log")
        assert daemon in pre and env in pre and "XMODIFIERS=%s" % plan["xmodifiers"] in pre and '"$DBUS_SESSION_BUS_ADDRESS" > /r/addr' in pre, pre
        assert " -d" not in pre, "a daemonized input method leaves the launcher's process group and would outlive the job"
        assert subprocess.run(["sh", "-n"], input=pre.encode(), capture_output=True).returncode == 0
    assert D.ime_switch_argv(fc, "pinyin") == (["fcitx5-remote", "-s", "pinyin"], ["fcitx5-remote", "-n"])
    assert D.ime_switch_argv(ib, "xkb:us::eng") == (["ibus", "engine", "xkb:us::eng"], ["ibus", "engine"])
    assert D.win_pinyin_installed([{"tag": "en-US", "tips": ["0409:00000409"]}, {"tag": "zh-Hans-CN", "tips": ["0804:{81D4E9C9-1D3B-41BC-9E6C-4B40BF79E35E}{FA550B04-5AD7-411F-A5AC-CA038EC515D7}"]}])
    assert not D.win_pinyin_installed([{"tag": "en-US", "tips": ["0409:00000409"]}]) and not D.win_pinyin_installed([{"tag": "zh-Hans-CN", "tips": []}])
    assert not D.win_pinyin_installed([]) and not D.win_pinyin_installed(None) and not D.win_pinyin_installed([None])
    assert D.PINYIN == (("nihao", "你好"), ("zhongwen", "中文")) and D.WinIme.LANG == 0x0804
    # the two classes are what cases_caseintl.compose_raw / compose_editor take as `sw`
    for cls in (D.LinuxIme, D.WinIme):
        assert callable(getattr(cls, "abc")) and callable(getattr(cls, "ime")) and callable(getattr(cls, "restore")), cls


def main():
    failed = 0
    for f in CHECKS:
        try:
            f()
            print("ok   %s" % f.__name__)
        except Exception as e:
            failed += 1
            print("FAIL %s: %s: %s" % (f.__name__, type(e).__name__, e))
    print("selftest_desk: %d checks, %d failed" % (len(CHECKS), failed))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
