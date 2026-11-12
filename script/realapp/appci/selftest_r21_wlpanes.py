# appci/selftest_r21_wlpanes.py: offline self-tests of round 21, Wayland (docs/design/r21-wlpanes.md; #2143 #2144 #2145 #2146): the four test faults
# behind FAIL / NOREF rows of the round 15 targeted run, and the names the two app fixes are switched off by. Pure Python: no app, no display.
# selftest.py appends CHECKS to its own list (two lines at its end), so `run.py --self-test` runs them.
# Every judge is fed a right input AND the wrong ones it exists to catch: an oracle that cannot fail is not a test.
import inspect, os, sys, tempfile, types

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.join(HERE, "..", "..", "..")
sys.path.insert(0, HERE)

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


def _src(*parts):
    return open(os.path.join(REPO, *parts), encoding="utf-8").read()


def _kd(key, t, seq, ctrl="false", alt="false"):
    return "x [DEBUG] input_order keydown key=%s ctrl=%s alt=%s shift=false cmd=false t=%.3f seq=%d\n" % (key, ctrl, alt, t * 1000, seq)


@check
def r21_injacct_counts_a_quoted_punctuation_chord():
    """#2146: Alt-. sent to a program is logged as `keydown key="." ... alt=true` with NO typed_characters line. The key-name pattern of before
    round 21 (`(\\S+)\\b`) did not match it, so meta_and_word_keys read "1 of 761 injected key(s) never reached the app's key trace". Every form of
    the name the app writes is one key: bare, quoted letter, quoted punctuation, quoted space, the masked `<1 chars>`; a modifier is none; a
    keydown and its typed_characters are still one key."""
    import injacct
    import re
    d = tempfile.mkdtemp()
    log = os.path.join(d, "phosphor.log")
    lines = [_kd('"b"', 100.01, 1, alt="true"), _kd('"."', 100.02, 2, alt="true"), _kd('" "', 100.03, 3), _kd("home", 100.04, 4),
             _kd("<1 chars>", 100.05, 5), _kd("a", 100.06, 6), _kd("Shift", 100.07, 7), _kd('"/"', 100.08, 8, ctrl="true"),
             "x [DEBUG] input_order typed_characters text=\" \" t=%.3f seq=3\n" % 100030,
             "x [DEBUG] input_order typed_characters text=a t=%.3f seq=6\n" % 100060]
    open(log, "w").write("".join(lines))
    keys = injacct.app_keys(log, 99, 101)
    assert sorted(seq for _t, seq, _k in keys) == [1, 2, 3, 4, 5, 6, 8], keys          # 7 is the modifier
    names = {seq: k for _t, seq, k in keys}
    assert names[2] == '"."' and names[4] == "home" and names[5] == "<1 chars>", names
    # the pattern of before round 21 loses exactly the two quoted punctuation chords that type nothing (2 and 8); 3 survived only by its typed line
    old = re.compile(r"input_order (keydown key=(\S+)|typed_characters)\b.* t=(\d+(?:\.\d+)?) seq=(\d+)")
    old_seqs = {int(m.group(4)) for m in old.finditer("".join(lines))}
    assert 2 not in old_seqs and 8 not in old_seqs and {1, 3, 4, 5, 6} <= old_seqs, old_seqs
    # an Enter keydown is still an Enter, a typed-only line (no keydown) is still a key
    open(log, "w").write(_kd("enter", 100.01, 1) + "x input_order typed_characters text=q t=%.3f seq=2\n" % 100020)
    keys = injacct.app_keys(log, 99, 101)
    assert len(keys) == 2 and [k for _t, _s, k in keys if k and k.lower() in injacct._ENTER_NAMES] == ["enter"], keys


@check
def r21_shell_lineage_tells_a_nested_shell_from_another_tabs():
    """#2146: the depth check typed `exit` in the root shell of another tab (the only tab left) because its pid was not the recorded root's.
    shell_lineage reads the process table: a shell started from the root shell is 'nested'; a shell under the app that is not is 'other_pane';
    anything the table cannot place is 'unknown' (the behaviour of before)."""
    import cases
    # app 10 -> terminal server 20 -> root shell 100 -> nested 900 -> nested 901; other tab's shell 200 -> its nested 950; sshd 5 -> remote 700
    table = {10: 1, 20: 10, 100: 20, 900: 100, 901: 900, 200: 20, 950: 200, 5: 1, 700: 5}
    L = cases.shell_lineage
    assert L(900, 100, 10, table) == "nested" and L("901", "100", "10", table) == "nested"
    assert L(200, 100, 10, table) == "other_pane" and L(950, 100, 10, table) == "other_pane"
    assert L(700, 100, 10, table) == "unknown"                    # over ssh: not under the app
    assert L(4242, 100, 10, table) == "unknown"                   # not in the table
    assert L(200, 100, None, table) == "unknown"                  # no app pid: nothing to place it by
    assert L(None, 100, 10, table) == "unknown" and L("x", 100, 10, table) == "unknown"
    assert L(900, 100, 10, {}) == "unknown"
    loop = {900: 901, 901: 900}
    assert L(900, 100, 10, loop) == "unknown"                     # a cycle ends the walk
    # the root shell of the pass's own tab is gone, the throw-away tab's shell is in front: the round 15 case
    assert L(200, 100, 10, {10: 1, 20: 10, 200: 20}) == "other_pane"


@check
def r21_leave_nested_types_no_exit_in_another_tabs_shell():
    """#2146: leave_nested with the process table of the round 15 run returns 'other' and types nothing; with a real nested shell it still
    types `exit` until the root answers; without a process table (no app pid) it behaves as before."""
    import cases
    typed = []

    def ctx(pids, app_pid):
        it = iter(pids)
        return types.SimpleNamespace(prof=types.SimpleNamespace(prompts=lambda: 0), pane=None, win=False,
                                     be=types.SimpleNamespace(type=lambda t, *a, **k: typed.append(t)), shell_pid=lambda: next(it),
                                     until=lambda *a, **k: None, reset=lambda: None, wait_prompt_after_exit=lambda *a, **k: True,
                                     app=types.SimpleNamespace(pid=lambda: app_pid))
    real = cases.ppid_table
    try:
        cases.ppid_table = lambda: {10: 1, 20: 10, 200: 20}                         # root 100 is gone; 200 is the other tab's shell
        c = ctx(["200", "200"], 10)
        assert cases.Ctx.leave_nested(c, "100") == "other" and typed == [], typed
        assert "pid 200" in c.other_pane_note and "gone" in c.other_pane_note and "NOT typed" in c.other_pane_note, c.other_pane_note
        cases.ppid_table = lambda: {10: 1, 20: 10, 100: 20, 900: 100}               # 900 was started from the root shell
        typed.clear()
        c = ctx(["900", "900", "100"], 10)
        assert cases.Ctx.leave_nested(c, "100") == "root" and typed == ["exit"], typed
        cases.ppid_table = lambda: {}                                               # ps gave nothing: as before round 21
        typed.clear()
        c = ctx(["200", "200", "100"], 10)
        assert cases.Ctx.leave_nested(c, "100") == "root" and typed == ["exit"], typed
        # Windows: no process table is asked for
        cases.ppid_table = lambda: (_ for _ in ()).throw(AssertionError("ps asked for on Windows"))
        typed.clear()
        c = ctx(["200", "200", "100"], 10)
        c.win = True
        assert cases.Ctx.leave_nested(c, "100") == "root" and typed == ["exit"], typed
    finally:
        cases.ppid_table = real
    run_src = _src("script", "realapp", "appci", "run.py")
    assert 'if nest in ("nested", "none", "other"):' in run_src and "other_pane_note" in run_src


@check
def r21_cb_ui_action_latency_closes_only_the_throwaway_tab():
    """#2146 (round 25: the throw-away tab's own pid is recorded when it is opened; N tabs): throwaway_tab_step decides from the pass's pid, the
    throw-away tab's pid, the pid in front and the number of previous-tab keys sent; close_throwaway_tab sends the close key only with the throw-away
    shell in front. The model of the app's tab ring is in selftest_r25_harness_a.py (any number of leaked tabs)."""
    import cases_coverage_b as B
    import cases_robust as R
    S = B.throwaway_tab_step
    assert S(100, 200, 200, 0) == "close" and S(100, 200, 200, 2) == "close"
    assert S(100, 200, 100, 0) == "switch" and S(100, 200, 300, 1) == "switch"     # the pass's own tab, or a leaked one: look again
    assert S(100, 200, 100, 2) == "refuse" and S(100, 200, None, 0) == "refuse" and S(100, 200, None, 2) == "refuse"
    assert S(100, None, 100, 0) == "refuse" and S(100, 100, 100, 0) == "refuse", "the throw-away tab was never told apart from the pass's own"
    assert S(None, 200, 200, 0) == "unproven" and S(None, None, None, 0) == "unproven"
    keys, fails, notes = [], [], []

    def run(pids, throw="200"):
        it = iter(pids)
        ctx = types.SimpleNamespace(be=types.SimpleNamespace(key=lambda k, *a, **kw: keys.append(k)), shell_pid=lambda: next(it))
        r = types.SimpleNamespace(check=lambda ok, msg: (fails.append(msg) if not ok else None) or ok, note=notes.append)
        return B.close_throwaway_tab(ctx, r, "100", throw, "PREV")
    saved = R.CLOSE_PANE
    R.CLOSE_PANE = "CLOSE"
    try:
        # the round 15 order: the pass's own tab is in front after the switch -> previous-tab, the throw-away shell answers, close, own shell is back
        keys.clear(); fails.clear()
        assert run(["100", "200", "100"]) == "closed" and keys == ["PREV", "CLOSE"] and not fails, (keys, fails)
        # the throw-away tab is already in front: no switch
        keys.clear(); fails.clear()
        assert run(["200", "100"]) == "closed" and keys == ["CLOSE"] and not fails, (keys, fails)
        # round 24: a leaked tab is in front first (not the pass's own, not the throw-away one): it is NOT closed, previous-tab is pressed
        keys.clear(); fails.clear()
        assert run(["300", "200", "100"]) == "closed" and keys == ["PREV", "CLOSE"] and not fails, (keys, fails)
        # the switches did not help: the close key is NOT sent
        keys.clear(); fails.clear()
        assert run(["100", "300", "100"]) == "refused" and keys == ["PREV", "PREV"] and len(fails) == 1 and "NOT sent" in fails[0], (keys, fails)
        # the close key did not bring the pass's own shell back: a failure, not a silent pass
        keys.clear(); fails.clear()
        assert run(["200", "200"]) == "closed" and keys == ["CLOSE"] and len(fails) == 1 and "expected the pass's own shell" in fails[0], fails
        # no PID probe in this pane: as before, said in a note
        keys.clear(); fails.clear(); notes.clear()
        ctx = types.SimpleNamespace(be=types.SimpleNamespace(key=lambda k, *a, **kw: keys.append(k)), shell_pid=lambda: None)
        r = types.SimpleNamespace(check=lambda ok, msg: ok, note=notes.append)
        assert B.close_throwaway_tab(ctx, r, None, None, "PREV") == "unproven" and keys == ["CLOSE"] and notes, (keys, notes)
    finally:
        R.CLOSE_PANE = saved
    body = inspect.getsource(B.c_ui_action_latency)
    assert body.index("own_pid = ctx.shell_pid()") < body.index("ctx.be.key(newtab)") < body.index("throw_pid = ctx.shell_pid()") < body.index("close_throwaway_tab(ctx, r, own_pid, throw_pid, prevtab)")
    assert "R.close_tab_best_effort(ctx, r)" not in body, "the case closes the tab in front without proving which one it is"


@check
def r21_network_gate_switch_proves_the_shell_in_front_on_every_platform():
    """#2146: the case left the Settings tab in front on Linux (the proof ran off Linux only; two Escapes do not close the tab), and
    tab_lifecycle_keys typed into the page. The proof is unconditional, and a page half that ends early is covered in the `finally`."""
    import cases_netless as N
    import cases_agent_netgate as NG
    body = inspect.getsource(N.c_network_gate_switch)
    assert "if not linux:\n            shell_back_in_front" not in body
    assert body.count('page_open = not shell_back_in_front(ctx, r, "ngs")') == 2, "the Windows early return and the end of the page half"
    assert body.index("page_open = True") < body.index("NG.open_network_kb(ctx, r)")
    fin = body[body.index("    finally:"):]
    assert "if page_open:" in fin and 'shell_back_in_front(ctx, r, "ngs_end")' in fin
    # the fact the case rests on: close_settings is Escape, which does not close the Settings tab (shell_back_in_front closes it by key)
    close = inspect.getsource(NG.close_settings)
    assert close.count('ctx.be.key("Escape")') == 2 and "editor_close" not in close
    assert "cases.editor_close(ctx, r)" in inspect.getsource(N.shell_back_in_front)


@check
def r21_scroll_keys_position_agent_view_judges():
    """#2146 / #2145: the control (is the job's output drawn?), the top keys per mode and the verdict after one key, each with the inputs that
    must fail; the app strings and behaviours the agent arm rests on are read from the Rust sources."""
    import cases_r14_w1a as W
    S = W.skp_stream_state
    assert S("terminal", 12, 6) == "drawn" and S("agent", 12, 4) == "drawn" and S("agent", 12, 1) == "agent_hidden"      # r27: one stale line is not drawn output
    assert S("agent", 12, 0) == "agent_hidden" and S("terminal", 12, 0) == "missing"
    assert S("agent", 3, 0) == "too_early" and S("terminal", 0, 0) == "too_early"
    assert W.skp_top_keys("terminal", "ctrl+up") == ["ctrl+up", "Home"] and W.skp_top_keys("agent", "ctrl+up") == ["shift+Home"]
    V = W.skp_after_key_verdict
    assert V("drawn", 3, [], 500) == ("pass", "")
    assert V("drawn", 0, [470, 500], 500)[0] == "fail"                     # numbered lines do not excuse a missing tail where output is drawn
    assert V("missing", 0, [500], 500)[0] == "fail" and V("too_early", 0, [500], 500)[0] == "fail"
    kind, why = V("agent_hidden", 0, [461, 480, 500], 500)
    assert kind == "noref" and "#%d" % W.ISSUE_AGENT_BG_OUTPUT in why and "NOT judged" in why and "shift+Home" in why, why
    assert V("agent_hidden", 0, [316, 361], 500)[0] == "fail"              # the view is still up: the key did not bring the end back
    assert V("agent_hidden", 0, [], 500)[0] == "fail"
    assert V("agent_hidden", 0, [485], 500)[0] == "noref" and V("agent_hidden", 0, [484], 500)[0] == "fail"
    body = inspect.getsource(W.c_scroll_keys_position)
    assert body.index('ctx.ocr("skp_0_stream")') < body.index('ctx.ocr("skp_1_pageup")') < body.index("skp_top_keys(ctx.mode") < body.index("skp_after_key_verdict(")
    assert 'stream_state == "drawn"' in body and "r.no_reference(why)" in body
    # the app side: Shift+Home is the scrollback-top key and logs key=top; it is live at a prompt
    keys = _src("app", "src", "terminal", "view", "scrollback_keys.rs")
    assert 'Self::Top => "shift-home"' in keys and 'Self::Top => "top"' in keys and 'format!("key={}", key.hop_name())' in keys
    assert '(!id!("LongRunningCommand") | !id!("EditorFocused"))' in keys
    # a keyboard block selection in AI input leaves the focus in the input (why Home after block-up is the editor's in the agent view)
    view = _src("app", "src", "terminal", "view.rs")
    assert "we don't want to divert the focus away from the input box" in view
    assert "if !is_shift_down && self.should_use_agent_transcript_navigation(ctx) {" in view
    # a block no command of the conversation produced is hidden in that conversation (why the job's output is on no screen there)
    block = _src("app", "src", "terminal", "model", "block.rs")
    assert "if !visible_in_conversation && !self.shown_to_tty_reader() {" in block


@check
def r21_app_kill_switches_named_in_the_sources_and_the_docs():
    """the two app changes of round 21 Wayland are switched off by a variable each; the names in docs/testing.md are the ones the Rust reads, and
    the reload set and the pair rule are where the notes say"""
    files = _src("crates", "warp_files", "src", "lib.rs")
    assert 'const RELOAD_ON_RENAME_ENV: &str = "PHOSPHOR_FILE_RELOAD_ON_RENAME";' in files
    assert "self.reload_file_paths(paths_to_reload(event, reload_on_rename_env_enabled()), ctx);" in files
    watcher = _src("crates", "watcher", "src", "lib.rs")
    assert "pub fn content_changed_set(&self) -> HashSet<PathBuf>" in watcher and ".chain(self.moved.keys())" in watcher
    tree = _src("app", "src", "pane_group", "tree.rs")
    assert 'const RESIZE_DRAWN_PAIR_ENV: &str = "PHOSPHOR_PANE_RESIZE_DRAWN_PAIR";' in tree
    assert "resize_pair_for_child(&self.drawn_node_indices(hidden_panes), idx)" in tree
    assert "resize_pair_for_divider(&self.drawn_node_indices(hidden_panes), idx)" in tree
    doc = _src("docs", "testing.md")
    for name in ("PHOSPHOR_FILE_RELOAD_ON_RENAME=0", "PHOSPHOR_PANE_RESIZE_DRAWN_PAIR=0", "selftest_r21_wlpanes.py"):
        assert name in doc, name
    # the app cases that prove the two fixes read what they read before: nothing was loosened
    import cases_r15_w5a as W5
    import cases_pointer as P
    assert "a file changed on disk with no unsaved edits was not reloaded" in inspect.getsource(W5._arm_external)
    assert "os.replace(tmp, path)" in inspect.getsource(W5.replace_file)
    assert "errs = divider_verdict(a1, brest, ak, bk)" in inspect.getsource(P.c_pane_resize_maximize)
