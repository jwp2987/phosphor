# appci/cases_agent_mcp.py (#1834): misbehaving MCP servers after the rmcp upgrade. One case, `agent_mcp_faults` (agent pass), with one stage per fault.
# The fault servers are fakemcp.py processes (stdio, stdlib Python) written into the profile's `~/.warp/.mcp.json` (the Zap-native global config: always
# detected and spawned, no feature toggle; app/src/ai/mcp/file_based_manager.rs), and the app is relaunched once so they start with it. The fake model
# (fake_model.py, APPCI_FAKE_MODEL=1) is asked "Please call mcp tool ping on <server> with {...}" and answers with the function call
# `mcp__<server>__ping` (the app's naming, ai/agent_providers/tools/mcp.rs) when the app offered it in the request's `tools`.
# Oracles are ON THE WIRE and on disk, not screenshots alone:
#   * fake.tool_results: the tool-role message the app sent back to the model for the call (the "clear tool error", or the success text);
#   * fake.user_turns: a follow-up prose turn typed after the call reaches the model (the agent turn completed and the conversation takes input again);
#   * the server's state file (one line per start / call): the server really ran, and how often (restart);
#   * the app log: no `panicked at`; `hop=ui_item` main-thread items (a stalled main thread shows as one huge item) and a tab-switch probe.
# Stages: crash_start, crash_call, hang (tool_timeout_secs ends it), garbage (malformed JSON-RPC), big (5 MB), flaky (exits after a call, must work again).
# Linux and macOS: the config lives under HOME, which both apps derive their paths from. Windows SKIPS (the profile is not HOME-relative there and the
# Windows pane has no verified MCP config location in this harness). A stage that cannot prove its precondition says so and does not pass.
import json, os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases as C  # noqa: E402
import cases_caseagent as CA  # noqa: E402
import cases_robust as R  # noqa: E402
from cases import case  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
SERVERS = [("fxok", "ok"), ("fxcrashstart", "crash_start"), ("fxcrashcall", "crash_call"), ("fxhang", "hang"), ("fxgarbage", "garbage"), ("fxbig", "big"),
           ("fxflaky", "flaky")]
TOOL_TIMEOUT_S = 20
ERR_RE = re.compile(r"(?i)(error|fail|closed|disconnect|exit|crash|terminat|timed out|timeout|invalid|malformed|unavailable|not connected|broken)")
UI_BLOCK_MS = float(os.environ.get("APPCI_MCP_UI_BLOCK_MS", "250"))        # one main-thread item this long = the UI was stalled


# ---------------------------------------------------------------- pure helpers (unit-tested in selftest.py)
def mcp_config(py, script, state_dir, timeout_s=TOOL_TIMEOUT_S):
    """the `~/.warp/.mcp.json` text: one entry per fault server, each with its own state file and the tool timeout that ends a hang"""
    return json.dumps({"mcpServers": {name: {"command": py, "args": [script, mode, os.path.join(state_dir, name + ".state")], "tool_timeout_secs": timeout_s}
                                      for name, mode in SERVERS}}, indent=1)


def state_events(text):
    """-> (starts, calls) of a server's state file"""
    ls = [l.strip() for l in (text or "").splitlines() if l.strip()]
    return sum(1 for l in ls if l == "start"), sum(1 for l in ls if l.startswith("call"))


def error_is_clear(text):
    """a tool result that reads as an error to the model: non-empty, says what went wrong, is not a success payload and not a raw panic/backtrace"""
    t = (text or "").strip()
    return bool(t) and bool(ERR_RE.search(t)) and "PONG:" not in t and "panicked" not in t and "RUST_BACKTRACE" not in t


def panics(log_text):
    return [l.strip()[:200] for l in log_text.splitlines() if "panicked at" in l]


def flaky_verdict(results):
    """results = the tool-result texts of the flaky server's calls in order. -> (ok, why). Call 1 must be LAUNCH-1; at least one later call must be a
    LAUNCH-n with n >= 2 (the server was restarted and answered); an error in between is allowed, a restart that never comes is not."""
    if not results or "LAUNCH-1:" not in results[0]:
        return False, "call 1 did not return LAUNCH-1: %r" % (results[0][:120] if results else None)
    later = [int(m.group(1)) for t in results[1:] for m in [re.search(r"LAUNCH-(\d+):", t)] if m]
    if any(n >= 2 for n in later):
        return True, "restarted: later calls answered by launch(es) %s" % later
    return False, "no later call was answered by a restarted server: %s" % [t[:80] for t in results[1:]]


# ---------------------------------------------------------------- driving
def drive(ctx, pred, timeout, off, tick=None):
    """wait for pred(); an action card (`queue_actions` in the log) is focused and approved with Return, Return like agent_approval does. tick() runs ~every 2 s."""
    t0, pressed, last_tick = time.time(), 0, 0.0
    while time.time() - t0 < timeout:
        if pred():
            return True
        cards = CA.count_in_log(ctx, off, "queue_actions")
        if cards > pressed:
            pressed = cards
            time.sleep(1.0)
            ctx.be.key("Return")
            time.sleep(0.8)
            ctx.be.key("Return")
        if tick and time.time() - last_tick >= 2.0:
            last_tick = time.time()
            tick()
        time.sleep(0.4)
    return pred()


def ui_probe(ctx, times):
    """tab switch 1 -> 2 -> 1 seen from screenshots (an UPPER bound, as in cb_ui_action_latency); appends seconds (or None) to `times`"""
    import cases_coverage_b as CB
    import cases_ui as U
    if not U.have_pillow():
        return
    for key in (R.tab_key(ctx, 2), R.tab_key(ctx, 1)):
        before = ctx.shot("mcp_probe_before")
        ctx.be.key(key)
        times.append(CB._until_changed(ctx, before, "mcp_probe", 0.0005, 3))
        time.sleep(0.3)


def ui_verdict(ctx, r, label, off, times):
    items = C.ui_items(ctx.log_since(off))
    worst = max([ms for _, ms, _ in items], default=0.0)
    r.metric("%s_ui_worst_item_ms" % label, worst)
    r.check(worst < UI_BLOCK_MS, "%s: a main-thread item of %.0f ms (>= %.0f ms) while the MCP call ran: the UI was stalled" % (label, worst, UI_BLOCK_MS))
    got = [t for t in times if t is not None and t >= 0]
    if times:
        r.metric("%s_tab_switch_s" % label, [round(t, 2) if t is not None else None for t in times])
        lim = float(os.environ.get("APPCI_UI_ACTION_MAX_S", "1.5"))
        late = [t for t in got if t > lim]
        r.check(not late, "%s: tab switch took %s s while the MCP call ran (bound %.1f s)" % (label, [round(t, 2) for t in late], lim))
        if len(got) < len(times):
            r.note("%s: %d tab-switch probe(s) saw no changed frame (two similar tabs can look alike; not a verdict)" % (label, len(times) - len(got)))
    else:
        r.note("%s: no Pillow, so no tab-switch probe; the UI verdict is the main-thread item log only" % label)


def call_prompt(ctx, r, fake, server, tok, off, label):
    """type the call request (10-25 ms/key, instant Enter); the Enter must reach the AGENT input byte for byte"""
    sent = 'Please call mcp tool ping on %s with {"text": "%s"}' % (server, tok)
    CA.type_fast(ctx, sent)
    ctx.until(lambda: CA.enter_buffers(ctx.log_since(off)), 6)
    err = CA.judge_agent_input(sent, CA.enter_buffers(ctx.log_since(off)))
    r.check(err is None, "%s: the call request: %s" % (label, err))


def followup_ok(ctx, r, fake, label, settle_s=90):
    """the turn is over and the conversation takes input again: a prose turn typed now reaches the model"""
    CA.settled(ctx, "mcp_idle_" + label, quiet=3, step=1.0, timeout=settle_s)
    tok = ctx.tok(5).upper()
    n = len(fake.user_turns)
    CA.type_fast(ctx, "Reply with the word AFTER%s" % tok)
    ok = ctx.until(lambda: any("AFTER" + tok in t for _, t in fake.user_turns[n:]), 30)
    r.check(bool(ok), "%s: a follow-up typed after the call never reached the model within 30 s (the agent turn did not complete, or the input is stuck)" % label)
    time.sleep(1.5)
    return bool(ok)


def one_call(ctx, r, fake, server, label, timeout, ticker=None, tok=None):
    """one fresh conversation, one call. -> (tool_result_text or None, seconds, log_offset)"""
    CA.fresh_conversation(ctx, r)
    tok = tok or ctx.tok(6).lower()
    off, n_res, n_miss = ctx.log_size(), len(fake.tool_results), len(fake.mcp_missing)
    t0 = time.time()
    call_prompt(ctx, r, fake, server, tok, off, label)
    got = drive(ctx, lambda: len(fake.tool_results) > n_res or len(fake.mcp_missing) > n_miss, timeout, off, ticker)
    dt = time.time() - t0
    res = fake.tool_results[n_res][1] if len(fake.tool_results) > n_res else None
    missing = len(fake.mcp_missing) > n_miss
    return res, missing, dt, off, got


def state_file(ctx, name):
    return os.path.join(ctx.W, "mcp_state", name + ".state")


def read_state(ctx, name):
    try:
        return state_events(open(state_file(ctx, name)).read())
    except OSError:
        return 0, 0


@case("agent_mcp_faults", modes=("agent",), order=96, gui=True)
def c_agent_mcp_faults(ctx, r):
    """#1834: six misbehaving MCP servers (crash at start, crash mid-call, hang, malformed JSON-RPC, 5 MB result, exit and work after a restart) plus a
    healthy control. Per stage: the call ends (a clear tool error or a result, never a hang past the tool timeout), the agent turn completes and takes
    the next prose turn, the UI stays responsive (no stalled main-thread item, tab switches within bound), no panic, and the restart server works again."""
    import fake_model
    if os.environ.get("APPCI_FAKE_MODEL_ACTIVE") != "1" or fake_model.ACTIVE is None:
        r.skip("the fake model is not running (APPCI_FAKE_MODEL=1): the MCP calls need a scripted provider that emits the tool calls")
    if ctx.win:
        r.skip("Windows: the profile's config home is not HOME-relative and the harness has no verified MCP config location there (Linux and macOS run this case)")
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.skip("no app log at %s: panics, approval cards and UI items cannot be read" % ctx.log_path)
    R.need_relaunch_app(ctx, "agent_mcp_faults")
    fake = fake_model.ACTIVE
    cfg_dir = os.path.join(ctx.prof.home, ".warp")
    cfg = os.path.join(cfg_dir, ".mcp.json")
    sdir = os.path.join(ctx.W, "mcp_state")
    os.makedirs(cfg_dir, exist_ok=True)
    os.makedirs(sdir, exist_ok=True)
    for n, _ in SERVERS:
        try:
            os.remove(state_file(ctx, n))
        except OSError:
            pass
    existing = open(cfg).read() if os.path.exists(cfg) else None
    try:
        open(cfg, "w").write(mcp_config(sys.executable, os.path.join(HERE, "fakemcp.py"), sdir))
        _, up = R.start_app(ctx, r, "mcp_faults")
        if not up:
            return
        time.sleep(6)                                  # the servers start with the app; the tool list is built after their handshakes
        CA.wait_view_mode(ctx, "agent", 10)
        if ctx.log_view_mode() != "agent":
            r.skip("after the relaunch the pane is not in agent view (hop=view_mode %s)" % ctx.log_view_mode())
        ctx.be.key(R.NEWTAB)                           # a second tab for the tab-switch probe
        time.sleep(1.5)
        ctx.be.key(R.tab_key(ctx, 1))
        time.sleep(1.0)
        log0 = ctx.log_size()
        # ---- control: a healthy server proves the precondition (servers load from the config and their tools reach the model)
        tok = ctx.tok(6).lower()
        res, missing, dt, off, got = one_call(ctx, r, fake, "fxok", "control", 90, tok=tok)
        if missing or res is None:
            ctx.shot("mcp_control_failed")
            r.skip("precondition: the healthy control server's tool was not offered to the model or the call never returned (servers seen: %s; starts %s): "
                   "the app did not load ~/.warp/.mcp.json, so no fault could be injected" % (sorted(t for t in fake.tools_seen if t and t.startswith("mcp__")), read_state(ctx, "fxok")))
        r.check("PONG:" + tok in res, "control: the healthy server's result was %r, expected PONG:%s" % (res[:120], tok))
        followup_ok(ctx, r, fake, "control")

        def fault_stage(server, label):
            ticks = []
            res, missing, dt, off, got = one_call(ctx, r, fake, server, label, TOOL_TIMEOUT_S + 40, ticker=lambda: ui_probe(ctx, ticks))
            r.metric("%s_call_s" % label, round(dt, 1))
            ctx.shot("mcp_%s_result" % label)
            r.check(not panics(ctx.log_since(off)), "%s: panic in the app log: %s" % (label, panics(ctx.log_since(off))[:2]))
            ui_verdict(ctx, r, label, off, ticks)
            r.check(got, "%s: neither a tool result nor a 'tool not offered' turn within %d s (the call hung the turn)" % (label, TOOL_TIMEOUT_S + 40))
            return res, missing

        # ---- crash at start: the server never answers initialize
        res, missing = fault_stage("fxcrashstart", "crash_start")
        starts, _ = read_state(ctx, "fxcrashstart")
        if not starts:
            r.note("crash_start: the server's state file has no start line (the app never spawned it): precondition unproven for this stage")
        elif res is not None:
            r.check(error_is_clear(res), "crash_start: the tool was offered and the call returned %r, which does not read as a clear error" % res[:160])
        else:
            r.check(missing, "crash_start: no tool result and no 'tool not offered' reply")
            r.note("crash_start: the app did not offer a tool from the server that died at start (ran %d time(s)); the turn went on without it" % starts)
        followup_ok(ctx, r, fake, "crash_start")
        # ---- crash in the middle of a call
        res, missing = fault_stage("fxcrashcall", "crash_call")
        if missing:
            r.note("crash_call: the tool was not offered, so a crash MID-call was not exercised (server starts %s)" % (read_state(ctx, "fxcrashcall"),))
        else:
            r.check(res is not None and error_is_clear(res), "crash_call: tool result %r does not read as a clear error" % (res or "")[:160])
            r.check(read_state(ctx, "fxcrashcall")[1] >= 1, "crash_call: the server logged no tools/call (the crash was not mid-call)")
        followup_ok(ctx, r, fake, "crash_call")
        # ---- hang: the tool timeout (tool_timeout_secs) ends the call
        res, missing = fault_stage("fxhang", "hang")
        if missing:
            r.note("hang: the tool was not offered, so a hanging call was not exercised")
        else:
            r.check(res is not None and error_is_clear(res), "hang: tool result %r does not read as a clear error (expected a timeout notice)" % (res or "")[:160])
            r.check(res is not None and re.search(r"(?i)timed out|timeout", res) is not None, "hang: the error does not say it timed out: %r" % (res or "")[:160])
        followup_ok(ctx, r, fake, "hang")
        # ---- malformed JSON-RPC
        res, missing = fault_stage("fxgarbage", "garbage")
        if missing:
            r.note("garbage: the tool was not offered, so malformed replies were not exercised")
        else:
            r.check(res is not None and (("PONG:" in res) or error_is_clear(res)), "garbage: tool result %r is neither the valid reply nor a clear error" % (res or "")[:160])
            r.note("garbage: the app %s" % ("survived the noise and returned the valid reply" if res and "PONG:" in res else "reported an error for the malformed replies"))
        followup_ok(ctx, r, fake, "garbage")
        # ---- 5 MB result
        res, missing = fault_stage("fxbig", "big")
        if missing:
            r.note("big: the tool was not offered, so the 5 MB result was not exercised")
        else:
            r.check(res is not None and len(res) > 0, "big: no tool result")
            r.metric("big_result_chars_to_model", len(res or ""))
            r.check(res is not None and (res.startswith("BIG:") or error_is_clear(res) or re.search(r"(?i)truncat|too large|too big|limit", res)),
                    "big: tool result is neither the payload, a truncation notice nor a clear error: %r" % (res or "")[:120])
        followup_ok(ctx, r, fake, "big")
        # ---- exit after a call; the next calls must work on a restarted server
        results = []
        for i in range(3):
            res, missing = fault_stage("fxflaky", "flaky_%d" % (i + 1))
            if missing:
                r.note("flaky: the tool was not offered at call %d: %s" % (i + 1, "the server did not come back" if i else "never offered"))
                results.append("")
                if i == 0:
                    break
                continue
            results.append(res or "")
            followup_ok(ctx, r, fake, "flaky_%d" % (i + 1))
            if i >= 1 and re.search(r"LAUNCH-([2-9])", res or ""):
                break
        if results and results[0]:
            ok, why = flaky_verdict(results)
            r.check(ok, "flaky: restart: %s; server starts %s" % (why, read_state(ctx, "fxflaky")))
            r.check(read_state(ctx, "fxflaky")[0] >= 2 or not ok, "flaky: the state file shows %d start(s); a restart needs 2" % read_state(ctx, "fxflaky")[0])
        else:
            r.note("flaky: the first call never produced LAUNCH-1, so the restart was not exercised")
        r.check(ctx.app.alive(), "the app died during the MCP fault stages")
        r.check(not panics(ctx.log_since(log0)), "panic somewhere in the MCP stages: %s" % panics(ctx.log_since(log0))[:2])
    finally:
        CA.cleanup_tmp(ctx)
        try:
            if existing is None:
                os.remove(cfg)
            else:
                open(cfg, "w").write(existing)
            R.start_app(ctx, r, "mcp_faults_cleanup")      # the fault servers must not outlive the case
        except Exception as e:  # noqa: BLE001
            r.note("cleanup relaunch failed: %s" % e)
