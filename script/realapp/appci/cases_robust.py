# appci/cases_robust.py: robustness cases (#1468). Registered with the same @case decorator as cases.py (imported by its last line).
#   session_restore       quit via the app's quit key, relaunch with the same private profile: tabs + cwds back, NOTHING re-runs
#   quit_24_tabs          #1410: 24 streaming tabs + a 2nd window, quit key (+ confirm dialog): exits within a bound, no 'Failed to send Shutdown'
#   flood_typed_line      a line typed (10-25 ms/key) behind a 2e6-line / 5e6-line flood runs once, in order, after the flood
#   flood_typing_latency  typing into a quiet tab while another tab floods: arrival gaps at the pty (loss/order exact, stalls bounded)
#   flood_scrollback_find find-in-output reaches a marker printed before the flood (needs OCR: tesseract)
#   editor_edit_save      code editor: typed edit, save, undo/redo, close pane, reopen (even after an external change): disk byte-exact
#   editor_paste          code editor: Ctrl-V (#1409) and Ctrl-Shift-V paste
#   soak                  ~60 min mixed use, RSS/CPU sampled every minute, growth rule below (full tier only, APPCI_SOAK_MIN shortens it)
# Linux only where the case needs Cmd-Q-free quit or the D-Bus URL forward (Mac/Windows SKIP with the reason).
import os, re, shutil, statistics, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402  (cases.py imports this module as its last line, so everything below is already defined)
import toml_text as T  # noqa: E402
from cases import case, stats, plat  # noqa: E402

NEWTAB = os.environ.get("APPCI_KEY_NEWTAB", "ctrl+shift+t")
NEWWIN = os.environ.get("APPCI_KEY_NEWWIN", "ctrl+shift+n")
QUIT_KEY = os.environ.get("APPCI_KEY_QUIT", "ctrl+shift+q")
CLOSE_PANE = os.environ.get("APPCI_KEY_CLOSE", "ctrl+shift+w")
# Phosphor's own close shortcut on macOS is cmd-shift-W (app/src/util/bindings.rs: CloseCurrentSession = cmd_or_ctrl_shift("w"); it is also bound to
# CloseWindow), and the backend refuses every Cmd+W chord on purpose (backend.MacBackend.NEVER_CMD, owner rule: Cmd-W/Cmd-Q hit the wrong app).
# So there the key is empty and a case that NEEDS a keyboard close must SKIP with this reason; it must never call be.key("").
NO_CLOSE_WHY = ("no keyboard close-pane/tab shortcut is usable on this platform (APPCI_KEY_CLOSE empty): Phosphor's close shortcut on macOS is "
                "cmd-shift-W, which the frontmost-app guard refuses by policy")
NOW = lambda ctx: "now" if ctx.be.name == "mac" else "date +%s.%N"     # BSD date has no %N; ~/bin/now is written by profile.py


# Precise skip reasons (a coverage gap, not "not applicable"): what is missing on macOS/Windows for each family of Linux-only cases.
SKIP_QUIT_WHY = ("%s needs a quit + relaunch on the same profile: the Linux flow sends the quit key (ctrl+shift+q) and App.proc waits on the child; "
                 "on macOS Cmd-Q is refused by the frontmost-app guard and MacBackend has no quit/relaunch (an osascript 'tell application to quit' path and a "
                 "relaunch through the LaunchServices PID are not implemented); Windows has a separate flow")
SKIP_EDITOR_WHY = ("the file is opened by a second `phosphor <path>` process that the running instance receives over the D-Bus URL forward (Linux) or the "
                   "Windows named pipe (#1544, runs there); macOS (open -a / an Apple Event open-document handler) has cb_editor_mac, and the editor key set differs "
                   "(Cmd-S/Cmd-V on macOS, close = cmd-shift-W which the guard refuses)")


def close_tab_best_effort(ctx, r):
    """end-of-case cleanup of a throw-away shell tab: the close key where there is one, else Ctrl-D (EOF ends the tab's shell). Never raises
    on an empty key; the note says which was used so a leftover tab is attributable."""
    if CLOSE_PANE:
        ctx.be.key(CLOSE_PANE)
    else:
        ctx.be.key("ctrl+d")
        r.note("no close key on this platform: the throw-away tab was ended with Ctrl-D (unverified that the tab disappears)")


def need_linux_app(ctx, why):
    if ctx.be.name == "mac" or plat().startswith("macos") or plat().startswith("windows"):
        ctx.skip_reason = why
        raise cases.Skip("%s (this platform: %s)" % (why, plat()))
    if ctx.app is None or not hasattr(ctx.app, "proc"):
        raise cases.Skip("needs the Linux App launcher (not the sim backend)")


def need_relaunch_app(ctx, why):
    """#1540/#1541: the cases that need only a KILL (or hard stop) and a relaunch on the same profile run on Linux, macOS and Windows: every App
    handle has stop()/start(), and the profile's settings.toml is (re)installed by start() (macOS: ~/.phosphor, Windows: %LOCALAPPDATA%). Only the sim
    backend (no app process) skips."""
    if ctx.app is None or not (hasattr(ctx.app, "start") and hasattr(ctx.app, "stop")):
        raise cases.Skip("%s (needs an app handle with stop()/start(), not the sim backend)" % why)


def data_roots(ctx):
    """directories that hold the app's own state on this platform: the scratch home (Linux, macOS: the app derives every path from HOME) or, on
    Windows, the known folders %LOCALAPPDATA%\\phosphor and %APPDATA%\\phosphor (not HOME-relative; WindowsApp.prepare wipes them per pass)"""
    if ctx.win:
        return [os.path.join(os.environ[v], "phosphor") for v in ("LOCALAPPDATA", "APPDATA") if os.environ.get(v)]
    return [ctx.prof.home]


def norm_path(p):
    """a cwd as the case compares it: the real path on POSIX; on Windows the drive-letter form (`pwd -W` prints C:/Users/..., Python's realpath may
    resolve a junction, so both sides go through os.path.normcase(normpath) only)"""
    return os.path.normcase(os.path.normpath(p)) if os.name == "nt" else os.path.realpath(p)


def PWD(ctx):
    """the shell command that prints the pane's cwd in a form norm_path() reads: Git Bash `pwd -W` (Windows drive path), else `pwd`"""
    return "pwd -W" if ctx.win else "pwd"


def tab_key(ctx, n):
    """activate tab n (9 = the last tab): cmdorctrl-N (app/src/workspace/mod.rs), so Cmd on macOS"""
    return "%s+%d" % ("cmd" if ctx.be.name == "mac" else "ctrl", n)


def fwd(path):
    """a path typed into a pane: forward slashes (Git Bash eats backslashes; PowerShell and cmd take both)"""
    return path.replace("\\", "/")


def hard_kill(ctx, r, label):
    """The app dies with no quit path and no flush: SIGKILL of the exact app pid (Linux, macOS), taskkill /PID <pid> /T /F (Windows: the exact pid
    the harness started, plus its tree so no orphaned shell keeps the profile's files open). True once it is gone within 8 s."""
    pid = ctx.app.pid()
    r.check(bool(pid), "%s: no app pid" % label)
    if not pid:
        return False
    if os.name == "nt":
        subprocess.run(["taskkill", "/PID", str(pid), "/T", "/F"], capture_output=True, timeout=30)
    else:
        import signal
        os.kill(pid, signal.SIGKILL)
    ok = wait_exit(ctx, 8)
    r.check(ok, "%s: the app is still alive 8 s after the kill" % label)
    keep_log(ctx, label)
    ctx.app.stop()                         # reaps the launcher / any orphaned shells of the dead app, and clears the handle
    return ok


def stop_for_restart(ctx, r, bound, label):
    """Take the app down so its settings can be edited and it can be relaunched on the same profile. Linux: through the app's own quit key (the
    session writer runs, as for a user). macOS / Windows have no usable quit key for the harness (Cmd-Q is refused by the frontmost-app guard; the
    Windows close is a confirm dialog), so the app is stopped by pid (SIGTERM then SIGKILL / taskkill) and the result says so: a restore case on
    those platforms proves the restore of a HARD stop, not of a quit. Returns the quit_app-shaped dict."""
    if not (ctx.be.name == "mac" or ctx.win):
        return quit_app(ctx, r, bound, label)
    t0 = time.time()
    ctx.app.stop()
    ok = wait_exit(ctx, bound)
    res = dict(exited=ok, confirm_to_exit_s=0.0, total_s=round(time.time() - t0, 2), dialog=False, attempt=0, how="stop-by-pid")
    r.metric("%s_quit" % label, res)
    return res


# ------------------------------------------------------------------ app lifecycle helpers (Linux)
def settings_file(ctx):
    return os.path.join(ctx.prof.home, ".config", "phosphor", "settings.toml")


def set_restore(ctx, on):
    """textual edit of the profile's settings.toml (the app must be stopped; the owner's seed file is never touched)"""
    p = settings_file(ctx)
    t = open(p).read()
    t = T.set_key(t, "general", "restore_session", "true" if on else "false")
    open(p, "w").write(t)


def keep_log(ctx, tag):
    try:
        import ci_env
        src = ctx.log_path
        if src and os.path.exists(src):
            d = os.path.join(ctx.run, "logs", ctx.mode)
            os.makedirs(d, exist_ok=True)
            txt = ci_env.redact(open(src, errors="replace").read(), ci_env.ollama()[0])
            open(os.path.join(d, "phosphor.%s.log" % tag), "w").write(txt)
    except Exception:
        pass


def wait_exit(ctx, secs):
    t = time.time()
    while time.time() - t < secs:
        if not ctx.app.alive():
            return True
        time.sleep(0.05)
    return not ctx.app.alive()


def quit_app(ctx, r, bound, label):
    """Quit through the app's own quit key (never a signal). If the confirm dialog ('Yes, quit') appears the app is still alive
    after a short wait; it is confirmed with Return, then quit-key+Tab+Return, then quit-key+Tab+Tab+Return (one of them lands on
    'Yes, quit'; which one is a metric). Returns dict(exited, confirm_to_exit_s, total_s, dialog, attempt)."""
    ctx.be.guard()
    t0 = time.time()
    ctx.be.key(QUIT_KEY)
    t_last, dialog, attempt = t0, False, 0
    if not wait_exit(ctx, 2.0):
        dialog = True
        ctx.shot("%s_quit_dialog" % label)
        txt = ctx.ocr("%s_quit_dialog_ocr" % label)
        if txt is not None:
            r.metric("%s_dialog_text_seen" % label, "yes, quit" in " ".join(txt.lower().split()))
        for attempt, ks in enumerate((["Return"], [QUIT_KEY, "Tab", "Return"], [QUIT_KEY, "Tab", "Tab", "Return"]), 1):
            ctx.be.guard()
            ctx.be.keys(ks, delay_ms=60)
            t_last = time.time()
            if wait_exit(ctx, 3.0):
                break
    ok = wait_exit(ctx, max(0.0, bound - (time.time() - t_last)))
    t_end = time.time()
    res = dict(exited=ok, confirm_to_exit_s=round(t_end - t_last, 2), total_s=round(t_end - t0, 2), dialog=dialog, attempt=attempt)
    r.metric("%s_quit" % label, res)
    return res


def start_app(ctx, r, label, restored_min=1, timeout=90):
    """(re)launch the app on the same profile; returns (new prompts, ok). Waits until the prompt count stops growing. Windows: the main window,
    then the prompt (Git Bash pane) or a fixed settle (PowerShell pane: no rc hook), then the key proof and pane state again (run.py's `reprove`),
    exactly what run.py does after a relaunch."""
    ctx.snapshot_log()                                  # the next launch overwrites phosphor.log; the injector accounting spans it
    ctx.app.stop()
    base = ctx.prompts()
    ctx.app.start()
    if ctx.win:
        up = ctx.app.wait_window(60) and (ctx.until(lambda: ctx.prompts() >= base + restored_min, timeout, step=0.5) if ctx.prof.local_prompts else (time.sleep(12) or ctx.app.alive()))
        if not up:
            r.check(False, "%s: no window / shell after the relaunch within %d s (alive %s)" % (label, timeout, ctx.app.alive()))
            return 0, False
        time.sleep(4)
        ctx.be.guard()
        bad = ctx.reprove() if getattr(ctx, "reprove", None) else None
        if bad:
            r.check(False, "%s: after the relaunch %s" % (label, bad))
            return 0, False
        return ctx.prompts() - base, True
    if not ctx.until(lambda: ctx.prompts() >= base + restored_min and ctx.app.alive(), timeout, step=0.5):
        r.check(False, "%s: no shell prompt within %d s after the relaunch (prompts %d -> %d, alive %s)" % (label, timeout, base, ctx.prompts(), ctx.app.alive()))
        return 0, False
    last, since = ctx.prompts(), time.time()
    while time.time() - since < 4.0:                    # prompt count stable for 4 s = every restored shell has started
        time.sleep(0.5)
        if ctx.prompts() != last:
            last, since = ctx.prompts(), time.time()
    time.sleep(2.0)
    ctx.be.guard()
    return ctx.prompts() - base, True


def fresh_app(ctx, r, label):
    """leave a clean single-tab app with restore_session=false for the cases that follow"""
    try:
        if ctx.app.alive():
            q = stop_for_restart(ctx, r, 20, label + "_cleanup")
            if not q["exited"]:
                r.note("%s: cleanup quit did not exit; falling back to the harness stop (SIGTERM to the app's own process group)" % label)
        keep_log(ctx, label + "_end")
        set_restore(ctx, False)
        start_app(ctx, r, label + "_fresh")
    except Exception as e:                                # never mask the case's own verdict
        r.check(False, "%s: could not leave a fresh app behind: %r" % (label, e))


def descendants_cwd_under(prefix):
    out = set()
    for p in os.listdir("/proc"):
        if p.isdigit():
            try:
                if os.readlink("/proc/%s/cwd" % p).startswith(prefix):
                    out.add(int(p))
            except OSError:
                pass
    return out


# ------------------------------------------------------------------ session_restore
@case("session_restore", order=85, gui=True)
def c_session_restore(ctx, r):
    """N tabs (cwd per tab, one command run in each), quit with the app's quit key, relaunch on the same profile: the N tabs come
    back in order with their cwds and NOTHING re-runs (effect file unchanged, rec.log has no command from the restore)."""
    need_relaunch_app(ctx, "session_restore")
    n = int(os.environ.get("APPCI_SR_TABS", "5"))
    bound = float(os.environ.get("APPCI_QUIT_BOUND_S", "10"))
    try:
        # 1. restart with restore_session=true (the profile default is false so the other cases start clean)
        q = stop_for_restart(ctx, r, 20, "sr_setup")
        r.check(q["exited"], "sr_setup: the app did not exit through the quit key within 20 s (%r)" % q)
        if not q["exited"]:
            return
        keep_log(ctx, "sr_setup")
        set_restore(ctx, True)
        _, up = start_app(ctx, r, "sr_first")
        if not up:
            return
        ctx.shot("sr_first_launch")
        # 2. N tabs, each with its own cwd and one command
        eff = os.path.join(ctx.W, "sr_eff.txt")
        for f in ["sr_eff.txt"] + ["sr_pwd_%d.txt" % i for i in range(n)]:
            ctx.rm(f)
        dirs, expect_cmds = [], []
        for i in range(n):
            d = fwd(os.path.join(ctx.W, "sr_dir_%d" % i))
            os.makedirs(d, exist_ok=True)
            dirs.append(norm_path(d))
            if i:
                ctx.be.key(NEWTAB)
                time.sleep(0.8)
                ctx.be.guard()
            tok = ctx.tok(6).lower()
            cmd = "cd %s && echo SR%d_%s >> %s" % (d, i, tok, fwd(eff))
            expect_cmds.append(cmd)
            ctx.be.type(cmd, ctx.rng.randint(10, 25), enter=True)
            ctx.until(lambda: len(ctx.lines("sr_eff.txt")) >= i + 1, 10)
            time.sleep(0.4)
        time.sleep(3.0)                     # the session writer runs at least once before a stop-by-pid (a quit flushes it itself)
        eff_before = ctx.lines("sr_eff.txt")
        r.check(len(eff_before) == n, "setup: effect file has %d/%d lines before the quit" % (len(eff_before), n))
        have = [(t, c) for t, c in ctx.prof.commands() if not ctx.APP_BOOTSTRAP_RE.search(c)][-n:]      # r18u8: not the app's own bootstrap lines
        r.check([c for _, c in have] == expect_cmds, "setup: shell received %r, expected %r" % ([c for _, c in have], expect_cmds))
        ctx.shot("sr_tabs_before_quit")
        # 3. quit with the app's own path, then relaunch on the same profile
        keep_off = ctx.log_size()
        q = stop_for_restart(ctx, r, bound, "sr")
        if q.get("how"):
            r.note("sr: the app was stopped by pid (%s), not through a quit key: this proves the restore of a hard stop after the session writer ran (3 s settle), not of a quit" % q["how"])
        r.check(q["exited"], "quit: the app was still alive %.1f s after the last quit key (bound %.0f s): %r" % (bound, bound, q))
        slice_ = ctx.log_since(keep_off)
        r.check("Failed to send Shutdown" not in slice_, "quit: 'Failed to send Shutdown' in the app log (#1410)")
        keep_log(ctx, "sr_quit")
        if not q["exited"]:
            return
        eff_after_quit = ctx.lines("sr_eff.txt")
        mark, prompts_before = ctx.cmd_mark(), ctx.prompts()
        newp, up = start_app(ctx, r, "sr_restored", restored_min=1)
        if not up:
            return
        r.metric("restored_shell_prompts", newp)
        ctx.shot("sr_restored")
        # 4. nothing re-ran: effect file unchanged, no command line reached any restored shell
        eff_after = ctx.lines("sr_eff.txt")
        r.check(eff_after == eff_before == eff_after_quit, "RE-RUN: effect file changed across the restore: before %r after %r" % (eff_before, eff_after))
        ran = ctx.cmds_since(mark)
        r.check(not ran, "RE-RUN: rec.log shows %d command(s) received by the restored shells before any key was sent: %r" % (len(ran), ran[:5]))
        if ctx.win and not ctx.prof.local_prompts:
            r.note("no prompt hook in this pane (PowerShell): `a restored shell started` rests on the window, the key proof and the cwd answers below, not on a prompt count (H10)")
        else:
            r.check(newp >= 1, "no restored shell started (prompts +%d)" % newp)
        # 5. the tabs and their cwds: ctrl+1..n, then ctrl+9 (last tab). Each answers with its own pwd.
        got = []
        for i in range(n):
            ctx.be.guard()
            ctx.be.key(tab_key(ctx, i + 1))
            time.sleep(0.8)
            ctx.be.type("%s >> %s/sr_pwd_%d.txt" % (PWD(ctx), fwd(ctx.W), i), ctx.rng.randint(10, 25), enter=True)
            ctx.until(lambda: ctx.lines("sr_pwd_%d.txt" % i), 8)
            got.append((ctx.lines("sr_pwd_%d.txt" % i) or ["<none>"])[0])
        got_r = [norm_path(g) if g != "<none>" else g for g in got]
        r.check(got_r == dirs, "tabs/cwds not restored as they were: expected %r got %r" % (dirs, got_r))
        ctx.be.guard()
        ctx.be.key(tab_key(ctx, 9))
        time.sleep(0.8)
        ctx.rm("sr_pwd_last.txt")
        ctx.be.type("%s >> %s/sr_pwd_last.txt" % (PWD(ctx), fwd(ctx.W)), 12, enter=True)
        ctx.until(lambda: ctx.lines("sr_pwd_last.txt"), 8)
        last = (ctx.lines("sr_pwd_last.txt") or ["<none>"])[0]
        r.check(norm_path(last) == dirs[-1] if last != "<none>" else False,
                "the last tab (last tab key) is %r, expected %r: a tab was added or dropped by the restore" % (last, dirs[-1]))
        ctx.shot("sr_after_verify")
        r.check(ctx.lines("sr_eff.txt") == eff_before, "RE-RUN: effect file changed while verifying: %r" % ctx.lines("sr_eff.txt"))
    finally:
        fresh_app(ctx, r, "sr")


# ------------------------------------------------------------------ quit_24_tabs (#1410)
@case("quit_24_tabs", order=86, gui=True)
def c_quit_24(ctx, r):
    """#1410: 24 streaming tabs + a second window, quit through the quit key (confirm dialog included): the process exits within
    a bound (APPCI_QUIT_BOUND_S, 10 s after the last confirm key), the app log has no 'Failed to send Shutdown', and no shell or
    streaming loop of the profile outlives the app. Repeated APPCI_QUIT_REPEAT times (the original failure was 1 in 3)."""
    need_linux_app(ctx, SKIP_QUIT_WHY % "quit_24_tabs")
    bound = float(os.environ.get("APPCI_QUIT_BOUND_S", "10"))
    ntabs = int(os.environ.get("APPCI_QUIT_TABS", "24"))
    reps = int(os.environ.get("APPCI_QUIT_REPEAT", "2"))
    base_procs = descendants_cwd_under(ctx.prof.home) | {os.getpid()}
    try:
        for rep in range(reps):
            tag = "q24_%d" % rep
            for i in range(ntabs):
                if i:
                    ctx.be.key(NEWTAB)
                    time.sleep(0.4)
                ctx.be.type("while :; do echo q24_%d_%d_$RANDOM; sleep 0.05; done" % (rep, i), 10, enter=True)
                time.sleep(0.15)
            ctx.be.key(NEWWIN)
            time.sleep(2.5)
            ctx.shot("%s_open" % tag)
            r.check(ctx.app.alive(), "%s: app died while opening %d streaming tabs + a window" % (tag, ntabs))
            if not ctx.app.alive():
                return
            r.metric("%s_windows" % tag, len(ctx.be.windows()))
            off = ctx.log_size()
            q = quit_app(ctx, r, bound, tag)
            r.check(q["exited"], "%s: still alive %.1f s after the last quit key (bound %.0f s) with %d tabs (#1410): %r" % (tag, bound, bound, ntabs, q))
            r.check("Failed to send Shutdown" not in ctx.log_since(off), "%s: 'Failed to send Shutdown' in the app log (#1410)" % tag)
            keep_log(ctx, tag)
            orphans = []
            if q["exited"]:
                ctx.until(lambda: not (descendants_cwd_under(ctx.prof.home) - base_procs), 8)
                orphans = sorted(descendants_cwd_under(ctx.prof.home) - base_procs)
                r.metric("%s_orphans" % tag, len(orphans))
                r.check(not orphans, "%s: %d process(es) with the profile as cwd outlived the app 8 s (pids %s)" % (tag, len(orphans), orphans[:10]))
            if not q["exited"] or orphans:
                return
            start_app(ctx, r, tag + "_relaunch")
    finally:
        fresh_app(ctx, r, "q24")


# ------------------------------------------------------------------ floods
# r18e4: a fixed line count is a guess about the machine's output rate (macOS finished 2e6 `yes` lines / 8e6 `seq` lines before the typing did, so three
# flood cases tested nothing). The producers below run until the CASE touches a stop file (after the typing / the checks), so the flood provably outlasts
# the typing on every platform; the "flood ended before the typing finished" self-checks stay as the proof.
def endless_flood(kind, stop_f, min_lines=0, min_s=0):
    """shell text (bash/zsh) that floods the pane until `stop_f` exists (and at least min_lines lines / min_s seconds have passed). kind: 'yes' | 'seq'"""
    chunk = "yes | head -n 200000" if kind == "yes" else "seq 1 500000"
    n = 200000 if kind == "yes" else 500000
    return ("__s=$SECONDS; __i=0; while [ ! -e %s ] || [ $__i -lt %d ] || [ $((SECONDS-__s)) -lt %d ]; do %s; __i=$((__i+%d)); done" % (stop_f, min_lines, min_s, chunk, n))


def touch(ctx, name):
    open(os.path.join(ctx.W, name), "w").close()


def flood_once(ctx, r, name, gen, n_expect_s):
    """flood command gen(stop_file) (runs until the case touches the file) in the pane; a line typed during it must run once, after it, exactly as typed"""
    now = NOW(ctx)
    end_f, typed_f, eff_f = "fl_end_%s.txt" % name, "fl_typed_%s.txt" % name, "fl_%s.txt" % name
    stop_f = "fl_stop_%s.txt" % name
    ctx.rm(end_f, typed_f, eff_f, stop_f)
    tok = ctx.tok(8).lower()
    flood_cmd = "%s; %s >> %s" % (gen(stop_f), now, end_f)
    typed_cmd = "%s >> %s; echo %s >> %s" % (now, typed_f, tok, eff_f)
    mark = ctx.cmd_mark()
    ctx.be.type(flood_cmd, 12, enter=True)
    ctx.until(lambda: ctx.cmds_since(mark), 10)
    time.sleep(0.8)                                    # the flood is running now
    p0 = ctx.prompts()
    t_type0 = time.time()
    ctx.be.type(typed_cmd, ctx.rng.randint(10, 25), enter=True)
    t_type1 = time.time()
    time.sleep(1.0)                                    # the flood was running through the whole typing; now let it end
    touch(ctx, stop_f)
    ctx.until(lambda: ctx.lines(eff_f), 300 if ctx.scale >= 1 else 60, step=0.25)
    time.sleep(0.8)
    ctx.shot("flood_%s_done" % name)
    ends = [float(x) for x in ctx.lines(end_f) if re.fullmatch(r"\d+\.\d+", x)]
    typed = [float(x) for x in ctx.lines(typed_f) if re.fullmatch(r"\d+\.\d+", x)]
    r.check(ctx.lines(eff_f) == [tok], "%s: the line typed during the flood ran %d time(s): file %r, expected [%r]" % (name, len(ctx.lines(eff_f)), ctx.lines(eff_f), tok))
    if ends and typed:
        dur = ends[0] - t_type1
        r.metric("%s_flood_remaining_after_typing_s" % name, round(dur, 2))
        r.check(typed[0] >= ends[0] - 0.02, "%s: the typed line ran %.2f s BEFORE the flood ended (typed at %.3f, flood end %.3f)" % (name, ends[0] - typed[0], typed[0], ends[0]))
        r.check(ends[0] > t_type1, "%s: the flood ended (%.2f s) before the typing finished: not a flood test (the producer should run until the case stops it)" % (name, ends[0] - t_type1))
    else:
        r.check(False, "%s: flood end / typed timestamps missing (end %r typed %r)" % (name, ends, typed))
    got = ctx.cmds_since(mark)
    r.check(got == [flood_cmd, typed_cmd], "%s: shell received %r, expected exactly [flood, typed line]" % (name, got))
    ctx.reset()


@case("flood_typed_line", order=60, gui=True)
def c_flood_typed(ctx, r):
    """a large output flood (`yes | head -n 2e6`, `seq 1 5e6`) while a line is typed at 10-25 ms/key + Enter: the typed line runs exactly
    once, exactly as typed, and only after the flood (shell-side timestamps), nothing else reaches the shell"""
    flood_once(ctx, r, "yes", lambda f: endless_flood("yes", f, ctx.count(2_000_000), 4), 0)
    flood_once(ctx, r, "seq", lambda f: endless_flood("seq", f, ctx.count(5_000_000), 4), 0)


@case("flood_typing_latency", order=61, gui=True)
def c_flood_lat(ctx, r):
    """tab A floods, tab B (active) reads raw keys at the pty (ts_reader): 80 chars typed at 15 ms/key arrive complete, in order, and
    the arrival-gap distribution is recorded. Verdicts: loss/order exact; stall = max gap > APPCI_FLOOD_STALL_MS (default 1000).
    No reference terminal is measured here, so p50/p95 are metrics, not a pass/fail against a reference."""
    now = NOW(ctx)
    ctx.rm("fla_end.txt", "flt.txt", "fla_stop.txt")
    mark = ctx.cmd_mark()
    ctx.be.type("%s; %s >> fla_end.txt" % (endless_flood("seq", "fla_stop.txt", ctx.count(8_000_000), 6), now), 12, enter=True)
    ctx.until(lambda: ctx.cmds_since(mark), 10)
    time.sleep(1.0)
    ctx.be.guard()
    ctx.be.key(NEWTAB)
    time.sleep(1.0)
    ctx.be.guard()
    n = 80
    ctx.be.type("python3 %s %s/flt.txt %d" % (os.path.join(os.path.dirname(os.path.abspath(__file__)), "ts_reader.py"), ctx.W, n), 12, enter=True)
    time.sleep(2.5)
    still = not os.path.exists(os.path.join(ctx.W, "fla_end.txt"))
    text = "".join(chr(97 + i % 26) for i in range(n))
    t0 = time.time()
    ctx.be.type(text, 15)
    out = os.path.join(ctx.W, "flt.txt")
    ctx.until(lambda: os.path.exists(out), 30)
    t1 = time.time()
    still_after = not os.path.exists(os.path.join(ctx.W, "fla_end.txt"))
    ctx.shot("flood_lat")
    touch(ctx, "fla_stop.txt")                         # the flood ran through the whole typing; let tab A finish
    r.check(still and still_after, "the flood in tab A ended before the typing finished (before=%s, after=%s): the producer should run until the case stops it" % (still, still_after))
    try:
        rows = [l.split() for l in open(out)]
    except FileNotFoundError:
        r.check(False, "ts_reader wrote nothing within 30 s: keys typed into the quiet tab never arrived while the other tab flooded")
        return
    got = "".join(chr(int(a)) for a, _ in rows)
    r.check(got == text, "keys typed during a flood in another tab: got %d/%d chars, first difference at %s" % (
        len(got), n, next((i for i, (a, b) in enumerate(zip(got, text)) if a != b), min(len(got), n))))
    ts = [float(b) for _, b in rows]
    gaps = [(b - a) * 1000 for a, b in zip(ts, ts[1:])]
    r.metric("flood_gap_ms", stats(gaps))
    r.metric("flood_typing_total_s", round(t1 - t0, 2))
    lim = float(os.environ.get("APPCI_FLOOD_STALL_MS", "1000"))
    if gaps:
        r.check(max(gaps) <= lim, "typing stalled %.0f ms (> %.0f ms) at char %d while another tab flooded" % (max(gaps), lim, gaps.index(max(gaps))))
        r.note("no reference terminal in this case: gap p50 %.1f / p95 %.1f ms are recorded, not judged against one" % (statistics.median(gaps), cases.pct(gaps, 0.95)))
    ctx.be.key("ctrl+c")                                # tab B: leave ts_reader if it is still waiting
    close_tab_best_effort(ctx, r)
    time.sleep(0.8)
    ctx.until(lambda: os.path.exists(os.path.join(ctx.W, "fla_end.txt")), 300 if ctx.scale >= 1 else 60, step=0.5)


@case("flood_scrollback_find", order=62, gui=True)
def c_flood_find(ctx, r):
    """MARKER is the output of its own command, printed BEFORE the flood (its own block: a block keeps at most 50,000 rows); after the flood, Find (ctrl+shift+f; Cmd-F on macOS) for it must bring it on screen (OCR of the
    screenshot; before the Find the marker is NOT on screen: control). Needs tesseract (package tesseract-ocr)."""
    if ctx.ocr("find_probe") is None:
        raise cases.Skip("no OCR engine (install tesseract-ocr): the marker can only be verified on the screenshot")
    marker = "".join(ctx.rng.choice("ABCDEFGHJKLMNPQRSTUVWXYZ") for _ in range(14))
    n = ctx.count(300_000)
    now = NOW(ctx)
    ctx.rm("ff_end.txt", "ff_stop.txt")
    touch(ctx, "ff_stop.txt")       # the producer runs until n lines AND 3 s have passed (a fast machine used to finish 300k lines before the screen had scrolled)
    mark = ctx.cmd_mark()
    # r18u8 (Mac): Phosphor keeps the running command's text as the block header, so a marker typed in the command line stays visible however far the output
    # scrolls and the control "marker not on screen" could never hold. r18u13 (Mac): splitting it with an empty string (`ABC""DEF`) was not enough, OCR
    # drops the thin quotes and read the header as the joined marker. The command carries the marker REVERSED and `rev` prints it: the header (and
    # the tab title) never contain the marker in any OCR reading, so only the OUTPUT line can match the control and the Find.
    # r18u23 (Mac, both modes): the marker used to be the first line of the SAME block as the flood. The app keeps at most terminal.maximum_grid_size
    # (default 50,000) rows per block, so the first line of a 500,000-line block is evicted: Find could never find it. The r18u19 "pass" was OCR reading the
    # marker in the Find FIELD (shots/terminal_find_after.png of that run: the output had not scrolled at all), and r18u22 failed only because OCR missed the
    # field. The marker is now its own command and block, printed BEFORE the flood and proven to have run (the shell's rec.log has the line; an Enter the
    # injector lost is retyped, noted) and the flood is the next block; Find must bring the older block's output line on screen.
    marker_cmd = "echo %s | rev" % marker[::-1]
    for attempt in range(3):
        m0 = ctx.cmd_mark()
        ctx.be.type(marker_cmd, 12, enter=True)
        if ctx.until(lambda: marker_cmd in ctx.cmds_since(m0), 10, step=0.1):
            break
        r.note("flood_scrollback_find: the marker command did not reach the shell within 10 s (attempt %d of 3: its Enter was lost before the app or the shell was busy); retyping it after a Ctrl-C/Ctrl-U reset" % (attempt + 1))
        ctx.reset()
    r.check(marker_cmd in ctx.cmds_since(mark), "the marker command never ran in the shell, so the Find below has nothing to find: %r" % ctx.cmds_since(mark))
    time.sleep(1.0)
    ctx.be.type("%s; %s >> ff_end.txt" % (endless_flood("seq", "ff_stop.txt", n, 3), now), 12, enter=True)
    ctx.until(lambda: os.path.exists(os.path.join(ctx.W, "ff_end.txt")), 200, step=0.5)
    time.sleep(1.0)
    r.check(os.path.exists(os.path.join(ctx.W, "ff_end.txt")), "the flood did not finish")
    before = ctx.ocr("find_before") or ""
    on_screen_before = marker in before.replace(" ", "").upper()
    r.metric("marker_visible_before_find", on_screen_before)
    r.check(not on_screen_before, "control: the marker is already visible before the Find (the flood did not scroll it away; the producer runs >= n lines and >= 3 s)")
    ctx.be.guard()
    # r18u8 (Mac): ctrl+shift+f did nothing there and the marker + Enter were typed into the shell. The terminal Find is `terminal:find` =
    # cmd_or_ctrl_shift("f") (terminal/view/init.rs), which is Cmd-F on macOS (util/bindings.rs: cmd-{key}), ctrl-shift-F elsewhere. r18u13 (Mac): the
    # r18u8 fix sent Cmd-Shift-F, which is the workspace's GLOBAL SEARCH (workspace/mod.rs OpenGlobalSearch, mac binding cmd-shift-F): the app log has
    # "GlobalSearch: starting warp_ripgrep CLI search with pattern=<marker>", the panel kept the focus after Escape, and the next case's `ssh fake` +
    # Enter went into that search field too (remote_heredoc_paste "no remote shell prompt", both modes).
    ctx.be.key(os.environ.get("APPCI_KEY_FIND", "cmd+f" if ctx.be.name == "mac" else "ctrl+shift+f"))
    time.sleep(0.8)
    ctx.be.type(marker, 20, enter=True)
    time.sleep(1.5)
    after = ctx.ocr("find_after") or ""
    after_flat = after.replace(" ", "").upper()
    seen = marker in after_flat
    r.metric("marker_visible_after_find", seen)
    # The marker is also typed in the Find field, so seeing it proves nothing about the output: the view must have SCROLLED. The flood's last lines
    # (499999 / 500000) are on screen before the Find; after a Find that went to the older block they are not.
    tail = ("500000", "499999")
    tail_before = any(t in before for t in tail)
    tail_after = any(t in after for t in tail)
    r.metric("tail_visible_before_find", tail_before)
    r.metric("tail_visible_after_find", tail_after)
    scrolled = not (tail_before and tail_after)
    if not tail_before:
        r.note("flood_scrollback_find: OCR did not read the flood's last lines before the Find, so the scroll cannot be checked from the screenshot (the marker check stands alone)")
    r.check(seen and scrolled, "Find for the marker printed before a %d-line flood did not bring it on screen: marker read=%s, view scrolled away from the flood's tail=%s (see shots/%s_find_after.png)" % (n, seen, scrolled, ctx.mode))
    ctx.be.key("Escape")
    time.sleep(0.3)
    got = ctx.cmds_since(mark)
    r.check(len(got) == 2, "the Find typed into the shell (expected only the marker and flood commands): %r" % got)


# ------------------------------------------------------------------ code editor
def open_in_app(ctx, path):
    """open `path` in the code editor the way a desktop file-open does: `<bin> file://<path>` on the app's own D-Bus session, which
    the running instance receives (release builds forward startup args). The display is removed from the opener's env so that, if
    the build did not forward, a second instance cannot open a window; it is killed after 30 s (exact pid) and reported."""
    if ctx.win:             # #1544: the Windows named-pipe hand-over (cases.open_file_in_windows_app)
        err = cases.open_file_in_windows_app(ctx, path)
        time.sleep(2.0)
        return err is None, err or "windows hand-over: phosphor.exe %s" % cases.windows_file_url(path)
    pid = ctx.app.pid()
    env = {}
    for kv in open("/proc/%d/environ" % pid, "rb").read().split(b"\0"):
        if b"=" in kv:
            k, v = kv.split(b"=", 1)
            env[k.decode(errors="replace")] = v.decode(errors="replace")
    for k in ("DISPLAY", "WAYLAND_DISPLAY"):
        env.pop(k, None)
    if not env.get("DBUS_SESSION_BUS_ADDRESS"):
        raise cases.Skip("the app's environment has no DBUS_SESSION_BUS_ADDRESS (cannot forward a file open)")
    p = subprocess.Popen([ctx.app.bin, "file://" + path], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    try:
        rc = p.wait(timeout=30)
    except subprocess.TimeoutExpired:
        p.kill()
        p.wait()
        return False, "the opener process did not exit in 30 s (this build does not forward startup args to the running instance?)"
    time.sleep(2.0)
    return True, "opener exit code %s" % rc


EDIT_ORIG = "alpha line one\nsecond line éü café\n\ttabbed third\nlast line without much\n"


def editor_prepare(ctx, r, name):
    if ctx.be.name == "mac" or plat().startswith("macos"):
        ctx.skip_reason = SKIP_EDITOR_WHY
        raise cases.Skip("%s (this platform: %s; cb_editor_mac covers the macOS editor)" % (SKIP_EDITOR_WHY, plat()))
    if not ctx.win:
        need_linux_app(ctx, SKIP_EDITOR_WHY)
    d = os.path.join(ctx.W, "ed")
    os.makedirs(d, exist_ok=True)
    path = os.path.join(d, name)
    with open(path, "wb") as f:
        f.write(EDIT_ORIG.encode())
    return path


def disk(path, tries=40):
    """the file's current text. r18u8 (Windows H15): reading while the editor is mid-save raised PermissionError (the editor holds the file for the write) and the
    uncaught exception ended editor_edit_save with the editor pane still open, wedging the pane for the cases after it. A sharing violation is retried for ~2 s;
    only a lasting one propagates."""
    for i in range(tries):
        try:
            return open(path, "rb").read().decode(errors="replace")
        except PermissionError:
            if i == tries - 1:
                raise
            time.sleep(0.05)


def editor_open(ctx, r, path, tag):
    ctx.be.guard()
    ok, msg = open_in_app(ctx, path)
    r.check(ok, "%s: open failed: %s" % (tag, msg))
    if ok:
        r.note("%s: %s" % (tag, msg))
    time.sleep(1.0)
    ctx.shot("editor_%s_open" % tag)
    return ok


@case("editor_edit_save", order=70, gui=True)
def c_editor_edit(ctx, r):
    try:
        return _editor_edit_save(ctx, r)
    except cases.Skip:
        raise
    except Exception:
        # r18u8 (H15): a harness exception must not leave the editor pane open in front of the shell (every later case then typed into the editor: pane_wedged_after_*)
        try:
            ctx.be.guard()
            ctx.be.key(CLOSE_PANE or "ctrl+shift+w")
            time.sleep(1.0)
        except Exception:
            pass
        raise


def _editor_edit_save(ctx, r):
    """open a file in the code editor, type an edit at 10-25 ms/key, save (Ctrl-S), verify the file on disk byte-exact; undo x N + save =
    original, redo x N + save = edited; close the pane, change the file externally, reopen: the editor shows the CURRENT content (a second
    edit lands on top of it, verified on disk). Nothing may reach the shell (no Enter typed; rec.log unchanged)."""
    path = editor_prepare(ctx, r, "edit_me_%s.txt" % ctx.mode)
    mark = ctx.cmd_mark()
    if not editor_open(ctx, r, path, "1"):
        return
    t1 = "EDIT_%s_" % ctx.tok(6)
    ctx.be.key("ctrl+Home")
    ctx.be.type(t1, ctx.rng.randint(10, 25))
    time.sleep(0.4)
    ctx.be.key("ctrl+s")
    ctx.until(lambda: disk(path) != EDIT_ORIG, 6)
    ctx.shot("editor_1_saved")
    r.check(disk(path) == t1 + EDIT_ORIG, "save: disk has %r, expected %r (bytes %d vs %d); the editor may not have had focus (see shots)" % (
        disk(path)[:80], (t1 + EDIT_ORIG)[:80], len(disk(path).encode()), len((t1 + EDIT_ORIG).encode())))
    r.check(open(path, "rb").read() == (t1 + EDIT_ORIG).encode(), "save: not byte-exact (encoding/line-ending/BOM change)")
    # undo / redo (typing may be one undo step per char or merged: press more than needed, the stack ends at the original)
    ctx.be.key("ctrl+z", repeat=len(t1) + 8, delay_ms=40)
    time.sleep(0.4)
    ctx.be.key("ctrl+s")
    ctx.until(lambda: disk(path) == EDIT_ORIG, 6)
    r.check(disk(path) == EDIT_ORIG, "undo x%d + save: disk has %r, expected the original" % (len(t1) + 8, disk(path)[:80]))
    ctx.be.key("ctrl+shift+z", repeat=len(t1) + 8, delay_ms=40)
    time.sleep(0.4)
    ctx.be.key("ctrl+s")
    ctx.until(lambda: disk(path) == t1 + EDIT_ORIG, 6)
    r.check(disk(path) == t1 + EDIT_ORIG, "redo x%d + save: disk has %r, expected %r" % (len(t1) + 8, disk(path)[:80], (t1 + EDIT_ORIG)[:80]))
    # close the pane (saved: no dialog), change the file outside, reopen
    ctx.be.guard()
    ctx.be.key(CLOSE_PANE)
    time.sleep(1.0)
    ctx.shot("editor_1_closed")
    ext = disk(path) + "EXTERNAL_%s\n" % ctx.tok(4)
    with open(path, "wb") as f:
        f.write(ext.encode())
    if not editor_open(ctx, r, path, "2"):
        return
    t2 = "SECOND_%s_" % ctx.tok(6)
    ctx.be.key("ctrl+Home")
    ctx.be.type(t2, ctx.rng.randint(10, 25))
    time.sleep(0.4)
    ctx.be.key("ctrl+s")
    ctx.until(lambda: disk(path) != ext, 6)
    r.check(disk(path) == t2 + ext, "reopen+edit+save: disk has %r, expected %r (the reopened editor showed stale or wrong content)" % (disk(path)[-120:], (t2 + ext)[-120:]))
    ctx.be.key(CLOSE_PANE)
    time.sleep(1.0)
    got = ctx.cmds_since(mark)
    r.check(not got, "keys meant for the editor reached the shell: %r" % got)


@case("editor_paste", order=71, gui=True)
def c_editor_paste(ctx, r):
    """#1409: Ctrl-V pastes in the code editor on Linux (and Ctrl-Shift-V too): clipboard text lands in the buffer, saved file byte-exact"""
    path = editor_prepare(ctx, r, "paste_me_%s.txt" % ctx.mode)
    mark = ctx.cmd_mark()
    if not editor_open(ctx, r, path, "p"):
        return
    a, b = "PASTE_CTRLV_%s" % ctx.tok(6), "PASTE_CTRLSHIFTV_%s" % ctx.tok(6)
    ctx.be.key("ctrl+Home")
    ctx.be.clip_set(a)
    ctx.be.key("ctrl+v")
    time.sleep(0.6)
    ctx.be.key("ctrl+s")
    ctx.until(lambda: disk(path) != EDIT_ORIG, 6)
    r.check(disk(path) == a + EDIT_ORIG, "Ctrl-V (#1409): disk has %r, expected %r after paste+save" % (disk(path)[:80], (a + EDIT_ORIG)[:80]))
    ctx.be.clip_set(b)
    ctx.be.key("ctrl+Home")
    ctx.be.key("ctrl+shift+v")
    time.sleep(0.6)
    ctx.be.key("ctrl+s")
    want = b + a + EDIT_ORIG
    ctx.until(lambda: disk(path) == want, 6)
    r.check(disk(path) == want, "Ctrl-Shift-V: disk has %r, expected %r" % (disk(path)[:100], want[:100]))
    ctx.shot("editor_pasted")
    ctx.be.key(CLOSE_PANE)
    time.sleep(1.0)
    got = ctx.cmds_since(mark)
    r.check(not got, "paste meant for the editor reached the shell: %r" % got)


# ------------------------------------------------------------------ soak
def proc_set(ctx):
    """pids of the app: every process running the same executable as the app (the main process and its terminal-server helper)"""
    pid = ctx.app.pid()
    try:
        exe = os.readlink("/proc/%d/exe" % pid)
    except OSError:
        return [pid] if pid else []
    out = []
    for p in os.listdir("/proc"):
        if p.isdigit():
            try:
                if os.readlink("/proc/%s/exe" % p) == exe:
                    out.append(int(p))
            except OSError:
                pass
    return out


def sample(ctx):
    """(rss_kib, cpu_ticks) summed over the app's processes (Linux); the platform's single-process figure elsewhere"""
    if ctx.be.name == "mac":
        t, k = ctx.app.cpu_ticks()
        return k, t
    rss = ticks = 0
    for p in proc_set(ctx):
        try:
            f = open("/proc/%d/stat" % p).read().rsplit(")", 1)[1].split()
            ticks += int(f[11]) + int(f[12])
            rss += int(open("/proc/%d/statm" % p).read().split()[1]) * os.sysconf("SC_PAGE_SIZE") // 1024
        except OSError:
            pass
    return rss, ticks


def growth_verdict(rss_mb):
    """THE RULE. Drop the first max(3, 10%) samples (warmup). Median of each window of 5 consecutive samples. The series is
    'monotonically growing' when it has >= 3 windows and each window median is >= 98% of the previous one. FAIL when it is monotonically
    growing AND last-window median - first-window median > max(APPCI_SOAK_GROW_MB (150), 25% of the first window median).
    Independently FAIL when any sample exceeds APPCI_SOAK_RSS_MAX_MB (4096). Returns (verdict, text)."""
    cap = float(os.environ.get("APPCI_SOAK_RSS_MAX_MB", "4096"))
    grow = float(os.environ.get("APPCI_SOAK_GROW_MB", "150"))
    if rss_mb and max(rss_mb) > cap:
        return "fail", "RSS reached %.0f MB (> %.0f MB cap)" % (max(rss_mb), cap)
    xs = rss_mb[max(3, len(rss_mb) // 10):]
    w = [statistics.median(xs[i:i + 5]) for i in range(0, len(xs) - 4, 5)]
    if len(w) < 3:
        return "short", "only %d window(s) of 5 samples after warmup: too short for a growth verdict" % len(w)
    mono = all(b >= a * 0.98 for a, b in zip(w, w[1:]))
    g = w[-1] - w[0]
    lim = max(grow, 0.25 * w[0])
    if mono and g > lim:
        return "fail", "RSS grew monotonically %.0f -> %.0f MB (+%.0f MB > %.0f MB) over %d windows: %s" % (w[0], w[-1], g, lim, len(w), [round(x) for x in w])
    return "pass", "window medians %s MB, monotonic=%s, growth %+.0f MB (limit %.0f MB)" % ([round(x) for x in w], mono, g, lim)


def idle_cpu(ctx, secs=30):
    time.sleep(2)
    _, a = sample(ctx)
    t = time.time()
    time.sleep(secs)
    _, b = sample(ctx)
    return (b - a) / os.sysconf("SC_CLK_TCK") / (time.time() - t) * 100


@case("soak", order=89, gui=True)
def c_soak(ctx, r):
    """FULL TIER ONLY. ~APPCI_SOAK_MIN (60) minutes of mixed use: every minute typed commands (10-25 ms/key), an instant-Enter paste, a
    tab opened/used/closed, every 5th minute a 300k-line flood, every 10th (agent mode with a CI Ollama only) an agent prompt in a throw-away
    tab. Every minute the RSS and CPU of the app's processes are sampled; the effect file proves every typed command arrived exactly.
    Verdicts: RSS growth rule (growth_verdict docstring), idle CPU at the end <= max(5 %, start + 3 points), all commands exact."""
    if os.environ.get("APPCI_TIER") == "smoke":
        raise cases.Skip("soak is full-tier only")
    if ctx.app is None or not hasattr(ctx.app, "pid"):
        raise cases.Skip("needs the real app (not the sim backend)")
    if not CLOSE_PANE:
        raise cases.Skip(NO_CLOSE_WHY)
    mins = float(os.environ.get("APPCI_SOAK_MIN", "60"))
    now = NOW(ctx)
    ctx.rm("soak.txt")
    ctx.reset()
    cpu0 = idle_cpu(ctx, 20)
    r.metric("idle_cpu_start_pct", round(cpu0, 2))
    rss_s, cpu_s, exp = [], [], []
    csv = open(os.path.join(ctx.run, "soak_samples.csv"), "w")
    csv.write("minute,rss_mb,cpu_pct\n")
    last_t, last_ticks = time.time(), sample(ctx)[1]
    tclk = os.sysconf("SC_CLK_TCK")
    m = 0
    t_start = time.time()
    while time.time() - t_start < mins * 60:
        m += 1
        t_min = time.time()
        if not ctx.app.alive():
            r.check(False, "soak: the app died in minute %d" % m)
            break
        ctx.be.guard()
        for i in range(8):                              # typed commands, exact
            tok = "s%d_%d_%s" % (m, i, ctx.tok(4).lower())
            exp.append(tok)
            ctx.be.type("echo %s >> soak.txt" % tok, ctx.rng.randint(10, 25), enter=True)
            time.sleep(0.2)
        tok = "p%d_%s" % (m, ctx.tok(4).lower())            # instant-Enter paste
        exp.append(tok)
        ctx.be.clip_set("echo %s >> soak.txt" % tok)
        ctx.be.paste(enter=True)
        time.sleep(1.0)
        ctx.be.guard()
        ctx.be.key(NEWTAB)                                  # a tab opened, used, closed
        time.sleep(0.8)
        tok = "t%d_%s" % (m, ctx.tok(4).lower())
        exp.append(tok)
        ctx.be.type("echo %s >> soak.txt" % tok, 12, enter=True)
        ctx.until(lambda: tok in ctx.lines("soak.txt"), 8)
        if m % 10 == 0 and ctx.mode == "agent" and ctx.ollama:
            ctx.be.type("reply with just the word ok", 15, enter=True)      # agent prompt, in this throw-away tab only
            time.sleep(25)
            ctx.be.key("Escape")
        ctx.be.key(CLOSE_PANE)
        time.sleep(0.8)
        if m % 5 == 0:
            ctx.be.guard()
            ctx.be.type("seq 1 300000", 12, enter=True)
            time.sleep(8)
        tok = "z%d_%s" % (m, ctx.tok(4).lower())          # sentinel back in the main tab: keys still go where they should
        exp.append(tok)
        ctx.be.type("echo %s >> soak.txt" % tok, 12, enter=True)
        if not ctx.until(lambda: tok in ctx.lines("soak.txt"), 30):
            ctx.shot("soak_min%d_stuck" % m)
            r.check(False, "soak minute %d: sentinel %s never reached the shell (a dialog or a stuck tab ate the keys)" % (m, tok))
            break
        # sample at the end of the activity, then idle to the minute boundary
        rss, ticks = sample(ctx)
        t = time.time()
        cpu = (ticks - last_ticks) / tclk / (t - last_t) * 100
        last_t, last_ticks = t, ticks
        rss_s.append(rss / 1024.0)
        cpu_s.append(cpu)
        csv.write("%d,%.1f,%.1f\n" % (m, rss / 1024.0, cpu))
        csv.flush()
        time.sleep(max(0.0, 60 - (time.time() - t_min)))
    csv.close()
    r.metric("soak_minutes", m)
    r.metric("rss_mb_first_last_max", [round(rss_s[0], 1), round(rss_s[-1], 1), round(max(rss_s), 1)] if rss_s else None)
    r.metric("cpu_pct_p50_p95_max", [round(statistics.median(cpu_s), 1), round(cases.pct(cpu_s, 0.95), 1), round(max(cpu_s), 1)] if cpu_s else None)
    ctx.reset()
    got = ctx.lines("soak.txt")
    r.check(got == exp, "soak: %d/%d typed/pasted commands arrived exactly (first difference %r)" % (
        sum(a == b for a, b in zip(got, exp)), len(exp), next(((a, b) for a, b in zip(exp + [None], got + [None]) if a != b), None)))
    v, text = growth_verdict(rss_s)
    r.metric("rss_growth_verdict", v)
    if v == "fail":
        r.check(False, "soak: " + text)
    elif v == "short":
        r.note("soak: " + text)
    else:
        r.note("soak: " + text)
    cpu1 = idle_cpu(ctx, 30)
    r.metric("idle_cpu_end_pct", round(cpu1, 2))
    lim = max(float(os.environ.get("APPCI_SOAK_IDLE_CPU_PCT", "5")), cpu0 + 3)
    r.check(cpu1 <= lim, "soak: idle CPU %.1f %% at the end exceeds max(%s %%, start %.1f %% + 3) = %.1f %%" % (cpu1, os.environ.get("APPCI_SOAK_IDLE_CPU_PCT", "5"), cpu0, lim))
    ctx.shot("soak_end")
