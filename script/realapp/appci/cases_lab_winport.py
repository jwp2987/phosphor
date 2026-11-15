# appci/cases_lab_winport.py: the ssh cases the Windows app job used to play against the harness's FAKE sshd (fakesshd_win.py), as lab families, so the same
# app behaviour is judged against the REAL lab sshd on all three platforms (owner rule 2026-11-13: ssh / remote tests on Linux, macOS AND Windows run against the
# lab, never against fakesshd_win.py, which cannot give the remote shell a real pty: #2007, #2051, #1992). Registered with @lab_case (cases_lab.py), one case per
# arm: lab_<family>.<shell>.<lan|150>.<nohelper|helper>. On Windows they run in the Git Bash pane (winmap.lab_skip_reason) with the one-connection managed ssh
# switched on by cases_lab._runner (R.in_msys_managed_ssh), as the old cases did for themselves.
#
#   old case (fake sshd)                                                  lab family
#   ssh_gitbash_typeahead_managed, ssh_connect_typing(_wrapper),          lab_connect_typeahead   (a line typed blind behind `ssh <host>`: exactly once,
#   ssh_enter_then_immediate_command                                                              remotely, in order, never locally; the wrapper's verdict)
#   ssh_gitbash_managed, ssh_gitbash_bootstrap_in_command                 lab_managed_connect     (nothing typed while connecting: the managed session's
#                                                                                                 stages, verdict, bootstrap in the command, a line once)
#   ssh_gitbash_kill_switch                                               lab_msys_ssh_switch     (Windows Git Bash only: default and =0 are plain ssh)
#   remote_tab_no_helper, remote_tab_enter_no_helper                      lab_tab_no_helper (+ lab_tab_remote_dir, which already judges unique-prefix Tab,
#                                                                                                 the menu and Tab + fast Enter against the same host)
#   remote_session_legacy                                                 lab_remote_session      (typing, subshells, a raw alt-screen program, Ctrl-C,
#                                                                                                 exit and routing both ways)
#   ssh_password_prompt                                                   lab_password_prompt     (existing family, same body)
# NOT moved (no lab family, still on the fake sshd, listed in docs/testing.md): the nested-ssh cases (the lab container has no key for a second hop), remote_session_ext,
# remote_heredoc_paste, latency_enter_ssh, osc52_write_read, grid_snapshot_render, zero_network_startup, agent_fault_disconnect (each uses `ssh fake` for one step).
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402
import cases_caseremote as CR  # noqa: E402
import cases_win_input as W  # noqa: E402
import cases_lab as CL  # noqa: E402
from cases_lab import lab_case, host_of, work, unique, type_pace, rtt_s, connect_or_fail, finish_remote, pwd_append_line, append_line  # noqa: E402

IN_BAND_RE = re.compile(r"candidates from the remote shell in (\d+) ms")        # #1801: the app asked the remote shell itself, in-band


# ------------------------------------------------------------------ pure pieces (selftest)
def wrapper_facts(log):
    """pure: what the app log of ONE connect says about the managed-ssh wrapper: dict(hops=[lines], stages=[accepted one-connection stages in order],
    managed=<the session was managed: the ManagedSSHSessionStarting hook or `verdict managed=true`>, verdict_true, verdict_false, ignored=[sync_ignored /
    verdict_ignored lines], announced, front_refused=[`typeahead_front adopted=false` lines], front_adopted=<number of adopted=true>)"""
    log = log or ""
    hops = [ln for ln in log.splitlines() if "hop=ssh_wrapper" in ln]
    return dict(hops=hops, stages=W._sync_stages(hops), managed=bool(CL.MANAGED_SESSION_RE.search(log)),
                verdict_true=any("verdict managed=true" in ln for ln in hops), verdict_false=any("verdict managed=false" in ln for ln in hops),
                ignored=[ln[-140:] for ln in hops if "sync_ignored" in ln or "verdict_ignored" in ln],
                announced=any("announced" in ln for ln in hops),
                front_refused=[ln[-120:] for ln in hops if "typeahead_front adopted=false" in ln],
                front_adopted=sum(1 for ln in hops if "typeahead_front adopted=true" in ln))


def judge_managed(r, facts, label, win, pinned):
    """the checks every managed connect of these families makes, from wrapper_facts(). win: the Windows Git Bash pane (the one-connection path: its stage hops
    and its `verdict managed=true` line are required); pinned: APPCI_SSH_WRAPPER=1 (the announcement is then required too, #1884)."""
    r.check(facts["managed"], "%s: the session was not managed (no ManagedSSHSessionStarting hook and no `verdict managed=true` in the app log of this connect)" % label)
    if win:
        r.check(facts["stages"] == ["auth", "auth_done", "managed"], "%s: one-connection stages in the app log: %r, want auth, auth_done, managed (a final `plain` is the old #1959 behaviour)" % (label, facts["stages"]))
        r.check(facts["verdict_true"], "%s: the wrapper's verdict was managed=true (round 8: managed=false, #1933)" % label)
    r.check(not facts["ignored"], "%s: a stage or verdict was ignored: %r" % (label, facts["ignored"][:3]))
    if pinned:
        r.check(facts["announced"], "%s: the wrapper announcement was not handled on the connect window (hop ssh_wrapper announced; %d wrapper hops)" % (label, len(facts["hops"])))
    r.check(facts["front_refused"] == [], "%s: the keys drained from the console were not put at the front of the hold (plain fallback): %r" % (label, facts["front_refused"][:2]))


def raw_program_line(path, n, secs=25):
    """pure: the line typed to start the stand-in for a full-screen program (the lab image has no vim and no less): it enters the alternate screen, puts the tty in
    raw mode, stores EXACTLY `n` bytes of input in `path` (under $HOME) and gives the terminal back. `timeout` ends it when the keys never come, so a lost key
    fails the byte comparison instead of leaving the pane in raw mode. `--foreground` is REQUIRED (#2217): without it GNU timeout puts the command in its own
    process group, a background group of the tty, so `head` is stopped by SIGTTIN, reads nothing, and every key lands in the shell's line. Single quotes
    only; no `!` and no backslash that a shell would read."""
    return ("sh -c 'printf \"\\033[?1049h\"; stty raw -echo; timeout --foreground %d head -c %d > \"$HOME/%s\"; stty sane; printf \"\\033[?1049l\"'" % (secs, n, path))


def raw_keys_expected(tok):
    """pure: (the typed pieces, the bytes the raw program must have stored) of the vim-like step: text, Escape, `:wq`, Return (CR in raw mode)"""
    text = "ihello remote " + tok
    return (text, ":wq"), (text + "\x1b" + ":wq" + "\r").encode()


def tab_lines(recs, uid):
    """pure: the `cd` lines of the Tab families out of what the remote shell received (the harness's own probes and the app's generator calls are already gone)"""
    return [c for c in recs if c.startswith("cd ") and ("/phx" + uid) in c]


# ------------------------------------------------------------------ 1. a line typed blind behind `ssh <host>` (ssh_connect_typing, ssh_gitbash_typeahead_managed, ...)
# gaps in seconds between the ssh Enter and the first typed key, with the key pace: (gap, pace_ms or None for 10-25 ms random)
TYPEAHEAD_TRIALS = ((0.0, None), (0.0, 12), (0.0, None), (0.1, 15), (0.2, 15), (0.3, 15))


@lab_case("connect_typeahead", shells=("bash", "zsh"), lat=("lan", "150"), helper=("nohelper", "helper"))
def c_connect_typeahead(ctx, r, arm):
    """The old ssh_connect_typing / ssh_connect_typing_wrapper / ssh_enter_then_immediate_command / ssh_gitbash_typeahead_managed, against the lab host. Six
    trials (the first three with no pause at all, the keys at 10-25 ms or 12 ms with an instant Enter, then gaps of 0.1, 0.2 and 0.3 s): `ssh <host>` + Enter,
    then `echo <tok> >> ~/rl.txt` + Enter. Required: the REMOTE shell received exactly that line (its rec.log, nothing else), the remote effect file holds every
    token exactly once and in typed order, the LOCAL shell received nothing but the ssh lines and wrote no file, the connect was managed (wrapper_facts /
    judge_managed: on the Windows Git Bash pane the stages are auth, auth_done, managed, none ignored, no `typeahead_front adopted=false`), and each trial
    ends with a proven `exit` back to the local prompt."""
    host, rt = host_of(arm), rtt_s(arm)
    pinned = os.environ.get("APPCI_SSH_WRAPPER", "") == "1"
    win = bool(getattr(ctx, "win", False)) and getattr(ctx, "pane", None) == "gitbash"
    ctx.rm("rl.txt")
    CR.rm_remote(ctx, "rl.txt")
    exp, verdicts, miss, adopted = [], [], 0, 0
    base = ctx.shell_count()
    stop = False
    for i, (gap, pace) in enumerate(TYPEAHEAD_TRIALS):
        tok = ctx.tok(6)
        exp.append(tok)
        line = append_line(arm, tok, "~/rl.txt")
        off, r0, lmark, t0 = ctx.log_size(), len(ctx.prof.commands(remote=True)), ctx.cmd_mark(), time.time()
        before = CR.rprompts(ctx)
        ctx.be.type("ssh " + host, 12, enter=True)
        if gap:
            time.sleep(gap)
        ctx.be.type(line, pace if pace is not None else ctx.rng.randint(10, 25), enter=True)
        label = "trial %d (gap %.1f s)" % (i + 1, gap)
        seen = bool(ctx.until(lambda: tok in ctx.lines("rl.txt", remote=True), 30 + 12 * rt))
        if not seen:
            miss += 1
            r.check(False, "%s: the line typed behind `ssh %s` never reached the remote shell. Test host: %s" % (label, host, ctx.ssh_diag()))
        ctx.until(lambda: CR.rprompts(ctx) > before, 20 + 6 * rt, step=0.1)
        time.sleep(1.0 + 2 * rt)
        cases.ssh_trial_note(ctx, r, verdicts, "lab_connect_typeahead " + label, t0, line, r0, effect_seen=seen)
        got = CR.remote_cmds_after(ctx, r0)
        r.check(got == [line], "%s: the remote shell received %r, want exactly [%r]" % (label, got, line))
        local = [c for c in ctx.cmds_since(lmark) if not c.startswith("ssh ")]
        r.check(local == [], "%s: the LOCAL shell received %r" % (label, local))
        facts = wrapper_facts(ctx.log_since(off))
        judge_managed(r, facts, label, win, pinned)
        adopted += 1 if facts["front_adopted"] else 0
        if i == 0:
            ctx.shot("lab_connect_typeahead_remote_prompt")
        if not ctx.leave_remote(r, "lab_connect_typeahead (%s)" % label, "rl.txt", tok, base):
            stop = True
            break
    got = ctx.lines("rl.txt", remote=True)
    lost = sum(1 for t in exp if t not in got)
    rep = cases.loss_report(lost, len(exp))
    r.metric("trials", len(exp))
    r.metric("loss", rep)
    r.note("lab_connect_typeahead loss: " + rep["text"])
    cases.ssh_trial_summary(r, verdicts)
    r.note("typed-during-connect: in %d of %d trials keys were in the console before the session and were held at the front (typeahead_front adopted=true)" % (adopted, len(exp)))
    if not stop:
        r.check(got == exp, "the remote shell received %d/%d typed-ahead commands exactly and in order (missing %d): %r" % (sum(1 for t in exp if t in got), len(exp), miss, got))
        r.check(not ctx.local_lines("rl.txt"), "a typed-ahead command ran LOCALLY: %r" % ctx.local_lines("rl.txt"))
    ctx.reset()


# ------------------------------------------------------------------ 2. nothing typed while connecting (ssh_gitbash_managed, ssh_gitbash_bootstrap_in_command)
@lab_case("managed_connect", shells=("bash",), lat=("lan", "150"), helper=("nohelper",))
def c_managed_connect(ctx, r, arm):
    """The old ssh_gitbash_managed and ssh_gitbash_bootstrap_in_command, against the lab host: `ssh <host>` with nothing typed while it connects. Required: a
    remote prompt, the pane provably REMOTE (one probe), the connect managed (judge_managed; Windows Git Bash: stages auth, auth_done, managed, `verdict
    managed=true`, none ignored), and on the Windows Git Bash pane the remote bash gets its bootstrap INSIDE the remote command (#2051: `hop=bootstrap_in_command
    ... wrote=0`, no single pty write of 4096 bytes or more, no `Bootstrapping failed`); then one line typed at the remote prompt ran once there and never locally, and
    `exit` brings the local prompt back. Not covered: the client-byte count of the Windows fake sshd (the lab keeps none; the app's own write sizes above are the
    app-side half of it)."""
    host, rt = host_of(arm), rtt_s(arm)
    pinned = os.environ.get("APPCI_SSH_WRAPPER", "") == "1"
    win = bool(getattr(ctx, "win", False)) and getattr(ctx, "pane", None) == "gitbash"
    off = ctx.log_size()
    secs = connect_or_fail(ctx, r, arm)
    if secs is None:
        ctx.reset()
        return
    r.metric("managed_connect_s", round(secs, 2))
    time.sleep(1.0 + 2 * rt)
    where = ctx.where_now()      # ONE probe per verdict (round 14)
    if where != "remote":
        r.check(False, "precondition: the pane is not provably in the REMOTE shell after `ssh %s` (probe ran in: %s)" % (host, where))
        ctx.reset()
        return
    log = ctx.log_since(off)
    judge_managed(r, wrapper_facts(log), "managed connect", win, pinned)
    if win:
        how = W.bootstrap_delivery(log)
        r.metric("managed_connect_delivery", how)
        r.check(how["in_command"], "the app log has no `hop=bootstrap_in_command ... wrote=0`: the remote command did not carry the bootstrap (the wrapper fell back to plain ssh, or the app typed it)")
        r.check(how["max_write"] < W.BOOTSTRAP_WRITE_LIMIT, "the app wrote %d bytes to the pty in one write during the connect (limit %d): a bootstrap was typed" % (how["max_write"], W.BOOTSTRAP_WRITE_LIMIT))
        r.check(not how["failed"], "the app gave up on the remote bootstrap (`Bootstrapping failed for shell` in its log)")
    ctx.shot("lab_managed_connect_remote_prompt")
    W._remote_line_once(ctx, r, "managed session")
    if not CR.leave(ctx):
        r.check(False, "not provably remote (probe ran in: %s), `exit` NOT typed" % ctx.left_unproven)
    ctx.reset()


# ------------------------------------------------------------------ 3. the Git Bash pane's switch (ssh_gitbash_kill_switch)
@lab_case("msys_ssh_switch", shells=("bash",), lat=("lan",), helper=("nohelper",))
def c_msys_ssh_switch(ctx, r, arm):
    """The old ssh_gitbash_kill_switch, against the lab host (Windows Git Bash pane only: the other panes have no such switch and SKIP). Two connects, each must be
    the user's PLAIN ssh (no stage hop, `verdict managed=false`, never `managed=true`) with a line typed at the remote prompt running once there: first with
    PHOSPHOR_MSYS_MANAGED_SSH unset (the default every user gets), then with `export PHOSPHOR_MSYS_MANAGED_SSH=0` (the explicit off still means off). cases_lab._runner
    switched the managed path ON for the other families; this one types the unset itself, and the runner's own restore proves the pane's shell afterwards."""
    if not (getattr(ctx, "win", False) and getattr(ctx, "pane", None) == "gitbash"):
        r.skip("Windows Git Bash pane only: the one-connection managed ssh and its switch exist only there (#1933, #2051)")
    host, rt = host_of(arm), rtt_s(arm)
    for what, line in (("the default (variable not set)", CR.msys_managed_ssh_lines(False)), ("PHOSPHOR_MSYS_MANAGED_SSH=0", "export %s=0" % CR.MSYS_MANAGED_SSH)):
        ctx.be.type(line, 12, enter=True)
        time.sleep(0.5)
        off = ctx.log_size()
        secs = connect_or_fail(ctx, r, arm, "%s: " % what)
        if secs is not None:
            time.sleep(1.0 + 2 * rt)
            facts = wrapper_facts(ctx.log_since(off))
            r.check(facts["stages"] == [], "%s: no one-connection stage: %r" % (what, facts["stages"]))
            r.check(facts["verdict_false"], "%s: the wrapper's verdict was the plain fallback (managed=false)" % what)
            r.check(not facts["verdict_true"], "%s: the session must not be managed" % what)
            where = ctx.where_now()
            if where == "remote":
                W._remote_line_once(ctx, r, "plain session, %s" % what)
                if not CR.leave(ctx):
                    r.check(False, "%s: not provably remote (probe ran in: %s), `exit` NOT typed" % (what, ctx.left_unproven))
            else:
                r.check(False, "%s: the pane is not provably remote after `ssh %s` (probe ran in: %s)" % (what, host, where))
        ctx.reset()


# ------------------------------------------------------------------ 4. Tab over ssh without the helper (remote_tab_no_helper, remote_tab_enter_no_helper)
@lab_case("tab_no_helper", shells=("bash", "zsh", "zshc"), lat=("lan", "150"), helper=("nohelper",))
def c_tab_no_helper(ctx, r, arm):
    """The old remote_tab_no_helper and remote_tab_enter_no_helper (#1801), against the lab host, and what lab_tab_remote_dir does not judge. `cd <prefix>` of a
    directory that exists only on the lab host + Tab + (1.5 s later) Enter: the exact command the REMOTE shell received is the completed `cd <dir>/`, the next
    `pwd` there is that directory, the LOCAL shell received nothing, and the app log has `candidates from the remote shell` (the in-band answer, not a
    ControlMaster side command). Then three trials of Tab + Enter 30 ms later with a fresh directory each: the remote shell received exactly the COMPLETED
    line, never the uncompleted prefix. Covered by lab_tab_remote_dir instead: the ambiguous-prefix menu against the shell's own candidates, the perf band of Tab -> insert."""
    lab, u, base, rt = ctx.lab, arm["user"], work(arm), rtt_s(arm)
    uid = unique(ctx)
    fx = "tnh_" + uid
    lab.tree(u, {"%s/phx%s_only/x" % (fx, uid): ""})
    CR.rm_remote(ctx, "tnh_pwd.txt")
    off0 = ctx.log_size()
    if connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    where = ctx.where_now()
    if where != "remote":      # precondition: never judge Tab against a local shell
        r.check(False, "precondition: the pane is not provably in the REMOTE shell after `ssh %s` (probe ran in: %s); Tab not judged" % (host_of(arm), where))
        ctx.reset()
        return
    CR.settle_remote_prompt(ctx)
    # (a) Tab, then an ordinary Enter (remote_tab_no_helper)
    rmark, lmark = len(ctx.prof.commands(remote=True)), ctx.cmd_mark()
    want_dir = "%s%s/phx%s_only" % (base, fx, uid)
    ctx.be.type("cd %s%s/phx%s" % (base, fx, uid), 15)
    ctx.be.key("Tab")
    time.sleep(1.5 + 2 * rt)         # remote Tab has no Tab+Enter wait for a slow Enter: let the in-band answer land first
    ctx.shot("lab_tab_no_helper_completed")
    ctx.be.key("Return")
    got = ctx.until(lambda: tab_lines(CR.remote_cmds_after(ctx, rmark), uid), 10 + 4 * rt)
    r.check(bool(got) and got[0] in ("cd %s/" % want_dir, "cd %s" % want_dir),
            "Tab after the prefix should have completed the REMOTE directory: the remote shell received %r, want %r" % (got, "cd %s/" % want_dir))
    ctx.be.type(pwd_append_line(arm, "%stnh_pwd.txt" % base), 12, enter=True)
    ctx.until(lambda: ctx.remote.lines("tnh_pwd.txt"), 8 + 4 * rt)
    pwd = ctx.remote.lines("tnh_pwd.txt")
    r.check(bool(pwd) and pwd[-1] == want_dir, "after the completed cd the remote cwd should be %s, got %r" % (want_dir, pwd))
    r.check(ctx.cmds_since(lmark) == [], "the LOCAL shell received commands while the remote shell was in front: %r" % ctx.cmds_since(lmark))
    hits = [m for m in (IN_BAND_RE.search(l) for l in ctx.log_since(off0).splitlines()) if m]
    # #2196: zsh WITHOUT compinit answers an in-band Tab with nothing by design (#2174: the app never runs compinit for the user), so only the completer's answer
    # lands there (the completed line below is still judged); bash and zshc (zsh WITH compinit) must have the remote shell's own candidates.
    if CL.native_tab_expectation(arm) == "empty":
        seg = ctx.log_since(off0)
        # #2225: once the session reported compsys missing the app skips the in-band request and asks the completer at once; a Tab before the first
        # Precmd still gets the empty in-band answer (#2174). Either is the completer path; a candidate line from the remote shell is not.
        empty, skipped = CL.native_tab_empty(seg), CL.native_tab_compsys_skips(seg)
        r.metric("zsh_nocompsys_tabs", {"empty_answer": empty, "compsys_skip": skipped})
        r.check(not hits and empty + skipped >= 1, "zsh without compinit must take the completer path (#2174/#2225): %d candidate line(s), %d empty "
                "answer(s), %d compsys-missing skip(s)" % (len(hits), empty, skipped))
        hits = []
    else:
        r.check(bool(hits), "the app log has no 'candidates from the remote shell' line: Tab did not take the in-band path (#1801)")
    if hits:
        r.metric("remote_tab_in_band_ms", int(hits[-1].group(1)))
    # (b) Tab + a fast Enter (remote_tab_enter_no_helper), three fresh directories
    lmark = ctx.cmd_mark()
    for n in range(3):
        uid2 = unique(ctx)
        lab.tree(u, {"%s/phx%s_only/x" % (fx, uid2): ""})
        CR.settle_remote_prompt(ctx)
        rmark = len(ctx.prof.commands(remote=True))
        prefix = "cd %s%s/phx%s" % (base, fx, uid2)
        ctx.be.type(prefix, type_pace(ctx))
        ctx.be.key("Tab")
        time.sleep(0.03)
        ctx.be.key("Return")
        got2 = ctx.until(lambda: tab_lines(CR.remote_cmds_after(ctx, rmark), uid2), 8 + 4 * rt)
        ok, msg = CL.tab_enter_received(got2 or [], "%s%s/phx%s_only" % (base, fx, uid2))
        r.check(ok, "Tab + Enter 30 ms later (trial %d): %s (the uncompleted %r must never run)" % (n + 1, msg, prefix))
        if not got2:
            ctx.be.key("ctrl+c")      # a give-up left the line in the editor: clear it for the next trial
            time.sleep(0.3)
        ctx.be.type("cd %s" % base, 12, enter=True)
        time.sleep(0.5 + 2 * rt)
    r.check(ctx.cmds_since(lmark) == [], "the LOCAL shell received commands while the remote shell was in front: %r" % ctx.cmds_since(lmark))
    finish_remote(ctx, r)


# ------------------------------------------------------------------ 5. a whole remote session (remote_session_legacy)
@lab_case("remote_session", shells=("bash",), lat=("lan", "150"), helper=("nohelper",))
def c_remote_session(ctx, r, arm):
    """The old remote_session_legacy (ssh WITHOUT the remote helper), against the lab host: connect; 12 lines typed at 10-25 ms/key with an instant Enter arrive
    exactly (remote effect file AND the remote shell's rec.log, nothing ran locally); nested bash / bash (no wait) / sh / clean-env bash subshells proven by pid; a
    full-screen stand-in (the lab image has no vim and no less: raw_program_line enters the alternate screen, takes the tty raw and stores the bytes it gets) receives
    text, Escape, `:wq` and Return byte for byte, and a pager-like one ` `, `G`, `q`; the shell is usable afterwards; Ctrl-C reaches a remote `sleep 60` and the
    shell goes on; `exit` returns the LOCAL prompt and a command typed there runs locally only; a second connect runs a remote command remotely only."""
    host, rt = host_of(arm), rtt_s(arm)
    tag = "rs_lab"
    for n in ("_ft", "_sub", "_raw", "_pager", "_cc", "_m", "_less"):
        CR.rm_remote(ctx, tag + n + ".txt")
    ctx.rm(tag + "_back.txt", tag + "_ft.txt")
    secs = connect_or_fail(ctx, r, arm)
    ctx.shot("%s_connected" % tag)
    if secs is None:
        ctx.reset()
        return
    r.metric("connect_s", round(secs, 2))
    time.sleep(1.0 + 2 * rt)
    # ---- typing + Enter exactness (10-25 ms/key, instant Enter)
    CR.settle_remote_prompt(ctx)
    rmark, lmark = len(ctx.prof.commands(remote=True)), ctx.cmd_mark()
    n = ctx.count(12)
    exp, cmds = [], []
    for _ in range(n):
        tok = ctx.tok(10).lower()
        exp.append(tok)
        cmds.append(append_line(arm, tok, "~/%s_ft.txt" % tag))
    CR.typed_lines(ctx, cmds)
    ctx.until(lambda: len(ctx.lines(tag + "_ft.txt", remote=True)) >= n, 20 + n + 6 * rt)
    time.sleep(0.5 + 2 * rt)
    CR.exact(r, "remote typing, %d lines" % n, exp, ctx.lines(tag + "_ft.txt", remote=True))
    CR.exact(r, "commands the REMOTE shell received", cmds, CR.remote_cmds_after(ctx, rmark))
    r.check(not ctx.lines(tag + "_ft.txt"), "a remote-typed line ran LOCALLY: %r" % ctx.lines(tag + "_ft.txt"))
    r.check(ctx.cmds_since(lmark) == [], "the LOCAL shell received commands while the remote shell was in front: %r" % ctx.cmds_since(lmark))
    # ---- nested subshells (sudo / su / ksu stand-ins), proven by pid. The remote side is a real Linux host here, so the clean-env step runs on every platform.
    subs = (("bash", "bash", 1.5), ("bash-no-wait", "bash", 0.0), ("sh", "sh", 1.5),
            ("clean-env bash (ksu/su stand-in)", 'env -i HOME="$HOME" PATH="$PATH" TERM="$TERM" bash --noprofile --norc -i', 2.0))
    for i, (label, cmd, gap) in enumerate(subs):
        f = "%s_sub.txt" % tag
        CR.rm_remote(ctx, f)
        tok = ctx.tok(8).lower()
        ctx.be.type(append_line(arm, "$$", "~/" + f), 12, enter=True)
        ctx.until(lambda: len(ctx.lines(f, remote=True)) >= 1, 6 + 2 * rt)
        ctx.be.type(cmd, 12, enter=True)
        time.sleep(gap + 2 * rt)
        CR.typed_lines(ctx, [append_line(arm, "$PPID", "~/" + f), append_line(arm, tok, "~/" + f), "exit"])
        ctx.until(lambda: len(ctx.lines(f, remote=True)) >= 3, 10 + 4 * rt)
        time.sleep(1.5 + 2 * rt)
        ctx.be.type(append_line(arm, "$$", "~/" + f), 12, enter=True)
        ctx.until(lambda: len(ctx.lines(f, remote=True)) >= 4, 8 + 4 * rt)
        lines = ctx.lines(f, remote=True)
        ok = len(lines) == 4 and lines[0] == lines[1] == lines[3] and lines[2] == tok and lines[0].isdigit()
        r.check(ok, "remote subshell %s (setup gap %.1fs): want [outer pid, same pid as inner $PPID, %s, outer pid again], got %r" % (label, gap, tok, lines))
        ctx.shot("%s_sub%d" % (tag, i))
        if not ok:
            break
    # ---- a full-screen program: raw bytes, alternate screen (stand-in for vim)
    tok = ctx.tok(6).lower()
    (text, rest), want = raw_keys_expected(tok)
    ctx.be.type(raw_program_line(tag + "_raw.txt", len(want)), 12, enter=True)
    time.sleep(2.0 + 2 * rt)
    ctx.be.type(text, 15)
    ctx.be.key("Escape")
    time.sleep(0.3)
    ctx.shot("%s_raw" % tag)
    ctx.be.type(rest, 15)
    ctx.be.key("Return")
    ctx.until(lambda: _has(ctx, "%s_raw.txt" % tag, len(want)), 30 + 4 * rt)
    got = _file_bytes(ctx, arm, "%s_raw.txt" % tag)
    r.check(got == want, "the full-screen stand-in received %r, want exactly %r" % (got, want))
    time.sleep(0.8)
    # ---- a pager-like program: three single keys
    ctx.be.type(raw_program_line(tag + "_pager.txt", 3), 12, enter=True)
    time.sleep(1.5 + 2 * rt)
    for k in (" ", "G", "q"):
        ctx.be.type(k, 20)
    ctx.until(lambda: _has(ctx, "%s_pager.txt" % tag, 3), 30 + 4 * rt)
    got = _file_bytes(ctx, arm, "%s_pager.txt" % tag)
    r.check(got == b" Gq", "the pager-like stand-in received %r, want exactly %r" % (got, b" Gq"))
    time.sleep(0.8)
    tok = ctx.tok(8).lower()
    ctx.be.type(append_line(arm, tok, "~/%s_less.txt" % tag), 12, enter=True)
    ctx.until(lambda: ctx.lines(tag + "_less.txt", remote=True), 8 + 4 * rt)
    CR.exact(r, "remote shell usable after the full-screen programs (their keys did not leak into the shell)", [tok], ctx.lines(tag + "_less.txt", remote=True))
    # ---- Ctrl-C of a remote long command
    f = tag + "_cc.txt"
    ctx.be.type("bash -c 'trap \"echo INT >> ~/%s; exit\" INT; sleep 60'; echo AFTER >> ~/%s" % (f, f), 10, enter=True)
    time.sleep(1.5 + 2 * rt)
    t0 = time.time()
    ctx.be.key("ctrl+c")
    ctx.until(lambda: len(ctx.lines(f, remote=True)) >= 2, 8 + 4 * rt, step=0.05)
    r.metric("remote_ctrl_c_wall_s", round(time.time() - t0, 2))
    CR.exact(r, "Ctrl-C reached the REMOTE command and the remote shell continued", ["INT", "AFTER"], ctx.lines(f, remote=True))
    # ---- back to local: the next local command runs locally, never remotely
    ok = CR.leave(ctx)
    r.check(ok, "no local prompt after `exit` from the remote shell" if ctx.left_unproven is None else
            "the pane is not provably REMOTE (probe ran in: %s), so `exit` was NOT typed (it would close the local pane)" % ctx.left_unproven)
    if ctx.left_unproven is not None:
        ctx.reset()
    tok = ctx.tok(8).lower()
    lmark = ctx.cmd_mark()
    ctx.be.type(append_line(arm, tok, "%s_back.txt" % tag), 12, enter=True)
    ctx.until(lambda: ctx.lines(tag + "_back.txt"), 8)
    CR.exact(r, "local command after exit ran locally", [tok], ctx.lines(tag + "_back.txt"))
    r.check(not ctx.lines(tag + "_back.txt", remote=True), "local command after exit ran on the REMOTE: %r" % ctx.lines(tag + "_back.txt", remote=True))
    if ctx.prof.commands():
        back = append_line(arm, tok, "%s_back.txt" % tag)
        r.check(bool(ctx.until(lambda: back in ctx.cmds_since(lmark), 5)), "local rec.log lacks the exact command typed after exit")
    # ---- and vice versa: connect again, a remote command runs remotely only
    if connect_or_fail(ctx, r, arm, "second connect: ") is not None:
        time.sleep(1.0 + 2 * rt)
        tok = ctx.tok(8).lower()
        ctx.be.type(append_line(arm, tok, "~/%s_m.txt" % tag), 12, enter=True)
        ctx.until(lambda: ctx.lines(tag + "_m.txt", remote=True), 8 + 4 * rt)
        CR.exact(r, "second session: remote command ran remotely", [tok], ctx.lines(tag + "_m.txt", remote=True))
        r.check(not ctx.lines(tag + "_m.txt"), "second session: remote command ran LOCALLY")
        if not CR.leave(ctx):
            r.check(False, "second session: not provably remote (probe ran in: %s), `exit` NOT typed" % ctx.left_unproven)
    ctx.reset()


def _file_bytes(ctx, arm, name):
    """the bytes of /home/<user>/<name> on the lab host (b"" when it is not there)"""
    try:
        return ctx.lab.file("/home/%s/%s" % (arm["user"], name))
    except Exception:       # noqa: BLE001  (LabError: no such file)
        return b""


def _has(ctx, name, n):
    """whether the lab user's file `name` holds at least n bytes"""
    arm = ctx.arm
    return len(_file_bytes(ctx, arm, name)) >= n
