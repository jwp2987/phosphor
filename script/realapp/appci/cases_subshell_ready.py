# appci/cases_subshell_ready.py: the subshell READINESS lab (#1679 / #1711 step 3 and 4; the redesign's and the reviewer's lab cases).
# Registered with the same @case decorator as cases.py (one import line at the end of cases.py). Every case drives a REAL shell
# in the pane, Phosphorizes it (Ctrl-I at the banner) and proves from effect files and the app log's hops that the setup was
# written only on the shell's proof (`subshell_setup_ready_write`), never on a prompt shape, a timer or an Enter, and that what the
# user typed ran exactly once, in the right shell. The hops are the ones view.rs logs: `subshell_setup_armed`,
# `subshell_ready_judged source= decision= kind= leader= comm= tail=`, `subshell_ready_line_end verdict=`,
# `subshell_setup_ready_write`, `subshell_written_judged verdict=`, `subshell_setup_cancelled why= dest=`.
#
# Facilities the cases need that a runner may lack (vim, tcsh, fish, zsh, a passwordless sudo, a usable docker with the alpine image)
# SKIP with the reason, never fail. No case pulls an image, installs a package or needs root beyond `sudo -n true` succeeding.
# POSIX only: the banner regexes the cases rely on are added by the profile (profile.PROBE_SUBSHELL_COMMANDS).
import functools, os, shutil, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402
from cases import case  # noqa: E402

BANNER_S = 1.6     # the Phosphorize banner comes 1 s after the subshell command starts; the step-1 case waits this long too


def need_posix(ctx, r):
    if ctx.win or os.name == "nt":
        r.skip("POSIX shells only: the readiness lab drives bash/zsh/fish/tcsh rc files and a real pty")


def write_rc(ctx, name, text):
    """an rc file under the pane's ~/w (ctx.W); returns the path the typed command uses (~ is the scratch HOME)"""
    with open(os.path.join(ctx.W, name), "w") as f:
        f.write(text)
    return "~/w/" + name


def hops(ctx, off):
    """the readiness hops logged since `off`, in order, as (hop, rest-of-line)"""
    out = []
    for ln in ctx.log_since(off).splitlines():
        for h in ("subshell_setup_armed", "subshell_ready_judged", "subshell_ready_line_end", "subshell_setup_ready_write",
                  "subshell_written_judged", "subshell_setup_cancelled", "subshell_setup_unsupported"):
            if h in ln:
                out.append((h, ln))
                break
    return out


def first_index(hs, name, contains=None):
    for i, (h, ln) in enumerate(hs):
        if h == name and (contains is None or contains in ln):
            return i
    return None


def run_in_inner(ctx, r, label, shell, f, timeout=10):
    """types `echo tok >> f` for the inner shell and waits for it; returns (tok, seconds after Enter or None)"""
    tok = ctx.tok(6)
    ctx.be.type(ctx.append_cmd(shell, tok, f), 12, enter=True)
    t0 = time.time()
    got = ctx.until(lambda: tok in ctx.lines(f), timeout)
    dt = (time.time() - t0) if got else None
    r.check(got, "%s: the line typed into the inner shell ran (file %r)" % (label, ctx.lines(f)))
    return tok, dt


def leave_inner(ctx, r, label):
    """Leave every LOCAL inner shell the case started and PROVE the shell in front is the pane's ROOT shell by its PID (a plain `exit` + a
    command proves nothing: the same command runs in the inner shell when the `exit` was eaten -- the Mac round-6 leak). `exit` is typed only
    while a probe proves a nested shell is in front. Without a recorded root pid / PID probe it falls back to one `exit` (and says so)."""
    ctx.left_inner = True
    state = ctx.leave_nested(getattr(ctx, "root_pid", None))
    if state == "unprobed":
        r.note("%s: no root shell pid recorded or no PID probe in this pane; left the inner shell with one `exit`, unproven" % label)
        ctx.be.type("exit", 12, enter=True)
        time.sleep(1.0)
        return
    r.check(state == "root", "%s: the shell in front after leaving is the pane's ROOT shell (got %s)" % (label, state))


def guarded(fn):
    """every readiness case leaves its inner shell(s) in a `finally` and proves it is back in the PARENT shell, whether it passed, failed or
    skipped (the run.py depth check is the second layer)"""
    @functools.wraps(fn)
    def run(ctx, r):
        ctx.left_inner = False
        try:
            return fn(ctx, r)
        finally:
            if not ctx.left_inner:
                try:
                    leave_inner(ctx, r, fn.__name__)
                except cases.Skip:
                    raise
                except Exception as e:       # never mask the case's own exception
                    r.note("%s: leaving the inner shell raised %r" % (fn.__name__, e))
    return run


def leave_and_prove_parent(ctx, r, label, f, want_lines):
    """leave the inner shell (proved by PID), then a line in the pane's bash: the file ends with exactly the expected lines plus the parent's"""
    leave_inner(ctx, r, label)
    back = "back_" + ctx.tok(4)
    ctx.be.type(ctx.append_cmd("bash", back, f), 12, enter=True)
    ctx.until(lambda: back in ctx.lines(f), 8)
    l = ctx.lines(f)
    r.check(l == want_lines + [back], "%s: exact lines in the inner shell then the pane's bash: got %r, wanted %r" % (label, l, want_lines + [back]))


@case("subshell_ready_rc_questions", order=22)
@guarded
def c_ready_rc_questions(ctx, r):
    """the redesign's "rc read -p x2" failure mode: a bash whose rc asks two questions, Phosphorized while it asks. Each answer is a
    non-empty line typed at an InputPrompt tail through a cooked shell keeping up with its reads: a ReArm (hop verdict=ReArm), never a
    write, never a cancel; the setup goes out at the prompt that follows (one `subshell_setup_ready_write`, AFTER both answers), the
    rc recorded the answers, the next line runs in the inner bash."""
    need_posix(ctx, r)
    f, ans = "lab_rcq.txt", "lab_rcq_answers.txt"
    ctx.rm(f, ans)
    rc = write_rc(ctx, "lab_rcq.rc", 'read -p "Name: " a; read -p "Team: " b; echo "$a $b" > ~/w/%s; PS1="LAB$ "\n' % ans)
    off = ctx.log_size()
    ctx.be.type("bash --rcfile " + rc, 12, enter=True)
    time.sleep(BANNER_S)
    ctx.shot("rcq_banner")
    ctx.be.key("ctrl+i")
    time.sleep(0.4)
    ctx.be.type("me", 12, enter=True)
    time.sleep(0.4)
    ctx.be.type("ops", 12, enter=True)
    r.check(ctx.until(lambda: ctx.lines(ans) == ["me ops"], 8), "both answers reached the rc's reads: %r" % ctx.lines(ans))
    tok, dt = run_in_inner(ctx, r, "rcq", "bash", f)
    if dt is not None:
        r.metric("rcq_inner_line_after_enter_s", round(dt, 3))
    hs = hops(ctx, off)
    rearms = [i for i, (h, ln) in enumerate(hs) if h == "subshell_ready_line_end" and "verdict=ReArm" in ln]
    write = first_index(hs, "subshell_setup_ready_write")
    r.check(len(rearms) >= 2, "two answers re-armed the setup (hops verdict=ReArm): %d" % len(rearms))
    r.check(write is not None, "the setup was written (hop subshell_setup_ready_write)")
    if write is not None and rearms:
        r.check(write > rearms[-1], "the write came AFTER the last answer, never between the questions")
    r.check(first_index(hs, "subshell_setup_cancelled") is None, "no cancel: the answers were ReArms, not UserDrove")
    leave_and_prove_parent(ctx, r, "rcq", f, [tok])


@case("subshell_ready_vim_typeahead", order=22)
@guarded
def c_ready_vim_typeahead(ctx, r):
    """the redesign's `vim f⏎` typeahead: a bash whose rc sleeps, Phosphorized during the sleep, `vim ...⏎` typed at once (typeahead into
    the cooked rc: a ReArm), vim then owns the alt screen (Wait(AltScreen) / an Unknown leader: nothing written), `:wq` back to the
    prompt: the setup is written then and the next line runs in the inner bash. The vim file holds exactly what was typed."""
    need_posix(ctx, r)
    if not shutil.which("vim"):
        r.skip("vim is not installed")
    f, vf = "lab_vim.txt", "lab_vim_edit.txt"
    ctx.rm(f, vf)
    rc = write_rc(ctx, "lab_vim.rc", 'sleep 3; PS1="LAB$ "\n')
    off = ctx.log_size()
    ctx.be.type("bash --rcfile " + rc, 12, enter=True)
    time.sleep(BANNER_S)
    ctx.be.key("ctrl+i")
    ctx.be.type("vim -u NONE ~/w/" + vf, 12, enter=True)
    time.sleep(3.5)                      # the rc ends, bash starts vim with the typeahead
    ctx.shot("vim_open")
    ctx.be.type("ihello", 15)
    ctx.be.key("Escape")
    ctx.be.type(":wq", 15, enter=True)
    time.sleep(1.0)
    tok, _ = run_in_inner(ctx, r, "vim", "bash", f)
    r.check(ctx.lines(vf) == ["hello"], "vim received exactly the typed text: %r" % ctx.lines(vf))
    hs = hops(ctx, off)
    write = first_index(hs, "subshell_setup_ready_write")
    alt = first_index(hs, "subshell_ready_judged", "AltScreen")
    unknown = first_index(hs, "subshell_ready_judged", "kind=Unknown")
    r.check(write is not None, "the setup was written after vim (hop subshell_setup_ready_write)")
    r.check(alt is not None or unknown is not None, "while vim ran the judge waited (AltScreen or an Unknown leader in the hops)")
    if write is not None:
        before = [ln for h, ln in hs[:write] if h == "subshell_ready_judged" and "decision=Write" in ln]
        r.check(not before, "no Write decision before the one that wrote: %r" % before[:2])
    leave_and_prove_parent(ctx, r, "vim", f, [tok])


@case("subshell_ready_unsupported_tcsh", order=22)
@guarded
def c_ready_unsupported_tcsh(ctx, r):
    """review R5: a known-unsupported leader (tcsh) is given up at once: no probe line is ever written into tcsh (it would print
    errors), the hop is `subshell_setup_cancelled why=Unsupported`, and the typed line runs in tcsh untouched."""
    need_posix(ctx, r)
    if not shutil.which("tcsh"):
        r.skip("tcsh is not installed")
    f = "lab_tcsh.txt"
    ctx.rm(f)
    off = ctx.log_size()
    ctx.be.type("tcsh -f", 12, enter=True)
    time.sleep(BANNER_S)
    ctx.be.key("ctrl+i")
    time.sleep(0.8)
    tok, _ = run_in_inner(ctx, r, "tcsh", "bash", f)
    hs = hops(ctx, off)
    r.check(first_index(hs, "subshell_setup_cancelled", "why=Unsupported") is not None, "tcsh was given up as unsupported by the leader list")
    r.check(first_index(hs, "subshell_setup_ready_write") is None, "nothing was written into tcsh")
    leave_and_prove_parent(ctx, r, "tcsh", f, [tok])


@case("subshell_ready_secret_after_prompt", order=22)
@guarded
def c_ready_secret_after_prompt(ctx, r):
    """E-SECRET after the first line: a bash whose PROMPT_COMMAND runs ONE `read -s` before its SECOND prompt, i.e. right after
    our first setup line was read and ran (the marker is progress, the secret read still wins). The line typed while the setup was
    written is handed back to the editor (hop why=Secret dest=Editor), the password typed next goes to the read (recorded by the
    rc, not run as a command), the setup is written again (a fresh attempt) and completes; the handed-back line is not run until
    the user presses Enter on it (the first Enter is the guard's)."""
    need_posix(ctx, r)
    f, pw = "lab_secret.txt", "lab_secret_pw.txt"
    ctx.rm(f, pw)
    rc = write_rc(ctx, "lab_secret.rc",
                  'n=0; PROMPT_COMMAND=\'if [ "$n" = 1 ]; then read -s -p "Secret: " x; echo "$x" > ~/w/%s; echo; fi; n=$((n+1))\'; PS1="LAB$ "\n' % pw)
    off = ctx.log_size()
    ctx.be.type("bash --rcfile " + rc, 12, enter=True)
    time.sleep(BANNER_S)
    ctx.be.key("ctrl+i")
    tok = ctx.tok(6)
    ctx.be.type(ctx.append_cmd("bash", tok, f), 12)          # typed while the first line is out: held, then handed back
    r.check(ctx.until(lambda: first_index(hops(ctx, off), "subshell_setup_cancelled", "why=Secret") is not None, 8),
            "the secret read after our first line cancelled the attempt (hop why=Secret)")
    ctx.be.type("hunter2", 12, enter=True)                   # the password: passes through to the read -s
    r.check(ctx.until(lambda: ctx.lines(pw) == ["hunter2"], 8), "the password reached the read -s, not a command: %r" % ctx.lines(pw))
    r.check(ctx.until(lambda: len([1 for h, _ in hops(ctx, off) if h == "subshell_setup_ready_write"]) >= 2, 10),
            "the setup was written again after the secret was answered")
    time.sleep(1.5)
    r.check(tok not in ctx.lines(f), "the handed-back line was NOT run on its own: %r" % ctx.lines(f))
    hs = hops(ctx, off)
    cancel = [ln for h, ln in hs if h == "subshell_setup_cancelled"]
    r.check(any("dest=Editor" in ln for ln in cancel), "the held line went to the editor, not the pty: %r" % cancel[:2])
    # The handed-back line sits in the editor under the Enter guard: the first Enter is consumed, the second runs it.
    ctx.be.key("Return")
    time.sleep(0.5)
    if tok not in ctx.lines(f):
        ctx.be.key("Return")
    r.check(ctx.until(lambda: ctx.lines(f) == [tok], 8), "the handed-back line ran once when the user pressed Enter on it: %r" % ctx.lines(f))
    leave_and_prove_parent(ctx, r, "secret", f, [tok])


@case("subshell_ready_relay_shape", order=22)
@guarded
def c_ready_relay_shape(ctx, r):
    """the relay shape (rows 9/10, R4): `sudo -s` with use_pty stays the foreground leader, so only the first new prompt behind it
    (with the local tty raw) says the shell is ready. Needs a passwordless sudo on the runner; skips otherwise."""
    need_posix(ctx, r)
    if not shutil.which("sudo") or subprocess.run(["sudo", "-n", "true"], capture_output=True).returncode != 0:
        r.skip("no passwordless sudo on this runner (sudo -n true failed)")
    f = "lab_relay.txt"
    ctx.rm(f)
    off = ctx.log_size()
    ctx.be.type("sudo -s", 12, enter=True)
    time.sleep(BANNER_S)
    ctx.be.key("ctrl+i")
    r.check(ctx.until(lambda: first_index(hops(ctx, off), "subshell_setup_ready_write") is not None, 10), "the setup was written behind sudo")
    time.sleep(1.5)
    tok = ctx.tok(6)
    ctx.be.type("sh -c 'id -u; echo %s' >> ~/w/%s" % (tok, f), 12, enter=True)
    r.check(ctx.until(lambda: tok in ctx.lines(f), 10), "the typed line ran in the root shell: %r" % ctx.lines(f))
    hs = hops(ctx, off)
    judged = [ln for h, ln in hs if h == "subshell_ready_judged"]
    r.check(any("kind=Relay" in ln for ln in judged) or any("leader=Shell" in ln for ln in judged),
            "the samples named sudo a relay (use_pty) or the root shell a shell leader: %r" % judged[-1:])
    l = ctx.lines(f)
    r.check(l[:2] == ["0", tok], "the line ran as root, once: %r" % l)
    leave_inner(ctx, r, "relay_or_fish")
    back = "back_" + ctx.tok(4)
    ctx.be.type(ctx.append_cmd("bash", back, f), 12, enter=True)
    r.check(ctx.until(lambda: back in ctx.lines(f), 8), "the pane's bash runs the next command")


@case("subshell_ready_exec_fish_rc", order=22)
@guarded
def c_ready_exec_fish_rc(ctx, r):
    """reviewer R5: `exec fish` in the rc of a `bash --rcfile`. The leader in front when the setup is ready is fish, not the bash the
    command named: the probe path is taken (comm=fish in the judged hop) and the line runs in fish."""
    need_posix(ctx, r)
    if not shutil.which("fish"):
        r.skip("fish is not installed")
    f = "lab_execfish.txt"
    ctx.rm(f)
    rc = write_rc(ctx, "lab_execfish.rc", "exec fish\n")
    off = ctx.log_size()
    ctx.be.type("bash --rcfile " + rc, 12, enter=True)
    time.sleep(BANNER_S)
    ctx.be.key("ctrl+i")
    r.check(ctx.until(lambda: first_index(hops(ctx, off), "subshell_setup_ready_write") is not None, 10), "the setup was written into the exec'd fish")
    time.sleep(2.0)
    tok, _ = run_in_inner(ctx, r, "execfish", "bash", f)
    ctx.be.type("sh -c 'ps -o comm= -p $PPID' >> ~/w/" + f, 12, enter=True)
    ctx.until(lambda: len(ctx.lines(f)) >= 2, 6)
    hs = hops(ctx, off)
    r.check(any(h == "subshell_ready_judged" and "comm=\"fish\"" in ln and "decision=Write" in ln for h, ln in hs),
            "the judge wrote on a fish leader (the exec'd shell), not on bash: %r" % [ln for h, ln in hs if "decision=Write" in ln][:1])
    l = ctx.lines(f)
    r.check(len(l) == 2 and l[0] == tok and l[1].startswith("fish"), "the line ran in fish: %r" % l)
    leave_inner(ctx, r, "relay_or_fish")
    back = "back_" + ctx.tok(4)
    ctx.be.type(ctx.append_cmd("bash", back, f), 12, enter=True)
    r.check(ctx.until(lambda: back in ctx.lines(f), 8), "the pane's bash runs the next command")


@case("subshell_ready_docker_t_without_i", order=22)
@guarded
def c_ready_docker_t_without_i(ctx, r):
    """reviewer R4: `docker run -t` WITHOUT `-i` shows a prompt shape behind a relay while the LOCAL tty stays cooked: the keys go to
    the parent shell, so nothing may be written (the judge waits: RelayCooked). The line typed meanwhile runs in the PARENT bash
    exactly once after docker exits. Needs a usable docker with the alpine image already present (never pulled); skips otherwise."""
    need_posix(ctx, r)
    if not shutil.which("docker") or subprocess.run(["docker", "image", "inspect", "alpine"], capture_output=True).returncode != 0:
        r.skip("docker with a local alpine image is not available (never pulled by the harness)")
    f = "lab_docker.txt"
    ctx.rm(f)
    off = ctx.log_size()
    ctx.be.type("docker run --rm -t alpine sh -c 'printf \"/ # \"; sleep 4'", 12, enter=True)
    time.sleep(BANNER_S)
    ctx.be.key("ctrl+i")
    time.sleep(0.5)
    tok = ctx.tok(6)
    ctx.be.type(ctx.append_cmd("bash", tok, f), 12, enter=True)   # typed into the parent (docker has no stdin): runs after docker exits
    r.check(ctx.until(lambda: tok in ctx.lines(f), 12), "the line typed during docker -t ran in the parent bash: %r" % ctx.lines(f))
    hs = hops(ctx, off)
    r.check(first_index(hs, "subshell_setup_ready_write") is None, "nothing was written behind a relay with a cooked local tty")
    r.check(any(h == "subshell_ready_judged" and ("RelayCooked" in ln or "kind=Unknown" in ln) for h, ln in hs),
            "the judge waited on the cooked relay (RelayCooked) or an unknown leader: %r" % [ln for h, ln in hs if h == "subshell_ready_judged"][-1:])
    r.check(ctx.lines(f) == [tok], "the line ran exactly once: %r" % ctx.lines(f))


def zsh_runs_newuser_install(home, wait=4.0):
    """True when an interactive zsh with the (empty) HOME `home` shows the zsh-newuser-install menu on a real pty. Judged by the menu text;
    the zsh is killed and the pty closed, nothing is typed into it (a key would pick an option)."""
    import pty, select, signal
    pid, fd = pty.fork()
    if pid == 0:
        os.environ["HOME"] = home
        try:
            os.execvp("zsh", ["zsh", "-i"])
        finally:
            os._exit(127)
    seen, end = b"", time.time() + wait
    try:
        while time.time() < end and b"newuser" not in seen:
            if select.select([fd], [], [], 0.2)[0]:
                try:
                    chunk = os.read(fd, 4096)
                except OSError:
                    break
                if not chunk:
                    break
                seen += chunk
    finally:
        try:
            os.kill(pid, signal.SIGKILL)
        except OSError:
            pass
        try:
            os.waitpid(pid, 0)
        except OSError:
            pass
        os.close(fd)
        shutil.rmtree(home, ignore_errors=True)
        os.makedirs(home, exist_ok=True)
    return b"newuser" in seen


@case("subshell_ready_zsh_newuser_install", order=22)
@guarded
def c_ready_zsh_newuser_install(ctx, r):
    """reviewer R6: a zsh with an EMPTY home runs zsh-newuser-install, whose menu reads ONE key: a setup written there picks an option
    (the probe's `2` of `2>&1` would populate ~/.zshrc). The footer counts as an InputPrompt: nothing is written until the user
    answers `q`, no ~/.zshrc appears, and the setup is written at the zle prompt that follows; the typed line runs in zsh."""
    need_posix(ctx, r)
    if not shutil.which("zsh"):
        r.skip("zsh is not installed")
    home = os.path.join(ctx.W, "lab_zsh_home")
    shutil.rmtree(home, ignore_errors=True)
    os.makedirs(home)
    if not zsh_runs_newuser_install(home):
        # Mac round 7: Apple's zsh starts at a plain prompt with an empty HOME (no menu), so the setup is rightly written at once and the
        # `q` this case types for the menu ran as a command ("qecho ...: command not found"). No menu = nothing to test here.
        r.skip("this zsh runs no zsh-newuser-install with an empty HOME (proved on a pty before the case types anything)")
    f = "lab_zshnew.txt"
    ctx.rm(f)
    off = ctx.log_size()
    ctx.be.type("HOME=~/w/lab_zsh_home zsh", 12, enter=True)
    time.sleep(BANNER_S)
    ctx.shot("zsh_newuser_menu")
    ctx.be.key("ctrl+i")
    time.sleep(1.0)
    r.check(first_index(hops(ctx, off), "subshell_setup_ready_write") is None, "nothing was written into the newuser-install menu")
    r.check(not os.path.exists(os.path.join(home, ".zshrc")), "no ~/.zshrc was created by a stray setup byte")
    ctx.be.type("q", 15)                                      # (q) Quit and do nothing
    r.check(ctx.until(lambda: first_index(hops(ctx, off), "subshell_setup_ready_write") is not None, 10), "the setup was written at the zle prompt after the menu")
    time.sleep(1.5)
    tok, _ = run_in_inner(ctx, r, "zshnew", "bash", f)
    r.check(not os.path.exists(os.path.join(home, ".zshrc")), "still no ~/.zshrc: the menu got only the user's `q`")
    hs = hops(ctx, off)
    r.check(any(h == "subshell_ready_judged" and "InputPrompt" in ln for h, ln in hs), "the menu footer read as an InputPrompt in at least one sample")
    leave_and_prove_parent(ctx, r, "zshnew", f, [tok])


@case("subshell_ready_rc_clear", order=22)
@guarded
def c_ready_rc_clear(ctx, r):
    """reviewer: an rc that runs `clear` moves the cursor to the top: the shape baselines are wrong-footed, the OS proof is not. The
    setup is written when readline reads, the first inner line runs < 2 s after its Enter."""
    need_posix(ctx, r)
    f = "lab_clear.txt"
    ctx.rm(f)
    rc = write_rc(ctx, "lab_clear.rc", 'sleep 1; clear; PS1="LAB$ "\n')
    off = ctx.log_size()
    ctx.be.type("bash --rcfile " + rc, 12, enter=True)
    time.sleep(BANNER_S)
    ctx.be.key("ctrl+i")
    tok, dt = run_in_inner(ctx, r, "clear", "bash", f)
    if dt is not None:
        r.metric("clear_inner_line_after_enter_s", round(dt, 3))
        r.check(dt < 2.0, "the first inner line ran %.2f s after its Enter (budget 2 s)" % dt)
    r.check(first_index(hops(ctx, off), "subshell_setup_ready_write") is not None, "the setup was written after the clear")
    leave_and_prove_parent(ctx, r, "clear", f, [tok])
