# appci/selftest_r15_w5c.py: offline self-tests of round 15 wave 5c (docs/design/r15-w5c.md): the scripted answer of agent_code_recycle (#2071),
# the privilege-boundary preconditions (T47, priv_boundary.py), process sampling without /proc and the soak's tier rules (T49, procsample.py),
# and the pure parts of gpu_real_render (T48, cases_r15_w5c.py). Pure Python plus `sh`: no app, display, sudo, docker or second user.
# selftest.py appends CHECKS to its own list (two lines at its end), so `run.py --self-test` runs them.
# Every judge is fed a right input AND the wrong ones it exists to catch: an oracle that cannot fail is not a test.
import os, random, stat, subprocess, sys, types

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.join(HERE, "..", "..", "..")
sys.path.insert(0, HERE)

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


@check
def w5c_cases_registered_proving_and_classified():
    """agent_code_recycle is an agent-only PROVING_FAKE case; gpu_real_render and soak are PROVING, never SMOKE, never PROVING_FAKE; every one is
    classified for Windows; docs/testing.md has the section and names each case and each NEEDS: machine change"""
    import cases, winmap
    reg = {c["name"]: c for c in cases.CASES}
    assert reg["agent_code_recycle"]["modes"] == ("agent",) and "agent_code_recycle" in cases.PROVING and "agent_code_recycle" in cases.PROVING_FAKE
    for n in ("gpu_real_render", "soak"):
        assert n in reg and n in cases.PROVING and n not in cases.SMOKE and n not in cases.PROVING_FAKE and reg[n]["gui"], n
    assert len(set(cases.PROVING_FAKE)) == len(cases.PROVING_FAKE), "PROVING_FAKE lists a case twice"
    assert winmap.skip_reason("soak", "powershell") is None and winmap.skip_reason("soak", "gitbash") is None, "the soak runs in both Windows panes (T49)"
    why = winmap.skip_reason("gpu_real_render", "gitbash")
    assert why and "by design" in why and "render node" in why, why
    assert winmap.skip_reason("agent_code_recycle", "powershell") is None
    for n in ("subshell_ready_relay_shape", "subshell_ready_docker_t_without_i", "sudo_su_real_shell"):
        assert n in reg and "by design" in winmap.NOT_WINDOWS[n], n
    doc = open(os.path.join(REPO, "docs", "testing.md"), encoding="utf-8").read()
    sec = doc[doc.index("## Round 15 wave 5c"):]
    for word in ("`gpu_real_render`", "`soak`", "`agent_code_recycle`", "`sudo_su_real_shell`", "`subshell_ready_relay_shape`", "`subshell_ready_docker_t_without_i`",
                 "APPCI_SUDO_USER", "selftest_r15_w5c.py", "procsample.py", "priv_boundary.py", "NEEDS:"):
        assert word in sec, "docs/testing.md (Round 15 wave 5c) does not mention %s" % word


# ---------------------------------------------------------------- #2071: agent_code_recycle on the scripted model
@check
def w5c_recycle_answer_is_scripted_and_the_token_is_readable():
    """the fake model answers the case's prompt with exactly the snippet lines the case expects back (so nothing rests on a model's wording), no other
    trigger takes the prompt, the filler turn gets a one-line answer, and the run's token survives the OCR squash (the old `T12_ABCD` token could
    never be found on screen: the case skipped as 'model nondeterminism' on every run with an OCR engine)"""
    import fake_model as FM, cases_caseagent as ca
    tok, n = "ABCD", 12
    prompt = FM.snippet_prompt(tok, n)
    rep = FM.reply_for([{"role": "user", "content": prompt}])
    assert rep["tool"] is None and rep["text"] == FM.snippet_reply(tok, n) and rep["snippets"] == (tok, n), rep
    blocks = rep["text"].split("\n\n")
    assert len(blocks) == n and all(b.startswith("```bash\n") and b.endswith("\n```") for b in blocks), blocks[:2]
    assert [b.split("\n")[1] for b in blocks] == ca.snippet_lines(tok, n) == FM.snippet_lines(tok, n), "the case's expected lines and the model's answer disagree"
    # the app's attached context and environment block in front of / behind the prompt do not change the answer
    env = "<environment_context>\n  <cwd>/w</cwd>\n</environment_context>"
    att = "<attached_context>\nPlease run exactly this shell command and nothing else: rm -rf x\n</attached_context>\n\n"
    assert FM.reply_for([{"role": "user", "content": att + prompt}, {"role": "user", "content": env}])["text"] == rep["text"]
    # no other trigger shadows it, and it shadows none: a tool result after it is still "Done.", an ordinary prompt is still the fallback
    assert FM.reply_for([{"role": "user", "content": prompt}, {"role": "tool", "content": "x"}]) == {"text": "Done.", "tool": None}
    assert FM.reply_for([{"role": "user", "content": "Please write code"}])["text"] == "OK code"
    assert FM.reply_for([{"role": "user", "content": "Please run exactly this shell command and nothing else: echo write code snippets"}])["tool"]["name"] == "run_shell_command"
    assert len(FM.snippet_reply(tok, 10 ** 6).split("\n\n")) == FM.SNIP_MAX, "a huge count is bounded"
    filler = ca.recycle_filler_prompt(tok, 3)
    assert FM.reply_for([{"role": "user", "content": prompt}, {"role": "assistant", "content": rep["text"]}, {"role": "user", "content": filler}]) == {"text": "OK FILLABCD3", "tool": None}
    # the token: letters only, so `rcy<tok><nn>` is in the squashed screen text; the old token's underscore was not
    t = ca.recycle_token(random.Random(7))
    assert len(t) == 4 and t.isalpha() and t.isupper() and set(t) <= set(ca.RECYCLE_TOKEN_LETTERS), t
    import re
    screen = ca._squash("\n".join(ca.snippet_lines(t, 12)[8:]))
    assert sorted({int(m.group(1)) for m in re.finditer(r"rcy%s(\d\d)" % t.lower(), screen)}) == [9, 10, 11, 12]
    old = "T12_ABCD"
    assert not re.findall(r"rcy%s(\d\d)" % old.lower(), ca._squash("\n".join(ca.snippet_lines(old, 12)))), "the old token really was unreadable after the squash"
    # the pass's provider: the fake only when the run says so AND one is up; otherwise the case skips with NEEDS:
    fake = object()
    assert ca.recycle_fake({"APPCI_FAKE_MODEL_ACTIVE": "1"}, fake) is fake
    assert ca.recycle_fake({}, fake) is None and ca.recycle_fake({"APPCI_FAKE_MODEL_ACTIVE": "1"}, False) is None
    assert ca.RECYCLE_NEEDS_FAKE.startswith("NEEDS:") and "APPCI_FAKE_MODEL=1" in ca.RECYCLE_NEEDS_FAKE
    src = open(os.path.join(HERE, "cases_caseagent.py"), encoding="utf-8").read()
    body = src[src.index("def c_agent_code_recycle"):src.index("# ---------------------------------------------------------------- #1796")]
    assert "r.skip(RECYCLE_NEEDS_FAKE)" in body and body.count("r.skip(") == 1, "the only skip left is the missing scripted model: nothing may skip on wording"
    assert "model nondeterminism" not in body and "Reply with exactly" not in body


# ---------------------------------------------------------------- T47: the privilege boundary's preconditions
@check
def w5c_sudo_target_and_needs_text():
    """who the pane may become without a password, on every branch, and the exact machine change when nobody"""
    import priv_boundary as PB
    calls = []

    def run_ok(argv):
        calls.append(argv)
        return 0, []

    def run_fail(argv):
        calls.append(argv)
        return 1, ["sudo: a password is required"]

    pw = {"appci-priv": (1500, "/home/appci-priv", "/bin/bash"), "root": (0, "/root", "/bin/bash"), "ci": (1000, "/home/ci", "/bin/bash")}

    def getpw(u):
        return pw[u]

    yes, no = (lambda n: "/usr/bin/" + n), (lambda n: None)
    t = PB.sudo_target(get=lambda k: "appci-priv", which=yes, run=run_ok, pw=getpw, euid=1000)
    assert t["ok"] and t["user"] == "appci-priv" and t["uid"] == 1500 and not t["root"] and t["flags"] == "-n -u appci-priv" and t["explicit"], t
    assert calls[-1] == ["sudo", "-n", "-u", "appci-priv", "true"], calls
    t = PB.sudo_target(get=lambda k: None, which=yes, run=run_ok, pw=getpw, euid=1000)
    assert t["ok"] and t["root"] and t["uid"] == 0 and t["flags"] == "-n" and t["home"] == "/root" and not t["explicit"] and calls[-1] == ["sudo", "-n", "true"], t
    for kw, word in ((dict(get=lambda k: None, which=no, run=run_ok), "not installed"),
                     (dict(get=lambda k: None, which=yes, run=run_fail), "`sudo -n true` failed"),
                     (dict(get=lambda k: "nobody-here", which=yes, run=run_ok), "no such user"),
                     (dict(get=lambda k: "ci", which=yes, run=run_ok), "the CI user itself"),
                     (dict(get=lambda k: "appci-priv", which=yes, run=run_fail), "`sudo -n -u appci-priv true` failed")):
        t = PB.sudo_target(pw=getpw, euid=1000, **kw)
        assert not t["ok"] and word in t["why"], (word, t)
    assert "a password is required" in PB.sudo_target(get=lambda k: None, which=yes, run=run_fail, pw=getpw, euid=1000)["why"], "the probe's own error is quoted"
    t = PB.sudo_target(get=lambda k: None, which=yes, run=run_ok, pw=getpw, euid=0)
    assert not t["ok"] and "root already" in t["why"], t
    needs = PB.sudo_needs("gitlab-runner", "linux")
    for word in ("NEEDS:", "sudoers entry", "useradd -m -s /bin/bash appci-priv", "gitlab-runner ALL=(appci-priv) NOPASSWD: ALL", "use_pty", "APPCI_SUDO_USER=appci-priv",
                 "ci.env", "never edits sudoers"):
        assert word in needs, word
    assert "sysadminctl -addUser appci-priv" in PB.sudo_needs("gitrunner", "darwin")
    # the three cases put that text in their skip, and the docker one names each of its three failures
    for f in ("cases_subshell_ready.py", "cases_coverage_a.py"):
        assert "PB.sudo_needs()" in open(os.path.join(HERE, f), encoding="utf-8").read(), f
    ok, why = PB.docker_ready(which=no, run=run_ok)
    assert not ok and "not installed" in why and "NEEDS:" in why
    ok, why = PB.docker_ready(which=yes, run=run_fail)
    assert not ok and "`docker` group" in why and "NEEDS:" in why and calls[-1][:2] == ["docker", "info"], why
    seq = iter([(0, []), (1, ["Error: No such image: alpine"])])
    ok, why = PB.docker_ready(which=yes, run=lambda argv: next(seq))
    assert not ok and "docker pull alpine" in why and "NEEDS:" in why, why
    assert PB.docker_ready(which=yes, run=run_ok) == (True, "")


@check
def w5c_shared_dir_and_typed_commands():
    """the directory the inner shell writes to is one the OTHER user can reach, and the lines the cases type are the ones the app's sudo regex knows"""
    import shutil, tempfile, re
    import priv_boundary as PB, cases_subshell_ready as sr, cases_coverage_a as cov
    modes = {"/": 0o755, "/a": 0o755, "/a/b": 0o700, "/a/b/w": 0o755, "/a/c": 0o755, "/a/c/w": 0o755}

    def st(p):
        if p not in modes:
            raise OSError(p)
        return types.SimpleNamespace(st_mode=stat.S_IFDIR | modes[p])

    assert PB.others_can_traverse("/a/c/w", st) and not PB.others_can_traverse("/a/b/w", st) and not PB.others_can_traverse("/a/missing", st)
    d = tempfile.mkdtemp()
    try:
        w = os.path.join(d, "w")
        os.makedirs(w)
        got, clean = PB.shared_dir(w, 0, "x")
        assert got == w and clean() is None and os.path.isdir(w), "root reaches the work dir itself"
        open_all = lambda p: types.SimpleNamespace(st_mode=stat.S_IFDIR | 0o755)      # noqa: E731
        got, clean = PB.shared_dir(w, 1500, "x", st=open_all)
        assert got == os.path.join(w, "pb_x") and (os.name == "nt" or stat.S_IMODE(os.stat(got).st_mode) == 0o777)
        clean()
        assert not os.path.exists(got)
        closed = lambda p: types.SimpleNamespace(st_mode=stat.S_IFDIR | 0o700)        # noqa: E731
        other = os.path.join(d, "vartmp")
        os.makedirs(other)
        got, clean = PB.shared_dir(w, 1500, "x", st=closed, tmp_root=other)
        assert os.path.dirname(got) == other and (os.name == "nt" or stat.S_IMODE(os.stat(got).st_mode) == 0o777), got
        open(os.path.join(got, "f"), "w").write("1\ntok\n")
        assert PB.read_lines(os.path.join(got, "f")) == ["1", "tok"] and PB.read_lines(os.path.join(got, "nope")) == []
        clean()
        assert not os.path.exists(got)
    finally:
        shutil.rmtree(d, ignore_errors=True)
    root = dict(ok=True, user="root", uid=0, home="/root", root=True, flags="-n", explicit=False)
    user = dict(ok=True, user="appci-priv", uid=1500, home="/home/appci-priv", root=False, flags="-n -u appci-priv", explicit=True)
    assert sr.relay_command(root) == "sudo -s" and sr.relay_command(user) == "sudo -n -H -u appci-priv -s"
    # the app's built-in sudo subshell regex (app/src/terminal/warpify/settings.rs): both typed lines must match it, or no banner is offered and Ctrl-I does nothing
    src = open(os.path.join(REPO, "app", "src", "terminal", "warpify", "settings.rs"), encoding="utf-8").read()
    m = re.search(r'Regex::new\(r"(\^/\?\(\[\\w\\\.-\]\+/\)\*sudo.*?)"\)\.expect\("sudo shell regex invalid"\)', src)
    assert m, "the sudo subshell regex was not found in warpify/settings.rs"
    rx = re.compile(m.group(1))
    for t in (root, user):
        assert rx.match(sr.relay_command(t)), "the app's sudo regex does not match %r" % sr.relay_command(t)
    assert not rx.match("sudo ls"), "the regex is the shell one (a plain command is not a subshell)"
    assert cov.sudo_shell_command(root) == 'sudo -n env -i HOME=/root PATH="$PATH" bash --noprofile --norc -i', cov.sudo_shell_command(root)
    assert cov.sudo_shell_command(user) == 'sudo -n -u appci-priv env -i HOME=/home/appci-priv PATH="$PATH" bash --noprofile --norc -i'
    pw = lambda u: {"nobody": (65534,), "tester": (1600,), "ci": (1000,)}[u]      # noqa: E731
    none = dict(ok=False, user=None, root=False)
    assert cov.su_plan(none, 0, {}, True, "linux", pw) == dict(run=True, cmd="su -s /bin/bash nobody", user="nobody", uid=65534, why="")
    assert cov.su_plan(root, 1000, {}, True, "linux", pw)["cmd"] == "sudo -n su -s /bin/bash nobody"
    assert cov.su_plan(root, 1000, {"APPCI_SU_USER": "tester"}, True, "darwin", pw)["cmd"] == "sudo -n su tester"
    for args, word in (((user, 1000, {}, True, "linux", pw), "asks for a password"), ((none, 1000, {}, True, "linux", pw), "asks for a password"),
                       ((root, 1000, {}, False, "linux", pw), "not installed"), ((root, 1000, {}, True, "darwin", pw), "APPCI_SU_USER"),
                       ((root, 1000, {"APPCI_SU_USER": "ghost"}, True, "linux", pw), "no user"), ((root, 1000, {"APPCI_SU_USER": "ci"}, True, "linux", pw), "no boundary")):
        p = cov.su_plan(*args)
        assert not p["run"] and word in p["why"], (word, p)


# ---------------------------------------------------------------- T49: process sampling without /proc, the soak's tier rules
@check
def w5c_process_sampling_parsers():
    """ps (macOS) and Get-Process (Windows) output become the same reading the /proc sampler gives; junk lines and exited processes are dropped"""
    import procsample as PS
    for s, want in (("0:01.23", 1.23), ("123:45.67", 123 * 60 + 45.67), ("01:02:03", 3723.0), ("2-01:02:03", 2 * 86400 + 3723.0), ("0:00.00", 0.0)):
        assert abs(PS.ps_time_seconds(s) - want) < 1e-6, (s, PS.ps_time_seconds(s))
    for bad in ("", "abc", "1", "1:2:3:4", "x-1:02"):
        assert PS.ps_time_seconds(bad) is None, bad
    bundle = "/Users/ci/job 7/Phosphor.app"
    ps = ("  501   204800   1:02.50 /Users/ci/job 7/Phosphor.app/Contents/MacOS/phosphor\n"
          "  502    51200   0:03.25 /Users/ci/job 7/Phosphor.app/Contents/MacOS/phosphor terminal-server --x\n"
          "  777   999999   9:00.00 /Applications/Phosphor.app/Contents/MacOS/phosphor\n"
          "  888       12   0:00.01 /bin/zsh -l\n"
          "garbage line\n"
          "  999        x   0:00.01 /bin/sh\n")
    rows = PS.mac_rows(ps, bundle)
    assert [r[0] for r in rows] == [501, 502], "only this job's bundle: the installed app (777) and the shells are not the app under test: %r" % rows
    pids, rss, cpu = PS.total(rows)
    assert pids == [501, 502] and rss == 256000 and abs(cpu - 65.75) < 1e-6, (pids, rss, cpu)
    assert PS.mac_rows(ps, bundle + "/") == rows and PS.mac_rows("", bundle) == [] and len(PS.parse_ps(ps)) == 4
    csvtxt = ('"Id","Cpu","WorkingSet64","PrivateMemorySize64"\r\n"4120","12.5","209715200","157286400"\r\n"4188","0,75","52428800","31457280"\r\n'
              '"4999","","",""\r\n')
    rows = PS.parse_get_process(csvtxt)
    assert rows == [(4120, 153600, 12.5, 204800), (4188, 30720, 0.75, 51200)], rows       # private KiB is the judged figure; the exited 4999 is dropped
    assert PS.parse_get_process("") == [] and PS.parse_get_process("not,csv\n1,2\n") == []
    assert "PrivateMemorySize64" in PS.GET_PROCESS_PS and "$_.Path -eq '%s'" in PS.GET_PROCESS_PS, "the listing is by the test build's own exe path, never by name"
    a, b = dict(cpu_s=10.0), dict(cpu_s=13.0)
    assert PS.cpu_pct(a, b, 60.0) == 5.0 and PS.cpu_pct(None, b, 60.0) is None and PS.cpu_pct(a, b, 0) is None
    if sys.platform.startswith("linux"):
        me = types.SimpleNamespace(app=types.SimpleNamespace(pid=lambda: os.getpid()), be=types.SimpleNamespace(name="x11"), win=False)
        s = PS.sample(me)
        assert s and os.getpid() in s["pids"] and s["rss_kib"] > 1024 and s["cpu_s"] >= 0 and s["how"] == "/proc", s
        gone = types.SimpleNamespace(app=types.SimpleNamespace(pid=lambda: None), be=types.SimpleNamespace(name="x11"), win=False)
        assert PS.sample(gone) is None, "no app pid: no reading, never zeros"


@check
def w5c_soak_tier_rules_and_lines():
    """the fix tier soaks macOS and Windows for 10 minutes (two of the four Windows passes), Linux stays full-tier only; the typed lines are the pane
    shell's; the growth rule gives a verdict on a 10-minute run's 20 samples and still fails a real climb"""
    import cases_robust as cr
    assert cr.soak_minutes({}) == 60 and cr.soak_minutes({"APPCI_TIER": "smoke"}) == 10 and cr.soak_minutes({"APPCI_TIER": "full"}) == 60
    assert cr.soak_minutes({"APPCI_TIER": "smoke", "APPCI_SOAK_MIN": "3"}) == 3 and cr.soak_minutes({"APPCI_SOAK_MIN": "90"}) == 90
    for plat in ("linux-wayland", "linux-x11", "linux"):
        assert "full-tier only" in cr.soak_tier_skip("smoke", plat, "terminal", None), plat
        assert cr.soak_tier_skip("full", plat, "terminal", None) is None and cr.soak_tier_skip("", plat, "agent", None) is None
    assert cr.soak_tier_skip("smoke", "macos", "terminal", None) is None and cr.soak_tier_skip("smoke", "macos", "agent", None) is None
    ran = [(m, p) for m in ("terminal", "agent") for p in ("powershell", "gitbash") if cr.soak_tier_skip("smoke", "windows", m, p) is None]
    assert ran == [("terminal", "powershell"), ("agent", "gitbash")], ran
    assert {m for m, _ in ran} == {"terminal", "agent"} and {p for _, p in ran} == {"powershell", "gitbash"}, "each mode and each pane is soaked once"
    assert "full tier soaks all four" in cr.soak_tier_skip("smoke", "windows", "agent", "powershell")
    assert all(cr.soak_tier_skip("full", "windows", m, p) is None for m in ("terminal", "agent") for p in ("powershell", "gitbash"))
    import cases
    mk = lambda pane, win: types.SimpleNamespace(pane=pane, win=win, W=os.path.join("C:\\job", "w") if win else "/job/w",      # noqa: E731
                                                 append_cmd=lambda sh, tok, f: cases.Ctx.append_cmd(None, sh, tok, f),
                                                 pane_shell=lambda: "pwsh" if pane == "powershell" else "bash")
    assert cr.soak_line(mk(None, False), "t1") == "echo t1 >> soak.txt"
    assert cr.soak_line(mk("powershell", True), "t1") == "Add-Content soak.txt t1"
    ab = cr.soak_line(mk("powershell", True), "t1", absolute=True)
    assert ab.startswith("Add-Content C:/job") and ab.endswith("soak.txt t1") and "\\" not in ab, ab
    assert cr.soak_line(mk(None, False), "t1", absolute=True) == "echo t1 >> /job/w/soak.txt"
    assert cr.soak_flood_line("pwsh") == "1..300000" and cr.soak_flood_line("bash") == "seq 1 300000"
    if os.name != "nt":       # os.kill(pid, 0) is a liveness probe on POSIX only (on Windows it would TERMINATE the process: pid_alive refuses there)
        assert cr.pid_alive(os.getpid()) and not cr.pid_alive(2 ** 22 + 12345)
    # 10 minutes at two samples a minute: 20 samples, 3 judged windows. Flat passes, a climb fails, one a minute would have been "short".
    assert cr.growth_verdict([300.0 + (i % 3) for i in range(20)])[0] == "pass"
    assert cr.growth_verdict([300.0 + 40 * i for i in range(20)])[0] == "fail"
    assert cr.growth_verdict([300.0] * 10)[0] == "short"
    src = open(os.path.join(HERE, "cases_robust.py"), encoding="utf-8").read()
    body = src[src.index("def c_soak"):src.index("# ---------------------------------------------------------------- #1956")]
    assert "/proc" not in body.split('"""', 2)[2] and "SC_CLK_TCK" not in body, "the soak body reads processes only through procsample"
    assert "PS.sample(ctx)" in body and "SOAK_MIN_PLAUSIBLE_MB" in body


# ---------------------------------------------------------------- T48: gpu_real_render's pure parts
GPU_LOG = """2026-11-11T10:00:00 [INFO] Enabled wgpu backends: Backends(VULKAN | GL)
2026-11-11T10:00:00 [INFO] Available wgpu adapters (in priority order):
2026-11-11T10:00:00 [INFO] IntegratedGpu: Intel(R) UHD Graphics 630 (CFL GT2)
\tBackend: Vulkan
\tDriver: Intel open-source Mesa driver (Mesa 25.0.7)
\tCan present: true
\tSupported texture format: Some(Bgra8UnormSrgb)
\tSupported alpha mode: [PreMultiplied, Opaque]
2026-11-11T10:00:00 [INFO] Cpu: llvmpipe (LLVM 20.1.2, 256 bits)
\tBackend: Vulkan
\tDriver: llvmpipe (Mesa 25.0.7 (LLVM 20.1.2))
\tCan present: true
2026-11-11T10:00:00 [INFO] IntegratedGpu: Mesa Intel(R) UHD Graphics 630 (CFL GT2)
\tBackend: Gl
\tDriver: Unknown
\tCan present: false
2026-11-11T10:00:01 [INFO] Using Vulkan IntegratedGpu (Intel(R) UHD Graphics 630 (CFL GT2)) for rendering new window.
2026-11-11T10:00:01 [INFO] Surface configured: backend=Vulkan adapter="Intel(R) UHD Graphics 630 (CFL GT2)" present_mode=Mailbox (via AutoNoVsync, supported: [Fifo, Mailbox]) frame_latency=2 alpha_mode=PostMultiplied software_renderer=false
"""
SOFT_LINE = 'x [INFO] Surface configured: backend=Gl adapter="llvmpipe (LLVM 20.1.2, 256 bits)" present_mode=Fifo frame_latency=2 alpha_mode=PostMultiplied software_renderer=true\n'


@check
def w5c_gpu_adapter_and_frame_judges():
    """the adapter a launch used and the adapters it could have used are read from the app's own lines (whose wording a Rust unit test pins), and the
    frame judge passes a fast GPU, fails a slow median, fails a slow tail and refuses to judge a handful of frames"""
    import cases_r15_w5c as G
    s = G.surfaces(GPU_LOG)
    assert s == [dict(backend="Vulkan", adapter="Intel(R) UHD Graphics 630 (CFL GT2)", present_mode="Mailbox (via AutoNoVsync, supported: [Fifo, Mailbox])", software=False)], s
    assert not G.is_software(s[0])
    soft = G.surfaces(SOFT_LINE)[0]
    assert soft["software"] and soft["backend"] == "Gl" and G.is_software(soft)
    assert G.is_software(dict(adapter="SwiftShader Device (Subzero)", software=False)), "a software rasteriser that reports another device type is still software"
    assert G.surfaces("no such line") == [] and G.surfaces(GPU_LOG + SOFT_LINE)[-1] == soft
    # the two strings above are the ones the Rust unit test pins, and the format string is still the one in resources.rs
    rs = open(os.path.join(REPO, "crates", "warpui", "src", "rendering", "wgpu", "resources.rs"), encoding="utf-8").read()
    assert 'Surface configured: backend={:?} adapter=\\"{}\\" present_mode={present_mode}' in rs and "software_renderer={}" in rs, "surface_config_summary changed: update SURFACE_RE"
    rt = open(os.path.join(REPO, "crates", "warpui", "src", "rendering", "wgpu", "resources_tests.rs"), encoding="utf-8").read()
    assert "surface_config_summary_names_the_adapter_and_flags_a_software_renderer" in rt and "software_renderer=true" in rt and "software_renderer=false" in rt
    ev = open(os.path.join(REPO, "crates", "warpui", "src", "windowing", "winit", "event_loop", "mod.rs"), encoding="utf-8").read()
    assert 'log_hop("frame_drawn"' in ev and '"total_ms={:.1} acquire_ms={:.1} present_ms={:.1}"' in ev, "the frame_drawn hop changed: update FRAME_RE"
    listed = G.listed_adapters(GPU_LOG)
    assert [(a["type"], a["backend"], a["can_present"]) for a in listed] == [("IntegratedGpu", "Vulkan", True), ("Cpu", "Vulkan", True), ("IntegratedGpu", "Gl", False)], listed
    hw = G.presentable_hardware(listed)
    assert [a["name"] for a in hw] == ["Intel(R) UHD Graphics 630 (CFL GT2)"], hw
    xvfb = GPU_LOG.replace("\tCan present: true\n\tSupported texture format", "\tCan present: false\n\tSupported texture format")
    assert G.presentable_hardware(G.listed_adapters(xvfb)) == [], "a GPU the session cannot present to (Xvfb, no DRI3) is not a usable adapter: SKIP with NEEDS, not FAIL"
    assert G.presentable_hardware(G.listed_adapters("x Cpu: llvmpipe\n\tBackend: Gl\n\tCan present: true\n")) == []

    def frames(n, total, acq, pres):
        return "".join("t input_order hop=frame_drawn total_ms=%.1f acquire_ms=%.1f present_ms=%.1f t=%d.0\n" % (total, acq, pres, 1000 + i) for i in range(n))

    fast = G.frame_stats(frames(60, 9.0, 6.5, 0.5))
    assert fast["n"] == 60 and fast["work_p50"] == 2.0 and fast["acquire_p50"] == 6.5, fast
    assert G.frame_verdict(fast)[0] == "pass", "a frame that waited 6.5 ms for the compositor and worked 2 ms is within the budget"
    assert G.frame_verdict(G.frame_stats(frames(60, 9.0, 1.0, 0.5)))[0] == "fail", "7.5 ms of the app's own work is over the 4 ms budget"
    tail = G.frame_stats(frames(50, 3.0, 0.5, 0.5) + frames(10, 30.0, 0.5, 0.5))
    assert tail["work_p50"] == 2.0 and G.frame_verdict(tail)[0] == "fail", "a slow tail (p95 over one refresh) fails even with a fast median: %r" % tail
    assert G.frame_verdict(G.frame_stats(frames(5, 3.0, 0.5, 0.5)))[0] == "few" and G.frame_verdict(G.frame_stats(""))[0] == "few"
    old = G.frame_stats("x hop=frame_drawn total_ms=4.0 acquire_ms=0.5 t=1.0\n")
    assert old["n"] == 1 and old["work_p50"] == 3.5, "a line without present_ms (an older build) still parses"
    assert G.node_state([]) == ("none", [])
    assert G.node_state(["/dev/dri/renderD128"], access=lambda p, m: False) == ("denied", [])
    assert G.node_state(["/dev/dri/renderD128", "/dev/dri/renderD129"], access=lambda p, m: p.endswith("128")) == ("ok", ["/dev/dri/renderD128"])
    assert G.SOFTWARE_ENV == {"WGPU_BACKEND": "gl", "LIBGL_ALWAYS_SOFTWARE": "1"}
    mod = open(os.path.join(REPO, "crates", "warpui", "src", "rendering", "wgpu", "mod.rs"), encoding="utf-8").read()
    assert "wgpu::Backends::from_env()" in mod, "the app no longer reads WGPU_BACKEND: the software launch would not be forced"
    assert "NOT RUN" in G.LOST_DEVICE_NOT_RUN and "NEEDS:" in G.LOST_DEVICE_NOT_RUN
    src = open(os.path.join(HERE, "cases_r15_w5c.py"), encoding="utf-8").read()
    body = src[src.index("def c_gpu_real_render"):]
    assert body.count("NEEDS:") >= 3 and "r.note(LOST_DEVICE_NOT_RUN)" in body, "every way the case cannot run names the machine change, and the unrun oracle is on every row"


@check
def w5c_gpu_pattern_is_the_same_every_time():
    """the drawn screen is identical in both launches: the script prints the same 24 coloured rows on every run"""
    import cases_r15_w5c as G
    sc = G.pattern_script("ABCD")
    assert sc == G.pattern_script("ABCD") and sc != G.pattern_script("ABCE")
    assert sc.count("\nprintf ") == G.PATTERN_ROWS and sc.startswith("#!/bin/sh\n")
    import shutil
    if os.name == "nt" or not shutil.which("sh"):
        return                  # the case is Linux only; the script is run below wherever there is a POSIX sh
    outs = [subprocess.run(["sh", "-c", sc], capture_output=True, timeout=20) for _ in range(2)]
    assert outs[0].returncode == 0 and outs[0].stdout == outs[1].stdout and not outs[0].stderr, outs[0].stderr
    rows = outs[0].stdout.decode().splitlines()
    assert len(rows) == G.PATTERN_ROWS and all("GPUABCD row %02d " % (i + 1) in l for i, l in enumerate(rows)), rows[:2]
    assert rows[0].startswith("\x1b[1;31m") and rows[1].startswith("\x1b[32m") and all(l.endswith("\x1b[0m") for l in rows), rows[:2]
