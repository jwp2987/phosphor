# appci/replay.py: the pure parts of the "real-use replay" case (cases_replay.py, docs/testing.md "Real-use replay"): the seeded scenario generator and the
# oracles that read logs. No I/O, so selftest.py can pin all of it. A scenario is a list of step dicts; every step has a `kind`, the exact `line` typed (if any),
# and `eff`, the effect file + token the shell must end up with (the proof the line arrived whole).
import random, re

# kind -> (weight, estimated seconds at 150 ms RTT). Terminal pass kinds and agent pass kinds are separate lists: the agent pass types prose to a model and
# keeps the shell for approved commands only (autodetect routes anything shell-shaped in the terminal pass).
TERMINAL_KINDS = {"local_cmd": (6, 4), "cd": (3, 4), "tab": (3, 6), "ssh_session": (3, 40), "ctrlc": (2, 8), "big_seq": (1, 25), "big_cat": (1, 20),
                  "subshell": (1, 15), "sudo": (1, 10)}
AGENT_KINDS = {"agent_approve": (5, 25), "agent_reject": (3, 25), "agent_prose": (3, 12), "agent_ctrlc": (2, 15)}
SSH_INNER = {"remote_cmd": (6, 4), "remote_cd": (2, 4), "remote_tab": (2, 6), "remote_ctrlc": (1, 8), "remote_big": (1, 25)}
ERROR_ALLOW_DEFAULT = (r"Settings file (could not be parsed|cannot be used)", r"keyring", r"Failed to (connect|fetch).*(update|telemetry|rudder|sentry)",
                       r"no_proxy|ephemeral", r"Session abandoned before bootstrap")


def _pick(rng, kinds):
    names = sorted(kinds)
    return rng.choices(names, weights=[kinds[k][0] for k in names])[0]


def est_seconds(step):
    k = step["kind"]
    base = (TERMINAL_KINDS.get(k) or AGENT_KINDS.get(k) or SSH_INNER.get(k) or (0, 5))[1]
    return base + sum(est_seconds(s) for s in step.get("inner", []))


def _word(rng, n=6):
    return "".join(rng.choice("abcdefghijklmnopqrstuvwxyz0123456789") for _ in range(n))


def make_step(rng, kind, idx):
    tok = "R%s%s" % (idx, _word(rng))
    s = dict(kind=kind, idx=idx, tok=tok, eff="rp_%s.txt" % idx)
    if kind in ("cd", "remote_cd"):
        s["dir"] = rng.choice(["..", ".", "/tmp"] + (["rpd_%s" % idx] if kind == "cd" else []))       # `cd <dir> && echo ... >> <abs eff>` is built at run time (needs the work dir)
    if kind in ("tab", "remote_tab"):
        s["dir"] = "rptab%s_%s" % (idx, _word(rng, 5))         # created by the case; typed as the unique prefix "rptab<idx>_" + 2 chars, then Tab
    if kind in ("big_seq", "remote_big"):
        s["n"] = rng.choice([50000, 200000])
    if kind in ("ctrlc", "remote_ctrlc", "agent_ctrlc"):
        s["delay_ms"] = rng.randint(150, 1800)                 # Ctrl-C lands this long after the command starts
    if kind == "subshell":
        s["shell"] = rng.choice(["bash", "dash"])
    if kind == "ssh_session":
        n = rng.randint(2, 5)
        s["inner"] = [make_step(rng, _pick(rng, SSH_INNER), "%d_%d" % (idx, i)) for i in range(n)]
    if kind in ("agent_approve", "agent_reject"):
        s["ask"] = "Please run exactly this shell command and nothing else: echo %s >> %s" % (tok, s["eff"])       # the eff path is made absolute by the case
    elif kind == "agent_prose":
        s["ask"] = "Reply with the word %s" % tok
    elif kind == "agent_ctrlc":
        s["ask"] = "Answer slowly SLOW %s" % tok                # the fake model holds a SLOW turn for 3 s, so the Ctrl-C lands mid-stream
    return s


def gen_scenario(seed, minutes, mode):
    """deterministic: the same (seed, minutes, mode) gives the same steps. Fills `minutes` of estimated time (at least 3 steps). mode: terminal | agent"""
    rng = random.Random("replay:%s:%s" % (seed, mode))
    kinds = AGENT_KINDS if mode == "agent" else TERMINAL_KINDS
    budget, steps, used = float(minutes) * 60, [], 0.0
    while used < budget or len(steps) < 3:
        s = make_step(rng, _pick(rng, kinds), len(steps))
        steps.append(s)
        used += est_seconds(s)
        if len(steps) > 5000:
            break
    return steps


def flatten(steps):
    out = []
    for s in steps:
        out.append(s)
        out.extend(flatten(s.get("inner", [])))
    return out


# ------------------------------------------------------------------ oracles
def expected_effects(steps, ran=None):
    """{effect file: [token]} for every step that must have left its token exactly once. Rejected agent steps and Ctrl-C'd steps expect NOTHING (empty
    list). `ran` (a set of step idx) limits it to steps the case actually executed."""
    out = {}
    for s in flatten(steps):
        if ran is not None and s["idx"] not in ran:
            continue
        if s["kind"] in ("local_cmd", "remote_cmd", "cd", "remote_cd", "tab", "remote_tab", "subshell", "sudo", "agent_approve"):
            out.setdefault(s["eff"], []).append(s["tok"])
        elif s["kind"] == "agent_reject":
            out.setdefault(s["eff"], [])
    return out


def judge_effects(expected, got):
    """[problem strings]: a lost key shows as a missing/short line, a duplicate as an extra one, a rejected command as a non-empty file. `got`: {file: [lines]}"""
    bad = []
    for f, want in sorted(expected.items()):
        have = got.get(f, [])
        if have != want:
            bad.append("%s: expected %r, got %r" % (f, want, have))
    return bad


ERR_LINE_RE = re.compile(r"\[ERROR\]|\bERROR\b|panicked at")


def error_lines(log_text, allow=ERROR_ALLOW_DEFAULT, extra_allow=()):
    """the app-log lines at ERROR level (or a panic) that no allowlist pattern covers"""
    pats = [re.compile(p, re.I) for p in tuple(allow) + tuple(extra_allow)]
    return [l.strip()[:240] for l in log_text.splitlines() if ERR_LINE_RE.search(l) and not any(p.search(l) for p in pats)]


_LOG_START = r"^\d{4}-\d\d-\d\dT\S+ \[\w+\] (?:\S+ )?"        # a real app-log line's start (timestamp, [LEVEL], optional target): a diff body that echoes a log line never matches
ENTER_RE = re.compile(_LOG_START + r'hop=enter_begin\b[^\n]*?\bbuffer=("(?:[^"\\]|\\.)*"|<\d+ chars>) input_type=(\w+)', re.M)
VIEW_RE = re.compile(_LOG_START + r"hop=view_mode mode=(agent|terminal)\b", re.M)
ROUTE_RE = re.compile(_LOG_START + r"hop=ctrl_c_route route=(\w+)", re.M)


def agent_hops(log_text):
    """parsed hop= lines: {"enters": [(buffer, input_type)], "views": [mode], "ctrl_c_routes": [route], "streams": n, "cards": n}"""
    enters = []
    for b, t in ENTER_RE.findall(log_text):
        if b.startswith('"'):
            try:
                import json
                b = json.loads(b)
            except ValueError:
                pass
        enters.append((b, t))
    return dict(enters=enters, views=VIEW_RE.findall(log_text), ctrl_c_routes=ROUTE_RE.findall(log_text),
                streams=log_text.count("[byop] opening stream"), cards=log_text.count("queue_actions"))


def judge_agent(hops, steps_run):
    """the agent invariants over a whole agent pass. steps_run: the agent steps executed, in order. Returns [problem strings].
    A1 every prose/ask Enter was routed to the AI (input_type=AI), never the shell; A2 the view never left `agent` except through the case's own Escape;
    A3 one model request at least per asked step (never fewer: a swallowed Enter)."""
    bad = []
    asks = [s for s in steps_run if s.get("ask")]
    ai = [b for b, t in hops["enters"] if t == "AI"]
    for s in asks:
        if not any(isinstance(b, str) and s["tok"] in b for b in ai):
            bad.append("A1 step %s: no hop=enter_begin input_type=AI carrying %s" % (s["idx"], s["tok"]))
        for b, t in hops["enters"]:
            if t == "Shell" and isinstance(b, str) and s["ask"][:30] in b:
                bad.append("A1 step %s: the prose Enter was routed to the SHELL" % s["idx"])
    v = hops["views"]
    if "agent" in v and "terminal" in v[v.index("agent"):]:
        bad.append("A2 the view left agent mode during the replay (hop=view_mode sequence %s)" % v[:12])
    if hops["streams"] < len(asks):
        bad.append("A3 %d model requests for %d asked steps" % (hops["streams"], len(asks)))
    return bad


def percentile(xs, p):
    if not xs:
        return None
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(round(p / 100.0 * (len(xs) - 1))))]


def latency_stats(samples):
    """{kind: {n, p50, p95, max}} from [(kind, seconds)]. Recorded, never gating."""
    by = {}
    for k, v in samples:
        by.setdefault(k, []).append(v)
    return {k: dict(n=len(v), p50=round(percentile(v, 50), 3), p95=round(percentile(v, 95), 3), max=round(max(v), 3)) for k, v in sorted(by.items())}


def replay_minutes(env=None):
    import os
    v = (env if env is not None else os.environ).get("APPCI_REPLAY_MINUTES", "15")
    try:
        m = float(v)
    except ValueError:
        raise ValueError("APPCI_REPLAY_MINUTES=%r is not a number" % v)
    if m <= 0:
        raise ValueError("APPCI_REPLAY_MINUTES must be > 0")
    return m
