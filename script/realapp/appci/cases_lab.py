# appci/cases_lab.py: the fake ssh lab cases (Wave 0.5b; script/realapp/sshlab, docs/testing.md 2.15). Registered with @lab_case, which expands one body into
# one case PER ARM: lab_<case>.<shell|scenario user>.<lan|150|300>.<nohelper|helper>[.<fault>]. They run only when --cases names them (the app-test-lab-*
# jobs pass 'lab_*'), only the arms APPCI_LAB_MATRIX selects (quick | full | spec), and need a lab (APPCI_LAB=local on the box, remote from the Mac/Windows
# runners): without one they SKIP with the reason. Before each arm the harness sets the network fault, MEASURES the echo RTT, and a mismatch makes the arm
# UNVERIFIABLE (never a pass). `nohelper` (no remote helper on the server) is the default and always the first arm; `helper` pre-installs the binary named by
# APPCI_LAB_HELPER_BIN (a built phosphor-remote; the lab blocks outbound network, so nothing could be downloaded anyway) with never_install; when that
# variable is not set the binary is the `lab-helper` job's artifact in this checkout (./helper-artifacts/, #2062) or this checkout's entry in the machine-local
# helper cache (script/ci/lab_helper.py, filled by the manual `lab-helper` job:
# resolve_helper_bin), and the arm SKIPs with `NEEDS: ...` (helper_missing_reason; round 15, #2063) when there is neither. The Wave H arms (#1804) use the same binary as the bundle the app uploads: `upload` (nothing on the server, always_install), `upgrade` (older
# versions pre-seeded), `never` (per-host policy never: nothing may be uploaded, the session still works). Scenario user p_noscp refuses scp/sftp.
#
# [hook] marks a check that rests on an app log line the Wave 0 code adds. The patterns below are the agreed names; until the app logs them the check is
# UNVERIFIABLE (r.unverifiable: status noref), never a pass.
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402  (cases.py imports this module at its end)
import cases_caseremote as CR  # noqa: E402
import cases_coverage_a as CA  # noqa: E402
import lab as L  # noqa: E402

HOOK_TAB_INSERT = re.compile(r"hop=tab_insert\b|completion_inserted")           # [hook] the completion text reached the editor
HOOK_TAB_MENU = re.compile(r"hop=tab_menu\b[^\n]*items=(\[[^\n]*\])")             # [hook] the ambiguous-prefix menu and its items
HOOK_CWD_LISTING = re.compile(r"hop=cwd_listing session=(\d+) kind=([LPT]) entries=(\d+)")            # the remote shell's pushed cwd listing arrived (#1801): ids and counts only
HOOK_PATH_DETECT = re.compile(r"hop=path_detect source=pushed session=(\d+) entries=(\d+) complete=(true|false)")   # path detection answered from that listing, no round trip (ids and counts only)
SESSION_ABANDONED = "Session abandoned before bootstrap"


def lab_case(name, shells=("bash", "zsh", "fish", "nu"), lat=("lan", "150"), helper=("nohelper",), faults=(), scenarios=(), flavor="full", modes=("terminal", "agent")):
    """register the body `fn(ctx, r, arm)` once per arm of the declared matrix. `shells`/`lat`/`helper` are the FULL matrix the case supports; APPCI_LAB_MATRIX
    narrows it at run time (run.py). `faults`: network faults that get their own arm (bash/zsh, LAN and 150 only). `scenarios`: scenario users (bash)."""
    def deco(fn):
        for arm in L.expand_arms(name, shells, lat, helper, faults, scenarios):
            arm = dict(arm, flavor=flavor)
            cases.CASES.append(dict(name=arm["name"], modes=modes, gui=True, order=700 + 100 * L.HELPERS.index(arm["helper"]), fn=_runner(fn, arm), lab=True, arm=arm))
        return fn
    return deco


def _runner(fn, arm):
    def run(ctx, r):
        # lab_prepare points ctx.remote at the lab arm BEFORE its gates (helper binary, RTT, relaunch), and each gate leaves by `return False` or by a
        # skip/unverifiable exception. Job 17885063245: all eight `.helper` arms left that way (no APPCI_LAB_HELPER_BIN) with the swap still in place, and
        # the runner's leak check reported each as `harness_state_reset_after_<case>` FAIL. The cleanup therefore covers lab_prepare too.
        try:
            if not lab_prepare(ctx, r, arm):
                return
            fn(ctx, r, arm)
        finally:
            lab_cleanup(ctx, arm)
    run.__name__ = arm["name"]
    return run


# helper arm -> (ssh_extension_install_mode, the helper must be offered to the app as its bundled asset)
HELPER_ARM_MODE = {"nohelper": "never_install", "helper": "never_install", "upload": "always_install", "upgrade": "always_install", "never": "always_install"}
HELPER_ASSET_ENV = "PHOSPHOR_REMOTE_HELPER_ASSET_DIR"           # remote_server::setup::HELPER_ASSET_DIR_ENV
SEED_VERSIONS = ("phosphor-remote-v0.0.1", "phosphor-remote-v0.0.2", "phosphor-oss", "phosphor-oss-v0.0.3")   # what an `upgrade` arm finds on the server


def helper_version_line(binary):
    """first line of `<binary> --version`: `phosphor-remote <version> protocol <n>`; None when it does not run. Where the binary cannot be STARTED at all (a
    Linux helper on a macOS or Windows runner: the lab host runs it, not this machine) the line recorded next to it when it was built is read instead
    (`<binary>.version`, written by script/ci/lab_helper.py). A helper that starts and fails is never excused by that file."""
    import subprocess
    try:
        out = subprocess.run([binary, "--version"], capture_output=True, text=True, timeout=20, stdin=subprocess.DEVNULL)
    except subprocess.SubprocessError:
        return None
    except OSError:
        try:
            line = open(binary + ".version", encoding="utf-8").readline().strip()
        except OSError:
            return None
        return line if line.startswith("phosphor-remote ") and len(line.split()) == 4 else None
    line = (out.stdout.splitlines() or [""])[0]
    return line if out.returncode == 0 and line.startswith("phosphor-remote ") else None


def lab_helper_cache():
    """script/ci/lab_helper.py as a module (the cached lab helper, docs/design/r12-policy.md 3); None when the checkout has no such file"""
    import importlib.util
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "ci", "lab_helper.py")
    if not os.path.isfile(path):
        return None
    spec = importlib.util.spec_from_file_location("appci_lab_helper_cache", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def resolve_helper_bin(env=None):
    """-> (path, why). The helper the non-`nohelper` arms use, first hit wins:
      1. APPCI_LAB_HELPER_BIN when it is set (an explicit choice always wins, and a wrong one is not papered over);
      2. the `lab-helper` job's artifact in this checkout, ./helper-artifacts/ (#2062: how the Mac and the Windows VM get it, `needs: lab-helper`), when
         it is THIS checkout's helper (script/ci/lab_helper.py artifact_entry: same key, or exported by this pipeline);
      3. this checkout's entry in the machine-local cache the same job fills on the build box (built once per set of helper inputs, not in every dev build).
    ('', reason) when none names a file. The reason carries the `NEEDS: ...` line of the last `lab-helper` build that could not start for a missing tool
    (from the artifact, else from the cache root), and on Linux the same line for zig when zig is not where that build looks for it: the row then says which
    machine change is missing, not only which job to play."""
    env = os.environ if env is None else env
    explicit = env.get("APPCI_LAB_HELPER_BIN", "")
    if explicit:
        return (explicit, None) if os.path.isfile(explicit) else ("", "APPCI_LAB_HELPER_BIN does not name a built phosphor-remote binary")
    cache = lab_helper_cache()
    if cache is None:
        return "", "APPCI_LAB_HELPER_BIN is not set and this checkout has no script/ci/lab_helper.py"
    art, art_why = cache.artifact_entry(cache.ROOT, env)
    if art is not None:
        return os.path.join(art, cache.BIN), None
    entry, why = cache.cached_entry(cache.ROOT, env)
    if entry is not None:
        return os.path.join(entry, cache.BIN), None
    if art_why:
        return "", "APPCI_LAB_HELPER_BIN is not set and %s" % art_why
    why = "APPCI_LAB_HELPER_BIN is not set, this checkout has no helper-artifacts/ from `lab-helper`, and the lab helper cache has none for it: %s" % why
    if "NEEDS:" not in why and sys.platform.startswith("linux"):
        needs = cache.zig_needs(env)
        if needs:
            why += ". " + needs
    return "", why


# round 15 (#2063; the machine change is #2062): what a helper arm says when there is no helper file for this checkout. The box builds it (the `lab-helper`
# job, script/ci/lab_helper.py: zig + cargo-zigbuild + the musl rust target, none of which the job installs); the macOS and Windows lab jobs take that
# job's artifact (helper-artifacts/, `needs: lab-helper` in .gitlab-ci.yml; resolve_helper_bin finds it and checks it is this checkout's, #2062). ci_seq.py plays lab-helper first in the candidate and release tiers.
HELPER_NEEDS = ("NEEDS: a built lab helper for this checkout: zig, cargo-zigbuild and the x86_64-unknown-linux-musl rust target for the runner user on the Linux box "
                "(owner, machine change, #2062), then the `lab-helper` job played on this ref (the macOS and Windows lab jobs take its artifact)")


def helper_missing_reason(helper_arm, why):
    """the SKIP text of a helper arm that has no helper file: the arm, what was looked for (resolve_helper_bin's reason), and the machine change"""
    return "%s arm: no helper file (%s). %s" % (helper_arm, why, HELPER_NEEDS)


# Arms in which the APP uploads (or must refuse to upload) the helper. A bundled build with no pinned helper digest refuses EVERY upload by design
# (setup::decide_helper_digest, INSTALL_REFUSED_NO_PINNED_DIGEST), which is what CI's dev builds are (.gitlab-ci.yml, "Build jobs"): there an `upload` /
# `upgrade` arm can only fail and a `never` arm can only pass, whatever the app does. The job says so with APPCI_APP_PINNED_DIGESTS=0 and these arms are then
# UNVERIFIABLE with the reason. Unset keeps the behaviour for a source build (it computes the digest of the asset it is handed); 1 is a build with pins.
UPLOAD_ARMS = ("upload", "upgrade", "never")


def helper_file_name(version_line):
    """the file the app installs for that helper: phosphor-remote-<version> (setup::helper_file_name)"""
    return "phosphor-remote-" + version_line.split()[1]


def host_label(arm):
    """the key the app stores a host's helper policy under (connection_label_for_session_info: user@host)"""
    return "%s@%s" % (arm["user"], host_of(arm))


ELF_MACHINES = {62: "x86_64", 183: "aarch64"}       # e_machine: EM_X86_64, EM_AARCH64 (the two the app has an asset name for)
LAB_HOST_ARCH = "x86_64"                            # the lab image is built and run on the x86_64 box (script/ci/lab_helper.py TARGET)


def helper_arch(helper_bin):
    """the CPU the helper binary is built FOR, from its ELF header ('x86_64' | 'aarch64'); None when it is not an ELF file of one of the two.
    #2042: the bundle has to carry the name the app asks for, which is the REMOTE's `uname -m` (remote_server::setup::helper_asset_name), and the
    helper runs on the lab host. The machine the harness runs on has nothing to do with it: an arm64 Mac staged `linux-aarch64` for an x86_64 lab."""
    try:
        with open(helper_bin, "rb") as f:
            head = f.read(20)
    except OSError:
        return None
    if len(head) < 20 or head[:4] != b"\x7fELF" or head[5] not in (1, 2):
        return None
    return ELF_MACHINES.get(int.from_bytes(head[18:20], "little" if head[5] == 1 else "big"))


def stage_helper_asset(ctx, helper_bin):
    """gzip the helper into <run>/helper-asset/phosphor-remote-linux-<arch>.gz and point the app at it (the env is read at launch). Returns the directory.
    <arch> is the helper's own (helper_arch); a file that is not such an ELF gets the lab host's, so a wrong file fails at the host, not by a missing name."""
    import gzip, shutil
    arch = helper_arch(helper_bin) or LAB_HOST_ARCH
    d = os.path.join(ctx.run, "helper-asset")
    os.makedirs(d, exist_ok=True)
    with open(helper_bin, "rb") as src, gzip.open(os.path.join(d, "phosphor-remote-linux-%s.gz" % arch), "wb", 9) as dst:
        shutil.copyfileobj(src, dst)
    os.environ[HELPER_ASSET_ENV] = d
    return d


def server_helper_files(lab, arm):
    """the phosphor-* file names in the lab user's ~/.phosphor/remote-server (one-shot ssh as that user), sorted; None when the listing failed"""
    try:
        out = lab.ssh(arm["user"], "ls -1A ~/.phosphor/remote-server 2>/dev/null")
    except Exception:
        return None
    return sorted(n for n in out.stdout.split() if n.startswith("phosphor-"))


def set_denied_hosts(ctx, hosts):
    """warpify.ssh.remote_helper_denied_hosts in the profile's settings.toml (read when the app starts)"""
    cfg = ctx.prof.home + "/.config/phosphor/settings.toml"
    t = open(cfg).read()
    arr = "[" + ", ".join('"%s"' % h for h in hosts) + "]"
    open(cfg, "w").write(CR.T.set_key(t, "warpify.ssh", "remote_helper_denied_hosts", arr))


def lab_prepare(ctx, r, arm):
    """True when the arm may run. Skips (no lab / wrong image / no helper file: NEEDS), or marks the arm UNVERIFIABLE and returns False (a helper file that is
    not a helper, a build that refuses uploads, RTT gate mismatch)."""
    lab = getattr(ctx, "lab", None)
    if lab is None:
        r.skip(getattr(ctx, "lab_why", None) or "no fake ssh lab for this pass (APPCI_LAB, docs/testing.md 2.15)")
    if arm.get("flavor") and lab.info.get("flavor") not in (None, arm["flavor"]):
        r.skip("needs the %s lab image; this pass started the %s one" % (arm["flavor"], lab.info.get("flavor")))
    ctx._lab_prev = (ctx.remote, ctx.prof.remote)          # restored by lab_cleanup: a non-lab case in a mixed run must read the same-machine remote
    ctx.arm = arm
    rem = L.LabRemote(lab, arm["user"], arm["shell"])
    ctx.remote = ctx.prof.remote = rem
    helper_bin = ""
    if arm["helper"] != "nohelper":
        if arm["helper"] in UPLOAD_ARMS and os.environ.get("APPCI_APP_PINNED_DIGESTS") == "0":
            r.unverifiable("%s arm: the app under test is a bundled build with no pinned helper digest (APPCI_APP_PINNED_DIGESTS=0), which refuses every helper "
                           "upload by design; the arm would prove nothing about it. Needs a build that pins the lab helper's digest (docs/design/r12-policy.md 3)" % arm["helper"])
            return False
        helper_bin, why = resolve_helper_bin()
        if not helper_bin:
            # round 15 (#2063, #2062): no helper file is a missing machine capability, not missing evidence: a SKIP that names the change (it was a NOREF
            # row, 40 a run, whose text did not say what to do). The runner's cleanup undoes the swap above (cases_lab._runner).
            r.skip(helper_missing_reason(arm["helper"], why))
        line = helper_version_line(helper_bin)
        if line is None:
            r.unverifiable("%s arm: `%s --version` did not print `phosphor-remote <version> protocol <n>`; APPCI_LAB_HELPER_BIN must be a built phosphor-remote" % (arm["helper"], helper_bin))
            return False
        arm["helper_file"] = helper_file_name(line)
        os.environ["APPCI_LAB_HELPER_BIN"] = helper_bin      # the cases that restage the asset read it (cases_lab_helper.py)
    if arm["helper"] == "helper":
        # already-installed state; the file name carries the version, which must be the one the app under test computes (built from the same tree)
        lab.helper(arm["user"], {arm["helper_file"]: open(helper_bin, "rb").read()})
    elif arm["helper"] == "upgrade":
        lab.helper(arm["user"], {n: b"#!/bin/sh\nexit 0\n" for n in SEED_VERSIONS})
    else:
        lab.helper(arm["user"])
    if arm["helper"] in ("upload", "upgrade", "never"):
        stage_helper_asset(ctx, helper_bin)
    elif HELPER_ASSET_ENV in os.environ:
        del os.environ[HELPER_ASSET_ENV]
    if arm["helper"] != ctx.lab_helper_mode:
        set_denied_hosts(ctx, [host_label(arm)] if arm["helper"] == "never" else [])
        if not CR.set_ext_mode(ctx, HELPER_ARM_MODE[arm["helper"]]):          # relaunch only when the helper mode changes (arms are ordered helper-major)
            r.skip("the app could not be relaunched for the helper mode change")
        ctx.lab_helper_mode = arm["helper"]
    lab.fault(rtt_ms=arm["lat_ms"]) if arm["lat_ms"] else lab.clear_fault()
    gate = L.rtt_gate_verdict(lab.measure_rtt(15), arm["lat_ms"])
    r.metric("rtt_gate", gate)
    if gate["verdict"] != "ok":
        r.unverifiable("RTT gate %s: measured median %s ms for a target of %s ms (tolerance %s); the arm is not run, a latency arm that cannot prove its network proves nothing" % (
            gate["verdict"], gate["median"], gate["target"], gate.get("tol", L.RTT_TOL_MS)))
        lab.clear_fault()
        return False
    try:
        lab.clock_offset()
    except L.LabError:
        pass
    return True


def lab_cleanup(ctx, arm):
    try:
        ctx.lab.clear_fault()
    except Exception:
        pass
    ctx.arm = None
    prev = getattr(ctx, "_lab_prev", None)
    if prev:
        ctx.remote, ctx.prof.remote = prev
        ctx._lab_prev = None


# ------------------------------------------------------------------ helpers shared by the cases
def host_of(arm):
    return "lab-" + arm["user"]


def work(arm, name=""):
    return "/work/%s/%s" % (arm["user"], name)


def connect_or_fail(ctx, r, arm, label=""):
    secs = CR.connect(ctx, host_of(arm), 90 if arm["lat_ms"] < 300 else 150)
    r.check(secs is not None, "%sno remote prompt after `ssh %s`. Test host: %s" % (label, host_of(arm), ctx.ssh_diag() if secs is None else ""))
    return secs


# ---- #2019: a host whose LOGIN shell is fish or nu gets a plain ssh session, by decision (DECLINED.md, PLAIN_SESSION_DECLINED). The agent works on a
# remote host through the managed session, so an agent-mode arm on such a host has nothing to judge: in a plain session the keys are ssh's and the agent
# prompt would be typed into the remote shell. The premise is read from the APP'S OWN LOG of this connect, never from the arm's name alone.
PLAIN_SESSION_LOGIN_SHELLS = ("fish", "nu")
PLAIN_SESSION_HOOK = "Received PlainSSHFallback hook"                                            # ansi/mod.rs: log::info!("Received {} hook")
MANAGED_SESSION_RE = re.compile(r"Received ManagedSSHSessionStarting hook|hop=ssh_wrapper verdict managed=true")
PLAIN_SESSION_DECLINED = "DECLINED.md, 'A remote host whose login shell is fish or nu gets a plain ssh session: no agent tools on that host (#2019)'"


def plain_session_limit(shell, log_text):
    """pure: the reason an agent-mode arm cannot be judged, or None when it must be judged in full. A reason needs ALL of: the arm's login shell is one the
    app gives a plain session by decision (fish, nu); the app log of this connect says it took the plain session (the PlainSSHFallback hook); and it has no
    managed-session line. So a bash / zsh / dash / ash / pwsh arm never gets one (a plain session there is a defect and the case's own checks fail), and a
    fish / nu arm that got a managed session, or whose connect logged neither line, is judged like any other."""
    if shell not in PLAIN_SESSION_LOGIN_SHELLS:
        return None
    text = log_text or ""
    if PLAIN_SESSION_HOOK not in text or MANAGED_SESSION_RE.search(text):
        return None
    return ("KNOWN LIMITATION (owner decision, #2019): the host's login shell is %s, which gets a plain ssh session by decision (the app log of this "
            "connect has '%s' and no managed-session line), and the agent's remote tools need a managed session. Not judged; see %s" % (
                shell, PLAIN_SESSION_HOOK, PLAIN_SESSION_DECLINED))


def agent_connect(ctx, r, arm):
    """connect for a case in which the AGENT works on the remote host. True = connected and the case goes on. False = the case ends here: either no remote
    prompt (the failed check of connect_or_fail, pane reset), or the arm is the decided limitation (plain_session_limit over the app log of this connect):
    the row is then UNVERIFIABLE with the DECLINED reference, the agent steps are NOT typed (in a plain session they would go to the remote shell), and the
    remote session is left through finish_remote, whose check stands."""
    off = ctx.log_size()
    if connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return False
    time.sleep(1.0 + 2 * rtt_s(arm))
    why = plain_session_limit(arm["shell"], ctx.log_since(off))
    if why is None:
        return True
    r.metric("plain_session_by_decision", arm["shell"])
    r.unverifiable(why)
    finish_remote(ctx, r)
    return False


def plain_session_feature_limit(shell, log_text, feature):
    """pure: plain_session_limit for a TERMINAL-mode case about a remote feature of the managed session (the code-review panel, the remote editor, the git
    dialog). Same premise, read from the app log of this connect; None = judge the arm in full."""
    if plain_session_limit(shell, log_text) is None:
        return None
    return ("KNOWN LIMITATION (owner decision, #2019): the host's login shell is %s, which gets a plain ssh session by decision (the app log of this connect "
            "has '%s' and no managed-session line); %s needs a managed session. Not judged; see %s" % (shell, PLAIN_SESSION_HOOK, feature, PLAIN_SESSION_DECLINED))


# ---- round 15 (#2063): keyboard drivers for the panels the lab cases had called unreachable. The command palette runs an action by its name; the names are
# the English keybinding descriptions (app/i18n/en/warp.ftl: keybinding-desc-workspace-toggle-right-panel, -toggle-project-explorer), pinned by the self-test.
PALETTE_CODE_REVIEW = "Toggle code review"
PALETTE_PROJECT_EXPLORER = "Toggle project explorer"
HOOK_RIGHT_PANEL = re.compile(r"hop=right_panel open=(true|false) repo_context=(true|false)")     # workspace/view/right_panel.rs right_panel_trace_line (trace only)


def right_panel_states(text):
    """[open: bool] of every right-panel state change the app logged in `text`, in order"""
    return [m.group(1) == "true" for m in HOOK_RIGHT_PANEL.finditer(text)]


def palette_run(ctx, text, tag=None):
    """the command palette by keyboard: open it, type the action's name, Enter. With `tag`, a screenshot of the filtered palette first (the evidence of which
    entry Enter ran)."""
    import cases_ui as U
    ctx.be.guard()
    ctx.be.key(U.K(ctx, "palette"))
    time.sleep(1.0)
    ctx.be.type(text, 15)
    time.sleep(0.9)
    if tag:
        ctx.shot(tag)
    ctx.be.key("Return")
    time.sleep(0.9)


def open_code_review(ctx, off, tag):
    """open the code-review (right) panel by keyboard and say what the app logged about it: 'open' (hop=right_panel open=true after this call), 'unknown' (no
    hop=right_panel line: a build before the trace line, or the palette entry did not run). A panel that was already open is toggled CLOSED by the first
    Enter (the app says open=false): it is then toggled once more."""
    for attempt in (1, 2):
        mark = ctx.log_size()
        palette_run(ctx, PALETTE_CODE_REVIEW, "%s_palette_%d" % (tag, attempt))
        st = ctx.until(lambda: right_panel_states(ctx.log_since(mark)), 4, step=0.1) or []
        if not st:
            return "unknown"
        if st[-1]:
            return "open"
    return "unknown"


def close_code_review(ctx, r, tag):
    """close the panel this case opened (the next case must find the window as every other case leaves it); closing gives the focus back to the terminal
    (Workspace::update_right_panel_open_state). A close the app does not confirm is a note on the row, with the screenshot."""
    mark = ctx.log_size()
    palette_run(ctx, PALETTE_CODE_REVIEW)
    st = ctx.until(lambda: right_panel_states(ctx.log_since(mark)), 4, step=0.1) or []
    if st and st[-1]:                   # it was closed already and this opened it: once more
        palette_run(ctx, PALETTE_CODE_REVIEW)
    elif not st:
        ctx.shot("%s_close_unconfirmed" % tag)
        r.note("the code-review panel was asked to close by the palette and the app logged no hop=right_panel line (shots/*%s_close_unconfirmed.png)" % tag)


def hold_line(gate):
    """a line for the remote shell that keeps the foreground busy (no prompt comes back) until the file `gate` exists; the harness creates it over its own ssh
    connection, so the prompt can be made to return WITHOUT a key typed into the app (the terminal need not have the focus). `sh -c '...'` with no `!`, no
    `$` and no backslash: bash, zsh, dash, ash, fish, nu and pwsh all hand the quoted text to sh unchanged."""
    assert re.fullmatch(r"[\w./-]+", gate), gate
    return "sh -c 'until [ -e %s ]; do sleep 0.2; done'" % gate


def finish_remote(ctx, r, label=""):
    """every lab case ends with the pane provably LOCAL"""
    ok = CR.leave(ctx)
    r.check(ok, "%s`exit` did not return to the local shell (%s)" % (label, ctx.left_unproven))
    ctx.reset()
    return ok


def rec_ts(ctx, tok):
    """epoch (REMOTE clock) of the first rec.log entry containing tok, or None"""
    for ts, cmd in ctx.remote.commands():
        if tok in cmd:
            return ts
    return None


def rtt_s(arm):
    return arm["lat_ms"] / 1000.0


def type_pace(ctx):
    return ctx.rng.randint(10, 25)


def unique(ctx, n=6):
    return ctx.tok(n).lower().replace("_", "")


# ------------------------------------------------------------------ 1. Tab completion of a remote directory
@lab_case("tab_remote_dir", shells=("bash", "zsh", "fish", "nu"), lat=("lan", "150", "300"), helper=("nohelper", "helper"),
          scenarios=("p_banner", "p_colon", "p_question", "p_slowrc", "p_rcerr"))
def c_tab_remote_dir(ctx, r, arm):
    """A unique prefix + Tab completes a REMOTE directory (proof: the completed `cd` really ran there: `&& pwd >> file` writes the exact directory); an ambiguous
    prefix offers the shell's OWN candidates (the lab's /complete runs bash-completion / zpty / fish `complete -C`; the app's menu [hook] must equal it);
    Tab -> insert <= 1 RTT + 100 ms [hook]; and with Enter 30 ms after Tab the COMPLETED line runs (the Tab+Enter rule, owner 2026-10-31)."""
    lab, u, base, rt = ctx.lab, arm["user"], work(arm), rtt_s(arm)
    uid = unique(ctx)
    fx = "tabfx_" + uid
    lab.tree(u, {"%s/zzq%s_only/x" % (fx, uid): "", "%s/amb%s_a/x" % (fx, uid): "", "%s/amb%s_b/x" % (fx, uid): ""})
    CR.rm_remote(ctx, "tab.txt", "tab2.txt")
    if connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    # (a) unique prefix
    off = ctx.log_size()
    ctx.be.type("cd %s%s/zzq%s" % (base, fx, uid), type_pace(ctx))
    t0 = time.time()
    ctx.be.key("Tab")
    hit = ctx.until(lambda: HOOK_TAB_INSERT.search(ctx.log_since(off)), 3 + 3 * rt, step=0.02)
    if hit:
        took = time.time() - t0
        r.metric("tab_insert_s", round(took, 3))
        r.check(took <= rt + 0.1 + 0.05, "Tab -> insert took %.0f ms; the budget is 1 RTT (%d ms) + 100 ms (+50 ms poll slack)" % (took * 1000, arm["lat_ms"]))
    else:
        r.unverifiable("[hook] no tab-insert line in the app log (hop=tab_insert / completion_inserted): the Tab -> insert time is not measured until the Wave 0 code logs it")
        time.sleep(0.5 + 2 * rt)
    ctx.be.type(" && pwd >> %stab.txt" % base, type_pace(ctx), enter=True)
    got = ctx.until(lambda: ctx.remote.lines("tab.txt"), 10 + 6 * rt)
    CA.exact_lines(r, "unique prefix + Tab: the remote directory the completed `cd` entered", ["%s%s/zzq%s_only" % (base, fx, uid)], got)
    ctx.shot("lab_tab_unique")
    # (b) ambiguous prefix: the shell's own candidates vs the app's menu
    line = "cd %s%s/amb%s" % (base, fx, uid)
    comp = lab.complete(u, line)
    cand = comp.get("candidates")
    if cand is None:
        r.unverifiable("/complete has no driver for %s: the shell's own candidate list is not compared" % arm["shell"])
    else:
        names = {c.rstrip("/").rsplit("/", 1)[-1] for c in cand}
        r.check({"amb%s_a" % uid, "amb%s_b" % uid} <= names, "the lab's own %s candidates for %r are %s, expected both amb%s_a and amb%s_b" % (arm["shell"], line, sorted(names), uid, uid))
    off = ctx.log_size()
    ctx.be.type(line, type_pace(ctx))
    ctx.be.key("Tab")
    m = ctx.until(lambda: HOOK_TAB_MENU.search(ctx.log_since(off)), 3 + 3 * rt, step=0.05)
    ctx.shot("lab_tab_menu")
    if m and cand is not None:
        shown = {x.strip(" \"'").rstrip("/").rsplit("/", 1)[-1] for x in m.group(1).strip("[]").split(",") if x.strip()}
        r.check(shown == names, "the app's completion menu shows %s, the shell's own candidates are %s" % (sorted(shown), sorted(names)))
    elif not m:
        r.unverifiable("[hook] no tab-menu line in the app log (hop=tab_menu items=[...]): the menu is not compared with the shell's candidates")
    ctx.reset()
    # (c) FAST ENTER: Enter 30 ms after Tab runs the COMPLETED line
    n0 = len(ctx.remote.commands())
    ctx.be.type("cd %s%s/zzq%s" % (base, fx, uid), type_pace(ctx))
    ctx.be.key("Tab")
    time.sleep(0.03)
    ctx.be.key("Return")
    time.sleep(0.5 + 3 * rt)
    ctx.be.type("pwd >> %stab2.txt" % base, type_pace(ctx), enter=True)
    got2 = ctx.until(lambda: ctx.remote.lines("tab2.txt"), 10 + 6 * rt)
    CA.exact_lines(r, "Tab then Enter 30 ms later: the directory the shell entered (the COMPLETED line ran)", ["%s%s/zzq%s_only" % (base, fx, uid)], got2)
    recs = [c for _, c in ctx.remote.commands()[n0:] if c.startswith("cd ")]
    if recs:
        r.check(any(("zzq%s_only" % uid) in c for c in recs), "the shell received %r, not the completed directory" % recs)
    finish_remote(ctx, r)


# ------------------------------------------------------------------ 2. path detection while typing
@lab_case("path_detect", shells=("bash", "zsh", "fish", "nu"), lat=("lan", "150", "300"), helper=("nohelper", "helper"), scenarios=("p_exec", "p_tinypath"))
def c_path_detect(ctx, r, arm):
    """`cd` into a remote fixture directory, then type a subdirectory's name (no Tab, no Enter): the remote shell pushed its bounded cwd listing with the prompt
    (hop=cwd_listing, #1801: ids and counts only, never a name) and the app detects the typed word from it (hop=path_detect source=pushed), instantly, from the
    cache. Typing it opens ZERO new exec channels on the server (sshd log 'Starting session: command'): the no-helper path must not run a command per keystroke."""
    lab, u, base, rt = ctx.lab, arm["user"], work(arm), rtt_s(arm)
    uid = unique(ctx)
    lab.tree(u, {"pd%s/a.txt" % uid: "", "pd%s/sub/b.txt" % uid: ""})          # the fixture directory holds exactly 2 entries: a.txt and sub/
    if connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(2.0 + 3 * rt)                         # the connection's own start-up channels settle first
    off = ctx.log_size()
    ctx.be.type("cd %spd%s" % (base, uid), type_pace(ctx), enter=True)
    got = ctx.until(lambda: [m for m in HOOK_CWD_LISTING.finditer(ctx.log_since(off)) if m.group(3) == "2"], 10 + 6 * rt, step=0.1)
    if got:
        r.check(got[-1].group(2) == "L", "the pushed listing of the fixture directory is kind %s, expected L (complete)" % got[-1].group(2))
    else:
        r.unverifiable("[hook] no hop=cwd_listing line with entries=2 after the cd (the remote shell's bootstrap sent no listing, or the shell is not one that does): detection from the pushed listing is not proven")
    time.sleep(0.5 + 2 * rt)
    size0 = lab.sshd_since(0)["size"]
    off = ctx.log_size()
    ctx.be.type("ls sub", 30)                         # a subdirectory of the cwd, typed whole: no Tab, no Enter
    hit = ctx.until(lambda: HOOK_PATH_DETECT.search(ctx.log_since(off)), 1.5 + 3 * rt, step=0.05)
    ctx.shot("lab_path_detect")
    if hit:
        r.check(hit.group(2) == "2" and hit.group(3) == "true", "path detection used a listing of %s entries (complete=%s), expected the fixture's 2 (complete)" % (hit.group(2), hit.group(3)))
    else:
        r.unverifiable("[hook] no hop=path_detect source=pushed line in the app log: detection from the pushed listing is not proven; only the channel count below is checked")
    time.sleep(0.5 + 2 * rt)
    starts = L.count_starts(lab.sshd_since(size0)["data"])
    r.metric("exec_channels_while_typing", starts)
    r.check(starts["command"] == 0 and starts["subsystem"] == 0, "typing a remote path opened %d exec and %d subsystem channel(s) on the server (expected none)" % (starts["command"], starts["subsystem"]))
    ctx.reset()
    finish_remote(ctx, r)


# ------------------------------------------------------------------ 3. git chips
@lab_case("chips", shells=("bash", "zsh", "fish", "nu"), lat=("lan", "150"), helper=("nohelper", "helper"))
def c_chips(ctx, r, arm):
    """In a git directory on the remote the branch chip is on screen (OCR) and the app made at most ONE batched side call per prompt (exec channels in the sshd
    log; with the helper it may make none)."""
    lab, u, base, rt = ctx.lab, arm["user"], work(arm), rtt_s(arm)
    uid = unique(ctx)
    br = "br" + uid          # a unique branch name: the chip OCR looks for THIS token, not for "main" (which any other text on screen could contain)
    lab.tree(u, {"README": "x\n", "src/a.rs": "fn main(){}\n"}, dir="chips" + uid, git=True, branch=br, branches=("feature-x",))      # <work>/chips<uid> is the git repo
    if connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    size0 = lab.sshd_since(0)["size"]
    p0 = ctx.remote.prompts()
    ctx.be.type("cd %schips%s" % (base, uid), type_pace(ctx), enter=True)
    seen, txt = False, None
    for i in range(6):
        time.sleep(1.0 + 2 * rt)
        txt = ctx.ocr("lab_chips_%d" % i)
        if txt is None:
            break
        if br in txt:
            seen = True
            break
    if txt is None:
        r.unverifiable("no OCR on this machine (tesseract): the branch chip on screen is not checked")
    else:
        r.check(seen, "no branch chip (branch %s) on screen after cd into a git directory on the remote" % br)
    prompts = max(1, ctx.remote.prompts() - p0)
    calls = L.count_starts(lab.sshd_since(size0)["data"])["command"]
    r.metric("side_calls", calls)
    r.metric("prompts", prompts)
    r.check(calls <= prompts, "%d side-call exec channels for %d prompt(s): the app must batch to at most one per prompt" % (calls, prompts))
    if arm["helper"] == "nohelper":
        r.check(calls >= 1, "no side call at all with no helper: where did the branch chip come from?")
    finish_remote(ctx, r)


# ------------------------------------------------------------------ 4. Enter -> command start, paired with plain ssh
@lab_case("enter_start", shells=("bash", "zsh", "fish", "nu", "pwsh", "dash", "ash"), lat=("lan", "150", "300"), helper=("nohelper", "helper"),
          scenarios=("p_sete", "p_colon"))
def c_enter_start(ctx, r, arm):
    """20 lines typed at 10-25 ms/key with an instant Enter, interleaved with the same lines typed into a PLAIN `ssh -tt` session (same lab user, same path):
    median(app) <= median(plain) + max(15 ms, 10 %) (owner 2026-11-08; the +-1 ms budget is for LAN key->echo only). The effect file proves every line arrived
    exactly once, in order.
    What is timed (round 10, #2027; the budget is unchanged):
      start  the moment the app RECEIVED the Enter, from its own log (lab.app_enter_epoch); the plain control: right before its CR write. Before, the app's
             Enter was stamped before the injector call, whose priming key and 20 ms wait on Wayland (about 35 ms in all) were charged to the app.
      stop   bash and zsh: the command's OWN start time, which it writes next to its token (lab.start_stamp_line), in both sessions. Before, the stop was the
             rec.log line, which those shells write at the NEXT prompt (after the command and the session's prompt hooks). nu: its rec.log line is written by
             pre_execution, which is the start; pwsh: unchanged (rec.log).
    The old quantities stay as metrics (enter_to_next_prompt_*_ms from rec.log, injector_enter_delay_ms), so a slow prompt hook is still seen, as a number."""
    lab, u, rt = ctx.lab, arm["user"], rtt_s(arm)
    n = ctx.count_lat(20)
    eff = work(arm, "es_%s.txt" % unique(ctx, 4))
    plain_eff = work(arm, "es_plain.txt")
    shell_kind = arm["shell"] if arm["shell"] in ("nu", "pwsh") else "bash"
    self_stamp = arm["shell"] in ("bash", "zsh")                 # $EPOCHREALTIME: bash >= 5, zsh with zsh/datetime (the lab's zsh hook loads it)
    plain = L.PlainSsh(os.path.join(ctx.prof.home, ".ssh", "config"), u)
    plain_up = False
    try:
        plain_up = plain.open(30 + 5 * rt) if not ctx.win else False
    except Exception as e:  # noqa: BLE001
        r.note("plain ssh control could not start: %r" % (e,))
    if not plain_up:
        r.no_reference("no plain ssh control (%s): the paired verdict cannot be given" % ("Windows" if ctx.win else "its session did not start"))
    if connect_or_fail(ctx, r, arm) is None:
        plain.close()
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    off = lab.offset or 0.0
    log_off = ctx.log_size()
    call_ts, plain_ts, rec_app, rec_plain, app_lines, app_toks, plain_toks = [], [], [], [], [], [], []
    timed = arm["shell"] not in ("fish", "dash", "ash")          # fish_rec.log has whole-second stamps; dash/ash have no history hook
    try:
        for i in range(n):
            tok = "es" + unique(ctx, 5)
            line = L.start_stamp_line(tok, eff) if self_stamp else ctx.append_cmd(shell_kind, tok, eff)
            ctx.be.type(line, type_pace(ctx))
            t_call = time.time()           # before the injector call; the verdict starts at the app's receipt of the Enter (see the docstring)
            ctx.be.key("Return")
            app_lines.append(line)
            app_toks.append(tok)
            call_ts.append(t_call)
            if timed:
                rec_app.append(ctx.until(lambda: rec_ts(ctx, tok), 10 + 4 * rt, step=0.01) or None)
            else:
                ctx.until(lambda: any(tok in x for x in ctx.remote.lines(eff)), 10 + 4 * rt, step=0.01)
            if plain_up:
                ptok = "ps" + unique(ctx, 5)
                pline = L.start_stamp_line(ptok, plain_eff) if self_stamp else ctx.append_cmd(shell_kind, ptok, plain_eff)
                plain_toks.append(ptok)
                plain_ts.append(plain.run_line(pline, rng=ctx.rng))
                if timed:
                    rec_plain.append(ctx.until(lambda: rec_ts(ctx, ptok), 10 + 4 * rt, step=0.01) or None)
                else:
                    ctx.until(lambda: any(ptok in x for x in ctx.remote.lines(plain_eff)), 10 + 4 * rt, step=0.01)
            time.sleep(0.05)
    finally:
        plain.close()
    ctx.until(lambda: len(ctx.remote.lines(eff)) >= n, 10 + 4 * rt)
    eff_lines = ctx.remote.lines(eff)
    CA.exact_lines(r, "effect file: every typed line arrived once, in order", app_toks, [(x.split() or [""])[0] for x in eff_lines])
    if timed:
        recs = [c for _, c in ctx.remote.commands() if any(t in c for t in app_toks)]
        CA.exact_lines(r, "rec.log: the shell received exactly the typed lines", app_lines, recs)
    pv = None
    if timed:
        log_lines = ctx.log_since(log_off).splitlines()
        seen = [L.app_enter_epoch(log_lines, t) for t in app_toks]
        app_enters = [s if s is not None else c for s, c in zip(seen, call_ts)]
        unseen = sum(1 for s in seen if s is None)
        if unseen:
            r.note("%d of %d Enters have no hop=enter_begin line with the line's text in the app log: those are timed from before the injector call, "
                   "which counts the injector's own time against the app" % (unseen, len(seen)))
        r.metric("injector_enter_delay_ms", [round((s - c) * 1000.0, 1) for s, c in zip(seen, call_ts) if s is not None])
        next_app = [None if ts is None else (ts - off - t) * 1000.0 for ts, t in zip(rec_app, app_enters)]
        next_plain = [None if ts is None else (ts - off - t) * 1000.0 for ts, t in zip(rec_plain, plain_ts)]
        r.metric("enter_to_next_prompt_app_ms" if self_stamp else "enter_to_rec_app_ms", next_app)
        r.metric("enter_to_next_prompt_plain_ms" if self_stamp else "enter_to_rec_plain_ms", next_plain)
        app_ms, plain_ms, basis = next_app, next_plain, "the shell's rec.log line"
        if self_stamp:
            a = L.enter_start_samples(app_toks, app_enters, L.start_stamps(eff_lines), off)
            p = L.enter_start_samples(plain_toks, plain_ts, L.start_stamps(ctx.remote.lines(plain_eff)), off) if plain_up else []
            if any(x is not None for x in a) and (not plain_up or any(x is not None for x in p)):
                app_ms, plain_ms, basis = a, p, "the command's own start stamp"
            else:
                r.note("this %s wrote no $EPOCHREALTIME start stamp: Enter -> command start falls back to the rec.log line (the next prompt) in BOTH sessions" % arm["shell"])
        r.metric("enter_start_basis", basis)
        r.metric("enter_start_app_ms", app_ms)
        r.metric("enter_start_plain_ms", plain_ms)
        pv = L.paired_verdict(app_ms, plain_ms) if plain_up else None
    r.metric("paired_verdict", pv)
    if pv is None:
        r.unverifiable("no paired timing for %s (%s)" % (arm["shell"], "no timestamped history hook for this shell" if not timed else "no plain control"))
    elif pv["verdict"] == "no-data":
        r.unverifiable("paired timing has too few samples: %s" % pv)
    else:
        r.check(pv["verdict"] == "pass", "Enter -> command start: app median %.1f ms vs plain ssh %.1f ms; allowed %.1f ms (over by %.1f ms)" % (pv["app"], pv["plain"], pv["allowed"], pv["over_ms"]))
    finish_remote(ctx, r)


# ------------------------------------------------------------------ 4b. Enter never waits on the network (#1970)
ENTER_WAIT_BUDGET_MS = 2.0          # owner's Enter budget: the app's own share of Enter; APPCI_LAB_ENTER_WAIT_BUDGET_MS overrides it for a diagnosis run


@lab_case("enter_no_network_wait", shells=("bash", "zsh", "fish", "nu"), lat=("150", "300"), helper=("nohelper", "helper"))
def c_enter_no_network_wait(ctx, r, arm):
    """#1970 ("Enter must never wait on the network"), judged from the APP's own log, no remote clock: lines typed at 10-25 ms/key with an instant Enter while
    the link has the arm's delay, then as many again in the SAME ssh session with the delay removed (the control; the RTT is measured again and the control is
    dropped when it is not LAN). Per Enter the app's `hop=enter_begin` -> line `pty_write` time (ctx.app_enter_write_samples). lab.enter_wait_verdict: no
    delayed sample reaches half the RTT (a wait on any round trip costs a whole one), and the delayed median is within the Enter budget of the undelayed one.
    The effect file holds every line of both phases once, in order. Each line is typed only after the previous one's effect is on the server and its prompt
    had 2 RTT to come back, so an Enter never meets a shell that is still busy. lab_enter_start is the other half: the whole Enter -> start against plain ssh."""
    lab, rt = ctx.lab, rtt_s(arm)
    n = ctx.count_lat(12)
    shell_kind = arm["shell"] if arm["shell"] in ("nu", "pwsh") else "bash"
    eff = work(arm, "enw_%s.txt" % unique(ctx, 4))
    budget = float(os.environ.get("APPCI_LAB_ENTER_WAIT_BUDGET_MS", ENTER_WAIT_BUDGET_MS))
    if connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    toks = []

    def phase(settle):
        t0 = time.time()
        for _ in range(n):
            tok = "enw" + unique(ctx, 5)
            toks.append(tok)
            ctx.be.type(ctx.append_cmd(shell_kind, tok, eff), type_pace(ctx), enter=True)
            ctx.until(lambda: any(tok in x for x in ctx.remote.lines(eff)), 10 + 4 * settle, step=0.02)
            time.sleep(0.15 + 2 * settle)
        time.sleep(0.3)
        return ctx.app_enter_write_samples(t0)

    delayed = phase(rt)
    lan_ms = None
    try:
        lab.clear_fault()
        gate = L.rtt_gate_verdict(lab.measure_rtt(15), 0)
        r.metric("control_rtt_gate", gate)
        if gate["verdict"] == "ok":
            lan_ms = phase(0.0)
        else:
            r.note("no undelayed control: after removing the delay the echo RTT measured %s ms (%s); only the half-RTT rule is judged" % (gate.get("median"), gate["verdict"]))
    except L.LabError as e:
        r.note("no undelayed control: the delay could not be removed (%s); only the half-RTT rule is judged" % (e,))
    CA.exact_lines(r, "effect file: every typed line arrived once, in order", toks, [x.strip() for x in ctx.remote.lines(eff)])
    v = L.enter_wait_verdict(delayed, lan_ms, arm["lat_ms"], budget_ms=budget)
    r.metric("enter_to_write_delayed_ms", delayed)
    r.metric("enter_to_write_lan_ms", lan_ms)
    r.metric("enter_wait_verdict", v)
    if v["verdict"] == "no-data":
        r.unverifiable("no Enter -> write times in the app log (%s): input-order logging is off, or the Enters took a path that logs no hop=enter_begin / pty_write" % v["why"])
    else:
        r.check(v["verdict"] == "pass", v["why"])
        if v["verdict"] == "pass":
            r.note(v["why"])
    finish_remote(ctx, r)


# ------------------------------------------------------------------ 4c. a key to a raw-input program, paired with plain ssh (#985)
PTY_WRITE_RE = re.compile(r"input_order pty_write bytes=(\d+) t=(\d+(?:\.\d+)?)")


def key_write_times(log_text, t0):
    """pure: the time (epoch seconds, this machine's clock) of every byte the APP wrote to its pty at or after wall time t0, in write order, from its
    `input_order pty_write bytes=N t=<ms>` lines (one per finished user write on every platform; replies to terminal queries are logged under another
    name and are not in it). A write of several bytes (keys the write queue coalesced) gives each byte that write's time. [] without such lines."""
    out = []
    for m in PTY_WRITE_RE.finditer(log_text):
        t = float(m.group(2)) / 1000.0
        if t >= t0:
            out += [t] * int(m.group(1))
    return out


def tos_note(app_tos, plain_tos):
    """pure: (in_cs1, words). `app_tos`: the TOS bytes of the app's ssh connection(s) to the lab, `plain_tos`: the plain control's (None = not seen).
    in_cs1: a connection of the app is in the lower-effort class CS1, which is what #985 was (OpenSSH <= 10.0 with a session without a tty on the
    connection). Any other difference from plain ssh is reported in words and judged by nobody: OpenSSH >= 10.1 marks a connection with such a session
    with its second IPQoS value, the system default, and the lab link treats every class alike."""
    in_cs1 = L.TOS_CS1 in app_tos
    if plain_tos is None or not app_tos:
        return in_cs1, "TOS of the app's ssh connection(s): %s; plain ssh: %s" % ([hex(x) for x in app_tos] or "not seen", "not seen" if plain_tos is None else hex(plain_tos))
    same = all(x == plain_tos for x in app_tos)
    return in_cs1, "TOS of the app's ssh connection(s): %s; plain ssh: %s (%s)" % ([hex(x) for x in app_tos], hex(plain_tos), "the same class" if same else "NOT the class plain ssh uses")


@lab_case("raw_key_arrival", shells=("bash", "zsh"), lat=("lan", "150"), helper=("nohelper", "helper"))
def c_raw_key_arrival(ctx, r, arm):
    """#985 ("keystrokes to raw-input programs arrive 40-100 ms late because the remote-server proxy shares the interactive ssh connection"): a program on the
    server reads its tty one byte at a time and stamps each byte (lab.RAW_KEY_LOGGER). Keys are typed at 10-25 ms into the app's managed session and,
    interleaved, written to a PLAIN `ssh -tt` session of the same user running the same program.
      delivery time of a key = the server's stamp (mapped to this clock by /health) minus the moment the key was handed to ssh: the app's own pty write
      (its `input_order pty_write` line), the plain control's write to its pty. What the app does BEFORE that write is other cases' subject (latency_key_log).
      verdict  median(app) <= median(plain) + max(15 ms, 10 %) (lab.paired_verdict, the owner's remote budget of 2026-11-08), and the logger read exactly the
               typed keys, in order, in both sessions.
    The `helper` arms have the remote-server proxy as a second session on the connection, the `nohelper` arms do not: that pair is the issue's A/B.
    The lab link delays every packet alike, so the DSCP class cannot show as time here. It is therefore read directly where the machine can
    (`ss --tos`, Linux): a connection of the app in CS1, the class behind the issue's numbers, fails the arm; any other difference from the plain control
    is a note and a metric (tos_app / tos_plain)."""
    lab, u, rt = ctx.lab, arm["user"], rtt_s(arm)
    n = ctx.count_lat(30)
    script = work(arm, L.RAW_KEY_LOGGER_NAME)
    uid = unique(ctx, 4)
    app_out, plain_out = work(arm, "rk_app_%s.txt" % uid), work(arm, "rk_plain_%s.txt" % uid)
    lab.tree(u, {L.RAW_KEY_LOGGER_NAME: L.RAW_KEY_LOGGER})
    plain = L.PlainSsh(os.path.join(ctx.prof.home, ".ssh", "config"), u)
    plain_up = False
    try:
        plain_up = plain.open(30 + 5 * rt) if not ctx.win else False
    except Exception as e:  # noqa: BLE001
        r.note("plain ssh control could not start: %r" % (e,))
    if not plain_up:
        r.no_reference("no plain ssh control (%s): the paired verdict cannot be given" % ("Windows" if ctx.win else "its session did not start"))
    if connect_or_fail(ctx, r, arm) is None:
        plain.close()
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    off = lab.offset or 0.0
    keys = [ctx.rng.choice(L.RAW_KEY_ALPHABET) for _ in range(n)]
    pkeys = [ctx.rng.choice(L.RAW_KEY_ALPHABET) for _ in range(n)]
    plain_sends, tos = [], None
    try:
        ctx.be.type(L.raw_key_logger_line(script, app_out, n), type_pace(ctx), enter=True)
        if plain_up:
            plain.run_line(L.raw_key_logger_line(script, plain_out, n), rng=ctx.rng)
        ready = ctx.until(lambda: ctx.remote.exists(app_out + ".ready") and (not plain_up or ctx.remote.exists(plain_out + ".ready")), 20 + 6 * rt, step=0.05)
        r.check(bool(ready), "the key logger did not start on the server (%s%s)" % (app_out + ".ready", " / " + plain_out + ".ready" if plain_up else ""))
        if ready:
            time.sleep(0.3 + rt)
            log_off, t0 = ctx.log_size(), time.time()
            for k, pk in zip(keys, pkeys):
                ctx.be.type(k, type_pace(ctx))
                time.sleep(type_pace(ctx) / 1000.0)
                if plain_up:
                    plain_sends.append(plain.send_key(pk))
                    plain._pump(type_pace(ctx) / 1000.0)
            ctx.until(lambda: len(ctx.remote.lines(app_out)) >= n and (not plain_up or len(ctx.remote.lines(plain_out)) >= n), 10 + 4 * rt, step=0.05)
            tos = L.ssh_conn_tos(lab.ports[22])          # both sessions are still connected: the logger ended, the shells did not
            time.sleep(0.2)
            app_arr = L.raw_key_arrivals(ctx.remote.lines(app_out))
            CA.exact_lines(r, "the server's logger read exactly the keys typed into the app, in order", keys, [c for _, c in app_arr])
            sends = key_write_times(ctx.log_since(log_off), t0)
            app_ms = L.raw_key_samples(keys, sends, app_arr, off)
            plain_ms = []
            if plain_up:
                plain_arr = L.raw_key_arrivals(ctx.remote.lines(plain_out))
                CA.exact_lines(r, "the server's logger read exactly the keys written to plain ssh, in order", pkeys, [c for _, c in plain_arr])
                plain_ms = L.raw_key_samples(pkeys, plain_sends, plain_arr, off)
            r.metric("key_arrival_app_ms", app_ms)
            r.metric("key_arrival_plain_ms", plain_ms)
            if tos is None:
                r.note("the DSCP class of the connections was not read on this machine (no `ss`): only the delivery times are judged")
            else:
                app_tos = sorted({t for pid, (prog, t) in tos.items() if pid != plain.pid and prog == "ssh"})
                plain_tos = (tos.get(plain.pid) or (None, None))[1] if plain_up else None
                in_cs1, words = tos_note(app_tos, plain_tos)
                r.metric("tos_app", app_tos)
                r.metric("tos_plain", plain_tos)
                r.note(words)
                r.check(not in_cs1, "the app's ssh connection is in the lower-effort DSCP class CS1 (TOS 0x20), the class behind #985: %s" % words)
            r.metric("app_bytes_written", len(sends))
            if len(sends) != n:
                r.unverifiable("the app log has %d byte(s) in `pty_write` lines for the %d keys (0: input-order logging is off): which write carried which key "
                               "is not known, so no delivery time is given" % (len(sends), n))
            elif plain_up:
                pv = L.paired_verdict(app_ms, plain_ms)
                r.metric("paired_verdict", pv)
                if pv["verdict"] == "no-data":
                    r.unverifiable("paired key delivery has too few samples: %s" % pv)
                else:
                    r.check(pv["verdict"] == "pass", "key -> raw-input program on the server: app median %.1f ms vs plain ssh %.1f ms; allowed %.1f ms (over by %.1f ms)" % (
                        pv["app"], pv["plain"], pv["allowed"], pv["over_ms"]))
        if not ctx.until(lambda: ctx.remote.exists(app_out + ".done"), 3 + 2 * rt, step=0.05):
            ctx.be.key("ctrl+c")           # a logger still waiting for a key that never came: end it, so `exit` meets a prompt
            time.sleep(0.5 + 2 * rt)
    finally:
        plain.close()
    ctx.remote.rm(app_out, app_out + ".ready", app_out + ".done", plain_out, plain_out + ".ready", plain_out + ".done")
    finish_remote(ctx, r)


# ------------------------------------------------------------------ 5. dropped / reset / stalled connections
@lab_case("ssh_drop_reconnect", modes=("terminal",), shells=("bash", "zsh", "nu"), lat=("lan", "150"), helper=("nohelper", "helper"), faults=("rst", "stall10"))
def c_lab_drop(ctx, r, arm):
    """The ssh_drop_reconnect body (the escape, escape-at-connect with and without Enter, kill -9 of the remote shell) against the lab, plus two network faults
    the same-machine sshd cannot make: the server RESETS the connection (rst: the pane must return to a local prompt, a reconnect must work) and the server STALLS
    for 10 s (SIGSTOP of its session: the pane stays on the remote and the lines typed meanwhile arrive exactly, in order)."""
    if ctx.win:
        r.skip("not yet ported to the Windows panes (see ssh_drop_reconnect)")
    host = host_of(arm)
    if arm["fault"] is None:
        CA.ssh_drop_body(ctx, r, host, ("escape", "escape_connect_enter", "escape_connect_bare", "kill"))
    elif arm["fault"] == "rst":
        CA.ssh_drop_body(ctx, r, host, ("rst",))
    else:
        rt = rtt_s(arm)
        f = "lab_stall.txt"
        CR.rm_remote(ctx, f)
        if connect_or_fail(ctx, r, arm) is None:
            ctx.reset()
            return
        time.sleep(1.0 + 2 * rt)
        toks = [unique(ctx, 8) for _ in range(3)]
        ctx.lab.drop(arm["user"], "stall", 10)
        time.sleep(0.5)
        CR.typed_lines(ctx, ["echo %s >> ~/%s" % (t, f) for t in toks])
        ctx.until(lambda: len(ctx.remote.lines(f)) >= 3, 30 + 6 * rt)
        CA.exact_lines(r, "lines typed during a 10 s server stall", toks, ctx.remote.lines(f))
        ctx.shot("lab_stall_after")
        r.check(ctx.where_now() == "remote", "the pane left the remote shell during a 10 s stall (a stall is not a drop)")
        finish_remote(ctx, r)


# ------------------------------------------------------------------ 6. Ctrl-C bursts (#1798 / #1672)
@lab_case("ctrl_c_no_esc_i", shells=("bash", "zsh", "fish", "nu"), lat=("lan", "150"), helper=("nohelper", "helper"))
def c_ctrl_c_no_esc_i(ctx, r, arm):
    """#1798/#1672: `sleep 30`, then Ctrl-C x1/x2/x3 at gaps 0/50/200 ms, then a typed echo. Per round: the effect file is exact, the shell received exactly two
    commands (the sleep and the echo), the prompt log grew by one per command (bash exact; zsh at least), no ESC-i reached the shell (rec.log has no \\x1bi),
    and the app log never says "Session abandoned before bootstrap". One OCR at the end looks for a visible ^[i.
    Both lines are typed in the arm's own shell syntax (#2045): nushell's `sleep` takes a duration (lab.long_sleep_line) and `echo x >> f` prints a table there
    instead of appending (ctx.append_cmd). The checks are the same for every shell.
    #2044: on a login shell the app gives a plain session (fish, nu) the first Ctrl-C used to END the ssh (SIGINT to the local client); the rounds after it
    then ran in the local shell, which is why every effect file was empty."""
    lab, rt = ctx.lab, rtt_s(arm)
    shell_kind = arm["shell"] if arm["shell"] in ("nu", "pwsh") else "bash"
    sleep_line = L.long_sleep_line(arm["shell"])
    if connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    f = "cc_%s.txt" % unique(ctx, 4)
    rem, toks = ctx.remote, []
    for nctl in (1, 2, 3):
        for gap in (0, 50, 200):
            label = "x%d gap %d ms" % (nctl, gap)
            off = ctx.log_size()
            c0, p0 = len(rem.commands()), rem.prompts()
            ctx.be.type(sleep_line, type_pace(ctx), enter=True)
            time.sleep(0.8 + 2 * rt)
            for i in range(nctl):
                ctx.be.key("ctrl+c")
                if i < nctl - 1 and gap:
                    time.sleep(gap / 1000.0)
            if arm["shell"] != "fish":
                back = ctx.until(lambda: rem.prompts() > p0, 10 + 3 * rt, step=0.05)
                r.check(bool(back), "[%s] no prompt after the Ctrl-C burst" % label)
            else:
                time.sleep(1.0 + 2 * rt)
            tok = unique(ctx, 8)
            toks.append(tok)
            ctx.be.type(ctx.append_cmd(shell_kind, tok, work(arm, f)), type_pace(ctx), enter=True)
            ctx.until(lambda: len(rem.lines(f)) >= len(toks), 10 + 4 * rt)
            CA.exact_lines(r, "[%s] effect file" % label, toks, rem.lines(f))
            time.sleep(0.3 + rt)
            new = rem.commands()[c0:]
            r.check(len(new) == 2 and new[0][1].strip() == sleep_line and tok in new[-1][1], "[%s] the shell received %r, expected exactly [%s, the echo]" % (label, [c for _, c in new], sleep_line))
            r.check(not any("\x1bi" in c or "^[i" in c for _, c in new), "[%s] an ESC-i (^[i) reached the shell: %r" % (label, [c for _, c in new]))
            if arm["shell"] == "bash":
                # one prompt per command, plus at most one for each further Ctrl-C the app WROTE while the prompt had not come back (lab.ctrl_c_prompt_bounds)
                lo, hi = L.ctrl_c_prompt_bounds(ctx.log_since(off), nctl)
                grew = rem.prompts() - p0
                r.check(lo <= grew <= hi, "[%s] prompt.log grew by %d, expected one per command (2)%s" % (
                    label, grew, "" if hi == lo else "; at most %d here: the app wrote %d further Ctrl-C(s) before the prompt was back, and a bash at its prompt answers each with a fresh prompt" % (hi, hi - lo)))
            elif arm["shell"] == "zsh":
                r.check(rem.prompts() - p0 >= 2, "[%s] prompt.log grew by %d, expected at least one per command (2)" % (label, rem.prompts() - p0))
            r.check(SESSION_ABANDONED not in ctx.log_since(off), "[%s] the app log says %r" % (label, SESSION_ABANDONED))
    txt = ctx.ocr("lab_ctrlc_end")
    if txt is None:
        r.note("no OCR: a visible ^[i on screen was not looked for")
    else:
        r.check("^[i" not in txt and "^[ i" not in txt, "a literal ^[i is on screen after the Ctrl-C rounds")
    ctx.shot("lab_ctrlc_end")
    finish_remote(ctx, r)


# ------------------------------------------------------------------ 7. The helper install itself (Wave H, #1804)
@lab_case("helper_install", shells=("bash",), lat=("lan", "150"), helper=("upload", "upgrade", "never"), scenarios=("p_noscp",), modes=("terminal",))
def c_helper_install(ctx, r, arm):
    """What the app does about the bundled helper on a host where the lab blocks every outbound connection:
    upload  -- nothing on the server, always_install: exactly one phosphor-remote-<version> appears (uploaded over the ssh connection, so no download is involved),
               it runs, and the pane is a working remote shell. On p_noscp (scp and sftp refused) this proves the in-band upload needs neither.
    upgrade -- older phosphor-remote-* and the previous remote server's phosphor-oss* are on the server: after the connect exactly the new version is left.
    never   -- the host's policy is never: the session works and NOTHING named phosphor-* was created or touched on the server.
    The marker file names come from `<APPCI_LAB_HELPER_BIN> --version`, the same tree the app was built from."""
    lab, rt = ctx.lab, rtt_s(arm)
    want = arm["helper_file"]
    off = ctx.log_size()
    if connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    mode = arm["helper"]
    if mode in ("upload", "upgrade"):
        ok = ctx.until(lambda: server_helper_files(lab, arm) == [want], 120 + 20 * rt, step=1.0)
        files = server_helper_files(lab, arm)
        r.check(bool(ok), "%s: expected exactly [%s] on the server, found %r" % (mode, want, files))
        shown = lab.ssh(arm["user"], "~/.phosphor/remote-server/%s --version" % want)
        r.check(shown.returncode == 0 and shown.stdout.startswith("phosphor-remote "), "the uploaded helper does not run on the server: rc=%s %r" % (shown.returncode, shown.stderr[-200:]))
        link = lab.ssh(arm["user"], "readlink ~/.phosphor/remote-server/current")
        r.check(link.stdout.strip() == want, "the `current` link points at %r, expected %r" % (link.stdout.strip(), want))
        if mode == "upgrade":
            r.check(files == [want], "older versions were not removed: %r" % (files,))
    else:
        time.sleep(3 + 4 * rt)
        files = server_helper_files(lab, arm)
        r.check(files == [], "policy never, but the server has %r" % (files,))
    log = ctx.log_since(off)
    r.check("remote helper install skipped" in log or mode != "never", "the app log never says the helper install was skipped for the denied host (label %s)" % host_label(arm))
    # the session is a working remote shell on every arm: the no-helper features do not depend on the helper
    tok = unique(ctx, 8)
    ctx.be.type("echo %s > %s" % (tok, work(arm, "hi_%s.txt" % tok)), type_pace(ctx), enter=True)
    ctx.until(lambda: ctx.remote.lines("hi_%s.txt" % tok) == [tok], 10 + 4 * rt)
    CA.exact_lines(r, "the command typed into the remote shell", [tok], ctx.remote.lines("hi_%s.txt" % tok))
    ctx.shot("lab_helper_install_%s" % mode)
    finish_remote(ctx, r)

# ------------------------------------------------------------------ 9. The "shell mode" chip (Wave 1.5, wording changed by Wave H parity P0)
@lab_case("no_helper_chip", shells=("bash", "zsh", "fish", "nu", "pwsh", "dash", "ash", "rbash"), lat=("lan",), helper=("nohelper",), modes=("terminal",))
def c_no_helper_chip(ctx, r, arm):
    """A Phosphorized ssh session WITHOUT the remote helper shows the small, informational chip "shell mode" (OCR). It never blocks: a command
    typed right after connecting still runs once, and the chip is not an error banner (no 'ERROR' text next to it)."""
    import cases_caseagent as CAG
    if connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rtt_s(arm))
    txt = ctx.ocr("lab_no_helper_chip")
    if txt is None:
        r.unverifiable("no OCR on this machine (tesseract): the chip on screen is not checked")
    else:
        flat = " ".join(txt.split())
        r.check(CAG.fuzzy_has(flat, "shell mode"), "no 'shell mode' chip on screen in a no-helper ssh session")
        r.check("ERROR" not in flat, "an ERROR text is on screen next to the no-helper chip: it must stay informational")
    tok = unique(ctx)
    ctx.be.type("echo chip%s" % tok, type_pace(ctx), enter=True)
    time.sleep(1.0 + 2 * rtt_s(arm))
    r.check(sum(1 for _, cmd in ctx.remote.commands() if "echo chip%s" % tok in cmd) == 1, "a command typed after connecting did not run exactly once with the chip up")
    finish_remote(ctx, r)

# ------------------------------------------------------------------ 6a. a login shell the remote command does not bootstrap: keys reach it (#2120)
# view.rs ssh_on_remote_plain_login (the hop) / terminal_model.rs TerminalModel::ssh (the INFO line): the SSH hook of an unbootstrapped login shell
SH_LOGIN_PLAIN_RE = re.compile(r"hop=ssh_wrapper remote_plain_login\b|SSH hook for a login shell the remote command does not bootstrap")
SH_LOGIN_GIVE_UP_RE = re.compile(r"hop=kh_ssh_give_up\b")


def sh_login_log_facts(log_text):
    """pure: what the app log of one connect says about #2120: dict(plain=the plain-login event of the SSH hook was logged, managed=the wrapper took the
    managed way (the premise: without it this is not the path of #2120), give_ups=number of ssh give-up nets that fired)"""
    text = log_text or ""
    return dict(plain=bool(SH_LOGIN_PLAIN_RE.search(text)), managed=bool(MANAGED_SESSION_RE.search(text)), give_ups=len(SH_LOGIN_GIVE_UP_RE.findall(text)))


@lab_case("nohelper_sh_login_keys", shells=("dash", "ash", "bash"), lat=("lan",), helper=("nohelper",))
def c_nohelper_sh_login_keys(ctx, r, arm):
    """#2120: `ssh <host>` to a host whose LOGIN shell is dash / busybox ash. The wrapper's remote command prints the SSH hook and execs the shell without a
    bootstrap, so no InitShell and no first prompt ever come; before the fix every key typed in that session was held for as long as ssh ran (Linux: no
    clock; macOS: 15 s, by a give-up). Oracle: one line typed right behind the ssh command's Enter (held while ssh connects) and five lines typed at
    10-25 ms/key with an instant Enter once the session is up all arrive in the host's effect file exactly once, in typed order (the host's file, read by
    labd: no prompt hook is needed, and dash / ash have none in this session); the app log of the connect has the plain-login event of the SSH hook and
    NO ssh give-up (the release is the hook, not a net). dash / ash: "the session is up" is that event in the app log, not `prompt.log`: when this case
    was written a dash started by the remote command was not a login shell and never read the `~/.profile` that carries the lab's hook (#2126;
    lab_nohelper_sh_login_profile judges that), and this case must not depend on it.
    The bash arm is the control (and the arm the candidate matrix, which has no dash, plays): a login shell the remote command DOES bootstrap. Same typed
    lines, same effect-file oracle, the session is up at its first remote prompt, and the plain-login event must NOT be in the log."""
    rt = rtt_s(arm)
    bootstrapped = arm["shell"] in ("bash", "zsh")                # bootstrap/bash_body.sh: the remote command's `case`; ssh_phase.rs ssh_remote_command_bootstraps
    eff = work(arm, "shk_%s.txt" % unique(ctx, 4))
    off = ctx.log_size()
    before = CR.rprompts(ctx)
    ahead = "ta" + unique(ctx, 5)
    ctx.be.type("ssh " + host_of(arm), 12, enter=True)
    ctx.be.type(ctx.append_cmd("bash", ahead, eff), type_pace(ctx), enter=True)          # typed while ssh connects: held, then the remote's
    if bootstrapped:
        seen = ctx.until(lambda: CR.rprompts(ctx) > before, 90, step=0.1)
    else:
        seen = ctx.until(lambda: sh_login_log_facts(ctx.log_since(off))["plain"], 90, step=0.1)
    facts = sh_login_log_facts(ctx.log_since(off))
    r.metric("sh_login_log_facts", facts)
    if bootstrapped:
        r.check(bool(seen), "control: no remote prompt 90 s after `ssh %s`. Test host: %s" % (host_of(arm), "" if seen else ctx.ssh_diag()))
    else:
        r.check(bool(seen), "no plain-login event in the app log 90 s after `ssh %s` (neither `hop=ssh_wrapper remote_plain_login` nor the model's SSH-hook "
                            "line): the SSH hook of a %s login shell did not end the connect hold (#2120). Managed way taken: %s. Test host: %s" % (
                                host_of(arm), arm["shell"], facts["managed"], "" if seen else ctx.ssh_diag()))
    if not seen:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    toks = []
    for _ in range(5):
        tok = "sk" + unique(ctx, 5)
        toks.append(tok)
        ctx.be.type(ctx.append_cmd("bash", tok, eff), type_pace(ctx), enter=True)
        ctx.until(lambda: any(tok in x for x in ctx.remote.lines(eff)), 10 + 4 * rt, step=0.05)
    ctx.until(lambda: len(ctx.remote.lines(eff)) >= len(toks) + 1, 10 + 4 * rt)
    got = [(x.split() or [""])[0] for x in ctx.remote.lines(eff)]
    CA.exact_lines(r, "effect file: the line typed while ssh connected, then every line typed at the remote prompt, once each, in typed order", [ahead] + toks, got)
    facts = sh_login_log_facts(ctx.log_since(off))
    r.metric("sh_login_log_facts_end", facts)
    r.check(facts["give_ups"] == 0, "%d ssh give-up net(s) fired in this session (hop=kh_ssh_give_up): the keys must be released by an event" % facts["give_ups"])
    if bootstrapped:
        r.check(not facts["plain"], "control: the app logged the plain-login event of an SSH hook for a %s login shell, which the remote command bootstraps" % arm["shell"])
    elif not facts["managed"]:
        r.note("the app log has no managed-session line for this connect: the wrapper did not take the managed way, so this run did not go through the path of #2120")
    finish_remote(ctx, r)


# ------------------------------------------------------------------ 6a2. that login shell is a LOGIN shell: ~/.profile is read (#2126)
def sh_login_profile_verdict(tok, hook, lines):
    """pure: None when the effect file holds exactly the one typed line and its $ENV is the lab hook that only ~/.profile sets; else the reason"""
    want = "%s %s" % (tok, hook)
    got = [" ".join(x.split()) for x in lines if x.strip()]
    if got == [want]:
        return None
    if got == ["%s noenv" % tok]:
        return "ENV is not set in the session's shell: ~/.profile was not read, the shell is not a login shell (#2126)"
    return "effect file: expected exactly %r, got %r" % ([want], got)


@lab_case("nohelper_sh_login_profile", shells=("dash", "ash", "bash"), lat=("lan",), helper=("nohelper",))
def c_sh_login_profile(ctx, r, arm):
    """#2126: `ssh <host>` to a host whose LOGIN shell is dash / busybox ash starts that shell as a login shell, as a plain `ssh <host>` does, so it reads
    `~/.profile`. The lab user's `~/.profile` is `ENV=<the lab hook>; export ENV; . <the lab hook>` (labd.rc_plan) and nothing else names that hook, so:
    (1) the connect is the REMOTE prompt in prompt.log, which the hook's PS1 writes (before the fix the remote command ran `exec "$SHELL"`: no ~/.profile,
    no hook, no line, and this step times out); (2) a line typed at that prompt appends `$ENV` to the host's effect file: exactly one line, and the value
    is the hook path of this user. The app log facts of #2120 (plain-login event, give-ups) are metrics here; lab_nohelper_sh_login_keys judges them.
    The bash arm is the control (and the arm the candidate matrix, which has no dash, plays): a login shell the remote command bootstraps, whose arm of
    the remote command #2126 did not touch. Same steps; the typed line carries the word `control` in place of `$ENV` and must arrive once."""
    rt = rtt_s(arm)
    control = arm["shell"] in ("bash", "zsh")
    tok = "pf" + unique(ctx, 5)
    eff = work(arm, "shp_%s.txt" % unique(ctx, 4))
    hook = "control" if control else "/etc/sshlab/hooks/hook.sh.%s" % arm["user"]      # labd.rc_plan, hook_shell_key: dash and ash share the `sh` hook
    off = ctx.log_size()
    secs = CR.connect(ctx, host_of(arm), 90)
    r.metric("sh_login_log_facts", sh_login_log_facts(ctx.log_since(off)))
    if control:
        r.check(secs is not None, "control: no remote prompt 90 s after `ssh %s`. Test host: %s" % (host_of(arm), ctx.ssh_diag() if secs is None else ""))
    else:
        r.check(secs is not None, "no remote prompt in prompt.log 90 s after `ssh %s`: the %s login shell did not read ~/.profile, which alone carries the "
                                  "lab's prompt hook, so it was not started as a login shell (#2126). Test host: %s" % (
                                      host_of(arm), arm["shell"], ctx.ssh_diag() if secs is None else ""))
    if secs is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    ctx.be.type('echo %s %s >> %s' % (tok, "control" if control else '"${ENV:-noenv}"', eff), type_pace(ctx), enter=True)
    ctx.until(lambda: any(tok in x for x in ctx.remote.lines(eff)), 10 + 4 * rt, step=0.05)
    time.sleep(0.5 + 2 * rt)                                       # a second copy of the line would land here
    why = sh_login_profile_verdict(tok, hook, ctx.remote.lines(eff))
    r.check(why is None, why or "")
    finish_remote(ctx, r)


# ------------------------------------------------------------------ 6b. nushell agent commands: pagers off, restored (#1813)
NU_PAGER_MARKER = "# __phosphor_ap_pagers"          # agent_shell_wrap.rs NU_PAGER_MARKER (selftest keeps them equal)


@lab_case("nu_agent_pager_env", shells=("nu",), lat=("lan",), helper=("nohelper",), modes=("terminal",))
def c_nu_agent_pager_env(ctx, r, arm):
    """#1813 on a REAL remote nushell (u_nu): the lines an agent command is typed as (` <command> # __phosphor_ap_pagers`, the exact wrapper form for Nu) are run
    in the app's ssh session. `cd` and `$env.X =` of the command persist; PAGER/LESS/... are set during it (for nushell and for an external child); after a
    success, after an external command that exits 3 and after a nushell error the user's own PAGER is back and the variables the user never had are gone; an
    unmarked line sees the user's own PAGER. The local twin with a throw-away HOME is script/test_nu_agent_pager_env."""
    tok = unique(ctx, 5)
    eff = work(arm, "pg_%s.txt" % tok)
    if connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rtt_s(arm))

    def sv(expr):
        return "((%s) + (char nl)) | save -a %s" % (expr, eff)

    def run(line, want_lines):
        ctx.be.type(line, type_pace(ctx), enter=True)
        ctx.until(lambda: len(ctx.remote.lines(eff)) >= want_lines, 10 + 4 * rtt_s(arm), step=0.1)

    after = sv("$\"after:($env.PAGER? | default 'unset'):($env.LESS? | default 'unset'):($env.PGTOK? | default 'none'):($env.PWD | path basename)\"")
    sub = "pgsub" + tok
    run(' $env.PAGER = "mypager"', 0)
    run(" mkdir %s; cd %s; $env.PGTOK = \"%s\"; %s; ^sh -c 'echo ext:$PAGER:$LESS >> %s' %s" % (
        sub, sub, tok, sv("$\"during:($env.PAGER):($env.LESS)\""), eff, NU_PAGER_MARKER), 2)
    run(" " + after, 3)
    run(" ^sh -c 'echo d3:$PAGER >> %s; exit 3' %s" % (eff, NU_PAGER_MARKER), 4)
    run(" " + after, 5)
    run(" %s; error make {msg: \"boom\"} %s" % (sv("$\"d4:($env.PAGER)\""), NU_PAGER_MARKER), 6)
    run(" " + after, 7)
    got = ctx.remote.lines(eff)
    keep = "after:mypager:unset:%s:%s" % (tok, sub)
    r.check(got[:2] == ["during:cat:-FRX", "ext:cat:-FRX"], "pagers not set during the command: %r" % got[:2])
    r.check(got[2:3] == [keep], "after success: cd/$env did not persist or the user's PAGER/LESS was not restored: %r" % got[2:3])
    r.check(got[3:5] == ["d3:cat", keep], "after an external command that exits non-zero: %r" % got[3:5])
    r.check(got[5:7] == ["d4:cat", keep], "after a nushell error: %r" % got[5:7])
    finish_remote(ctx, r)


# ------------------------------------------------------------------ 7. a real password / keyboard-interactive prompt (#1535)
@lab_case("password_prompt", shells=(), lat=("lan", "150"), helper=("nohelper",), scenarios=("p_password", "p_kbdint"))
def c_password_prompt(ctx, r, arm):
    """The owner's secret-prompt rule against a REAL sshd that asks for the unix password (the same-machine harness sshd cannot: no account password, no PAM):
    p_password (password auth) and p_kbdint (keyboard-interactive via PAM). The ssh_password_prompt body (CR.secret_prompt): nothing is typed until ssh's prompt
    is on screen; the secret typed after it logs in; the secret is in no shell's rec.log and not on screen; a wrong secret ends the ssh (NumberOfPasswordPrompts 1)
    and the next command runs in the LOCAL shell. The password is the throw-away one labd leased for this run. Not covered: p_mfa (a TOTP that must be computed at
    typing time, not at prompt time) and the passphrase prompt (ssh_password_prompt covers it on the same-machine sshd)."""
    secret = (getattr(ctx.lab, "leased", None) or {}).get("password")
    if not secret:
        r.unverifiable("the lab lease carries no password for %s" % arm["user"])
        return
    CR.rm_remote(ctx, "pw_labpw.txt")
    CR.secret_prompt(ctx, r, host_of(arm), secret, "password", "labpw")
    ctx.reset()
