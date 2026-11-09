# appci/cases_agent_interactive.py (#1829): agent-run commands that need the USER (password prompt, vim, ssh, a pager) are handed over, typed into at the
# owner's stress rate (10-25 ms/key), and handed back; plus the negative control (a non-interactive command is never taken over).
# Agent mode only: "the agent ran it" exists only there. The same programs typed directly in a terminal-mode pane are covered by alt_screen_programs (vim/less),
# sudo_su and the ssh_* cases, so there is no second mode to add here.
# Needs the scripted fake model (APPCI_FAKE_MODEL=1, fake_model.py): its request log (APPCI_FAKE_MODEL_LOG, written by the model server itself) is the
# oracle for "the model's next request carried the command's output" and "nothing typed into the program reached the conversation".
# Oracles (never a screenshot alone): the file the program wrote (vim/less/ssh), the command's own echoed output in the model's NEXT request (password), the
# model's request log (no typed token in any user message; the follow-up prose arrives exact), the app log (no hand-over of the negative control, no
# 'Failed to enter inline agent view for LRC handoff', the secret never logged). A step that cannot prove its precondition is unverifiable/skip, never a pass.
# Platforms: Linux yes; Mac yes (bash/vim/less/ssh exist; the fake ssh host needs sshd or fakesshd); Windows: agent_interactive_password only, in the PowerShell pane
# (#1882: Read-Host -AsSecureString, handed over by the bootstrap's marker); the rest skip (POSIX vim/less/ssh programs).
import os, re, time

import cases
from cases import case
import cases_caseagent as AG
import fake_model as FM

PW_CMD = "bash -c 'read -s -p \"pw: \" x; echo; echo got:$x'"        # a real tty read with echo off; bash -c keeps it shell-independent (zsh/fish `read` differ)
# #1882: the Windows PowerShell pane has no termios; its hand-over is the bootstrap's Read-Host marker (TtyRead, secret=true). Same shape: a prompt, echo off, `got:<secret>`.
PW_CMD_PS = "$s = Read-Host -AsSecureString -Prompt 'pw: '; $x = [System.Net.NetworkCredential]::new('', $s).Password; Write-Output \"got:$x\""


def password_cmd(pane):
    """pure: the agent-run secret-read command for the pane ('powershell' = the Windows PowerShell pane, anything else the POSIX bash -c form)"""
    return PW_CMD_PS if pane == "powershell" else PW_CMD


def model_log():
    return os.environ.get("APPCI_FAKE_MODEL_LOG") or ""


def need_fake(ctx, r, windows_powershell_ok=False):
    if os.environ.get("APPCI_FAKE_MODEL_ACTIVE") != "1" or not model_log():
        r.skip("the scripted fake model is not running (APPCI_FAKE_MODEL=1): its request log is the oracle for what the model was sent next")
    AG.need_provider(ctx, r)
    if (ctx.win or os.name == "nt") and not (windows_powershell_ok and ctx.pane == "powershell"):
        r.skip("POSIX read/vim/less/ssh programs: not run in a Windows pane")


def rows_since(n):
    return FM.read_request_log(model_log(), n)


def judge_requests(rows, secret, want_tool=None):
    """pure. rows = the model-request rows of one arm. Returns [problem, ...] ([] = fine): the secret/typed token must be in no user message, a tool result must have
    come back to the model, and `want_tool` (when given) must be in the LAST tool result."""
    bad = []
    for i, row in enumerate(rows):
        for u in row.get("users", []):
            if secret and secret in u:
                bad.append("request %d carries the typed text %r in a user message (it leaked into the conversation)" % (i + 1, secret))
    tools = [row["tool"] for row in rows if row.get("tool") is not None]
    if not tools:
        bad.append("no request carried a tool result: the command's output never went back to the model (%d request(s))" % len(rows))
    elif want_tool is not None and want_tool not in tools[-1]:
        bad.append("the model's next request did not carry %r from the command (tool result: %r)" % (want_tool, tools[-1][-200:]))
    return bad


def ask_run(ctx, r, command, label):
    """ask the fake model to run `command` and approve its card (Enter focuses, a second Enter approves). Returns (app log offset, request-log row count before)."""
    n0, off = len(rows_since(0)), ctx.log_size()
    AG.type_fast(ctx, "Please run exactly this shell command and nothing else: %s" % command)
    if not AG.wait_marker(ctx, off, "queue_actions", timeout=60):
        ctx.shot("%s_nocard" % label)
        r.skip("%s: no approval card within 60 s (the model did not propose the command)" % label)
    time.sleep(1.2)
    ctx.shot("%s_card" % label)
    ctx.be.key("Return")
    time.sleep(0.8)
    ctx.be.key("Return")
    return off, n0


def wait_tool_result(ctx, n0, timeout=40):
    return ctx.until(lambda: any(x.get("tool") is not None for x in rows_since(n0)), timeout, step=0.3)


ALT_ENTER_RE = re.compile(r"hop=alt_screen_enter\b")
FS_HANDOVER_RE = re.compile(r"hop=fullscreen_handover agent_in_control=0")      # app: the user's first key into an agent-run full-screen program took the command over


# The user's keys reached the full-screen program's pty as the user's own bytes (`hop=hand_route ... state=pty_owned ... command_interactive=true`, written by both input
# modes). An ordinary agent command waiting for completion has NO control state (take_over_for_user refuses a Manual take-over then, by design #1796/#1811), so the
# `fullscreen_handover` hop above never fires for it, yet the agent is not driving the program: its keys are exact and the agent only waits for the result.
FS_KEYS_RE = re.compile(r"hop=hand_route reason=unheld state=pty_owned \S+ \S+ \S+ command_interactive=true")


def hop_count(text, rx):
    """pure: how many lines of an app-log excerpt match the hop pattern"""
    return len(rx.findall(text))


def fullscreen_keys_proven(text):
    """pure: the app took the command over (agent had LRC control), or the user's keys went to the interactive program's pty after it took the screen"""
    i = text.find("hop=alt_screen_enter")
    return hop_count(text, FS_HANDOVER_RE) > 0 or (i >= 0 and hop_count(text[i:], FS_KEYS_RE) > 0)


def wait_alt_screen(ctx, off, timeout=30):
    """an EVENT, not a sleep: the app's `hop=alt_screen_enter` says the full-screen program owns the screen"""
    return ctx.until(lambda: hop_count(ctx.log_since(off), ALT_ENTER_RE) > 0, timeout, step=0.2)


def check_fullscreen_handover(ctx, r, label, off):
    """after the first keys: the app logged the take-over of the full-screen program (keys flow to it), so a green file alone cannot hide a missing hand-over"""
    ok = ctx.until(lambda: fullscreen_keys_proven(ctx.log_since(off)), 10, step=0.2)
    r.check(bool(ok), "%s: neither hop=fullscreen_handover nor a pty-owned hand_route in the app log after typing into the full-screen program: the keys' route is not proven" % label)


# cli_controller.rs switch_control_to_user logs `hop=handover action=<id> reason=<variant>` for EVERY take-over of an agent command. The prompt-shape path logs
# "handing the command to the user" first, but a password prompt (echo off, `read -s`) takes the command over via on_possible_password_prompt with no such line: the
# hop is the one signal all of them share. Only the system hand-overs count (a user Stop / Manual take-over is not the app handing a prompt over).
HANDOVER_RE = re.compile(r"hop=handover action=\S+ reason=(?:blocked_on_input|transfer_from_agent)\b")


def handover_seen(text):
    """pure: the app handed an agent command to the user (the real hand-over hop, or the prompt-shape line that precedes it)"""
    return bool(HANDOVER_RE.search(text or "")) or AG.LRC_HANDED_TO_USER in (text or "")


def handed(ctx, off):
    return handover_seen(ctx.log_since(off))


def after_checks(ctx, r, label, off, n0, secret, want_tool=None):
    """the common tail: the model got the output, nothing typed leaked, no hand-back ERROR, then agent input still works and arrives exact"""
    got = wait_tool_result(ctx, n0)
    r.check(bool(got), "%s: the model's next request never carried the command's output within 40 s (control did not return to the agent)" % label)
    time.sleep(1.0)
    rows = rows_since(n0)
    for p in judge_requests(rows, secret, want_tool if got else None):
        if got or "no request carried" not in p:
            r.check(False, "%s: %s" % (label, p))
    errs = ctx.log_since(off).count(AG.LRC_HANDOFF_ERROR)
    r.check(errs == 0, "%s: the hand-back logged '%s' %d time(s)" % (label, AG.LRC_HANDOFF_ERROR, errs))
    # the agent input is alive and clean after the hand-back: prose typed now arrives at the model byte-exact
    AG.settled(ctx, label + "_idle", timeout=60)
    n1 = len(rows_since(0))
    follow = "Reply with the word AFTER%s" % ctx.tok(5).upper()
    AG.type_fast(ctx, follow)
    ok = ctx.until(lambda: any(follow in (x.get("users") or [""])[-1] for x in rows_since(n1)), 30, step=0.3)
    ctx.shot(label + "_after")
    r.check(bool(ok), "%s: prose typed after the hand-back did not reach the model exactly as typed (%r): the agent input lost or changed keys" % (label, follow))
    AG.settled(ctx, label + "_idle2", timeout=60)


@case("agent_interactive_password", modes=("agent",), order=52, gui=True)
def c_agent_password(ctx, r):
    """#1829 (a): the agent runs a password prompt (`read -s -p 'pw: '`, not sudo). The hand-over must come (echo off = at once), the typed secret + Enter must reach the
    program exactly: the command echoes `got:<secret>` and that text is in the model's NEXT request. The secret is in no user message and not in the app log.
    Windows PowerShell pane (#1882): `Read-Host -AsSecureString` (PW_CMD_PS); the hand-over is the bootstrap's TtyRead marker, an event, not a probe."""
    need_fake(ctx, r, windows_powershell_ok=True)
    AG.fresh_conversation(ctx, r)
    try:
        secret = "PW" + ctx.tok(8).upper().replace("_", "")
        off, n0 = ask_run(ctx, r, password_cmd(ctx.pane), "ai_pw")
        if not ctx.until(lambda: handed(ctx, off), 45, step=0.5):
            ctx.shot("ai_pw_nohandover")
            aw = AG.lrc_awaited(ctx.log_since(off))
            if AG.LRC_NOTICE in ctx.log_since(off):
                r.unverifiable("ai_pw: the OS cannot say the command reads the tty; only the take-over notice was logged (no hand-over on silence, #1811)")
            if aw is False:
                r.unverifiable("ai_pw: agent_command_wait awaited=false: the tool call did not wait, the hand-over detector was never armed")
            r.check(False, "ai_pw: `read -s` at its prompt was never handed to the user within 45 s (awaited=%s)" % aw)
            return
        time.sleep(0.8)
        ctx.shot("ai_pw_prompt")
        ctx.be.type(secret, ctx.rng.randint(10, 25), enter=True)
        after_checks(ctx, r, "ai_pw", off, n0, secret, want_tool="got:" + secret)
        r.check(secret not in ctx.log_since(off).replace("got:" + secret, ""), "ai_pw: the typed secret appears in the app log")
    finally:
        AG.cleanup_tmp(ctx)
        ctx.be.key("ctrl+c")


@case("agent_interactive_vim", modes=("agent",), order=53, gui=True)
def c_agent_vim(ctx, r):
    """#1829 (b): the agent runs `vim -u NONE -N ai_vim.txt`. Once vim owns the screen (its swap file exists), `i<text>`, Escape, `:wq` + Enter are typed at 10-25 ms/key:
    the file must hold exactly the text, the text is in no user message, the model's next request carries a tool result, and the agent input works again."""
    need_fake(ctx, r)
    if not ctx.which("vim"):
        r.unverifiable("vim is not installed (package vim): the agent-run vim hand-over did not run")
        r.skip("vim missing")
    AG.fresh_conversation(ctx, r)
    try:
        ctx.rm("ai_vim.txt", ".ai_vim.txt.swp")
        tok = "vimtext" + ctx.tok(6).lower().replace("_", "")
        off, n0 = ask_run(ctx, r, "vim -u NONE -N ai_vim.txt", "ai_vim")
        if not ctx.until(lambda: os.path.exists(os.path.join(ctx.W, ".ai_vim.txt.swp")), 30, step=0.2):
            ctx.shot("ai_vim_notstarted")
            r.skip("ai_vim: vim never started (no swap file in 30 s), so no keys were typed")
        time.sleep(0.8)
        ctx.shot("ai_vim_up")
        r.note("ai_vim: hand-over line in the app log: %s" % ("yes" if handed(ctx, off) else "no (a full-screen program may be driven without the prompt hand-over)"))
        r.check(bool(wait_alt_screen(ctx, off, 5)), "ai_vim: no hop=alt_screen_enter in the app log although vim is up")
        ctx.be.type("i" + tok, ctx.rng.randint(10, 25))
        check_fullscreen_handover(ctx, r, "ai_vim", off)
        ctx.shot("ai_vim_insert")
        ctx.be.key("Escape")
        time.sleep(0.2)
        ctx.be.type(":wq", ctx.rng.randint(10, 25), enter=True)
        ctx.until(lambda: ctx.lines("ai_vim.txt"), 10)
        r.check(ctx.lines("ai_vim.txt") == [tok], "ai_vim: vim received the typed text exactly: file is %r, want %r" % (ctx.lines("ai_vim.txt"), [tok]))
        after_checks(ctx, r, "ai_vim", off, n0, tok)
    finally:
        ctx.rm("ai_vim.txt", ".ai_vim.txt.swp")
        AG.cleanup_tmp(ctx)
        ctx.be.key("ctrl+c")


@case("agent_interactive_ssh", modes=("agent",), order=54, gui=True)
def c_agent_ssh(ctx, r):
    """#1829 (c): the agent runs `ssh -tt fake '<remote tty read>'` against the local test sshd. The typed text must reach the REMOTE program exactly (~/ai_ssh.txt on the
    remote side, and `got:` in the model's next request), never the local shell or the agent input. Skips with the reason when no local ssh test host exists."""
    need_fake(ctx, r)
    if not ctx.ssh:
        r.skip(ctx.ssh_why or "no local ssh server (needs /usr/sbin/sshd + ssh-keygen, or python3-paramiko for fakesshd.py)")
    AG.fresh_conversation(ctx, r)
    try:
        try:
            os.remove(os.path.join(ctx.prof.rhome, "ai_ssh.txt"))
        except FileNotFoundError:
            pass
        ctx.rm("ai_ssh.txt")
        tok = "SSH" + ctx.tok(8).upper().replace("_", "")
        cmd = "ssh -tt fake 'read -s -r -p \"rpw: \" x; echo; echo got:$x >> ~/ai_ssh.txt; echo got:$x'"
        off, n0 = ask_run(ctx, r, cmd, "ai_ssh")
        if not ctx.until(lambda: handed(ctx, off), 60, step=0.5):
            ctx.shot("ai_ssh_nohandover")
            if AG.LRC_NOTICE in ctx.log_since(off) or AG.lrc_awaited(ctx.log_since(off)) is False:
                r.unverifiable("ai_ssh: no hand-over (take-over notice only, or the tool call did not wait): the ssh hand-over is not verified")
                return
            r.check(False, "ai_ssh: the remote `read` at its prompt was never handed to the user within 60 s")
            return
        time.sleep(1.0)
        ctx.shot("ai_ssh_prompt")
        ctx.be.type(tok, ctx.rng.randint(10, 25), enter=True)
        ctx.until(lambda: ctx.lines("ai_ssh.txt", remote=True), 20)
        r.check(ctx.lines("ai_ssh.txt", remote=True) == ["got:" + tok], "ai_ssh: the remote program received %r, want %r" % (ctx.lines("ai_ssh.txt", remote=True), ["got:" + tok]))
        r.check(not ctx.lines("ai_ssh.txt"), "ai_ssh: the typed text reached a LOCAL file: %r" % ctx.lines("ai_ssh.txt"))
        after_checks(ctx, r, "ai_ssh", off, n0, tok, want_tool="got:" + tok)
    finally:
        try:
            os.remove(os.path.join(ctx.prof.rhome, "ai_ssh.txt"))
        except FileNotFoundError:
            pass
        AG.cleanup_tmp(ctx)
        ctx.be.key("ctrl+c")


@case("agent_interactive_less", modes=("agent",), order=55, gui=True)
def c_agent_less(ctx, r):
    """#1829 (d): the agent runs `less ai_long.txt` (600 lines). Typed at 10-25 ms/key: `G` `g` `jjjjj`, a forward-search `/line 450` + Enter, then `!echo <tok> >> ai_less.txt`
    + Enter (less runs it through the shell: the byte-exact proof), Enter to leave the shell escape, `q`. The effect file must hold exactly the token, the token is in no
    user message, and the model's next request carries a tool result."""
    need_fake(ctx, r)
    if not ctx.which("less"):
        r.unverifiable("less is not installed (package less): the agent-run pager hand-over did not run")
        r.skip("less missing")
    AG.fresh_conversation(ctx, r)
    try:
        with open(os.path.join(ctx.W, "ai_long.txt"), "w") as f:
            f.write("".join("line %d\n" % i for i in range(1, 601)))
        ctx.rm("ai_less.txt")
        tok = "less" + ctx.tok(6).lower().replace("_", "")
        off, n0 = ask_run(ctx, r, "less ai_long.txt", "ai_less")
        if not wait_alt_screen(ctx, off, 30):
            ctx.shot("ai_less_notup")
            r.skip("ai_less: no hop=alt_screen_enter within 30 s, so less never took the screen and no keys were typed")
        ctx.shot("ai_less_up")
        r.note("ai_less: hand-over line in the app log: %s" % ("yes" if handed(ctx, off) else "no (a pager may be driven without the prompt hand-over)"))
        for chunk in ("G", "g", "jjjjj"):
            ctx.be.type(chunk, ctx.rng.randint(10, 25))
            time.sleep(0.3)
        check_fullscreen_handover(ctx, r, "ai_less", off)
        ctx.be.type("/line 450", ctx.rng.randint(10, 25), enter=True)
        time.sleep(0.5)
        ctx.shot("ai_less_search")
        ctx.be.type("!echo %s >> ai_less.txt" % tok, ctx.rng.randint(10, 25), enter=True)
        ctx.until(lambda: ctx.lines("ai_less.txt"), 10)
        ctx.be.key("Return")
        time.sleep(0.5)
        ctx.be.type("q", ctx.rng.randint(10, 25))
        r.check(ctx.lines("ai_less.txt") == [tok], "ai_less: less ran the line typed inside it: file is %r, want %r" % (ctx.lines("ai_less.txt"), [tok]))
        after_checks(ctx, r, "ai_less", off, n0, tok)
    finally:
        ctx.rm("ai_less.txt", "ai_long.txt")
        AG.cleanup_tmp(ctx)
        ctx.be.key("ctrl+c")


@case("agent_noninteractive_no_takeover", modes=("agent",), order=56, gui=True)
def c_agent_no_takeover(ctx, r):
    """#1829 negative control: a non-interactive command that merely looks busy (prints a prompt-shaped `Continue? ` then finishes by itself, no tty read) must run to the end
    with the agent: no hand-over line in the app log, its output in the model's next request, and the keys typed meanwhile (none) cannot be mistaken: nothing typed lands anywhere."""
    need_fake(ctx, r)
    AG.fresh_conversation(ctx, r)
    try:
        tok = "NOTAKE" + ctx.tok(6).upper().replace("_", "")
        off, n0 = ask_run(ctx, r, "printf 'Continue? '; sleep 6; echo done:%s" % tok, "ai_notake")
        wait_tool_result(ctx, n0, timeout=60)
        r.check(not handed(ctx, off), "ai_notake: a non-interactive command that never reads the tty was handed to the user (#1796)")
        after_checks(ctx, r, "ai_notake", off, n0, None, want_tool="done:" + tok)
    finally:
        AG.cleanup_tmp(ctx)
        ctx.be.key("ctrl+c")
