# appci/cases_precmd_secret.py (#1923; round 12, docs/design/r12-ps1case.md): a secret asked for by a bash precmd function, with the app's prompt and with
# the shell's own prompt ("honor PS1", terminal.input.honor_ps1).
#   precmd_secret_honor_ps1   A precmd function runs `read -s -p "vault pin: "` after the prompt's Precmd and before the prompt. The PIN is typed in two halves
#                             with a pause longer than the ConPTY prompt hold's give-up between them. It must reach the `read` whole and in typed order, never
#                             be in the input editor at an Enter, never be a command the shell received, and the line typed after it must run. Two arms in one
#                             case: honor PS1 OFF (the app draws the prompt; the control, and on Windows the same scenario as gitbash_precmd_secret) and honor
#                             PS1 ON (bash draws its own prompt: its Precmd carries no `ps1`, the prompt's start marker rides in PS1 and comes after every
#                             precmd function). No case ran with honor PS1 on before this one.
# Why two arms: the setting is read when a pane's shell starts (WARP_HONOR_PS1 in its environment), so the honor arm needs an app relaunch with the setting in
# the profile's settings.toml; the control arm in the same case tells "broken with honor PS1" from "a precmd read is broken on this platform at all".
# Panes: bash only. Linux: the pane's own bash. Windows: the Git Bash pane (winmap.GITBASH_ONLY). macOS: the pane is zsh, so both arms relaunch with
# session.startup_shell_override = "bash" (the system bash, 3.2). Both session modes. The profile's settings.toml is put back and the app relaunched at the end.
# Each arm proves its own premise first (a line typed into the pane records $WARP_HONOR_PS1 and whether the shell is bash): an arm whose premise is not met is
# UNVERIFIABLE, never a pass.
#   precmd_question_echo      #2052 (docs/design/r14-readmark.md): a precmd function asks with echo ON, `read -p "continue? " x` (zsh: `read "x?continue? "`), after the
#                             prompt's Precmd. No tty sample tells an echoing read from a shell between its Precmd and its line editor, so on Linux and macOS the
#                             answer was typed into the input editor and its Enter ran it as a command or sent it to the model. The bootstraps now announce the read
#                             (the TtyReaderAsks marker) and the app gives the keys to the pty. The answer must reach the `read` whole, never be in the input editor at
#                             an Enter (input_type=AI there = sent to the model), never be a command, the question must be on screen, and the next line must run.
#                             Arm `own`: the pane's own shell (Linux bash, macOS zsh, Windows Git Bash). Arm `bash` (macOS only): a relaunch with the system bash 3.2.
#                             The function is registered by one line and asks at a LATER prompt: at the first prompt after its registration a late function runs
#                             after the bootstrap's last precmd function (the dispatcher iterates a copy), outside the stage the marker covers.
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402  (cases.py imports this module as its last line)
from cases import case  # noqa: E402
import cases_robust as R  # noqa: E402

HOLD_GIVE_UP_S = 1.5     # CONPTY_PROMPT_HOLD_GIVE_UP (typeahead_handoff.rs); the pause between the PIN's halves is longer on every platform
QUESTION = "vault pin: "
QUESTION_WORD = "vault"  # no line the case TYPES contains it (the function is sourced from a file), so on screen it can only be the question
ENTER_BEGIN = "hop=enter_begin"
INPUT_TYPE_RE = re.compile(r"\binput_type=(\w+)")


def names(tag):
    """pure: the files of one arm, all relative to the pane's case dir (ctx.W)"""
    return dict(script="pp_%s.sh" % tag, installed="pp_%s_inst.txt" % tag, err="pp_%s_err.txt" % tag, go="pp_%s_go.txt" % tag, asked="pp_%s_asked.txt" % tag,
                pin="pp_%s_pin.txt" % tag, after="pp_%s_after.txt" % tag, mode="pp_%s_mode.txt" % tag)


def install_script(n):
    """pure: the bash file the pane sources. The function asks ONCE, at the first precmd that finds the trigger file; it records that it is about to ask (a file
    that stays, unlike the trigger, which it removes within milliseconds of its creation) and what the `read` returned. The last line proves the file was sourced."""
    return ('__pp_pin() { if [ -f %(go)s ]; then rm -f %(go)s; echo asked > %(asked)s; read -s -p "%(q)s" x; echo "$x" > %(pin)s; echo; fi; }\n'
            'precmd_functions+=(__pp_pin)\n'
            'echo installed > %(installed)s\n') % dict(n, q=QUESTION)


def mode_probe_line(fname):
    """pure: the line that records the pane's prompt mode as its SHELL sees it and whether that shell is bash. Valid in bash and zsh; `$`, quotes and `>` make it
    shell syntax in an agent conversation too (#1560)."""
    return 'echo "hp=${WARP_HONOR_PS1:-unset} sh=${BASH_VERSION:+bash}" > %s' % fname


def parse_mode_probe(lines):
    """pure: (honor, is_bash) from the probe file's lines: honor is True / False / None (unset or unreadable); None when the file holds no probe line"""
    for ln in reversed(lines or []):
        m = re.match(r"^hp=(\S*) sh=(\S*)\s*$", ln)
        if m:
            return ({"1": True, "0": False}.get(m.group(1)), m.group(2) == "bash")
    return None


def settings_for(text, honor, force_bash, T=R.T):
    """pure: the profile's settings.toml text with terminal.input.honor_ps1 set, and (macOS: the pane is zsh) the startup shell set to bash"""
    t = T.set_key(text, "terminal.input", "honor_ps1", "true" if honor else "false")
    if force_bash:
        t = T.set_key(t, "session", "startup_shell_override", '"bash"')
    return t


def pin_in_editor_enters(log_text, pin):
    """pure: the input_type of every `hop=enter_begin` line of an app-log excerpt whose buffer shows the PIN or one of its halves: the secret sat in the input
    editor when an Enter was pressed (visible, and submitted as a command or to the agent). The buffer text is in the log only with WARP_INPUT_ORDER_LOG_TEXT=1
    (the harness sets it); without it the line reads `<N chars>` and nothing matches, which the caller reports as not judged."""
    half = len(pin) // 2
    parts = [p for p in (pin, pin[:half], pin[half:]) if len(p) >= 3]
    out = []
    for ln in (log_text or "").splitlines():
        if ENTER_BEGIN in ln and any(p in ln for p in parts):
            m = INPUT_TYPE_RE.search(ln)
            out.append(m.group(1) if m else "?")
    return out


def enter_text_logged(log_text):
    """pure: True when the excerpt's enter_begin lines carry buffer TEXT (`buffer="..."`), False when they carry counts only, None when there is no such line"""
    ls = [ln for ln in (log_text or "").splitlines() if ENTER_BEGIN in ln]
    if not ls:
        return None
    return any('buffer="' in ln for ln in ls)


ENTER_COUNT_RE = re.compile(r"hop=enter_begin\b[^\n]*?\bbuffer=<(\d+) chars> input_type=(\w+)")


def withheld_enters(log_text):
    """pure (#2040): [(chars, input_type)] of every `hop=enter_begin` whose buffer is logged by its COUNT and is not empty. The harness opts in to text, so
    a count there means the app withheld it: something read a secret, or a reader ahead of the line editor had the keys, when that Enter was pressed with text
    in the input editor. That is the state `pin_in_editor_enters` used to read by text and no longer can (the app logs no text while a secret is read)."""
    return [(int(m.group(1)), m.group(2)) for m in ENTER_COUNT_RE.finditer(log_text or "") if int(m.group(1)) > 0]


# #2040: the app's own account of a typed key. Text form: `typed_characters text="4"`, `keydown key="4"`; count form (no opt-in, or withheld because a secret
# is read): `text=<1 chars>`, `key=<1 chars>`. Named keys (`key=enter`) are neither. `hop=secret_text_withheld active=<bool>` marks each change of the guard.
KEY_LINE_RE = re.compile(r'input_order (?:typed_characters text|keydown key)=("(?:[^"\\]|\\.)*"|<\d+ chars>)[^\n]*? seq=(\d+)')
WITHHELD_RE = re.compile(r"hop=secret_text_withheld active=(true|false)")


def key_text_seqs(log_text):
    """pure: (shown, counted): the distinct key numbers (`seq=`) of an excerpt's key lines that carry the key's TEXT, and of those that carry its count only"""
    shown, counted = set(), set()
    for m in KEY_LINE_RE.finditer(log_text or ""):
        (shown if m.group(1).startswith('"') else counted).add(int(m.group(2)))
    return shown, counted - shown


def secret_parts(secret):
    """pure: the secret and its two typed halves, 5 characters or longer: what must never be in ANY log line. (The case's PIN is `pin` + 6 random characters,
    so its first half is `pin` + one character, which words of ordinary log lines contain: `ping`, `typing`. The whole PIN and its random half are searched.)"""
    half = len(secret) // 2
    return [p for p in (secret, secret[:half], secret[half:]) if len(p) >= 5]


def secret_log_verdict(before, during, secret, first_burst, sampled_tty):
    """pure (#2040): what the app log says about a secret typed at a `read -s`. `before` is the excerpt of a line typed at the prompt just before the read
    (the control: its keys must show text, else the log has no text at all and nothing is judged); `during` is the excerpt from the first key of the secret to
    after its Enter. Returns (judged, problems, facts).
      - no line of `during` holds the secret or a typed half of it (any hop: an editor buffer, a pty write, a shell report);
      - the guard engaged: a `hop=secret_text_withheld active=true` in `before` or `during`;
      - key lines that still show their character: none where the tty is sampled (`sampled_tty`: Linux, macOS: echo is off before the first key). On ConPTY
        the reader is noted from the screen at the first key or at the prompt hold's cap, so the keys of the first burst (`first_burst` of them) were logged
        before the app could know; no more than those may show, and at least one key must be count-only."""
    shown, counted = key_text_seqs(during)
    control, _ = key_text_seqs(before)
    leaked = [ln.strip()[:160] for ln in (during or "").splitlines() if any(p in ln for p in secret_parts(secret))]
    engaged = any(m.group(1) == "true" for m in WITHHELD_RE.finditer((before or "") + "\n" + (during or "")))
    facts = dict(control_keys_shown=len(control), keys_shown=len(shown), keys_counted=len(counted), guard_engaged=engaged, lines_with_secret=len(leaked))
    problems = []
    if leaked:
        problems.append("the app log holds the secret or a typed half of it in %d line(s), first: %r" % (len(leaked), leaked[0]))
    if not control:
        return False, problems, facts
    if not engaged:
        problems.append("no `hop=secret_text_withheld active=true` in the app log: the log's secret guard never engaged while the `read -s` waited")
    allowed = 0 if sampled_tty else first_burst
    if len(shown) > allowed:
        problems.append("%d key(s) typed at the `read -s` are in the app log with their character (`typed_characters text=\"..\"` / `keydown key=\"..\"`), "
                        "at most %d may be (%s)" % (len(shown), allowed, "the tty is sampled: echo was off before the first key" if sampled_tty
                                                    else "ConPTY: the keys before the reader was noted"))
    if not counted:
        problems.append("no key typed at the `read -s` is logged by its count only: the log shows every key of the secret")
    return True, problems, facts


def question_in(ocr_text):
    """pure: is the question's word in an OCR text (case and line breaks ignored)?"""
    return QUESTION_WORD in " ".join((ocr_text or "").lower().split())


def asked_look_is_judged(mode, win):
    """pure: (judged, why) for the look taken BEFORE a key is typed. Terminal mode: judged everywhere (a background block is drawn under the last command).
    Agent mode (#2053): the conversation shows the question only while a reader ahead of the line editor is noted. Where the tty is sampled (not Windows) that
    is at the question's output, so the look is judged; on ConPTY the reader is noted at the first key or the hold's cap, so this look is recorded only."""
    if mode != "agent":
        return True, "terminal mode"
    if win:
        return False, "agent mode on ConPTY: the reader is noted at the first key"
    return True, "agent mode: the reader is noted from the tty at the question's output"


def arm_plan(honor_now, is_bash, want_honor):
    """pure: does the arm need a relaunch before it runs? Only a bash pane already in the wanted prompt mode runs as it is."""
    return not (is_bash and honor_now is want_honor)


def probe_mode(ctx, n):
    """type the mode probe into the pane; -> (honor, is_bash) or None when the line had no effect within 8 s"""
    ctx.rm(n["mode"])
    ctx.be.guard()
    ctx.be.type(mode_probe_line(n["mode"]), 12, enter=True)
    ctx.until(lambda: parse_mode_probe(ctx.lines(n["mode"])) is not None, 8, step=0.2)
    return parse_mode_probe(ctx.lines(n["mode"]))


def relaunch_with(ctx, r, text, label):
    """write `text` as the profile's settings.toml with the app stopped, then start it and wait for the pane's shell (cases_robust.start_app: every
    platform, the Windows key proof included). -> True when the new instance is up."""
    q = R.stop_for_restart(ctx, r, 20, label)
    if not q["exited"]:
        r.note("%s: the app did not exit within 20 s on the quit path; stopped by the harness" % label)
        ctx.app.stop()
    with open(R.settings_file(ctx), "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
    _, up = R.start_app(ctx, r, label)
    if not up:
        return False
    if ctx.win and hasattr(ctx.app, "find_log") and not os.environ.get("APPCI_APP_LOG"):       # the new instance writes a new log file (run.py re-resolves it per case only)
        try:
            ctx.log_path = ctx.app.find_log() or ctx.log_path
        except Exception:
            pass
    time.sleep(2.0)
    if ctx.mode != "agent":       # the first-run overlay; in a fresh agent view Escape would LEAVE agent mode (mode_proof), so only the terminal pass sends it
        try:
            ctx.be.key("Escape")
            time.sleep(0.5)
        except Exception:
            pass
    return True


def pin_scenario(ctx, r, tag, n):
    """one arm, in a bash pane whose prompt mode was already proven. Every message carries the arm's tag."""
    ctx.rm(n["installed"], n["err"], n["go"], n["asked"], n["pin"], n["after"])
    with open(os.path.join(ctx.W, n["script"]), "w", newline="\n") as f:
        f.write(install_script(n))
    pin = "pin" + ctx.tok(6).replace("_", "").lower()
    half = len(pin) // 2
    mark = ctx.cmd_mark()
    ctx.be.type("source ./%s 2> %s" % (n["script"], n["err"]), 12, enter=True)
    if not r.check(bool(ctx.until(lambda: ctx.lines(n["installed"]) == ["installed"], 10, step=0.2)),
                   "[%s] the line that installs the precmd function had no effect in the shell (stderr file: %r): the PIN was NOT typed" % (tag, ctx.lines(n["err"])[:3])):
        return
    time.sleep(2.0)        # the install line's own prompt is drawn and its line-editor signal sent: the shell has signalled this session (the premise of #1923's hold)
    off_control = ctx.log_size()
    ctx.be.type("touch " + n["go"], 12, enter=True)
    if not r.check(bool(ctx.until(lambda: ctx.lines(n["asked"]) == ["asked"], 10, step=0.2)),
                   "[%s] the precmd function never reached its `read` after `touch %s` (no %s): the PIN was NOT typed" % (tag, n["go"], n["asked"])):
        return
    time.sleep(0.6)
    # Is the question on screen? What is printed between the Precmd and the prompt is early output, drawn as its own block, in either prompt mode. In an agent
    # conversation such a block belongs to no command of the conversation and was hidden (#2053, round 12: false in the agent pass); the app now shows it there
    # while a reader ahead of the line editor has the keys. Where the app samples the tty (Linux, macOS: #2050) that reader is noted at the question's own
    # output, so the question is judged here, before a key, in both modes. On ConPTY the reader is noted from the screen at the first key or at the prompt
    # hold's cap (#2038): in the agent pass this first look is recorded and the look after the first half of the PIN (below) is the judged one.
    judged, where = asked_look_is_judged(ctx.mode, ctx.win)
    txt = ctx.ocr("pp_%s_asked" % tag)
    if txt is None:
        ctx.shot("pp_%s_asked" % tag)
        r.note("[%s] no OCR engine: whether the question is drawn is in the screenshot pp_%s_asked only" % (tag, tag))
    else:
        seen = question_in(txt)
        r.metric("question_on_screen_%s" % tag, seen)
        if judged:
            r.check(seen, "[%s] the question %r is not on screen while the shell waits for the PIN (OCR of pp_%s_asked, %s): the user cannot see what is asked"
                    % (tag, QUESTION, tag, where))
        elif not seen:
            r.note("[%s] the question %r was not found on screen by OCR before a key was typed (shots/..pp_%s_asked; %s): recorded here, judged after the first "
                   "half of the PIN" % (tag, QUESTION, tag, where))
    control_log = ctx.log_since(off_control)      # #2040: the keys of `touch <go>`, typed at a prompt: the control of the log check below
    off = ctx.log_size()
    ctx.be.type(pin[:half], ctx.rng.randint(10, 25))
    time.sleep(HOLD_GIVE_UP_S + 1.2)      # past the give-up cap the first key armed (ConPTY); elsewhere just a user who pauses in the middle of a PIN
    if ctx.mode == "agent":
        # #2053: half of the PIN is typed and the `read` still waits. The question must be on screen in the conversation on every platform (the only judged
        # look of the agent pass on ConPTY), and the keys must not be: a `read -s` echoes nothing, and the PIN in the input editor is the #2050 failure.
        txt = ctx.ocr("pp_%s_after_pause" % tag)
        if txt is None:
            ctx.shot("pp_%s_after_pause" % tag)
            r.note("[%s] no OCR engine: whether the question is drawn while the PIN is typed is in the screenshot pp_%s_after_pause only" % (tag, tag))
        else:
            seen = question_in(txt)
            r.metric("question_on_screen_typing_%s" % tag, seen)
            r.check(seen, "[%s] the question %r is not on screen in the agent conversation after half of the PIN was typed and while the `read` waits (OCR of "
                    "pp_%s_after_pause): the user answers a question they cannot see (#2053; app log: no `hop=reader_question_in_conversation shown=true` "
                    "means no reader was noted or no block was found)" % (tag, QUESTION, tag))
    else:
        ctx.shot("pp_%s_after_pause" % tag)
    ctx.be.type(pin[half:], ctx.rng.randint(10, 25), enter=True)
    ok = ctx.until(lambda: ctx.lines(n["pin"]) == [pin], 20, step=0.3)
    ctx.shot("pp_%s_done" % tag)
    r.check(bool(ok), "[%s] the PIN did not reach `read -s` whole and in order: %s holds %r, want %r (keys went to the input editor, stayed held, or were lost)"
            % (tag, n["pin"], ctx.lines(n["pin"]), [pin]))
    time.sleep(1.0)
    log = ctx.log_since(off)
    in_editor = pin_in_editor_enters(log, pin)
    r.check(not in_editor, "[%s] the PIN (or half of it) was in the input editor when an Enter was pressed, input_type %s: a secret shown in the editor and submitted"
            % (tag, ", ".join(in_editor)))
    # #2040: while the `read -s` waits the app logs no text, so an Enter that found the PIN in the editor shows `buffer=<N chars>`: judged by the count.
    held = withheld_enters(log)
    r.check(not held, "[%s] an Enter was pressed with text in the input editor while the app withheld the text from its log (a secret was being read): %s "
            "(chars, input_type): the PIN was typed into the editor and submitted" % (tag, held))
    logged = enter_text_logged(log)
    if logged is False and not held:
        r.note("[%s] the app log's Enter lines carry no editor text and no count above 0: 'the PIN was never in the editor at an Enter' rests on the counts" % tag)
    # #2040: the PIN itself must not be in the app log (with the harness's text opt-ins on), in any line.
    judged, problems, facts = secret_log_verdict(control_log, log, pin, half, not ctx.win)
    for k, v in sorted(facts.items()):
        r.metric("secret_log_%s_%s" % (k, tag), v)
    for p in problems:
        r.check(False, "[%s] #2040: %s" % (tag, p))
    if not judged:
        r.note("[%s] #2040: the keys of the control line typed at the prompt are not in the app log as text (WARP_INPUT_ORDER_LOG_TEXT off, or no key lines): "
               "'the app log withholds the keys of a secret' was not judged" % tag)
    ran = [c for c in ctx.cmds_since(mark) if pin in c or pin[:half] in c or pin[half:] in c]
    r.check(not ran, "[%s] the PIN (or half of it) reached the shell as a command: %r" % (tag, ran))
    tok = ctx.tok(8).lower()
    ctx.be.type(ctx.append_cmd("bash", tok, n["after"]), 12, enter=True)
    r.check(bool(ctx.until(lambda: tok in ctx.lines(n["after"]), 10)), "[%s] the line typed after the read did not run" % tag)


@case("precmd_secret_honor_ps1", order=76, gui=True)
def c_precmd_secret_honor_ps1(ctx, r):
    """#1923 with the shell's own prompt: a bash precmd function's `read -s -p "vault pin: "`, the PIN typed in two halves around a pause longer than the prompt
    hold's give-up. Arm hp0: honor PS1 off (the app's prompt). Arm hp1: honor PS1 on (settings.toml terminal.input.honor_ps1 = true, app relaunched; the pane
    itself reports WARP_HONOR_PS1=1). Per arm: the question is on screen (both modes; #2053: in an agent conversation while the read waits, judged before a key
    where the tty is sampled and after the first half of the PIN everywhere), the `read` got the PIN whole and in order, no
    Enter found the PIN in the input editor, no command the shell received holds it, and the next typed line runs. The settings file is put back at the end."""
    if ctx.win and ctx.pane != "gitbash":
        r.skip("bash panes only (a precmd function's `read -s`): on Windows the Git Bash pane")
    R.need_relaunch_app(ctx, "precmd_secret_honor_ps1 relaunches the app with terminal.input.honor_ps1 set")
    p = R.settings_file(ctx)
    with open(p, encoding="utf-8", errors="replace") as f:
        original = f.read()
    r.metric("honor_ps1_in_profile", R.T.get_key(original, "terminal.input", "honor_ps1") or "absent")
    touched = False
    force_bash = None        # decided once, by the first probe: a POSIX pane that is not bash (macOS: zsh) gets session.startup_shell_override = "bash" in BOTH arms
    try:
        for want in (False, True):
            tag = "hp1" if want else "hp0"
            n = names(tag)
            touched = True
            state = probe_mode(ctx, n)
            honor_now, is_bash = state if state else (None, False)
            if force_bash is None:
                force_bash = not ctx.win and not is_bash
                r.metric("pane_made_bash", force_bash)
            if arm_plan(honor_now, is_bash, want):
                if not relaunch_with(ctx, r, settings_for(original, want, force_bash), "pp_" + tag):
                    return
                state = probe_mode(ctx, n)
                honor_now, is_bash = state if state else (None, False)
            r.metric("pane_%s" % tag, "no probe answer" if state is None else "honor_ps1=%s bash=%s" % (honor_now, is_bash))
            if arm_plan(honor_now, is_bash, want):
                r.unverifiable("[%s] the pane is not a bash with WARP_HONOR_PS1=%d after the relaunch with terminal.input.honor_ps1 = %s (the pane says: %s): this arm was not run"
                               % (tag, int(want), "true" if want else "false", "nothing, the probe line had no effect" if state is None
                                  else "honor_ps1=%s bash=%s" % (honor_now, is_bash)))
                continue
            if ctx.mode == "agent" and ctx.mode_now("pp_%s_mode" % tag) == "terminal":
                r.unverifiable("[%s] the agent pass's pane shows a terminal session (after the case's own relaunch): this arm was not run in agent mode" % tag)
                continue
            pin_scenario(ctx, r, tag, n)
    finally:
        # The profile as the pass had it, and a fresh pane, whatever happened: the function, a `read` left waiting by a failed arm and the bash of the macOS
        # arms all go with the old instance. One of the two arms always relaunches (they want different prompt modes), so this costs no pass an extra launch
        # it could have avoided.
        if touched:
            try:
                relaunch_with(ctx, r, original, "pp_restore")
            except Exception as e:
                r.check(False, "could not put the profile's settings.toml back and relaunch: %r" % (e,))


# ---- #2052: an ECHOING question from a precmd function (docs/design/r14-readmark.md) ----------------------------------------------------------------------

ECHO_QUESTION = "continue? "
ECHO_QUESTION_WORD = "continue"     # no line the case TYPES contains it (the function is sourced from a file)
MARKER_HOP = "hop=prompt_reader_asks by=shell_marker"
MARKER_IGNORED_HOP = "hop=tty_reader_marker_ignored"


def echo_names(tag):
    """pure: the files of one arm, all relative to the pane's case dir (ctx.W)"""
    return dict(script="qe_%s.sh" % tag, installed="qe_%s_inst.txt" % tag, err="qe_%s_err.txt" % tag, go="qe_%s_go.txt" % tag, asked="qe_%s_asked.txt" % tag,
                answer="qe_%s_ans.txt" % tag, after="qe_%s_after.txt" % tag, mode="qe_%s_mode.txt" % tag)


def echo_read_line(shell):
    """pure: the echoing question in the shell's own spelling (zsh's `read -p` reads from the coprocess; its prompt form is `name?prompt`)"""
    return 'read "x?%s"' % ECHO_QUESTION if shell == "zsh" else 'read -p "%s" x' % ECHO_QUESTION


def echo_install_script(n, shell):
    """pure: the file the pane sources (bash or zsh). The function asks ONCE, at the first precmd that finds the trigger file, with echo on; it records that
    it is about to ask and what the `read` returned. The last line proves the file was sourced."""
    return ('__qe_ask() { if [ -f %(go)s ]; then rm -f %(go)s; echo asked > %(asked)s; %(read)s; echo "$x" > %(answer)s; fi; }\n'
            'precmd_functions+=(__qe_ask)\n'
            'echo installed > %(installed)s\n') % dict(n, read=echo_read_line(shell))


def shell_probe_line(fname):
    """pure: the line that records which shell the pane runs. Valid in bash and zsh; `$`, quotes and `>` make it shell syntax in an agent conversation too."""
    return 'echo "sh=${BASH_VERSION:+bash}${ZSH_VERSION:+zsh}" > %s' % fname


def parse_shell_probe(lines):
    """pure: "bash" / "zsh" from the probe file's lines, "" for another shell, None when the file holds no probe line"""
    for ln in reversed(lines or []):
        m = re.match(r"^sh=(\S*)\s*$", ln)
        if m:
            return m.group(1) if m.group(1) in ("bash", "zsh") else ""
    return None


def echo_question_in(ocr_text):
    """pure: is the echoing question's word in an OCR text (case and line breaks ignored)?"""
    return ECHO_QUESTION_WORD in " ".join((ocr_text or "").lower().split())


def echo_arms(win, own_shell):
    """pure: the arms of precmd_question_echo as (tag, wanted shell, needs a relaunch with the bash override). The pane's own shell always; on macOS, whose
    pane is zsh, also the system bash (3.2), the bash the wrapper's semantics were not run against anywhere else."""
    arms = [("own", own_shell, False)]
    if not win and own_shell != "bash":
        arms.append(("bash", "bash", True))
    return arms


def marker_noted(log_text):
    """pure: did the app note the reader from the shell's marker (True), refuse a marker (the `why=` values, a list), or log neither (None)?"""
    t = log_text or ""
    if MARKER_HOP in t:
        return True
    whys = re.findall(re.escape(MARKER_IGNORED_HOP) + r"\s+why=(\w+)", t)
    return whys or None


def probe_shell(ctx, n):
    """type the shell probe into the pane; -> "bash" / "zsh" / "" or None when the line had no effect within 8 s"""
    ctx.rm(n["mode"])
    ctx.be.guard()
    ctx.be.type(shell_probe_line(n["mode"]), 12, enter=True)
    ctx.until(lambda: parse_shell_probe(ctx.lines(n["mode"])) is not None, 8, step=0.2)
    return parse_shell_probe(ctx.lines(n["mode"]))


def echo_scenario(ctx, r, tag, n, shell):
    """one arm, in a pane whose shell (bash or zsh) was already proven. Every message carries the arm's tag."""
    ctx.rm(n["installed"], n["err"], n["go"], n["asked"], n["answer"], n["after"])
    with open(os.path.join(ctx.W, n["script"]), "w", newline="\n") as f:
        f.write(echo_install_script(n, shell))
    answer = "ok" + ctx.tok(6).replace("_", "").lower()
    mark = ctx.cmd_mark()
    ctx.be.type("source ./%s 2> %s" % (n["script"], n["err"]), 12, enter=True)
    if not r.check(bool(ctx.until(lambda: ctx.lines(n["installed"]) == ["installed"], 10, step=0.2)),
                   "[%s] the line that installs the precmd function had no effect in the shell (stderr file: %r): the answer was NOT typed" % (tag, ctx.lines(n["err"])[:3])):
        return
    time.sleep(2.0)        # the install line's own prompt has passed: from the next prompt on the function runs inside the bootstrap's user stage
    off = ctx.log_size()
    ctx.be.type("touch " + n["go"], 12, enter=True)
    if not r.check(bool(ctx.until(lambda: ctx.lines(n["asked"]) == ["asked"], 10, step=0.2)),
                   "[%s] the precmd function never reached its `read` after `touch %s` (no %s): the answer was NOT typed" % (tag, n["go"], n["asked"])):
        return
    time.sleep(0.6)
    # The question is early output in a background block. Terminal mode draws it; an agent conversation shows it while a reader ahead of the line editor is
    # noted (#2053), and the marker notes that reader at the question's own output on every platform. Judged before a key in both modes.
    txt = ctx.ocr("qe_%s_asked" % tag)
    if txt is None:
        ctx.shot("qe_%s_asked" % tag)
        r.note("[%s] no OCR engine: whether the question is drawn is in the screenshot qe_%s_asked only" % (tag, tag))
    else:
        seen = echo_question_in(txt)
        r.metric("echo_question_on_screen_%s" % tag, seen)
        r.check(seen, "[%s] the question %r is not on screen while the shell waits for the answer (OCR of qe_%s_asked, %s mode): the user cannot see what is "
                "asked (agent mode: no `hop=reader_question_in_conversation shown=true` in the app log means no reader was noted)" % (tag, ECHO_QUESTION, tag, ctx.mode))
    ctx.be.type(answer, ctx.rng.randint(10, 25), enter=True)
    ok = ctx.until(lambda: ctx.lines(n["answer"]) == [answer], 20, step=0.3)
    ctx.shot("qe_%s_done" % tag)
    r.check(bool(ok), "[%s] the answer did not reach the echoing `read` whole and in order: %s holds %r, want %r (#2052: the keys went to the input editor, "
            "stayed held, or were lost)" % (tag, n["answer"], ctx.lines(n["answer"]), [answer]))
    time.sleep(1.0)
    log = ctx.log_since(off)
    in_editor = pin_in_editor_enters(log, answer)
    r.check(not in_editor, "[%s] the answer (or half of it) was in the input editor when an Enter was pressed, input_type %s: typed into the editor and "
            "submitted%s" % (tag, ", ".join(in_editor), " TO THE MODEL" if "AI" in in_editor else ""))
    # #2040: while a reader ahead of the line editor is noted the app logs no text; an Enter with text in the editor then shows by its count.
    held = withheld_enters(log)
    r.check(not held, "[%s] an Enter was pressed with text in the input editor while a reader ahead of the line editor had the keys (the log withheld the "
            "text): %s (chars, input_type)%s" % (tag, held, " TO THE MODEL" if any(t == "AI" for _, t in held) else ""))
    if enter_text_logged(log) is False and not held:
        r.note("[%s] the app log's Enter lines carry no editor text and no count above 0: 'the answer was never in the editor at an Enter' rests on the counts" % tag)
    half = len(answer) // 2
    ran = [c for c in ctx.cmds_since(mark) if answer in c or answer[:half] in c or answer[half:] in c]
    r.check(not ran, "[%s] the answer (or half of it) reached the shell as a command: %r" % (tag, ran))
    # How the reader was noted: the path the fix is. Recorded, and said when it was another one; the judged facts are the ones above.
    noted = marker_noted(log)
    r.metric("echo_noted_by_marker_%s" % tag, noted is True)
    if noted is not True:
        r.note("[%s] the app log has no `%s` after the trigger (%s): the reader was not noted from the shell's marker" % (
            tag, MARKER_HOP, "the marker was refused: why=" + ",".join(noted) if noted else "no marker hop at all: the wrapper was not installed, "
            "or the function ran outside the stage"))
    tok = ctx.tok(8).lower()
    ctx.be.type(ctx.append_cmd("bash", tok, n["after"]), 12, enter=True)
    r.check(bool(ctx.until(lambda: tok in ctx.lines(n["after"]), 10)), "[%s] the line typed after the read did not run" % tag)


@case("precmd_question_echo", order=77, gui=True)
def c_precmd_question_echo(ctx, r):
    """#2052: a precmd function's ECHOING question (`read -p "continue? " x`; zsh `read "x?continue? "`) asked after the prompt's Precmd. The answer, typed at
    10-25 ms per key with its Enter, must reach the `read` whole and in order, never be in the input editor at an Enter (in an agent conversation that Enter
    sends it to the model), never be a command the shell received; the question is on screen in both modes; the next typed line runs. Arm `own`: the pane's
    own shell (bash or zsh). Arm `bash`, macOS only: the system bash 3.2 (session.startup_shell_override, app relaunched; the settings file is put back)."""
    if ctx.win and ctx.pane != "gitbash":
        r.skip("bash and zsh panes only (a precmd function's `read`): on Windows the Git Bash pane")
    own = probe_shell(ctx, echo_names("own"))
    r.metric("pane_shell", "no probe answer" if own is None else (own or "neither bash nor zsh"))
    original = None
    try:
        for tag, want, relaunch in echo_arms(ctx.win, own):
            n = echo_names(tag)
            shell = own
            if relaunch:
                R.need_relaunch_app(ctx, "precmd_question_echo's bash arm relaunches the app with session.startup_shell_override")
                with open(R.settings_file(ctx), encoding="utf-8", errors="replace") as f:
                    original = f.read()
                if not relaunch_with(ctx, r, R.T.set_key(original, "session", "startup_shell_override", '"bash"'), "qe_" + tag):
                    return
                shell = probe_shell(ctx, n)
            if shell not in ("bash", "zsh") or (relaunch and shell != want):
                r.unverifiable("[%s] the pane's shell is %s, want %s: this arm was not run" % (
                    tag, "unknown (the probe line had no effect)" if shell is None else (shell or "neither bash nor zsh"), want or "bash or zsh"))
                continue
            if ctx.mode == "agent" and ctx.mode_now("qe_%s_mode" % tag) == "terminal":
                r.unverifiable("[%s] the agent pass's pane shows a terminal session: this arm was not run in agent mode" % tag)
                continue
            r.metric("shell_%s" % tag, shell)
            echo_scenario(ctx, r, tag, n, shell)
    finally:
        if original is not None:      # the profile as the pass had it, and a fresh pane (the bash of the macOS arm goes with the old instance)
            try:
                relaunch_with(ctx, r, original, "qe_restore")
            except Exception as e:
                r.check(False, "could not put the profile's settings.toml back and relaunch: %r" % (e,))
