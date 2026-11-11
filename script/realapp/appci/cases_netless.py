# appci/cases_netless.py: Wave N, "no network at startup" (docs/testing.md 2.17). Registered with the same @case decorator as cases.py (imported by its last line).
# Round 15 (#2076 T41, #2077): zero_network_startup also runs on macOS and Windows with a polled per-process connection table (nettrace.py) and a
# control connection that proves the tool on the machine; network_gate_switch runs on macOS and in the Windows Git Bash pane; the two plugin cases run
# on macOS with `sandbox-exec` denying the network, and on every platform prove that their no-network prefix really blocks before they claim "offline".
#   zero_network_startup   Linux: the app, with the harness profile (no cloud account, no provider key), is launched under `strace -f` and left idle for APPCI_ZNS_IDLE_S
#                          (default 120 s), then drives a short scripted session (the settings page walked with the keyboard, which visits the AI page, and an ssh connect to the
#                          fake host); NO connect()/sendto() to an inet address beyond loopback + the fake ssh host (+ APPCI_ZNS_ALLOW, comma separated) may appear in the trace
#                          (the runtime network gate, crates/network_gate: docs/testing.md "Network gate")
#   agent_plugins_offline  Linux: the same for the plugins bundled for Codex, Gemini CLI and OpenCode (fakecodex.py / fakegemini.py install the bundled folder; OpenCode's dist/ is run
#                          with node); the hooks run under a pty because they write to /dev/tty; the app must log the Codex and OpenCode events (Gemini's notifications are opt-in)
#   claude_plugin_offline  Linux: with the network namespace unshared (`unshare -rn`), the plugin bundled in the build is registered as a local marketplace and installed with
#                          fakeclaude.py, and its real hook scripts produce the prompt / tool / permission / stop events; a fake `claude` in a pane prints those
#                          sequences and the app must log one 'CLI agent event received' line per event, with no prompt text in the log
import json, os, re, select, shutil, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402  (cases.py imports this module as its last line)
from cases import case  # noqa: E402
import cases_robust as R  # noqa: E402
import ci_env  # noqa: E402

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


def provider_addresses():
    """addresses of the model endpoint the harness profile configures (the CI Ollama, machine-local): what the app may contact AFTER the user runs a command (prompt /
    next-command suggestions use the configured model; the user configured it, so that is opt-in traffic). Empty when none is configured or it does not resolve."""
    import socket
    from urllib.parse import urlparse
    url = ci_env.get("APPCI_OLLAMA_URL")
    host = urlparse(url if "://" in url else "http://" + url).hostname if url else None
    if not host:
        return set()
    try:
        return {ai[4][0] for ai in socket.getaddrinfo(host, None)}
    except OSError:
        return {host}


def fingerprint(addr, port):
    """a short hash of 'addr:port' so a failing run's artifacts can say WHICH destination without carrying the address (owner endpoints never reach artifacts)"""
    import hashlib
    return hashlib.sha256(("%s:%d" % (addr, port)).encode()).hexdigest()[:10]


def strace_works():
    if not shutil.which("strace"):
        return False
    try:
        return subprocess.run(["strace", "-f", "-o", os.devnull, "true"], capture_output=True, timeout=20).returncode == 0
    except Exception:
        return False


@case("zero_network_startup", modes=("terminal",), order=84, gui=True)
def c_zero_network_startup(ctx, r):
    """the pass profile + idle: the app makes no outbound connection from launch through APPCI_ZNS_IDLE_S seconds of idling (loopback allowed).
    NOT the app's default settings (round 16: this said "default settings"): the launch is on the pass's own settings file, the owner seed with the
    harness overrides, which names a model provider when one is configured (case_settings.py: reads `providers`, needs the network gate on).
    Linux: `strace -f` (every connect / sendto / sendmsg). macOS and Windows: the per-process connection table polled (zero_network_polled)."""
    import nettrace
    if nettrace.platform_kind(ctx):
        return zero_network_polled(ctx, r, nettrace.platform_kind(ctx))
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
        idle_end = len(open(trace, errors="replace").read().splitlines()) if os.path.exists(trace) else 0
        allow = scripted_session(ctx, r)
    finally:
        ctx.app.wrap = ""
        ctx.app.stop()
    text = open(trace, errors="replace").read() if os.path.exists(trace) else ""
    r.check(bool(text), "the trace is empty: strace did not follow the app (no socket call at all is implausible)")
    lines = text.splitlines()
    # launch + idle: NOTHING beyond loopback, no allowance. The scripted session runs commands, and with a model configured the app sends prompt / next-command
    # suggestions to THAT model (the user's own opt-in endpoint): the configured provider is allowed there and only there (r8: 4 connects to the CI Ollama).
    bad = [b + ("idle",) for b in outbound_attempts("\n".join(lines[:idle_end]), ())]
    prov = provider_addresses()
    sess_all = outbound_attempts("\n".join(lines[idle_end:]), set(allow))
    to_prov = [b for b in sess_all if b[1] in prov]
    bad += [b + ("session",) for b in sess_all if b[1] not in prov]
    r.metric("network_syscalls_traced", len(lines))
    r.metric("outbound_attempts", len(bad))
    r.metric("provider_connects_in_session", len(to_prov))
    if to_prov:
        r.note("the scripted session's commands made %d connect/send to the configured model endpoint (destination hash %s): allowed, the user configured it" % (len(to_prov), fingerprint(to_prov[0][1], to_prov[0][2])))
    r.check(not bad, "the app tried to reach the network at startup/idle: %s" % "; ".join("%s dest#%s port %d (%s)" % (b[0], fingerprint(b[1], b[2]), b[2], b[3]) for b in bad[:8]))
    R.start_app(ctx, r, "zns_restore")                      # leave a normally-launched app for the cases that follow


POLL_LIMITS = ("a connection that opens and closes between two polls (%.2f s apart on this run), a UDP datagram sent without connect(), and DNS (the resolver "
               "service makes the query, not the app) are NOT visible to this trace; the Linux strace run sees all three")


def polled_verdict(idle_rows, session_rows, provider):
    """pure: (bad, to_provider). idle_rows / session_rows: [(proto, host, port, t)] of non-local destinations first seen during launch + idle and during the
    scripted session. Launch and idle allow nothing; the session allows the configured model endpoint (`provider`, a set of addresses) and nothing else."""
    bad = [r[:3] + ("idle",) for r in idle_rows]
    to_provider = [r for r in session_rows if r[1] in provider]
    bad += [r[:3] + ("session",) for r in session_rows if r[1] not in provider]
    return bad, to_provider


def zero_network_polled(ctx, r, kind):
    """T41 (#2076): zero_network_startup where there is no strace. The OS connection table is polled for the app's processes (nettrace.py) from before
    the launch until the scripted session ends. Control first: a child of the harness holds a connection to this machine's own address and the tool
    must list it for that pid, else the case SKIPs with NEEDS (never a pass on a tool that sees nothing)."""
    import nettrace
    R.need_relaunch_app(ctx, "zero_network_startup")
    sampler = nettrace.sample_mac if kind == "mac" else nettrace.sample_windows
    ok, why = nettrace.control(sampler)
    if not ok:
        ctx.skip_reason = "the polled network trace failed its control"
        raise cases.Skip("NEEDS: `%s` must list an established TCP connection of a process of the runner user%s. Control: %s. The check is NOT passed."
                         % (nettrace.TOOL[kind], "" if ok is False else " and this machine needs a non-loopback address to make the control connection to", why))
    idle = float(os.environ.get("APPCI_ZNS_IDLE_S", "120"))
    ctx.snapshot_log()
    ctx.app.stop()
    poller = nettrace.ConnPoller(sampler, lambda: nettrace.app_pids(ctx))
    t_launch = time.time()
    poller.start()
    t_idle_end = None
    allow = set()
    try:
        # #2114 (round 14, Windows terminal@gitbash: "held a connection ... port 11434 (idle)"): on Windows start_app ends with the key proof (ctx.reprove:
        # a typed marker COMMAND, the pane's cwd, the root-pid probe). A command that ran makes the app ask the configured model for a suggestion, which
        # this case allows in its scripted session only, so the harness's own launch put a provider connection inside launch + idle. Linux and macOS
        # type nothing in a launch and passed the same round. The launch here types nothing either; the key proof runs after the idle, as the first
        # step of the session (where the provider, and only the provider, is allowed).
        reprove, ctx.reprove = getattr(ctx, "reprove", None), None
        try:
            _, up = R.start_app(ctx, r, "zns_polled")
        finally:
            ctx.reprove = reprove
        if not up:
            return
        t0 = time.time()
        while time.time() - t0 < idle:
            time.sleep(min(5.0, idle - (time.time() - t0)))
            if not r.check(ctx.app.alive(), "the app exited while idling"):
                break
        t_idle_end = time.time()
        unproven = reprove() if reprove else None
        if unproven:
            r.check(False, "zns_polled: after the idle %s (the scripted session was not played)" % unproven)
        else:
            allow = scripted_session(ctx, r)
    finally:
        poller.stop()
    t_end = time.time() + 1.0
    period = (t_end - 1.0 - t_launch) / max(1, poller.ticks)          # the tool's own run time is part of it: measured, not the requested interval
    r.metric("poll_period_s", period)
    r.check(period <= 2.0, "the connection table was read only every %.1f s on average: too coarse to call the run traced" % period)
    r.check(poller.ticks >= 4 and poller.errors == 0, "the connection table was read %d time(s) with %d error(s): the trace did not cover the run" % (poller.ticks, poller.errors))
    seen_pids = {k[0] for k in poller.seen}
    r.metric("trace", "poll %s" % nettrace.TOOL[kind])
    r.metric("polls", poller.ticks)
    r.metric("rows_seen", len(poller.seen))
    r.metric("pids_with_rows", len(seen_pids))
    idle_rows = poller.outbound_between(t_launch, t_idle_end or t_end)
    sess_rows = poller.outbound_between(t_idle_end or t_end, t_end, allow=set(allow))
    bad, to_prov = polled_verdict(idle_rows, sess_rows, provider_addresses())
    r.metric("outbound_attempts", len(bad))
    r.metric("provider_connects_in_session", len(to_prov))
    if to_prov:
        r.note("the scripted session's commands made %d connection(s) to the configured model endpoint (destination hash %s): allowed, the user configured it" % (len(to_prov), fingerprint(to_prov[0][1], to_prov[0][2])))
    r.note("trace=poll (%s, control connection listed): %s" % (nettrace.TOOL[kind], POLL_LIMITS % period))
    r.check(not bad, "the app held a connection to the network at startup/idle: %s" % "; ".join("%s dest#%s port %d (%s)" % (b[0], fingerprint(b[1], b[2]), b[2], b[3]) for b in bad[:8]))


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
    # Escape does not close the settings TAB (the same fact cases_coverage_b settings_open_close handles): keys typed next went to the settings page,
    # and the ssh connect below never reached the shell (round 7 Linux: app log keydowns with live_focus=no). Prove the shell is in front with a probe
    # line; if not, close the settings tab with the platform's close and prove it again.
    shell_back_in_front(ctx, r, "zns")
    ctx.reset()
    r.note("scripted session: settings page walked with the keyboard (best effort; shots/zns_settings_*), allowlist beyond loopback: %s" % (sorted(allow) or "none"))
    if getattr(ctx, "ssh", None):
        secs = CR.connect(ctx, "fake", 60)
        r.check(secs is not None, "the scripted ssh connect to the fake host did not reach a remote prompt")
        ctx.exit_nested(None, r, "scripted ssh")           # audit #2: `exit` only while a probe proves the remote shell is in front (never when the connect failed)
    else:
        r.note("no local ssh server: the ssh part of the scripted session was skipped (%s)" % (getattr(ctx, "ssh_why", None) or "unavailable"))
    r.check(ctx.app.alive(), "the app exited during the scripted session")
    return allow


def shell_back_in_front(ctx, r, tag):
    """after a settings page was shown: prove the shell takes typing with a probe line; if it does not, close the page with the platform's close
    (cases.editor_close: the close key, on macOS the palette's 'Close Current Session') and prove it again. -> whether the shell is in front."""
    probe_f = "%s_probe.txt" % tag

    def shell_in_front(n):
        ctx.rm(probe_f)
        ctx.be.type(ctx.append_cmd(ctx.pane_shell(), "%s%d" % (tag, n), probe_f), 12, enter=True)
        return bool(ctx.until(lambda: any("%s%d" % (tag, n) in l for l in ctx.lines(probe_f)), 3))
    if shell_in_front(1):
        return True
    how = cases.editor_close(ctx, r)
    r.note("Escape left the settings page open (the shell did not take the probe line); closed it with %s" % how)
    time.sleep(0.8)
    if how is None:
        return False
    return bool(r.check(shell_in_front(2), "the shell pane did not take typing after the settings page was closed with %s" % how))


def bundle_roots(ctx):
    """directories that may hold the build's `resources/bundled` tree: beside the binary (Linux, Windows), inside the macOS bundle
    (Contents/Resources, with and without a `resources` level), then the checkout"""
    exe = os.path.dirname(os.path.realpath(getattr(ctx.app, "bin", "") or "/nonexistent"))
    roots = [os.path.join(exe, "resources", "bundled")]
    b = getattr(ctx.app, "bundle", None)
    if b:
        roots += [os.path.join(b, "Contents", "Resources", "bundled"), os.path.join(b, "Contents", "Resources", "resources", "bundled")]
    return roots + [os.path.join(REPO, "resources", "bundled")]


def bundle_dir(ctx):
    """the plugin folder shipped with the build under test: APPCI_PLUGIN_DIR, else <binary dir>/resources/bundled/claude_plugin, else the checkout's copy"""
    for d in [os.environ.get("APPCI_PLUGIN_DIR")] + [os.path.join(root, "claude_plugin") for root in bundle_roots(ctx)]:
        if d and os.path.isfile(os.path.join(d, ".claude-plugin", "marketplace.json")):
            return d
    return None


SANDBOX_NO_NET = "(version 1)(allow default)(deny network*)"


def netns_prefix(platform=None):
    """argv prefix that runs a command with no network, or None when the runner has none. Linux: `unshare -rn` (an empty network namespace).
    macOS: `sandbox-exec -p '(version 1)(allow default)(deny network*)'` (no root needed). The prefix is only a candidate until
    prefix_blocks_network() has seen it refuse a connection."""
    platform = platform or sys.platform
    try:
        if platform == "darwin":
            if shutil.which("sandbox-exec") and subprocess.run(["sandbox-exec", "-p", SANDBOX_NO_NET, "true"], capture_output=True, timeout=20).returncode == 0:
                return ["sandbox-exec", "-p", SANDBOX_NO_NET]
            return None
        if shutil.which("unshare") and subprocess.run(["unshare", "-rn", "true"], capture_output=True, timeout=20).returncode == 0:
            return ["unshare", "-rn"]
    except Exception:
        pass
    return None


BLOCK_PROBE = ("import socket,sys\n"
               "try:\n"
               "    socket.create_connection(('127.0.0.1', int(sys.argv[1])), timeout=5).close()\n"
               "except OSError:\n"
               "    sys.exit(7)\n"
               "sys.exit(0)\n")


def prefix_blocks_network(prefix):
    """the control of the two offline cases: a connect to a loopback listener of this process, made INSIDE `prefix`, must fail, and the same connect
    made without the prefix must succeed (so a failure is the prefix's doing, not a broken probe). -> (blocks, why)"""
    import socket
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        srv.bind(("127.0.0.1", 0))
        srv.listen(4)
        port = str(srv.getsockname()[1])
        probe = [sys.executable, "-I", "-c", BLOCK_PROBE, port]
        plain = subprocess.run(probe, capture_output=True, timeout=30).returncode
        if plain != 0:
            return False, "the probe could not connect to a loopback listener even without the prefix (exit %s): nothing is proven" % plain
        inside = subprocess.run(list(prefix) + probe, capture_output=True, timeout=30).returncode
        if inside == 7:
            return True, "a connect made inside the prefix was refused"
        return False, "a connect made inside `%s` %s" % (" ".join(prefix[:1]), "SUCCEEDED" if inside == 0 else "ended with exit %s (not the refusal the probe reports as 7)" % inside)
    except Exception as e:
        return False, "the block probe could not run (%r)" % (e,)
    finally:
        srv.close()


NO_WINDOWS_PLUGINS = ("NEEDS: (harness, not built) a console capture for the Codex plugin's PowerShell hooks and a per-process network block, which on Windows is a "
                      "firewall rule of an elevated runner; the Claude and Gemini plugins ship bash hooks only")


def offline_prefix(ctx, r, name):
    """the no-network prefix of the two plugin cases, proven. Windows SKIPs with what is missing. Without a prefix, or with one that does not block,
    the hooks still run and the case is NOT judged for 'offline' (no_reference), as before."""
    if getattr(ctx, "win", False) or cases.plat().startswith("windows"):
        ctx.skip_reason = name
        raise cases.Skip("%s: %s" % (name, NO_WINDOWS_PLUGINS))
    R.need_relaunch_app(ctx, name)
    prefix = netns_prefix()
    tool = "`sandbox-exec` with the network denied" if sys.platform == "darwin" else "`unshare -rn`"
    if prefix is None:
        r.no_reference("%s is not permitted on this runner: the installs and hooks ran WITH network access, so 'works offline' is not proven" % tool)
        return []
    blocks, why = prefix_blocks_network(prefix)
    if not blocks:
        r.no_reference("%s does not block the network here (%s): the installs and hooks ran WITH network access, so 'works offline' is not proven" % (tool, why))
        return []
    r.note("no-network prefix proven: %s" % why)
    return prefix


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
    if getattr(ctx, "win", False) or cases.plat().startswith("windows"):
        ctx.skip_reason = "claude_plugin_offline"
        raise cases.Skip("claude_plugin_offline: %s" % NO_WINDOWS_PLUGINS)
    R.need_relaunch_app(ctx, "claude_plugin_offline")
    if not shutil.which("jq") or not shutil.which("bash"):
        raise cases.Skip("NEEDS: bash and jq on the runner (the plugin's hook scripts are bash + jq)")
    bundle = bundle_dir(ctx)
    r.check(bundle is not None, "no bundled plugin folder (resources/bundled/claude_plugin) beside the binary or in the checkout")
    if not bundle:
        return
    prefix = offline_prefix(ctx, r, "claude_plugin_offline")
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
    for d in [os.environ.get("APPCI_%s_PLUGIN_DIR" % agent.upper())] + [os.path.join(root, agent + "_plugin") for root in bundle_roots(ctx)]:
        if d and os.path.isfile(os.path.join(d, *marker)):
            return d
    return None


def run_in_pty(argv, stdin_text, env, cwd, timeout=60):
    """-> (returncode, output bytes). argv runs with stdin from a file and the pty as its controlling terminal and stdout/stderr, so a hook that writes to /dev/tty is captured"""
    import pty  # Unix only; this module is imported on Windows too (its cases are Linux-only)
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
    if getattr(ctx, "win", False) or cases.plat().startswith("windows"):
        ctx.skip_reason = "agent_plugins_offline"
        raise cases.Skip("agent_plugins_offline: %s" % NO_WINDOWS_PLUGINS)
    R.need_relaunch_app(ctx, "agent_plugins_offline")
    if not shutil.which("jq") or not shutil.which("bash"):
        raise cases.Skip("NEEDS: bash and jq on the runner (the plugins' hook scripts are bash + jq)")
    prefix = offline_prefix(ctx, r, "agent_plugins_offline")
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
    the app issue a request the gate refuses, because every setting that names an origin also allows it. The page's origin rows (Remove, typed add) ARE judged here, by the keyboard
    ring and the `hop=network_gate_ui` lines (#1830 follow-up).
    Round 15 (#2077): also macOS and the Windows Git Bash pane. The launch half is the same everywhere (the settings file of the NEXT launch is the
    profile's; start() installs it). The page half reads the file the running app writes (cases_robust.live_settings_file) and ends by proving the
    shell is in front again. Windows: the settings chord has never been played there, so a page that shows no action is 'not judged', not a failure."""
    R.need_relaunch_app(ctx, "network_gate_switch")
    linux = not (ctx.be.name == "mac" or ctx.win)
    if linux:
        R.need_linux_app(ctx, "network_gate_switch")
    marker_host = "gate-switch-%s.example.invalid" % ctx.tok(5).lower()
    # Round 21 (#2146): the Settings tab this case opens must not be in front when it ends, on ANY platform. Until then the proof below ran off Linux
    # only (round 15 added it with the macOS / Windows port), and `close_settings` is two Escapes, which do not close the Settings tab: round 15
    # Wayland ended this case with the keyboard in the "Allowed origins" field, and tab_lifecycle_keys typed its first probe into it.
    page_open = False
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
        R.T.edit_file(p, "network", "gate_enabled", "false")
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
        R.T.edit_file(p, "network", "gate_enabled", "true")
        _, up = R.start_app(ctx, r, "ngs_on")
        if not up:
            return
        log = ctx.log_since(0)
        r.check(GATE_OFF_LOG not in log, "gate_enabled=true: the launch still logged '%s'" % GATE_OFF_LOG)
        r.metric("gate_lines_last_launch", len(gate_log_lines(log)))
        # 4. The Settings > Network page rows, driven by KEYBOARD through the page's focus ring (#1830): the palette entry "Settings: Network: focus allowed origins" puts the ring on
        # the origin editor, Tab moves to the listed origin's Remove, Delete removes it; typing the origin + Enter adds it. Judged by the app's own `hop=network_gate_ui` lines, the
        # settings file and the log never naming the host (the hop lines carry a host hash only).
        import cases_agent_netgate as NG
        off = ctx.log_size()
        page_open = True
        NG.open_network_kb(ctx, r)
        # round 6: the page showed "No extra origins" here, so there was no Remove stop to Tab to. Say whether the file still held the origin.
        r.check(NG.origin_count(ctx, marker_host) == 1, "before the keyboard Remove, settings.toml no longer holds the allowed origin the case wrote (the page then has no Remove stop)")
        ctx.be.key("Tab")
        time.sleep(0.5)
        ctx.shot("ngs_kb_remove_focus")
        ctx.be.key("Delete")
        time.sleep(0.8)
        if ctx.win and not ctx.until(lambda: NG.hop_line("remove_origin", marker_host) in ctx.log_since(off), 10, step=0.3) and NG.origin_count(ctx, marker_host) == 1:
            NG.close_settings(ctx)
            page_open = not shell_back_in_front(ctx, r, "ngs")
            r.unverifiable("the Settings > Network page showed no action on Windows (no remove_origin hop, the origin still in the file): the settings chord "
                           "(ctrl+comma) and the palette entry have never been proven on the Windows backend, so the page half is NOT judged here; the launch half was")
            return
        NG.expect_hop(ctx, r, off, "remove_origin", "keyboard Remove of the allowed origin", host=marker_host)
        r.check(NG.origin_count(ctx, marker_host) == 0, "keyboard Remove: settings.toml still holds the allowed origin")
        NG.close_settings(ctx)
        off = ctx.log_size()
        NG.open_network_kb(ctx, r)
        ctx.be.type(marker_host, 12)
        time.sleep(0.3)
        ctx.be.key("Return")
        time.sleep(0.8)
        NG.expect_hop(ctx, r, off, "add_origin", "keyboard add (typed origin + Enter)", host=marker_host)
        r.check(NG.origin_count(ctx, marker_host) == 1, "keyboard add: settings.toml holds the origin %d times (expected once)" % NG.origin_count(ctx, marker_host))
        ctx.shot("ngs_kb_added")
        NG.close_settings(ctx)
        page_open = not shell_back_in_front(ctx, r, "ngs")
        r.check(marker_host not in ctx.log_since(0), "the app log names the allowed origin's host after the page edits (hop lines carry a host hash only)")
        r.unverifiable("the denial notice (toast naming the purpose and host), its 'Allow this origin' link and the log row's 'Allow' button need a request the gate refuses, which no setting "
                       "this case writes can cause (every setting that names an origin also allows it); agent_gate_deny_allow and network_gate_notice trigger one and judge those rows")
    finally:
        if page_open:
            # the page half ended early (an exception, or the proof above failed once): the next case must still not type into the Settings tab
            try:
                shell_back_in_front(ctx, r, "ngs_end")
            except Exception as e:
                r.note("network_gate_switch: proving the shell in front after the Settings page raised %r" % (e,))
        try:
            p = R.settings_file(ctx)
            t = open(p, encoding="utf-8", errors="replace").read()
            t = R.T.set_key(t, "network", "gate_enabled", "true")
            open(p, "w", encoding="utf-8").write(R.T.set_key(t, "network", "allowed_origins", '""'))
            if not linux:
                # the app runs on a COPY of that file here: the copy is replaced at the next start, so start once more for the cases that follow
                R.stop_for_restart(ctx, r, 20, "ngs_restore")
                R.start_app(ctx, r, "ngs_restore")
        except Exception:
            pass


# ------------------------------------------------------------------ network_gate_notice
def fnv_host_hash(host):
    """network_gate::hash_host (FNV-1a 64 over the lowercased host): the hash the app log shows instead of a host"""
    h = 0xcbf29ce484222325
    for b in host.lower().encode():
        h = ((h ^ b) * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return "%016x" % h


@case("network_gate_notice", modes=("agent",), order=89, gui=True)
def c_network_gate_notice(ctx, r):
    """AGENT MODE ONLY (the denial path is the same in terminal mode: the toast belongs to the workspace, not the input mode). A one-image markdown file naming an unlisted
    TEST-NET origin is opened in the app's markdown viewer (cases_agent_netgate.show_image; rounds 6-9 asked the fake model for a markdown image, which agent output never
    loads, so no request was ever made: docs/design/r10-wl-gate.md). Remote images load through the network gate (purpose remote-asset, Automatic kind: allowlist only), so
    the gate refuses: the app log has 'network gate: denied "remote-asset" (host hash <hash>)' (no host, no URL), and the toast with 'Allow this origin' shows. The case clicks
    Allow (OCR + pointer; skipped as not-judged without them), then settings.toml has network.allowed_origins with the origin exactly once; a second image from the same
    origin is not refused. Finally gate_enabled=false: a third image is not refused and the log says 'turned OFF'."""
    import cases_replay as CRP
    import cases_ui as UI
    import cases_agent_netgate as NG
    R.need_linux_app(ctx, "network_gate_notice: Linux only. NEEDS (harness, not built): a pointer tool and OCR for the toast's 'Allow this origin' link "
                          "(macOS: r14-test-gaps T32; Windows: T31) and a close key for the viewer tab on macOS")
    CRP.need_model(ctx, r)
    m = UI.mouse_or_skip(ctx, r)
    UI.need_tesseract(r)
    host, port = "192.0.2.1", 8099                      # TEST-NET-1: never routable; the gate refuses before any connection is made
    origin = "%s:%d" % (host, port)                      # what the toast's Allow writes: the request's host and port (rounds 6-9 asked for port 80 and looked for :8099)
    hh = fnv_host_hash(host)
    denied_line = 'network gate: denied "remote-asset" (host hash %s)' % hh
    p = R.settings_file(ctx)
    url = lambda tok, tag: "http://%s/%s_%s.png" % (origin, tag, tok)

    def show(tag):
        off, tok, opened = NG.show_image(ctx, r, tag, url)
        time.sleep(4.0)
        return off, tok, opened

    try:
        off, tok, opened = NG.show_image(ctx, r, "a", url)      # no fixed sleep here: the toast is up for about 4 s from the denial and is looked for at once
        ctx.until(lambda: denied_line in ctx.log_since(off), 30, step=0.2)
        w, _ = NG.find_toast_word(ctx, "ngn_find_allow", "allow")
        log = ctx.log_since(off)
        r.check(denied_line in log, "the remote image from an unlisted host was not refused by the gate (no '%s' line)" % denied_line)
        r.check(host not in log and tok not in log, "the app log names the host or the image URL of a refused request")
        ctx.shot("ngn_denied")
        if not w:
            NG.close_viewer(ctx, opened)
            r.unverifiable("the 'Allow this origin' link was not found on screen by OCR (shots/ngn_find_allow.png): the click, the settings.toml write and the retry were not judged")
            return
        m.click(w[1], w[2])
        text = ""
        for _ in range(20):
            time.sleep(0.5)
            text = open(p, encoding="utf-8", errors="replace").read()
            if origin in text:
                break
        r.check(text.count(origin) == 1, "after Allow, settings.toml holds the origin %d times (expected once)" % text.count(origin))
        NG.close_viewer(ctx, opened)
        off2, _, opened = show("b")
        r.check(denied_line not in ctx.log_since(off2), "a second image from the now-allowed origin was still refused")
        NG.close_viewer(ctx, opened)
        # the gate off: a third origin is passed and the switch is logged
        q = R.stop_for_restart(ctx, r, 20, "ngn_off")
        r.check(q["exited"], "ngn_off: the app did not exit within 20 s")
        t = open(p, encoding="utf-8", errors="replace").read()
        t = R.T.set_key(t, "network", "allowed_origins", '""')
        open(p, "w", encoding="utf-8").write(R.T.set_key(t, "network", "gate_enabled", "false"))
        _, up = R.start_app(ctx, r, "ngn_off")
        if not up:
            return
        off3, _, opened = show("c")
        log3 = ctx.log_since(0)
        r.check(GATE_OFF_LOG in log3, "gate_enabled=false: no '%s' line" % GATE_OFF_LOG)
        r.check(denied_line not in ctx.log_since(off3), "the gate is off but the image was still refused")
        NG.close_viewer(ctx, opened)
    finally:
        try:
            t = open(p, encoding="utf-8", errors="replace").read()
            t = R.T.set_key(t, "network", "gate_enabled", "true")
            open(p, "w", encoding="utf-8").write(R.T.set_key(t, "network", "allowed_origins", '""'))
        except Exception:
            pass
