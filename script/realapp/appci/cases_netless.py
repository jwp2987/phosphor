# appci/cases_netless.py: Wave N, "no network at startup" (docs/testing.md 2.17). Registered with the same @case decorator as cases.py (imported by its last line).
#   zero_network_startup   Linux: the app, with the harness profile (no cloud account, no provider key), is launched under `strace -f` and left idle for APPCI_ZNS_IDLE_S
#                          (default 120 s); NO connect()/sendto() to a non-loopback inet address may appear in the trace
#   claude_plugin_offline  Linux: with the network namespace unshared (`unshare -rn`), the plugin bundled in the build is registered as a local marketplace and installed with
#                          fakeclaude.py, and its real hook scripts produce the prompt / tool / permission / stop events; a fake `claude` in a pane prints those
#                          sequences and the app must log one 'CLI agent event received' line per event, with no prompt text in the log
import json, os, re, shutil, subprocess, sys, time

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


def outbound_attempts(trace_text):
    """-> [(syscall, address, port)] for every connect()/sendto()/sendmsg() in an `strace -f -e trace=network` text whose inet address is NOT loopback or unspecified.
    AF_UNIX / AF_NETLINK lines never match (no sa_family=AF_INET)."""
    out = []
    for line in trace_text.splitlines():
        m = ATTEMPT.search(line)
        if not m:
            continue
        addr = m.group(4) or m.group(5) or ""
        if not HOST_ONLY.match(addr):
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
    finally:
        ctx.app.wrap = ""
        ctx.app.stop()
    text = open(trace, errors="replace").read() if os.path.exists(trace) else ""
    r.check(bool(text), "the trace is empty: strace did not follow the app (no socket call at all is implausible)")
    bad = outbound_attempts(text)
    r.metric("network_syscalls_traced", len(text.splitlines()))
    r.metric("outbound_attempts", len(bad))
    r.check(not bad, "the app tried to reach the network at startup/idle: %s" % "; ".join("%s %s:%d" % b for b in bad[:8]))
    R.start_app(ctx, r, "zns_restore")                      # leave a normally-launched app for the cases that follow


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
