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
# Informational: a notes row + a metric on every case; it never fails a case by itself.
import glob
import re

from backend import is_modifier

_LINE = re.compile(r"input_order (keydown key=(\S+)|typed_characters)\b.* t=(\d+(?:\.\d+)?) seq=(\d+)")
_HOP_ENTER = re.compile(r"input_order hop=enter_begin deferred=(\w+)\b.* t=(\d+(?:\.\d+)?)")
# Cases whose whole point is key loss: a non-zero lost-before-app count there is flagged WARN so it is read next to their verdict.
LOSS_CASES = ("key_loss_control", "enter_loss_control", "fast_typing", "ssh_connect_typing", "cadence")


def app_keys(log_path, t_from, t_to):
    """[(wall_s, seq, key_name_or_None)] of the keys the app logged in [t_from, t_to] (modifier keydowns excluded), from the active log plus its
    rotated chunks; None when the app has no key trace at all (no log, or the debug target is off)."""
    if not log_path:
        return None
    parts = sorted(glob.glob(glob.escape(log_path) + ".in_session.*"),
                   key=lambda f: int(f.rsplit(".", 1)[1]) if f.rsplit(".", 1)[1].isdigit() else -1, reverse=True) + [log_path]
    seen_any, out = False, {}
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
                out.setdefault(int(m.group(4)), (t, key))      # keydown and its typed_characters share a seq: one physical key
    if not seen_any:
        return None
    return sorted((t, seq, key) for seq, (t, key) in out.items())


def app_enter_hops(log_path, t_from, t_to):
    """number of `hop=enter_begin` (a fresh, not deferred, Enter reaching the input editor) in [t_from, t_to]; None without a log"""
    if not log_path:
        return None
    parts = sorted(glob.glob(glob.escape(log_path) + ".in_session.*"),
                   key=lambda f: int(f.rsplit(".", 1)[1]) if f.rsplit(".", 1)[1].isdigit() else -1, reverse=True) + [log_path]
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


def join(calls, keys, enter_hops=0, slack_before=0.5, slack_after=3.0):
    """calls: the backend's records for one case; keys: app_keys() for the same wall window (or None). Returns a dict, or None when nothing was
    injected for the app (no calls, or every call was aimed at a reference terminal)."""
    mine = [c for c in calls if not c.get("ref")]
    if not mine:
        return None
    errs = sum(1 for c in mine if c.get("err"))
    pastes = sum(c.get("pastes", 0) for c in mine if not c.get("err"))
    sent = sum(c["chars"] + len(c["special"]) for c in mine if not c.get("err")) - pastes   # errored call posted nothing (errs); a paste chord is no per-char key
    sent_enter = sum(c["enters_posted"] for c in mine)
    if keys is None:
        return dict(sent=sent, enters_sent=sent_enter, seen=None, lost=None, extra=None, enters_seen=None, errs=errs, pastes=pastes)
    enters_kd = sum(1 for _t, _s, k in keys if k and k.lower() in ("enter", "return", "kp_enter"))
    enters_seen = enters_kd + (enter_hops or 0)
    seen = len(keys) + (enter_hops or 0)
    if sent <= 0 and pastes:
        # paste-only: no per-character app keys exist to compare
        return dict(sent=sent, enters_sent=sent_enter, seen=seen, lost=None, extra=None, enters_seen=enters_seen, errs=errs, pastes=pastes, na=True)
    return dict(sent=sent, enters_sent=sent_enter, seen=seen, lost=max(sent - seen, 0), extra=max(seen - sent, 0), enters_seen=enters_seen, errs=errs, pastes=pastes)


def window(calls):
    mine = [c for c in calls if not c.get("ref")]
    return (min(c["wall0"] for c in mine) - 0.5, max(c["wall1"] for c in mine) + 3.0) if mine else None


def account(calls, log_path, case_name=""):
    """-> (note text or None, metric dict or None) for the calls one case made"""
    w = window(calls)
    if w is None:
        return None, None
    j = join(calls, app_keys(log_path, *w), app_enter_hops(log_path, *w))
    if j is None:
        return None, None
    if j["seen"] is None:
        return ("injector sent %d keys (%d Enter); app key trace not available, so nothing to compare" % (j["sent"], j["enters_sent"]), j)
    if j.get("na"):
        return ("injector posted %d paste chord(s) (%d Enter; app saw %d): per-key accounting n/a, no per-character keys are expected" % (
            j["pastes"], j["enters_sent"], j["enters_seen"]), j)
    txt = "injector sent %d; app saw %d; lost-before-app %d (Enter: sent %d, app saw %d)%s" % (
        j["sent"], j["seen"], j["lost"], j["enters_sent"], j["enters_seen"], ("; %d injector call(s) errored" % j["errs"]) if j["errs"] else "")
    if j["pastes"]:
        txt += "; %d paste chord(s) not counted as keys" % j["pastes"]
    if j["extra"]:
        txt += "; app logged %d more than were injected (keys from an earlier case or another source)" % j["extra"]
    if j["lost"] and case_name in LOSS_CASES:
        txt = "WARN loss case: " + txt
    return txt, j
