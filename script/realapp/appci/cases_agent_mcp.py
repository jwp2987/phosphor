# appci/cases_agent_mcp.py (#1834, #2065): MCP in the real app. Two cases, both in the fake-model pass (agent mode):
#   mcp_basic          the healthy control: ONE well-behaved server in the app's global MCP config is started by the app, its tool is offered to the
#                      model, the call is made (after the approval card where the profile shows one), the result reaches the model, the turn ends.
#                      Every step is a check: a server that never starts or a tool that is never offered FAILS here (it is the subject).
#   agent_mcp_faults   misbehaving MCP servers after the rmcp upgrade, one stage per fault, behind the same control.
# The servers are fakemcp.py processes (stdio, stdlib Python) written into the app's global MCP config and the app is relaunched once so they start
# with it (always detected and spawned, no feature toggle; app/src/ai/mcp/file_based_manager.rs).
# WHERE THAT CONFIG IS (#2065: the case had never been judged on any platform, because it wrote `~/.warp/.mcp.json`, which the app under test never reads):
#   the app reads `<home>/<config dir>/.mcp.json` (warp_core::paths::warp_home_mcp_config_file_path), where
#   <config dir> = `.phosphor` for the Oss channel (paths.rs base_warp_config_dir_name; every build the harness tests is app/src/bin/phosphor_oss.rs;
#                  `.warp` is the Stable/Preview name of upstream), and
#   <home>       = dirs::home_dir(): $HOME on Linux and macOS (the scratch home of the profile), and on Windows the user's PROFILE known folder
#                  (SHGetKnownFolderPath(FOLDERID_Profile); HOME is not consulted, so the Git Bash pane's scratch HOME does not move it).
#   On Windows the file therefore sits in the runner user's own profile: the case keeps and restores whatever was there, and a file left by a killed
#   run (every server name is one of ours) is recognised and removed, never restored. selftest.py pins the directory name against paths.rs.
# The fake model (fake_model.py, APPCI_FAKE_MODEL=1) is asked "Please call mcp tool ping on <server> with {...}" and answers with the function call
# `mcp__<server>__ping` (the app's naming, ai/agent_providers/tools/mcp.rs) when the app offered it in the request's `tools`.
# Oracles are ON THE WIRE and on disk, not screenshots alone:
#   * fake.tool_results: the tool-role message the app sent back to the model for the call (the "clear tool error", or the success text);
#   * fake.user_turns: a follow-up prose turn typed after the call reaches the model (the agent turn completed and the conversation takes input again);
#   * the server's state file (one line per start / call): the server really ran, and how often (restart);
#   * the app log: no `panicked at`; `hop=ui_item` main-thread items (a stalled main thread shows as one huge item) and a tab-switch probe.
# Stages: crash_start, crash_call, hang (tool_timeout_secs ends it), garbage (malformed JSON-RPC), big (5 MB), flaky (exits after a call, must work again).
# Windows: the Git Bash pane only (winmap.py; the MCP path does not depend on the pane's shell, one pane is enough). No stage passes on an unproven
# precondition: the healthy control failing FAILS both cases with what was found on disk and in the log.
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


# the app's home config directory: warp_core::paths::base_warp_config_dir_name() for Channel::Oss (selftest.py reads paths.rs and fails when this drifts)
APP_CONFIG_DIR = ".phosphor"
START_WAIT_S = 30            # the app spawns the global servers at startup; a healthy one writes `start` within a second or two


# ---------------------------------------------------------------- pure helpers (unit-tested in selftest.py)
def mcp_config(py, script, state_dir, timeout_s=TOOL_TIMEOUT_S, servers=SERVERS):
    """the global MCP config text: one entry per server, each with its own state file and the tool timeout that ends a hang"""
    return json.dumps({"mcpServers": {name: {"command": py, "args": [script, mode, os.path.join(state_dir, name + ".state")], "tool_timeout_secs": timeout_s}
                                      for name, mode in servers}}, indent=1)


def mcp_config_path(home, dirname=APP_CONFIG_DIR):
    """where the app under test reads its global MCP config, given the directory its dirs::home_dir() returns"""
    return os.path.join(home, dirname, ".mcp.json")


def harness_owned(text):
    """True when `text` is a config THIS harness wrote (a non-empty mcpServers object whose every name is one of SERVERS): the leftover of a killed
    run, which is removed and never restored as if it were the user's own file"""
    try:
        servers = json.loads(text).get("mcpServers")
    except (ValueError, AttributeError):
        return False
    ours = {n for n, _ in SERVERS}
    return isinstance(servers, dict) and bool(servers) and set(servers) <= ours


def mcp_log_lines(log_text, limit=6):
    """the app log lines that mention MCP (evidence for a control that failed: was the config read, was a spawn attempted, what was the error)"""
    return [l.strip()[-220:] for l in (log_text or "").splitlines() if "mcp" in l.lower()][-limit:]


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
def drive(ctx, pred, timeout, off, tick=None, on_card=None):
    """wait for pred(); an action card (`queue_actions` in the log) is focused and approved with Return, Return like agent_approval does. tick() runs ~every 2 s.
    on_card() runs once per card, after it has been on screen for a second and BEFORE the first Return (mcp_basic reads the server's call count there)."""
    t0, pressed, last_tick = time.time(), 0, 0.0
    while time.time() - t0 < timeout:
        if pred():
            return True
        cards = CA.count_in_log(ctx, off, "queue_actions")
        if cards > pressed:
            pressed = cards
            time.sleep(1.0)
            if on_card:
                on_card()
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


def one_call(ctx, r, fake, server, label, timeout, ticker=None, tok=None, on_card=None):
    """one fresh conversation, one call. -> (tool_result_text or None, tool not offered, seconds, log_offset, ended in time)"""
    CA.fresh_conversation(ctx, r)
    tok = tok or ctx.tok(6).lower()
    off, n_res, n_miss = ctx.log_size(), len(fake.tool_results), len(fake.mcp_missing)
    t0 = time.time()
    call_prompt(ctx, r, fake, server, tok, off, label)
    got = drive(ctx, lambda: len(fake.tool_results) > n_res or len(fake.mcp_missing) > n_miss, timeout, off, ticker, on_card)
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


def app_home(ctx):
    """the directory the app's dirs::home_dir() returns (the header says why): the scratch home on Linux and macOS, the user's profile folder on Windows"""
    if ctx.win:
        import winapi
        return winapi.profile_dir()
    return ctx.prof.home


def need_mcp(ctx, r, name):
    """the fake model, the app log and an app handle that can relaunch; -> the fake model"""
    import fake_model
    if os.environ.get("APPCI_FAKE_MODEL_ACTIVE") != "1" or fake_model.ACTIVE is None:
        r.skip("the fake model is not running (APPCI_FAKE_MODEL=1): the MCP calls need a scripted provider that emits the tool calls")
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.skip("no app log at %s: panics, approval cards and UI items cannot be read" % ctx.log_path)
    R.need_relaunch_app(ctx, name)
    return fake_model.ACTIVE


class McpSession:
    """The app relaunched on a global MCP config that holds `servers`. leave() puts back what was at that path (or removes the file) and relaunches,
    so no test server outlives the case and the next case gets the profile it expects."""

    def __init__(self, ctx, r, label, servers):
        self.ctx, self.r, self.label, self.servers = ctx, r, label, servers
        self.cfg = mcp_config_path(app_home(ctx))
        self.shown = "<app home>/%s/.mcp.json" % APP_CONFIG_DIR         # the full path names the machine's user on Windows: not put into results
        self.existing, self.wrote = None, False

    def enter(self):
        """-> True when the app is up on the config"""
        ctx, r = self.ctx, self.r
        os.makedirs(os.path.dirname(self.cfg), exist_ok=True)
        os.makedirs(os.path.join(ctx.W, "mcp_state"), exist_ok=True)
        for n, _ in SERVERS:
            try:
                os.remove(state_file(ctx, n))
            except OSError:
                pass
        if os.path.exists(self.cfg):
            raw = open(self.cfg, "rb").read()
            if harness_owned(raw.decode("utf-8", "replace")):
                r.note("%s held a config written by an earlier, killed run of this harness (only its server names): removed, not restored" % self.shown)
            else:
                self.existing = raw
        self.wrote = True
        with open(self.cfg, "w", encoding="utf-8") as f:
            f.write(mcp_config(sys.executable, os.path.join(HERE, "fakemcp.py"), os.path.join(ctx.W, "mcp_state"), servers=self.servers))
        _, up = R.start_app(ctx, r, self.label)
        return bool(up)

    def leave(self):
        ctx, r = self.ctx, self.r
        CA.cleanup_tmp(ctx)
        if not self.wrote:
            return
        try:
            if self.existing is None:
                os.remove(self.cfg)
            else:
                with open(self.cfg, "wb") as f:
                    f.write(self.existing)
            R.start_app(ctx, r, self.label + "_cleanup")      # the test servers must not outlive the case
        except Exception as e:  # noqa: BLE001
            r.note("cleanup relaunch failed: %s" % e)

    def evidence(self, off):
        """what a failed control reports: is the file there, did the server start, what did the model get offered, what does the app log say about MCP"""
        fake = getattr(sys.modules.get("fake_model"), "ACTIVE", None)
        return "config %s %s; fxok state (starts, calls) %s; mcp tools the model was offered: %s; app log lines naming MCP: %s" % (
            self.shown, "present" if os.path.exists(self.cfg) else "MISSING", read_state(self.ctx, "fxok"),
            sorted(t for t in getattr(fake, "tools_seen", ()) if t and t.startswith("mcp__")) or "none",
            mcp_log_lines(self.ctx.log_since(off)) or "none")


def control_call(ctx, r, fake, sess, label):
    """The healthy server, judged step by step: started by the app, tool offered, called (after the approval card where one is shown), result back at
    the model, turn over. -> True when every step held. A failure names the step and carries McpSession.evidence(), so the row itself says whether the
    config was read, whether a spawn happened and what the app logged."""
    log0 = ctx.log_size()
    started = ctx.until(lambda: read_state(ctx, "fxok")[0] >= 1, START_WAIT_S, step=0.5)
    ok = r.check(bool(started), "%s: the app never started the healthy MCP server from its global config within %d s of the relaunch: %s" % (
        label, START_WAIT_S, sess.evidence(log0)))
    if not ok:
        ctx.shot("mcp_%s_not_started" % label)
        return False
    time.sleep(4)                                      # the tool list is built after the handshake (initialize, tools/list)
    CA.wait_view_mode(ctx, "agent", 10)
    if ctx.log_view_mode() != "agent":
        r.skip("after the relaunch the pane is not in agent view (hop=view_mode %s)" % ctx.log_view_mode())
    tok = ctx.tok(6).lower()
    seen_at_card = []
    res, missing, dt, off, got = one_call(ctx, r, fake, "fxok", label, 90, tok=tok, on_card=lambda: seen_at_card.append(read_state(ctx, "fxok")[1]))
    r.metric("%s_call_s" % label, round(dt, 1))
    r.metric("%s_approval_cards" % label, len(seen_at_card))
    ok = r.check(not missing, "%s: the healthy server runs but its tool was not offered to the model (the request's `tools` had no mcp__fxok__ping): %s" % (
        label, sess.evidence(log0)))
    ok = r.check(res is not None, "%s: the call never returned a tool result to the model within 90 s: %s" % (label, sess.evidence(log0))) and ok
    if res is not None:
        ok = r.check("PONG:" + tok in res, "%s: the healthy server's result was %r, expected PONG:%s" % (label, res[:120], tok)) and ok
        ok = r.check(read_state(ctx, "fxok")[1] == 1, "%s: the server logged %d tools/call for one request (expected exactly 1)" % (label, read_state(ctx, "fxok")[1])) and ok
    if seen_at_card:
        ok = r.check(seen_at_card[0] == 0, "%s: the server had already received the call (%d) while the approval card was still unanswered" % (
            label, seen_at_card[0])) and ok
    else:
        r.note("%s: no approval card was shown for the MCP call (the profile's permissions run it without one), so 'called only after approval' was not exercised" % label)
    if not ok:
        ctx.shot("mcp_%s_failed" % label)
        return False
    return followup_ok(ctx, r, fake, label)


@case("mcp_basic", modes=("agent",), order=95, gui=True)
def c_mcp_basic(ctx, r):
    """#2065 (T40): one healthy MCP server in the app's global config. The app starts it, offers its tool to the model, makes the call (after the
    approval card where the profile shows one), returns the result to the model and takes the next prose turn. No step is a precondition: each fails
    the case, with the config path's state, the server's own record and the app log lines about MCP in the message."""
    fake = need_mcp(ctx, r, "mcp_basic")
    sess = McpSession(ctx, r, "mcp_basic", [("fxok", "ok")])
    try:
        if not sess.enter():
            return
        log0 = ctx.log_size()
        control_call(ctx, r, fake, sess, "basic")
        r.check(ctx.app.alive(), "the app died during the MCP call")
        r.check(not panics(ctx.log_since(log0)), "panic in the app log: %s" % panics(ctx.log_since(log0))[:2])
    finally:
        sess.leave()


@case("agent_mcp_faults", modes=("agent",), order=96, gui=True)
def c_agent_mcp_faults(ctx, r):
    """#1834: six misbehaving MCP servers (crash at start, crash mid-call, hang, malformed JSON-RPC, 5 MB result, exit and work after a restart) plus a
    healthy control. Per stage: the call ends (a clear tool error or a result, never a hang past the tool timeout), the agent turn completes and takes
    the next prose turn, the UI stays responsive (no stalled main-thread item, tab switches within bound), no panic, and the restart server works again.
    #2065: the control is a check, not a skip: if the healthy server does not work no fault was injected, and the case FAILS saying so (mcp_basic is
    the same control alone)."""
    fake = need_mcp(ctx, r, "agent_mcp_faults")
    sess = McpSession(ctx, r, "mcp_faults", SERVERS)
    try:
        if not sess.enter():
            return
        ctx.be.key(R.NEWTAB)                           # a second tab for the tab-switch probe
        time.sleep(1.5)
        ctx.be.key(R.tab_key(ctx, 1))
        time.sleep(1.0)
        log0 = ctx.log_size()
        # ---- control: the healthy server proves the servers load from the config and their tools reach the model
        if not control_call(ctx, r, fake, sess, "control"):
            r.check(False, "the healthy control did not hold, so NO fault stage was run: nothing about fault handling is proven by this row")
            return

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
            r.unverifiable("crash_start: the server's state file has no start line (the app never spawned it, although it started the healthy one): this stage proved nothing")
        elif res is not None:
            r.check(error_is_clear(res), "crash_start: the tool was offered and the call returned %r, which does not read as a clear error" % res[:160])
        else:
            r.check(missing, "crash_start: no tool result and no 'tool not offered' reply")
            r.note("crash_start: the app did not offer a tool from the server that died at start (ran %d time(s)); the turn went on without it" % starts)
        followup_ok(ctx, r, fake, "crash_start")
        # ---- crash in the middle of a call
        res, missing = fault_stage("fxcrashcall", "crash_call")
        if missing:
            r.unverifiable("crash_call: the tool was not offered, so a crash MID-call was not exercised (server starts %s): this stage proved nothing" % (read_state(ctx, "fxcrashcall"),))
        else:
            r.check(res is not None and error_is_clear(res), "crash_call: tool result %r does not read as a clear error" % (res or "")[:160])
            r.check(read_state(ctx, "fxcrashcall")[1] >= 1, "crash_call: the server logged no tools/call (the crash was not mid-call)")
        followup_ok(ctx, r, fake, "crash_call")
        # ---- hang: the tool timeout (tool_timeout_secs) ends the call
        res, missing = fault_stage("fxhang", "hang")
        if missing:
            r.unverifiable("hang: the tool was not offered, so a hanging call was not exercised (server starts %s): this stage proved nothing" % (read_state(ctx, "fxhang"),))
        else:
            r.check(res is not None and error_is_clear(res), "hang: tool result %r does not read as a clear error (expected a timeout notice)" % (res or "")[:160])
            r.check(res is not None and re.search(r"(?i)timed out|timeout", res) is not None, "hang: the error does not say it timed out: %r" % (res or "")[:160])
        followup_ok(ctx, r, fake, "hang")
        # ---- malformed JSON-RPC
        res, missing = fault_stage("fxgarbage", "garbage")
        if missing:
            r.unverifiable("garbage: the tool was not offered, so malformed replies were not exercised (server starts %s): this stage proved nothing" % (read_state(ctx, "fxgarbage"),))
        else:
            r.check(res is not None and (("PONG:" in res) or error_is_clear(res)), "garbage: tool result %r is neither the valid reply nor a clear error" % (res or "")[:160])
            r.note("garbage: the app %s" % ("survived the noise and returned the valid reply" if res and "PONG:" in res else "reported an error for the malformed replies"))
        followup_ok(ctx, r, fake, "garbage")
        # ---- 5 MB result
        res, missing = fault_stage("fxbig", "big")
        if missing:
            r.unverifiable("big: the tool was not offered, so the 5 MB result was not exercised (server starts %s): this stage proved nothing" % (read_state(ctx, "fxbig"),))
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
            r.unverifiable("flaky: the first call never produced LAUNCH-1, so the restart was not exercised (server starts %s): this stage proved nothing" % (read_state(ctx, "fxflaky"),))
        r.check(ctx.app.alive(), "the app died during the MCP fault stages")
        r.check(not panics(ctx.log_since(log0)), "panic somewhere in the MCP stages: %s" % panics(ctx.log_since(log0))[:2])
    finally:
        sess.leave()
