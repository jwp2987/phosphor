# script/ci/lat_evidence.py: the latency evidence lines of one case log, for the committed test-results/ archive.
# The archive keeps no whole app logs (size, privacy) and the job artifacts expire after 7 days, so a latency analysis (round 8 pulled jobs
# 17879373014/17879373016 by hand) lost its raw material. collect_results.py keeps, for the latency cases only, the lines the harness itself
# derives its latency metrics from (render_hops.py / lat.py): `input_order hop=...` and the frame hops. Filtered, capped, scrubbed. Pure functions.
import re

CASE_CAP = 2 * 1024 * 1024            # bytes of kept lines per case log (head and tail halves when over)
PLATFORM_CAP = 2 * 1024 * 1024        # bytes of evidence per platform and run (cases are visited in name order, each capped by what is left)
# cases whose metrics come from input_order / frame hops (script/realapp/appci/cases*.py)
LATENCY_CASES = {"latency_paint", "latency_paint_ab", "latency_enter", "latency_key_log", "agent_latency_key", "agent_latency_enter", "ctrl_c_flood_ab"}
LOG_PATH = re.compile(r"^(?:run|run_slim)/logs/[^/]+/case_([^/]+)\.log$")
LINE = re.compile(r"input_order hop=|\bframe_drawn\b|\bkey_frame_sched\b|\bkey_presented\b|\bkey_frame_now\b|\bframe \w+=")


def is_latency_case(case):
    return case in LATENCY_CASES or case.startswith(("latency_", "agent_latency_"))


def case_of(rel):
    """case name when `rel` (a path inside the platform's artifact folder) is a per-case app log of a latency case, else None"""
    m = LOG_PATH.match(rel)
    return m.group(1) if m and is_latency_case(m.group(1)) else None


def evidence(text, cap=CASE_CAP):
    """-> (kept text, matched lines, kept lines). Lines in order; over the cap the first and last halves are kept with a marker line between."""
    hits = [l for l in text.splitlines() if LINE.search(l)]
    out, size = [], 0
    for l in hits:
        size += len(l) + 1
    if size <= cap:
        return "\n".join(hits) + ("\n" if hits else ""), len(hits), len(hits)
    head, tail, h, t = [], [], 0, 0
    for l in hits:
        if h + len(l) + 1 > cap // 2:
            break
        head.append(l)
        h += len(l) + 1
    for l in reversed(hits[len(head):]):
        if t + len(l) + 1 > cap // 2:
            break
        tail.append(l)
        t += len(l) + 1
    tail.reverse()
    mid = "=== lat_evidence: %d of %d matching lines omitted (cap %d bytes) ===" % (len(hits) - len(head) - len(tail), len(hits), cap)
    return "\n".join(head + [mid] + tail) + "\n", len(hits), len(head) + len(tail)
