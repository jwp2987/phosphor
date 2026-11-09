# appci/cases_netless.py: Wave N, "no network at startup" (docs/testing.md 2.17). Registered with the same @case decorator as cases.py (imported by its last line).
#   zero_network_startup   Linux: the app, with the harness profile (no cloud account, no provider key), is launched under `strace -f` and left idle for APPCI_ZNS_IDLE_S
#                          (default 120 s), then drives a short scripted session (the settings page walked with the keyboard, which visits the AI page, and an ssh connect to the
#                          fake host); NO connect()/sendto() to an inet address beyond loopback + the fake ssh host (+ APPCI_ZNS_ALLOW, comma separated) may appear in the trace
#                          (the runtime network gate, crates/network_gate: docs/testing.md "Network gate")
#   agent_plugins_offline  Linux: the same for the plugins bundled for Codex, Gemini CLI and OpenCode (fakecodex.py / fakegemini.py install the bundled folder; OpenCode's dist/ is run
#                          with node); the hooks run under a pty because they write to /dev/tty; the app must log the Codex and OpenCode events (Gemini's notifications are opt-in)
#   claude_plugin_offline  Linux: with the network namespace unshared (`unshare -rn`), the plugin bundled in the build is registered as a local marketplace and installed with
#                          fakeclaude.py, and its real hook scripts produce the prompt / tool / permission / stop events; a fake `claude` in a pane prints those
#                          sequences and the app must log one 'CLI agent event received' line per event, with no prompt text in the log
import json, os, pty, re, select, shutil, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402  (cases.py imports this module as its last line)
from cases import case  # noqa: E402
import cases_robust as R  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", ".."))

# strace line forms for an inet address: connect(5, {sa_family=AF_INET, sin_port=htons(443), sin_addr=inet_addr("1.2.3.4")}, 16) = ...
#                                         sendto(5, "...", 40, MSG_NOSIGNAL, {sa_family=AF_INET6, sin6_port=htons(53), sin6_addr=inet_pton(AF_INET6, "2001:db8::1", ...), ...}, 28) = 40
ATTEMPT = re.compile(r"\b(connect|sendto|sendmsg)\(.*?sa_family=(AF_INET6?)\b.*?(?:sin6?_port=htons\((\d+)\)).*?(?:inet_addr\(\"([^\"]+)\"\)|inet_pton\(AF_INET6, \"([^\"]+)\")")
HOST_ONLY = re.compile(r"^(127\.|0\.0\.0\.0$|::1$|::$|::ffff:127\.)")


def outbound_attempts(trace_text, allow=()):
    """-> [(syscall, address, port)] for every connect()/sendto()/sendmsg() in an `strace -f -e trace=network` text whose inet address is NOT loopback, unspecified or in
    `allow` (the destinations the scripted session is expected to reach, e.g. the fake ssh host).
    AF_UNIX / AF_NETLINK lines never match (no sa_family=AF_INET)."""
    out = []
    for line in trace_text.splitlines():
        m = ATTEMPT.search(line)
        if not m:
            continue
        addr = m.group(4) or m.group(5) or ""
        if not HOST_ONLY.match(addr) and addr not in allow:
            out.append((m.group(1), addr, int(m.group(3))))
    return out


def strace_works():
    if not shutil.which("strace"):
        return False
    try:
        return subprocess.run(["strace", "-f", "-o", os.devnull, "true"], capture_output=True, timeout=20).returncode == 0
    except Exception:
        return False


@case("zero_network_startup", modes=("terminal",), order=84, gui=True)
def c_zero_network_startup(ctx, r):
    """default settings + idle: the app makes no outbound connection from launch through APPCI_ZNS_IDLE_S seconds of idling (loopback allowed)"""
    R.need_linux_app(ctx, "zero_network_startup")
    if not strace_works():
        ctx.skip_reason = "strace missing or ptrace not permitted on this runner"
        raise cases.Skip("zero_network_startup needs a working `strace -f` (not available here); the check is NOT passed")
    idle = float(os.environ.get("APPCI_ZNS_IDLE_S", "120"))
    trace = os.path.join(ctx.run if hasattr(ctx, "run") else ctx.W, "zns.strace")
    try:
        os.remove(trace)
    except OSError:
        pass
    ctx.app.wrap = "strace -f -qq -e trace=network -s 48 -o '%s' " % trace
    try:
        _, up = R.start_app(ctx, r, "zns_traced")
        if not up:
            return
        t0 = time.time()
        while time.time() - t0 < idle:
            time.sleep(min(5.0, idle - (time.time() - t0)))
            r.check(ctx.app.alive(), "the app exited while idling")
            if not ctx.app.alive():
                break
        allow = scripted_session(ctx, r)
    finally:
        ctx.app.wrap = ""
        ctx.app.stop()
    text = open(trace, errors="replace").read() if os.path.exists(trace) else ""
    r.check(bool(text), "the trace is empty: strace did not follow the app (no socket call at all is implausible)")
    bad = outbound_attempts(text, allow)
    r.metric("network_syscalls_traced", len(text.splitlines()))
    r.metric("outbound_attempts", len(bad))
    r.check(not bad, "the app tried to reach the network at startup/idle: %s" % "; ".join("%s %s:%d" % b for b in bad[:8]))
    R.start_app(ctx, r, "zns_restore")                      # leave a normally-launched app for the cases that follow


def session_allowlist(ctx):
    """addresses the scripted session may reach beyond loopback: APPCI_ZNS_ALLOW plus the fake ssh host's address when it is an IP literal"""
    allow = {a.strip() for a in os.environ.get("APPCI_ZNS_ALLOW", "").split(",") if a.strip()}
    host = (getattr(ctx, "ssh", None) or {}).get("host") if isinstance(getattr(ctx, "ssh", None), dict) else None
    if isinstance(host, str) and re.match(r"^[0-9a-fA-F:.]+$", host):
        allow.add(host)
    return allow


def scripted_session(ctx, r):
    """a short user session under the trace: open the settings page and walk its pages with the keyboard (the AI page among them), close it, then ssh to the fake host
    and back. Returns the extra addresses the trace may contain. Every step is best effort EXCEPT the final liveness check; the network assertion is the caller's."""
    import cases_coverage_b as CB
    import cases_caseremote as CR
    allow = session_allowlist(ctx)
    key = CB.SETTINGS_KEY["mac" if ctx.be.name == "mac" else "linux"]
    ctx.reset()
    time.sleep(0.3)
    ctx.be.key(key)
    time.sleep(1.5)
    for i in range(int(os.environ.get("APPCI_ZNS_PAGES", "12"))):         # nav list: Down moves to the next page; the AI page is one of them
        ctx.be.key("Down")
        time.sleep(0.4)
        if i in (3, 6, 9):
            ctx.shot("zns_settings_%d" % i)
    ctx.be.key("Escape")
    time.sleep(0.8)
    r.note("scripted session: settings page walked with the keyboard (best effort; shots/zns_settings_*), allowlist beyond loopback: %s" % (sorted(allow) or "none"))
    if getattr(ctx, "ssh", None):
        secs = CR.connect(ctx, "fake", 60)
        r.check(secs is not None, "the scripted ssh connect to the fake host did not reach a remote prompt")
        ctx.be.type("exit", 12, enter=True)
        time.sleep(1.0)
    else:
        r.note("no local ssh server: the ssh part of the scripted session was skipped (%s)" % (getattr(ctx, "ssh_why", None) or "unavailable"))
    r.check(ctx.app.alive(), "the app exited during the scripted session")
    return allow


def bundle_dir(ctx):
    """the plugin folder shipped with the build under test: APPCI_PLUGIN_DIR, else <binary dir>/resources/bundled/claude_plugin, else the checkout's copy"""
    for d in (os.environ.get("APPCI_PLUGIN_DIR"),
              os.path.join(os.path.dirname(os.path.realpath(getattr(ctx.app, "bin", "") or "/nonexistent")), "resources", "bundled", "claude_plugin"),
              os.path.join(REPO, "resources", "bundled", "claude_plugin")):
        if d and os.path.isfile(os.path.join(d, ".claude-plugin", "marketplace.json")):
            return d
    return None


def netns_prefix():
    """argv prefix that runs a command with no network, or None when the runner cannot unshare"""
    try:
        if shutil.which("unshare") and subprocess.run(["unshare", "-rn", "true"], capture_output=True, timeout=20).returncode == 0:
            return ["unshare", "-rn"]
    except Exception:
        pass
    return None


# the hook inputs Claude Code gives the plugin's scripts, and the event each must produce
SECRET = "zq7 prompt; with semicolon ééé " + "x" * 300
HOOKS = [("on-prompt-submit.sh", "PromptSubmit", {"prompt": SECRET}),
         ("on-post-tool-use.sh", "ToolComplete", {"tool_name": "Bash", "tool_input": {"command": "ls"}}),
         ("on-permission-request.sh", "PermissionRequest", {"tool_name": "Bash", "tool_input": {"command": "rm -rf build"}}),
         ("on-stop.sh", "Stop", {"stop_hook_active": False, "transcript_path": ""})]


def run_plugin_offline(work, bundle, claude_cfg, prefix):
    """-> (problems, events_file): register + install the bundle with fakeclaude.py and run its hook scripts, all inside `prefix` (no network).
    Pure of the app, so the self-test can run it."""
    env = dict(os.environ, CLAUDE_CONFIG_DIR=claude_cfg, WARP_CLI_AGENT_PROTOCOL_VERSION="1", WARP_CLIENT_VERSION="local", CLAUDE_CODE_VERSION="2.1.141")
    fake = [sys.executable, "-I", os.path.join(HERE, "fakeclaude.py")]
    problems = []
    for args in (["plugin", "marketplace", "add", bundle], ["plugin", "install", "warp@claude-code-warp"]):
        p = subprocess.run(prefix + fake + args, env=env, capture_output=True, text=True, timeout=60)
        if p.returncode:
            problems.append("fakeclaude %s failed: %s" % (" ".join(args[:3]), p.stderr.strip()))
    events = os.path.join(work, "plugin_events.bin")
    open(events, "wb").close()
    try:
        installed = json.load(open(os.path.join(claude_cfg, "plugins", "installed_plugins.json")))["plugins"]["warp@claude-code-warp"][0]["installPath"]
    except Exception as e:
        return problems + ["the plugin is not in installed_plugins.json after the install (%r)" % e], events
    for script, _event, extra in HOOKS:
        stdin = json.dumps(dict({"session_id": "zns-session", "cwd": work}, **extra))
        p = subprocess.run(prefix + [os.path.join(installed, "scripts", script)], input=stdin, env=env, capture_output=True, text=True, timeout=60)
        try:
            seq = json.loads(p.stdout)["terminalSequence"]
        except Exception:
            problems.append("%s printed no terminalSequence (rc %s, stdout %r, stderr %r)" % (script, p.returncode, p.stdout[:200], p.stderr[:200]))
            continue
        with open(events, "ab") as f:
            f.write(seq.encode("utf-8"))
    return problems, events


@case("claude_plugin_offline", modes=("terminal",), order=86, gui=True)
def c_claude_plugin_offline(ctx, r):
    """the bundled Claude plugin installs and works with the network blocked, and the app parses the events it emits"""
    R.need_linux_app(ctx, "claude_plugin_offline")
    if not shutil.which("jq") or not shutil.which("bash"):
        raise cases.Skip("the plugin's hook scripts need bash and jq on the runner")
    bundle = bundle_dir(ctx)
    r.check(bundle is not None, "no bundled plugin folder (resources/bundled/claude_plugin) beside the binary or in the checkout")
    if not bundle:
        return
    prefix = netns_prefix()
    if prefix is None:
        prefix = []
        r.no_reference("`unshare -rn` is not permitted on this runner: the install and hooks ran WITH network access, so 'works offline' is not proven")
    r.note("bundle: %s; network namespace: %s" % (bundle, "unshared" if prefix else "NOT unshared"))
    work = os.path.join(ctx.W, "plugin_offline")
    shutil.rmtree(work, ignore_errors=True)
    os.makedirs(work)
    problems, events = run_plugin_offline(work, bundle, os.path.join(work, "claude_cfg"), prefix)
    for p in problems:
        r.check(False, p)
    if problems:
        return
    # a fake `claude` in the pane prints the recorded sequences (the app starts the CLI agent session from the command name)
    fakebin = os.path.join(work, "bin")
    os.makedirs(fakebin)
    with open(os.path.join(fakebin, "claude"), "w") as f:
        f.write("#!/bin/bash\nsleep 1\ncat '%s'\nsleep 2\n" % events)
    os.chmod(os.path.join(fakebin, "claude"), 0o755)
    off = ctx.log_size()
    ctx.be.type("export PATH='%s':$PATH" % fakebin, 15, enter=True)
    time.sleep(0.5)
    ctx.be.type("claude", 15, enter=True)
    want = [e for _s, e, _x in HOOKS]
    got = lambda: re.findall(r"CLI agent event received: agent=\w+ event=(\w+) body_bytes=\d+", ctx.log_since(off))
    ctx.until(lambda: len(got()) >= len(want), 40, step=0.5)
    r.check(got()[:len(want)] == want, "events parsed by the app: %r, expected %r (a missing event means the sequence was dropped or the CLI agent session did not start)" % (got(), want))
    log = ctx.log_since(off)
    r.check("Dropped a CLI agent event" not in log, "the app logged a parse failure for a plugin event")
    r.check("zq7 prompt" not in log, "the prompt text reached the app log")
    ctx.shot("claude_plugin_offline")


# ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------
# agent_plugins_offline: the plugins bundled for Codex, Gemini CLI and OpenCode (owner decision 2026-11-09: all four agent plugins ship with the app, nothing is fetched)

def plugin_bundle_dir(ctx, agent, marker):
    """the folder resources/bundled/<agent>_plugin of the build under test (APPCI_<AGENT>_PLUGIN_DIR, else beside the binary, else the checkout's), or None"""
    for d in (os.environ.get("APPCI_%s_PLUGIN_DIR" % agent.upper()),
              os.path.join(os.path.dirname(os.path.realpath(getattr(ctx.app, "bin", "") or "/nonexistent")), "resources", "bundled", agent + "_plugin"),
              os.path.join(REPO, "resources", "bundled", agent + "_plugin")):
        if d and os.path.isfile(os.path.join(d, *marker)):
            return d
    return None


def run_in_pty(argv, stdin_text, env, cwd, timeout=60):
    """-> (returncode, output bytes). argv runs with stdin from a file and the pty as its controlling terminal and stdout/stderr, so a hook that writes to /dev/tty is captured"""
    stdin_file = os.path.join(cwd, "pty_stdin.json")
    with open(stdin_file, "w") as f:
        f.write(stdin_text)
    pid, fd = pty.fork()
    if pid == 0:
        try:
            os.chdir(cwd)
            os.execvpe("sh", ["sh", "-c", 'f=$1; shift; exec "$@" <"$f"', "sh", stdin_file] + list(argv), env)
        finally:
            os._exit(127)
    out = b""
    deadline = time.time() + timeout
    while time.time() < deadline:
        ready, _, _ = select.select([fd], [], [], 0.5)
        if ready:
            try:
                chunk = os.read(fd, 65536)
            except OSError:
                break
            if not chunk:
                break
            out += chunk
    try:
        _, status = os.waitpid(pid, os.WNOHANG)
    except ChildProcessError:
        status = 0
    try:
        os.close(fd)
    except OSError:
        pass
    return (os.waitstatus_to_exitcode(status) if status else 0), out


SEQUENCE = re.compile(rb"\x1b\]777;notify;warp://cli-agent;.*?\x07", re.S)

CODEX_HOOKS = [("on-session-start.sh", "SessionStart", {}),
               ("on-prompt-submit.sh", "PromptSubmit", {"prompt": SECRET}),
               ("on-post-tool-use.sh", "ToolComplete", {"tool_name": "Bash"}),
               ("on-permission-request.sh", "PermissionRequest", {"tool_name": "Bash", "tool_input": {"command": "rm -rf build"}}),
               ("on-stop.sh", "Stop", {"last_assistant_message": "done " + "y" * 300})]
GEMINI_HOOKS = [("on-session-start.sh", "SessionStart", {}),
                ("on-prompt-submit.sh", "PromptSubmit", {"prompt": SECRET}),
                ("on-post-tool-use.sh", "ToolComplete", {"tool_name": "run_shell_command"}),
                ("on-notification.sh", "PermissionRequest", {"notification_type": "ToolPermission", "details": {"rootCommand": "rm", "command": "rm -rf build"}}),
                ("on-stop.sh", "Stop", {"prompt": SECRET, "prompt_response": "done"})]
OPENCODE_EVENTS = [("session_start", "SessionStart", {"plugin_version": "0.2.0"}),
                   ("prompt_submit", "PromptSubmit", {"query": "a question"}),
                   ("tool_complete", "ToolComplete", {"tool_name": "bash"}),
                   ("permission_request", "PermissionRequest", {"summary": "Wants to run bash", "tool_name": "bash", "tool_input": {"command": "ls"}}),
                   ("stop", "Stop", {"query": "a question", "response": "done"})]


def hook_env(work, extra):
    return dict(os.environ, WARP_CLI_AGENT_PROTOCOL_VERSION="1", WARP_CLIENT_VERSION="local", **extra)


def run_hooks(prefix, scripts_dir, hooks, env, work, problems, events):
    for script, _event, extra in hooks:
        stdin = json.dumps(dict({"session_id": "zns-session", "cwd": work}, **extra))
        rc, out = run_in_pty(prefix + [os.path.join(scripts_dir, script)], stdin, env, work)
        seq = SEQUENCE.findall(out)
        if len(seq) != 1:
            problems.append("%s wrote %d notification sequences to the terminal (rc %s, output %r)" % (script, len(seq), rc, out[:200]))
            continue
        with open(events, "ab") as f:
            f.write(seq[0])


def run_agent_plugin_offline(agent, work, bundle, prefix):
    """-> (problems, events_file, expected_event_names): install the bundled `agent` plugin with the fake CLI and run its real hooks / modules, all inside `prefix` (no network).
    Pure of the app, so the self-test can run it."""
    problems = []
    events = os.path.join(work, "%s_events.bin" % agent)
    open(events, "wb").close()
    if agent == "codex":
        home = os.path.join(work, "codex_home")
        env = hook_env(work, {"CODEX_HOME": home})
        fake = [sys.executable, "-I", os.path.join(HERE, "fakecodex.py")]
        for args in (["plugin", "marketplace", "add", bundle], ["plugin", "add", "warp@codex-warp"]):
            p = subprocess.run(prefix + fake + args, env=env, capture_output=True, text=True, timeout=60)
            if p.returncode:
                problems.append("fakecodex %s failed: %s" % (" ".join(args[:3]), p.stderr.strip()))
        scripts = os.path.join(home, "plugins", "cache", "codex-warp", "warp", json.load(open(os.path.join(bundle, "plugins", "warp", ".codex-plugin", "plugin.json")))["version"], "scripts")
        if not os.path.isdir(scripts):
            return problems + ["the plugin is not in the Codex plugin cache after `plugin add`"], events, []
        run_hooks(prefix, scripts, CODEX_HOOKS, env, work, problems, events)
        return problems, events, [e for _s, e, _x in CODEX_HOOKS]
    if agent == "gemini":
        home = os.path.join(work, "gemini_home")
        os.makedirs(home, exist_ok=True)
        env = hook_env(work, {"GEMINI_CLI_HOME": home})
        fake = [sys.executable, "-I", os.path.join(HERE, "fakegemini.py")]
        p = subprocess.run(prefix + fake + ["extensions", "install", bundle, "--consent"], env=env, capture_output=True, text=True, timeout=60)
        if p.returncode:
            problems.append("fakegemini install failed: %s" % p.stderr.strip())
        scripts = os.path.join(home, ".gemini", "extensions", "gemini-warp", "scripts")
        if not os.path.isdir(scripts):
            return problems + ["the extension is not in ~/.gemini/extensions after the install"], events, []
        run_hooks(prefix, scripts, GEMINI_HOOKS, env, work, problems, events)
        return problems, events, [e for _s, e, _x in GEMINI_HOOKS]
    # opencode: no CLI command installs it (a file:// entry in the global config does), so the bundled modules the plugin uses to emit its notifications are run with node
    if not shutil.which("node"):
        return problems + ["node is not on PATH"], events, []
    js = ("import {warpNotify} from %s; import {buildPayload} from %s; "
          "for (const [ev, extra] of JSON.parse(process.argv[1])) warpNotify('warp://cli-agent', buildPayload(ev, 'zns-session', process.cwd(), extra));"
          % (json.dumps("file://" + os.path.join(bundle, "dist", "notify.js")), json.dumps("file://" + os.path.join(bundle, "dist", "payload.js"))))
    env = hook_env(work, {})
    rc, out = run_in_pty(prefix + ["node", "--input-type=module", "-e", js, json.dumps([[ev, x] for ev, _n, x in OPENCODE_EVENTS])], "", env, work)
    seq = SEQUENCE.findall(out)
    if len(seq) != len(OPENCODE_EVENTS):
        problems.append("the OpenCode plugin wrote %d of %d notification sequences (rc %s, output %r)" % (len(seq), len(OPENCODE_EVENTS), rc, out[:300]))
    with open(events, "ab") as f:
        for one in seq:
            f.write(one)
    return problems, events, [e for _ev, e, _x in OPENCODE_EVENTS]


PLUGIN_MARKERS = {"codex": (".agents", "plugins", "marketplace.json"), "gemini": ("gemini-extension.json",), "opencode": ("package.json",)}
APP_PARSED = ("codex", "opencode")        # Gemini CLI notifications are an opt-in feature flag, so the app ignores that agent's events by default
APP_NAME = {"codex": "Codex", "opencode": "OpenCode"}


@case("agent_plugins_offline", modes=("terminal",), order=87, gui=True)
def c_agent_plugins_offline(ctx, r):
    """the bundled Codex, Gemini CLI and OpenCode plugins install and work with the network blocked, and the app parses the events Codex and OpenCode emit"""
    R.need_linux_app(ctx, "agent_plugins_offline")
    if not shutil.which("jq") or not shutil.which("bash"):
        raise cases.Skip("the plugins' hook scripts need bash and jq on the runner")
    prefix = netns_prefix()
    if prefix is None:
        prefix = []
        r.no_reference("`unshare -rn` is not permitted on this runner: the installs and hooks ran WITH network access, so 'works offline' is not proven")
    work = os.path.join(ctx.W, "agent_plugins_offline")
    shutil.rmtree(work, ignore_errors=True)
    os.makedirs(work)
    results = {}
    for agent in ("codex", "gemini", "opencode"):
        bundle = plugin_bundle_dir(ctx, agent, PLUGIN_MARKERS[agent])
        r.check(bundle is not None, "no bundled plugin folder (resources/bundled/%s_plugin) beside the binary or in the checkout" % agent)
        if not bundle:
            continue
        if agent == "opencode" and not shutil.which("node"):
            r.no_reference("node is not on the runner: the OpenCode plugin's modules were not run")
            continue
        sub = os.path.join(work, agent)
        os.makedirs(sub)
        r.note("%s bundle: %s; network namespace: %s" % (agent, bundle, "unshared" if prefix else "NOT unshared"))
        problems, events, expected = run_agent_plugin_offline(agent, sub, bundle, prefix)
        for p in problems:
            r.check(False, "%s: %s" % (agent, p))
        if not problems:
            results[agent] = (events, expected)
    if not any(a in results for a in APP_PARSED):
        return
    fakebin = os.path.join(work, "bin")
    os.makedirs(fakebin)
    for agent in APP_PARSED:
        if agent in results:
            with open(os.path.join(fakebin, agent), "w") as f:
                f.write("#!/bin/bash\nsleep 1\ncat '%s'\nsleep 2\n" % results[agent][0])
            os.chmod(os.path.join(fakebin, agent), 0o755)
    ctx.be.type("export PATH='%s':$PATH" % fakebin, 15, enter=True)
    time.sleep(0.5)
    for agent in APP_PARSED:
        if agent not in results:
            continue
        off = ctx.log_size()
        ctx.be.type(agent, 15, enter=True)
        want = results[agent][1]
        got = lambda: re.findall(r"CLI agent event received: agent=%s event=(\w+) body_bytes=\d+" % APP_NAME[agent], ctx.log_since(off))
        ctx.until(lambda: len(got()) >= len(want), 40, step=0.5)
        r.check(got()[:len(want)] == want, "%s: events parsed by the app: %r, expected %r (a missing event means the sequence was dropped or the CLI agent session did not start)" % (agent, got(), want))
        log = ctx.log_since(off)
        r.check("Dropped a CLI agent event" not in log, "%s: the app logged a parse failure for a plugin event" % agent)
        r.check("zq7 prompt" not in log, "%s: the prompt text reached the app log" % agent)
        ctx.shot("agent_plugins_offline_%s" % agent)
        time.sleep(3)           # let the fake agent exit before the next one starts


# ------------------------------------------------------------------ network_gate_switch
GATE_OFF_LOG = "network gate: turned OFF"
GATE_ON_LOG = "network gate: turned ON"


def gate_log_lines(text):
    """the gate's own switch lines of an app log (the log never names a host: a host hash at most)"""
    return [l for l in text.splitlines() if "network gate:" in l]


@case("network_gate_switch", modes=("terminal",), order=88, gui=True)
def c_network_gate_switch(ctx, r):
    """Settings > Network > Network gate switch (`network.gate_enabled`, default true). Absent key: no 'turned OFF' line. `gate_enabled = false` in settings.toml:
    the next launch logs 'network gate: turned OFF' and a later launch with the key true logs no OFF line. The app log never contains the host of the allowed-origin
    entry the case writes. The denial notice, its 'Allow this origin' link and the Settings > Network log are UNVERIFIABLE here (see the note): no setting can make
    the app issue a request the gate refuses, because every setting that names an origin also allows it."""
    R.need_linux_app(ctx, "network_gate_switch")
    marker_host = "gate-switch-%s.example.invalid" % ctx.tok(5).lower()
    try:
        # 1. gate_enabled absent (the v0.1.17 file shape): the gate is on, nothing is logged about the switch.
        q = R.stop_for_restart(ctx, r, 20, "ngs_absent")
        r.check(q["exited"], "ngs_absent: the app did not exit within 20 s")
        p = R.settings_file(ctx)
        text = open(p, encoding="utf-8", errors="replace").read()
        r.check(R.T.get_key(text, "network", "gate_enabled") is None, "the profile's settings.toml already carries network.gate_enabled before the case wrote it")
        open(p, "w", encoding="utf-8").write(R.T.set_key(text, "network", "allowed_origins", '"%s"' % marker_host))
        _, up = R.start_app(ctx, r, "ngs_absent")
        if not up:
            return
        log = ctx.log_since(0)
        r.check(GATE_OFF_LOG not in log, "the gate was turned OFF with no gate_enabled key in settings.toml (absent must mean on)")
        r.check(marker_host not in log, "the app log names the allowed origin's host (the log may carry a host hash only)")
        # 2. gate_enabled = false: the next launch turns the gate off and says so once.
        q = R.stop_for_restart(ctx, r, 20, "ngs_off")
        r.check(q["exited"], "ngs_off: the app did not exit within 20 s")
        open(p, "w", encoding="utf-8").write(R.T.set_key(open(p, encoding="utf-8", errors="replace").read(), "network", "gate_enabled", "false"))
        _, up = R.start_app(ctx, r, "ngs_off")
        if not up:
            return
        ctx.shot("ngs_gate_off")
        log = ctx.log_since(0)
        r.check(log.count(GATE_OFF_LOG) == 1, "gate_enabled=false: expected exactly one '%s' line, saw %d" % (GATE_OFF_LOG, log.count(GATE_OFF_LOG)))
        r.check(marker_host not in log, "the app log names the allowed origin's host with the gate off")
        # 3. back to true: no OFF line on the next launch.
        q = R.stop_for_restart(ctx, r, 20, "ngs_on")
        r.check(q["exited"], "ngs_on: the app did not exit within 20 s")
        open(p, "w", encoding="utf-8").write(R.T.set_key(open(p, encoding="utf-8", errors="replace").read(), "network", "gate_enabled", "true"))
        _, up = R.start_app(ctx, r, "ngs_on")
        if not up:
            return
        log = ctx.log_since(0)
        r.check(GATE_OFF_LOG not in log, "gate_enabled=true: the launch still logged '%s'" % GATE_OFF_LOG)
        r.metric("gate_lines_last_launch", len(gate_log_lines(log)))
        r.unverifiable("the denial notice (toast naming the purpose and host), its 'Allow this origin' link, the Settings > Network log row and 'Allow' button: the harness has no "
                       "trigger for a refused request (models_dev_url and provider URLs are configured origins, so they are allowed; the Network page probe only reaches its fixed host) "
                       "and no verified keyboard path to the Network page widgets. Covered by the unit tests in crates/network_gate, app/src/network_gate_log_tests.rs, "
                       "app/src/network_gate_notice_tests.rs and app/src/settings_view/network_page_tests.rs")
    finally:
        try:
            p = R.settings_file(ctx)
            t = open(p, encoding="utf-8", errors="replace").read()
            t = R.T.set_key(t, "network", "gate_enabled", "true")
            open(p, "w", encoding="utf-8").write(R.T.set_key(t, "network", "allowed_origins", '""'))
        except Exception:
            pass
