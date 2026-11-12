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
#   ssh_drop_reconnect         ssh dropped by `~.` (after a command, and right after connect with and without an Enter: #1793) and by killing the
#                              remote shell: the local pane works, a reconnect works, exact lines each time
#   crash_kill9_restore        SIGKILL of the app with restore_session on: relaunch comes up, nothing re-runs, the pane is usable and exact
#   crash_corrupt_db           a garbage warp.sqlite: the app starts, moves it aside (warp.sqlite.corrupt-*, #860) and the pane is usable and exact
import glob, os, re, shutil, signal, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402  (cases.py imports this module near its end, so everything below is already defined)
import cases_robust as R  # noqa: E402
import cases_caseremote as CR  # noqa: E402
import cases_caseagent as CA  # noqa: E402
import cases_ui as UI  # noqa: E402
import priv_boundary as PB  # noqa: E402  (T47: who the pane may become without a password, and the exact NEEDS: text)
from cases import case  # noqa: E402


def exact_lines(r, label, exp, got):
    ok = got == exp
    r.check(ok, "%s: expected %r, got %r" % (label, exp, got))
    return ok


def local_cmd_exact(ctx, r, label, absolute=False):
    """one typed line to the LOCAL shell (10-25 ms/key, instant Enter): the exact line must be what rec.log / the effect file shows.
    absolute=True names the effect file by its absolute path: a restored pane starts in its SAVED cwd, not the work dir (r18u13 Mac crash_kill9_restore:
    the shell received `echo TOK >> ca_local_ck_after.txt` exactly and wrote it into ck_dir_2, where the harness did not look)."""
    tok = ctx.tok(6).lower()
    f = "ca_local_%s.txt" % label
    ctx.rm(f)
    mark = ctx.cmd_mark()
    line = ctx.append_cmd(ctx.pane_shell(), tok, R.fwd(os.path.join(ctx.W, f)) if absolute else f)
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


def wait_program_exited(ctx, prompts_before, timeout=10):
    """r18u8 (H14, Windows Git Bash): the file vim writes exists BEFORE vim has left the alt screen, and the next typed line lost its first character to the still-running
    vim (`204_80p7y4 >> ...` for `echo t204_...`). Wait for the shell's prompt to come back (its hook logs it); a pane with no prompt hook gets a fixed settle."""
    if ctx.prof.local_prompts:
        ctx.until(lambda: ctx.prompts() > prompts_before, timeout, step=0.1)
        time.sleep(0.3)
    else:
        time.sleep(2.0)


@case("alt_screen_programs", order=22, gui=True)
def c_alt_programs(ctx, r):
    """Each full-screen program gets keys typed at 10-25 ms and proves it received them; afterwards the shell is back and a typed line is exact.
    vim: typed text saved to a file. less: `!cmd` inside less runs a shell line that writes a token. htop: starts, draws, q returns. tmux: a pane
    inside tmux receives a typed line (rec.log of the inner shell sees it exactly), `exit` ends tmux. A missing program is UNVERIFIABLE, not skipped."""
    # ---- vim
    if ctx.which("vim"):
        ctx.rm("ap_vim.txt")
        tok = ctx.tok(6).lower()
        p0 = ctx.prompts()
        ctx.be.type("vim -u NONE -N ap_vim.txt", ctx.rng.randint(10, 25), enter=True)
        time.sleep(1.5)
        ctx.be.type("i" + tok, ctx.rng.randint(10, 25))
        ctx.shot("ap_vim_insert")
        ctx.be.key("Escape")
        ctx.be.type(":wq", ctx.rng.randint(10, 25), enter=True)
        ctx.until(lambda: ctx.lines("ap_vim.txt"), 8)
        exact_lines(r, "vim received the typed text", [tok], ctx.lines("ap_vim.txt"))
        wait_program_exited(ctx, p0)
        local_cmd_exact(ctx, r, "ap_after_vim")
    else:
        missing(r, "vim", "package vim")
    # ---- less
    if ctx.which("less"):
        ctx.rm("ap_less.txt")
        tok = ctx.tok(6).lower()
        p1 = ctx.prompts()
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
        wait_program_exited(ctx, p1)
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
        before, base = ctx.prompts(), ctx.shell_count()
        ctx.be.type("tmux -f /dev/null new-session -s appci_%s" % tok, ctx.rng.randint(10, 25), enter=True)
        ctx.until(lambda: ctx.prompts() > before, 15, step=0.2)
        time.sleep(1.0)
        ctx.shot("ap_tmux_up")
        line = ctx.append_cmd("bash", tok, "ap_tmux.txt")
        ctx.be.type(line, ctx.rng.randint(10, 25), enter=True)
        ctx.until(lambda: ctx.lines("ap_tmux.txt"), 8)
        exact_lines(r, "the shell inside tmux ran the typed line once", [tok], ctx.lines("ap_tmux.txt"))
        if ctx.prof.commands():
            ctx.await_commands(mark, lambda cs: line in cs)
            r.check(line in ctx.cmds_since(mark), "the shell inside tmux received %r, not %r" % (ctx.cmds_since(mark)[:3], line))
        ctx.exit_nested(base, r, "tmux")                 # the shell inside tmux is not a descendant of the app: the PID probe (root_pid) is what proves it (audit #2)
        ctx.shot("ap_tmux_exited")
        local_cmd_exact(ctx, r, "ap_after_tmux")
    else:
        missing(r, "tmux", "package tmux")


MOUSE_PROG = r'''import sys, os, tty, termios, select, time
fd = sys.stdin.fileno(); old = termios.tcgetattr(fd); tty.setraw(fd)
buf = b""
try:
    # five rows that carry a token the command line does not (the two halves are joined here): the click goes ON one of them, inside this block
    sys.stdout.write("\x1b[?1000h\x1b[?1006h" + ("MOUSEREADY mrdy" + sys.argv[2] + "\r\n") * 5); sys.stdout.flush()
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
    The pointer is xdotool (X11) / the virtual pointer (Wayland).
    #2069: the case never passed (r18u28, both Linux backends: the program got b'k' only). The app reports a click to a running program only when the
    click is ON that program's block (block_list_element.rs: on_long_running_block && !should_intercept_mouse); the case clicked 60 px BELOW a banner
    of one row, or swept fixed fractions of the window. The program now prints five rows carrying a token and the click goes on the token itself (the
    app's find hook, else OCR); the sweep and the window centre remain as fallbacks, and the result names which placement was used and where."""
    if ctx.win or ctx.be.name == "mac":
        r.skip("no pointer synthesis in the %s backend (Linux xdotool / sway seat only); the program itself needs python3 termios" % ctx.be.name)
    if not ctx.which("python3"):
        r.skip("python3 missing on the pane's PATH")
    mouse = UI.mouse_or_skip(ctx, r)
    prog = os.path.join(ctx.W, "ca_mouse.py")
    with open(prog, "w") as f:
        f.write(MOUSE_PROG)
    ctx.rm("ca_mouse.out")
    half = UI.ocr_safe(ctx.tok(6).lower().replace("_", ""))
    tok = "mrdy" + half
    UI.register_find(ctx, tok)
    seen = ctx.log_size()
    ctx.be.type("python3 ca_mouse.py ca_mouse.out %s" % half, ctx.rng.randint(10, 25), enter=True)
    time.sleep(1.5)
    pos = None
    sweep = None
    how = None
    hit = UI._hook_word(ctx, tok, seen)
    if hit:
        how = "the app's find hook"
    elif shutil.which("tesseract"):
        hit, _ = UI.find_token(ctx, "ca_mouse_find", tok, since=seen)
        how = "OCR" if hit else None
    if hit:
        pos = (hit[1], hit[2])
        r.note("click placed ON the program's own output: token %r at (%d, %d), located by %s" % (tok, pos[0], pos[1], how))
    if pos is None and UI.wl_pointer_real(ctx):
        # #1789: OCR-free placement. The window rectangle comes from sway (exact); the banner's row is not known without an app hook, so the click is
        # swept down the left side of the window: only a click inside the program's block is reported to it, any other click does nothing to the test.
        rect = UI.sway_window_rect(ctx.be.pid_fn()) if getattr(ctx.be, "pid_fn", None) and ctx.be.pid_fn() else None
        if rect:
            sweep = [(rect[0] + 140, rect[1] + int(rect[3] * f)) for f in (0.08, 0.14, 0.22, 0.32, 0.45, 0.6)]
            how = "a sweep of fixed window fractions"
            r.note("neither the find hook nor OCR located the program's rows; clicks swept down x=%d over the window %r, which may miss the program's block" % (sweep[0][0], rect))
    if sweep is None and pos is None:
        shot = ctx.shot("ca_mouse_size")
        try:
            from PIL import Image
            w, h = Image.open(shot).size
            pos = (w // 2, h // 2)
            how = "the window centre"
            r.note("neither the find hook nor OCR located the program's rows; clicked the window centre %r, which may not be on the program's block" % (pos,))
        except Exception:
            r.skip("cannot place the pointer: no tesseract hit and no Pillow to read the screenshot size (packages: tesseract-ocr, python3-pil)")
    ctx.shot("ca_mouse_before_click")
    for px, py in (sweep or [pos]):
        mouse.click(px, py)
    time.sleep(0.4)
    ctx.be.type("k", 20)
    ctx.until(lambda: os.path.exists(os.path.join(ctx.W, "ca_mouse.out")), 10)
    time.sleep(0.4)
    ctx.shot("ca_mouse_after")
    got = open(os.path.join(ctx.W, "ca_mouse.out")).read() if os.path.exists(os.path.join(ctx.W, "ca_mouse.out")) else ""
    if UI.on_wayland(ctx) and not UI.wl_pointer_real(ctx) and not re.search(r"\\x1b\[<0;\d+;\d+[Mm]", got):
        # no mouse event of any kind reached the program: the sway seat's synthetic click is unobservable here (NOREF); a press WITHOUT its release, or
        # mangled events, still fail below
        UI.wl_check(ctx, r, False, "no SGR mouse event reached the program that asked for mouse reporting: %s" % got[:200], "pointer")
        local_cmd_exact(ctx, r, "ca_mouse_after")
        return
    placed = "click placed by %s at %r" % (how, sweep or pos)
    r.check(bool(re.search(r"\\x1b\[<0;\d+;\d+M", got)), "no SGR mouse press ESC[<0;x;yM reached the program that asked for mouse reporting: %s (%s)" % (got[:200], placed))
    r.check(bool(re.search(r"\\x1b\[<0;\d+;\d+m", got)), "no SGR mouse release ESC[<0;x;ym reached the program: %s (%s)" % (got[:200], placed))
    r.check(got.endswith("k'") or got.endswith('k"'), "the key typed after the click is not last/intact: %s" % got[-80:])
    local_cmd_exact(ctx, r, "ca_mouse_after")


# ------------------------------------------------------------------ #1536 sudo / su / ksu
def nested_root_lines(ctx, r, label, start_cmd, n, wanted_uid, d=None):
    """type start_cmd, prove the inner shell by `id -u` into a file, type n exact lines into it, exit, and prove the local shell is back.
    `d`: the directory of the inner shell's effect file, one BOTH users can write (priv_boundary.shared_dir); the pane's work dir when None.
    Returns True when the inner shell was the wanted user."""
    f = "ca_%s.txt" % label
    abs_f = os.path.join(d or ctx.W, f)
    try:
        os.remove(abs_f)
    except OSError:
        pass
    base = ctx.shell_count()
    ctx.be.type(start_cmd, ctx.rng.randint(10, 25), enter=True)
    time.sleep(2.5)
    ctx.shot("%s_inner" % label)
    ctx.be.type("id -u >> %s" % abs_f, ctx.rng.randint(10, 25), enter=True)
    ctx.until(lambda: PB.read_lines(abs_f), 10)
    first = PB.read_lines(abs_f)
    inner = r.check(first == [str(wanted_uid)], "%s: the inner shell is not the expected user: `id -u` gave %r, expected %r (the CI user is %d: that uid here means "
                                                "`%s` never started a shell and the line ran in the pane's own)" % (label, first, wanted_uid, os.geteuid(), start_cmd))
    toks = [ctx.tok(8).lower() for _ in range(n)]
    for t in toks:
        ctx.be.type("echo %s >> %s" % (t, abs_f), ctx.rng.randint(10, 25), enter=True)
    ctx.until(lambda: len(PB.read_lines(abs_f)) >= n + 1, 15)
    exact_lines(r, "%s: lines typed into the privileged shell" % label, [str(wanted_uid)] + toks, PB.read_lines(abs_f))
    ctx.exit_nested(base, r, label, timeout=15)          # the privileged shell answers probes in ROOT's home ('nowhere'); the shell count proves it (audit #2)
    ctx.shot("%s_exited" % label)
    uid = str(os.geteuid())
    f2 = "ca_%s_back.txt" % label
    ctx.rm(f2)
    ctx.be.type("id -u >> %s" % os.path.join(ctx.W, f2), ctx.rng.randint(10, 25), enter=True)
    ctx.until(lambda: ctx.lines(f2), 8)
    exact_lines(r, "%s: after exit the LOCAL shell is back (own uid)" % label, [uid], ctx.lines(f2))
    local_cmd_exact(ctx, r, label + "_after")
    return inner


def sudo_shell_command(target):
    """pure: the sudo arm's line: a bash with a clean environment as the target user (root as the case was written; a test user through -u), with
    that user's HOME, no rc files and no profile: the only thing between the keys and the inner bash is sudo."""
    return "sudo %s env -i HOME=%s PATH=\"$PATH\" bash --noprofile --norc -i" % (target["flags"], target["home"])


def su_plan(target, euid, environ, have_su, platform, pw):
    """pure given pw: the su arm -> dict(run, cmd, user, uid, why). su asks no password only when ROOT runs it, so the arm needs the runner to be root
    (`su ...`) or root through passwordless sudo (`sudo -n su ...`: su is still the program that starts the shell and holds the tty in front of it).
    The user is APPCI_SU_USER, else `nobody` on Linux (given a shell with -s; BSD su has no -s and macOS's nobody has no shell, so macOS needs
    APPCI_SU_USER). A sudo target that is only a test user cannot run su without a password: not run, with the reason."""
    no = dict(run=False, cmd=None, user=None, uid=None)
    if not have_su:
        return dict(no, why="su is not installed")
    rooted = euid == 0 or (target["ok"] and target["root"])
    if not rooted:
        return dict(no, why="su asks for a password unless root runs it: needs the runner to be root or passwordless ROOT sudo (euid %d; sudo target: %s)" % (
            euid, target["user"] if target["ok"] else "none"))
    mac = platform == "darwin"
    user = (environ.get(PB.SU_USER_ENV) or "").strip() or (None if mac else "nobody")
    if not user:
        return dict(no, why="macOS su has no -s and its `nobody` has no shell: set %s to a user with a login shell" % PB.SU_USER_ENV)
    try:
        uid = pw(user)[0]
    except KeyError:
        return dict(no, why="su: no user %r on this machine" % user)
    if uid == euid:
        return dict(no, why="su: %r is the runner's own user (uid %d): no boundary to cross" % (user, uid))
    su = ("su %s" % user) if (mac or environ.get(PB.SU_USER_ENV)) else ("su -s /bin/bash %s" % user)
    return dict(run=True, cmd=su if euid == 0 else "sudo -n " + su, user=user, uid=uid, why="")


def priv_arms_verdict(ran, passed, not_run, needs):
    """pure: the rule of sudo_su_real_shell (owner 2026-11-11: "notes OK but at least one arm must run and pass"). ran: the arms that started; passed:
    those of them with no failed check; not_run: "<arm>: <reason>" texts; needs: the machine change (PB.sudo_needs(), starts with `NEEDS: `).
    -> ("skip", text) when no arm ran: the text STARTS with the NEEDS: change, so the row and the tier list it as a machine change and nothing reads
       it as coverage (never a pass); ("fail", text) when arms ran and none passed; ("pass", None) when at least one arm ran and passed. An arm that
       ran and failed has its own failed checks on the row, so a pass here does not hide it."""
    if not ran:
        return "skip", ("%s. No privilege boundary could run here: passwordless sudo and root-for-su are both absent (%s); the su half also needs the runner to be root or "
                        "passwordless ROOT sudo, the ksu half a Kerberos realm. Has never run on any machine" % (needs, "; ".join(not_run[:2])))
    if not [a for a in passed if a in ran]:
        return "fail", "no privilege-boundary arm passed: %s ran and every one of them failed (not run: %s)" % (", ".join(ran), "; ".join(not_run) or "none")
    return "pass", None


@case("sudo_su_real_shell", modes=("terminal",), order=65, gui=True)
def c_sudo_su(ctx, r):
    """The real privilege boundary the nested-bash stand-in of remote_session_* only imitates (#1536). Three arms, each needing something of the
    machine (priv_boundary.py; T47):
      sudo  a user the pane may become with no password: the test user named by APPCI_SUDO_USER (`sudo -n -u <user>`), else root (`sudo -n true`);
      su    root runs su (the runner is root, or root through passwordless sudo: `sudo -n su ...`);
      ksu   a Kerberos realm and a target principal, which are the owner's site: never run by the harness.
    Proof of the boundary is `id -u` typed INTO the inner shell (the target's uid, never the CI user's and never the prompt), then exact lines, then
    `exit` and the pane's own uid again. An arm the machine cannot run is listed as `NOT RUN:` with its reason and in `boundaries_not_run`.
    The verdict (priv_arms_verdict; owner 2026-11-11): the case PASSES only if at least one arm actually ran and passed (metrics `boundaries_run`,
    `boundaries_passed`); no arm run = SKIP whose text starts with the `NEEDS:` machine change, never a pass.
    Round 15: until then every arm that could not run made the row NOREF, and ksu can never run, so the case could not PASS on any machine."""
    if ctx.win or os.name == "nt":
        r.skip("sudo/su/ksu do not exist in a Windows pane")
    ran, passed, not_run = [], [], []

    def arm(name, user, cmd, n, uid):
        d, cleanup = PB.shared_dir(ctx.W, uid, name)
        tag, before = "%s:%s" % (name, user), len(r.fails)
        try:
            ran.append(tag)
            # the inner `id -u` is the proof the boundary was crossed: an arm whose shell never started has not passed, whatever else matched
            if nested_root_lines(ctx, r, name, cmd, n, uid, d) and len(r.fails) == before:
                passed.append(tag)
        finally:
            cleanup()
    t = PB.sudo_target()
    if t["ok"]:
        arm("sudo", t["user"], sudo_shell_command(t), ctx.count(8), t["uid"])
    else:
        not_run.append("sudo: %s" % t["why"])
    import pwd
    plan = su_plan(t, os.geteuid(), os.environ, bool(shutil.which("su") and shutil.which("id")), sys.platform, lambda u: (pwd.getpwnam(u).pw_uid,))
    if plan["run"]:
        arm("su", plan["user"], plan["cmd"], ctx.count(6), plan["uid"])
    else:
        not_run.append("su: %s" % plan["why"])
    if shutil.which("ksu") and subprocess.run(["klist", "-s"], capture_output=True).returncode == 0:
        not_run.append("ksu: present with a valid ticket, but the target principal is site-specific; run it by hand (owner's realm)")
    else:
        not_run.append("ksu: not installed or no Kerberos ticket (package krb5-user + kinit); the realm is the owner's site, never the harness's")
    r.metric("boundaries_run", ran)
    r.metric("boundaries_passed", passed)
    r.metric("boundaries_not_run", [x.split(":", 1)[0] for x in not_run])
    for x in not_run:
        r.note("NOT RUN: " + x)
    verdict, text = priv_arms_verdict(ran, passed, not_run, PB.sudo_needs())
    if verdict == "skip":
        r.skip(text)
    r.check(verdict != "fail", text)


# ------------------------------------------------------------------ #1535 ssh drop + reconnect
def remote_lines_exact(ctx, r, label, n):
    f = "ca_%s_r.txt" % label
    CR.rm_remote(ctx, f)
    toks = [ctx.tok(8).lower() for _ in range(n)]
    arm = getattr(ctx, "arm", None)
    if arm:     # a lab arm: the remote login shell may be nu/fish (managed since r24), so the line is built in its own syntax (cases_lab.append_line)
        import cases_lab as CLB
        CR.typed_lines(ctx, [CLB.append_line(arm, t, "~/" + f) for t in toks])
    else:
        CR.typed_lines(ctx, ["echo %s >> ~/%s" % (t, f) for t in toks])
    ctx.until(lambda: len(ctx.lines(f, remote=True)) >= n, 15)
    exact_lines(r, "%s: lines typed into the remote shell" % label, toks, ctx.lines(f, remote=True))


@case("ssh_drop_reconnect", modes=("terminal",), order=66, gui=True)
def c_ssh_drop(ctx, r):
    """Two ways to lose a live ssh session, then a reconnect (#1535 'no disconnect/reconnect'): (a) the client escape `<Enter>~.`, (b) the remote shell
    killed from inside (`kill -9 $$`, the connection ends like a dropped link). After each: the pane's LOCAL shell works and is exact (a line typed right
    behind the drop must not be lost or run remotely), a new `ssh fake` gives a remote prompt, and lines typed there are exact.
    #1793: two more escape arms at the FIRST remote prompt, right after connect: `<Enter>~.` at once, and `~.` with no Enter (the app puts the `\\r`
    ssh needs in front of the escape when the remote line is proven empty). Each must give a local prompt within 20 s and an exact local line; they
    are their own reconnect, so no second `ssh fake` follows them."""
    CR.need_ssh(ctx, r)
    if ctx.win:
        r.skip("not yet ported to the Windows panes: the `~.` escape and kill -9 $$ need the msys ssh client (unverified there)")
    ssh_drop_body(ctx, r, "fake", ("escape", "escape_connect_enter", "escape_connect_bare", "kill"))


def ssh_drop_body(ctx, r, host, variants):
    """the arms of ssh_drop_reconnect against ssh host alias `host` (the lab cases, cases_lab.py, run the same body against lab-<user>). Variants: the four above,
    plus 'rst' (lab only: the lab server resets the live connection, as a dropped link does; ctx.arm names the user)."""
    for variant in variants:
        escape = variant.startswith("escape")
        at_connect = variant.startswith("escape_connect")
        if escape and os.environ.get("APPCI_SSH_ESCAPE_THROUGH", "1") == "0":
            # #1638 (owner, 2026-11-04): a managed ssh session passes `~X` typed at the start of an empty line to ssh, as a plain terminal does, so the
            # escape arm runs by default. APPCI_SSH_ESCAPE_THROUGH=0 is the opt-out (a build from before #1638): the arm is then UNVERIFIABLE (noref).
            r.unverifiable("ssh_drop_reconnect [%s]: skipped by APPCI_SSH_ESCAPE_THROUGH=0 (a build without #1638's ssh escape pass-through)" % variant)
            continue
        secs = CR.connect(ctx, host, 90)
        r.check(secs is not None, "[%s] no remote prompt after `ssh %s`. Test host: %s" % (variant, host, ctx.ssh_diag() if secs is None else ""))
        if secs is None:
            ctx.reset()
            return
        if not at_connect:
            time.sleep(1.0)
            remote_lines_exact(ctx, r, variant + "_1", ctx.count(5))
        ctx.shot("ssh_drop_%s_connected" % variant)
        before = ctx.prompts()
        off = ctx.log_size()
        if variant == "escape":
            ctx.be.key("Return")
            time.sleep(0.3)
            ctx.be.type("~.", 40)
        elif variant == "escape_connect_enter":
            # #1793: at the first remote prompt, at once: the bare Enter and the escape right behind it.
            ctx.be.key("Return")
            ctx.be.type("~.", 40)
        elif variant == "escape_connect_bare":
            # #1793: at the first remote prompt, `~.` with no Enter before it.
            ctx.be.type("~.", 40)
        elif variant == "rst":
            ctx.lab.drop(ctx.arm["user"], "rst")
        else:
            arm = getattr(ctx, "arm", None)
            if arm:
                import cases_lab as CLB
                ctx.be.type(CLB.kill_self_line(arm), 15, enter=True)
            else:
                ctx.be.type("kill -9 $$", 15, enter=True)
        up = ctx.until(lambda: ctx.prompts() > before, 20, step=0.1)
        r.check(bool(up), "[%s] no local prompt within 20 s after the connection was lost" % variant)
        ctx.shot("ssh_drop_%s_dropped" % variant)
        if not up:
            # r27: ANY arm whose drop did not end the session (the kill line was rejected by the shell, the reset did nothing) stops here: typing the
            # next lines "locally" would type them into the live remote shell, and the later arms would read as that shell's failures.
            # r18u22 (Wayland): the escape left the session up and every later step of BOTH arms ran inside it (the reconnect nested a second
            # ssh, the kill arm's `kill -9 $$` returned to the first remote shell), so one failure read as nine. What the app did with `~.` is
            # quoted from its log (hop=ssh_escape_to_pty: written to ssh; hop=ssh_escape_refused why=...: kept in the editor and why), then the
            # line is cleared and the session left with `exit`, so the kill arm starts from the local shell and is judged on its own.
            # #1793: hop=ssh_escape_cr says whether the `\r` went in front of the escape (added) or why not (skipped=<reason>).
            esc = [l.strip()[-160:] for l in ctx.log_since(off).splitlines() if "ssh_escape" in l]
            if escape:
                r.note("[%s] app log after the escape: %s" % (variant, esc[:6] or "no ssh_escape hop (a build without the r18u22 refusal log, the input_order log off, or the keys never reached the editor)"))
                r.metric("%s_hops" % variant, esc[:6])
            else:
                r.note("[%s] proven: no new local prompt in 20 s, so the remote session is still up (the drop line did not end it); nothing more is typed as a local command" % variant)
            ctx.reset()
            left = CR.leave(ctx)
            r.note("[%s] the rest of this arm is not run (it would nest inside the live session); `exit` back to the local shell: %s" % (
                variant, "ok" if left else "FAILED (%s)" % ctx.left_unproven))
            if not left:
                r.check(False, "[%s] the session the escape did not end could not be left with `exit` either (%s): the later arms are not run" % (
                    variant, ctx.left_unproven))
                return
            continue
        where = ctx.where_now()
        r.check(where == "local", "[%s] after the drop the pane is %r, expected local" % (variant, where))
        if where != "local":
            # r27: a prompt came back but the pane is not local: never type the local-command lines blind; reset and leave the session
            ctx.reset()
            left = CR.leave(ctx)
            r.note("[%s] pane proven %r, not local; the rest of this arm is not run; `exit` back to the local shell: %s" % (variant, where, "ok" if left else "FAILED (%s)" % ctx.left_unproven))
            if not left:
                return
            continue
        local_cmd_exact(ctx, r, "ssh_drop_%s" % variant)
        if at_connect:
            # The arm itself began with a fresh connect: that was its reconnect.
            continue
        secs = CR.connect(ctx, host, 90)
        r.check(secs is not None, "[%s] reconnect: no remote prompt after `ssh %s`" % (variant, host))
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
        if ctx.win and not ctx.prof.local_prompts:
            # r18u8 (H10): the PowerShell pane has no prompt hook, so prompts() never moves there; start_app already proved the shell by the window + fixed settle + key proof
            r.note("no prompt hook in this pane (PowerShell): `a shell started` rests on the window, the key proof and the exact typing below, not on a prompt count")
        else:
            r.check(newp >= 1, "no shell started after the crash (prompts +%d)" % newp)
        local_cmd_exact(ctx, r, "ck_after", absolute=True)
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
        local_cmd_exact(ctx, r, "cd_after", absolute=True)
    finally:
        R.fresh_app(ctx, r, "cd")
