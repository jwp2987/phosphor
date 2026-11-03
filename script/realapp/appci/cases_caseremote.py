# appci/cases_caseremote.py: remote work over ssh to the harness's local sshd (sshd.py), and per-shell variants (issue #1464).
# Registered with the same @case decorator as cases.py (one import line at the end of cases.py). All full tier (each is > 60 s).
# Every check compares the EXACT expected lines with what the shell received (effect files + the rc's rec.log), expected vs got in the message.
#
# Stand-in for ksu/sudo/su (no non-interactive sudo on CI): a nested `bash`, a nested `sh`, and `env -i ... bash --noprofile --norc -i`
# (a new interactive shell with a cleaned environment, the closest unprivileged equivalent of `ksu`/`su -`); the nesting is PROVEN by pid
# (the inner shell's $PPID is the outer shell's $$, and after `exit` $$ is the outer pid again) before any verdict is made.
import os, re, shutil, socket, subprocess, sys, threading, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cases import case, stats  # noqa: E402
import toml_text as T  # noqa: E402


# ------------------------------------------------------------------ helpers
def need_ssh(ctx, r):
    if not ctx.ssh:
        r.skip(ctx.ssh_why or "no local ssh server (needs /usr/sbin/sshd + ssh-keygen, i.e. apt package openssh-server; or python3-paramiko for fakesshd.py)")


def rm_remote(ctx, *names):
    for n in names:
        try:
            os.remove(os.path.join(ctx.prof.rhome, n))
        except FileNotFoundError:
            pass


def rprompts(ctx):
    return ctx.prof.prompts(remote=True)


def connect(ctx, host="fake", timeout=90):
    """`ssh <host>` + Enter, wait for a REMOTE shell prompt (the remote rc logs to its own prompt.log). Returns seconds, or None."""
    before = rprompts(ctx)
    t0 = time.time()
    ctx.be.type("ssh " + host, 12, enter=True)
    return (time.time() - t0) if ctx.until(lambda: rprompts(ctx) > before, timeout, step=0.1) else None


def leave(ctx, timeout=15):
    """`exit` in the remote shell, wait for the LOCAL shell's next prompt. r18e9 (macOS remote_session_*): when the remote steps had failed this `exit` ran in the pane's
    LOCAL shell and closed the pane (then "APP DIED"). The pane is proven REMOTE by a probe first; otherwise `exit` is NOT typed (ctx.left_unproven says why)."""
    where = ctx.where_now()
    if where != "remote":
        ctx.left_unproven = where
        return False
    ctx.left_unproven = None
    before = ctx.prompts()
    ctx.be.type("exit", 12, enter=True)
    return bool(ctx.until(lambda: ctx.prompts() > before, timeout, step=0.1))


def typed_lines(ctx, cmds, delay=(10, 25)):
    for c in cmds:
        ctx.be.type(c, ctx.rng.randint(*delay), enter=True)


def exact(r, label, exp, got):
    ok = got == exp
    r.check(ok, "%s: expected %r, got %r" % (label, exp, got) if len(repr(exp)) < 400 else
            "%s: %d expected lines, %d got, exact=%s, first diff %r" % (label, len(exp), len(got), ok,
                                                                        next(((a, b) for a, b in zip(exp + [None], got + [None]) if a != b), None)))
    return ok


# ------------------------------------------------------------------ with / without Phosphor's ssh extension
def set_ext_mode(ctx, mode):
    """rewrite warpify.ssh.ssh_extension_install_mode in the profile's settings.toml and relaunch the app (the setting is read at start).
    Returns False when the app cannot be relaunched here (sim backend)."""
    if ctx.app is None or not hasattr(ctx.app, "stop"):
        return False
    cfg = ctx.prof.home + "/.config/phosphor/settings.toml"
    t = open(cfg).read()
    open(cfg, "w").write(T.set_key(t, "warpify.ssh", "ssh_extension_install_mode", '"%s"' % mode))
    try:        # keep the dying instance's log next to the others (#1457 style), best effort
        dst = os.path.join(ctx.run, "logs", ctx.mode)
        os.makedirs(dst, exist_ok=True)
        if os.path.exists(ctx.log_path):
            shutil.copy(ctx.log_path, os.path.join(dst, "phosphor.before_ext_%s.log" % mode))
    except Exception:
        pass
    n = ctx.prompts()
    ctx.snapshot_log()                # r18e9: injector accounting spans the relaunch
    ctx.app.stop()
    time.sleep(1.0)
    ctx.app.start()
    up = ctx.until(lambda: ctx.prompts() > n, 90, step=0.5)
    time.sleep(4)                     # first-run overlay settles
    try:
        ctx.be.key("Escape")          # Escape in a fresh agent view leaves agent mode (cases.py mode_proof), so only a terminal-mode dismissal is safe
    except Exception:
        pass
    return bool(up)


def remote_session(ctx, r, variant):
    need_ssh(ctx, r)
    tag = "rs_" + variant
    rm_remote(ctx, tag + "_ft.txt", tag + "_sub.txt", tag + "_vim.txt", tag + "_less.txt", tag + "_cc.txt", tag + "_back.txt", tag + "_m.txt")
    ctx.rm(tag + "_back.txt", tag + "_ft.txt")
    log_off = ctx.log_size()
    # ---- 1. connect
    secs = connect(ctx, "fake", 120 if variant == "ext" else 60)
    r.check(secs is not None, "[%s] no remote shell prompt after `ssh fake` (see sshd.log, phosphor.log). Test host: %s" % (variant, ctx.ssh_diag() if secs is None else ""))
    ctx.shot("%s_connected" % tag)
    if secs is None:
        ctx.reset()
        return
    r.metric("connect_s", round(secs, 2))
    time.sleep(1.0)
    # ---- 2. typing + Enter exactness inside the remote shell (10-25 ms/key, instant Enter)
    rmark, lmark = len(ctx.prof.commands(remote=True)), ctx.cmd_mark()
    n = ctx.count(12)
    exp, cmds = [], []
    for _ in range(n):
        tok = ctx.tok(10).lower()
        exp.append(tok)
        cmds.append("echo %s >> ~/%s_ft.txt" % (tok, tag))
    typed_lines(ctx, cmds)
    ctx.until(lambda: len(ctx.lines(tag + "_ft.txt", remote=True)) >= n, 20 + n)
    time.sleep(0.5)
    exact(r, "[%s] remote typing, %d lines" % (variant, n), exp, ctx.lines(tag + "_ft.txt", remote=True))
    rem = [c for _, c in ctx.prof.commands(remote=True)[rmark:]]
    if rem:
        exact(r, "[%s] commands the REMOTE shell received" % variant, cmds, rem)
    r.check(not ctx.lines(tag + "_ft.txt"), "[%s] a remote-typed line ran LOCALLY: %r" % (variant, ctx.lines(tag + "_ft.txt")))
    r.check(ctx.cmds_since(lmark) == [], "[%s] the LOCAL shell received commands while the remote shell was in front: %r" % (variant, ctx.cmds_since(lmark)))
    # ---- 3. nested subshells (sudo/su/ksu stand-ins), proven by pid
    subs = (("bash", "bash", 1.5), ("bash-no-wait", "bash", 0.0), ("sh", "sh", 1.5),
            ("clean-env bash (ksu/su stand-in)", "env -i HOME=$HOME PATH=$PATH TERM=$TERM bash --noprofile --norc -i", 2.0))
    for i, (label, cmd, gap) in enumerate(subs):
        f = "%s_sub.txt" % tag
        rm_remote(ctx, f)
        tok = ctx.tok(8).lower()
        ctx.be.type("echo $$ >> ~/" + f, 12, enter=True)
        ctx.until(lambda: len(ctx.lines(f, remote=True)) >= 1, 6)
        ctx.be.type(cmd, 12, enter=True)
        time.sleep(gap)
        typed_lines(ctx, ["echo $PPID >> ~/" + f, "echo %s >> ~/%s" % (tok, f), "exit"])
        ctx.until(lambda: len(ctx.lines(f, remote=True)) >= 3, 10)
        time.sleep(1.5)
        ctx.be.type("echo $$ >> ~/" + f, 12, enter=True)
        ctx.until(lambda: len(ctx.lines(f, remote=True)) >= 4, 8)
        l = ctx.lines(f, remote=True)
        ok = len(l) == 4 and l[0] == l[1] == l[3] and l[2] == tok and l[0].isdigit()
        r.check(ok, "[%s] remote subshell %s (setup gap %.1fs): want [outer pid, same pid as inner $PPID, %s, outer pid again], got %r" % (variant, label, gap, tok, l))
        ctx.shot("%s_sub%d" % (tag, i))
        if not ok:
            break
    # ---- 4. vim over ssh
    f = tag + "_vim.txt"
    tok = ctx.tok(6).lower()
    if shutil.which("vim"):
        ctx.be.type("vim -u NONE -N ~/" + f, 12, enter=True)
        time.sleep(2.0)
        ctx.be.type("ihello remote " + tok, 15)
        ctx.be.key("Escape")
        time.sleep(0.3)
        ctx.shot("%s_vim" % tag)
        ctx.be.type(":wq", 15, enter=True)
        ctx.until(lambda: ctx.lines(f, remote=True), 8)
        time.sleep(0.8)
        exact(r, "[%s] vim over ssh wrote the typed text" % variant, ["hello remote " + tok], ctx.lines(f, remote=True))
    else:
        r.note("vim not installed on the box (apt: vim): vim step skipped")
    # ---- 5. less over ssh
    if shutil.which("less"):
        ctx.be.type("seq 1 400 | less", 12, enter=True)
        time.sleep(1.5)
        ctx.be.type(" ", 20)
        ctx.be.type("G", 20)
        ctx.shot("%s_less" % tag)
        ctx.be.type("q", 20)
        time.sleep(0.8)
    else:
        r.note("less not installed on the box (apt: less): less step skipped")
    tok = ctx.tok(8).lower()
    ctx.be.type("echo %s >> ~/%s_less.txt" % (tok, tag), 12, enter=True)
    ctx.until(lambda: ctx.lines(tag + "_less.txt", remote=True), 8)
    exact(r, "[%s] remote shell usable after vim/less (keys of less/vim did not leak into the shell)" % variant, [tok], ctx.lines(tag + "_less.txt", remote=True))
    # ---- 6. Ctrl-C of a remote long command
    f = tag + "_cc.txt"
    rm_remote(ctx, f)
    ctx.be.type("bash -c 'trap \"echo INT >> ~/%s; exit\" INT; sleep 60'; echo AFTER >> ~/%s" % (f, f), 10, enter=True)
    time.sleep(1.5)
    t0 = time.time()
    ctx.be.key("ctrl+c")
    ctx.until(lambda: len(ctx.lines(f, remote=True)) >= 2, 8, step=0.05)
    r.metric("remote_ctrl_c_wall_s", round(time.time() - t0, 2))
    exact(r, "[%s] Ctrl-C reached the REMOTE command and the remote shell continued" % variant, ["INT", "AFTER"], ctx.lines(f, remote=True))
    # ---- 7. back to local: the next local command runs locally, never remotely
    ok = leave(ctx)
    r.check(ok, "[%s] no local prompt after `exit` from the remote shell" % variant if ctx.left_unproven is None else
            "[%s] the pane is not provably REMOTE (probe ran in: %s), so `exit` was NOT typed (it would close the local pane)" % (variant, ctx.left_unproven))
    if ctx.left_unproven is not None:
        ctx.reset()
    tok = ctx.tok(8).lower()
    lmark = ctx.cmd_mark()
    ctx.be.type("echo %s >> %s_back.txt" % (tok, tag), 12, enter=True)
    ctx.until(lambda: ctx.lines(tag + "_back.txt"), 8)
    exact(r, "[%s] local command after exit ran locally" % variant, [tok], ctx.lines(tag + "_back.txt"))
    r.check(not ctx.lines(tag + "_back.txt", remote=True), "[%s] local command after exit ran on the REMOTE: %r" % (variant, ctx.lines(tag + "_back.txt", remote=True)))
    if ctx.prof.commands():
        r.check(("echo %s >> %s_back.txt" % (tok, tag)) in ctx.cmds_since(lmark), "[%s] local rec.log lacks the exact command typed after exit" % variant)
    # ---- 8. and vice versa: connect again, a remote command runs remotely only
    if connect(ctx, "fake", 90) is None:
        r.check(False, "[%s] second connect never reached a remote prompt. Test host: %s" % (variant, ctx.ssh_diag()))
    else:
        tok = ctx.tok(8).lower()
        ctx.be.type("echo %s >> ~/%s_m.txt" % (tok, tag), 12, enter=True)
        ctx.until(lambda: ctx.lines(tag + "_m.txt", remote=True), 8)
        exact(r, "[%s] second session: remote command ran remotely" % variant, [tok], ctx.lines(tag + "_m.txt", remote=True))
        r.check(not ctx.lines(tag + "_m.txt"), "[%s] second session: remote command ran LOCALLY" % variant)
        if not leave(ctx):
            r.check(False, "[%s] second session: not provably remote (probe ran in: %s), `exit` NOT typed" % (variant, ctx.left_unproven))
    ext_hits = [l for l in ctx.log_since(log_off).splitlines() if re.search(r"remote.?server|ssh.?extension|warpif", l, re.I)]
    r.metric("app_log_ssh_ext_lines", len(ext_hits))
    for l in ext_hits[:6]:
        r.note("[%s] app log: %s" % (variant, l[:200]))
    ctx.reset()


@case("remote_session_legacy", order=60)
def c_remote_legacy(ctx, r):
    """ssh WITHOUT Phosphor's remote extension (ssh_extension_install_mode = never_install, the harness default; the legacy warpify path):
    connect, exact typing, nested subshells, vim, less, remote Ctrl-C, exit and local-vs-remote routing both ways."""
    remote_session(ctx, r, "legacy")


@case("remote_session_ext", order=61)
def c_remote_ext(ctx, r):
    """ssh WITH the remote extension path (ssh_extension_install_mode = always_install; the app is relaunched with it and relaunched back after).
    The CI sshd host has no route to the extension download, so the app may fall back to the legacy path after trying: what is asserted is
    that the typed input is exact either way and that the attempt costs a bounded connect time (metric connect_s)."""
    need_ssh(ctx, r)
    if not set_ext_mode(ctx, "always_install"):
        r.skip("the app cannot be relaunched with always_install on this backend (sim/no app handle)")
    try:
        remote_session(ctx, r, "ext")
    finally:
        if not set_ext_mode(ctx, "never_install"):
            r.check(False, "the app did not come back with never_install after the extension case: later cases would run on the wrong path")


# ------------------------------------------------------------------ large here-doc / long line over ssh, byte-exact (#1129)
def _payload_lines(rr, total_bytes, width=(20, 120)):
    """lines of [a-z0-9 ] (no quote, no `$`, no backslash: nothing a shell could expand inside a here-doc or a single-quoted string) totalling ~total_bytes"""
    out, tot = [], 0
    while tot < total_bytes:
        l = "".join(rr.choice("abcdefghijklmnopqrstuvwxyz0123456789 ") for _ in range(rr.randint(*width)))
        out.append(l)
        tot += len(l) + 1
    return out


def _remote_bytes(ctx, name):
    p = os.path.join(ctx.prof.rhome, name)
    try:
        return open(p, "rb").read()
    except OSError:
        return None


def _wait_remote_file(ctx, name, want_len, timeout):
    """until the remote file has want_len bytes (or stops growing for 3 s after reaching any size): returns the bytes or None"""
    last, still = -1, 0.0
    t_end = time.time() + timeout
    while time.time() < t_end:
        b = _remote_bytes(ctx, name)
        n = -1 if b is None else len(b)
        if n == want_len:
            return b
        if n == last and n >= 0:
            still += 0.25
            if still >= 3.0 and n > 0:
                return b
        else:
            still = 0.0
        last = n
        time.sleep(0.25)
    return _remote_bytes(ctx, name)


def _send_and_check(ctx, r, label, name, command, expected, timeout):
    rm_remote(ctx, name)
    t0 = time.time()
    ctx.be.clip_set(command)
    ctx.be.paste(enter=True)
    got = _wait_remote_file(ctx, name, len(expected), timeout)
    secs = round(time.time() - t0, 2)
    r.metric("%s_s" % name.replace(".", "_"), secs)
    r.metric("%s_bytes_got" % name.replace(".", "_"), -1 if got is None else len(got))
    ok = got == expected
    first = None
    if got is not None and not ok:
        first = next((i for i, (a, b) in enumerate(zip(expected, got)) if a != b), min(len(expected), len(got)))
    r.check(ok, "%s: remote file not byte-exact (expected %d bytes, got %s%s, %.1fs)" % (
        label, len(expected), "no file" if got is None else "%d bytes" % len(got), "" if first is None else ", first difference at byte %d" % first, secs))
    ctx.shot("heredoc_%s" % name.split(".")[0])
    return ok


@case("remote_heredoc_paste", order=62)
def c_remote_heredoc(ctx, r):
    """#1129: a large here-doc and a long single line sent to the REMOTE shell over ssh (the way an agent writes a file) arrive byte-exact. Pasted into the
    input and submitted with an instant Enter, so they take the same command-write path as an agent's command: a 2,000-line here-doc (~140 KB: above the
    128 KB no-bracketed-paste wrapper size), a 10 KB single line, and a ~600 KB here-doc (above the 512 KB size from which a bracketed multi-line command
    goes through the base64 wrapper, #1255). Then, when installed, the same 2,000-line payload in zsh and fish started inside the remote session (fish has
    no here-docs: a multi-line single-quoted printf). Every file is compared byte for byte with what was sent; the time each took is a metric."""
    need_ssh(ctx, r)
    import random as _random
    rr = _random.Random(1129)
    secs = connect(ctx, "fake", 60)
    r.check(secs is not None, "no remote shell prompt after `ssh fake`. Test host: %s" % ("" if secs is not None else ctx.ssh_diag()))
    if secs is None:
        ctx.reset()
        return
    time.sleep(1.0)
    eof = "PHOSPHOR_EOF_1129"
    lines2k = _payload_lines(rr, 200_000, (40, 100))[:2000]
    heredoc = lambda name, ls: "cat > ~/%s <<'%s'\n%s\n%s" % (name, eof, "\n".join(ls), eof)
    expect = lambda ls: ("\n".join(ls) + "\n").encode()
    _send_and_check(ctx, r, "bash: %d-line here-doc (%d bytes)" % (len(lines2k), len(expect(lines2k))), "hd_2k.txt", heredoc("hd_2k.txt", lines2k), expect(lines2k), 90)
    long_line = "".join(rr.choice("abcdefghijklmnopqrstuvwxyz0123456789") for _ in range(10_000))
    _send_and_check(ctx, r, "bash: 10 KB single line", "hd_line.txt", "printf '%%s\\n' '%s' > ~/hd_line.txt" % long_line, (long_line + "\n").encode(), 60)
    big = _payload_lines(rr, 600_000)
    _send_and_check(ctx, r, "bash: %d-line here-doc (%d bytes, wrapper path)" % (len(big), len(expect(big))), "hd_big.txt", heredoc("hd_big.txt", big), expect(big), 180)
    # zsh / fish inside the remote session (same machine: the harness's sshd runs the remote shell locally, so find_bin applies)
    for key in ("zsh", "fish"):
        path, adddir = find_bin(key)
        if not path:
            r.note("%s not installed: the remote %s step is skipped" % (key, key))
            continue
        probe = "hd_%s_probe.txt" % key
        rm_remote(ctx, probe)
        # zsh -f: the remote home has no .zshrc, and an interactive zsh would start zsh-newuser-install and swallow the probe lines
        ctx.be.type(path + (" -f" if key == "zsh" else ""), 12, enter=True)
        time.sleep(SHELLS[key][2])
        ready = False
        for _ in range(4):
            ctx.be.type("echo probe >> ~/%s" % probe, 12, enter=True)
            if ctx.until(lambda: _remote_bytes(ctx, probe), 3):
                ready = True
                break
        r.check(ready, "remote %s never executed a probe line" % key)
        if not ready:
            # leave the inner shell (if it started at all) so leave() below talks to the remote bash
            ctx.be.type("exit", 12, enter=True)
            time.sleep(1.5)
            break
        name = "hd_%s.txt" % key
        if key == "fish":
            cmd = "printf '%%s\\n' '%s' > ~/%s" % ("\n".join(lines2k), name)
        else:
            cmd = heredoc(name, lines2k)
        _send_and_check(ctx, r, "remote %s: %d-line payload" % (key, len(lines2k)), name, cmd, expect(lines2k), 120)
        ctx.be.type("exit", 12, enter=True)
        time.sleep(1.5)
    ok = leave(ctx)
    r.check(ok, "no local prompt after `exit` from the remote shell" if ctx.left_unproven is None else
            "the pane is not provably REMOTE (probe ran in: %s), so `exit` was NOT typed" % ctx.left_unproven)
    ctx.reset()


# ------------------------------------------------------------------ ssh Enter latency against a plain-ssh control (#1532)
def remote_stamp_line(ctx):
    """The timestamp line for the REMOTE shell, writing ~/lat_<i> into the remote home. Phosphor and the reference ssh into the SAME remote
    shell, so the line is symmetric whichever shell that is: macOS perl (BSD date has no %N; works in bash 3.2 and zsh), else `date +%s.%N`."""
    if ctx.be.name == "mac":
        return "perl -MTime::HiRes=time -e 'printf \"%%.6f\\n\", time' > ~/lat_%d"
    return "date +%%s.%%N > ~/lat_%d"


@case("latency_enter_ssh", order=64, gui=True)
def c_lat_enter_ssh(ctx, r):
    """#1532: Enter -> command start INSIDE an ssh session, Phosphor vs plain ssh in the reference terminal (same session, same remote shell,
    same keys/cadence/stamp line), verdict on the overhead against the owner's ssh budget (plain ssh +/- 1 ms, APPCI_SSH_ENTER_BUDGET_MS).
    A/B/A/B interleaved in blocks (both windows hold an ssh session); NOREF when that is impossible. The stamp files land in the remote home, so the measurement includes the ssh round trip on
    BOTH sides and cancels out of the overhead."""
    need_ssh(ctx, r)
    import lat
    from cases import enter_to_start, ref_terminal, ref_note, plat, lat_checks
    n, warm = ctx.count_lat(60), lat.warmup()
    stamp, rhome = remote_stamp_line(ctx), ctx.prof.rhome
    rm_remote(ctx, "lat_ready")
    base = ctx.shell_count()
    if connect(ctx) is None:
        r.check(False, "ssh fake: no remote prompt within 90 s (sshd.log). Test host: %s" % ctx.ssh_diag())
        ctx.reset()
        return
    tok = ctx.tok(6)
    ctx.be.type("echo %s >> ~/lat_ready" % tok, 12, enter=True)
    if not ctx.until(lambda: tok in ctx.lines("lat_ready", remote=True), 10):
        r.check(False, "after ssh fake, a probe line did not reach the REMOTE home: not measuring Enter latency in an unknown shell")
        ctx.reset()
        return
    # r18e0 follow-up: A/B/A/B blocks (cases.run_interleaved, as latency_enter): BOTH windows hold an open ssh session to the same sshd, then Phosphor and the
    # plain-ssh reference alternate in blocks. A reference measured after Phosphor confounded run order / drift with the overhead (7.56 +/- 2.49 ms in r18e0).
    # Where the reference cannot be kept open next to Phosphor the row is NOREF (Phosphor's numbers only, the reason recorded), never a sequential verdict.
    from cases import run_interleaved, close_ref, record_interleave
    ref, mode, fallback, blocks, ds, rs = ref_terminal(ctx), "sequential", None, None, [], []
    if ref and ref.switch and lat.interleave_enabled():
        ref_note(ctx, r)
        try:
            ref.switch("reference")
            time.sleep(2.5)
            if connect(ctx) is None:
                fallback = "reference: plain `ssh fake` gave no remote prompt within 90 s"
            else:
                try:
                    ds, rs, blocks = run_interleaved(ctx, ref, n, warm, lambda side, cnt: enter_to_start(ctx, cnt, budget_s=max(60, cnt * 2.5), stamp=stamp, d=rhome))
                    mode = "interleaved"
                except RuntimeError as e:
                    ds, rs, blocks = [], [], None
                    fallback = "interleaving failed: %s" % str(e)[:160]
                ref.switch("reference")
                ctx.be.type("exit", 12, enter=True)       # leave the reference's ssh (its window is closed below)
                time.sleep(1.0)
        except RuntimeError as e:
            fallback = fallback or "reference focus failed: %s" % str(e)[:160]
        finally:
            close_ref(ref)
        time.sleep(1.0)      # focus returns to Phosphor
        try:
            ctx.be.refocus()
        except RuntimeError:
            pass
    else:
        fallback = ("no reference terminal available%s" % (": " + ctx.ref_reason if getattr(ctx, "ref_reason", None) else "")) if not ref else \
            "this backend cannot keep both windows, or interleaving is disabled (APPCI_LAT_INTERLEAVE=0)"
        if ref:
            ref.close()
            time.sleep(1.0)
    if mode != "interleaved":
        ds = enter_to_start(ctx, n + warm, stamp=stamp, d=rhome)[warm:]       # NOREF: Phosphor alone
        rs = []
    left = ctx.leave_remote(r, "latency_enter_ssh", "lat_ready", tok, base)
    record_interleave(r, mode, blocks, fallback)
    r.metric("ssh_enter_to_start_ms", stats(ds))
    if rs:
        r.metric("reference_ssh_enter_to_start_ms", stats(rs))
        r.metric("reference", ref.name)
    r.check(len(ds) >= n * 0.8, "only %d/%d Enter timings captured over ssh" % (len(ds), n))
    if not left:
        return
    if ds:
        rec = lat.evaluate("%s/latency_enter_ssh/enter_to_start" % ctx.label, ds, rs, float(os.environ.get("APPCI_SSH_ENTER_BUDGET_MS", "1")), "overhead",
                           plat(), ref_name=ref.name if ref and rs else None, floor=ctx.be.key_floor_ms, expected=n, mode=mode,
                           notes=["both sides type into the SAME remote shell over ssh to the harness's local sshd; " +
                                  ("reference interleaved with Phosphor in blocks of %d (A/B/A/B, same session)" % lat.interleave_block() if mode == "interleaved"
                                   else "NOREF: the reference was not interleaved (%s), so no overhead is judged" % fallback)])
        r.metrics.setdefault("lat", {})["ssh_enter_to_start"] = rec
        lat_checks(r, rec, "Enter to command start over ssh")
    ctx.reset()


# ------------------------------------------------------------------ failing / hanging host, password prompt
@case("ssh_hanging_host_not_local", order=62, gui=True)
def c_ssh_hang(ctx, r):
    """`ssh` to (a) a port that refuses and (b) a TCP listener that accepts and never speaks (a hanging host): the line typed right behind the
    ssh Enter must never run in the LOCAL shell (the Enter-was-held-back notice), and the local shell must stay usable afterwards."""
    srv = socket.socket()
    srv.bind(("127.0.0.1", 0))
    srv.listen(5)
    hang_port = srv.getsockname()[1]
    held = []
    stop = threading.Event()

    def acceptor():
        srv.settimeout(0.5)
        while not stop.is_set():
            try:
                held.append(srv.accept()[0])
            except OSError:
                pass
    th = threading.Thread(target=acceptor, daemon=True)
    th.start()
    s2 = socket.socket()
    s2.bind(("127.0.0.1", 0))
    dead_port = s2.getsockname()[1]
    s2.close()                       # nothing listens: connection refused
    try:
        for label, port, wait in (("refused", dead_port, 6), ("hanging", hang_port, 14)):
            f = "sh_%s.txt" % label
            ctx.rm(f)
            tok = ctx.tok(6)
            lmark = ctx.cmd_mark()
            # r18e0 follow-up: when did the ssh client exist? A line typed AFTER a refused ssh already exited correctly runs locally; only a line typed while the
            # ssh was alive (and not yet failed) must be held. The poller records the ssh's presence by its -p <port> so the verdict rests on timing, not on a guess.
            seen = []         # (wall time, alive)
            pstop = threading.Event()

            def poll_ssh(port=port):
                while not pstop.is_set():
                    seen.append((time.time(), ps_has_port(port)))
                    time.sleep(0.05)
            pth = threading.Thread(target=poll_ssh, daemon=True)
            pth.start()
            t_ssh = time.time()
            ctx.be.type("ssh -o ConnectTimeout=4 -o BatchMode=yes -o StrictHostKeyChecking=no -p %d 127.0.0.1" % port, 12, enter=True)
            time.sleep(0.15)
            t_first = time.time()
            ctx.be.type("echo MUSTNOT_%s >> %s" % (tok, f), 15, enter=True)
            t_last = time.time()
            time.sleep(wait / 2)
            ctx.shot("hang_%s_mid" % label)
            time.sleep(wait / 2)
            pstop.set()
            pth.join(2)
            alive_ts = [t for t, a in seen if a]
            t_exit = next((t for t, a in seen if not a and alive_ts and t > alive_ts[0]), None)
            gate = ctx.app_hop_lines(re.compile(r"hop=kh_hold_(?:entry|exit)\b|hop=kh_iq_(?:hold|release)\b.*Q7_ssh_typeahead|ssh hold:"), t_first - 0.5)
            r.note("[%s host] timing: ssh started t0; ssh client first seen %s, exited %s; first MUSTNOT key +%.2f s, last MUSTNOT key +%.2f s; app ssh gate lines after the first key: %d" % (
                label, ("+%.2f s" % (alive_ts[0] - t_ssh)) if alive_ts else "never (exited before the first 50 ms poll or never started)",
                ("+%.2f s" % (t_exit - t_ssh)) if t_exit else "not observed", t_first - t_ssh, t_last - t_ssh, len(gate)))
            exited_before_key = (t_exit is not None and t_exit < t_first) or not alive_ts
            if exited_before_key:
                unverifiable = True
                # the ssh was already gone when the first key was typed: running locally is correct, so the 'held' property was not exercised
                r.no_reference("UNVERIFIABLE [%s host]: the ssh client had %s before the first MUSTNOT key, so a local run of the line is correct and says nothing about the hold" % (
                    label, "already exited" if alive_ts else "not been seen"))
            else:
                unverifiable = False
                r.check(not ctx.lines(f), "[%s host] the line typed behind ssh ran LOCALLY: %r (ssh alive at the first key; exit %s)" % (
                    label, ctx.lines(f), ("+%.2f s" % (t_exit - t_ssh)) if t_exit else "not observed"))
                r.check(not any("MUSTNOT_" + tok in c for c in ctx.cmds_since(lmark)),
                        "[%s host] the LOCAL shell received the line typed behind ssh: %r" % (label, ctx.cmds_since(lmark)))
                if not gate:
                    r.no_reference("UNVERIFIABLE [%s host]: no kh_hold_*/ssh hold line in the app log after the first MUSTNOT key (key trace off or rotated: %s); the line not running locally is not proven to be the app's hold" % (
                        label, "input_order lines present" if ctx.app_hop_lines(re.compile(r"input_order "), t_first - 0.5) else "NO input_order lines at all"))
            ctx.reset()
            tok2 = ctx.tok(6).lower()
            ctx.be.type("echo %s >> %s" % (tok2, f), 12, enter=True)
            ctx.until(lambda: ctx.lines(f), 8)
            if unverifiable:
                # r18e4: the ssh had already exited, so the MUSTNOT line correctly ran locally (a refused port exits in milliseconds, before the first key):
                # it may be in the file; only the usability probe is judged, and it must be there exactly once
                exact(r, "[%s host] the local shell is usable afterwards (ssh had already exited: the earlier line running locally is correct and ignored)" % label,
                      [tok2], [l for l in ctx.lines(f) if l != "MUSTNOT_" + tok])
            else:
                exact(r, "[%s host] the local shell is usable afterwards (and has none of the held line)" % label, [tok2], ctx.lines(f))
    finally:
        stop.set()
        th.join(2)
        for c in held:
            c.close()
        srv.close()
    ctx.reset()


def ps_has_port(port):
    """is an ssh client for 127.0.0.1 -p <port> running (matched on the args)"""
    try:
        out = subprocess.run(["ps", "-eo", "args"], capture_output=True, text=True, timeout=5).stdout
    except (OSError, subprocess.TimeoutExpired):
        return False
    return any(re.match(r"(\S*/)?ssh\s.*-p\s*%d\b" % port, l) for l in out.splitlines())


def ps_has(host):
    """is an ssh client for <host> running (Phosphor's ssh wrapper may add options, so match the args, not the typed text)"""
    out = subprocess.run(["ps", "-eo", "args"], capture_output=True, text=True).stdout
    return any(re.match(r"(\S*/)?ssh\s.*\b%s\b" % re.escape(host), l) for l in out.splitlines())


@case("ssh_password_prompt", order=63)
def c_ssh_password(ctx, r):
    """The owner's rule for a password-style prompt: keys typed BEFORE the prompt are discarded (never sent to the prompt, never run in a shell),
    keys typed AFTER it reach the prompt. The prompt is ssh's own `Enter passphrase for key` on the tty (host `fakepw`: a passphrase-protected
    key the local sshd accepts, NumberOfPasswordPrompts 1, so one wrong try ends the ssh). Evidence: the correct passphrase typed after the prompt
    logs in (junk typed before did not consume the single attempt), the junk and the passphrase are in no shell's rec.log, a wrong passphrase
    returns to the LOCAL shell where the next command runs locally."""
    need_ssh(ctx, r)
    if not ctx.ssh.get("pwident"):
        r.skip("the passphrase-key prompt needs the system OpenSSH sshd (apt: openssh-server); fakesshd.py cannot prompt")
    pw = ctx.ssh["passphrase"]
    f = "pw.txt"
    rm_remote(ctx, f)
    ctx.rm(f)
    lm, rm_ = ctx.cmd_mark(), len(ctx.prof.commands(remote=True))
    junk = "JUNKpre" + ctx.tok(5)
    before = rprompts(ctx)
    ctx.be.type("ssh fakepw", 12, enter=True)
    time.sleep(0.1)
    ctx.be.type(junk, 15, enter=True)                  # impatient typing before the prompt exists
    ctx.until(lambda: ps_has("fakepw"), 5, step=0.05)
    time.sleep(2.0)                                    # the passphrase prompt is up
    ctx.shot("pw_prompt")
    txt = ctx.ocr("pw_prompt_ocr")
    if txt is not None:
        r.check("passphrase" in txt.lower(), "OCR does not show the `Enter passphrase` prompt before the post-prompt keys are typed: ssh may have failed before prompting")
    ctx.be.type(pw, 15, enter=True)                    # after the prompt: must reach it
    ok = ctx.until(lambda: rprompts(ctx) > before, 30, step=0.1)
    r.check(bool(ok), "keys typed AFTER the prompt did not reach it: no remote shell after typing the passphrase (junk typed before may have consumed the single attempt)")
    ctx.shot("pw_after")
    if ok:
        tok = ctx.tok(8).lower()
        time.sleep(1.0)
        ctx.be.type("echo %s >> ~/%s" % (tok, f), 12, enter=True)
        ctx.until(lambda: ctx.lines(f, remote=True), 8)
        exact(r, "remote command after the password login", [tok], ctx.lines(f, remote=True))
        leave(ctx)
    allcmds = [c for _, c in ctx.prof.commands()[lm:]] + [c for _, c in ctx.prof.commands(remote=True)[rm_:]]
    r.check(not any(junk in c for c in allcmds), "the key burst typed BEFORE the prompt reached a shell instead of being discarded: %r" % [c for c in allcmds if junk in c])
    r.check(not any(pw in c for c in allcmds), "the passphrase ended up in a shell's command history (it was typed into a shell, not the prompt)")
    if txt is not None:
        after = ctx.ocr("pw_after_ocr") or ""
        r.check(pw not in after, "the passphrase is visible on screen (echoed)")
    # wrong passphrase: ssh ends, the LOCAL shell is back and the next line runs locally
    ctx.reset()
    lm = ctx.cmd_mark()
    ctx.be.type("ssh fakepw", 12, enter=True)
    ctx.until(lambda: ps_has("fakepw"), 5, step=0.05)
    time.sleep(2.0)
    ctx.be.type("wrong-" + ctx.tok(4), 15, enter=True)
    ctx.until(lambda: not ps_has("fakepw"), 15, step=0.2)
    time.sleep(1.5)
    tok = ctx.tok(8).lower()
    ctx.be.type("echo %s >> %s" % (tok, f), 12, enter=True)
    ctx.until(lambda: ctx.lines(f), 8)
    exact(r, "after a wrong passphrase the next command ran in the LOCAL shell", [tok], ctx.lines(f))
    r.check(not any("wrong-" in c for _, c in ctx.prof.commands()[lm:]), "the wrong passphrase was run as a shell command")
    ctx.reset()


# ------------------------------------------------------------------ per-shell variants (shell started from the harness's bash tab)
# shell: (binary, apt/other package for the skip message, start-up wait s, append(tok, file), sleep command, exit)
SHELLS = {
    "zsh": ("zsh", "zsh (apt install zsh)", 2.5, lambda t, f: "echo %s >> %s" % (t, f), "sleep 30", "exit"),
    "fish": ("fish", "fish (apt install fish)", 3.0, lambda t, f: "echo %s >> %s" % (t, f), "sleep 30", "exit"),
    "nu": ("nu", "nushell (not in Ubuntu repos: github.com/nushell/nushell release tarball, or `cargo install nu --locked`; Mac: brew install nushell)", 4.0,
           lambda t, f: '"%s\\n" | save --append %s' % (t, f), "sleep 30sec", "exit"),
    "pwsh": ("pwsh", "PowerShell 7 (Microsoft apt repo: packages-microsoft-prod.deb, then apt install powershell)", 7.0,
             lambda t, f: "Add-Content %s %s" % (f, t), "Start-Sleep 30", "exit"),
}


EXTRA_DIRS = ("~/.local/bin", "/home/gitlab-runner/.local/bin", "/opt/homebrew/bin", "/usr/local/bin", "~/.cargo/bin", "/snap/bin")


def find_bin(name):
    """(path, dir-to-add-to-PATH or None): PATH first, then the places the CI users keep shells (gitlab-runner ~/.local/bin for nu on the box,
    /opt/homebrew/bin on the Mac, whose gitrunner PATH lacks it)"""
    p = shutil.which(name)
    if p:
        return p, None
    for d in EXTRA_DIRS:
        d = os.path.expanduser(d)
        c = os.path.join(d, name)
        if os.path.isfile(c) and os.access(c, os.X_OK):
            return c, d
    return None, None


def make_shell_case(key):
    binary, pkg, wait, app, slp, ex = SHELLS[key]

    @case("shell_" + key, order=70)
    def run(ctx, r):
        """the core typing / subshell path with <shell> as a subshell of the pane's bash: it starts (readiness proven by a probe line, not a
        fixed sleep), 12 lines typed at 10-25 ms with instant Enter arrive exactly, a pasted line + instant Enter arrives, Ctrl-C ends a long
        command and the shell takes the next line, a nested `<shell> -c` / nested shell works, and `exit` returns to the pane's bash where the
        next command runs. fish adds: autosuggestions never steal keys, fish_postexec saw the exact lines; nu adds: its own hooks/bootstrap
        (plain `nu` with the scratch config, so Phosphor's nushell bootstrap path runs) and `nu -c`."""
        path, adddir = find_bin(binary)
        if not path:
            r.skip("package %s not installed (looked on PATH and in %s)" % (pkg, ", ".join(EXTRA_DIRS)))
        p, f, g, h = "sh_%s_probe.txt" % key, "sh_%s_ft.txt" % key, "sh_%s_pe.txt" % key, "sh_%s_cc.txt" % key
        ctx.rm(p, f, g, h, "sh_%s_back.txt" % key, "sh_%s_x.txt" % key, "sh_%s_log.txt" % key)
        if adddir:
            ctx.be.type('export PATH="%s:$PATH"' % adddir, 12, enter=True)
            time.sleep(0.5)
            r.note("%s found at %s (not on the pane's PATH): its directory was added to PATH in the pane" % (key, path))
        cmd = "pwsh -NoLogo -NoProfile" if key == "pwsh" else binary
        shells0 = ctx.shell_count()       # r18e9: `exit` below is typed only when the subshell is provably in front (macOS shell_zsh closed the pane)
        ctx.be.type(cmd, 12, enter=True)
        time.sleep(wait)
        ready = False
        for _ in range(4):
            ctx.be.type(app("probe", p), 12, enter=True)
            if ctx.until(lambda: ctx.lines(p), 3):
                ready = True
                break
        ctx.shot("shell_%s_started" % key)
        r.check(ready, "%s never executed a probe line after start (wait %.1fs + 4 probes)" % (key, wait))
        if not ready:
            ctx.reset()
            return
        n = ctx.count(12)
        exp = []
        for _ in range(n):
            tok = ctx.tok(10).lower()
            exp.append(tok)
            ctx.be.type(app(tok, f), ctx.rng.randint(10, 25), enter=True)
        ctx.until(lambda: len(ctx.lines(f)) >= n, 20 + n)
        time.sleep(0.5)
        exact(r, "%s: %d typed lines" % (key, n), exp, ctx.lines(f))
        # paste + instant Enter
        pexp = []
        for _ in range(ctx.count(4)):
            tok = ctx.tok(10).lower()
            pexp.append(tok)
            ctx.be.clip_set(app(tok, g))
            ctx.be.paste(enter=True)
            time.sleep(0.9)
        ctx.until(lambda: len(ctx.lines(g)) >= len(pexp), 12)
        exact(r, "%s: pasted lines + instant Enter" % key, pexp, ctx.lines(g))
        x = "sh_%s_x.txt" % key
        if key == "fish":
            # autosuggestions must not steal keys: run a long command, then type a command sharing its prefix but ending differently
            t1, t2 = ctx.tok(8).lower(), ctx.tok(8).lower()
            ctx.be.type("echo AUTOSUGG_alpha_%s >> %s" % (t1, x), 12, enter=True)
            ctx.until(lambda: ctx.lines(x), 5)
            ctx.be.type("echo AUTOSUGG_beta_%s >> %s" % (t2, x), 25, enter=True)
            ctx.until(lambda: len(ctx.lines(x)) >= 2, 5)
            exact(r, "fish: autosuggestion (a history line sharing the typed prefix) did not change the executed line", ["AUTOSUGG_alpha_" + t1, "AUTOSUGG_beta_" + t2], ctx.lines(x))
            t3 = ctx.tok(8).lower()
            ctx.be.type("fish -c 'echo %s >> %s'" % (t3, x), 12, enter=True)
            ctx.until(lambda: len(ctx.lines(x)) >= 3, 6)
            exact(r, "fish: nested `fish -c`", ["AUTOSUGG_alpha_" + t1, "AUTOSUGG_beta_" + t2, t3], ctx.lines(x))
            lg = os.path.join(ctx.prof.ctl, "fish_rec.log")
            rec = open(lg, errors="replace").read() if os.path.exists(lg) else ""
            if rec:
                r.check(("echo AUTOSUGG_beta_%s >> %s" % (t2, x)) in [l.split("\t", 2)[-1] for l in rec.splitlines()],
                        "fish_postexec log (fish_rec.log) lacks the exact typed line `echo AUTOSUGG_beta_%s ...`" % t2)
            else:
                r.note("fish_rec.log empty: the fish_postexec hook from config.fish did not run (effect files were still checked)")
        if key == "nu":
            t1 = ctx.tok(8).lower()
            ctx.be.type("nu -c '\"%s\\n\" | save --append %s'" % (t1, x), 12, enter=True)
            ctx.until(lambda: ctx.lines(x), 8)
            exact(r, "nu: nested `nu -c`", [t1], ctx.lines(x))
            ctx.be.type("nu", 12, enter=True)             # nested nushell
            time.sleep(wait)
            t2 = ctx.tok(8).lower()
            ctx.be.type(app(t2, x), 12, enter=True)
            ctx.until(lambda: len(ctx.lines(x)) >= 2, 8)
            ctx.be.type("exit", 12, enter=True)
            time.sleep(1.5)
            t3 = ctx.tok(8).lower()
            ctx.be.type(app(t3, x), 12, enter=True)
            ctx.until(lambda: len(ctx.lines(x)) >= 3, 8)
            exact(r, "nu: nested nu, exit to the outer nu, next line", [t1, t2, t3], ctx.lines(x))
            hits = [l for l in ctx.log_since(0).splitlines() if re.search(r"nu(shell)?\b.*(bootstrap|subshell|warpif)|(bootstrap|subshell|warpif).*nu(shell)?\b", l, re.I)]
            r.metric("app_log_nu_bootstrap_lines", len(hits))
            for l in hits[:4]:
                r.note("nu app log: " + l[:200])
        # Ctrl-C
        ctx.be.type(slp, 12, enter=True)
        time.sleep(1.5)
        ctx.be.key("ctrl+c")
        time.sleep(0.8)
        tok = ctx.tok(8).lower()
        ctx.be.type(app(tok, h), 12, enter=True)
        got = ctx.until(lambda: ctx.lines(h), 6)
        exact(r, "%s: Ctrl-C ended the long command and the shell ran the next line (within ~7 s, not after the 30 s sleep)" % key, [tok], got or [])
        # exit back to the pane's bash
        if key != "pwsh" and ctx.in_subshell(shells0) is False:      # pwsh is not in Ctx.SHELLS: its count never rises
            r.check(False, "%s: no extra shell under the app any more (count %s, was %s before it started), so `%s` was NOT typed (it would close the pane's own shell)" % (
                key, ctx.shell_count(), shells0, ex))
            ctx.reset()
            return
        ctx.be.type(ex, 12, enter=True)
        time.sleep(2.0)
        tok = ctx.tok(8).lower()
        b = "sh_%s_back.txt" % key
        ctx.be.type("echo %s >> %s" % (tok, b), 12, enter=True)
        ctx.until(lambda: ctx.lines(b), 8)
        exact(r, "%s: after exit the pane's bash ran the next command" % key, [tok], ctx.lines(b))
        ctx.reset()
    return run


for _k in SHELLS:
    make_shell_case(_k)
