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
    ctx.be.type("touch " + n["go"], 12, enter=True)
    if not r.check(bool(ctx.until(lambda: ctx.lines(n["asked"]) == ["asked"], 10, step=0.2)),
                   "[%s] the precmd function never reached its `read` after `touch %s` (no %s): the PIN was NOT typed" % (tag, n["go"], n["asked"])):
        return
    time.sleep(0.6)
    # Is the question on screen? Read from the code it is, in either prompt mode: what is printed between the Precmd and the prompt is early output, drawn as its
    # own block. In an agent conversation such a block belongs to no command of the conversation; whether the view shows it there is recorded, not judged.
    txt = ctx.ocr("pp_%s_asked" % tag)
    if txt is None:
        ctx.shot("pp_%s_asked" % tag)
        r.note("[%s] no OCR engine: whether the question is drawn is in the screenshot pp_%s_asked only" % (tag, tag))
    else:
        seen = QUESTION_WORD in " ".join(txt.lower().split())
        r.metric("question_on_screen_%s" % tag, seen)
        if ctx.mode == "agent":
            if not seen:
                r.note("[%s] the question %r was not found on screen by OCR in the agent conversation (shots/..pp_%s_asked): recorded, not judged" % (tag, QUESTION, tag))
        else:
            r.check(seen, "[%s] the question %r is not on screen while the shell waits for the PIN (OCR of pp_%s_asked): the user cannot see what is asked" % (tag, QUESTION, tag))
    off = ctx.log_size()
    ctx.be.type(pin[:half], ctx.rng.randint(10, 25))
    time.sleep(HOLD_GIVE_UP_S + 1.2)      # past the give-up cap the first key armed (ConPTY); elsewhere just a user who pauses in the middle of a PIN
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
    logged = enter_text_logged(log)
    if logged is False:
        r.note("[%s] the app log carries no editor text (WARP_INPUT_ORDER_LOG_TEXT is off): 'the PIN was never in the editor at an Enter' was not judged" % tag)
    ran = [c for c in ctx.cmds_since(mark) if pin in c or pin[:half] in c or pin[half:] in c]
    r.check(not ran, "[%s] the PIN (or half of it) reached the shell as a command: %r" % (tag, ran))
    tok = ctx.tok(8).lower()
    ctx.be.type(ctx.append_cmd("bash", tok, n["after"]), 12, enter=True)
    r.check(bool(ctx.until(lambda: tok in ctx.lines(n["after"]), 10)), "[%s] the line typed after the read did not run" % tag)


@case("precmd_secret_honor_ps1", order=76, gui=True)
def c_precmd_secret_honor_ps1(ctx, r):
    """#1923 with the shell's own prompt: a bash precmd function's `read -s -p "vault pin: "`, the PIN typed in two halves around a pause longer than the prompt
    hold's give-up. Arm hp0: honor PS1 off (the app's prompt). Arm hp1: honor PS1 on (settings.toml terminal.input.honor_ps1 = true, app relaunched; the pane
    itself reports WARP_HONOR_PS1=1). Per arm: the question is on screen (terminal mode; recorded in agent mode), the `read` got the PIN whole and in order, no
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
