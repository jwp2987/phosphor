# appci/cases_coverage_a.py: real-app cases for the CI-review coverage gaps #1535 ssh, #1536 sudo/su/ksu, #1537 alt-screen programs,
# #1538 paste, #1540/#1541 crash + session restore. Registered with the same @case decorator as cases.py (one import line at its end).
# Same rules as cases.py: the EXACT command / bytes the program received (effect file + rec.log), typing at 10-25 ms/key with an instant
# Enter, a screenshot at every decisive step, a case that cannot run SKIPS with the precise reason and the package/permission it needs, and a
# program that is MISSING marks the case unverifiable (status noref, never a quiet pass: #1537 "silently uses less when vim is missing").
#
#   paste_multiline_prompt     a 3-line paste into the prompt, then Enter: each line runs exactly once, in order (terminal mode)
#   paste_bracketed_off        a program that turned bracketed paste OFF (`cat`) receives a multi-line paste byte for byte, no ESC[200~ markers
#   paste_agent_input          multi-line paste into the AGENT input: the app's own hop=enter_begin line holds the pasted text exactly, the shell nothing
#   alt_screen_programs        vim / less / htop / tmux each receive their typed keys exactly and the shell is usable and exact afterwards
#   alt_screen_mouse_reporting a program that enables SGR mouse reporting gets the click as ESC[<0;x;yM and the keys typed after it (pointer: Linux)
#   sudo_su_real_shell         a real privilege-boundary shell (sudo -n, and su when the runner is root): typed lines exact inside, exit returns local
#   ssh_drop_reconnect         ssh dropped by `~.` and by killing the remote shell: the local pane works, a reconnect works, exact lines each time
#   crash_kill9_restore        SIGKILL of the app with restore_session on: relaunch comes up, nothing re-runs, the pane is usable and exact
#   crash_corrupt_db           a garbage warp.sqlite: the app starts, moves it aside (warp.sqlite.corrupt-*, #860) and the pane is usable and exact
import glob, os, re, shutil, signal, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402  (cases.py imports this module near its end, so everything below is already defined)
import cases_robust as R  # noqa: E402
import cases_caseremote as CR  # noqa: E402
import cases_caseagent as CA  # noqa: E402
import cases_ui as UI  # noqa: E402
from cases import case  # noqa: E402


def exact_lines(r, label, exp, got):
    ok = got == exp
    r.check(ok, "%s: expected %r, got %r" % (label, exp, got))
    return ok


def local_cmd_exact(ctx, r, label):
    """one typed line to the LOCAL shell (10-25 ms/key, instant Enter): the exact line must be what rec.log / the effect file shows"""
    tok = ctx.tok(6).lower()
    f = "ca_local_%s.txt" % label
    ctx.rm(f)
    mark = ctx.cmd_mark()
    line = ctx.append_cmd(ctx.pane_shell(), tok, f)
    ctx.be.type(line, ctx.rng.randint(10, 25), enter=True)
    ctx.until(lambda: ctx.lines(f), 8)
    exact_lines(r, "%s: the local shell's effect file" % label, [tok], ctx.lines(f))
    if ctx.prof.commands():
        got = ctx.until(lambda: [c for c in ctx.cmds_since(mark) if c == line], 5)
        r.check(bool(got), "%s: the local shell received %r, not %r" % (label, ctx.cmds_since(mark)[:2], line))


# ------------------------------------------------------------------ #1538 paste
@case("paste_multiline_prompt", modes=("terminal",), order=16)
def c_paste_multiline(ctx, r):
    """A 3-line paste into the shell prompt, then Enter (twice, the second on an empty prompt is harmless): every line runs exactly once and in
    order, whatever the editor does with the newlines (the effect file is the judge). Run for a short and a long middle line."""
    sh = ctx.pane_shell()
    for variant, pad in (("short", ""), ("long", "w" * 3000)):
        f = "pm_%s.txt" % variant
        ctx.rm(f)
        toks = [ctx.tok(6).lower() + pad for _ in range(3)]
        text = "\n".join(ctx.append_cmd(sh, t, f) for t in toks)
        ctx.be.clip_set(text)
        ctx.reset()
        ctx.be.paste()
        time.sleep(0.8)
        ctx.shot("pm_%s_pasted" % variant)
        ran = len(ctx.lines(f))
        r.metric("%s_lines_run_before_enter" % variant, ran)
        ctx.be.key("Return")
        time.sleep(0.4)
        ctx.be.key("Return")
        ctx.until(lambda: len(ctx.lines(f)) >= 3, 10)
        time.sleep(0.5)
        exact_lines(r, "multi-line paste (%s): each line exactly once, in order" % variant, toks, ctx.lines(f))
        ctx.shot("pm_%s_done" % variant)
    local_cmd_exact(ctx, r, "pm_after")


@case("paste_bracketed_off", modes=("terminal",), order=17)
def c_paste_bracketed_off(ctx, r):
    """`cat > file` is a program that never enabled bracketed paste: a multi-line paste (tabs, a trailing line, no final newline) must arrive byte
    for byte, with no ESC[200~ / ESC[201~ marker and nothing doubled. The program is started after the shell's own bracketed-paste mode is turned off
    (printf '\\e[?2004l'), so the app must follow the terminal's CURRENT mode, not a stale one."""
    ctx.rm("pb.out")
    ctx.be.type("printf '\\033[?2004l'; cat > pb.out", ctx.rng.randint(10, 25), enter=True)
    time.sleep(1.0)
    data = "line1 %s\n\tindented\tline2\nline3 end" % ctx.tok(6)
    ctx.be.clip_set(data)
    ctx.be.paste()
    time.sleep(1.0)
    ctx.shot("pb_pasted")
    ctx.be.key("Return")
    time.sleep(0.2)
    ctx.be.key("ctrl+d")
    ctx.until(lambda: os.path.exists(os.path.join(ctx.W, "pb.out")) and ctx.lines("pb.out"), 8)
    time.sleep(0.4)
    got = open(os.path.join(ctx.W, "pb.out"), "rb").read().decode(errors="replace") if os.path.exists(os.path.join(ctx.W, "pb.out")) else ""
    r.check("\x1b[200~" not in got and "\x1b[201~" not in got, "bracketed-paste markers reached a program that had them OFF: %r" % got[:80])
    r.check(got.replace("\r\n", "\n") == data + "\n", "paste into a program with bracketed paste off: expected %r, got %r" % (data + "\n", got))
    ctx.be.type("printf '\\033[?2004h'", 12, enter=True)       # restore the shell's mode for the cases that follow
    time.sleep(0.3)
    local_cmd_exact(ctx, r, "pb_after")


@case("paste_agent_input", modes=("agent",), order=18, gui=True)
def c_paste_agent(ctx, r):
    """Multi-line prose pasted into the AGENT input, then Enter: the app's own `hop=enter_begin` line holds the pasted text byte for byte and routed
    AI, and the shell receives nothing (#1538). The request that starts is stopped with one Ctrl-C."""
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.skip("no app log at %s: the agent input at Enter cannot be read" % ctx.log_path)
    if ctx.log_view_mode() is None:
        r.skip("the app log has no hop=view_mode line (a build older than #1568): the view cannot be proven")
    r.check(ctx.log_view_mode() == "agent", "agent view expected but the log says hop=view_mode mode=%s" % ctx.log_view_mode())
    ctx.reset()
    for i, sent in enumerate(["please explain why the build %s fails,\nit says: cannot find -lfoo\nand then stops" % ctx.tok(5),
                              "summarise this list please:\n- alpha %s\n- beta (two words)\n- gamma 'quoted'" % ctx.tok(4)]):
        off, mark = ctx.log_size(), ctx.cmd_mark()
        ctx.be.clip_set(sent)
        ctx.be.paste()
        time.sleep(0.8)
        ctx.shot("paste_agent_%d_pasted" % i)
        ctx.be.key("Return")
        ctx.until(lambda: CA.enter_buffers(ctx.log_since(off)), 6)
        rows = CA.enter_buffers(ctx.log_since(off))
        err = CA.judge_agent_input(sent, rows)
        r.check(err is None, "agent paste #%d: %s" % (i + 1, err))
        r.check(not [c for c in ctx.cmds_since(mark) if "please" in c or "summarise" in c or "alpha" in c],
                "agent paste #%d reached the SHELL: %r" % (i + 1, ctx.cmds_since(mark)[:2]))
        ctx.shot("paste_agent_%d_sent" % i)
        ctx.be.key("ctrl+c")
        ctx.ctrlc_ts.append(time.time())
        time.sleep(0.8)
    r.check(ctx.log_view_mode() == "agent", "the view left agent mode during the case (hop=view_mode says %s)" % ctx.log_view_mode())
    ctx.reset()


# ------------------------------------------------------------------ #1537 alt-screen programs
def missing(r, prog, why):
    """a program the case wants is not installed: unverifiable (status noref), with the package; never a silent substitute"""
    r.unverifiable("%s is not installed (%s): that part of the alt-screen coverage did not run" % (prog, why))


@case("alt_screen_programs", order=22, gui=True)
def c_alt_programs(ctx, r):
    """Each full-screen program gets keys typed at 10-25 ms and proves it received them; afterwards the shell is back and a typed line is exact.
    vim: typed text saved to a file. less: `!cmd` inside less runs a shell line that writes a token. htop: starts, draws, q returns. tmux: a pane
    inside tmux receives a typed line (rec.log of the inner shell sees it exactly), `exit` ends tmux. A missing program is UNVERIFIABLE, not skipped."""
    # ---- vim
    if ctx.which("vim"):
        ctx.rm("ap_vim.txt")
        tok = ctx.tok(6).lower()
        ctx.be.type("vim -u NONE -N ap_vim.txt", ctx.rng.randint(10, 25), enter=True)
        time.sleep(1.5)
        ctx.be.type("i" + tok, ctx.rng.randint(10, 25))
        ctx.shot("ap_vim_insert")
        ctx.be.key("Escape")
        ctx.be.type(":wq", ctx.rng.randint(10, 25), enter=True)
        ctx.until(lambda: ctx.lines("ap_vim.txt"), 8)
        exact_lines(r, "vim received the typed text", [tok], ctx.lines("ap_vim.txt"))
        local_cmd_exact(ctx, r, "ap_after_vim")
    else:
        missing(r, "vim", "package vim")
    # ---- less
    if ctx.which("less"):
        ctx.rm("ap_less.txt")
        tok = ctx.tok(6).lower()
        ctx.be.type("seq 1 400 | less", ctx.rng.randint(10, 25), enter=True)
        time.sleep(1.2)
        ctx.be.type("G", 20)
        time.sleep(0.4)
        ctx.shot("ap_less_end")
        ctx.be.type("!echo %s >> ap_less.txt" % tok, ctx.rng.randint(10, 25), enter=True)
        time.sleep(0.8)
        ctx.be.key("Return")           # less waits for a key after a ! command
        ctx.until(lambda: ctx.lines("ap_less.txt"), 8)
        ctx.be.type("q", 20)
        time.sleep(0.6)
        exact_lines(r, "less ran the shell line typed inside it", [tok], ctx.lines("ap_less.txt"))
        local_cmd_exact(ctx, r, "ap_after_less")
    else:
        missing(r, "less", "package less")
    # ---- htop
    if ctx.which("htop"):
        ctx.be.type("htop", ctx.rng.randint(10, 25), enter=True)
        time.sleep(2.0)
        ctx.shot("ap_htop")
        ctx.be.type("q", 20)
        time.sleep(0.8)
        local_cmd_exact(ctx, r, "ap_after_htop")
    else:
        missing(r, "htop", "package htop")
    # ---- tmux (an alt-screen program that hosts another shell and keeps its own mouse/keys handling)
    if ctx.which("tmux"):
        ctx.rm("ap_tmux.txt")
        tok = ctx.tok(6).lower()
        mark = ctx.cmd_mark()
        before = ctx.prompts()
        ctx.be.type("tmux -f /dev/null new-session -s appci_%s" % tok, ctx.rng.randint(10, 25), enter=True)
        ctx.until(lambda: ctx.prompts() > before, 15, step=0.2)
        time.sleep(1.0)
        ctx.shot("ap_tmux_up")
        line = ctx.append_cmd("bash", tok, "ap_tmux.txt")
        ctx.be.type(line, ctx.rng.randint(10, 25), enter=True)
        ctx.until(lambda: ctx.lines("ap_tmux.txt"), 8)
        exact_lines(r, "the shell inside tmux ran the typed line once", [tok], ctx.lines("ap_tmux.txt"))
        if ctx.prof.commands():
            r.check(line in ctx.cmds_since(mark), "the shell inside tmux received %r, not %r" % (ctx.cmds_since(mark)[:3], line))
        ctx.be.type("exit", 15, enter=True)
        time.sleep(1.0)
        ctx.shot("ap_tmux_exited")
        local_cmd_exact(ctx, r, "ap_after_tmux")
    else:
        missing(r, "tmux", "package tmux")


MOUSE_PROG = r'''import sys, os, tty, termios, select, time
fd = sys.stdin.fileno(); old = termios.tcgetattr(fd); tty.setraw(fd)
buf = b""
try:
    sys.stdout.write("\x1b[?1000h\x1b[?1006hMOUSEREADY\r\n"); sys.stdout.flush()
    end = time.time() + 45
    while time.time() < end and not buf.endswith(b"k"):
        if select.select([fd], [], [], 0.2)[0]:
            buf += os.read(fd, 4096)
finally:
    sys.stdout.write("\x1b[?1000l\x1b[?1006l"); sys.stdout.flush()
    termios.tcsetattr(fd, termios.TCSADRAIN, old)
    open(sys.argv[1], "w").write(repr(buf))
'''


@case("alt_screen_mouse_reporting", modes=("terminal",), order=23, gui=True)
def c_alt_mouse(ctx, r):
    """A program turns on SGR mouse reporting (ESC[?1000h ESC[?1006h); a real pointer click inside the terminal must reach it as ESC[<0;col;rowM
    (press) and ...m (release), and the key typed after the click ('k') arrives intact and last. Afterwards the mode is off and the shell exact.
    The pointer is xdotool (X11) / the sway seat (Wayland); the click position is found by OCR of the program's banner, else the window centre."""
    if ctx.win or ctx.be.name == "mac":
        r.skip("no pointer synthesis in the %s backend (Linux xdotool / sway seat only); the program itself needs python3 termios" % ctx.be.name)
    if not ctx.which("python3"):
        r.skip("python3 missing on the pane's PATH")
    mouse = UI.mouse_or_skip(ctx, r)
    prog = os.path.join(ctx.W, "ca_mouse.py")
    with open(prog, "w") as f:
        f.write(MOUSE_PROG)
    ctx.rm("ca_mouse.out")
    ctx.be.type("python3 ca_mouse.py ca_mouse.out", ctx.rng.randint(10, 25), enter=True)
    time.sleep(1.5)
    pos = None
    if shutil.which("tesseract"):
        hit, _ = UI.find_word(ctx, "ca_mouse_find", "mouseready")
        pos = (hit[1], hit[2] + 60) if hit else None
    if pos is None:
        shot = ctx.shot("ca_mouse_size")
        try:
            from PIL import Image
            w, h = Image.open(shot).size
            pos = (w // 2, h // 2)
            r.note("OCR did not locate the banner; clicked the window centre %r" % (pos,))
        except Exception:
            r.skip("cannot place the pointer: no tesseract hit and no Pillow to read the screenshot size (packages: tesseract-ocr, python3-pil)")
    ctx.shot("ca_mouse_before_click")
    mouse.click(pos[0], pos[1])
    time.sleep(0.4)
    ctx.be.type("k", 20)
    ctx.until(lambda: os.path.exists(os.path.join(ctx.W, "ca_mouse.out")), 10)
    time.sleep(0.4)
    ctx.shot("ca_mouse_after")
    got = open(os.path.join(ctx.W, "ca_mouse.out")).read() if os.path.exists(os.path.join(ctx.W, "ca_mouse.out")) else ""
    r.check(bool(re.search(r"\\x1b\[<0;\d+;\d+M", got)), "no SGR mouse press ESC[<0;x;yM reached the program that asked for mouse reporting: %s" % got[:200])
    r.check(bool(re.search(r"\\x1b\[<0;\d+;\d+m", got)), "no SGR mouse release ESC[<0;x;ym reached the program: %s" % got[:200])
    r.check(got.endswith("k'") or got.endswith('k"'), "the key typed after the click is not last/intact: %s" % got[-80:])
    local_cmd_exact(ctx, r, "ca_mouse_after")


# ------------------------------------------------------------------ #1536 sudo / su / ksu
def nested_root_lines(ctx, r, label, start_cmd, n, wanted_uid):
    """type start_cmd, prove the inner shell by `id -u` into a file, type n exact lines into it, exit, and prove the local shell is back"""
    f = "ca_%s.txt" % label
    ctx.rm(f)
    abs_f = os.path.join(ctx.W, f)
    ctx.be.type(start_cmd, ctx.rng.randint(10, 25), enter=True)
    time.sleep(2.5)
    ctx.shot("%s_inner" % label)
    ctx.be.type("id -u >> %s" % abs_f, ctx.rng.randint(10, 25), enter=True)
    ctx.until(lambda: ctx.lines(f), 10)
    first = ctx.lines(f)
    r.check(first == [str(wanted_uid)], "%s: the inner shell is not the expected user: `id -u` gave %r, expected %r" % (label, first, wanted_uid))
    toks = [ctx.tok(8).lower() for _ in range(n)]
    for t in toks:
        ctx.be.type("echo %s >> %s" % (t, abs_f), ctx.rng.randint(10, 25), enter=True)
    ctx.until(lambda: len(ctx.lines(f)) >= n + 1, 15)
    exact_lines(r, "%s: lines typed into the privileged shell" % label, [str(wanted_uid)] + toks, ctx.lines(f))
    before = ctx.prompts()
    ctx.be.type("exit", 15, enter=True)
    ctx.until(lambda: ctx.prompts() > before, 15)
    ctx.shot("%s_exited" % label)
    uid = str(os.geteuid())
    f2 = "ca_%s_back.txt" % label
    ctx.rm(f2)
    ctx.be.type("id -u >> %s" % os.path.join(ctx.W, f2), ctx.rng.randint(10, 25), enter=True)
    ctx.until(lambda: ctx.lines(f2), 8)
    exact_lines(r, "%s: after exit the LOCAL shell is back (own uid)" % label, [uid], ctx.lines(f2))
    local_cmd_exact(ctx, r, label + "_after")


@case("sudo_su_real_shell", modes=("terminal",), order=65, gui=True)
def c_sudo_su(ctx, r):
    """The real privilege boundary the nested-bash stand-in of remote_session_* only imitates (#1536). sudo: needs passwordless sudo (`sudo -n true`);
    su: needs the runner to be root (su then needs no password); ksu: needs a Kerberos ticket. Each part that cannot run is UNVERIFIABLE with the reason,
    and the case SKIPS when none can. Proof of the boundary is `id -u` typed INTO the inner shell, never the prompt."""
    if ctx.win or os.name == "nt":
        r.skip("sudo/su/ksu do not exist in a Windows pane")
    ran = 0
    if shutil.which("sudo") and subprocess.run(["sudo", "-n", "true"], capture_output=True).returncode == 0:
        ran += 1
        nested_root_lines(ctx, r, "sudo", "sudo -n env -i HOME=/root PATH=\"$PATH\" bash --noprofile --norc -i", ctx.count(8), 0)
    else:
        r.unverifiable("sudo: `sudo -n true` fails on this machine (no passwordless sudo, or sudo missing): the sudo boundary did not run")
    if os.geteuid() == 0 and shutil.which("su") and shutil.which("id"):
        ran += 1
        who = "nobody"
        try:
            import pwd
            uid = pwd.getpwnam(who).pw_uid
            os.chmod(ctx.W, 0o777)
            nested_root_lines(ctx, r, "su", "su -s /bin/bash %s" % who, ctx.count(6), uid)
        except KeyError:
            r.unverifiable("su: no user %r on this machine" % who)
    else:
        r.unverifiable("su: needs the runner to be root so that su asks no password (euid %d)" % os.geteuid())
    if shutil.which("ksu") and subprocess.run(["klist", "-s"], capture_output=True).returncode == 0:
        r.unverifiable("ksu: present with a valid ticket, but the target principal is site-specific; run it by hand (owner's realm)")
    else:
        r.unverifiable("ksu: not installed or no Kerberos ticket (package krb5-user + kinit)")
    if not ran:
        r.skip("no privilege boundary could run here: passwordless sudo and root-for-su are both absent")


# ------------------------------------------------------------------ #1535 ssh drop + reconnect
def remote_lines_exact(ctx, r, label, n):
    f = "ca_%s_r.txt" % label
    CR.rm_remote(ctx, f)
    toks = [ctx.tok(8).lower() for _ in range(n)]
    CR.typed_lines(ctx, ["echo %s >> ~/%s" % (t, f) for t in toks])
    ctx.until(lambda: len(ctx.lines(f, remote=True)) >= n, 15)
    exact_lines(r, "%s: lines typed into the remote shell" % label, toks, ctx.lines(f, remote=True))


@case("ssh_drop_reconnect", modes=("terminal",), order=66, gui=True)
def c_ssh_drop(ctx, r):
    """Two ways to lose a live ssh session, then a reconnect (#1535 'no disconnect/reconnect'): (a) the client escape `<Enter>~.`, (b) the remote shell
    killed from inside (`kill -9 $$`, the connection ends like a dropped link). After each: the pane's LOCAL shell works and is exact (a line typed right
    behind the drop must not be lost or run remotely), a new `ssh fake` gives a remote prompt, and lines typed there are exact."""
    CR.need_ssh(ctx, r)
    if ctx.win:
        r.skip("not yet ported to the Windows panes: the `~.` escape and kill -9 $$ need the msys ssh client (unverified there)")
    for variant in ("escape", "kill"):
        secs = CR.connect(ctx, "fake", 90)
        r.check(secs is not None, "[%s] no remote prompt after `ssh fake`. Test host: %s" % (variant, ctx.ssh_diag() if secs is None else ""))
        if secs is None:
            ctx.reset()
            return
        time.sleep(1.0)
        remote_lines_exact(ctx, r, variant + "_1", ctx.count(5))
        ctx.shot("ssh_drop_%s_connected" % variant)
        before = ctx.prompts()
        if variant == "escape":
            ctx.be.key("Return")
            time.sleep(0.3)
            ctx.be.type("~.", 40)
        else:
            ctx.be.type("kill -9 $$", 15, enter=True)
        up = ctx.until(lambda: ctx.prompts() > before, 20, step=0.1)
        r.check(bool(up), "[%s] no local prompt within 20 s after the connection was lost" % variant)
        ctx.shot("ssh_drop_%s_dropped" % variant)
        where = ctx.where_now()
        r.check(where == "local", "[%s] after the drop the pane is %r, expected local" % (variant, where))
        local_cmd_exact(ctx, r, "ssh_drop_%s" % variant)
        secs = CR.connect(ctx, "fake", 90)
        r.check(secs is not None, "[%s] reconnect: no remote prompt after `ssh fake`" % variant)
        if secs is None:
            ctx.reset()
            return
        time.sleep(1.0)
        remote_lines_exact(ctx, r, variant + "_2", ctx.count(5))
        ctx.shot("ssh_drop_%s_reconnected" % variant)
        r.check(CR.leave(ctx), "[%s] `exit` on the reconnected session did not return to the local shell (%s)" % (variant, ctx.left_unproven))
        local_cmd_exact(ctx, r, "ssh_drop_%s_end" % variant)


# ------------------------------------------------------------------ #1540 / #1541 crash + restore
def sigkill_app(ctx, r, label):
    """the app dies with no quit path (R.hard_kill: SIGKILL on Linux/macOS, taskkill /F /T on Windows)"""
    return R.hard_kill(ctx, r, label)


def cd_and_append(ctx, d, tok, f):
    """one line for the pane: cd into `d`, then append `tok` to the absolute effect file `f` (PowerShell 5.1 has no `&&`: `;`)"""
    d, f = R.fwd(d), R.fwd(f)
    if ctx.pane == "powershell":
        return "Set-Location -LiteralPath '%s'; Add-Content -LiteralPath '%s' %s" % (d, f, tok)
    return "cd %s && echo %s >> %s" % (d, tok, f)


@case("crash_kill9_restore", modes=("terminal",), order=87, gui=True)
def c_crash_kill9(ctx, r):
    """The app is SIGKILLed (no quit path, no flush) with restore_session on; the relaunch must come up with a shell, must not re-run anything the
    old shells ran (effect file unchanged, rec.log has no command before the first key), and the pane must be usable and exact. How many tabs came
    back is a metric: a hard kill may lose the last debounce of the session state, but must never replay a command."""
    R.need_relaunch_app(ctx, "crash_kill9_restore")
    n = int(os.environ.get("APPCI_SR_TABS", "3"))
    try:
        q = R.stop_for_restart(ctx, r, 20, "ck_setup")
        r.check(q["exited"], "ck_setup: the app did not exit through the quit key within 20 s (%r)" % q)
        if not q["exited"]:
            return
        R.set_restore(ctx, True)
        _, up = R.start_app(ctx, r, "ck_first")
        if not up:
            return
        ctx.rm("ck_eff.txt")
        for i in range(n):
            d = os.path.join(ctx.W, "ck_dir_%d" % i)
            os.makedirs(d, exist_ok=True)
            if i:
                ctx.be.key(R.NEWTAB)
                time.sleep(0.8)
                ctx.be.guard()
            ctx.be.type(cd_and_append(ctx, d, "CK%d_%s" % (i, ctx.tok(5).lower()), os.path.join(ctx.W, "ck_eff.txt")), ctx.rng.randint(10, 25), enter=True)
            ctx.until(lambda: len(ctx.lines("ck_eff.txt")) >= i + 1, 10)
            time.sleep(0.4)
        eff = ctx.lines("ck_eff.txt")
        r.check(len(eff) == n, "setup: effect file has %d/%d lines before the kill" % (len(eff), n))
        time.sleep(3.0)                     # let the session writer run at least once
        ctx.shot("ck_before_kill")
        if not sigkill_app(ctx, r, "ck_kill"):
            return
        mark, before = ctx.cmd_mark(), ctx.prompts()
        newp, up = R.start_app(ctx, r, "ck_relaunch", restored_min=1)
        if not up:
            return
        r.metric("restored_shell_prompts", newp)
        ctx.shot("ck_relaunched")
        r.check(ctx.lines("ck_eff.txt") == eff, "RE-RUN: the effect file changed across the crash restore: %r -> %r" % (eff, ctx.lines("ck_eff.txt")))
        ran = ctx.cmds_since(mark)
        r.check(not ran, "RE-RUN: rec.log shows %d command(s) before any key was sent: %r" % (len(ran), ran[:5]))
        if not ctx.prof.commands():
            r.note("no rec.log in this pane (PowerShell): the re-run check rests on the effect file alone")
        r.check(newp >= 1, "no shell started after the crash (prompts +%d)" % newp)
        local_cmd_exact(ctx, r, "ck_after")
    finally:
        R.fresh_app(ctx, r, "ck")


def find_db(ctx):
    """every warp.sqlite under the app's state directories: the scratch home (Linux, macOS) or the Windows known folders (R.data_roots)"""
    hits = []
    for base in R.data_roots(ctx):
        for root, _dirs, files in os.walk(base):
            if "warp.sqlite" in files:
                hits.append(os.path.join(root, "warp.sqlite"))
    return hits


@case("crash_corrupt_db", modes=("terminal",), order=88, gui=True)
def c_crash_corrupt_db(ctx, r):
    """warp.sqlite replaced by garbage while the app is down (a torn write, a full disk): the next launch must come up with a working shell, move the
    file aside as warp.sqlite.corrupt-<secs> (#860) instead of looping or crashing, and the pane must be exact. (The in-app notice is a screenshot.)"""
    R.need_relaunch_app(ctx, "crash_corrupt_db")
    try:
        q = R.stop_for_restart(ctx, r, 20, "cd_setup")
        r.check(q["exited"], "cd_setup: the app did not exit through the quit key within 20 s (%r)" % q)
        if not q["exited"]:
            return
        dbs = find_db(ctx)
        if not dbs:
            r.skip("no warp.sqlite under %s: nothing to corrupt (the app keeps its state elsewhere on this build)" % R.data_roots(ctx))
        for db in dbs:
            for suffix in ("-wal", "-shm"):
                try:
                    os.remove(db + suffix)
                except FileNotFoundError:
                    pass
            with open(db, "wb") as f:
                f.write(b"this is not a sqlite database\n" * 4000)
        R.set_restore(ctx, True)
        _, up = R.start_app(ctx, r, "cd_relaunch")
        r.check(up, "the app did not come up with a shell on a corrupt warp.sqlite")
        ctx.shot("cd_relaunched")
        if not up:
            return
        aside = [p for db in dbs for p in glob.glob(db + ".corrupt-*")]
        r.check(bool(aside), "the garbage warp.sqlite was not moved aside (no warp.sqlite.corrupt-* next to %r)" % dbs)
        r.check(ctx.app.alive(), "the app died after the corrupt-database launch")
        local_cmd_exact(ctx, r, "cd_after")
    finally:
        R.fresh_app(ctx, r, "cd")
