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


def need_bash_pane(ctx, r):
    """#1886: like need_posix, but the Windows Git Bash pane (bash --rcfile, HOME = the scratch home) is a bash pane too; a PowerShell or cmd pane is not.
    Windows has no tty probe: the readiness judge decides from the screen facts there (subshell_ready.rs, no_tty_probe)."""
    if ctx.win and ctx.pane == "gitbash":
        return
    need_posix(ctx, r)


def write_rc(ctx, name, text):
    """an rc file under the pane's ~/w (ctx.W); returns the path the typed command uses (~ is the scratch HOME)"""
    with open(os.path.join(ctx.W, name), "w") as f:
        f.write(text)
    return "~/w/" + name


# Every hop a case of this module looks up MUST be listed here: hops() drops every other line. Round 10 (macOS and Wayland, both modes):
# subshell_ready_secret_after_probe failed only "stage 2 waited for the prompt after the probe's hook (hop subshell_stage_two_pending)" while the
# app log of the same case has that hop, before the Secret cancel, with no write between them: the name was missing from this list, so
# first_index() could never find it. selftest.py (subshell_ready_hops_cover_every_lookup) reads the names the cases look up from this file.
HOPS = ("subshell_setup_armed", "subshell_ready_judged", "subshell_ready_line_end", "subshell_setup_ready_write", "subshell_written_judged",
        "subshell_setup_cancelled", "subshell_setup_unsupported", "subshell_stage_two_pending")


def hops_in(text):
    """pure: the readiness hops of an app-log excerpt, in order, as (hop, line)"""
    out = []
    for ln in text.splitlines():
        for h in HOPS:
            if h in ln:
                out.append((h, ln))
                break
    return out


def hops(ctx, off):
    """the readiness hops logged since `off`, in order, as (hop, rest-of-line)"""
    return hops_in(ctx.log_since(off))


def setup_timeline(text):
    """pure: where a setup's time went, from an app-log excerpt: the judge's decision when the setup was armed, then every progress event
    of the setup with its age (ms since the first setup line was written), e.g. `armed decision=Write(None); InitSubshell@1189 ms,
    InitShell@2778 ms, Bootstrapped@3742 ms`. Chunk and marker echoes are left out. #2051 follow-up (round 12, agent mode, Git Bash pane):
    subshell_ready_rc_clear's inner line ran 3.75 s after its Enter and the row could not say whether the app had waited (a judge, a net)
    or the shell had worked that long; the app log showed the setup written at age 0 and Bootstrapped at 3742 ms."""
    armed, stages = None, []
    for ln in text.splitlines():
        if armed is None and "hop=subshell_ready_judged source=armed" in ln and "decision=" in ln:
            armed = ln.split("decision=", 1)[1].split(" ", 1)[0]
        if "hop=subshell_setup_progress kind=" in ln and "age_ms=" in ln:
            kind = ln.split("kind=", 1)[1].split(" ", 1)[0]
            age = ln.split("age_ms=", 1)[1].split(" ", 1)[0]
            if kind not in ("StageTwoChunk", "Marker") and age.isdigit():
                stages.append("%s@%s ms" % (kind, age))
    return "armed decision=%s; %s" % (armed or "not logged", ", ".join(stages) or "no setup progress event logged")


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
        # no root pid / PID probe: the shell count under the app (taken by guarded() before the case) decides; never a blind `exit` (audit #2)
        typed = ctx.exit_nested(getattr(ctx, "sub_base", None), r, label)
        r.note("%s: no root shell pid recorded or no PID probe in this pane; `exit` %s by the shell-count proof" % (label, "typed" if typed else "NOT typed"))
        return
    r.check(state == "root", "%s: the shell in front after leaving is the pane's ROOT shell (got %s)" % (label, state))


def guarded(fn):
    """every readiness case leaves its inner shell(s) in a `finally` and proves it is back in the PARENT shell, whether it passed, failed or
    skipped (the run.py depth check is the second layer)"""
    @functools.wraps(fn)
    def run(ctx, r):
        ctx.left_inner = False
        ctx.sub_base = ctx.shell_count()        # leave_inner's fallback proof when no root pid is known
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
    need_bash_pane(ctx, r)
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
        # The judged hop of the decision that wrote is logged just BEFORE the write hop (round 8 Linux: it was counted as an earlier
        # decision, a harness bug); only Write decisions ahead of that one mean the app wrote or tried to while vim owned the tty.
        judged_writes = [ln for h, ln in hs[:write] if h == "subshell_ready_judged" and "decision=Write" in ln]
        before = judged_writes[:-1]
        r.check(bool(judged_writes), "the write came from a Write decision (hop subshell_ready_judged decision=Write)")
        r.check(not before, "no Write decision before the one that wrote: %r" % before[:2])
        if alt is not None:
            r.check(alt < write, "the setup was written after vim's alt screen, not while it ran")
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


def secret_read_case(ctx, r, at, label):
    """the body of the two E-SECRET cases: a bash whose PROMPT_COMMAND runs ONE `read -s` before the prompt numbered `at` (0 = the first prompt)"""
    need_posix(ctx, r)
    f, pw = "lab_%s.txt" % label, "lab_%s_pw.txt" % label
    ctx.rm(f, pw)
    # The `sleep 1` keeps the secret read from starting before the typist's first key: Linux round 7 saw it start 5 ms after the setup write
    # (cancel with 0 held chunks), so the line typed next went to the read in order and the case's "held line" premise was gone.
    rc = write_rc(ctx, "lab_%s.rc" % label,
                  'n=0; PROMPT_COMMAND=\'if [ "$n" = %d ]; then sleep 1; read -s -p "Secret: " x; echo "$x" > ~/w/%s; echo; fi; n=$((n+1))\'; PS1="LAB$ "\n' % (at, pw))
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
    leave_and_prove_parent(ctx, r, label, f, [tok])
    return hs


@case("subshell_ready_secret_after_prompt", order=22)
@guarded
def c_ready_secret_after_prompt(ctx, r):
    """E-SECRET after the first line: a bash whose PROMPT_COMMAND runs ONE `read -s` before its SECOND prompt, i.e. right after
    our first setup line was read and ran (the marker is progress, the secret read still wins). The line typed while the setup was
    written is handed back to the editor (hop why=Secret dest=Editor), the password typed next goes to the read (recorded by the
    rc, not run as a command), the setup is written again (a fresh attempt) and completes; the handed-back line is not run until
    the user presses Enter on it (the first Enter is the guard's)."""
    secret_read_case(ctx, r, 1, "secret")


@case("subshell_ready_secret_after_probe", order=22)
@guarded
def c_ready_secret_after_probe(ctx, r):
    """#1905's first line (review r8 D3): the same rc with the `read -s` before the THIRD prompt, i.e. right after the PROBE answered
    (the prelude and the probe line both ran; InitSubshell named bash). Stage 2's first line used to be written on that event,
    into the read. Now stage 2 waits for the prompt after the probe's hook (hop subshell_stage_two_pending), the secret read
    cancels it (why=Secret dest=Editor), the password alone reaches the read, and the attempt is re-armed KNOWING the shell: the
    second write names bash (shell=Some(Bash)), no second probe. Everything else as subshell_ready_secret_after_prompt."""
    hs = secret_read_case(ctx, r, 2, "secret2")
    pend = first_index(hs, "subshell_stage_two_pending")
    cancel = first_index(hs, "subshell_setup_cancelled", "why=Secret")
    r.check(pend is not None, "stage 2 waited for the prompt after the probe's hook (hop subshell_stage_two_pending)")
    if pend is not None and cancel is not None:
        r.check(pend < cancel, "the secret read was met while stage 2 was pending, not after a stage-2 line")
        r.check(not any(h == "subshell_setup_ready_write" for h, _ in hs[pend:cancel]), "nothing of stage 2 was written before the cancel")
    writes = [ln for h, ln in hs if h == "subshell_setup_ready_write"]
    r.check(len(writes) >= 2 and "shell=Some(Bash)" in writes[1], "the re-armed attempt wrote the bash script, no second probe: %r" % writes[1:2])


@case("subshell_sh_probe_empty_ps1", order=22)
@guarded
def c_sh_probe_empty_ps1(ctx, r):
    """#1935 (review r8 D2 and addendum 4): a dash with an EMPTY PS1 draws no prompt at all (round 8 Linux: the pane's bash exported PS1 and
    Phosphor blanked it, so a child dash inherited ""). Phosphorize must still write the probe at once (the idle reader with nothing
    unread needs no prompt), the probe's second line must follow the prelude's marker without waiting for a prompt, and the
    unsupported-shell EVENT ends the setup: the line typed right behind Ctrl-I runs in dash < 2 s after its Enter, no give-up, no
    cancel. The dash gets its empty PS1 from an $ENV file (`PS1=`, read by an interactive dash before its first prompt) on EVERY
    platform: on macOS the pane is zsh and `export PS1` handed the dash zsh's prompt string (round 9); on Linux the bash bootstrap
    un-exports PS1 at every prompt since #1907 (round 10, on purpose: a child shell gets its own default prompt instead of none), so
    the `export PS1` this case typed there no longer reached the dash, which drew `$ ` and failed the case's own "PS1 is empty" check
    while the app did the right thing (#2027). A shell with an empty PS1 still exists for real users (an rc file, `env PS1= sh`), which
    is what the case keeps proving. The case's own "PS1 is empty" check is unchanged.
    A second leg (empty_ps1_typed_before_setup, Linux) types a line before the setup is written."""
    need_posix(ctx, r)
    if not shutil.which("dash"):
        r.skip("dash is not installed")
    f = "lab_ps1.txt"
    ctx.rm(f)
    write_rc(ctx, "lab_ps1_empty.rc", EMPTY_PS1_RC)
    ctx.be.type("export ENV=$HOME/w/lab_ps1_empty.rc", 12, enter=True)     # $HOME, not ~: no shell-specific tilde rule after `=`
    time.sleep(0.5)
    try:
        empty_ps1_first_leg(ctx, r, f)
    finally:
        if not ctx.left_inner:
            leave_inner(ctx, r, "empty_ps1")
        ctx.be.type("unset ENV", 12, enter=True)               # the pane's shell: later dash starts get their own prompt again
        time.sleep(0.3)
    empty_ps1_typed_before_setup(ctx, r)


EMPTY_PS1_RC = "PS1=\n"                      # the $ENV file of the first leg: the dash's prompt string is empty before its first prompt
EMPTY_PS1_SLEEP_RC = "PS1=\nsleep 4\n"      # the second leg's: the same empty prompt, then 4 s in which the dash reads nothing


def empty_ps1_first_leg(ctx, r, f):
    """the body of subshell_sh_probe_empty_ps1's first leg: the dash is started with whatever the caller arranged for its PS1"""
    off = ctx.log_size()
    ctx.be.type("dash", 12, enter=True)
    time.sleep(BANNER_S)
    ctx.shot("empty_ps1_banner")
    ctx.be.key("ctrl+i")
    tok = ctx.tok(6)
    ctx.be.type(ctx.append_cmd("sh", tok, f), 12, enter=True)  # held through the probe, delivered by the unsupported event
    t0 = time.time()
    got = ctx.until(lambda: tok in ctx.lines(f), 10)
    dt = time.time() - t0
    r.metric("empty_ps1_inner_line_after_enter_s", round(dt, 3))
    r.check(got, "the first inner line ran at all (file %r)" % ctx.lines(f))
    r.check(dt < 2.0, "the first inner line ran %.2f s after its Enter (budget 2 s)" % dt)
    r.check(ctx.until(lambda: first_index(hops(ctx, off), "subshell_setup_unsupported") is not None, 6),
            "the probe answered and the unsupported-shell event ended the setup (its second line was not stalled for a prompt)")
    ctx.be.type("printf '[%s]\\n' \"$PS1\" >> " + f, 12, enter=True)   # what PS1 this dash really has
    ctx.until(lambda: len(ctx.lines(f)) >= 2, 6)
    l = ctx.lines(f)
    r.check(l[1:2] == ["[]"], "this dash has an EMPTY PS1 (else the case proved nothing about #1935): %r" % l[1:2])
    hs = hops(ctx, off)
    r.check(first_index(hs, "subshell_setup_ready_write") is not None, "the probe was written with no prompt drawn (hop subshell_setup_ready_write)")
    r.check(first_index(hs, "subshell_setup_cancelled") is None, "no cancel")
    r.check(not any("subshell_setup_given_up" in ln for ln in ctx.log_since(off).splitlines()), "no give-up net fired")
    leave_and_prove_parent(ctx, r, "empty_ps1", f, l[:2])


def empty_ps1_typed_before_setup(ctx, r):
    """The second leg of subshell_sh_probe_empty_ps1 (owner decision 2026-11-09, #1935 "reader idle"): the same empty-PS1 dash, but the
    user types a line BEFORE the setup is written. dash's $ENV file sleeps, so the setup armed by Ctrl-I waits (a program in front);
    the line typed then is typeahead that dash runs when its rc ends. No prompt ever proves that line ran, so before this fact the
    attempt stayed armed until the 60 s ceiling. Now the look after the line's output finds the queue empty and dash blocked in
    read(2) on the tty (Linux: /proc/<pid>/syscall) and writes the probe: hop `subshell_ready_judged decision=Write ...
    reader_blocked=Some(true)` with bytes > 0, then the unsupported-shell event. The typed line ran exactly once, the next line runs.
    The typed line ends in an `echo` so the pane gets an output event after dash is done with it (the judge looks at events only).
    Linux only: other platforms have no such fact and keep the old rule."""
    if not sys.platform.startswith("linux"):
        r.note("empty_ps1 typed-before-the-setup leg: needs the Linux blocked-in-read fact (/proc/<pid>/syscall); not run on %s" % sys.platform)
        return
    label, f = "empty_ps1_typed", "lab_ps1_typed.txt"
    ctx.rm(f)
    rc = write_rc(ctx, "lab_ps1_sleep.rc", EMPTY_PS1_SLEEP_RC)   # PS1= first: since #1907 the pane's bash no longer hands a child an empty PS1
    ctx.left_inner = False
    try:
        ctx.be.type("export ENV=" + rc, 12, enter=True)      # an interactive dash reads $ENV: PS1 emptied, then busy for 4 s
        time.sleep(0.5)
        off = ctx.log_size()
        ctx.be.type("dash", 12, enter=True)
        time.sleep(BANNER_S)
        ctx.be.key("ctrl+i")
        r.check(ctx.until(lambda: first_index(hops(ctx, off), "subshell_setup_armed") is not None, 4), "%s: Ctrl-I armed the setup" % label)
        tok = ctx.tok(6)
        ctx.be.type("echo %s >> %s; echo ran" % (tok, f), 12, enter=True)    # typed while the rc sleeps: before any setup byte
        r.check(ctx.until(lambda: tok in ctx.lines(f), 10), "%s: the line typed before the setup ran in dash (file %r)" % (label, ctx.lines(f)))
        r.check(ctx.until(lambda: first_index(hops(ctx, off), "subshell_setup_unsupported") is not None, 6),
                "%s: the probe was written once dash sat in its read again, and answered (before: armed until the 60 s ceiling)" % label)
        hs = hops(ctx, off)
        line_end = first_index(hs, "subshell_ready_line_end")
        write = first_index(hs, "subshell_setup_ready_write")
        r.check(line_end is not None and write is not None and line_end < write,
                "%s: premise: the line was ended BEFORE the setup was written (line_end %r, write %r)" % (label, line_end, write))
        judged = [ln for h, ln in hs[:write or 0] if h == "subshell_ready_judged" and "decision=Write" in ln]
        r.check(len(judged) == 1 and "reader_blocked=Some(true)" in judged[0] and " bytes=0 " not in judged[0],
                "%s: one Write decision, on the blocked-read fact with the user's bytes counted: %r" % (label, judged[:2]))
        r.check(first_index(hs, "subshell_setup_cancelled") is None, "%s: no cancel" % label)
        r.check(not any("subshell_setup_given_up" in ln for ln in ctx.log_since(off).splitlines()), "%s: no give-up net fired" % label)
        tok2, _ = run_in_inner(ctx, r, label, "sh", f)
        r.check(ctx.lines(f) == [tok, tok2], "%s: the typed-ahead line ran exactly once, then the next: %r" % (label, ctx.lines(f)))
        ps1f = "lab_ps1_typed_ps1.txt"                      # the leg's premise, checked like the first leg's: this dash draws no prompt
        ctx.rm(ps1f)
        ctx.be.type("printf '[%s]\\n' \"$PS1\" >> " + ps1f, 12, enter=True)
        ctx.until(lambda: len(ctx.lines(ps1f)) >= 1, 6)
        r.check(ctx.lines(ps1f)[:1] == ["[]"], "%s: this dash has an EMPTY PS1 (else the leg proved nothing about a shell that draws no prompt): %r" % (label, ctx.lines(ps1f)[:1]))
        leave_and_prove_parent(ctx, r, label, f, [tok, tok2])
    finally:
        if not ctx.left_inner:
            leave_inner(ctx, r, label)
        ctx.be.type("unset ENV", 12, enter=True)             # the pane's bash: later dash starts must not sleep
        time.sleep(0.3)


@case("subshell_ready_relay_shape", order=22)
@guarded
def c_ready_relay_shape(ctx, r):
    """the relay shape (rows 9/10, R4): `sudo -s` with use_pty stays the foreground leader, so only the first new prompt behind it
    (with the local tty raw) says the shell is ready. Needs a passwordless sudo on the runner; skips otherwise."""
    need_posix(ctx, r)
    if not shutil.which("sudo") or subprocess.run(["sudo", "-n", "true"], capture_output=True).returncode != 0:
        r.skip("no passwordless sudo on this runner (`sudo -n true` failed). NEEDS: a sudoers entry on this test machine that lets the CI user run sudo without a password, "
               "with `Defaults use_pty` in effect (owner: machine change; the harness never edits sudoers). Has never run on any machine")
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
        r.skip("docker with a local alpine image is not available (`docker image inspect alpine` failed). NEEDS: docker usable by the CI user on this test machine and the "
               "`alpine` image pulled once by hand (owner: machine change; the harness never pulls an image). Has never run on any machine")
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


NEWUSER_FOOTER = b"Type one of the keys in parentheses"     # the last line of zsh-newuser-install's first screen
NEWUSER_MIN_TTY = (15, 72)      # zsh-newuser-install returns at once on a smaller terminal ("Don't run unless terminal is sane")


def newuser_probe_env(environ, dotdir, home):
    """The environment of the zsh the guard starts: the caller's, with ZDOTDIR = the empty directory and HOME = the pane's HOME. zsh looks for its
    startup files in $ZDOTDIR and only without one in $HOME, so both are set exactly as the case's own command line leaves them in the pane."""
    env = dict(environ)
    env["ZDOTDIR"] = dotdir
    env["HOME"] = home
    return env


def zsh_runs_newuser_install(dotdir, home, wait=4.0):
    """True when an interactive zsh whose startup-file directory (ZDOTDIR) is the EMPTY `dotdir` shows the zsh-newuser-install menu on a real pty
    (24x80: the menu refuses a terminal under 15x72). Judged by the menu's footer (the word "newuser" is also in the function's error messages); the
    zsh is killed and the pty closed, nothing is typed into it (a key would pick an option). `dotdir` is emptied again afterwards (zsh leaves a
    .zcompdump). Rounds 7-9b (#2015): this ran with only HOME set, in the harness's environment; the PANE has ZDOTDIR = the scratch home (app.py),
    whose .zshrc zsh read instead, so the guard saw a menu the pane never showed."""
    import fcntl, pty, select, signal, struct, termios
    env = newuser_probe_env(os.environ, dotdir, home)
    pid, fd = pty.fork()
    if pid == 0:
        try:
            fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
            os.execvpe("zsh", ["zsh", "-i"], env)
        finally:
            os._exit(127)
    seen, end = b"", time.time() + wait
    try:
        while time.time() < end and NEWUSER_FOOTER not in seen:
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
        shutil.rmtree(dotdir, ignore_errors=True)
        os.makedirs(dotdir, exist_ok=True)
    return NEWUSER_FOOTER in seen


def environ_value(raw, var):
    """the value of `var` in a /proc/<pid>/environ image (NUL-separated KEY=VALUE bytes), or None when it is not set"""
    key = var.encode() + b"="
    for kv in raw.split(b"\0"):
        if kv.startswith(key):
            return kv[len(key):].decode("utf-8", "replace")
    return None


def pane_zsh_facts(root_pid):
    """What the OS says about the zsh started under the pane's ROOT shell (Linux /proc): {"zdotdir": ZDOTDIR of its environment or None, "rows",
    "cols": its tty's size or None}. None when it cannot be told (no root pid, no /proc, no zsh child yet, a process of another user). Nothing is
    typed and the pty is not written: the environment is read from /proc/<pid>/environ, the size by TIOCGWINSZ on the zsh's own stdin."""
    if root_pid is None or not os.path.isdir("/proc/self"):
        return None
    try:
        out = subprocess.run(["ps", "-A", "-o", "pid=,ppid=,comm="], capture_output=True, text=True, timeout=10).stdout
    except Exception:
        return None
    pid = None
    for l in out.splitlines():
        f = l.split(None, 2)
        if len(f) == 3 and f[0].isdigit() and f[1] == str(root_pid) and os.path.basename(f[2].strip()).lstrip("-") == "zsh":
            pid = int(f[0])
    if pid is None:
        return None
    try:
        with open("/proc/%d/environ" % pid, "rb") as f:
            raw = f.read()
    except OSError:
        return None
    rows = cols = None
    try:
        import fcntl, struct, termios
        fd = os.open("/proc/%d/fd/0" % pid, os.O_RDONLY | os.O_NOCTTY | os.O_NONBLOCK)
        try:
            rows, cols = struct.unpack("HHHH", fcntl.ioctl(fd, termios.TIOCGWINSZ, b"\0" * 8))[:2]
        finally:
            os.close(fd)
    except (OSError, ImportError):
        pass
    return {"zdotdir": environ_value(raw, "ZDOTDIR"), "rows": rows, "cols": cols}


def newuser_precondition(facts, dotdir):
    """Is the zsh in the pane one that shows the menu? -> (verdict, text): "ok"; "unknown" (the OS could not be asked: the case goes on, with a
    note); "wrong_env" (its ZDOTDIR is not the empty directory: it read some other startup files and sits at a prompt, a TEST fault, #2015);
    "small" (its terminal is under 15x72: zsh-newuser-install returns without a menu)."""
    if facts is None:
        return "unknown", "the pane's zsh could not be inspected (no /proc, no root pid or no zsh under the root shell): its environment is not proven"
    got = facts.get("zdotdir")
    if got is None or os.path.realpath(got) != os.path.realpath(dotdir):
        return "wrong_env", "the pane's zsh was not started with the empty startup directory (ZDOTDIR=%r, wanted %r): no menu can show" % (got, dotdir)
    rows, cols = facts.get("rows"), facts.get("cols")
    if rows and cols and (rows < NEWUSER_MIN_TTY[0] or cols < NEWUSER_MIN_TTY[1]):
        return "small", "the pane is %dx%d: zsh-newuser-install shows no menu under %dx%d" % (rows, cols, NEWUSER_MIN_TTY[0], NEWUSER_MIN_TTY[1])
    return "ok", "the pane's zsh has ZDOTDIR=%s on a %sx%s tty" % (got, rows, cols)


@case("subshell_ready_zsh_newuser_install", order=22)
@guarded
def c_ready_zsh_newuser_install(ctx, r):
    """reviewer R6: a zsh with NO startup files runs zsh-newuser-install, whose menu reads ONE key: a setup written there picks an option
    (the probe's `2` of `2>&1` would populate ~/.zshrc). The footer counts as an InputPrompt: nothing is written until the user
    answers `q`, no .zshrc appears, and the setup is written at the zle prompt that follows; the typed line runs in zsh.
    #2015 (rounds 7-9b, Linux): zsh looks for its startup files in $ZDOTDIR, and the pane has ZDOTDIR = the scratch home (app.py), which has a
    .zshrc. `HOME=<empty dir> zsh` therefore never showed a menu, the case's `q` ran as part of the next command, and with HOME changed the PID
    probe (`~/.appci_pid`) answered where the harness does not read. The case now empties the directory zsh really looks in (ZDOTDIR) and leaves
    HOME alone; the guard starts its zsh the same way, and the pane's zsh is inspected (its environment, its tty size) before anything is judged."""
    need_posix(ctx, r)
    if not shutil.which("zsh"):
        r.skip("zsh is not installed")
    home = os.path.join(ctx.W, "lab_zsh_home")          # the EMPTY directory zsh looks for its startup files in (ZDOTDIR), not a HOME
    shutil.rmtree(home, ignore_errors=True)
    os.makedirs(home)
    if not zsh_runs_newuser_install(home, ctx.prof.home):
        # Mac round 7: Apple's zsh starts at a plain prompt with no startup files (no menu), so the setup is rightly written at once and the
        # `q` this case types for the menu ran as a command ("qecho ...: command not found"). No menu = nothing to test here.
        r.skip("this zsh runs no zsh-newuser-install with an empty startup directory (proved on a pty before the case types anything)")
    f = "lab_zshnew.txt"
    ctx.rm(f)
    off = ctx.log_size()
    ctx.be.type("ZDOTDIR=~/w/lab_zsh_home zsh", 12, enter=True)
    time.sleep(BANNER_S)
    ctx.shot("zsh_newuser_menu")
    verdict, text = newuser_precondition(pane_zsh_facts(getattr(ctx, "root_pid", None)), home)
    if verdict == "small":
        r.skip(text)
    if verdict == "unknown":
        r.note(text)
    else:
        r.check(verdict == "ok", "TEST precondition: " + text)
        if verdict != "ok":
            return                                            # every check below would be judged against a zsh that shows no menu
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
    need_bash_pane(ctx, r)
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
        # where the time went, in the row (the budget below is unchanged): a line typed behind a setup that was already written is held to
        # the setup's own end, so its time is the shell's setup time; a line typed before the write runs at once (setup_timeline)
        r.note("clear: setup " + setup_timeline(ctx.log_since(off)))
        r.check(dt < 2.0, "the first inner line ran %.2f s after its Enter (budget 2 s)" % dt)
    r.check(first_index(hops(ctx, off), "subshell_setup_ready_write") is not None, "the setup was written after the clear")
    leave_and_prove_parent(ctx, r, "clear", f, [tok])
