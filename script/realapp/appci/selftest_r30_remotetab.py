# appci/selftest_r30_remotetab.py: offline self-tests of the #2196 step of lab_tab_remote_dir (r30): a zsh lab user WITH compinit (zshc), the in-band answer
# the app logs per shell, the Tab press time taken from the injector, and the fish / nu / zsh facts the fix rests on. Pure Python, no display.
import os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


def _root(*parts):
    return os.path.abspath(os.path.join(HERE, "..", "..", "..", *parts))


def _src(*parts):
    return open(_root(*parts), encoding="utf-8").read()


def _usersdb():
    import importlib.util
    path = _root("script", "realapp", "sshlab", "rootfs", "usr", "local", "lib", "sshlab", "usersdb.py")
    spec = importlib.util.spec_from_file_location("usersdb_r30", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _labd():
    import importlib.util
    path = _root("script", "realapp", "sshlab", "rootfs", "usr", "local", "sbin", "labd.py")
    lib = _root("script", "realapp", "sshlab", "rootfs", "usr", "local", "lib", "sshlab")
    if lib not in sys.path:
        sys.path.insert(0, lib)
    spec = importlib.util.spec_from_file_location("labd_r30", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


@check
def r30_lab_has_a_zsh_user_with_compinit_2196():
    """the lab's zsh users never run compinit (#2174: the app never runs it for them), so their in-band answer is empty by design; the new `zshc` user is a
    zsh login whose .zshrc sources the same hook and THEN runs compinit, and keeps its own hook file name (the hook is the zsh one)"""
    U, D = _usersdb(), _labd()
    rows = {r.user: r for r in U.load()}
    assert "u_zshc" in rows and rows["u_zshc"].shell == "zshc" and rows["u_zshc"].profile == "default", rows.get("u_zshc")
    assert U.SHELLS["zshc"] == U.SHELLS["zsh"] == "/usr/bin/zsh"
    assert "/usr/bin/zsh" in U.etc_shells(list(rows.values()))
    plain = D.rc_plan(rows["u_zsh"])[".zshrc"]
    assert "compinit" not in plain, "the existing no-compinit arm stays exactly that: %r" % plain
    rc = D.rc_plan(rows["u_zshc"])[".zshrc"]
    assert "hook.zsh.u_zshc" in rc and "compinit" in rc and rc.index("hook.zsh.u_zshc") < rc.index("compinit"), rc
    assert "autoload -Uz compinit" in rc
    assert D.hook_shell_key("zshc") == "zsh" and D.hook_shell_key("zsh") == "zsh" and D.hook_shell_key("dash") == "sh"
    assert D.completer_key("zshc") == "zsh" and D.completer_key("rbash") == "bash" and D.completer_key("fish") == "fish"
    ctl = _src("script", "realapp", "sshlab", "labctl.py")
    assert '"zsh", "zshc"' in ctl or '"zshc"' in ctl, "labctl's per-user complete check lists the new shell"


@check
def r30_lab_tab_remote_dir_matrix_has_the_zshc_arms_2196():
    """lab_tab_remote_dir has a zshc arm on every latency and both helper modes (these are the arms that prove the zsh in-band answer); no other case grew one,
    and the quick matrix runs it"""
    import cases  # noqa: F401
    import lab as L
    names = {c["name"] for c in cases.CASES if c.get("lab")}
    for lat in ("lan", "150", "300"):
        for h in ("nohelper", "helper"):
            assert "lab_tab_remote_dir.zshc.%s.%s" % (lat, h) in names, (lat, h)
    assert "zshc" in L.MATRIX_SHELLS and "zshc" not in L.ALL_SHELLS and "zshc" in L.matrix_selection("quick")["shells"]
    assert L.matrix_selection("shells=zshc;lat=lan")["shells"] == {"zshc"}
    assert not [n for n in names if ".zshc." in n and not n.startswith("lab_tab_remote_dir.")], "only lab_tab_remote_dir declares zshc"
    arm = next(c["arm"] for c in cases.CASES if c["name"] == "lab_tab_remote_dir.zshc.lan.nohelper")
    assert arm["shell"] == "zshc" and arm["user"] == "u_zshc" and arm["fault"] is None


@check
def r30_native_tab_expectation_per_shell_2196():
    """what the app's log must say about the unique-prefix Tab of a no-helper arm: bash, zsh with compinit and fish ANSWER in-band; zsh without compinit
    answers EMPTY by design (#2174); nu stays on the completer (no non-interactive completion API); a helper arm is not an in-band arm at all"""
    import cases  # noqa: F401
    import cases_lab as CL
    f = CL.native_tab_expectation
    for sh in ("bash", "zshc", "fish"):
        assert f(dict(shell=sh, helper="nohelper")) == "answers", sh
    assert f(dict(shell="zsh", helper="nohelper")) == "empty"
    assert f(dict(shell="nu", helper="nohelper")) == "completer"
    for sh in ("bash", "zsh", "zshc", "fish", "nu"):
        assert f(dict(shell=sh, helper="helper")) is None, sh
    assert f(dict(shell="bash", helper="nohelper", scenario=True)) is None, "a scenario user is bash with its own rc: not judged"


@check
def r30_native_tab_lines_are_read_from_the_app_log_2196():
    """the app's 'N candidates from the remote shell in M ms' line (input.rs) gives the in-band time in ms; the refusal ('Native completions not started (why)')
    and the empty-answer line are told apart; other lines are not"""
    import cases  # noqa: F401
    import cases_lab as CL
    log = ("[INFO] Tab on remote session SessionId(3): native use Preferred (shell supports native: true)\n"
           "[INFO] Tab on remote session SessionId(3) without the helper: 2 candidates from the remote shell in 163 ms\n"
           "[INFO] Native completions not started (a command is executing); the Tab uses the completer\n"
           "[INFO] The remote shell had no Tab answer; the completer answers instead\n"
           "[INFO] Tab on remote session SessionId(3) without the helper: x candidates from the remote shell in 5 ms\n")
    assert CL.native_tab_ms(log) == [163]
    assert CL.native_tab_refusals(log) == ["a command is executing"]
    assert CL.native_tab_empty(log) == 1
    assert CL.native_tab_ms("") == [] and CL.native_tab_ms(None) == [] and CL.native_tab_empty(None) == 0
    inp = _src("app", "src", "terminal", "input.rs")
    assert "candidates from the remote shell in {} ms" in inp and "The remote shell had no Tab answer; the completer answers instead" in inp
    pty = _src("app", "src", "terminal", "writeable_pty", "pty_controller.rs")
    assert "Native completions not started ({why}); the Tab uses the completer" in pty


@check
def r30_tab_press_time_is_the_injectors_post_time_2196():
    """macOS: key('Tab') runs htype2 as a process and returns after it exits, so a clock started before the call carried the process start-up and the key
    gap (the r29a bash.lan.nohelper 303 ms on a LAN with no remote_tab_wait line). The injector's own post time of the Tab is the press; without one (X11, a
    backend with no record) the clock before the call is kept"""
    import cases  # noqa: F401
    import cases_lab as CL

    class Be:
        calls = [dict(wall0=100.0, wall1=100.4, post_wall=[100.31])]

    assert CL.tab_press_wall(Be(), 100.0, 100.4) == 100.31
    assert CL.tab_press_wall(Be(), 100.0, 100.2) == 100.2, "a post time after the call returned is not believable: clamp"
    assert CL.tab_press_wall(Be(), 100.35, 100.4) == 100.35, "a post time before the call began is another call's: clamp"

    class Empty:
        calls = [dict(wall0=100.0, wall1=100.4, post_wall=[])]

    assert CL.tab_press_wall(Empty(), 100.0, 100.4) == 100.0
    assert CL.tab_press_wall(object(), 100.0, 100.4) == 100.0
    body = open(os.path.join(HERE, "cases_lab.py"), encoding="utf-8").read()
    assert "tab_press_wall(ctx.be, t0, t_sent)" in body


@check
def r30_case_checks_the_in_band_answer_and_says_why_not_2196():
    """c_tab_remote_dir turns the per-shell expectation into a verdict on the unique-prefix Tab: a native answer line (judged), or the refusal reason named
    (unverifiable, never a pass), or for the no-compinit zsh arm the empty-answer line; and it records the app's own in-band ms"""
    import cases  # noqa: F401
    import cases_lab as CL
    import inspect
    body = inspect.getsource(CL.c_tab_remote_dir)
    assert "native_tab_expectation(arm)" in body and "native_tab_ms(" in body and "native_tab_refusals(" in body and "native_tab_empty(" in body
    assert "r.metric(\"tab_native_ms\"" in body


@check
def r30_fish_bootstrap_answers_the_in_band_exchange_2196():
    """fish.sh binds ^Y and RS, advertises the protocol in its Bootstrapped hook and answers with `complete -C` in the common wire format; the app counts fish
    among the shells that answer; nu and PowerShell stay off (nu has no non-interactive completion API to call from its bootstrap)"""
    fish = _src("app", "assets", "bundled", "bootstrap", "fish.sh")
    assert "bind \\cy __warp_native_complete" in fish and "complete -C" in fish
    assert "!phosphor_nc_frames_v1" in fish and "shell_options" in fish
    assert "9280;A;incrementally_typed" in fish and "9280;C;" in fish and "9280;B" in fish and "9280;P;" in fish
    mod = _src("crates", "warp_terminal", "src", "shell", "mod.rs")
    assert "matches!(self, ShellType::Zsh | ShellType::Bash | ShellType::Fish)" in mod
    doc = _src("docs", "design", "r10-remote.md")
    assert "#2196" in doc and "nu stays on the app's completer" in doc and "--ide-complete" in doc


@check
def r1918_ext_relaunch_settle_waits_for_the_new_instances_view_mode_hop():
    """#1918: set_ext_mode's relaunch no longer sleeps a fixed 4 s: it waits for a `hop=view_mode` line logged AFTER the relaunch (not the dead instance's),
    keeps the 4 s as the give-up net, and the app is launched after t_launch is taken"""
    src = _src("script", "realapp", "appci", "cases_caseremote.py")
    assert "time.sleep(4)" not in src, "the fixed first-run settle must stay gone"
    i = src.index("t_launch = time.time()")
    body = src[i:i + 1200]
    assert body.index("ctx.app.start()") > 0 and "ctx.app_log_window(t_launch)" in body and '"hop=view_mode"' in body
    assert "4.0, step=0.2" in body, "the give-up net stays 4 s"
