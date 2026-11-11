# appci/selftest_r15_w5a.py: offline self-tests of round 15 wave 5 part A (cases_r15_w5a.py, nettrace.py and the round's changes to cases_netless.py).
# Pure Python plus the tools this machine happens to have (lsof, unshare): no app, display or network use beyond this machine's own addresses.
# selftest.py appends CHECKS to its own list, so `run.py --self-test` and `selftest.py` both run these.
import inspect, os, shutil, sys, tempfile, types

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, HERE)
import cases, winmap  # noqa: E402
import app as A  # noqa: E402
import backend  # noqa: E402
import cases_agent_netgate as NG  # noqa: E402
import cases_netless as NL  # noqa: E402
import cases_r15_w5a as W  # noqa: E402
import cases_robust as R  # noqa: E402
import nettrace as N  # noqa: E402

CHECKS = []
NEW = ("keychain_startup_macos", "keybinding_rebind", "editor_daily")


def check(f):
    CHECKS.append(f)
    return f


def _src(*parts):
    return open(os.path.join(ROOT, *parts), encoding="utf-8").read()


@check
def r15_w5a_registry_and_windows_classification():
    """the three new cases are registered once, in PROVING only; every case this round touched has a Windows classification whose reason names what
    is missing (NEEDS) or says it is by design; docs/testing.md describes each"""
    reg = {c["name"]: c for c in cases.CASES}
    for n in NEW:
        assert n in reg and n in cases.PROVING and n not in cases.SMOKE and n not in cases.PROVING_FAKE, n
        assert reg[n]["gui"], n
    assert reg["keychain_startup_macos"]["modes"] == ("terminal",) and reg["keybinding_rebind"]["modes"] == ("terminal", "agent") == reg["editor_daily"]["modes"]
    for n in ("keybinding_rebind", "editor_daily"):
        assert winmap.skip_reason(n, "powershell") is None and winmap.skip_reason(n, "gitbash") is None, n
    for n in ("zero_network_startup", "network_gate_switch"):
        assert n in cases.PROVING and winmap.skip_reason(n, "gitbash") is None and "Git Bash pane" in winmap.skip_reason(n, "powershell"), n
        assert n not in winmap.NOT_WINDOWS, n
    for n in ("claude_plugin_offline", "keyring_unavailable_startup", "keychain_startup_macos"):
        assert "by design" in winmap.skip_reason(n, "gitbash"), n
    for n in ("agent_plugins_offline", "agent_gate_deny_allow", "network_gate_notice", "agent_gate_off_live", "agent_gate_key_never_leaves"):
        assert "NEEDS" in winmap.skip_reason(n, "gitbash") and "NEEDS" in winmap.skip_reason(n, "powershell"), n
    assert not winmap.unclassified(sorted(reg))
    doc = _src("docs", "testing.md")
    for n in NEW:
        assert ("`%s`" % n) in doc, "docs/testing.md does not describe %s" % n
    assert "Round 15 wave 5 part A" in doc and "nettrace.py" in doc and "selftest_r15_w5a.py" in doc
    # a Skip after a failed check would hide the failure: the new cases skip only before their first check
    for fn in (W.c_keybinding_rebind, W.c_editor_daily, W.c_keychain_startup_macos):
        src = inspect.getsource(fn)
        first = min(i for i in (src.find("r.check("), src.find("launch("), src.find("_arm_replace("), src.find("k.press("), len(src)) if i >= 0)
        assert "r.skip(" not in src[first:] and "raise C.Skip" not in src[first:], fn.__name__
    for fn in (W._arm_replace, W._arm_close_question, W._arm_external, W.close_editor, W.open_editor, W._launch_with, W.Keys.press):
        assert "r.skip(" not in inspect.getsource(fn), fn.__name__


@check
def r15_nettrace_parsers():
    """T41: the two connection-table parsers, the process tree and the 'is this outbound' rule"""
    lsof = ("COMMAND   PID USER   FD   TYPE             DEVICE SIZE/OFF NODE NAME\n"
            "phosphor  1234 user   23u  IPv4 0x1a2b      0t0  TCP 192.168.1.5:52100->93.184.216.34:443 (ESTABLISHED)\n"
            "phosphor  1234 user   24u  IPv6 0x3c4d      0t0  UDP [fe80::1%en0]:55555->[fe80::2%en0]:5353\n"
            "phosphor  1234 user   25u  IPv4 0x5e6f      0t0  TCP *:8080 (LISTEN)\n"
            "zsh       1300 user   26u  IPv6 0x5e70      0t0  TCP [::1]:5000->[::1]:6000 (ESTABLISHED)\n"
            "phosphor  1234 user   27u  IPv4 0x5e71      0t0  TCP 192.168.1.5:52101->192.0.2.1:9 (SYN_SENT)\n"
            "phosphor  1234 user   28u  IPv6 0x5e72      0t0  TCP [2001:db8::5]:52102->[2001:db8::9]:443 (ESTABLISHED)\n")
    rows = N.parse_lsof(lsof)
    assert rows == [(1234, "TCP", "93.184.216.34", 443, "ESTABLISHED"), (1234, "UDP", "fe80::2", 5353, ""), (1300, "TCP", "::1", 6000, "ESTABLISHED"),
                    (1234, "TCP", "192.0.2.1", 9, "SYN_SENT"), (1234, "TCP", "2001:db8::9", 443, "ESTABLISHED")], rows
    assert [r[2] for r in N.outbound(rows)] == ["93.184.216.34", "192.0.2.1", "2001:db8::9"], "loopback and link-local are not outbound; a SYN that was never answered is"
    assert [r[2] for r in N.outbound(rows, allow={"192.0.2.1"})] == ["93.184.216.34", "2001:db8::9"]
    assert N.parse_lsof("") == [] and N.parse_lsof("lsof: no process\n") == []
    netstat = ("\nActive Connections\n\n  Proto  Local Address          Foreign Address        State           PID\n"
               "  TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       1040\n"
               "  TCP    10.0.2.15:49712        93.184.216.34:443      ESTABLISHED     5120\n"
               "  TCP    [::1]:49670            [::1]:49671            ESTABLISHED     900\n"
               "  TCP    [::]:445               [::]:0                 ABHÖREN         4\n"
               "  UDP    0.0.0.0:5353           *:*                                    2040\n"
               "  TCP    127.0.0.1:50000        127.0.0.1:50001        ESTABLISHED     5120\n"
               "  TCP    10.0.2.15:49713        192.0.2.1:9            SYN_SENT        5121\n")
    rows = N.parse_netstat(netstat)
    assert rows == [(5120, "TCP", "93.184.216.34", 443, "ESTABLISHED"), (900, "TCP", "::1", 49671, "ESTABLISHED"), (5120, "TCP", "127.0.0.1", 50001, "ESTABLISHED"),
                    (5121, "TCP", "192.0.2.1", 9, "SYN_SENT")], rows
    assert "LISTENING" not in repr(rows) and len(N.outbound(rows)) == 2, "a listening socket has no remote end, whatever the state word is in the system's language"
    assert N.split_host_port("[fe80::1%en0]:5353") == ("fe80::1", 5353) and N.split_host_port("*:*") == ("*", None) and N.split_host_port("10.0.0.1:22") == ("10.0.0.1", 22)
    for a in ("127.0.0.1", "127.9.9.9", "::1", "0.0.0.0", "::", "*", "::ffff:127.0.0.1", "fe80::2"):
        assert N.is_local(a), a
    for a in ("10.0.0.1", "192.0.2.1", "2001:db8::1", "::ffff:10.0.0.1", "1.2.3.4"):
        assert not N.is_local(a), a
    tree = N.parse_ps_tree("  10     1\n  11    10\n  12    11\n  20     1\n junk\n  30    20\n")
    assert tree == {10: 1, 11: 10, 12: 11, 20: 1, 30: 20}
    assert N.descendants(tree, [10]) == {10, 11, 12} and N.descendants(tree, [10, 20]) == {10, 11, 12, 20, 30} and N.descendants(tree, [None]) == set()
    assert N.sample_mac(set()) == [] and N.sample_windows(set()) == [], "no pids: nothing is asked of the tool"
    mac_, win, lin = types.SimpleNamespace(win=False, be=types.SimpleNamespace(name="mac")), types.SimpleNamespace(win=True, be=types.SimpleNamespace(name="windows")), \
        types.SimpleNamespace(win=False, be=types.SimpleNamespace(name="wayland"))
    assert (N.platform_kind(mac_), N.platform_kind(win), N.platform_kind(lin)) == ("mac", "windows", None)


@check
def r15_nettrace_control_and_poller():
    """T41: a tool that lists nothing fails the control; a machine without an address cannot run it; where lsof exists the control passes for real;
    the poller keeps first-seen times and the verdict splits launch + idle from the session"""
    assert N.control(lambda pids: [], hold_s=1.0)[0] is False, "a sampler that lists nothing must fail the control"
    assert N.control(lambda pids: [(next(iter(pids)), "TCP", "203.0.113.9", 1, "")], hold_s=1.0)[0] is False, "the listed row must be the control's own address and port"
    keep = N.own_address
    try:
        N.own_address = lambda: None
        ok, why = N.control(lambda pids: [], hold_s=0.5)
        assert ok is None and "no non-loopback address" in why
    finally:
        N.own_address = keep
    if shutil.which("lsof") and N.own_address():
        ok, why = N.control(N.sample_mac, hold_s=4.0)
        assert ok is True, "lsof is on this machine but the control connection was not listed: %s" % why
    feed = [[(7, "TCP", "127.0.0.1", 80, "")], [(7, "TCP", "198.51.100.7", 443, ""), (8, "UDP", "198.51.100.8", 53, "")], [(7, "TCP", "198.51.100.7", 443, "")],
            [(9, "TCP", "198.51.100.9", 22, "")]]
    asked = []
    p = N.ConnPoller(lambda pids: (asked.append(set(pids)) or feed.pop(0)) if feed else [], lambda: {7, 8, 9})
    import time as _t
    t0 = _t.time()
    p.tick()
    p.tick()
    t_mid = _t.time() + 0.001
    _t.sleep(0.01)
    p.tick()
    p.tick()
    t1 = _t.time() + 1
    assert p.ticks == 4 and p.errors == 0 and asked[0] == {7, 8, 9}
    idle, sess = p.outbound_between(t0, t_mid), p.outbound_between(t_mid, t1)
    assert sorted(r[:3] for r in idle) == [("TCP", "198.51.100.7", 443), ("UDP", "198.51.100.8", 53)] and [r[:3] for r in sess] == [("TCP", "198.51.100.9", 22)], (idle, sess)
    assert p.outbound_between(t_mid, t1, allow={"198.51.100.9"}) == [], "the session allowlist (the fake ssh host) is not outbound"
    bad, prov = NL.polled_verdict(idle, sess, set())
    assert sorted(b[3] for b in bad) == ["idle", "idle", "session"] and prov == []
    bad, prov = NL.polled_verdict([], sess, {"198.51.100.9"})
    assert bad == [] and len(prov) == 1, "the configured model endpoint is allowed in the session"
    bad, prov = NL.polled_verdict(sess, [], {"198.51.100.9"})
    assert len(bad) == 1 and bad[0][3] == "idle", "and NOT during launch and idle"
    boom = N.ConnPoller(lambda pids: 1 / 0, lambda: {1})
    boom.tick()
    assert boom.errors == 1 and boom.ticks == 0
    # the case: the control runs before the app is stopped, a failed control is a SKIP that says NEEDS, and the limits of a poll are stated in the result
    src = inspect.getsource(NL.zero_network_polled)
    assert src.index("nettrace.control(sampler)") < src.index("ctx.app.stop()") and 'raise cases.Skip("NEEDS:' in src and "POLL_LIMITS" in src
    assert "poller.ticks >= 4 and poller.errors == 0" in src
    top = inspect.getsource(NL.c_zero_network_startup)
    assert top.index("nettrace.platform_kind(ctx)") < top.index("R.need_linux_app"), "macOS and Windows take the polled path before the Linux-only guard"
    assert "DNS" in NL.POLL_LIMITS and "between two polls" in NL.POLL_LIMITS


@check
def r15_offline_prefix_is_proven():
    """#2077: the no-network prefix of the two plugin cases is only believed after a connect made inside it failed"""
    blocks, why = NL.prefix_blocks_network([])
    assert blocks is False and "SUCCEEDED" in why, (blocks, why)
    refuse = [sys.executable, "-I", "-c", "import sys; sys.exit(7)", "--"]
    assert NL.prefix_blocks_network(refuse)[0] is True
    other = [sys.executable, "-I", "-c", "import sys; sys.exit(3)", "--"]
    blocks, why = NL.prefix_blocks_network(other)
    assert blocks is False and "exit 3" in why
    real = NL.netns_prefix()
    if real:
        assert NL.prefix_blocks_network(real)[0] is True, "`%s` is permitted here but a connect inside it was not refused" % " ".join(real)
    if not shutil.which("sandbox-exec"):
        assert NL.netns_prefix("darwin") is None
    assert "deny network*" in NL.SANDBOX_NO_NET
    for fn in (NL.c_claude_plugin_offline, NL.c_agent_plugins_offline):
        s = inspect.getsource(fn)
        assert "offline_prefix(ctx, r," in s and "NO_WINDOWS_PLUGINS" in s and "need_linux_app" not in s, fn.__name__
    assert "NEEDS" in NL.NO_WINDOWS_PLUGINS
    s = inspect.getsource(NL.offline_prefix)
    assert s.count("r.no_reference(") == 2 and "prefix_blocks_network(prefix)" in s
    # the build's bundled plugins are looked for inside a macOS bundle too
    ctx = types.SimpleNamespace(app=types.SimpleNamespace(bin="/x/P.app/Contents/MacOS/phosphor", bundle="/x/P.app"))
    roots = NL.bundle_roots(ctx)
    assert "/x/P.app/Contents/Resources/bundled" in roots and roots[-1].endswith(os.path.join("resources", "bundled"))
    assert "cli_agent_sessions::event=info" in A.MAC_RUST_LOG_MORE, "the macOS launcher must log the line the plugin cases read"


@check
def r15_live_settings_paths():
    """#2077: what the running app reads is the profile's file on Linux and a copy elsewhere; the gate cases read the copy"""
    prof = types.SimpleNamespace(home="/h")
    lin = types.SimpleNamespace(win=False, be=types.SimpleNamespace(name="wayland"), prof=prof, app=types.SimpleNamespace())
    mac_ = types.SimpleNamespace(win=False, be=types.SimpleNamespace(name="mac"), prof=prof, app=types.SimpleNamespace(settings="/h/.phosphor/settings.toml"))
    win = types.SimpleNamespace(win=True, be=types.SimpleNamespace(name="windows"), prof=prof, app=types.SimpleNamespace(config=os.path.join("C:", "cfg")))
    assert R.live_settings_file(lin) == R.settings_file(lin) == "/h/.config/phosphor/settings.toml"
    assert R.live_settings_file(mac_) == "/h/.phosphor/settings.toml" and R.app_config_dir(mac_) == "/h/.phosphor"
    assert R.live_settings_file(win) == os.path.join("C:", "cfg", "settings.toml")
    assert W.keybindings_path(mac_) == "/h/.phosphor/keybindings.yaml" and W.keybindings_path(lin) == "/h/.config/phosphor/keybindings.yaml"
    assert "R.live_settings_file(ctx)" in inspect.getsource(NG.origin_count)
    s = inspect.getsource(NG.open_network_kb)
    assert 'CB.SETTINGS_KEY["mac" if ctx.be.name == "mac" else "linux"]' in s
    s = inspect.getsource(NL.c_network_gate_switch)
    assert "R.need_relaunch_app(ctx" in s and "shell_back_in_front(ctx, r" in s and "r.unverifiable(" in s and 'R.start_app(ctx, r, "ngs_restore")' in s
    assert "NEEDS" in NG.NOT_PORTED_WHY


@check
def r15_keybinding_rebind_parts():
    """T45: the file text, the tab verdict, the frame evidence, and that the app really has the names and no default on the test chords"""
    assert W.keybindings_yaml([("workspace:new_tab", "ctrl-shift-f8")]) == '"workspace:new_tab": ctrl-shift-f8\n'
    assert W.keybindings_yaml([(W.CMD_SEARCH_ACTION, W.MOVED[0]), (W.NEW_TAB_ACTION, W.CTRL_R[0])]) == '"workspace:show_command_search": ctrl-shift-f9\n"workspace:new_tab": ctrl-r\n'
    assert W.tab_delta_verdict(2, 3, 1, "x") is None and W.tab_delta_verdict(2, 2, 0, "x") is None
    assert "expected one more" in W.tab_delta_verdict(2, 2, 1, "x") and "expected no change" in W.tab_delta_verdict(2, 3, 0, "x")
    assert W.tab_delta_verdict(None, 3, 1, "x") is None and W.tab_delta_verdict(2, None, 1, "x") is None, "no count: not this oracle's verdict"
    log = 'a frame 12: updated=["TerminalView", "CommandSearchView"]\n'
    assert W.view_rendered(log, "CommandSearchView") is True and W.view_rendered('frame 3: updated=["TerminalView"]\n', "CommandSearchView") is False
    assert W.view_rendered("nothing\n", "CommandSearchView") is None
    # the app: the two binding names, the view name, the tab hop, launch-only loading
    assert 'const NEW_TAB_BINDING_NAME: &str = "workspace:new_tab";' in _src("app", "src", "workspace", "view.rs")
    assert "NEW_TAB_BINDING_NAME," in _src("app", "src", "workspace", "mod.rs") and "CustomAction::NewTab" in _src("app", "src", "workspace", "mod.rs")
    assert '"workspace:show_command_search"' in _src("app", "src", "terminal", "input.rs")
    assert '"CommandSearchView"' in _src("app", "src", "search", "command_search", "view.rs")
    assert "pub fn load_custom_keybindings" in _src("app", "src", "keyboard.rs")
    # no default binding on the test chords: no Rust source under app/src or crates binds f8 or f9 with ctrl-shift
    import re
    hits = []
    for base in (os.path.join(ROOT, "app", "src"), os.path.join(ROOT, "crates")):
        for d, _dirs, files in os.walk(base):
            for f in files:
                if f.endswith(".rs"):
                    t = open(os.path.join(d, f), encoding="utf-8", errors="replace").read()
                    if re.search(r'"(?:cmdorctrl|ctrl|cmd)-shift-f[89]"|"shift-(?:cmdorctrl|ctrl|cmd)-f[89]"', t, re.I):
                        hits.append(f)
    assert not hits, "a default binding now uses a test chord of keybinding_rebind: %s" % hits
    # every backend can send the chords
    assert backend.MacBackend.SPECIAL["f8"] == 100 and backend.MacBackend.SPECIAL["f9"] == 101 and backend.MacBackend.SPECIAL["end"] == 119
    assert backend.x11_keyname("F8") == "F8" and backend.split_key(W.REBOUND[1]) == (["ctrl", "shift"], "F8")
    wb = backend.WindowsBackend.__new__(backend.WindowsBackend)
    wb.w = types.SimpleNamespace(user32=types.SimpleNamespace(MapVirtualKeyW=lambda vk, kind: 0))
    for spec in (W.REBOUND[1], W.MOVED[1], "ctrl+alt+r", "ctrl+alt+Return", "ctrl+comma", "ctrl+shift+f", "ctrl+Home"):
        down, up, vk = backend.WindowsBackend._chord(wb, spec)
        assert len(down) >= 2 and len(up) == len(down), spec
    mb = backend.MacBackend.__new__(backend.MacBackend)
    for spec in (W.REBOUND[1], "cmd+alt+r", "cmd+alt+Return", "cmd+f", "cmd+s", "cmd+up", "End", "BackSpace"):
        backend.MacBackend._kspec(mb, spec)
    s = inspect.getsource(W.c_keybinding_rebind)
    assert s.index('"control: the default new-tab chord') < s.index("text_a = ") < s.index("text_b = "), "the control runs before either arm"
    assert "_same_bytes(r, path, text_a" in s and "_same_bytes(r, path, text_b" in s and "R.fresh_app(ctx, r" in s


@check
def r15_editor_daily_parts():
    """T50: the fixture, the one-match verdict, the trace reader, the keys, and the app strings the case reads (#2087, #2088)"""
    q, rep = "qabc1234", "Rep_X_longer_than_the_query"
    text, n = W.replace_fixture(q)
    assert text.count(q) == n == 4 and "qabc123 is shorter" in text and "qab c1234 has a space" in text and "日本" in text and text.endswith("last %s\n" % q)
    assert len(text.encode("utf-8")) > len(text), "multi-byte text sits before the first match"
    parts = text.split(q)
    for i in range(n):
        one = q.join(parts[:i + 1]) + rep + q.join(parts[i + 1:])
        assert W.replaced_once(text, one, q, rep) == i
    assert W.replaced_once(text, text, q, rep) is None, "nothing replaced"
    assert W.replaced_once(text, text.replace(q, rep), q, rep) is None, "all replaced is not one"
    assert W.replaced_once(text, text.replace(q, rep, 2), q, rep) is None
    assert W.replaced_once(text, text.replace(q, rep, 1).replace("qabc123 is", "Rep is"), q, rep) is None, "a near-miss word was touched"
    assert W.replaced_once(text, text.replace(q, rep, 1) + " ", q, rep) is None, "one stray byte"
    log = ("x editor_disk outcome=reloaded external=true flagged=false\n"
           "y editor_disk outcome=conflict external=false flagged=false\n"
           "z editor_disk outcome=conflict external=true flagged=true\n"
           "old editor_disk outcome=conflict external=true\n")
    ev = W.disk_events(log)
    assert ev == [("reloaded", True, False), ("conflict", False, False), ("conflict", True, True), ("conflict", True, None)], ev
    assert W.external_verdict([("conflict", True, True)], 10)[0] == "flagged"
    assert W.external_verdict([("conflict", True, False)], 10)[0] == "not_flagged"
    assert W.external_verdict([("conflict", False, False)], 10)[0] == "no_trace", "the echo of the editor's own save is not the other program's change"
    assert W.external_verdict([("reloaded", True, False)], 10)[0] == "reloaded" and W.external_verdict([], 10)[0] == "no_trace"
    # the app's side of those strings
    gbm = _src("app", "src", "code", "global_buffer_model.rs")
    assert 'format!("editor_disk outcome={outcome} external={external} flagged={flagged}")' in gbm and 'target: "editor_disk"' in gbm
    assert "PHOSPHOR_EDITOR_DISK_CONFLICT" in gbm and "disk_changed_under_edits(file_id)" in _src("app", "src", "code", "local_code_editor.rs")
    assert W.CLOSE_WARNING_LINE in _src("app", "src", "pane_group", "mod.rs")
    find = _src("app", "src", "code", "editor", "find", "view.rs")
    assert 'TOGGLE_REPLACE_BINDING_NAME: &str = "find:toggle_replace"' in find and 'TOGGLE_REPLACE_KEY: &str = "alt-cmdorctrl-r"' in find
    assert 'REPLACE_ALL_BINDING_NAME: &str = "find:replace_all"' in find and 'REPLACE_ALL_KEY: &str = "alt-cmdorctrl-enter"' in find
    mac_, lin = types.SimpleNamespace(be=types.SimpleNamespace(name="mac")), types.SimpleNamespace(be=types.SimpleNamespace(name="wayland"))
    assert W.ekeys(mac_) == dict(save="cmd+s", top="cmd+up", find="cmd+f", toggle_replace="cmd+alt+r", replace_all="cmd+alt+Return")
    assert W.ekeys(lin) == dict(save="ctrl+s", top="ctrl+Home", find="ctrl+shift+f", toggle_replace="ctrl+alt+r", replace_all="ctrl+alt+Return")
    # the launchers let the lines through
    assert "editor_disk=debug" in A.INNER and "editor_disk=debug" in A.MAC_RUST_LOG_MORE
    assert A.rust_log_enabled(A.WINDOWS_RUST_LOG, "editor_disk", "debug") and A.rust_log_enabled(A.WINDOWS_RUST_LOG, "warp::pane_group", "info")
    assert not A.rust_log_enabled(A.WINDOWS_RUST_LOG, "warp::code::view", "info"), "the base level stays warn"
    # the field-clearing and the find-and-fill key order, with a recording backend
    sent = []
    be = types.SimpleNamespace(name="wayland", key=lambda spec, repeat=1, delay_ms=15: sent.append((spec, repeat)), type=lambda text, delay, enter=False: sent.append(("type", text, enter)))
    import random
    ctx = types.SimpleNamespace(be=be, rng=random.Random(1))
    keep = W.time.sleep
    W.time.sleep = lambda s: None
    try:
        W.find_and_fill(ctx, W.ekeys(ctx), "qq", "RR")
    finally:
        W.time.sleep = keep
    assert sent == [("ctrl+Home", 1), ("ctrl+shift+f", 1), ("End", 1), ("BackSpace", 8), ("type", "qq", False), ("ctrl+alt+r", 1), ("End", 1), ("BackSpace", 8),
                    ("type", "RR", False)], sent
    with tempfile.TemporaryDirectory() as d:
        p = os.path.join(d, "f.txt")
        open(p, "wb").write(b"old")
        W.replace_file(p, b"new bytes")
        assert open(p, "rb").read() == b"new bytes" and os.listdir(d) == ["f.txt"], "replaced by rename, nothing left behind"
    s = inspect.getsource(W._arm_external)
    assert s.index("ext1 = ") < s.index("ext2 = ") and 'state == "flagged"' in s and "untouched = R.disk(path) == ext2" in s
    s = inspect.getsource(W.c_editor_daily)
    assert "r.no_reference(NO_CLOSE_QUESTION_MAC)" in s and "NEEDS" in W.NO_CLOSE_QUESTION_MAC


@check
def r15_keychain_startup_parts():
    """T42: the tool's output, one launch's facts and verdicts per arm, and the prompt watcher"""
    assert W.parse_keychain_path('    "/Users/x/Library/Keychains/login.keychain-db"\n') == "/Users/x/Library/Keychains/login.keychain-db"
    assert W.parse_keychain_path("security: SecKeychainCopyDefault: A default keychain could not be found.\n") is None and W.parse_keychain_path("") is None
    warn = "[WARN] %s: A default keychain could not be found.. Stored credentials (API keys, proxy password) are unavailable until it can be.\n" % W.KEYCHAIN_WARN_LINE
    base = "startup timing: a=1ms total=640ms\n"
    notice = "x %s\n" % W.U.KEYRING_NOTICE_LINE
    f = W.keychain_launch_facts(base + warn + notice)
    assert (f["first_frame_ms"], f["keychain_warn"], f["notices"], f["secure_read_errors"]) == (640, 1, 1, 0)
    assert W.keychain_launch_verdicts(f, "none", 2000) == []
    assert any("exactly once per launch" in m for m in W.keychain_launch_verdicts(W.keychain_launch_facts(base + warn + warn + notice), "none", 2000))
    assert any("expected exactly once" in m for m in W.keychain_launch_verdicts(W.keychain_launch_facts(base + warn), "none", 2000))
    assert any("exactly once per launch" in m for m in W.keychain_launch_verdicts(W.keychain_launch_facts(base + notice), "none", 2000))
    err = "[ERROR] Failed to read API keys from secure storage: unknown\n[ERROR] Failed to read local control mode from secure storage: unknown\n"
    assert any("2 'Failed to read" in m for m in W.keychain_launch_verdicts(W.keychain_launch_facts(base + warn + notice + err), "none", 2000)), "#1977's two ERROR lines"
    ok = W.keychain_launch_facts(base)
    assert W.keychain_launch_verdicts(ok, "unlocked", 2000) == [] and W.keychain_launch_verdicts(ok, "locked", 2000, prompts=1) == [] == W.keychain_launch_verdicts(ok, "locked", 2000, prompts=0)
    assert any("came back 2 times" in m for m in W.keychain_launch_verdicts(ok, "locked", 2000, prompts=2))
    assert any("with a default keychain present" in m for m in W.keychain_launch_verdicts(W.keychain_launch_facts(base + warn), "unlocked", 2000))
    assert any("first frame after 2640 ms" in m for m in W.keychain_launch_verdicts(W.keychain_launch_facts("startup timing: total=2640ms\n"), "locked", 2000))
    assert any("never drawn" in m for m in W.keychain_launch_verdicts(W.keychain_launch_facts(""), "unlocked", 2000))
    assert W.KEYCHAIN_WARN_LINE in _src("crates", "warpui_extras", "src", "secure_storage", "mac.rs")

    class Clock:
        t = 0.0

    def run(windows):
        """windows: what count() answers, one per call (the last repeats)"""
        seq, cancels = list(windows), []
        clk = Clock()
        count = lambda: seq.pop(0) if len(seq) > 1 else seq[0]
        sleep = lambda s: setattr(clk, "t", clk.t + s)
        return W.watch_prompts(3.0, count=count, cancel=lambda: cancels.append(clk.t), sleep=sleep, now=lambda: clk.t), len(cancels)
    assert run([0]) == ((0, 0), 0), "no prompt"
    assert run([1, 0]) == ((1, 0), 1), "one prompt, cancelled, none after"
    assert run([1, 0, 0, 1, 0]) == ((2, 0), 2), "the prompt came back after the cancel: counted twice"
    assert run([1]) == ((1, 1), 1), "a prompt that cannot be cancelled is reported, the watch ends"
    s = inspect.getsource(W.c_keychain_startup_macos)
    assert s.index("home_redirects(priv, real, tok)") < s.index("create-keychain") and "NEEDS_HOME" in s and "NEEDS_AX" in s and "real_kc.view() != real_before" in s
    assert s.index("if not ax_ok:") < s.index('launch("locked")'), "the locked arm is never played without a way to dismiss its prompt"
    assert "NEEDS" in W.NEEDS_HOME and "NEEDS" in W.NEEDS_AX and 'ctx.be.name != "mac"' in s
    k = W.Keychain("/nonexistent-home")
    if not shutil.which("security"):
        assert k.default() is None and k.search_list() == [] and k.view() == (None, ()), "no tool: nothing is claimed"
