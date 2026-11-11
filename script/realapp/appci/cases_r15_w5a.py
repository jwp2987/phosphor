# appci/cases_r15_w5a.py: round 15, wave 5 part A of docs/design/r14-test-gaps.md 7.1 (#2076, #2077). Design note: docs/design/r15-w5a.md.
# All three are in cases.PROVING (owner rule 2026-11-09: a new case runs in the fix tier; graduation is the owner's).
#   keychain_startup_macos  T42 (#1977), macOS. The app is started with NO default keychain, with an unlocked test keychain as the default, and with
#                           that keychain locked. Each launch: first frame within a bound, typing exact, no secure-storage ERROR; with no keychain the
#                           one WARN line and the one notice; a locked keychain asks at most once (each prompt is cancelled and counted).
#                           The keychain is the harness's own, inside the private HOME the app runs with; the runner's keychain settings are read
#                           before and after and must not move.
#   keybinding_rebind       T45, all five panes. keybindings.yaml is read at launch. Control: the default new-tab chord opens a tab, the test chord
#                           does not, Ctrl-R opens command search. Arm A (new tab on the test chord): it opens a tab, the default chord does not.
#                           Arm B (command search moved, new tab on Ctrl-R): Ctrl-R opens a tab and no command search; the moved chord opens it.
#                           Arm C (round 16, #1988: new tab on Ctrl-R, command search NOT moved): the user's line wins, Ctrl-R opens a tab and no
#                           command search. Control and arm C (not macOS without Option as Meta): Alt-R opens the history search (#1988 B).
#                           The file is byte-identical after each launch.
#   editor_daily            T50, Linux, Windows (both panes), macOS. Find and replace by keyboard (#2088): one Enter replaces exactly one match, the
#                           replace-all key the rest, bytes exact, near-miss words untouched. Close with unsaved edits asks, whatever
#                           general.show_warning_before_quitting says (#2092); Escape keeps the edits, Enter closes (not on macOS: the question
#                           is a native dialog the harness cannot answer). A file changed on disk: a clean buffer is reloaded; a buffer with
#                           unsaved edits is flagged (#2087) and the disk is left alone; a save while flagged ASKS (#2092): nothing is written
#                           until the answer, Escape (Cancel) keeps disk and edits, Enter ALONE is Cancel too (owner decision 2026-11-11:
#                           disk untouched), Tab then Enter (Overwrite) writes the editor's text.
# Same rules as cases.py: the EXACT bytes, typing at 10-25 ms per key, a case that cannot run SKIPS with the capability that is missing ("NEEDS: ..."),
# and a verdict that rests on evidence that is absent is NOREF, never a pass.
import os, re, subprocess, sys, time

try:
    import pwd as _pwd_mod          # POSIX only; this module is imported on Windows too (keychain_startup_macos skips there)
except ImportError:
    _pwd_mod = None

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases as C  # noqa: E402  (cases.py imports this module as its last line, so everything below is already defined)
import cases_caseagent as CA  # noqa: E402
import cases_netless as NL  # noqa: E402
import cases_robust as R  # noqa: E402
import cases_ui as U  # noqa: E402
import cases_wrap_vtabs as WV  # noqa: E402
from cases import case  # noqa: E402


def typed(ctx, text, enter=True):
    """type at the owner's rate: 10-25 ms per key, Enter in the same call"""
    ctx.be.type(text, ctx.rng.randint(10, 25), enter=enter)


def view_rendered(text, view):
    """True if an app-log excerpt has a frame that rendered `view`; None if it has no `frame N: updated=[...]` line at all (no evidence either
    way); else False. (cases_ui.palette_rendered, for any view name.)"""
    if re.search(r'frame \d+: updated=\[[^\]]*"%s"' % re.escape(view), text):
        return True
    return False if re.search(r"frame \d+: updated=\[", text) else None


KEY_FATE_RE = re.compile(r'hop=keydown_keybound\b[^\n]*?key="?([^" ]+)"? ctrl=(true|false) alt=(true|false) shift=(true|false)[^\n]*?action_type=(\S+)')
KEY_DOWN_RE = re.compile(r'input_order keydown key="?([^" ]+)"? ctrl=(true|false) alt=(true|false) shift=(true|false)')
SEARCH_OPEN_NET_S = 8.0


def key_fate(text):
    """pure: one sentence on what the app's key trace says became of the keys in a log excerpt: every chord a key binding took, with the type of
    the action it dispatched (`hop=keydown_keybound`), and the chords that reached no binding (`keydown key=`). For a failure text: 'the binding
    fired and the view never rendered' and 'the chord was bound to nothing' are different defects."""
    def chord(m):
        return "+".join([n for n, on in (("ctrl", m.group(2)), ("alt", m.group(3)), ("shift", m.group(4))) if on == "true"] + [m.group(1)])
    bound = ["%s -> %s" % (chord(m), m.group(5).rsplit("::", 1)[-1]) for m in KEY_FATE_RE.finditer(text or "")]
    plain = [chord(m) for m in KEY_DOWN_RE.finditer(text or "")]
    if not bound and not plain:
        return "the app's key trace has no key in that time (the key never reached the app, or the key trace is off)"
    return "the app's key trace: taken by a binding: %s; reached no binding: %s" % (", ".join(bound[:4]) or "none", ", ".join(plain[:4]) or "none")


def await_view(ctx, off, view, net=SEARCH_OPEN_NET_S):
    """wait until a frame after log offset `off` rendered `view` (a give-up net of `net` seconds, not a settle: the first open of command search
    after a launch reads the whole history, which on a slow machine with a long history takes longer than any fixed sleep) -> (True | False |
    None as view_rendered, the log excerpt)"""
    ctx.until(lambda: view_rendered(ctx.log_since(off), view) is True, net, step=0.1)
    time.sleep(0.3)
    log = ctx.log_since(off)
    return view_rendered(log, view), log


# ================================================================ T45 keybinding_rebind
NEW_TAB_ACTION = "workspace:new_tab"                       # app/src/workspace/mod.rs NEW_TAB_BINDING_NAME (default ctrl-shift-T, cmd-t on macOS)
CMD_SEARCH_ACTION = "workspace:show_command_search"        # app/src/terminal/input.rs (default ctrl-r)
# every letter chord is one of the app's defaults, so the test chords are function keys nothing is bound to (selftest scans the Rust sources)
REBOUND = ("ctrl-shift-f8", "ctrl+shift+F8")               # (keybindings.yaml trigger, injector key spec)
MOVED = ("ctrl-shift-f9", "ctrl+shift+F9")
CTRL_R = ("ctrl-r", "ctrl+r")
ALT_R = "alt+r"                                            # #1988 B: the default key of input:search_command_history (meta-r on macOS)
ISSUE_CTRL_R = 1988


def alt_r_playable(backend_name, alt_acts_as_meta):
    """pure: -> None when Alt-R can be judged here, else why not. Off macOS the Alt key itself is the binding (alt-r), unless the profile turns the
    left Alt into Meta, where the fixed key is meta-r only on macOS. On macOS the binding is meta-r: Option-R with Option set to act as Meta."""
    if backend_name == "mac":
        return None if alt_acts_as_meta else "macOS: Option-R types a character by design unless Option is set to act as Meta (terminal.input.extra_meta_keys)"
    return "this profile sets the left Alt key to act as Meta: the key arrives as meta-r, and the Linux / Windows default is alt-r" if alt_acts_as_meta else None
SEARCH_VIEW = "CommandSearchView"                          # app/src/search/command_search/view.rs ui_name


def keybindings_yaml(pairs):
    """pure: the user file for [(action name, trigger)]: one quoted name per line, as the app writes it (app/src/keyboard.rs)"""
    return "".join('"%s": %s\n' % (name, trig) for name, trig in pairs)


def keybindings_path(ctx):
    return os.path.join(R.app_config_dir(ctx), "keybindings.yaml")


def tab_delta_verdict(before, after, want, what):
    """pure: -> failure text or None. before / after: tab counts (None = the app logged none); want: +1 or 0."""
    if before is None or after is None:
        return None
    if after - before == want:
        return None
    return "%s: the window went from %d to %d tab(s), expected %s" % (what, before, after, "one more" if want else "no change")


class Keys:
    """the steps of keybinding_rebind: a tab count, a key, a tab count; plus the prompt log where the pane has one"""

    def __init__(self, ctx, r):
        self.ctx, self.r, self.judged = ctx, r, 0

    def count(self):
        off = self.ctx.log_size()
        self.ctx.be.guard()
        self.ctx.be.key(R.tab_key(self.ctx, 9))
        time.sleep(0.6)
        return WV.window_tab_count(self.ctx, off)

    def press(self, spec, want, what):
        """press `spec`; the window must gain `want` tabs (1 or 0). -> (ok or None when nothing could judge it, app log since the key)"""
        ctx, r = self.ctx, self.r
        n0 = self.count()
        p0 = ctx.prof.prompts() if ctx.prof.local_prompts else None
        off = ctx.log_size()
        ctx.be.guard()
        ctx.be.key(spec)
        if want and p0 is not None:
            ctx.until(lambda: ctx.prof.prompts() > p0, 10, step=0.1)
        time.sleep(1.5 if p0 is not None else 4.0)
        log = ctx.log_since(off)
        n1 = self.count()
        bad = tab_delta_verdict(n0, n1, want, what)
        verdicts = []
        if n0 is not None and n1 is not None:
            verdicts.append(bad is None)
            r.check(bad is None, bad or "")
        if p0 is not None:
            dp = ctx.prof.prompts() - p0
            ok = dp >= 1 if want else dp == 0
            verdicts.append(ok)
            r.check(ok, "%s: the prompt log grew by %d (expected %s)" % (what, dp, "at least 1: a new tab's shell" if want else "0: no new shell"))
        if not want:
            ctx.reset()                # an unbound chord may have reached the line editor (F8 is an escape sequence to a shell): leave a clean line
            time.sleep(0.8)            # and let a prompt that the clean-up itself caused be logged before the next step takes its count
        if not verdicts:
            return None, log
        self.judged += 1
        return all(verdicts), log


def search_closed_probe(ctx, r, tag):
    """after a key that must NOT have opened command search: a typed effect line runs in the shell (a search panel would have taken the text)"""
    f = "kbr_%s.txt" % tag
    tok = "kbr" + ctx.tok(6).lower()
    ctx.rm(f)
    typed(ctx, ctx.append_cmd(ctx.pane_shell(), tok, f))
    ok = bool(ctx.until(lambda: tok in [l.strip() for l in ctx.lines(f)], 8))
    r.check(ok, "%s: a line typed after the key did not run in the shell (something else has the keyboard: command search?)" % tag)
    return ok


@case("keybinding_rebind", order=88, gui=True)
def c_keybinding_rebind(ctx, r):
    """T45: a change in keybindings.yaml takes effect at the next launch. Control on the profile as it is: the default new-tab chord opens a tab, the
    test chord (ctrl-shift-F8) opens none, Ctrl-R opens command search. Arm A, `workspace:new_tab: ctrl-shift-f8`: the test chord opens a tab, the
    default chord opens none. Arm B, `workspace:show_command_search: ctrl-shift-f9` and `workspace:new_tab: ctrl-r`: Ctrl-R opens a tab and no
    command search (a line typed next runs in the shell), ctrl-shift-F9 opens command search. Arm C (#1988, round 16), `workspace:new_tab: ctrl-r`
    alone: the user's line wins over the built-in default, Ctrl-R opens a tab and no command search. Control and arm C: Alt-R opens the history
    search (#1988 B; not on macOS without Option as Meta). The file is the same bytes after each launch, and the
    user's own file (if any) is put back at the end. Tab counts come from the app's `hop=tab_activate`, the prompt log where the pane has one."""
    R.need_relaunch_app(ctx, "keybinding_rebind")
    if ctx.pane == "cmd":
        r.skip("cmd.exe pane: no effect-line probe in one form for this pane; the PowerShell and Git Bash panes run this case")
    path = keybindings_path(ctx)
    try:
        orig = open(path, "rb").read()
    except OSError:
        orig = None
    k = Keys(ctx, r)
    newtab = R.NEWTAB
    relaunched = False
    try:
        # ---- control: the profile as it is
        ctx.reset()
        ok0, _ = k.press(REBOUND[1], 0, "control: the test chord %s with no binding" % REBOUND[1])
        ok1, _ = k.press(newtab, 1, "control: the default new-tab chord %s" % newtab)
        if ok1 is None:
            r.unverifiable("this pane has no prompt log and the app logged no hop=tab_activate line: nothing can count tabs, so no arm is judged")
            return
        if not ok1:
            return                                             # the default chord itself does not open a tab here: the arms would prove nothing
        off = ctx.log_size()
        ctx.be.guard()
        ctx.be.key(CTRL_R[1])
        search_seen, _ = await_view(ctx, off, SEARCH_VIEW)
        ctx.shot("kbr_control_ctrl_r")
        ctx.be.key("Escape")
        time.sleep(0.5)
        ctx.reset()
        r.metric("control_ctrl_r_opens_search", search_seen)
        # ---- control (#1988 B): Alt-R opens the history search, the same view filtered to history
        import cases_daily_b as D          # late: cases.py imports that module before this one, but keep this file importable on its own
        alt_why = alt_r_playable(ctx.be.name, D.alt_is_meta(D._settings_text(ctx)))

        def alt_r_opens_search(tag):
            if alt_why:
                r.note("%s: Alt-R not played: %s" % (tag, alt_why))
                return
            off = ctx.log_size()
            ctx.be.guard()
            ctx.be.key(ALT_R)
            seen, klog = await_view(ctx, off, SEARCH_VIEW)
            ctx.shot("kbr_%s_alt_r" % tag)
            ctx.be.key("Escape")
            time.sleep(0.5)
            ctx.reset()
            r.metric("%s_alt_r_opens_search" % tag, seen)
            if search_seen is True:
                r.check(seen is True, "%s: Alt-R did not open the history search (no frame rendered %s within %.0f s; #%d option B); %s" % (
                    tag, SEARCH_VIEW, SEARCH_OPEN_NET_S, ISSUE_CTRL_R, key_fate(klog)))
            else:
                r.unverifiable("%s: the control's Ctrl-R showed no %s frame in the app log, so 'Alt-R opens the history search' is NOT judged" % (tag, SEARCH_VIEW))

        alt_r_opens_search("control")
        # ---- arm A: new tab on the test chord
        text_a = keybindings_yaml([(NEW_TAB_ACTION, REBOUND[0])])
        relaunched = True
        if not _launch_with(ctx, r, path, text_a, "kbr_a"):
            return
        k.press(REBOUND[1], 1, "arm A: %s bound to new tab in keybindings.yaml" % REBOUND[1])
        k.press(newtab, 0, "arm A: the default chord %s after new tab moved off it" % newtab)
        _same_bytes(r, path, text_a, "arm A")
        # ---- arm B: command search moved, new tab on Ctrl-R
        text_b = keybindings_yaml([(CMD_SEARCH_ACTION, MOVED[0]), (NEW_TAB_ACTION, CTRL_R[0])])
        if not _launch_with(ctx, r, path, text_b, "kbr_b"):
            return
        ok, log = k.press(CTRL_R[1], 1, "arm B: Ctrl-R bound to new tab in keybindings.yaml")
        seen = view_rendered(log, SEARCH_VIEW)
        r.check(seen is not True, "arm B: Ctrl-R still opened command search although keybindings.yaml moved it to %s" % MOVED[0])
        search_closed_probe(ctx, r, "b_ctrl_r")
        off = ctx.log_size()
        ctx.be.guard()
        ctx.be.key(MOVED[1])
        moved_seen, klog = await_view(ctx, off, SEARCH_VIEW)
        ctx.shot("kbr_b_moved")
        ctx.be.key("Escape")
        time.sleep(0.5)
        ctx.reset()
        r.metric("arm_b_moved_opens_search", moved_seen)
        if search_seen is True:
            r.check(moved_seen is True, "arm B: %s did not open command search (no frame rendered %s within %.0f s) although keybindings.yaml binds it; %s" % (
                MOVED[1], SEARCH_VIEW, SEARCH_OPEN_NET_S, key_fate(klog)))
        else:
            r.unverifiable("the control's Ctrl-R showed no %s frame in the app log (frame lines off, or the view has another name): 'the moved chord opens "
                           "command search' is NOT judged; the tab arms were" % SEARCH_VIEW)
        _same_bytes(r, path, text_b, "arm B")
        # ---- arm C (#1988, owner decision 2026-11-11): new tab on Ctrl-R, command search NOT moved. The user's line wins over the built-in default,
        # although command search belongs to the focused input. Until round 16 this arm was "not played: an owner question".
        text_c = keybindings_yaml([(NEW_TAB_ACTION, CTRL_R[0])])
        if not _launch_with(ctx, r, path, text_c, "kbr_c"):
            return
        ok, log = k.press(CTRL_R[1], 1, "arm C: Ctrl-R bound to new tab in keybindings.yaml, command search left on its default (#%d)" % ISSUE_CTRL_R)
        seen = view_rendered(log, SEARCH_VIEW)
        r.check(seen is not True, "arm C: Ctrl-R opened command search although keybindings.yaml gives Ctrl-R to new tab (#%d: the user's binding wins)" % ISSUE_CTRL_R)
        search_closed_probe(ctx, r, "c_ctrl_r")
        alt_r_opens_search("arm_c")        # command search is still reachable: its history half on Alt-R
        _same_bytes(r, path, text_c, "arm C")
        r.metric("steps_judged", k.judged)
    finally:
        if relaunched:                 # the app runs on the case's file: stop it, put the user's file back (or none), start clean
            try:
                if ctx.app.alive():
                    R.stop_for_restart(ctx, r, 20, "kbr_restore")
                if orig is None:
                    if os.path.exists(path):
                        os.remove(path)
                else:
                    open(path, "wb").write(orig)
            except Exception as e:
                r.check(False, "could not put the keybindings file back: %r" % (e,))
            R.fresh_app(ctx, r, "kbr")


def _launch_with(ctx, r, path, text, label):
    q = R.stop_for_restart(ctx, r, 20, label)
    r.check(q["exited"], "%s: the app did not exit within 20 s" % label)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(text.encode())
    _, up = R.start_app(ctx, r, label)
    if up:
        log = ctx.log_since(0)
        r.check("Custom keybindings in" not in log and "unparsable keybinding" not in log, "%s: the app logged that it could not use the keybindings file" % label)
        ctx.reset()
    return up


def _same_bytes(r, path, text, label):
    try:
        now = open(path, "rb").read()
    except OSError:
        now = None
    r.check(now == text.encode(), "%s: keybindings.yaml is no longer the bytes the case wrote (%s): the app rewrote the user's file at launch"
            % (label, "missing" if now is None else "%d bytes, wrote %d" % (len(now), len(text.encode()))))


# ================================================================ T50 editor_daily
CLOSE_WARNING_LINE = "Displaying unsaved changes warning for pane"      # app/src/pane_group/mod.rs close_pane_with_confirmation (log::info!)
# Round 14 (#2109): with an editor pane in front the close key closes the editor TAB (pane_group close_active_pane_with_confirmation ->
# CodeViewAction::RemoveTabAtIndex -> code/view.rs remove_tab_with_confirmation). That question (Save / Don't save / Cancel) never logs the pane
# path's line above, so the case said "did not ask" while the question was on screen. The in-app dialog logs its own line whichever path asked
# (#2108, app/src/workspace/view.rs show_native_modal).
MODAL_SHOWN_LINE = "hop=native_modal event=shown"


def close_question_up(log):
    """pure: whether an app-log excerpt shows a close question coming up: the pane path's line, or the in-app dialog's own"""
    return CLOSE_WARNING_LINE in (log or "") or MODAL_SHOWN_LINE in (log or "")


# #2132 (owner decision 2026-11-11): the editor tab's close question has a keyboard selection. The dialog logs where Tab / Shift-Tab put it and
# which button was pressed by what (app/src/workspace/native_modal.rs selected_trace / pressed_trace; indexes only).
MODAL_SELECTED = re.compile(r"hop=native_modal event=selected button=(\d+)")
MODAL_PRESSED = re.compile(r"hop=native_modal event=pressed button=(\d+) by=(\w+)")
CLOSE_Q_SAVE, CLOSE_Q_DONT_SAVE, CLOSE_Q_CANCEL = 0, 1, 2      # app/src/quit_warning/mod.rs build(): Save, Don't Save, Cancel
DONT_SAVE_KEYS = ("Tab", "Tab", "shift+Tab")                  # from Save: Don't Save, Cancel, back to Don't Save
DONT_SAVE_WALK = [CLOSE_Q_DONT_SAVE, CLOSE_Q_CANCEL, CLOSE_Q_DONT_SAVE]
CLOSE_KEYS_SWITCH = "PHOSPHOR_CLOSE_QUESTION_KEYS"            # =0 in the app's environment: the question has no keyboard selection, as before #2132


def modal_selected(log):
    """pure: the buttons the in-app question's keyboard selection moved to, in order, from an app-log excerpt"""
    return [int(i) for i in MODAL_SELECTED.findall(log or "")]


def modal_pressed(log):
    """pure: [(button, by)] of the in-app question's pressed buttons, in order (by: pointer | enter | space | escape)"""
    return [(int(i), by) for i, by in MODAL_PRESSED.findall(log or "")]


def dont_save_walk_verdict(selected):
    """pure: what DONT_SAVE_KEYS did on the editor tab's close question, from the selection trace since the keys -> ('on_dont_save' | 'no_selection'
    | 'elsewhere', text). Only 'on_dont_save' may be followed by the pressing key: anywhere else it would press another answer."""
    if selected == DONT_SAVE_WALK:
        return "on_dont_save", "Tab, Tab, Shift+Tab walked Don't Save, Cancel, Don't Save"
    if not selected:
        return "no_selection", ("Tab / Shift+Tab moved no selection in the close question (no `hop=native_modal event=selected` line): a build before "
                                "#2132, %s=0, or the question up is not the editor tab's (Save / Don't Save / Cancel)" % CLOSE_KEYS_SWITCH)
    return "elsewhere", "Tab, Tab, Shift+Tab moved the selection through buttons %r, expected %r (a key was lost, or the buttons are others)" % (selected, DONT_SAVE_WALK)


def dont_save_verdict(pressed, end, kept, with_edit):
    """pure: what Space did with the selection on Don't Save -> ('discarded' | 'not_pressed' | 'wrong_button' | 'saved' | 'other', text). `pressed`:
    modal_pressed since the keys; `end`: the file after; `kept`: the last saved text (Don't Save leaves exactly it); `with_edit`: that text with the
    unsaved edit (what Save writes)."""
    if not pressed:
        return "not_pressed", "Space pressed no button of the close question (no `hop=native_modal event=pressed` line): the question is still up"
    if pressed != [(CLOSE_Q_DONT_SAVE, "space")]:
        return "wrong_button", "the close question's pressed buttons are %r, expected only Don't Save by Space %r" % (pressed, [(CLOSE_Q_DONT_SAVE, "space")])
    if end == with_edit:
        return "saved", "Don't Save was pressed and the unsaved edit is on disk all the same"
    if end != kept:
        return "other", "after Don't Save the file is neither the saved text nor the saved text plus the edit: %r" % end[:80]
    return "discarded", "Don't Save by keyboard dropped the edit and wrote nothing"


DISK_TRACE = re.compile(r"editor_disk outcome=(\w+) external=(true|false)(?: flagged=(true|false))?")     # app/src/code/global_buffer_model.rs disk_event_trace
SAVE_ASK_TRACE = re.compile(r"editor_save_ask outcome=(\w+)")                                             # app/src/code/save_conflict.rs save_ask_trace (#2092)
NO_CLOSE_QUESTION_MAC = ("macOS: the close question was not played. NEEDS (harness, not built): a usable close key (Phosphor's is cmd-shift-W, which the "
                         "frontmost-app guard refuses) and a way to answer the native dialog (r14-test-gaps T32)")


def ekeys(ctx):
    """the editor's chords on this platform (app/src/code/view.rs, editor/view/actions.rs, find/view.rs)"""
    if ctx.be.name == "mac":
        return dict(save="cmd+s", top="cmd+up", find="cmd+f", toggle_replace="cmd+alt+r", replace_all="cmd+alt+Return")
    return dict(save="ctrl+s", top="ctrl+Home", find="ctrl+shift+f", toggle_replace="ctrl+alt+r", replace_all="ctrl+alt+Return")


def replace_fixture(q):
    """pure: (text, occurrences) of the find-and-replace file for query `q` (lower-case ASCII, 6+ characters). Four matches; multi-byte text before
    the first (a byte / character offset mix-up lands the replacement in the wrong place); near-miss words that must stay: `q` without its last
    character, and `q` split by a space."""
    near1, near2 = q[:-1], q[:3] + " " + q[3:]
    text = ("head éü 日本 %s tail\n"
            "%s is shorter, %s has a space\n"
            "\t%s twice %s on one line\n"
            "last %s\n" % (q, near1, near2, q, q, q))
    return text, 4


def replaced_once(orig, got, q, rep):
    """pure: the index (0-based) of the ONE occurrence of `q` that `got` has replaced by `rep`, everything else byte-identical; None if `got` is
    not `orig` with exactly one occurrence replaced"""
    parts = orig.split(q)
    for i in range(len(parts) - 1):
        if q.join(parts[:i + 1]) + rep + q.join(parts[i + 1:]) == got:
            return i
    return None


def disk_events(text):
    """pure: [(outcome, external, flagged or None)] of the editor_disk trace lines of an app-log excerpt"""
    return [(o, e == "true", (f == "true") if f else None) for o, e, f in DISK_TRACE.findall(text)]


def external_verdict(events, waited_s):
    """pure: what the trace says about ONE external change made while the buffer had unsaved edits -> ('flagged' | 'not_flagged' | 'reloaded' |
    'no_trace', text). 'reloaded' means the app took the disk's text over the unsaved edits."""
    ext = [e for e in events if e[1]]
    if any(o == "conflict" and f for o, _e, f in ext):
        return "flagged", "the app flagged the change"
    if any(o == "reloaded" for o, _e, _f in ext):
        return "reloaded", "the app loaded the disk's text over a buffer that had unsaved edits"
    if any(o == "conflict" for o, _e, _f in ext):
        return "not_flagged", "the app refused the disk's text and flagged nothing (flagged=false: the kill switch, or a build before #2087's fix)"
    return "no_trace", "no editor_disk line for the change within %.0f s (a build without the trace, or the file watcher did not report it)" % waited_s


def save_ask_events(text):
    """pure: the outcomes, in order, of the editor_save_ask trace lines of an app-log excerpt (asked | overwrite | reload | cancel | not_shown |
    banner_overwrite)"""
    return SAVE_ASK_TRACE.findall(text)


def save_ask_verdict(events, disk_untouched):
    """pure: what ONE save pressed while the conflict banner shows did, from the trace since the key and whether the disk still holds the other
    program's text a moment later -> ('asked' | 'overwrote' | 'not_shown' | 'nothing', text). Only 'asked' is the decided behaviour (#2092): the
    question is up and nothing was written."""
    if not disk_untouched:
        return "overwrote", ("the save wrote the file %s (#2092: it must ask first; PHOSPHOR_EDITOR_SAVE_CONFLICT_ASK=0, or a build before the change)"
                             % ("after an answer nobody gave (%s)" % ",".join(events) if events else "without asking"))
    if "asked" in events:
        return "asked", "the save asked and wrote nothing"
    if "not_shown" in events:
        return "not_shown", "the app could not put the overwrite question on screen (editor_save_ask outcome=not_shown); nothing was written, and nothing told the user why"
    return "nothing", "the save neither asked (no editor_save_ask line) nor wrote the file: the save key did not reach the editor, or this build logs no editor_save_ask line"


SAVE_ASK_ANSWERS = ("overwrite", "reload", "cancel")


def enter_alone_verdict(events, disk_untouched):
    """pure: what Enter ALONE did on the overwrite question, from the trace since the key and whether the disk still holds the other program's
    text a moment later -> ('cancelled' | 'overwrote' | 'reloaded' | 'no_answer', text). Only 'cancelled' is the decided behaviour (owner decision
    2026-11-11: Enter is Cancel, never Overwrite). A changed disk is 'overwrote' whatever the trace says."""
    if not disk_untouched or "overwrite" in events:
        return "overwrote", ("Enter alone on the overwrite question wrote the file (answers since the key: %r). Owner decision 2026-11-11: Enter is Cancel, never "
                             "Overwrite (PHOSPHOR_EDITOR_OVERWRITE_ENTER_CANCELS=0, or a build before the change)" % (events,))
    if "reload" in events:
        return "reloaded", "Enter alone on the overwrite question answered Reload: the unsaved edits were dropped (expected Cancel)"
    if "cancel" in events:
        return "cancelled", "Enter alone cancelled and wrote nothing"
    return "no_answer", "Enter alone on the overwrite question gave no `editor_save_ask outcome=` answer within the wait (answers seen: %r): the key did not reach the question" % (events,)


def press_overwrite(ctx):
    """answer the overwrite question with Overwrite by keyboard: the question comes up with Cancel selected (Enter alone cancels), one Tab moves
    the selection on to Overwrite, Enter presses it. On a build before that change (or with PHOSPHOR_EDITOR_OVERWRITE_ENTER_CANCELS=0) Tab does
    nothing in the question and Enter is Overwrite, so the same two keys overwrite there too."""
    ctx.be.key("Tab")
    ctx.be.key("Return")


def clear_field(ctx, n):
    """empty the single-line field that has the keyboard, whatever it holds and whether or not it is selected: End, then more BackSpaces than it
    can have characters (a BackSpace in an empty field does nothing)"""
    ctx.be.key("End")
    ctx.be.key("BackSpace", repeat=n + 6, delay_ms=12)
    time.sleep(0.2)


def find_and_fill(ctx, k, q, rep):
    """the find bar by key with query `q`, then the replace row by key (#2088) with replacement `rep`; the keyboard is left in the replace field"""
    ctx.be.key(k["top"])
    ctx.be.key(k["find"])
    time.sleep(0.8)
    clear_field(ctx, max(len(q), len(rep)))
    typed(ctx, q, enter=False)
    time.sleep(0.6)
    ctx.be.key(k["toggle_replace"])
    time.sleep(0.6)
    clear_field(ctx, max(len(q), len(rep)))
    typed(ctx, rep, enter=False)
    time.sleep(0.4)


def replace_file(path, data, tries=20):
    """put `data` at `path` the way git or an editor's atomic save does: a new file renamed over the old one, so no reader ever sees half of it.
    A sharing violation (Windows, the app mid-read) is retried for about a second."""
    tmp = path + ".ext"
    with open(tmp, "wb") as f:
        f.write(data)
    for i in range(tries):
        try:
            os.replace(tmp, path)
            return
        except PermissionError:
            if i == tries - 1:
                raise
            time.sleep(0.05)


def open_editor(ctx, r, path, tag):
    """hand `path` to the running app and give the editor time to take the focus; -> ok"""
    ctx.be.guard()
    if ctx.be.name == "mac":
        err = C.open_file_in_app(ctx, path)
        ok = r.check(err is None, "%s: could not hand the file to the running app: %s" % (tag, err))
        time.sleep(2.5)
    else:
        ok = R.editor_open(ctx, r, path, tag)
    ctx.shot("edd_%s_open" % tag)
    return bool(ok)


def close_editor(ctx, r, tag, clean):
    """close the editor pane and prove the shell is in front. A pane that may hold unsaved edits is saved first on macOS (its close question is a
    native dialog); elsewhere a close question is answered with Enter."""
    k = ekeys(ctx)
    try:
        ctx.be.guard()
        if ctx.be.name == "mac":
            if not clean:
                off = ctx.log_size()
                ctx.be.key(k["save"])
                # #2092: a save on the conflict banner asks (the in-app dialog, on macOS too). The file is the case's own: Overwrite (Tab, Enter;
                # Enter alone is Cancel).
                if ctx.until(lambda: "asked" in save_ask_events(ctx.log_since(off)), 1.5):
                    press_overwrite(ctx)
                time.sleep(0.8)
            C.editor_close(ctx, r)
            time.sleep(1.0)
        else:
            off = ctx.log_size()
            ctx.be.key(R.CLOSE_PANE or "ctrl+shift+w")
            if ctx.until(lambda: close_question_up(ctx.log_since(off)), 1.5 if clean else 3.0):
                off2 = ctx.log_size()
                ctx.be.key("Return")               # the editor tab's question: Save (its first button); a pane's: Yes, close
                # the Save met the conflict banner (#2092): it asks before it overwrites. The file is the case's own: Overwrite (Tab, Enter)
                if ctx.until(lambda: "asked" in save_ask_events(ctx.log_since(off2)), 1.5):
                    press_overwrite(ctx)
            time.sleep(1.0)
    except Exception as e:
        r.note("%s: closing the editor pane raised %r" % (tag, e))
    return NL.shell_back_in_front(ctx, r, "edd_%s" % tag)


def _arm_replace(ctx, r, d):
    k = ekeys(ctx)
    q = "q" + ctx.tok(7).lower()
    rep = "Rep_%s_longer_than_the_query" % ctx.tok(5)
    orig, n = replace_fixture(q)
    path = os.path.join(d, "replace_%s.txt" % ctx.mode)
    with open(path, "wb") as f:
        f.write(orig.encode("utf-8"))
    if not open_editor(ctx, r, path, "replace"):
        return
    try:
        # one match: find, the replace row by key (#2088), the replacement, Enter
        find_and_fill(ctx, k, q, rep)
        ctx.shot("edd_replace_row")
        ctx.be.key("Return")
        time.sleep(0.6)
        ctx.be.key("Escape")
        time.sleep(0.5)
        ctx.be.key(k["save"])
        ctx.until(lambda: R.disk_bytes(path) != orig.encode("utf-8"), 6)
        time.sleep(0.3)
        got1 = R.disk_bytes(path).decode("utf-8", "replace")
        which = replaced_once(orig, got1, q, rep)
        r.metric("replace_one_index", which if which is not None else -1)
        if not r.check(which is not None, "find + replace-row key + Enter: the file is not the original with exactly one match replaced (%s). "
                       "Disk: %r" % ("unchanged: the replace row did not open by key (#2088), or Enter did not replace" if got1 == orig else
                                     "%d of %d matches left" % (got1.count(q), n), got1[:160])):
            return
        # the rest: replace all by key
        find_and_fill(ctx, k, q, rep)
        ctx.be.key(k["replace_all"])
        time.sleep(0.8)
        ctx.be.key("Escape")
        time.sleep(0.5)
        ctx.be.key(k["save"])
        want = orig.replace(q, rep)
        ctx.until(lambda: R.disk_bytes(path) == want.encode("utf-8"), 6)
        got2 = R.disk_bytes(path)
        ctx.shot("edd_replace_all")
        r.check(got2 == want.encode("utf-8"), "replace all by key: the file is not byte-exact (%d of %d matches left; near-miss words must be untouched). Disk: %r"
                % (got2.decode("utf-8", "replace").count(q), n - 1, got2[:200]))
    finally:
        close_editor(ctx, r, "replace", clean=False)


def _arm_close_question(ctx, r, d):
    k = ekeys(ctx)
    path = os.path.join(d, "close_%s.txt" % ctx.mode)
    with open(path, "wb") as f:
        f.write(R.EDIT_ORIG.encode())
    if not open_editor(ctx, r, path, "close"):
        return
    closed = False
    try:
        t1 = "KEEP_%s_" % ctx.tok(6)
        ctx.be.key(k["top"])
        typed(ctx, t1, enter=False)
        time.sleep(0.4)
        off = ctx.log_size()
        ctx.be.guard()
        ctx.be.key(R.CLOSE_PANE)
        asked = bool(ctx.until(lambda: close_question_up(ctx.log_since(off)), 4))
        ctx.shot("edd_close_question")
        if not asked:
            time.sleep(1.0)
            what = "unchanged: the edits were dropped without a question" if R.disk(path) == R.EDIT_ORIG else "changed: saved without a question"
            closed = True                                      # whatever happened, the pane is not in a known state: only the shell probe below
            # #2092 (owner decision 2026-11-11): unsaved editor edits ask whatever general.show_warning_before_quitting says
            # (app/src/quit_warning/mod.rs should_display_warning_for). Until round 16 a profile with the setting off was "not judged" here.
            off_note = (" general.show_warning_before_quitting is false in this profile; that setting no longer covers unsaved editor edits (#2092), so this "
                        "is a build before that change or one run with PHOSPHOR_UNSAVED_EDITS_ALWAYS_ASK=0."
                        if CA.setting_bool(ctx, "show_warning_before_quitting") is False else "")
            r.check(False, "the close key on an editor with unsaved edits did not ask (neither %r nor %r within 4 s). The file on disk is %s.%s"
                    % (MODAL_SHOWN_LINE, CLOSE_WARNING_LINE, what, off_note))
            return
        r.check(R.disk(path) == R.EDIT_ORIG, "while the close question is up the file on disk already changed")
        ctx.be.key("Escape")                                   # keep editing
        time.sleep(0.6)
        ctx.be.key(k["save"])
        ctx.until(lambda: R.disk(path) != R.EDIT_ORIG, 6)
        r.check(R.disk(path) == t1 + R.EDIT_ORIG, "after Escape on the close question a save wrote %r, expected the kept edit %r (the pane closed, or the edit was lost)"
                % (R.disk(path)[:60], (t1 + R.EDIT_ORIG)[:60]))
        t2 = "DROP_%s_" % ctx.tok(6)
        ctx.be.key(k["top"])
        typed(ctx, t2, enter=False)
        time.sleep(0.4)
        off = ctx.log_size()
        ctx.be.guard()
        ctx.be.key(R.CLOSE_PANE)
        if not r.check(bool(ctx.until(lambda: close_question_up(ctx.log_since(off)), 4)), "the second close key did not ask again (after Escape the "
                       "keyboard is not back in the editor, #2108, or the question is gone); no answer was sent"):
            return
        # #2132 (owner decision 2026-11-11): "Don't Save" by keyboard. The question comes up with Save selected; Tab, Tab, Shift+Tab walk Don't
        # Save, Cancel, Don't Save; Space presses it. Until round 19 this step pressed Enter and accepted "saved" or "discarded".
        off = ctx.log_size()
        for spec in DONT_SAVE_KEYS:
            ctx.be.key(spec)
        ctx.until(lambda: len(modal_selected(ctx.log_since(off))) >= len(DONT_SAVE_WALK), 3)
        time.sleep(0.3)
        sel = modal_selected(ctx.log_since(off))
        ctx.shot("edd_close_dont_save")                        # the focus ring is on Don't Save
        state, why = dont_save_walk_verdict(sel)
        r.metric("close_dont_save_walk", sel)
        if not r.check(state == "on_dont_save", "the close question by keyboard: %s; Space was NOT sent" % why):
            ctx.be.key("Escape")                               # Cancel loses nothing; close_editor below closes the pane
            time.sleep(0.6)
            return
        ctx.be.key("space")
        ctx.until(lambda: modal_pressed(ctx.log_since(off)), 3)
        time.sleep(1.5)
        pressed = modal_pressed(ctx.log_since(off))
        end = R.disk(path)
        state, why = dont_save_verdict(pressed, end, t1 + R.EDIT_ORIG, t2 + t1 + R.EDIT_ORIG)
        r.metric("close_confirm_outcome", state)
        if not r.check(state == "discarded", "Space on Don't Save: %s" % why):
            if state == "not_pressed":
                ctx.be.key("Escape")
                time.sleep(0.6)
            else:
                closed = True
            return
        closed = True
        if not NL.shell_back_in_front(ctx, r, "edd_close_dont_save"):
            return
        # Enter ALONE on the same question is Save, as before the decision (the selection starts on Save)
        closed = False
        if not open_editor(ctx, r, path, "close_enter"):
            closed = True
            return
        t3 = "SAVE_%s_" % ctx.tok(6)
        ctx.be.key(k["top"])
        typed(ctx, t3, enter=False)
        time.sleep(0.4)
        off = ctx.log_size()
        ctx.be.guard()
        ctx.be.key(R.CLOSE_PANE)
        if not r.check(bool(ctx.until(lambda: close_question_up(ctx.log_since(off)), 4)), "the close key on the reopened editor with a new edit did not ask; Enter was NOT sent"):
            return
        ctx.be.key("Return")
        ctx.until(lambda: R.disk(path) != t1 + R.EDIT_ORIG, 6)
        time.sleep(1.0)
        closed = True
        end = R.disk(path)
        pressed = modal_pressed(ctx.log_since(off))
        r.metric("close_enter_alone", "saved" if end == t3 + t1 + R.EDIT_ORIG else ("discarded" if end == t1 + R.EDIT_ORIG else "other"))
        r.check(end == t3 + t1 + R.EDIT_ORIG and pressed == [(CLOSE_Q_SAVE, "enter")], "Enter alone on the close question must be Save (its first button, selected when "
                "the question comes up): pressed %r, file %r, expected %r" % (pressed, end[:80], (t3 + t1 + R.EDIT_ORIG)[:80]))
    finally:
        if closed:
            NL.shell_back_in_front(ctx, r, "edd_close")
        else:
            close_editor(ctx, r, "close", clean=False)


def _wait_disk_event(ctx, off, pred, secs):
    ctx.until(lambda: pred(disk_events(ctx.log_since(off))), secs, step=0.2)
    return disk_events(ctx.log_since(off))


def _wait_save_ask(ctx, off, want, secs):
    ctx.until(lambda: want in save_ask_events(ctx.log_since(off)), secs, step=0.2)
    return save_ask_events(ctx.log_since(off))


def _wait_save_answer(ctx, off, secs):
    ctx.until(lambda: set(SAVE_ASK_ANSWERS) & set(save_ask_events(ctx.log_since(off))), secs, step=0.2)
    return save_ask_events(ctx.log_since(off))


def _arm_external(ctx, r, d):
    k = ekeys(ctx)
    path = os.path.join(d, "external_%s.txt" % ctx.mode)
    with open(path, "wb") as f:
        f.write(R.EDIT_ORIG.encode())
    if not open_editor(ctx, r, path, "external"):
        return
    clean = True
    ask_up = False                                             # the overwrite question may be on screen: Escape (Cancel) before anything else
    try:
        # (a) no unsaved edits: the change is loaded
        ext1 = "EXTERNAL_%s\n" % ctx.tok(6) + R.EDIT_ORIG
        off = ctx.log_size()
        replace_file(path, ext1.encode())
        ev = _wait_disk_event(ctx, off, lambda es: any(o == "reloaded" and e for o, e, _f in es), 10)
        traced = bool(ev)
        time.sleep(0.8 if traced else 3.0)                     # a build without the trace: give the watcher its time, the bytes below still judge
        t3 = "AFTER_%s_" % ctx.tok(6)
        ctx.be.key(k["top"])
        typed(ctx, t3, enter=False)
        time.sleep(0.4)
        clean = False
        ctx.be.key(k["save"])
        ctx.until(lambda: R.disk(path) != ext1, 6)
        time.sleep(0.3)
        base = R.disk(path)
        clean = True
        if not r.check(base == t3 + ext1, "a file changed on disk with no unsaved edits was not reloaded: an edit typed and saved afterwards wrote %r, expected it on top "
                       "of the new content %r (%s)" % (base[:70], (t3 + ext1)[:70], "the other program's change was overwritten without a word" if base == t3 + R.EDIT_ORIG else "neither old nor new content")):
            return
        # (b) unsaved edits: the change is flagged, the disk is left alone
        t4 = "DIRTY_%s_" % ctx.tok(6)
        ctx.be.key(k["top"])
        typed(ctx, t4, enter=False)
        time.sleep(0.5)
        clean = False
        ext2 = "SECOND_%s\n" % ctx.tok(6) + base
        off = ctx.log_size()
        replace_file(path, ext2.encode())
        ev = _wait_disk_event(ctx, off, lambda es: any(e for _o, e, _f in es), 10)
        state, why = external_verdict(ev, 10)
        r.metric("external_under_edits", state)
        ctx.shot("edd_external_banner")
        time.sleep(2.0)
        untouched = R.disk(path) == ext2
        r.check(untouched, "the app wrote the file on its own after another program changed it under unsaved edits: disk %r, expected the other program's %r" % (R.disk(path)[:70], ext2[:70]))
        if state == "no_trace" and not traced:
            r.unverifiable("this build logs no editor_disk line (#2087's trace), so 'a change under unsaved edits is flagged' is NOT judged; that the app left "
                           "the disk alone was")
        else:
            r.check(state == "flagged", "a file changed on disk while the editor had unsaved edits: %s (#2087). The next save would replace the other program's change" % why)
        if state != "flagged" or not untouched:
            return
        # (c) #2092 (owner decision 2026-11-11): a save while the banner shows ASKS (Overwrite / Reload / Cancel) and writes nothing until the
        # answer. Until round 16 this save was the banner's Overwrite and was recorded, not judged.
        want = t4 + base                                       # the editor's text
        off = ctx.log_size()
        ctx.be.guard()
        ctx.be.key(k["save"])
        ask_up = True
        ev = _wait_save_ask(ctx, off, "asked", 5)
        time.sleep(1.0)                                        # a silent overwrite has landed by now
        during = R.disk(path)
        state, why = save_ask_verdict(ev, during == ext2)
        r.metric("save_while_flagged", state)
        ctx.shot("edd_save_ask")
        if not r.check(state == "asked", "a save while the conflict banner shows: %s. Disk: %r" % (why, during[:70])):
            clean, ask_up = during == want, state != "overwrote"
            return
        # Escape = Cancel: nothing written, the edits and the banner stay
        off = ctx.log_size()
        ctx.be.key("Escape")
        ev = _wait_save_ask(ctx, off, "cancel", 4)
        ask_up = "cancel" not in ev
        time.sleep(0.8)
        if not r.check("cancel" in ev and R.disk(path) == ext2, "Escape on the overwrite question: %s; disk %r, expected the other program's text untouched %r"
                       % ("answered cancel" if "cancel" in ev else "no `editor_save_ask outcome=cancel` line within 4 s (answers seen: %r)" % (ev,), R.disk(path)[:70], ext2[:70])):
            clean = R.disk(path) == want
            return
        # a second save asks again (the edits are still there, the keyboard is back in the editor). Enter ALONE = Cancel (owner decision
        # 2026-11-11: never Overwrite): the disk keeps the other program's text
        off = ctx.log_size()
        ctx.be.guard()
        ctx.be.key(k["save"])
        ask_up = True
        ev = _wait_save_ask(ctx, off, "asked", 5)
        if not r.check("asked" in ev and R.disk(path) == ext2, "after Cancel a second save did not ask again (lines since the key: %r; disk %s): the keyboard did not go "
                       "back to the editor, or the banner state was lost" % (ev, "untouched" if R.disk(path) == ext2 else "CHANGED: %r" % R.disk(path)[:70])):
            clean, ask_up = R.disk(path) == want, R.disk(path) == ext2
            return
        off = ctx.log_size()
        ctx.be.key("Return")
        ev = _wait_save_answer(ctx, off, 4)
        ask_up = not (set(SAVE_ASK_ANSWERS) & set(ev))
        time.sleep(1.0)                                        # an overwrite has landed by now
        after_enter = R.disk(path)
        state, why = enter_alone_verdict(ev, after_enter == ext2)
        r.metric("save_ask_enter_alone", state)
        ctx.shot("edd_save_ask_enter")
        if not r.check(state == "cancelled", "%s. Disk: %r, expected the other program's text untouched %r" % (why, after_enter[:70], ext2[:70])):
            clean = after_enter == want
            return
        # a third save asks again; Overwrite is chosen explicitly: Tab (from Cancel the selection wraps to Overwrite), then Enter
        off = ctx.log_size()
        ctx.be.guard()
        ctx.be.key(k["save"])
        ask_up = True
        ev = _wait_save_ask(ctx, off, "asked", 5)
        if not r.check("asked" in ev and R.disk(path) == ext2, "after Enter (Cancel) a third save did not ask again (lines since the key: %r; disk %s): the keyboard did "
                       "not go back to the editor, or the banner state was lost" % (ev, "untouched" if R.disk(path) == ext2 else "CHANGED: %r" % R.disk(path)[:70])):
            clean, ask_up = R.disk(path) == want, R.disk(path) == ext2
            return
        press_overwrite(ctx)
        ctx.until(lambda: R.disk(path) != ext2, 6)
        time.sleep(0.3)
        ev = save_ask_events(ctx.log_since(off))
        ask_up = not ({"overwrite", "reload", "cancel"} & set(ev))
        end = R.disk(path)
        clean = end == want
        r.metric("save_ask_overwrite", "wrote" if end == want else ("kept" if end == ext2 else "other"))
        r.check("overwrite" in ev and end == want, "Tab then Enter (Overwrite) on the overwrite question: answers %r; disk %r, expected the editor's text %r%s"
                % (ev, end[:70], want[:70], " (answered cancel: Tab did not move the selection to Overwrite)" if "cancel" in ev else ""))
    finally:
        if ask_up:
            try:
                ctx.be.key("Escape")                           # Cancel loses nothing; with no question up Escape does nothing in the editor
                time.sleep(0.5)
            except Exception as e:
                r.note("external: Escape for a question that may be up raised %r" % e)
        close_editor(ctx, r, "external", clean=clean)


@case("editor_daily", order=73, gui=True)
def c_editor_daily(ctx, r):
    """T50: what a user does in the code editor beyond edit and save. (1) Find and replace by keyboard: the find key, the query, the replace-row key
    (#2088), the replacement, Enter: exactly ONE match is replaced; again with the replace-all key: every match, bytes exact, the near-miss words and
    the multi-byte text untouched. (2) Close with unsaved edits: the close key asks; Escape keeps the edits (a save then has them); the close key,
    then Tab, Tab, Shift+Tab and Space answers "Don't Save" by keyboard (#2132: the selection trace walks Don't Save, Cancel, Don't Save, Space
    presses it, the file keeps the saved text); reopened with a new edit, the close key and Enter ALONE saves it; the question comes whatever
    general.show_warning_before_quitting says (#2092). Not played on macOS (native dialog). (3) A file changed on disk: with no unsaved edits the
    editor reloads it (an edit typed and saved afterwards lands on the NEW content); with unsaved edits the change is flagged (`editor_disk
    outcome=conflict ... flagged=true`, #2087) and the app does not write the file on its own; a save in that state asks (`editor_save_ask
    outcome=asked`, #2092) and writes nothing, Escape cancels (disk untouched), a second save asks again and Enter ALONE cancels too (owner
    decision 2026-11-11: disk untouched), a third save asks again and Tab then Enter overwrites with the editor's text. Nothing may reach the
    shell."""
    if ctx.app is None or not hasattr(ctx.app, "start"):
        r.skip("needs the app (not the sim backend)")
    mac = ctx.be.name == "mac"
    if not mac and not ctx.win:
        R.need_linux_app(ctx, "editor_daily")
    if not mac and not R.CLOSE_PANE:
        r.skip(R.NO_CLOSE_WHY)
    d = os.path.join(ctx.W, "edd")
    os.makedirs(d, exist_ok=True)
    mark = ctx.cmd_mark()
    probes = lambda: [c for c in ctx.cmds_since(mark) if "edd_" not in c]
    _arm_replace(ctx, r, d)
    if mac:
        r.no_reference(NO_CLOSE_QUESTION_MAC)
    else:
        _arm_close_question(ctx, r, d)
    _arm_external(ctx, r, d)
    got = probes()
    r.check(not got, "keys meant for the editor reached the shell: %r" % got[:4])
    r.check(ctx.app.alive(), "the app died in the code editor")


# ================================================================ T42 keychain_startup_macos
KEYCHAIN_WARN_LINE = "The default keychain could not be opened"                 # crates/warpui_extras/src/secure_storage/mac.rs note_keychain_open
SECURE_READ_ERR = re.compile(r"Failed to read (?:local control mode|API keys) from secure storage")
KEYCHAIN_ARMS = ("none", "unlocked", "locked")
NEEDS_HOME = ("NEEDS: the Security framework must take the keychain search list from the HOME the app is started with (a keychain made with a relative name under "
              "the private HOME landed %s). Without that the case would have to change the runner user's own default keychain, which it never does")
NEEDS_AX = ("NEEDS: Accessibility permission for the runner's `osascript` (System Settings > Privacy & Security > Accessibility), so an unlock prompt can be counted "
            "and cancelled. Without it a prompt would stay up over every later case, so the locked arm was NOT played")


def parse_keychain_path(out):
    """pure: the path in one line of `security default-keychain` / `list-keychains` output (`    "/path/x.keychain-db"`), or None"""
    m = re.search(r'"([^"]+)"', out or "")
    return m.group(1) if m else None


def keychain_launch_facts(text):
    """pure: what ONE launch's app log says about the keychain: cases_ui.keyring_launch_facts (first frame, the keyring notice) plus the count of
    the 'could not be opened' line and of the readers' ERROR lines"""
    f = U.keyring_launch_facts(text)
    f["keychain_warn"] = text.count(KEYCHAIN_WARN_LINE)
    f["secure_read_errors"] = len(SECURE_READ_ERR.findall(text))
    return f


def keychain_launch_verdicts(facts, arm, first_frame_max_ms, prompts=None):
    """pure: -> failure messages for one launch (empty = pass). Every arm: the first frame drawn within the bound, no reader ERROR. `none`: the
    'could not be opened' line exactly once and the notice exactly once. `unlocked` / `locked`: neither. `locked`: at most one unlock prompt
    (prompts = how many were seen; None = not counted)."""
    out = []
    if facts["first_frame_ms"] is None:
        out.append("%s: no `startup timing` line: the first frame was never drawn" % arm)
    elif facts["first_frame_ms"] > first_frame_max_ms:
        out.append("%s: first frame after %d ms (bound %d ms)" % (arm, facts["first_frame_ms"], first_frame_max_ms))
    if facts["secure_read_errors"]:
        out.append("%s: %d 'Failed to read ... from secure storage' ERROR line(s) (#1977: a missing keychain is one WARN, not an ERROR per reader)" % (arm, facts["secure_read_errors"]))
    if arm == "none":
        if facts["keychain_warn"] != 1:
            out.append("none: the '%s' line was logged %d times (expected exactly once per launch)" % (KEYCHAIN_WARN_LINE, facts["keychain_warn"]))
        if facts["notices"] != 1:
            out.append("none: the keyring notice was shown %d times (expected exactly once)" % facts["notices"])
    else:
        if facts["keychain_warn"]:
            out.append("%s: the '%s' line was logged %d time(s) with a default keychain present" % (arm, KEYCHAIN_WARN_LINE, facts["keychain_warn"]))
        if facts["notices"]:
            out.append("%s: the keyring notice was shown %d time(s) with a default keychain present" % (arm, facts["notices"]))
    if arm == "locked" and prompts is not None and prompts > 1:
        out.append("locked: the unlock prompt came back %d times after being cancelled (at most one prompt per launch)" % prompts)
    return out


class Keychain:
    """the `security` tool seen with a given HOME. Every call is the tool's own; nothing is read from or written to a keychain item."""

    def __init__(self, home):
        self.home = home

    def run(self, *args, timeout=30):
        env = dict(os.environ, HOME=self.home)
        try:
            p = subprocess.run(["security"] + list(args), env=env, capture_output=True, text=True, timeout=timeout)
            return p.returncode, (p.stdout or "") + (p.stderr or "")
        except Exception as e:
            return 255, repr(e)

    def default(self):
        rc, out = self.run("default-keychain", "-d", "user")
        return parse_keychain_path(out) if rc == 0 else None

    def search_list(self):
        rc, out = self.run("list-keychains", "-d", "user")
        return [m for m in re.findall(r'"([^"]+)"', out)] if rc == 0 else []

    def view(self):
        return (self.default(), tuple(self.search_list()))


def real_home():
    try:
        return _pwd_mod.getpwuid(os.getuid()).pw_dir
    except Exception:
        return os.path.expanduser("~")


def keychain_files(home, stem):
    d = os.path.join(home, "Library", "Keychains")
    try:
        return sorted(os.path.join(d, f) for f in os.listdir(d) if f.startswith(stem))
    except OSError:
        return []


def home_redirects(priv, real, tok):
    """-> (True, where) when a keychain created with a RELATIVE name by `security` run with HOME=priv lands under priv; (False, where) when it lands
    under the real home (it is deleted again); (None, why) when it could not be created at all. Creating a keychain file changes no search list
    and no default."""
    stem = "appci-probe-%s" % tok
    kc = Keychain(priv)
    os.makedirs(os.path.join(priv, "Library", "Keychains"), exist_ok=True)
    rc, out = kc.run("create-keychain", "-p", tok, stem + ".keychain")
    if rc != 0:
        return None, "`security create-keychain` failed (exit %s): %s" % (rc, out.strip()[:160])
    in_priv, in_real = keychain_files(priv, stem), (keychain_files(real, stem) if os.path.realpath(real) != os.path.realpath(priv) else [])
    for f in in_priv + in_real:
        kc.run("delete-keychain", f)
        try:
            os.remove(f)
        except OSError:
            pass
    if in_priv and not in_real:
        return True, "under the private HOME"
    return False, ("under the runner user's real home" if in_real else "nowhere the case can find")


AX_COUNT = ('tell application "System Events"\n'
            '  if exists (process "SecurityAgent") then\n'
            '    return count of windows of process "SecurityAgent"\n'
            '  end if\n'
            '  return 0\n'
            'end tell')
AX_PREFLIGHT = 'tell application "System Events" to count windows of (first process whose frontmost is true)'
AX_CANCEL = ('tell application "System Events" to tell process "SecurityAgent"\n'
             '  try\n'
             '    click button "Cancel" of window 1\n'
             '  on error\n'
             '    key code 53\n'
             '  end try\n'
             'end tell')


def osa(script, timeout=20):
    try:
        p = subprocess.run(["osascript", "-e", script], capture_output=True, text=True, timeout=timeout)
        return p.returncode, (p.stdout or "").strip()
    except Exception as e:
        return 255, repr(e)


def prompt_windows():
    rc, out = osa(AX_COUNT)
    return int(out) if rc == 0 and out.isdigit() else 0


def watch_prompts(secs, count=prompt_windows, cancel=lambda: osa(AX_CANCEL), sleep=time.sleep, now=time.time):
    """watch for unlock prompts for `secs`; every prompt that shows is counted once and cancelled. -> (prompts seen, prompts that could not be
    cancelled). The clock restarts after each cancel, so a prompt that comes back is seen."""
    seen = stuck = 0
    t_end = now() + secs
    while now() < t_end:
        if count() > 0:
            seen += 1
            cancel()
            gone = False
            for _ in range(15):
                sleep(0.2)
                if count() == 0:
                    gone = True
                    break
            if not gone:
                stuck += 1
                break
            t_end = now() + secs
        sleep(0.3)
    return seen, stuck


@case("keychain_startup_macos", modes=("terminal",), order=86, gui=True)
def c_keychain_startup_macos(ctx, r):
    """T42 (#1977): three launches on macOS. `none`: no default keychain; `unlocked`: a test keychain the harness created is the default, unlocked;
    `locked`: the same keychain, locked. Each launch draws its first frame within APPCI_KEYCHAIN_FIRST_FRAME_MAX_MS (default 2000) and logs no
    'Failed to read ... from secure storage' ERROR; typing right after the first prompt is exact. `none` logs 'The default keychain could not be
    opened' exactly once and shows the one notice; the other two log neither. `locked`: unlock prompts are counted and cancelled, more than one is
    a failure. The keychain lives in the private HOME the app runs with; the runner user's own default keychain and search list are compared before
    and after and must be the same."""
    if ctx.be.name != "mac":
        r.skip("macOS only: Linux has keyring_unavailable_startup (the Secret Service variants); Windows keeps secrets in DPAPI-protected files, there is no keychain service to be absent or locked")
    R.need_relaunch_app(ctx, "keychain_startup_macos")
    import shutil
    if not shutil.which("security"):
        r.skip("NEEDS: /usr/bin/security on the runner")
    limit = int(os.environ.get("APPCI_KEYCHAIN_FIRST_FRAME_MAX_MS", "2000"))
    priv, real = ctx.prof.home, real_home()
    tok = ctx.tok(8).lower()
    ok, where = home_redirects(priv, real, tok)
    if not ok:
        ctx.skip_reason = "keychain HOME redirect"
        raise C.Skip(NEEDS_HOME % where if ok is False else "NEEDS: a working `security create-keychain` for the runner user (%s)" % where)
    real_kc, priv_kc = Keychain(real), Keychain(priv)
    real_before = real_kc.view() if os.path.realpath(real) != os.path.realpath(priv) else None
    test_kc = os.path.join(priv, "Library", "Keychains", "appci-test-%s.keychain-db" % tok)
    created = False
    ax_ok = osa(AX_PREFLIGHT)[0] == 0

    def launch(arm):
        t = U.relaunch(ctx, False)
        r.metric("%s_prompt_s" % arm, t if t is not None else -1)
        prompts = None
        if arm == "locked":
            seen, stuck = watch_prompts(12.0)
            prompts = seen
            if stuck:
                r.check(False, "locked: an unlock prompt could not be cancelled; the app is stopped so the prompt is withdrawn")
                ctx.app.stop()
                return
        if t is None:
            r.check(False, "%s: no shell prompt within 90 s" % arm)
            return
        ctx.be.key("Escape")
        U.exact_burst(ctx, r, "keychain_%s" % arm, 4)
        if arm == "none":
            ctx.until(lambda: U.KEYRING_NOTICE_LINE in U._app_log(ctx), 30, step=0.5)
            ctx.shot("keychain_none_notice")
        if arm == "locked":
            more, stuck = watch_prompts(5.0)
            prompts += more
            if stuck:
                r.check(False, "locked: an unlock prompt could not be cancelled; the app is stopped so the prompt is withdrawn")
                ctx.app.stop()
        else:
            time.sleep(2)
        text = U._app_log(ctx)
        facts = keychain_launch_facts(text)
        r.metric("%s_facts" % arm, dict(facts, slow_secure=facts["slow_secure"][:4], prompts=prompts))
        for msg in keychain_launch_verdicts(facts, arm, limit, prompts):
            r.check(False, msg)
        R.keep_log(ctx, "keychain_%s" % arm)

    try:
        # ---- none: only when the private HOME really has no default keychain
        if priv_kc.default() is None:
            launch("none")
        else:
            r.no_reference("the private HOME already has a default keychain, and the tool cannot take one away: the `none` arm was NOT played")
        # ---- the test keychain
        rc, out = priv_kc.run("create-keychain", "-p", tok, test_kc)
        created = rc == 0
        if not r.check(created, "could not create the test keychain (exit %s): %s" % (rc, out.strip()[:160])):
            return
        priv_kc.run("set-keychain-settings", test_kc)                       # no lock-on-sleep, no timeout
        priv_kc.run("list-keychains", "-d", "user", "-s", test_kc)
        priv_kc.run("default-keychain", "-d", "user", "-s", test_kc)
        if real_before is not None and real_kc.view() != real_before:
            r.check(False, "making the test keychain the default under the private HOME changed the RUNNER USER's keychain settings; they are being put back and no launch is made")
            return
        if os.path.realpath(priv_kc.default() or "") != os.path.realpath(test_kc):
            r.no_reference("the test keychain did not become the default under the private HOME (`security default-keychain` says %s): the unlocked and locked arms were NOT played"
                           % ("another keychain" if priv_kc.default() else "none"))
            return
        priv_kc.run("unlock-keychain", "-p", tok, test_kc)
        launch("unlocked")
        if not ax_ok:
            r.no_reference(NEEDS_AX)
            return
        priv_kc.run("lock-keychain", test_kc)
        launch("locked")
    finally:
        try:
            ctx.app.stop()                                                   # a pending unlock prompt goes with its client
            if created:
                priv_kc.run("unlock-keychain", "-p", tok, test_kc)
                priv_kc.run("delete-keychain", test_kc)
                for f in keychain_files(priv, "appci-test-%s" % tok):
                    try:
                        os.remove(f)
                    except OSError:
                        pass
            if real_before is not None:
                after = real_kc.view()
                if after != real_before:
                    r.check(False, "the runner user's keychain settings changed during the case (default or search list); putting them back")
                    if real_before[1]:
                        real_kc.run("list-keychains", "-d", "user", "-s", *real_before[1])
                    if real_before[0]:
                        real_kc.run("default-keychain", "-d", "user", "-s", real_before[0])
        except Exception as e:
            r.check(False, "keychain cleanup raised %r" % (e,))
        U.relaunch(ctx, False)
