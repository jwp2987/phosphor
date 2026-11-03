# appci/taint.py: per-case "did every Return macOS targeted at Phosphor reach its NSApplication?" (r18e9).
#
# The macOS job runs `ktap` (Returns at the annotated session tap, with the process macOS targets) beside the run; mac/ci/enter_route.py joins it with the app's
# `hop=mac_enter stage=app_send_event ts=` lines AFTER the run. r18e9 validation: 437 Returns "never reached NSApplication", all for ONE pid, all after 14:05:06 -- the
# second the saved app log (phosphor.3.log, 10.4 MB) ends, for an app that kept running for 6 more minutes. So most of that window was an app-log GAP (nothing to join
# against), not a measured loss, and no case row said anything. This module classifies each such Return per case, while the run is live:
#   LOST      the case's app-log slice has app_send_event lines within 3 s of the Return's timestamp but none for it: the app really did not see it (key-event delivery loss)
#   UNCOVERED the slice has no app_send_event within 3 s: we cannot tell (log gap / log not written / app stalled); the cause is NOT established
# Either taints the case: its row says the app-side Enter evidence is not clean, and logs/enter_taint.jsonl keeps every entry (wall, ts, pid, class, app liveness).
import json
import os
import re

TOL = 0.0005          # the NSEvent timestamp is the CGEvent timestamp in seconds: 0.5 ms
NEAR_S = 3.0
_TAP = re.compile(r"E down keycode=\d+ ts_ns=(\d+) target_pid=(-?\d+) source_pid=-?\d+ wall=([\d.]+)")
_APP = re.compile(r"hop=mac_enter stage=app_send_event .* ts=([\d.]+)")


def tap_returns(ktap_path, t0, t1):
    """[(ts_s, target_pid, wall)] of the Return key-downs the tap saw with wall in [t0, t1]"""
    out = []
    try:
        for l in open(ktap_path, errors="replace"):
            m = _TAP.match(l)
            if m and t0 <= float(m.group(3)) <= t1:
                out.append((int(m.group(1)) / 1e9, int(m.group(2)), float(m.group(3))))
    except OSError:
        pass
    return out


def classify(taps, app_pids, slice_text):
    """-> dict(lost=[...], uncovered=[...], elsewhere=n, taps=n). Each entry is (ts, pid, wall)."""
    app_ts = [float(m.group(1)) for m in _APP.finditer(slice_text or "")]
    lost, unc, elsewhere = [], [], 0
    for ts, pid, wall in taps:
        if app_pids and pid not in app_pids:
            elsewhere += 1
            continue
        if any(abs(ts - a) <= TOL for a in app_ts):
            continue
        (lost if any(abs(ts - a) <= NEAR_S for a in app_ts) else unc).append((ts, pid, wall))
    return dict(lost=lost, uncovered=unc, elsewhere=elsewhere, taps=len(taps))


def case_taint(run, case, label, t0, t1, app_pids, slice_text, alive=None):
    """Classify the Returns of one case; append every non-clean one to <run>/logs/enter_taint.jsonl. -> (note or None, metric dict or None). Never raises."""
    try:
        ktap = os.path.join(os.path.dirname(os.path.abspath(run)), "ktap.log")
        if not os.path.exists(ktap):
            return None, None
        c = classify(tap_returns(ktap, t0 - 0.5, t1 + 1.0), set(app_pids or ()), slice_text)
        bad = c["lost"] + c["uncovered"]
        if not bad:
            return None, dict(taps=c["taps"], lost=0, uncovered=0, elsewhere=c["elsewhere"])
        os.makedirs(os.path.join(run, "logs"), exist_ok=True)
        with open(os.path.join(run, "logs", "enter_taint.jsonl"), "a") as f:
            for kind, rows in (("LOST", c["lost"]), ("UNCOVERED", c["uncovered"])):
                for ts, pid, wall in rows:
                    f.write(json.dumps(dict(case=case, mode=label, kind=kind, ts=ts, wall=wall, target_pid=pid, app_alive=alive, app_slice_chars=len(slice_text or ""))) + "\n")
        note = ("TAINTED (not a clean app signal): %d of %d Return(s) macOS targeted at Phosphor have no app_send_event in the case's app log: %d LOST (the log has neighbouring "
                "Returns within %.0f s, this one is missing: delivery loss) and %d UNCOVERED (no app_send_event near it at all: app-log gap or stall, cause NOT established; "
                "r18e9 saw 437 of these where the saved log simply ended). Each is in logs/enter_taint.jsonl; any Enter-dependent failure of this case is suspect." % (
                    len(bad), c["taps"], len(c["lost"]), NEAR_S, len(c["uncovered"])))
        return note, dict(taps=c["taps"], lost=len(c["lost"]), uncovered=len(c["uncovered"]), elsewhere=c["elsewhere"])
    except Exception as e:
        return "enter-taint check failed: %r" % (e,), None
