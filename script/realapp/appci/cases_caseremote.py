# appci/cases_caseremote.py: remote work over ssh to the harness's local sshd (sshd.py), and per-shell variants (issue #1464).
# Registered with the same @case decorator as cases.py (one import line at the end of cases.py). All full tier (each is > 60 s).
# Every check compares the EXACT expected lines with what the shell received (effect files + the rc's rec.log), expected vs got in the message.
#
# Stand-in for ksu/sudo/su (no non-interactive sudo on CI): a nested `bash`, a nested `sh`, and `env -i ... bash --noprofile --norc -i`
# (a new interactive shell with a cleaned environment, the closest unprivileged equivalent of `ksu`/`su -`); the nesting is PROVEN by pid
# (the inner shell's $PPID is the outer shell's $$, and after `exit` $$ is the outer pid again) before any verdict is made.
import os, re, shutil, socket, subprocess, sys, threading, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402
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
            ("clean-env bash (ksu/su stand-in)", 'env -i HOME="$HOME" PATH="$PATH" TERM="$TERM" bash --noprofile --norc -i', 2.0))      # r18u8: quoted, a PATH with spaces split the words
    for i, (label, cmd, gap) in enumerate(subs):
        if getattr(ctx, "win", False) and cmd.startswith("env -i"):
            # r18u8 Windows (H6): the remote side is the harness's fake sshd running msys bash; `env -i` there drops the vars the msys runtime needs and the fake
            # session ends, taking the rest of the case with it. Not a verdict on the app: recorded as a note, never as a pass of the step.
            r.no_reference("[%s] clean-env subshell step NOT RUN on Windows: `env -i` under msys ends the fake sshd session (harness limitation, H6); the bash/sh subshell steps ran" % variant)
            continue
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
        # r18u19 (Windows Git Bash): the rc hook logs a command at the NEXT prompt (PROMPT_COMMAND forks `history 1`, slow under msys), so the effect file
        # can be there before its rec.log line: wait for the line (as local_cmd_exact does) instead of reading rec.log once. Same exact-line verdict.
        back = "echo %s >> %s_back.txt" % (tok, tag)
        r.check(bool(ctx.until(lambda: back in ctx.cmds_since(lmark), 5)), "[%s] local rec.log lacks the exact command typed after exit" % variant)
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


def _wait_remote_file(ctx, name, want_len, timeout, on_tick=None):
    """until the remote file has want_len bytes (or stops growing for 3 s after reaching any size): returns the bytes or None. on_tick() runs about
    every 2 s while waiting (the #1129 CPU sampler)."""
    last, still = -1, 0.0
    t_end = time.time() + timeout
    next_tick = time.time()
    while time.time() < t_end:
        if on_tick is not None and time.time() >= next_tick:
            on_tick()
            next_tick = time.time() + 2.0
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


DIALOG_VIEW = "MultilineSubmitConfirmationDialog"            # the dialog's view name in the app's `frame N: updated=[...]` debug lines
DIALOG_ANSWERED = "MultilineSubmitConfirmationAction"          # `core_enter ... action_type=...MultilineSubmitConfirmationAction handled=true`
SUBMITTED = "hop=try_execute_done executed=true"


def dialog_state(text):
    """From an app-log excerpt that starts at the paste: 'open' (the `Run N lines one at a time?` dialog was drawn after the last answer to it),
    'submitted' (the editor submitted a command and no dialog is open), or None (neither yet). Pure, for the self-test."""
    shown = text.rfind(DIALOG_VIEW + '"')
    answered = text.rfind(DIALOG_ANSWERED)
    if shown >= 0 and shown > answered:
        return "open"
    if text.rfind(SUBMITTED) > max(shown, answered):
        return "submitted"
    return None


def dialog_wait_s(nbytes):
    """How long to look for the dialog after a paste of `nbytes`. r18u19 (Mac, terminal): the 600 KB here-doc's dialog was drawn just after a
    6 s look had ended (the Enter waits for the shell-or-chat verdict of the whole paste, #1604), stayed up for the case's 180 s and was then
    confirmed by the next step's `zsh -f` Enter. 6 s plus 1 s per 50 KB, at most 40 s; the look ends early once the app logs a submit."""
    return min(40.0, 6.0 + nbytes / 50_000.0)


def confirm_unbracketed_paste(ctx, r, label, off=None, wait_s=6.0):
    """r18u8 (Mac): the remote /bin/bash 3.2 has no bracketed paste, so the app asks `Run N lines one at a time?` (Cancel / Run) before it runs a multi-line paste, and
    every payload sat behind that dialog until the case timed out (`got no file`). The dialog is the app's designed answer for a shell that cannot take a bracketed
    paste, so the case answers it with Run (Return, the dialog's default) and says so. r18u19: read from the app log first (`off` = the log size before the
    paste: the dialog's view in a frame line, a submit ends the look), OCR only without a log. Neither: nothing is pressed (a blind Return could land in the
    shell) and the note says why a timeout may follow."""
    t_end = time.time() + wait_s
    use_log = off is not None and bool(getattr(ctx, "log_path", None)) and os.path.exists(ctx.log_path)
    while time.time() < t_end:
        if use_log:
            st = dialog_state(ctx.log_since(off))
            if st == "submitted":
                return False
            if st == "open":
                ctx.shot("heredoc_dialog")
                ctx.be.key("Return")
                r.note("%s: answered the app's `Run N lines one at a time?` dialog (the remote shell has no bracketed paste) with Run (dialog seen in the app log)" % label)
                time.sleep(0.5)
                return True
            time.sleep(0.25)
            continue
        txt = ctx.ocr("heredoc_dialog_probe")
        if txt is None:
            r.note("%s: no app log and no OCR engine, so a `Run N lines one at a time?` dialog cannot be answered; a timeout below may be that dialog" % label)
            return False
        low = " ".join(txt.lower().split())
        if "one at a time" in low:
            ctx.shot("heredoc_dialog")
            ctx.be.key("Return")
            r.note("%s: answered the app's `Run N lines one at a time?` dialog (the remote shell has no bracketed paste) with Run" % label)
            time.sleep(0.5)
            return True
        time.sleep(0.5)
    return False


def dismiss_open_dialog(ctx, r, label, off):
    """After a timeout: a dialog still open would take the next step's Enter as Run (r18u19: `zsh -f` + Enter confirmed the 600 KB here-doc three
    minutes late and every later step cascaded). Escape is the dialog's Cancel; the text stays in the input and ctx.reset() clears it."""
    if off is None or not getattr(ctx, "log_path", None) or dialog_state(ctx.log_since(off)) != "open":
        return
    ctx.shot("heredoc_dialog_left_open")
    ctx.be.key("Escape")
    time.sleep(0.5)
    r.note("%s: the `Run N lines one at a time?` dialog was still open after the timeout; cancelled it with Escape so the next step's Enter cannot run it" % label)
    ctx.reset()


PS_CPU_RE = re.compile(r"^\s*(\d+)\s+([\d.]+)\s+(.+?)\s*$")


def busy_sample(ctx):
    """{'app': %cpu, '<command basename>': %cpu, ...} for the app and the shells / ssh processes on this machine (the harness's sshd runs the remote
    shell locally), or {} when ps fails. #1129 rate diagnosis: a remote here-doc that arrives at ~2 KB/s is either the remote line editor (bash at
    ~100 %) or the app consuming the echo (the app at ~100 %); ps %cpu is a decaying average on macOS, so the PEAK over the wait is recorded."""
    try:
        out = subprocess.run(["ps", "-A", "-o", "pid=,pcpu=,comm="], capture_output=True, text=True, timeout=10).stdout
    except (OSError, subprocess.SubprocessError):
        return {}
    try:
        app_pid = ctx.app.pid()
    except Exception:
        app_pid = None
    got = {}
    for line in out.splitlines():
        m = PS_CPU_RE.match(line)
        if not m:
            continue
        pid, cpu, comm = int(m.group(1)), float(m.group(2)), os.path.basename(m.group(3)).lstrip("-")
        if pid == app_pid:
            key = "app"
        elif comm in ("bash", "sh", "zsh", "fish", "sshd", "ssh", "sshd-session"):
            key = comm
        else:
            continue
        got[key] = max(got.get(key, 0.0), cpu)
    return got


def sshd_log_path(ctx):
    return os.path.join(getattr(ctx, "run", "") or "", "ssh", "sshd.log")


def sshd_log_size(ctx):
    try:
        return os.path.getsize(sshd_log_path(ctx))
    except OSError:
        return 0


SSHD_IO_RE = re.compile(r"client input|conpty input|conpty output")


def sshd_io_lines(ctx, off):
    """r18u19 (H7): the Windows fake sshd's input/output accounting lines written since byte `off` of its log: whether the paste reached the
    test sshd at all and whether its pseudo console took it (fakesshd_win.py). Empty for the POSIX sshd, which writes no such lines."""
    try:
        with open(sshd_log_path(ctx), "rb") as f:
            f.seek(off)
            text = f.read().decode("utf-8", "replace")
    except OSError:
        return []
    return [l.strip() for l in text.splitlines() if SSHD_IO_RE.search(l)]


def sshd_input_bytes(lines):
    """the most client bytes the Windows fake sshd says it received, from its accounting lines (sshd_io_lines), or None when it logged none"""
    got = [int(m.group(1)) for l in lines for m in [re.search(r"client input (?:so far|ended): (\d+) bytes", l)] if m]
    return max(got) if got else None


def _send_step(ctx, r, label, name, command, expected, timeout, grace=0.0):
    """#1129: one paste + instant Enter into the pane's remote shell, then wait for ~/<name> in the remote home. NO verdict here (judge_step gives it,
    after the plain-ssh control has run). `timeout` is the verdict's budget; with `grace` the wait goes on up to `grace` s longer so the remote shell
    finishes the command and the session stays usable for the next step (the real duration is recorded either way). Returns the step's record."""
    rm_remote(ctx, name)
    t0 = time.time()
    sshd_off = sshd_log_size(ctx)
    off = ctx.log_size() if getattr(ctx, "log_path", None) else None
    ctx.be.clip_set(command)
    ctx.be.paste(enter=True)
    confirm_unbracketed_paste(ctx, r, label, off, dialog_wait_s(len(command)))
    peak = {}

    def sample():
        for k, v in busy_sample(ctx).items():
            peak[k] = max(peak.get(k, 0.0), v)
    got = _wait_remote_file(ctx, name, len(expected), timeout, on_tick=sample)
    secs = round(time.time() - t0, 2)
    late = None
    if got is None and grace > 0:
        got_late = _wait_remote_file(ctx, name, len(expected), grace, on_tick=sample)
        if got_late is not None:
            late = round(time.time() - t0, 2)
            got = got_late
    key = name.replace(".", "_")
    r.metric("%s_s" % key, secs)
    if late is not None:
        r.metric("%s_s_after_budget" % key, late)       # done only after the verdict's budget: the verdict is still the budget's
    r.metric("%s_bytes_got" % key, -1 if got is None else len(got))
    if peak:
        r.metric("%s_peak_cpu" % key, {k: round(v, 1) for k, v in peak.items()})
    ctx.shot("heredoc_%s" % name.split(".")[0])
    rec = dict(label=label, name=name, sent=len(command), want=len(expected), got=None if got is None else len(got), secs=secs, late=late, timeout=timeout,
               peak=peak, wrapped_len=len(wrapper_line(command)), ok=got == expected and late is None, exact=got == expected, first=None, sshd_off=sshd_off,
               timed_out=got is None or (late is not None))
    if got is not None and got != expected:
        rec["first"] = next((i for i, (a, b) in enumerate(zip(expected, got)) if a != b), min(len(expected), len(got)))
    if not rec["exact"]:
        dismiss_open_dialog(ctx, r, label, off)
    return rec


# ------------------------------------------------------------------ #1129 the app's one-line wrapper, mirrored (the self-test compares it with the Rust source)
WRAPPER_SCRIPT_HEAD = 'rm -f "$__pc"; unset __pc\n'
WRAPPED_MIN, WRAPPED_MIN_BRACKETED = 128 * 1024, 512 * 1024       # posix_long_command.rs MIN_WRAPPED_BYTES / MIN_WRAPPED_BYTES_BRACKETED


def wrapper_from_encoded(encoded):
    """app/src/terminal/writeable_pty/posix_long_command.rs wrapper_line's format string with `encoded` in place"""
    return ('__pc=$(mktemp "${TMPDIR:-/tmp}/.phosphor-cmd-XXXXXX") && { '
            "if printf %s '" + encoded + "' | { base64 -d 2>/dev/null || base64 -D; } > \"$__pc\"; "
            'then . "$__pc"; '
            'else rm -f "$__pc"; unset __pc; '
            "echo 'phosphor: could not decode a long command' >&2; false; fi; }")


def wrapper_line(command):
    """the ONE line the app types for a bash command it wraps (#1129 / #1255), byte for byte"""
    import base64
    return wrapper_from_encoded(base64.b64encode((WRAPPER_SCRIPT_HEAD + command + "\n").encode()).decode())


BASH_WRAPPER_STEPS = ("hd_2k.txt", "hd_big.txt")             # the remote-bash here-doc steps (zsh / fish / the 10 KB line are never wrapped)


def app_wraps_len(nbytes, multiline, bracketed_paste):
    """posix_long_command.rs needs_wrapper_for, by size: whether the app sends a bash command of `nbytes` through the wrapper"""
    if bracketed_paste:
        return nbytes >= WRAPPED_MIN_BRACKETED and multiline
    return nbytes >= WRAPPED_MIN


def app_wraps(command, bracketed_paste):
    """posix_long_command.rs needs_wrapper_for: whether the app sends `command` to a bash through the wrapper"""
    return app_wraps_len(len(command.encode()), "\n" in command or "\r" in command, bracketed_paste)


def remote_bash_bracketed(ctx):
    """whether the remote bash turns bracketed paste on by default: True for bash >= 5.1, False below 4.4, None when unknown (4.4 / 5.0 depend on the
    user's inputrc; Windows: the fake sshd's Git Bash, not asked here). The harness's sshd runs /bin/bash as the remote shell (sshd.py)."""
    if getattr(ctx, "win", False):
        return None
    try:
        v = subprocess.run(["/bin/bash", "-c", "echo ${BASH_VERSINFO[0]}.${BASH_VERSINFO[1]}"], capture_output=True, text=True, timeout=10).stdout.strip()
        major, minor = (int(x) for x in v.split(".")[:2])
    except (OSError, subprocess.SubprocessError, ValueError):
        return None
    if (major, minor) >= (5, 1):
        return True
    if (major, minor) < (4, 4):
        return False
    return None


# ------------------------------------------------------------------ #1129 the remote session's own shell: recorded, and ended by the harness after a timeout
RPID_FILE = ".appci_rpid"
RPID_LINE = "echo $$ $(cat /proc/$$/winpid 2>/dev/null) > ~/%s"        # msys: /proc/$$/winpid is the Windows pid taskkill needs; elsewhere empty


def read_rpid(ctx, fname=RPID_FILE):
    """(posix pid, windows pid or None) the remote shell wrote with RPID_LINE, or (None, None)"""
    b = _remote_bytes(ctx, fname)
    parts = (b or b"").decode(errors="replace").split()
    pid = int(parts[0]) if parts and parts[0].isdigit() else None
    wpid = int(parts[1]) if len(parts) > 1 and parts[1].isdigit() else None
    return pid, wpid


def record_rpid(ctx, fname=RPID_FILE, timeout=8):
    rm_remote(ctx, fname)
    ctx.be.type(RPID_LINE % fname, 12, enter=True)
    ctx.until(lambda: read_rpid(ctx, fname)[0] is not None, timeout)
    return read_rpid(ctx, fname)


def _pid_is_shell(pid):
    try:
        comm = subprocess.run(["ps", "-o", "comm=", "-p", str(pid)], capture_output=True, text=True, timeout=10).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return False
    return os.path.basename(comm).lstrip("-") in ("bash", "sh", "zsh", "fish")


def kill_remote_shell(ctx, pid, wpid):
    """end the remote session's shell from the harness (the remote shell runs on THIS machine under the harness's sshd). POSIX: SIGHUP, then SIGKILL
    after 3 s, only when the pid is still a shell; Windows: taskkill /T /F of the msys shell's Windows pid. Returns what was done (for the note)."""
    import signal
    if getattr(ctx, "win", False):
        if not wpid:
            return "no Windows pid recorded for the remote shell: not killed"
        p = subprocess.run(["taskkill", "/PID", str(wpid), "/T", "/F"], capture_output=True, text=True, timeout=30)
        return "taskkill /T /F of the remote shell (Windows pid %d): exit %d" % (wpid, p.returncode)
    if not pid:
        return "no pid recorded for the remote shell: not killed"
    if not _pid_is_shell(pid):
        return "pid %d is no longer a shell: not killed" % pid
    # its descendants first (a `zsh -f` / `fish` started inside it would keep the session open, and bash resends SIGHUP only to its jobs)
    kids = descendants(pid)
    for k in kids:
        try:
            os.kill(k, signal.SIGKILL)
        except OSError:
            pass
    try:
        os.kill(pid, signal.SIGHUP)
        for _ in range(30):
            time.sleep(0.1)
            os.kill(pid, 0)
        if _pid_is_shell(pid):
            os.kill(pid, signal.SIGKILL)
            return "SIGKILL to %d descendant(s), SIGHUP then SIGKILL to the remote shell (pid %d)" % (len(kids), pid)
    except ProcessLookupError:
        pass
    except PermissionError as e:
        return "could not signal the remote shell (pid %d): %s" % (pid, e)
    return "SIGKILL to %d descendant(s), SIGHUP to the remote shell (pid %d): it exited" % (len(kids), pid)


def descendants(pid):
    """every descendant pid of `pid` (POSIX ps), deepest last; [] when ps fails"""
    try:
        out = subprocess.run(["ps", "-A", "-o", "pid=,ppid="], capture_output=True, text=True, timeout=10).stdout
    except (OSError, subprocess.SubprocessError):
        return []
    kids = {}
    for line in out.splitlines():
        parts = line.split()
        if len(parts) == 2 and parts[0].isdigit() and parts[1].isdigit():
            kids.setdefault(int(parts[1]), []).append(int(parts[0]))
    seen, todo = [], [pid]
    while todo:
        for c in kids.get(todo.pop(), []):
            if c not in seen and c != pid:
                seen.append(c)
                todo.append(c)
    return seen


def end_stuck_session(ctx, r, label, mark=None, payload=None):
    """#1129 r18u22: after a step timed out the remote shell is still chewing the payload (a 600 KB wrapper line takes bash 3.2 over 15 min), so the
    case's next keys (`zsh -f`, the `exit`, run.py's where-probe) queued behind it and every later check failed, then pane_not_local_after_remote_heredoc_paste.
    The harness ends that remote shell (the remote is this machine) and proves the pane LOCAL again. Returns "local", "remote" or "none".
    r18u23 (Windows): the ssh client had read only part of the paste (284 KB of 600 KB); the rest stayed queued as console input of the LOCAL pane,
    so once ssh exited the local Git Bash ran it line by line as commands (541 payload lines over 150 s) and the where-probe found "none". `mark`
    (rec.log count before the step) and `payload` (the step's lines) detect that backlog; the app is then relaunched (cases.relaunch_after_paste_backlog)."""
    pid, wpid = read_rpid(ctx)
    what = kill_remote_shell(ctx, pid, wpid)
    time.sleep(2.0)
    if ctx.win and payload and cases.relaunch_after_paste_backlog(ctx, r, label, mark, payload):
        what += "; the local shell then ran the unread rest of the paste as commands, so the app was relaunched"
    where = ctx.where_now()
    if where == "none":
        ctx.reset()
        where = ctx.where_now()
    if where == "remote":           # a remote parent shell took over: leave it the proven way
        leave(ctx)
        where = ctx.where_now()
    r.note("%s: the remote shell was still busy with the payload after the timeout; the harness ended it (%s) so the pane is not left inside it; pane now: %s" % (label, what, where))
    return where


# ------------------------------------------------------------------ #1129 the CONTROL: the same bytes through plain ssh, no Phosphor
class _PtyPump(threading.Thread):
    """reads everything the plain ssh client prints (as fast as it comes: a faster consumer than any terminal) and writes the queued input when the
    pty takes it. Non-blocking both ways, so a full pty never deadlocks the reader; stop() ends it before the fd is closed."""

    def __init__(self, fd):
        super().__init__(daemon=True)
        self.fd, self.pending, self.lock, self.halt = fd, bytearray(), threading.Lock(), threading.Event()
        self.read_n = 0

    def put(self, data):
        with self.lock:
            self.pending += data

    def left(self):
        with self.lock:
            return len(self.pending)

    def stop(self):
        self.halt.set()
        self.join(3)

    def run(self):
        import select
        while not self.halt.is_set():
            with self.lock:
                want_w = bool(self.pending)
            try:
                rd, wr, _ = select.select([self.fd], [self.fd] if want_w else [], [], 0.2)
            except (OSError, ValueError):
                return
            if rd:
                try:
                    d = os.read(self.fd, 65536)
                except BlockingIOError:
                    d = None
                except OSError:
                    return
                if d == b"":
                    return
                if d:
                    self.read_n += len(d)
            if wr:
                with self.lock:
                    chunk = bytes(self.pending[:4096])
                try:
                    n = os.write(self.fd, chunk)
                except BlockingIOError:
                    n = 0
                except OSError:
                    return
                with self.lock:
                    del self.pending[:n]


def plain_ssh_send(ctx, label, name, text, expected, timeout, cols=160, rows=48):
    """#1129 CONTROL (POSIX): `ssh -tt fake` in a pty the harness drains as fast as the client prints, the same remote shell (the harness's sshd, same
    rc), `text` written as a paste (LF -> CR, as a terminal sends it) plus CR, then the same wait for ~/<name>. Nothing of Phosphor is involved, so if
    this is as slow as the app the remote shell is the bottleneck (ENV). Returns dict(s=seconds or None, got=bytes or -1, exact, peak, out, why)."""
    import fcntl, pty, signal, struct, termios
    res = dict(label=label, sent=len(text), s=None, got=-1, exact=False, peak={}, out=0, why=None)
    ssh = shutil.which("ssh")
    if getattr(ctx, "win", False) or not ssh:
        res["why"] = "no POSIX ssh client for the harness"
        return res
    rm_remote(ctx, name, ".appci_ctl_ready")
    env = dict(os.environ, HOME=ctx.prof.home, TERM="xterm-256color")
    argv = [ssh, "-F", os.path.join(ctx.prof.home, ".ssh", "config"), "-tt", "fake"]
    try:
        pid, fd = pty.fork()
    except OSError as e:
        res["why"] = "pty.fork failed: %s" % e
        return res
    if pid == 0:
        try:
            os.execve(ssh, argv, env)
        finally:
            os._exit(127)
    pump, rpid = None, None
    try:
        try:
            fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
            fcntl.fcntl(fd, fcntl.F_SETFL, fcntl.fcntl(fd, fcntl.F_GETFL) | os.O_NONBLOCK)
        except OSError:
            pass
        pump = _PtyPump(fd)
        pump.start()
        tok = ctx.tok(8).lower()
        for _ in range(3):
            pump.put(("echo %s $$ > ~/.appci_ctl_ready\r" % tok).encode())
            if ctx.until(lambda: tok in (_remote_bytes(ctx, ".appci_ctl_ready") or b"").decode(errors="replace"), 15):
                break
        ready = (_remote_bytes(ctx, ".appci_ctl_ready") or b"").decode(errors="replace").split()
        if len(ready) < 2 or ready[0] != tok:
            res["why"] = "the plain ssh session never ran a probe line"
            return res
        rpid = int(ready[1]) if ready[1].isdigit() else None
        time.sleep(0.5)
        peak = res["peak"]

        def sample():
            for k, v in busy_sample(ctx).items():
                peak[k] = max(peak.get(k, 0.0), v)
        t0 = time.time()
        pump.put((text.replace("\r\n", "\n").replace("\n", "\r") + "\r").encode())
        got = _wait_remote_file(ctx, name, len(expected), timeout, on_tick=sample)
        if got is not None:
            res["got"], res["exact"] = len(got), got == expected
            if got == expected:
                res["s"] = round(time.time() - t0, 2)
        if got is None:
            res["why"] = "no file within %d s (%d of %d bytes still unwritten to the pty)" % (timeout, pump.left(), len(text) + 1)
        res["out"] = pump.read_n
        return res
    finally:
        if rpid and _pid_is_shell(rpid):
            try:
                os.kill(rpid, signal.SIGKILL)      # a bash still in the line editor would burn a core for minutes after the client is gone
            except OSError:
                pass
        try:
            os.kill(pid, signal.SIGKILL)
        except OSError:
            pass
        if pump is not None:
            pump.stop()
        try:
            os.close(fd)
        except OSError:
            pass
        try:
            os.waitpid(pid, 0)
        except OSError:
            pass


def conhost_ssh_send(ctx, label, name, text, expected, timeout):
    """#1129 CONTROL (Windows Git Bash): the same `ssh fake` + paste + Enter in the reference console (conhost or Windows Terminal running Git Bash
    with the pane's rc, so the same msys ssh client and the same fake sshd), as big_paste's control does for a local paste. Same result dict as
    plain_ssh_send; why is set when it could not run."""
    from cases import ref_terminal
    res = dict(label=label, sent=len(text), s=None, got=-1, exact=False, peak={}, out=0, why=None)
    ref = None
    rm_remote(ctx, name, ".appci_ctl_ready")
    try:
        ref = ref_terminal(ctx)
        if ref is None:
            res["why"] = "reference console not available: %s" % (getattr(ctx, "ref_reason", None) or "unknown")
            return res
        ctx.be.type("ssh fake", 12, enter=True)
        tok = ctx.tok(8).lower()
        for _ in range(4):
            time.sleep(3)
            ctx.be.type((RPID_LINE % ".appci_ctl_ready").replace("echo ", "echo %s " % tok, 1), 12, enter=True)
            if ctx.until(lambda: tok in (_remote_bytes(ctx, ".appci_ctl_ready") or b"").decode(errors="replace"), 12):
                break
        if tok not in (_remote_bytes(ctx, ".appci_ctl_ready") or b"").decode(errors="replace"):
            res["why"] = "ssh in the reference console never ran a probe line in the remote shell"
            return res
        t0 = time.time()
        ctx.be.clip_set(text)
        ctx.be.paste(enter=True)
        got = _wait_remote_file(ctx, name, len(expected), timeout)
        if got is not None:
            res["got"], res["exact"] = len(got), got == expected
            if got == expected:
                res["s"] = round(time.time() - t0, 2)
        else:
            res["why"] = "no file within %d s" % timeout
        return res
    except Exception as e:
        res["why"] = "control failed: %s: %s" % (type(e).__name__, str(e)[:120])
        return res
    finally:
        parts = (_remote_bytes(ctx, ".appci_ctl_ready") or b"").decode(errors="replace").split()
        if len(parts) >= 3 and parts[2].isdigit():
            subprocess.run(["taskkill", "/PID", parts[2], "/T", "/F"], capture_output=True, timeout=30)
        if ref is not None:
            try:
                ref.close()
            except Exception:
                pass


_CONTROL_CACHE = {}         # run dir -> control results: the control does not depend on the app, so the second mode reuses the first's


def heredoc_control(ctx, r, eof, lines2k):
    """#1129: the plain-ssh / reference-console control for the 2,000-line here-doc, cached per run. POSIX runs two variants: W = the app's wrapper
    line for that here-doc (the same ONE long line the app types for a bash without bracketed paste) and H = the here-doc itself (what a plain
    terminal's paste sends: 2,000 short lines). Windows runs H in the reference console."""
    key = getattr(ctx, "run", "") or ""
    if key in _CONTROL_CACHE:
        r.note("plain-ssh control: reused from the first mode of this run (it does not involve the app)")
        return _CONTROL_CACHE[key]
    heredoc = lambda name: "cat > ~/%s <<'%s'\n%s\n%s" % (name, eof, "\n".join(lines2k), eof)      # noqa: E731
    expected = ("\n".join(lines2k) + "\n").encode()
    ctl = {}
    if getattr(ctx, "win", False):
        ctl["H"] = conhost_ssh_send(ctx, "reference console + ssh, the here-doc pasted", "ctl_h.txt", heredoc("ctl_h.txt"), expected, 120)
    else:
        ctl["H"] = plain_ssh_send(ctx, "plain ssh, the here-doc pasted", "ctl_h.txt", heredoc("ctl_h.txt"), expected, 120)
        w = wrapper_line(heredoc("ctl_w.txt"))
        ctl["W"] = plain_ssh_send(ctx, "plain ssh, the app's %d-byte wrapper line" % len(w), "ctl_w.txt", w, expected, 200)
        ctl["W"]["line"] = len(w)
    for k, v in ctl.items():
        r.metric("control_%s_s" % k, -1 if v["s"] is None else v["s"])
        r.metric("control_%s_bytes_got" % k, v["got"])
        if v["peak"]:
            r.metric("control_%s_peak_cpu" % k, {a: round(b, 1) for a, b in v["peak"].items()})
        r.note("control %s (%s): %s%s" % (k, v["label"], "%.1f s" % v["s"] if v["s"] is not None else "not done", "" if not v["why"] else " (%s)" % v["why"]))
    _CONTROL_CACHE[key] = ctl
    return ctl


def classify_timeout(rec, ctl, bracketed):
    """#1129: why a step that produced no file in its budget was slow, from the control. -> (class, text). class is
    'env'      the remote shell is the bottleneck: plain ssh needed as long (measured for the same line, or scaled from the 2k wrapper line);
    'app'      plain ssh with the same bytes was clearly faster: Phosphor is the slow side;
    'platform' Windows: the reference console's ssh did not get it through either (as big_paste's ConPTY/msys finding);
    None       no control evidence: the plain failure stands.
    The app's own wrapper is what turns the here-doc into one long line on a bash without bracketed paste: that is said whenever H shows it."""
    if ctl is None:
        return None, "no control ran"
    h, w = ctl.get("H"), ctl.get("W")
    big_enough = rec["name"] in BASH_WRAPPER_STEPS          # the 2k here-doc itself, or the bigger one
    if "W" not in ctl:      # Windows: only the reference-console paste of the 2k here-doc
        if h is None:
            return None, "no reference-console control ran"
        if h["exact"]:
            return "app", "the reference console's ssh delivered the same here-doc in %.1f s" % h["s"]
        if h["got"] < 0 and not (h["why"] or "").startswith("no file"):
            return None, "the reference-console control could not run (%s)" % h["why"]
        if not big_enough:
            return None, "the reference console's ssh failed the 2,000-line here-doc too, but this step's payload is a different kind/size, so that does not cover it"
        return "platform", ("the reference console's ssh did not deliver the 2,000-line here-doc either (%s): the msys ssh client under a Windows console does not take "
                            "a paste this size, as big_paste's control showed for msys `cat`; not shown to be the app" % (h["why"] or "%d of %d bytes" % (h["got"], rec["want"])))
    if rec["name"] not in BASH_WRAPPER_STEPS:
        return None, "not a bash wrapper-path step, so the wrapper-line control does not apply"
    if bracketed is None:
        wrapped = rec["sent"] >= WRAPPED_MIN_BRACKETED          # a multi-line command this size is wrapped with or without bracketed paste
    else:
        wrapped = app_wraps_len(rec["sent"], True, bracketed)
    if not wrapped:
        return None, "the step was not (provably) sent through the app's wrapper, so the wrapper-line control does not apply"
    if w is None or (w["s"] is None and "no file" not in (w["why"] or "")):
        return None, "the wrapper-line control could not run (%s)" % (w or {}).get("why")
    w_s = w["s"] if w["s"] is not None else 200.0          # a control that timed out is a lower bound
    scaled = w_s * rec["wrapped_len"] / float(w["line"])    # linear: a LOWER bound (bash 3.2 measured super-linear, r18u22: 10 KB 0.54 s, 190 KB 86 s)
    hs = "" if h is None else (" A plain paste of the here-doc itself (no wrapper: 2,000 short lines) took %.1f s in the same plain ssh, so on this remote bash the app's one-line wrapper is the slow path (design, #1129)" % h["s"]
                               if h["s"] is not None and h["s"] < w_s / 2 else " A plain paste of the here-doc itself: %s." % ("%.1f s" % h["s"] if h["s"] is not None else (h["why"] or "not done")))
    t_app = rec["late"] if rec["late"] is not None else rec["secs"]      # what the app took (late) or was given (no file at all: a lower bound)
    allowed = 1.5 * scaled + 10                                           # the same tolerance a finished step is held to
    if allowed >= t_app:
        return "env", ("plain ssh (no Phosphor) needed %s%.1f s for the app's %d-byte wrapper line of the 2,000-line here-doc%s; scaled linearly (a lower bound) to this "
                       "%d-byte line: >= %.0f s, so plain ssh could not have done it in the %.0f s the app %s (budget %d s): the remote shell's line editor is the "
                       "bottleneck, not the app.%s" % ("over " if w["s"] is None else "", w_s, w["line"],
                                                       "" if not w["peak"] else " (peak %%cpu %s)" % {a: round(b) for a, b in w["peak"].items()},
                                                       rec["wrapped_len"], scaled, t_app, "took" if rec["late"] is not None else "was given", rec["timeout"], hs))
    return "app", "plain ssh took %.1f s for the %d-byte wrapper line (scaled to this line: %.0f s; 1.5x + 10 s = %.0f s) where the app %s %.0f s: Phosphor was the slow side.%s" % (
        w_s, w["line"], scaled, allowed, "took" if rec["late"] is not None else "had no file after", t_app, hs)


def judge_step(ctx, r, rec, ctl, bracketed, io_lines=None):
    """#1129 verdict for one step. Wrong bytes always FAIL. A step that got no file in its budget is KNOWN (numbers + class) only when the control
    proves the remote shell is the bottleneck (class env) and ACCEPTED_FOR_RELEASE['remote_heredoc_remote_editor'] is in force, NOREF on Windows when
    the reference console's ssh fails the same way (platform), else FAIL. A step that finished is also compared with the control: much slower
    than plain ssh with the same wrapper line FAILS (the app starving the session)."""
    from cases import accepted_for_release
    label = rec["label"]
    io = ""
    if io_lines:
        n = sshd_input_bytes(io_lines)
        io = "; test sshd: %s client bytes received of the %d-byte paste (%s)" % ("no accounting line" if n is None else n, rec["sent"], " | ".join(io_lines[-4:]))
        if n is not None:
            r.metric("%s_sshd_got" % rec["name"].replace(".", "_"), n)
    peak = "" if not rec["peak"] else "; peak %%cpu while waiting: %s (the busy side is the bottleneck: the remote line editor or the app)" % {k: round(v, 1) for k, v in rec["peak"].items()}
    if rec["got"] is not None and not rec["exact"]:
        r.check(False, "%s: remote file not byte-exact (expected %d bytes, got %d bytes, first difference at byte %s, %.1fs)%s%s" % (
            label, rec["want"], rec["got"], rec["first"], rec["secs"], peak, io))
        return
    if rec["ok"]:
        w = (ctl or {}).get("W")
        if w and w["s"] is not None and rec["name"] == "hd_2k.txt" and bracketed is False and rec["secs"] > 1.5 * w["s"] + 10:
            r.check(False, "%s: took %.1f s where plain ssh took %.1f s for the same wrapper line (more than 1.5x + 10 s): Phosphor is the slow side%s" % (label, rec["secs"], w["s"], peak))
        return
    late = "" if rec["late"] is None else " (the file arrived after %.1f s, past the budget)" % rec["late"]
    base = "%s: remote file not byte-exact (expected %d bytes, got no file in the %d s budget, %.1fs)%s%s%s" % (label, rec["want"], rec["timeout"], rec["secs"], late, peak, io)
    cls, why = classify_timeout(rec, ctl, bracketed)
    if cls == "env":
        acc = accepted_for_release("remote_heredoc_remote_editor")
        if acc:
            r.known("%s. ENV: %s [ACCEPTED_FOR_RELEASE #%d v%s: %s]" % (base, why, acc["issue"], acc["release"], acc["why"]))
            return
        r.check(False, "%s. ENV by the control (%s), but no ACCEPTED_FOR_RELEASE entry is in force for it" % (base, why))
        return
    if cls == "platform":
        r.no_reference("%s. PLATFORM: %s" % (base, why))
        return
    if cls == "app":
        r.check(False, "%s. APP: %s" % (base, why))
    else:
        r.check(False, "%s (control: %s)" % (base, why))


@case("remote_heredoc_paste", order=62)
def c_remote_heredoc(ctx, r):
    """#1129: a large here-doc and a long single line sent to the REMOTE shell over ssh (the way an agent writes a file) arrive byte-exact. Pasted into the
    input and submitted with an instant Enter, so they take the same command-write path as an agent's command: a 2,000-line here-doc (~140 KB: above the
    128 KB no-bracketed-paste wrapper size), a 10 KB single line, the same 2,000-line payload in zsh and fish started inside the remote session (when
    installed; fish has no here-docs: a multi-line single-quoted printf) and LAST a ~600 KB here-doc (above the 512 KB size from which a bracketed
    multi-line command goes through the base64 wrapper, #1255). Every file is compared byte for byte with what was sent; the time each took is a metric.
    r18u22: a step with no file in its budget is judged against a CONTROL (the same bytes through plain ssh, no Phosphor; Windows: the reference
    console), and a remote shell still busy after a timeout is ended by the harness so the pane is left LOCAL."""
    need_ssh(ctx, r)
    import random as _random
    rr = _random.Random(1129)
    eof = "PHOSPHOR_EOF_1129"
    lines2k = _payload_lines(rr, 200_000, (40, 100))[:2000]
    long_line = "".join(rr.choice("abcdefghijklmnopqrstuvwxyz0123456789") for _ in range(10_000))
    big = _payload_lines(rr, 600_000)
    heredoc = lambda name, ls: "cat > ~/%s <<'%s'\n%s\n%s" % (name, eof, "\n".join(ls), eof)      # noqa: E731
    expect = lambda ls: ("\n".join(ls) + "\n").encode()                                              # noqa: E731
    bracketed = remote_bash_bracketed(ctx)
    r.metric("remote_bash_bracketed_paste", "unknown" if bracketed is None else bracketed)
    state = {"up": False}

    def session():
        """a usable remote session for the next step: connected, its shell's pid recorded (to end it after a timeout)"""
        if state["up"]:
            return True
        secs = connect(ctx, "fake", 60)
        r.check(secs is not None, "no remote shell prompt after `ssh fake`. Test host: %s" % ("" if secs is not None else ctx.ssh_diag()))
        if secs is None:
            return False
        time.sleep(1.0)
        record_rpid(ctx)
        state["up"] = True
        return True

    recs = []

    def step(label, name, command, expected, timeout, grace=0.0):
        if state.get("dead"):
            r.note("%s: not run, the pane could not be brought back from the stuck remote session (%s)" % (label, state["dead"]))
            return None
        if not session():
            return None
        mark = ctx.cmd_mark() if ctx.prof.local_prompts else None
        rec = _send_step(ctx, r, label, name, command, expected, timeout, grace)
        if rec["got"] is None:          # the remote shell is still in the payload: end it, or every later key queues behind it
            where = end_stuck_session(ctx, r, label, mark, command.split("\n"))
            state["up"] = False
            if where != "local":
                state["dead"] = where
        rec["io"] = sshd_io_lines(ctx, rec["sshd_off"]) if not rec["ok"] else []
        recs.append(rec)
        return rec

    # the 2k here-doc gets a grace period: bash 3.2 took 86 s for its 190 KB wrapper line (r18u22) against the 90 s budget, so a run that is just
    # over keeps a usable session and records the real time; the verdict is still the 90 s budget's
    step("bash: %d-line here-doc (%d bytes)" % (len(lines2k), len(expect(lines2k))), "hd_2k.txt", heredoc("hd_2k.txt", lines2k), expect(lines2k), 90, grace=150)
    step("bash: 10 KB single line", "hd_line.txt", "printf '%%s\\n' '%s' > ~/hd_line.txt" % long_line, (long_line + "\n").encode(), 60)
    # zsh / fish inside the remote session (same machine: the harness's sshd runs the remote shell locally, so find_bin applies)
    for key in ("zsh", "fish"):
        path, adddir = find_bin(key)
        if not path:
            r.note("%s not installed: the remote %s step is skipped" % (key, key))
            continue
        if not session():
            break
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
        rec = step("remote %s: %d-line payload" % (key, len(lines2k)), name, cmd, expect(lines2k), 120)
        if rec is not None and rec["got"] is None:
            continue            # the session (with the inner shell) was ended
        ctx.be.type("exit", 12, enter=True)
        time.sleep(1.5)
    # LAST: on a bash without bracketed paste its ~800 KB wrapper line can keep the remote shell busy far past the budget (r18u22 macOS: bash 3.2 at
    # 100 % cpu), and that must not take the steps above with it
    step("bash: %d-line here-doc (%d bytes, wrapper path)" % (len(big), len(expect(big))), "hd_big.txt", heredoc("hd_big.txt", big), expect(big), 180)
    if state.get("dead"):
        r.check(False, "after the harness ended the stuck remote shell the pane is not provably LOCAL (probe ran in: %s)" % state["dead"])
    elif state["up"]:
        ok = leave(ctx)
        r.check(ok, "no local prompt after `exit` from the remote shell" if ctx.left_unproven is None else
                "the pane is not provably REMOTE (probe ran in: %s), so `exit` was NOT typed" % ctx.left_unproven)
    else:
        where = ctx.where_now()
        r.check(where == "local", "after the harness ended the stuck remote shell the pane is not provably LOCAL (probe ran in: %s)" % where)
    ctx.reset()
    # the verdicts: a slow step is compared with the same bytes through plain ssh (run now, with the pane local and idle)
    ctl = None
    if any(not x["ok"] for x in recs) or any(x["name"] == "hd_2k.txt" and x["secs"] > 20 for x in recs):
        ctl = heredoc_control(ctx, r, eof, lines2k)
        ctx.reset()
    for rec in recs:
        judge_step(ctx, r, rec, ctl, bracketed, rec.get("io"))


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
        lat_checks(r, rec, "Enter to command start over ssh", known_key="latency_enter_ssh_overhead")
    ctx.reset()


# ------------------------------------------------------------------ failing / hanging host, password prompt
@case("ssh_hanging_host_not_local", order=62, gui=True)
def c_ssh_hang(ctx, r):
    """`ssh` to (a) a port that refuses and (b) a TCP listener that accepts and never speaks (a hanging host): the line typed right behind the
    ssh Enter behaves as in a plain terminal / iTerm2 (owner decision 2026-11-04, DECLINED.md): it runs in the LOCAL shell exactly once, as typed, after the
    ssh exited; the local shell must stay usable afterwards."""
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
            # Plain-terminal ssh pass-through (DECLINED.md, "A FAILING ssh behaves like a plain terminal too"): keys typed after ssh owns the tty sit in ssh's tty
            # queue; when ssh exits the local shell reads them and the line runs LOCALLY exactly once, as typed, and only after the ssh exited. If the ssh had already
            # exited before the first key the line is simply a local command: it must also run exactly once.
            got = ctx.lines(f)
            if exited_before_key:
                r.note("[%s host] the ssh client had %s before the first MUSTNOT key: the line is an ordinary local command (still must run exactly once)" % (
                    label, "already exited" if alive_ts else "not been seen"))
            r.check(got == ["MUSTNOT_" + tok], "[%s host] a plain terminal runs the line typed behind ssh locally exactly once after ssh exits; got %r (ssh alive at the first key: %s; exit %s)" % (
                label, got, not exited_before_key, ("+%.2f s" % (t_exit - t_ssh)) if t_exit else "not observed"))
            if got and t_exit is not None and not exited_before_key:
                try:
                    ran_at = os.path.getmtime(os.path.join(ctx.W, f))
                    r.check(ran_at >= t_exit - 1.0, "[%s host] the line ran locally %.2f s BEFORE the ssh exited: it must reach the local shell only after ssh exits" % (label, t_exit - ran_at))
                except OSError:
                    pass
            r.check(sum(1 for c in ctx.cmds_since(lmark) if "MUSTNOT_" + tok in c) == 1,
                    "[%s host] the LOCAL shell must receive the line typed behind ssh exactly once; got %r" % (label, [c for c in ctx.cmds_since(lmark) if "MUSTNOT_" + tok in c]))
            ctx.reset()
            tok2 = ctx.tok(6).lower()
            ctx.be.type("echo %s >> %s" % (tok2, f), 12, enter=True)
            ctx.until(lambda: len(ctx.lines(f)) >= 2, 8)
            exact(r, "[%s host] the local shell is usable afterwards (the typed-behind line ran once, then this one)" % label, ["MUSTNOT_" + tok, tok2], ctx.lines(f))
    finally:
        stop.set()
        th.join(2)
        for c in held:
            c.close()
        srv.close()
    ctx.reset()


def ps_has_port(port):
    """is an ssh client for 127.0.0.1 -p <port> running (matched on the args)"""
    if os.name == "nt":
        # H5 (r18u8 Windows): POSIX `ps` does not exist in the Windows harness PATH, so the poll never saw the ssh and the case reported "ssh never seen". The
        # command lines come from CIM (Win32_Process), as in ps_has below; the msys ssh and ssh.exe are both matched.
        try:
            out = subprocess.run(["powershell", "-NoProfile", "-Command", "Get-CimInstance Win32_Process -Filter \"Name='ssh.exe'\" | ForEach-Object { $_.CommandLine }"],
                                 capture_output=True, text=True, timeout=30).stdout
        except (OSError, subprocess.TimeoutExpired):
            return False
        return any(re.search(r"\bssh(\.exe)?\"?\s.*-p\s*%d\b" % port, l) for l in out.splitlines())
    try:
        out = subprocess.run(["ps", "-eo", "args"], capture_output=True, text=True, timeout=5).stdout
    except (OSError, subprocess.TimeoutExpired):
        return False
    return any(re.match(r"(\S*/)?ssh\s.*-p\s*%d\b" % port, l) for l in out.splitlines())


def ps_has(host):
    """is an ssh client for <host> running (Phosphor's ssh wrapper may add options, so match the args, not the typed text). Windows has no POSIX `ps`
    in the harness's PATH: the command lines come from CIM (Win32_Process)."""
    if os.name == "nt":
        try:
            out = subprocess.run(["powershell", "-NoProfile", "-Command", "Get-CimInstance Win32_Process -Filter \"Name='ssh.exe'\" | ForEach-Object { $_.CommandLine }"],
                                 capture_output=True, text=True, timeout=30).stdout
        except (OSError, subprocess.TimeoutExpired):
            return False
        return any(re.search(r"\bssh(\.exe)?\"?\s.*\b%s\b" % re.escape(host), l) for l in out.splitlines())
    out = subprocess.run(["ps", "-eo", "args"], capture_output=True, text=True).stdout
    return any(re.match(r"(\S*/)?ssh\s.*\b%s\b" % re.escape(host), l) for l in out.splitlines())


def password_variants(ctx):
    """[(host, secret, prompt word, label)] of the secret prompts this ssh test host can give: the passphrase of a protected key (system OpenSSH sshd
    only: `ssh fakepw`) and a REAL password (#1535: `ssh fakepass`, the harness fake sshd's one user; also on Windows). Empty -> the case skips."""
    out = []
    if ctx.ssh.get("pwident"):
        out.append(("fakepw", ctx.ssh["passphrase"], "passphrase", "passphrase"))
    pw = ctx.ssh.get("pwhost")
    if pw and pw.get("password"):
        out.append(("fakepass", pw["password"], "password", "password"))
    return out


@case("ssh_password_prompt", order=63)
def c_ssh_password(ctx, r):
    """The owner's rule for a secret prompt: keys typed BEFORE the prompt are discarded (never sent to the prompt, never run in a shell), keys typed
    AFTER it reach the prompt. Two prompts, each with ssh's own text on the tty: `Enter passphrase for key` (host `fakepw`: a passphrase-protected key
    the system sshd accepts) and, since #1535, a real `pwuser@127.0.0.1's password:` (host `fakepass`: the harness fake sshd's one password user, so
    no account or PAM is needed; Linux, macOS and the Windows ssh clients). NumberOfPasswordPrompts 1, so one wrong try ends the ssh. Evidence per
    prompt: the correct secret typed after the prompt logs in (junk typed before did not consume the single attempt), the junk and the secret are in no
    shell's rec.log, a wrong secret returns to the LOCAL shell where the next command runs locally."""
    need_ssh(ctx, r)
    variants = password_variants(ctx)
    if not variants:
        r.skip("no secret-prompt host: the passphrase prompt needs the system OpenSSH sshd (apt: openssh-server), the password prompt needs the paramiko fake sshd (pip/apt: python3-paramiko)")
    kinds = {v[3] for v in variants}
    if "password" not in kinds:
        r.note("the real PASSWORD prompt was not exercised: no fake sshd with a password user (python3-paramiko missing?); only the passphrase prompt ran")
    if "passphrase" not in kinds:
        r.note("the key PASSPHRASE prompt was not exercised: it needs the system OpenSSH sshd; only the password prompt ran")
    for host, secret, word, label in variants:
        secret_prompt(ctx, r, host, secret, word, label)


def wait_secret_prompt(ctx, r, label, word):
    """wait until ssh's secret prompt is on screen before ANYTHING is typed (r18u8: under pass-through a key typed earlier is ssh's, and ssh.exe takes it as a
    password attempt). OCR (when an engine is installed) is polled for the prompt word up to 20 s; with no OCR the case waits a fixed 3 s and says so."""
    shot = "pw_%s_prompt" % label
    deadline = time.time() + 20
    txt = None
    probe = ctx.ocr(shot + "_ocr")
    if probe is None:
        time.sleep(3.0)
        r.note("[%s] no OCR here: the `%s` prompt was not seen, a fixed 3 s wait stood in (nothing was typed before it)" % (label, word))
        ctx.shot(shot)
        return
    while time.time() < deadline:
        txt = ctx.ocr(shot + "_ocr") or ""
        if word in txt.lower():
            break
        time.sleep(0.5)
    ctx.shot(shot)
    r.check(word in (txt or "").lower(), "[%s] OCR does not show the `%s` prompt within 20 s: ssh may have failed before prompting" % (label, word))
    time.sleep(0.3)


def secret_prompt(ctx, r, host, pw, word, label):
    f = "pw_%s.txt" % label
    rm_remote(ctx, f)
    ctx.rm(f)
    lm, rm_ = ctx.cmd_mark(), len(ctx.prof.commands(remote=True))
    before = rprompts(ctx)
    ctx.be.type("ssh %s" % host, 12, enter=True)
    # r18u8 (Mac + Windows): under the plain-terminal ssh pass-through, keys typed before the prompt exists reach ssh like any terminal (ssh.exe on Windows
    # consumes them as a password attempt, and NumberOfPasswordPrompts is 1). The case therefore types NOTHING until the prompt is up, then the secret,
    # and types local commands only after ssh exited.
    ctx.until(lambda: ps_has(host), 8, step=0.05)
    wait_secret_prompt(ctx, r, label, word)
    ctx.be.type(pw, 15, enter=True)                    # after the prompt: must reach it
    ok = ctx.until(lambda: rprompts(ctx) > before, 30, step=0.1)
    r.check(bool(ok), "[%s] keys typed AFTER the prompt did not reach it: no remote shell after typing the %s " % (label, word))
    ctx.shot("pw_%s_after" % label)
    if ok:
        tok = ctx.tok(8).lower()
        time.sleep(1.0)
        ctx.be.type("echo %s >> ~/%s" % (tok, f), 12, enter=True)
        ctx.until(lambda: ctx.lines(f, remote=True), 8)
        exact(r, "[%s] remote command after the %s login" % (label, word), [tok], ctx.lines(f, remote=True))
        leave(ctx)
    allcmds = [c for _, c in ctx.prof.commands()[lm:]] + [c for _, c in ctx.prof.commands(remote=True)[rm_:]]
    r.check(not any(pw in c for c in allcmds), "[%s] the %s ended up in a shell's command history (it was typed into a shell, not the prompt)" % (label, word))
    after = ctx.ocr("pw_%s_after_ocr" % label)
    if after is not None:
        r.check(pw not in after, "[%s] the %s is visible on screen (echoed)" % (label, word))
    # wrong secret: ssh ends, the LOCAL shell is back and the next line runs locally
    ctx.reset()
    lm = ctx.cmd_mark()
    ctx.be.type("ssh %s" % host, 12, enter=True)
    ctx.until(lambda: ps_has(host), 8, step=0.05)
    wait_secret_prompt(ctx, r, label + "-wrong", word)
    ctx.be.type("wrong-" + ctx.tok(4), 15, enter=True)
    gone = ctx.until(lambda: not ps_has(host), 20, step=0.2)
    r.check(bool(gone), "[%s] the ssh client was still running 20 s after a wrong %s (NumberOfPasswordPrompts 1): local commands are not typed until it exits" % (label, word))
    time.sleep(1.5)
    tok = ctx.tok(8).lower()
    ctx.be.type(ctx.append_cmd(ctx.pane_shell(), tok, f), 12, enter=True)
    ctx.until(lambda: ctx.lines(f), 8)
    exact(r, "[%s] after a wrong %s the next command ran in the LOCAL shell" % (label, word), [tok], ctx.lines(f))
    r.check(not any("wrong-" in c for _, c in ctx.prof.commands()[lm:]), "[%s] the wrong %s was run as a shell command" % (label, word))
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
