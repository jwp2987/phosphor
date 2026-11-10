# appci/injacct.py: injector accounting (#1509). "injector sent N; app saw M; lost-before-app K" per case.
#
# The backends keep one record per injector call (backend.Backend.calls / <run>/inject.log): how many characters and which named keys were
# posted. The app logs every key it receives (`input_order keydown ... seq=N` / `typed_characters ... seq=N`, RUST_LOG=warp::input_order=debug).
# join() compares the two for the calls a case made. A key the injector posted that the app never logged was lost between the injector and the
# app (compositor, XTEST, focus, or the injector itself) - NOT inside Phosphor's input queue, which only sees keys the app logged.
# Counts only: typed characters are never compared by value here (they can be passwords; the content check is cadence/fast_typing's job).
# Enter (r18d8 validation): an Enter handled by the input editor logs NO `keydown key=enter` line; the app's own evidence of it is the
# `input_order hop=enter_begin ... deferred=false` hop (input.rs). An Enter that goes to the pty (alt screen, running program) logs the keydown
# instead. The two are disjoint, so the app-side Enter count is keydown-enters + enter_begin hops (deferred=true re-runs of a held Enter are not
# new keys). Paste chords post no per-character keys: they are excluded from "sent", and a paste-only row reports n/a, not 0.
# A notes row + a metric on every case, on every backend that records calls (Windows included). It never FAILS a case; since the harness audit (#5,
# #1912) a case outside LOSS_CASES whose checks passed while keys were lost before the app is UNVERIFIABLE (status noref), see loss_verdict().
#
# Round 10 (docs/design/r10-injector.md): what counts as "the app saw this key". Rounds 7-9 read 1-10 % of the keys as lost in most rows, with the SAME
# counts on macOS (CGEvent) and Wayland (wtype): the `keydown` line is written on the UNBOUND path only (core/app.rs handle_non_keybound_event), so
# every key a key binding took (the harness's own Ctrl-C / Ctrl-U reset before each case, Tab, Escape, Ctrl-D, paste chords) was "lost". One physical
# key that reached the app's key dispatch leaves exactly ONE of three lines, and they are disjoint by construction (core/app.rs handle_window_event /
# dispatch_keystroke):
#   `input_order keydown key=K ... seq=N`                        no binding took it (its `typed_characters` shares the seq)
#   `input_order hop=keydown_keybound dispatch_seq=D key=K ...`  a binding took it, or it is the first key of a pending chord (#1127; never an Enter)
#   `input_order hop=core_enter stage=dispatch_result handled=true`   an Enter a binding took (the input editor's, a menu's, a dialog's). A trace
#        without core_enter lines (hand-written, or older than #1482) falls back to the `enter_begin deferred=false` count.
# macOS has one more, earlier, hop: `input_order hop=os_key_event ... enter=<bool>` is written by -[NSApplication sendEvent:] for EVERY key-down the
# process dequeues, before AppKit picks a window, a menu or the input method for it. Where a log has it, THAT is "reached the app" (lost-before-app =
# sent - os_key_event), and the dispatch lines above give a second figure: key-downs that reached NSApplication and were never dispatched as a key
# event. A Cmd chord the main menu takes (Cmd-V) is one of those by design, so the injector's own count of Cmd chords is allowed; anything past it is
# a key the app received and did not dispatch, and is UNVERIFIABLE with its own sentence (loss_verdict).
# Round 12 (#2041, docs/design/r12-macssh.md): a Control chord can be a menu key equivalent too. Ctrl-C with the input editor in front is the "Clear editor"
# item's (CustomAction::ClearEditor), so the harness's own reset Ctrl-C, before and after every case, was 2 key-downs "never dispatched" and every macOS row
# outside the loss cases was NOREF (97 of 99 in round 11). The app now names such a key-down itself:
#   `input_order hop=keydown_menu ctrl=<bool> cmd=<bool>`      the main menu took it as an item's key equivalent (the item's action ran)
# That line is a fourth, disjoint kind of "dispatched". The Cmd-chord allowance is what the app did NOT account for: it shrinks by the Cmd menu lines, so
# no key is excused twice. A log without the line (a build before #2041) is judged exactly as before.
# A paste chord is a key like any other now (it leaves a keydown_keybound line, or on macOS an os_key_event): it is no longer taken out of `sent`.
import glob
import re

from backend import is_modifier

_LINE = re.compile(r"input_order (keydown key=(\S+)|typed_characters)\b.* t=(\d+(?:\.\d+)?) seq=(\d+)")
_HOP_ENTER = re.compile(r"input_order hop=enter_begin deferred=(\w+)\b.* t=(\d+(?:\.\d+)?)")
_KEYBOUND = re.compile(r"input_order hop=keydown_keybound dispatch_seq=(\d+) key=(.*?) ctrl=\w+\b.* t=(\d+(?:\.\d+)?)")
_CORE_ENTER = re.compile(r"input_order hop=core_enter stage=dispatch_result handled=(\w+)\b.* t=(\d+(?:\.\d+)?)")
_OS_KEY = re.compile(r"input_order hop=os_key_event age_ms=\S+ enter=(\w+)\b.* t=(\d+(?:\.\d+)?)")
_MENU = re.compile(r"input_order hop=keydown_menu ctrl=(\w+) cmd=(\w+)\b.* t=(\d+(?:\.\d+)?)")
_ENTER_NAMES = ("enter", "return", "kp_enter", "numpadenter")
# Cases whose whole point is key loss: a non-zero lost-before-app count there is flagged WARN so it is read next to their verdict.
LOSS_CASES = ("key_loss_control", "enter_loss_control", "fast_typing", "ssh_connect_typing", "cadence")


def _groups(log_path):
    """r18e9: log_path may be one path or a list (the case's earlier launches' log copies, then the live log); each is a separate app instance with its OWN seq counter"""
    return [p for p in (log_path if isinstance(log_path, (list, tuple)) else [log_path]) if p]


def app_keys(log_path, t_from, t_to):
    """[(wall_s, seq, key_name_or_None)] of the keys the app logged in [t_from, t_to] (modifier keydowns excluded), from the active log plus its
    rotated chunks; None when the app has no key trace at all (no log, or the debug target is off)."""
    groups = _groups(log_path)
    if not groups:
        return None
    seen_any, out = False, {}
    for gi, gp in enumerate(groups):
        parts = sorted(glob.glob(glob.escape(gp) + ".in_session.*"),
                       key=lambda f: int(f.rsplit(".", 1)[1]) if f.rsplit(".", 1)[1].isdigit() else -1, reverse=True) + [gp]
        for f in parts:
            try:
                txt = open(f, errors="replace").read()
            except OSError:
                continue
            for m in _LINE.finditer(txt):
                seen_any = True
                key = m.group(2)
                if key is not None and is_modifier(key):
                    continue
                t = float(m.group(3)) / 1000.0
                if t_from <= t <= t_to:
                    out.setdefault((gi, int(m.group(4))), (t, key))      # keydown and its typed_characters share a seq: one physical key; seq restarts per launch (gi)
    if not seen_any:
        return None
    return sorted((t, seq[1], key) for seq, (t, key) in out.items())


def app_enter_hops(log_path, t_from, t_to):
    """number of `hop=enter_begin` (a fresh, not deferred, Enter reaching the input editor) in [t_from, t_to]; None without a log"""
    groups = _groups(log_path)
    if not groups:
        return None
    parts = []
    for gp in groups:
        parts += sorted(glob.glob(glob.escape(gp) + ".in_session.*"),
                        key=lambda f: int(f.rsplit(".", 1)[1]) if f.rsplit(".", 1)[1].isdigit() else -1, reverse=True) + [gp]
    n = 0
    for f in parts:
        try:
            txt = open(f, errors="replace").read()
        except OSError:
            continue
        for m in _HOP_ENTER.finditer(txt):
            if m.group(1) != "true" and t_from <= float(m.group(2)) / 1000.0 <= t_to:
                n += 1
    return n


def _files(gp):
    """one app instance's log files, oldest first: its rotated chunks, then the live file"""
    return sorted(glob.glob(glob.escape(gp) + ".in_session.*"),
                  key=lambda f: int(f.rsplit(".", 1)[1]) if f.rsplit(".", 1)[1].isdigit() else -1, reverse=True) + [gp]


def app_trace(log_path, t_from, t_to):
    """the key evidence beside app_keys() in [t_from, t_to] (see the header): dict(
         bound=[(wall_s, key)]   keys a binding took (`keydown_keybound`; modifier names excluded),
         bound_enters=n | None   Enters a binding took (`core_enter dispatch_result handled=true`); None when the trace has no core_enter lines at all,
         os=(n, n_enter) | None  key-downs -[NSApplication sendEvent:] dequeued (`os_key_event`); None when the trace has none at all (not macOS),
         menu=(n, n_cmd)         key-downs the main menu took as a key equivalent (`keydown_menu`, #2041), and how many of them carried Cmd).
    None without a log."""
    groups = _groups(log_path)
    if not groups:
        return None
    bound, n_be, have_core, n_os, n_os_enter, have_os, n_menu, n_menu_cmd = {}, 0, False, 0, 0, False, 0, 0
    for gi, gp in enumerate(groups):
        for f in _files(gp):
            try:
                txt = open(f, errors="replace").read()
            except OSError:
                continue
            for m in _KEYBOUND.finditer(txt):
                t = float(m.group(3)) / 1000.0
                if t_from <= t <= t_to and not is_modifier(m.group(2)):
                    bound.setdefault((gi, int(m.group(1))), (t, m.group(2)))      # dispatch_seq restarts per launch (gi)
            for m in _CORE_ENTER.finditer(txt):
                have_core = True
                if m.group(1) == "true" and t_from <= float(m.group(2)) / 1000.0 <= t_to:
                    n_be += 1
            for m in _OS_KEY.finditer(txt):
                have_os = True
                if t_from <= float(m.group(2)) / 1000.0 <= t_to:
                    n_os += 1
                    n_os_enter += m.group(1) == "true"
            for m in _MENU.finditer(txt):
                if t_from <= float(m.group(3)) / 1000.0 <= t_to:
                    n_menu += 1
                    n_menu_cmd += m.group(2) == "true"
    return dict(bound=sorted(bound.values()), bound_enters=n_be if have_core else None, os=(n_os, n_os_enter) if have_os else None, menu=(n_menu, n_menu_cmd))


def _enters_keyed(c):
    """Returns one macOS call posted through the k: op (keys(["Return"]), paste(enter=True)): htype2 logs them `KEY36` / `KEY76`, not `ENTER`, so
    enters_posted (the e: op) never had them and rows read "Enter: sent 0, app saw 12". Other backends name the key and count it themselves."""
    if c.get("err"):
        return 0
    return sum(1 for s in c.get("special") or [] if s.split("@")[0] in ("KEY36", "KEY76"))


def join(calls, keys, enter_hops=0, slack_before=0.5, slack_after=3.0, trace=None):
    """calls: the backend's records for one case; keys: app_keys() for the same wall window (or None); trace: app_trace() for it (None = keydown
    lines and enter_begin hops only, the pre-round-10 count). Returns a dict, or None when nothing was injected for the app (no calls, or every call
    was aimed at a reference terminal)."""
    mine = [c for c in calls if not c.get("ref")]
    if not mine:
        return None
    errs = sum(1 for c in mine if c.get("err"))
    pastes = sum(c.get("pastes", 0) for c in mine if not c.get("err"))
    sent = sum(c["chars"] + len(c["special"]) for c in mine if not c.get("err"))      # an errored call posted nothing (errs); a paste chord is one key
    sent_enter = sum(c["enters_posted"] + _enters_keyed(c) for c in mine)
    cmd_chords = sum(c.get("cmd_chords", 0) for c in mine if not c.get("err"))
    if keys is None:
        return dict(sent=sent, enters_sent=sent_enter, seen=None, lost=None, extra=None, enters_seen=None, errs=errs, pastes=pastes)
    tr = trace or {}
    bound = tr.get("bound") or []
    enter_bound = tr["bound_enters"] if tr.get("bound_enters") is not None else (enter_hops or 0)
    enters_kd = sum(1 for _t, _s, k in keys if k and k.lower() in _ENTER_NAMES)
    menu, menu_cmd = tr.get("menu") or (0, 0)
    dispatched = len(keys) + len(bound) + enter_bound + menu
    out = dict(sent=sent, enters_sent=sent_enter, errs=errs, pastes=pastes, dispatched=dispatched, unbound=len(keys), bound=len(bound), enters_bound=enter_bound)
    if tr.get("os") is not None:
        # macOS: the key-downs NSApplication dequeued are "reached the app"; the dispatch lines say what became of them. A key-down the menu took
        # and the app named (keydown_menu) is dispatched; a Cmd chord the injector posted that left NO line of its own may still be a menu's
        # (a build before #2041, or an item with a standard action), so only those are allowed, never the ones already counted.
        seen, enters_seen = tr["os"]
        undispatched = max(seen - dispatched, 0)
        allowed = max(cmd_chords - menu_cmd, 0)
        out.update(os_keys=seen, undispatched=undispatched, cmd_chords=cmd_chords, menu=menu, menu_cmd=menu_cmd, cmd_allowed=allowed,
                   app_undelivered=max(undispatched - allowed, 0))
    else:
        seen, enters_seen = dispatched, enters_kd + enter_bound
    out.update(seen=seen, lost=max(sent - seen, 0), extra=max(seen - sent, 0), enters_seen=enters_seen)
    return out


def window(calls, t_end=None, t_floor=None):
    """the wall window the app's keys are read in: 0.5 s before the first injector call to 3 s after the last, or to `t_end` (the moment of accounting,
    i.e. the case's end) when that is later. Audit #5: a held ssh-typeahead Enter or a slow-classifier Enter is processed more than 3 s after the last
    injector call, and the fixed 3 s cut it off ("Enter: sent 2, app saw 0").
    `t_floor` (the case's start): the window never starts before it. Rounds 8 and 9: the calls are sliced by index, the app's keys by time, so the
    0.5 s before a case's first call took in the tail of the harness's OWN previous line (the PID probe typed after the case before): every row,
    SKIP rows too, said "app logged 10-26 more than were injected" and "Enter: sent k, app saw k+1"."""
    mine = [c for c in calls if not c.get("ref")]
    if not mine:
        return None
    end = max(c["wall1"] for c in mine) + 3.0
    start = min(c["wall0"] for c in mine) - 0.5
    if t_floor is not None:
        start = max(start, min(t_floor, start + 0.5))       # never past the first call itself
    return (start, max(end, t_end) if t_end is not None else end)


def loss_verdict(j, case_name=""):
    """audit #5: the sentence for a case whose injector accounting shows keys (or Enters) the injector posted and the app never logged, or None.
    None for the loss cases (their own verdict measures it) and a run without an app key trace (nothing to compare).
    Round 10, macOS: a second sentence for key-downs that DID reach NSApplication and were never dispatched as a key event, beyond the Cmd chords
    the injector posted (a menu key equivalent is taken before the view sees it): that key was not lost before the app, it stopped inside it."""
    if not j or j.get("seen") is None or j.get("na") or case_name in LOSS_CASES:
        return None
    lost, el = j.get("lost") or 0, max((j.get("enters_sent") or 0) - (j.get("enters_seen") or 0), 0)
    if lost or el:
        return ("injector accounting: %d of %d injected key(s) never reached the app's key trace (Enter: sent %d, app saw %d) in a case that is not a "
                "key-loss case; its checks passed on effects alone, so the pass cannot be trusted (lost between the injector and the app: compositor, "
                "focus, or the injector)" % (max(lost, el), j["sent"], j["enters_sent"], j["enters_seen"]))
    if j.get("app_undelivered"):
        return ("injector accounting: %d key-down(s) reached the app (NSApplication sendEvent, hop=os_key_event) and were never dispatched as a key event "
                "(%d reached it, %d dispatched, of them %d as a menu key equivalent the app named with hop=keydown_menu; past those only %d of the %d Cmd "
                "chord(s) the injector posted may be taken by a menu before the view) in a case that is not "
                "a key-loss case; its checks passed on effects alone, so the pass cannot be trusted (stopped INSIDE the app: input method, a key-down "
                "with no characters, a native panel, no key window; or a menu key equivalent on a build without the keydown_menu line, #2041)" % (
                    j["app_undelivered"], j["os_keys"], j["dispatched"], j.get("menu") or 0, j.get("cmd_allowed", j.get("cmd_chords") or 0), j.get("cmd_chords") or 0))
    return None


def account(calls, log_path, case_name="", t_end=None, t_floor=None):
    """-> (note text or None, metric dict or None) for the calls one case made; t_end: wall time of the accounting (the case's end);
    t_floor: wall time the case started (keys the app logged before it are an earlier case's, see window())"""
    w = window(calls, t_end, t_floor)
    if w is None:
        return None, None
    j = join(calls, app_keys(log_path, *w), app_enter_hops(log_path, *w), trace=app_trace(log_path, *w))
    if j is None:
        return None, None
    if j["seen"] is None:
        return ("injector sent %d keys (%d Enter); app key trace not available, so nothing to compare" % (j["sent"], j["enters_sent"]), j)
    txt = "injector sent %d; app saw %d; lost-before-app %d (Enter: sent %d, app saw %d)%s" % (
        j["sent"], j["seen"], j["lost"], j["enters_sent"], j["enters_seen"], ("; %d injector call(s) errored" % j["errs"]) if j["errs"] else "")
    txt += "; of the app's: %d unbound keydown/typed, %d taken by a key binding, %d Enter by a binding" % (j["unbound"], j["bound"], j["enters_bound"])
    if "os_keys" in j:
        txt += "; macOS: %d key-down(s) reached NSApplication, %d dispatched as key events%s, %d not (%d Cmd chord(s) injected)" % (
            j["os_keys"], j["dispatched"], (" (%d of them a menu key equivalent)" % j["menu"]) if j.get("menu") else "", j["undispatched"], j["cmd_chords"])
    if j["pastes"]:
        txt += "; %d of the keys are paste chord(s)" % j["pastes"]
    if j["extra"]:
        txt += "; app logged %d more than were injected (keys from an earlier case or another source)" % j["extra"]
    if j["lost"] and case_name in LOSS_CASES:
        txt = "WARN loss case: " + txt
    return txt, j
