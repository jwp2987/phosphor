# appci/cases.py: the release-test matrix of docs/testing.md 3.5 as machine-checkable cases.
# Every case verifies the EXACT command / bytes the shell received (effect files written by the typed commands
# plus the shell's own rec.log from the scratch rc), never "something ran". Cases run once per session mode
# (terminal, agent) unless marked otherwise. A case function gets (ctx, r); r.check(ok, msg) records failures,
# r.metric(k, v) numbers, r.skip(msg) a skip. Typing is at the owner's stress rate: 10-25 ms/key, Enter in the same call.
import hashlib, json, os, random, re, shutil, statistics, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lat  # noqa: E402
import backend  # noqa: E402

CASES = []
# APPCI_TIER=smoke (CI tiers 'fix', docs/testing.md 'Test tiers'): the input-path cases a fix push needs, both modes, ~10 min per platform.
# ONE list for Linux, macOS and Windows (#1488): a case that cannot run on a platform / pane shell SKIPS there with a reason (winmap.py), never silently.
# The two ssh hand-over cases are in it for the ConPTY / hand-over release blocker (#1349).
SMOKE = ("mode_proof", "key_loss_control", "enter_loss_control", "cadence", "fast_typing", "paste_instant_enter", "tab_enter", "ctrl_c_d", "ssh_connect_typing",
         "ssh_enter_then_immediate_command", "ssh_nested_typeahead", "shell_fish", "shell_nu", "latency_enter", "latency_key_log", "latency_paint", "md_viewer",
         # owner 2026-11-02 ("still not doing agent?"): the agent pass must also send REAL prompts, not only shell lines typed into the agent view
         "routing_table", "agent_stream_cancel",
         # #1510: agent input proven byte-exact from the app log, and routing after each agent<->terminal switch
         "agent_input_exact", "agent_mode_switch")


def case(name, modes=("terminal", "agent"), gui=False, order=50):
    def deco(f):
        CASES.append(dict(name=name, modes=modes, gui=gui, order=order, fn=f))
        return f
    return deco


class Skip(Exception):
    pass


class Result:
    def __init__(self, name, mode):
        self.name, self.mode = name, mode
        self.fails, self.notes, self.metrics, self.shots, self.skipped = [], [], {}, [], None
        self.noref = []      # reasons this case could not be judged against a reference terminal: status becomes "noref" (NOT a pass) unless a check failed

    def check(self, ok, msg):
        if not ok:
            self.fails.append(msg)
        return ok

    def note(self, msg):
        self.notes.append(msg)

    def no_reference(self, msg):
        self.noref.append(msg)
        self.notes.append("NO-REFERENCE: " + msg)

    def unverifiable(self, msg):
        """the evidence the case claims to rest on is missing (e.g. the app's key trace): status 'noref', never a pass"""
        self.no_reference("UNVERIFIABLE: " + msg)

    def metric(self, k, v):
        self.metrics[k] = round(v, 3) if isinstance(v, float) else v

    def skip(self, msg):
        raise Skip(msg)


def dest_mode(env=None):
    """The destination mode the app under test runs in (#1532): PHOSPHOR_QUEUE_DESTINATION as the app parses it
    (input_queue/facts.rs DestinationMode::parse): unset / empty = table (the default), shadow, table, anything else = legacy (the kill switch).
    The app inherits this process's environment (Linux, Windows) or gets the variable passed through (macOS, app.passthrough_env), so this is
    the mode it runs in; the app itself logs no startup line for it."""
    v = (os.environ if env is None else env).get("PHOSPHOR_QUEUE_DESTINATION")
    v = "" if v is None else v.strip()
    if not v:
        return "table"
    return v if v in ("shadow", "table") else "legacy"


def dest_disagreements(text):
    """(all, untagged) counts of `kh_dest_disagree` lines in an app log (#1528). Shadow mode logs one per difference between the legacy chain and
    the table; a line carrying `expected=<tag>` is a known cause (OD1 OD5 OD7 B3 B7 B13 B14), an untagged one is a regression."""
    ls = [l for l in text.splitlines() if "kh_dest_disagree" in l]
    return len(ls), len([l for l in ls if " expected=" not in l])


def pct(xs, p):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(len(xs) * p))] if xs else None


def stats(xs):
    return {"n": len(xs), "p50": round(statistics.median(xs), 2) if xs else None, "p95": round(pct(xs, 0.95), 2) if xs else None,
            "max": round(max(xs), 2) if xs else None}


def plat():
    return os.environ.get("APPCI_PLATFORM", "linux")


def lat_checks(r, rec, label):
    """turn a latency record's verdicts into case failures (FAIL); WARN stays a note; no-reference makes the case status 'noref' (not a pass, #1499)"""
    if rec["budget_verdict"] == "fail":
        if rec["budget_kind"] == "overhead":
            r.check(False, "%s: Phosphor overhead %.2f +/- %s ms (SE) over %s exceeds the %.0f ms budget (overhead - 2 SE > budget)" % (label, rec["overhead_ms"], rec.get("overhead_se_ms"), rec["reference_name"], rec["budget_ms"]))
        else:
            r.check(False, "%s: %.2f ms over the harness floor exceeds the %.0f ms budget" % (label, rec["overhead_ms"], rec["budget_ms"]))
    for tag in ("prev", "baseline"):
        x = rec.get(tag) or {}
        if x.get("verdict") in ("fail", "warn") and x.get("delta_ms") is not None:
            msg = "%s: median %s vs %s build %+.2f ms (%+.1f%%)" % (label, rec["phosphor"]["p50"], tag, x["delta_ms"], x["delta_pct"])
            if rec.get("remeasure"):
                msg += "; re-measured once: median %s (%s)" % (rec["remeasure"]["phosphor"]["p50"], rec["remeasure"][tag]["verdict"])
            if x["verdict"] == "fail":
                r.check(False, "REGRESSION " + msg)
            else:
                r.note("WARN " + msg)
    if rec["verdict"] in ("inconclusive", "no-compare") or rec.get("budget_verdict") in ("inconclusive", "no-compare"):  # (#1514: includes too-few-samples and resolution-limited)
        why = "overhead %s +/- %s ms (SE) vs budget %g ms is within noise" % (rec.get("overhead_ms"), rec.get("overhead_se_ms"), rec["budget_ms"])
        if rec.get("implausible"):
            why = "the reference measured implausibly slow (overhead %s ms)" % rec.get("overhead_ms")
        elif rec.get("budget_verdict") == "no-compare":
            why = "pane shell %s != reference shell %s" % (rec.get("pane_shell"), rec.get("reference_shell"))
        elif rec.get("insufficient"):
            why = "insufficient evidence (%s)" % "; ".join(rec["insufficient"])
        r.no_reference("%s: %s: INCONCLUSIVE/NO-COMPARE, the overhead budget was NOT judged (not a pass)" % (label, why))
    if rec["verdict"] == "no-reference":
        r.no_reference("%s: no reference terminal measured in this session, the overhead budget was NOT judged (absolute numbers recorded as metrics)" % label)
    for n_ in rec.get("notes", []):
        r.note("%s: %s" % (label, n_))


_SSH_HB_RE = re.compile(r"hop=(?:young_hold_handed_back|prompt_hold_handed_back)\b.*?\bwhy=(\w+)")
_SSH_HOLD_RE = re.compile(r"hop=kh_iq_hold\b.*queue=Q7_ssh_typeahead bytes=(\d+)")
_SSH_HANDOVER_RE = re.compile(r"hop=kh_handover\b.*?source=(\w+).*?held_bytes=(\d+)")
_SSH_FINISH_RE = re.compile(r"hop=kh_finish_begin\b.*?\bheld_bytes=(\d+)")
_SSH_HANDED_BACK_RE = re.compile(r"hop=ssh_handed_back\b.*?chars=(\d+) dest=(\w+)")


def ssh_trial_facts(lines, expected, remote_new, enters_before=1):
    """Pure: one ssh typeahead trial from its app-log window `lines` (raw, oldest first), the line typed during the connect (`expected`) and
    the commands the remote shell logged in the trial (`remote_new`). `enters_before`: Enters typed before that line (`ssh fake` = 1; a nested
    `ssh fake` twice = 2). Names the hop where the line went wrong, by issue:
    #1614 the ssh Enter itself handed back (ssh never ran: the hold was not exercised); #1292 keys not held / line lost whole;
    #1332 the held Enter refused (HistoryNotAppendable); #1345/#1394 one key lost at the hand-over (index vs held count);
    #1400 a run of keys lost between the hold window and the hand-over; #1596 the held text kept, invisibly, until the ssh session ends."""
    import difflib
    f = dict(exact=False, typed="", app_saw_line=None, enter_handed_back=None, hold_seen=False, held_keys=0, held_bytes=0, deferred_busy=0,
             handovers=[], finish=[], give_ups=0, refused=0, discards=0, table_drops=0, handed_back=[], remote=None, missing=[], extra=[])
    enters, typed, core_pending, after_handover = 0, [], False, False
    for l in lines:
        # a physical Enter: the window's dispatch (`core_enter stage=handle_window_event`, every platform) and, when no binding took it, the
        # keydown that follows (macOS/Linux); Windows' editor binding takes it, so only the first is logged. Counted once either way.
        if "hop=core_enter stage=handle_window_event" in l:
            enters += 1
            core_pending = True
            continue
        if "input_order keydown key=enter" in l:
            if core_pending:
                core_pending = False
            else:
                enters += 1
            continue
        m = re.search(r'input_order typed_characters text="((?:[^"\\]|\\.)*)"', l)
        if m:
            if enters == enters_before:
                typed.append(m.group(1).replace('\\"', '"').replace("\\\\", "\\"))
            continue
        m = _SSH_HB_RE.search(l)
        if m and f["enter_handed_back"] is None:
            f["enter_handed_back"] = m.group(1)
            f["handed_back_enter_no"] = enters
            f["handed_back_after_hold"] = f["hold_seen"]
        if ("kh_holding_set" in l and "holding=true" in l) or "gate_open kind=ssh_connect" in l:
            f["hold_seen"] = True
        m = _SSH_HOLD_RE.search(l)
        if m:
            f["hold_seen"] = True
            f["held_keys"] += 1
            f["held_bytes"] += int(m.group(1))
        if "hop=kh_ssh_hold_deferred" in l:
            f["deferred_busy"] += 1
        m = _SSH_HANDOVER_RE.search(l)
        if m:
            f["handovers"].append((m.group(1), int(m.group(2))))
            after_handover = True
        m = _SSH_FINISH_RE.search(l)
        if m:
            # `finish_ssh_typeahead` is also entered without `kh_handover` (another event ended the connect): its held count is the hand-over's
            if not after_handover and "give_up=" not in l:
                f["handovers"].append(("finish", int(m.group(1))))
            after_handover = False
        m = re.search(r"hop=kh_finish_(?:begin|end)\b.*?\b((?:delivered|give_up)=\w+)", l)
        if m:
            f["finish"].append(m.group(1))
        if "hop=kh_ssh_give_up" in l:
            f["give_ups"] += 1
        if "can_execute_command was false" in l:
            f["refused"] += 1
        if "hop=kh_iq_discard" in l and "Q7_ssh_typeahead" in l:
            f["discards"] += 1
        if "hop=kh_table_drop" in l:
            f["table_drops"] += 1
        m = _SSH_HANDED_BACK_RE.search(l)
        if m:
            f["handed_back"].append((int(m.group(1)), m.group(2)))
        elif "handed-back char(s) kept until the running block ends" in l:
            f["handed_back"].append((None, "stashed_until_block_end"))
        m = re.search(r"hop=ssh_kept_text action=(\w+)", l)
        if m:
            f.setdefault("kept_banner", []).append(m.group(1))
    f["typed"] = "".join(typed)
    if typed:
        f["app_saw_line"] = expected in f["typed"]
    got = [c for c in remote_new if c.strip()]
    if expected in got:
        f["exact"] = True
        f["remote"] = expected
    elif got:
        best = max(got, key=lambda c: difflib.SequenceMatcher(None, expected, c, autojunk=False).ratio())
        f["remote"] = best
        for op, i1, i2, j1, j2 in difflib.SequenceMatcher(None, expected, best, autojunk=False).get_opcodes():
            if op in ("delete", "replace"):
                f["missing"].append((i1, expected[i1:i2]))
            if op in ("insert", "replace"):
                f["extra"].append((j1, best[j1:j2]))
    hold = "held %d key(s)/%d byte(s)%s, hand-over %s, finish %s" % (
        f["held_keys"], f["held_bytes"], " (+%d deferred: queue busy)" % f["deferred_busy"] if f["deferred_busy"] else "",
        ",".join("%s@%d" % h for h in f["handovers"]) or "none", ",".join(f["finish"]) or "none")
    if f["app_saw_line"] is None:
        saw = "no typed_characters trace"
    elif f["app_saw_line"]:
        saw = "the app saw every key of the line"
    else:
        saw = "the app's typed keys %r lack the line (injector/OS loss, or a key dropped before typed_characters)" % f["typed"][:60]
    if f["exact"]:
        why = "exact"
    elif f["enter_handed_back"]:
        if f.get("handed_back_enter_no", 0) <= enters_before and not f.get("handed_back_after_hold"):
            why = ("#1614: the `ssh` Enter was HANDED BACK to the editor (why=%s) before any ssh hold, so ssh never ran and the ssh typeahead "
                   "hold was NOT exercised" % f["enter_handed_back"])
        else:
            why = ("#1609/#1614: an Enter typed after the ssh hold engaged was HANDED BACK to the editor by the local young hold (why=%s): the "
                   "text reached the remote, its Enter did not" % f["enter_handed_back"])
    elif f["refused"]:
        why = "#1332: the delivered Enter was REFUSED (can_execute_command false / HistoryNotAppendable)"
    elif any(d == "stashed_until_block_end" for _, d in f["handed_back"]):
        kb = f.get("kept_banner") or []
        why = ("#1596: the held text was handed back and KEPT until the ssh block ends (%s)"
               % ("kept-text banner: %s" % ",".join(kb) if kb else "NO kept-text banner shown"))
    elif f["handed_back"]:
        why = "held text handed back to the editor as text (%s), not run remotely" % f["handed_back"]
    elif f["remote"] is None and not f["hold_seen"]:
        why = "#1292: no ssh hold was seen and the remote shell logged nothing for the line"
    elif f["remote"] is None:
        why = "#1292: the line never reached the remote shell (discards=%d table_drops=%d give_ups=%d)" % (f["discards"], f["table_drops"], f["give_ups"])
    elif len(f["missing"]) == 1 and len(f["missing"][0][1]) == 1 and not f["extra"]:
        i, ch = f["missing"][0]
        boundary = [hb for _, hb in f["handovers"] if hb in (i, i - 1)]
        why = "#1345/#1394: ONE key lost: char %d (%r) of %d%s" % (
            i + 1, ch, len(expected), "; it is the key right after the %d held at the hand-over (the in-flight key)" % boundary[0] if boundary else "")
    elif f["missing"] and not f["extra"]:
        why = "#1400: keys lost: %s of %d" % (", ".join("chars %d-%d %r" % (i + 1, i + len(s), s) for i, s in f["missing"]), len(expected))
    else:
        why = "mangled line at the remote: %r (missing %r, extra %r)" % (f["remote"], f["missing"], f["extra"])
    f["text"] = "%s; %s; %s; remote got %r" % (why, hold, saw, f["remote"])
    return f


def enter_segments(lines, t0):
    """app log lines -> the split of Enter->command-start the APP can see, medians in ms (#1583 W3, #1606):
      event_age_ms             `hop=mac_enter stage=app_send_event ... age_ms=` (macOS, #1606): the OS's own timestamp of the Return key-down ->
                               the app dequeuing it on the main thread. Large = the main thread was busy (rendering) when the key arrived.
      event_to_enter_ms        that app_send_event line -> `hop=enter_begin` (AppKit routing + keystroke matching on the main thread)
      enter_to_write_ms        `hop=enter_begin` -> the `pty_write` of the line (> 10 bytes; handoff writes excluded)
      submit_bookkeeping_ms    `hop=try_execute_done` -> `hop=enter_submit_done` (#1606): post-submit model updates that run before the queued
                               ExecuteCommand effect reaches the pty (agent mode measured ~0.7 ms here in r18e4)
      effect_flush_ms          `hop=enter_submit_done` -> `hop=write_command`
      write_to_first_read_ms   that write -> the first `pty_read` after it (the shell's first answer: echo/Preexec OSC); the app logs this as
                               `since_write_ms` on the pty_read line
      write_to_preexec_ms      from the app's `hop=preexec_received` line (input_order::note_shell_signal(Preexec), #1583 W3). Builds before
                               that line existed only stamped an atomic (since_preexec_ms on the NEXT write), so there the key is absent and
                               the report says so. This is when the APP parsed the hook, after any wait for the model lock (below), not
                               when the shell sent it: the shell-side times come from the stamp file (enter_split_summary).
      pty_lock_wait_ms         per Enter, the summed `hop=pty_lock_wait waited_ms=` between the write and the preexec parse (#1606): output
                               answering the Enter waited for the model lock the UI held while building a frame (0 when none was logged,
                               absent when the build has none of the #1606 hops)
    Lines before wall time t0 are ignored."""
    keys = ("event_age_ms", "event_to_enter_ms", "enter_to_write_ms", "submit_bookkeeping_ms", "effect_flush_ms", "write_to_first_read_ms",
            "write_to_preexec_ms", "pty_lock_wait_ms")
    seg = {k: [] for k in keys}
    pending, wrote, read_seen, chord = None, None, True, False
    sent, executed, submitted, lock_wait = None, None, None, None
    # pty_lock_wait is logged only when a wait happened, so its absence means 0 only in a build that has the #1606 hops (the
    # enter_submit_done hop is logged on every shell submit); an older build reports the key absent, never 0.0
    has_lock_hop = any("hop=enter_submit_done" in l or "hop=pty_lock_wait" in l for l in lines)
    for l in lines:
        m = re.search(r" t=(\d+(?:\.\d+)?)", l)
        if not m:
            continue
        t = float(m.group(1))
        if t / 1000.0 < t0:
            continue
        if "stage=app_send_event" in l:
            sent = t
            a = re.search(r" age_ms=(-?\d+(?:\.\d+)?)", l)
            if a and 0 <= float(a.group(1)) < 10000:
                seg["event_age_ms"].append(float(a.group(1)))
        elif "hop=enter_begin" in l:
            pending, chord = t, False
            if sent is not None:
                seg["event_to_enter_ms"].append(t - sent)
            sent, executed, submitted = None, None, None
        elif "hop=pwsh_command_via_pipe" in l and pending is not None:
            # #1581: a PowerShell line goes on the session's pipe and only the 2-byte insert chord is written, so the "> 10 bytes" rule below
            # never matched and r18e4 had no app segment metric for the PowerShell pane at all (#1401)
            chord = True
        elif "hop=try_execute_done" in l and pending is not None:
            executed = t
        elif "hop=enter_submit_done" in l and executed is not None:
            seg["submit_bookkeeping_ms"].append(t - executed)
            submitted, executed = t, None
        elif "hop=write_command" in l and submitted is not None:
            seg["effect_flush_ms"].append(t - submitted)
            submitted = None
        elif "hop=preexec_received" in l and wrote is not None:
            seg["write_to_preexec_ms"].append(t - wrote)
            if has_lock_hop:
                seg["pty_lock_wait_ms"].append(lock_wait or 0.0)
            wrote, lock_wait = None, None
        elif "hop=pty_lock_wait" in l and wrote is not None:
            w = re.search(r"waited_ms=(\d+(?:\.\d+)?)", l)
            if w:
                lock_wait = (lock_wait or 0.0) + float(w.group(1))
        elif "pty_write bytes=" in l and "handoff" not in l:
            nb = re.search(r"pty_write bytes=(\d+)", l)
            if nb and (int(nb.group(1)) > 10 or chord) and pending is not None:
                seg["enter_to_write_ms"].append(t - pending)
                wrote, pending, read_seen, lock_wait, chord = t, None, False, None, False
        elif wrote is not None and not read_seen and " pty_read bytes=" in l:
            sw = re.search(r"since_write_ms=(\d+(?:\.\d+)?)", l)
            seg["write_to_first_read_ms"].append(float(sw.group(1)) if sw else t - wrote)
            read_seen = True          # only the FIRST read after a write counts; `wrote` stays for a later preexec hop
    out = {}
    for k in keys:
        v = seg[k]
        if v:
            out[k] = round(statistics.median(v), 2)
            out[k.replace("_ms", "_n")] = len(v)
    return out


def os_key_ages(lines, t0, t1=None, enter=None, ctrl=None):
    """#1606: `input_order hop=os_key_event age_ms=X enter=.. ctrl=..` lines (macOS, every key-down) in [t0, t1] wall seconds -> the ages in
    ms (how long each key queued between the OS stamping it and the app's main thread dequeuing it). enter/ctrl filter on the flags when set."""
    out = []
    for l in lines:
        if "hop=os_key_event" not in l:
            continue
        m = re.search(r" age_ms=(-?\d+(?:\.\d+)?) enter=(true|false) ctrl=(true|false) t=(\d+(?:\.\d+)?)", l)
        if not m:
            continue
        t = float(m.group(4)) / 1000.0
        if t < t0 or (t1 is not None and t > t1):
            continue
        if enter is not None and (m.group(2) == "true") != enter:
            continue
        if ctrl is not None and (m.group(3) == "true") != ctrl:
            continue
        a = float(m.group(1))
        if 0 <= a < 10000:              # a clock mismatch gives nonsense; never let it into a median
            out.append(a)
    return out


def ctrl_c_app_split(lines, t0):
    """#1606: the app-side split of the Ctrl-C presses since wall time t0, from the app log: ctrl_event_age (os_key_event ctrl=true: how long the
    key queued before the main thread dequeued it), key_to_write (`hop=ctrl_c_keydown` -> `hop=ctrl_c_pty_write`), and the nextDrawable waits
    (`hop=drawable_wait`) logged in the same window: a main thread blocked in nextDrawable is the suspect when the event age is large."""
    key_to_write, waits, k = [], [], None
    for l in lines:
        m = re.search(r" t=(\d+(?:\.\d+)?)", l)
        if not m or float(m.group(1)) / 1000.0 < t0:
            continue
        t = float(m.group(1))
        if "hop=ctrl_c_keydown" in l:
            k = t
        elif "hop=ctrl_c_pty_write" in l and k is not None:
            key_to_write.append(t - k)
            k = None
        elif "hop=drawable_wait" in l:
            w = re.search(r"wait_ms=(\d+(?:\.\d+)?)", l)
            if w:
                waits.append(float(w.group(1)))
    out = {}
    for name, v in (("ctrl_event_age_ms", os_key_ages(lines, t0, ctrl=True)), ("key_to_write_ms", key_to_write), ("drawable_wait_ms", waits)):
        st_ = age_stats(v)
        if st_:
            out[name] = st_
    return out


def age_stats(ages):
    """os_key_ages -> {n, p50, p95, max} (ms) or None"""
    if not ages:
        return None
    xs = sorted(ages)
    return dict(n=len(xs), p50=round(statistics.median(xs), 2), p95=round(xs[min(len(xs) - 1, int(len(xs) * 0.95))], 2), max=round(xs[-1], 2))


class Ctx:
    def __init__(self, be, prof, app, run, mode, seed=1, ssh=None, ollama=False, log_path=None, ssh_why=None, pane=None, variant=None):
        self.be, self.prof, self.app, self.run, self.mode = be, prof, app, run, mode
        self.pane = pane                      # Windows only: the shell of the app's pane, 'powershell' | 'gitbash' | 'cmd' (None = the POSIX bash/zsh harness)
        self.variant = variant                # #1534: 'pwsh7' | 'cmd' in a startup_shell_override pass (pane is then its family: pwsh 7 is 'powershell', cmd.exe 'cmd'), else None
        self.win = pane is not None
        # label = the mode column of results.json / junit and the id prefix of latency records: 'terminal' everywhere except a non-default
        # Windows pane ('terminal@gitbash'), so the two panes' numbers and previous-build comparisons never mix
        self.label = mode if (pane in (None, "powershell") and not variant) else "%s@%s" % (mode, variant or pane)
        self.t_case0 = time.time()
        self.rng = random.Random(seed)
        self.ssh, self.ollama, self.log_path = ssh, ollama, log_path
        self.ssh_why = ssh_why     # why there is no usable ssh test host (sshd missing, or the setup probe `ssh fake true` failed); None when ctx.ssh is set
        self.W = prof.home + "/w"
        self.reprove = None           # Windows (run.py): callable() -> None | reason; re-proves key delivery + the PowerShell pane state after a case relaunched the app
        self.opts = {}                # tier options of this pass (winmap.tier_plan): ref, ft_gaps, cc_short; empty = the full tier
        self.enter_control = None     # cached result of run_enter_control (#1482)
        self.cc_integrity = {}        # ctrlc_samples: per output file, how many typed sample lines the shell received exactly
        self.ctrlc_ts = []            # wall time of every Ctrl-C the harness sent (r18d2: see reset())
        self.left_unproven = None     # leave(): where the probe ran when the pane was NOT provably remote (no `exit` typed)
        self.extra_logs = []          # earlier app launches' log copies made by this case (snapshot_log), cleared per case by run.py
        self.typed_log = []           # (time, text) of every be.type() (r18e9: tells "the case's own `exit` closed the pane" from an app death)
        self._wrap_ctrlc(be)
        self._wrap_type(be)
        try:
            be.inject_log = os.path.join(run, "inject.log")
            os.makedirs(os.path.join(run, "logs"), exist_ok=True)
            backend.STDERR_LOG = os.path.join(run, "logs", "injector-stderr.log")      # r18d8: wtype/xdotool stderr is kept, not /dev/null
        except Exception:
            pass
        self.n = 0
        self.scale = float(os.environ.get("APPCI_SCALE", "1"))   # <1 shortens counts for smoke runs
        # the app's own pane has no rc hook (Windows PowerShell): rec.log is empty, prompt.log is empty. Cases already treat an empty rec.log as
        # "effect file only"; places that WAIT for a local prompt use a fixed settle instead (leave_remote)

    def count(self, n):
        return max(2, int(n * self.scale))

    def count_lat(self, n):
        """sample count for a LATENCY case (#1514): never below lat.min_n(), however small the scale (a scaled run judged on 2 samples proved nothing)"""
        return max(lat.min_n(), int(n * self.scale))

    def tok(self, n=8):
        self.n += 1
        return "T%d_%s" % (self.n, "".join(self.rng.choice("abcdefghijklmnopqrstuvwxyz0123456789") for _ in range(n)))

    def until(self, pred, timeout=10, step=0.05):
        t = time.time()
        while time.time() - t < timeout:
            v = pred()
            if v:
                return v
            time.sleep(step)
        return pred()

    def lines(self, name, remote=False):
        base = self.prof.rhome if remote else self.W
        for attempt in range(5):
            try:
                return open(os.path.join(base, name), errors="replace").read().splitlines()
            except FileNotFoundError:
                return []
            except PermissionError:       # Windows: the shell still has the file open for writing (r18cm crashed here); read again shortly
                time.sleep(0.05 * (attempt + 1))
        return []

    def rm(self, *names):
        for n in names:
            try:
                os.remove(os.path.join(self.W, n))
            except FileNotFoundError:
                pass

    def append_cmd(self, shell, tok, fname):
        if shell == "nu":
            return '"%s\\n" | save --append %s' % (tok, fname)     # nu's save --append adds no newline: give each line one
        if shell == "pwsh":      # PowerShell: `>>` would write UTF-16 in 5.1; Add-Content writes ASCII lines
            return "Add-Content %s %s" % (fname, tok)
        if shell == "cmd":       # cmd.exe: no space before `>>` (it would end up in the line); #1534
            return "echo %s>>%s" % (tok, fname)
        return "echo %s >> %s" % (tok, fname)

    def pane_shell(self):
        """the shell name append_cmd() takes for the app's own pane: 'pwsh' in a Windows PowerShell pane (and pwsh 7), 'cmd' in a cmd.exe pane, else 'bash'"""
        return "pwsh" if self.pane == "powershell" else ("cmd" if self.pane == "cmd" else "bash")

    def which(self, name):
        """shutil.which, on Windows against the PATH the app gets (Git for Windows' bin + usr/bin appended; never WSL's System32 bash.exe)"""
        if not self.win:
            return shutil.which(name)
        import winapi
        return shutil.which(name, path=os.pathsep.join(winapi.git_dirs()))

    def local_lines(self, name):
        """lines of a file the LOCAL shell would have written for `echo X >> ~/name` / `>> name`: the case dir, and in a PowerShell pane also
        the user's real home (PowerShell resolves ~ to USERPROFILE, not the scratch HOME); a home file counts only if created after this case began"""
        out = self.lines(name)
        if self.pane == "powershell":
            hp = os.path.join(os.path.expanduser("~"), name)
            try:
                if os.path.getmtime(hp) >= self.t_case0 - 1:
                    out += open(hp, errors="replace").read().splitlines()
            except OSError:
                pass
        return out

    def prompts(self):
        return self.prof.prompts()

    SHELLS = ("bash", "zsh", "fish", "nu", "sh", "dash")

    def shell_count(self):
        """number of shells (bash/zsh/fish/nu/sh) among the descendants of the Phosphor process, from `ps` (no pgrep/pkill, no name matching of
        the app); None when the app PID or ps is unavailable. The app logs nothing when the local shell exits, so the process tree is the evidence."""
        if self.win:
            return None        # no `ps`; a Windows process count would be fooled by the app's own short-lived -NoProfile PowerShell helpers
        try:
            root = self.app.pid() if self.app else None
            if not root:
                return None
            out = subprocess.run(["ps", "-A", "-o", "pid=,ppid=,comm="], capture_output=True, text=True, timeout=10).stdout
        except Exception:
            return None
        kids, comm = {}, {}
        for l in out.splitlines():
            f = l.split(None, 2)
            if len(f) == 3 and f[0].isdigit() and f[1].isdigit():
                kids.setdefault(int(f[1]), []).append(int(f[0]))
                comm[int(f[0])] = os.path.basename(f[2].strip()).lstrip("-")
        n, todo, seen = 0, [int(root)], set()
        while todo:
            x = todo.pop()
            if x in seen:
                continue
            seen.add(x)
            todo += kids.get(x, [])
            if x != int(root) and comm.get(x) in self.SHELLS:
                n += 1
        return n

    def shell_gone(self, base):
        """True when fewer shells run under the app than at `base` (a shell_count() taken at case start): the LOCAL shell exited"""
        now = self.shell_count()
        return base is not None and now is not None and now < base

    def where_now(self, timeout=8):
        """Which shell is in front, proven by an effect: a fresh token written to ~/.appci_where lands in the REMOTE home ("remote"), the local
        scratch home ("local"), or nowhere ("none")."""
        probe = "w" + self.tok(8)
        self.be.type("echo %s >> ~/.appci_where" % probe, 12, enter=True)
        local_where = self.local_where
        self.until(lambda: probe in self.lines(".appci_where", remote=True) or probe in local_where(), timeout)
        if probe in self.lines(".appci_where", remote=True):
            return "remote"
        return "local" if probe in local_where() else "none"

    def local_where(self):
        """tokens of the LOCAL shell's `~/.appci_where`: the scratch home, and in a Windows PowerShell pane also the user's real home (PowerShell
        resolves `~` to USERPROFILE) in UTF-16 (`>>` in Windows PowerShell 5.1) -- r18cz: the PowerShell pane was never provably local because only
        the scratch home was read"""
        out = []
        homes = [self.prof.home]
        if self.pane == "powershell":
            homes.append(os.path.expanduser("~"))
        for h in homes:
            try:
                raw = open(os.path.join(h, ".appci_where"), "rb").read()
            except OSError:
                continue
            out += raw.replace(b"\x00", b"").replace(b"\xff\xfe", b"").decode("utf-8", "replace").split()
        return out

    def return_to_local(self, max_levels=3):
        """After an ssh case, pass or fail: leave every remote level until a probe lands in the LOCAL home. `exit` is typed only while the
        probe proves a remote shell is in front (never at the local shell). Returns "local", "remote" (still remote after max_levels) or
        "none" (no shell answered, even after Ctrl-C/Ctrl-U)."""
        for _ in range(max_levels + 1):
            where = self.where_now()
            if where == "none":
                self.reset()
                where = self.where_now()
            if where != "remote":
                return where
            before = self.prof.prompts()
            self.be.type("exit", 12, enter=True)
            self.until(lambda: self.prof.prompts() > before, 10)
            time.sleep(0.5)
        return "remote"

    def leave_remote(self, r, label, rfile, tok, base, timeout=10, level=False):
        """Type `exit` ONLY if a remote shell is provably in front (tok reached the REMOTE file rfile). Otherwise `exit` would end the
        LOCAL shell and every later key would go to the agent (#1481): record the miss, reset the pane, and return False so the caller stops
        its trials. level=True leaves one level of a nested ssh (waits for a REMOTE prompt). Fails fast when the shell count under the app
        dropped below `base`. Returns True after a clean exit."""
        if tok not in self.lines(rfile, remote=True):
            r.check(False, "%s: %s never reached the REMOTE file (ssh did not give a remote shell in front), `exit` NOT typed; stopping (sshd.log, phosphor.log). Test host: %s" % (label, tok, self.ssh_diag()))
            self.reset()
            return False
        # #1481 follow-up (r18ck): `tok` proves a remote shell was in front at SOME point, not that one still is (a nested ssh that failed
        # left one level, and the second `exit` closed the last LOCAL pane, quitting the app). Prove it again, now, before EVERY exit:
        # a fresh token must land in the REMOTE home, never the local one.
        probe = "w" + self.tok(8)
        self.be.type("echo %s >> ~/.appci_where" % probe, 12, enter=True)
        local_where = self.local_where      # `~` in the LOCAL shell is the scratch home (PowerShell: the real home), not the work dir that lines() reads
        self.until(lambda: probe in self.lines(".appci_where", remote=True) or probe in local_where(), timeout)
        if probe not in self.lines(".appci_where", remote=True):
            where = "the LOCAL shell" if probe in local_where() else "no shell"
            r.check(False, "%s: before `exit`, a fresh probe ran in %s, not the remote one: `exit` NOT typed; stopping" % (label, where))
            self.reset()
            return False
        count = (lambda: self.prof.prompts(remote=True)) if level else self.prompts
        before = count()
        self.be.type("exit", 12, enter=True)
        if not level and not self.prof.local_prompts:
            time.sleep(min(timeout, 2.0))        # no local prompt hook in this pane (Windows PowerShell): fixed settle
        else:
            self.until(lambda: count() > before, timeout)
        if self.shell_gone(base):
            r.check(False, "%s: the LOCAL shell exited after `exit` (fewer shells under the app than at case start); stopping" % label)
            return False
        time.sleep(0.5)
        return True

    def pane_responsive(self, first=8, second=40):
        """The pane's liveness by an EFFECT: type a marker command that appends to a file and look for the file. Returns 0 when the first try worked, 1 when
        only the retry did (the pane needed Ctrl-C x3 to take typing again: r18co, Git Bash pane after cadence), None when no marker ever appeared (wedged).
        A wedged pane (a command that ignores Ctrl-C) fails every later case for a reason that is not theirs, so run.py checks after a failing case and
        relaunches the app. NOT judged by a prompt: the PowerShell pane has no prompt hook (the app starts it with -NoProfile), r18co's 'no prompt came back'
        wording was wrong, and the probe's file went to the real home because the pane was not in the case dir (local_lines also looks there)."""
        for tries, wait in ((0, first), (1, second)):
            tok = self.tok(8).lower()
            fname = "alive_%s.txt" % tok
            mark = self.cmd_mark()
            try:
                if tries:
                    for _ in range(3):
                        self.be.key("ctrl+c")
                        time.sleep(0.8)
                else:
                    self.be.key("ctrl+c")
                    time.sleep(0.3)
                self.be.key("ctrl+u")
                self.be.type(self.append_cmd(self.pane_shell(), tok, fname), 12, enter=True)
            except Exception:
                return None
            if self.until(lambda: tok in self.local_lines(fname), wait):
                # the shell's own rec.log (Git Bash pane) logs the command AFTER it ran: wait for that line, or it lands in the NEXT case's cmds_since(mark)
                # (r18co: paste_instant_enter 'first mismatch' was this probe's `echo t147... >> alive_...`)
                if self.prof.commands():
                    self.until(lambda: any(tok in c for _, c in self.prof.commands()[mark:]), 5)
                return tries
        return None

    def app_log_chars(self, t0):
        """the characters the APP logged as typed since wall time t0, in order (`input_order typed_characters text="x"`; needs
        WARP_INPUT_ORDER_LOG_TEXT=1, which the Windows harness sets), from the active log plus its rotated chunks. None when there is no such trace."""
        import glob
        src = self.log_path
        if not src:
            return None
        parts = sorted(glob.glob(glob.escape(src) + ".in_session.*"), key=lambda f: int(f.rsplit(".", 1)[1]) if f.rsplit(".", 1)[1].isdigit() else -1, reverse=True) + [src]
        out, seen = [], False
        for f in parts:
            try:
                txt = open(f, errors="replace").read()
            except OSError:
                continue
            for m in re.finditer(r'input_order typed_characters text="((?:[^"\\]|\\.)*)" t=(\d+(?:\.\d+)?)', txt):
                seen = True
                if float(m.group(2)) / 1000.0 >= t0 - 1:
                    out.append(m.group(1).replace('\\"', '"').replace("\\\\", "\\"))
        return "".join(out) if seen else None

    def app_enter_to_write_ms(self, t0):
        """median ms from the app's `enter_begin` hop to the `pty_write` that carries the line (>10 bytes) since wall time t0: the APP's own share
        of Enter->command-start, read from its input_order log (None without a trace). The rest is the shell/ConPTY side."""
        return self.app_enter_segments(t0).get("enter_to_write_ms")

    def app_enter_segments(self, t0):
        """the app-log split of one Enter (#1583 W3) since wall time t0: {enter_to_write_ms, write_to_first_read_ms, write_to_preexec_ms?}, medians
        (+ n). Empty without a trace. See enter_segments for the rules."""
        return enter_segments(self.app_log_lines(), t0)

    def app_log_lines(self):
        """every line of the app's log for this launch, including the in-session rotations (oldest first); [] without a log"""
        import glob
        src = self.log_path
        if not src:
            return []
        parts = sorted(glob.glob(glob.escape(src) + ".in_session.*"), key=lambda f: int(f.rsplit(".", 1)[1]) if f.rsplit(".", 1)[1].isdigit() else -1, reverse=True) + [src]
        lines = []
        for f in parts:
            try:
                lines += open(f, errors="replace").read().splitlines()
            except OSError:
                continue
        return lines

    def app_key_ages(self, t0, t1=None, enter=None, ctrl=None):
        """#1606: os_key_ages over the app log since wall time t0 (macOS builds with the os_key_event hop; [] elsewhere)"""
        return os_key_ages(self.app_log_lines(), t0, t1, enter=enter, ctrl=ctrl)

    def app_pty_text(self, t0):
        """#1490/#1393: the bytes the APP wrote to its pty since wall time t0, in write order (`input_order hop=pty_write_bytes ... bytes="..."`, Rust
        `escape_ascii` form; needs WARP_INPUT_ORDER_LOG_TEXT=1), decoded, from the active log plus its rotated chunks. None when there is no such trace
        (a log without opted-in text shows `<N bytes>` and is no trace)."""
        import glob
        src = self.log_path
        if not src:
            return None
        parts = sorted(glob.glob(glob.escape(src) + ".in_session.*"), key=lambda f: int(f.rsplit(".", 1)[1]) if f.rsplit(".", 1)[1].isdigit() else -1, reverse=True) + [src]
        out, seen = [], False
        for f in parts:
            try:
                txt = open(f, errors="replace").read()
            except OSError:
                continue
            for m in PTY_WRITE_BYTES_RE.finditer(txt):
                seen = True
                if float(m.group(2)) / 1000.0 >= t0 - 1:
                    out.append(unescape_rust_ascii_bytes(m.group(1)))
        # bytes first, decoded once: a UTF-8 character split over two writes is still one character
        return b"".join(out).decode("utf-8", errors="replace") if seen else None

    def app_pty_wrote_line(self, want, have, t0):
        """#1490/#1393: which line the app's OWN pty writes since t0 carry; see pty_line_verdict."""
        return pty_line_verdict(self.app_pty_text(t0), want, have)

    def app_log_all_lines(self):
        """every line of the app log (rotated chunks first, oldest first); [] without a trace"""
        import glob
        src = self.log_path
        if not src:
            return []
        parts = sorted(glob.glob(glob.escape(src) + ".in_session.*"), key=lambda f: int(f.rsplit(".", 1)[1]) if f.rsplit(".", 1)[1].isdigit() else -1, reverse=True) + [src]
        lines = []
        for f in parts:
            try:
                lines += open(f, errors="replace").read().splitlines()
            except OSError:
                continue
        return lines

    def pwsh_hook_timing(self, on):
        """#1401: turn pwsh.ps1's hook timing marks on/off (the shell checks `<file>.on` once per prompt, so the first command after a switch
        may be on either side). -> the marks file path, or None when this pane has none (not Windows PowerShell, an app without the env)"""
        path = getattr(self.app, "hook_timing_path", None) if self.win and self.pane == "powershell" else None
        if not path:
            return None
        try:
            if on:
                open(path + ".on", "w").close()
            elif os.path.exists(path + ".on"):
                os.remove(path + ".on")
        except OSError:
            return None
        return path

    def pwsh_hook_segments(self, path, t0, stamps_ms=()):
        """#1401: the per-hop split of PowerShell's own work for the commands since wall time t0 (pwsh_timing.segments), {} without marks"""
        import pwsh_timing
        try:
            text = open(path, errors="replace").read()
        except (OSError, TypeError):
            return {}
        return pwsh_timing.segments(text, self.app_log_all_lines(), stamps_ms, t0 * 1000.0)

    def app_saw_line(self, want, t0):
        """True: every character of `want` is in the app's own typed-character log, in order and contiguous (the key ARRIVED and was routed);
        False: the app's log has typed characters but not this line (the injector/OS never delivered it, or the log rotated it away); None: no trace."""
        got = self.app_log_chars(t0)
        if got is None:
            return None
        return want in got

    SSH_HOP_RE = re.compile(r"kh_iq_(?:hold|release)\b.*Q7_ssh_typeahead|kh_hold_(?:entry|exit)\b|kh_holding_set\b|gate_open kind=ssh_connect|ssh hold:|apply_block_completed_to_ssh_gate|hop=hand_route|kh_handover|handoff_commit")

    def app_hop_lines(self, rx, since):
        """[(wall_s or None, line)] of the app log lines (rotated chunks included, oldest first) matching `rx` whose t= is >= `since`"""
        import glob
        src = self.log_path
        if not src:
            return []
        parts = sorted(glob.glob(glob.escape(src) + ".in_session.*"), key=lambda f: int(f.rsplit(".", 1)[1]) if f.rsplit(".", 1)[1].isdigit() else -1, reverse=True) + [src]
        out = []
        for f in parts:
            try:
                for l in open(f, errors="replace").read().splitlines():
                    if rx.search(l):
                        m = re.search(r" t=(\d+(?:\.\d+)?)", l)
                        t = float(m.group(1)) / 1000.0 if m else None
                        if t is None or t >= since:
                            out.append((t, re.sub(r"^\S+\s+", "", l)[:150]))
            except OSError:
                continue
        return out

    def ssh_app_hops(self, since, limit=10):
        """the app's own ssh hold / hand-over lines since wall time `since` (Q7_ssh_typeahead hold+release, kh_hold_entry/exit, `ssh hold:` log lines, hand-over hops): the last `limit`"""
        return [l for _t, l in self.app_hop_lines(self.SSH_HOP_RE, since)][-limit:]

    def app_log_window(self, t0, t1=None):
        """the raw app-log lines (rotated chunks included, oldest first) between wall times t0 and t1 (None: now). A line without its own t= (a
        WARN such as `can_execute_command was false`) takes the t= of the last line before it, so it lands in the right trial window."""
        import glob
        src = self.log_path
        if not src:
            return []
        parts = sorted(glob.glob(glob.escape(src) + ".in_session.*"), key=lambda f: int(f.rsplit(".", 1)[1]) if f.rsplit(".", 1)[1].isdigit() else -1, reverse=True) + [src]
        out, last = [], None
        for f in parts:
            try:
                txt = open(f, errors="replace").read()
            except OSError:
                continue
            for l in txt.splitlines():
                m = re.search(r" t=(\d+(?:\.\d+)?)", l)
                if m:
                    last = float(m.group(1)) / 1000.0
                if last is not None and last >= t0 and (t1 is None or last <= t1):
                    out.append(l)
        return out

    def ssh_trial_verdict(self, t0, expected, remote_before, enters_before=1, t1=None):
        """#1292/#1332/#1345/#1394/#1400/#1596/#1614: what ONE ssh typeahead trial did, from the app's own log between t0 and t1 and the remote
        shell's rec.log lines logged after index `remote_before`. -> ssh_trial_facts() dict; its "text" is the one-line verdict."""
        try:
            remote_new = [c for _, c in self.prof.commands(remote=True)[remote_before:] if ".appci_where" not in c]
        except Exception:
            remote_new = []
        return ssh_trial_facts(self.app_log_window(t0, t1), expected, remote_new, enters_before)

    def ssh_client_probe(self):
        """r18d8: what the ssh CLIENT says for the same host alias, run now in the app's environment: exit status and the last stderr lines of `ssh -vv fake true`
        (the app's own ssh runs in its pty, whose stderr nobody captured). The full text goes to logs/ssh-client-probe.txt."""
        env = dict(os.environ, HOME=self.prof.home, PATH=os.path.join(self.prof.home, "bin") + os.pathsep + os.environ.get("PATH", os.defpath))
        try:
            p = subprocess.run(["ssh", "-vv", "fake", "true"], env=env, capture_output=True, text=True, timeout=15, stdin=subprocess.DEVNULL)
            err, rc = p.stderr or "", p.returncode
        except subprocess.TimeoutExpired as e:
            err, rc = (e.stderr.decode(errors="replace") if isinstance(e.stderr, bytes) else (e.stderr or "")), "timeout(15 s)"
        except OSError as e:
            return "ssh client probe could not run: %s" % e
        try:
            os.makedirs(os.path.join(self.run, "logs"), exist_ok=True)
            with open(os.path.join(self.run, "logs", "ssh-client-probe.txt"), "a") as f:
                f.write("=== %s ssh -vv fake true: exit %s ===\n%s\n" % (time.strftime("%H:%M:%S"), rc, err))
        except OSError:
            pass
        tail = [l.strip() for l in err.splitlines() if l.strip()][-3:]
        return "ssh client probe exit %s: %s" % (rc, " | ".join(tail) or "(no stderr)")

    def ssh_diag(self):
        """what the ssh test host saw, for a failing ssh case: the tail of sshd.log (connect / auth / session lines, now LogLevel DEBUG1) and fakesshd's stderr;
        since r18d8 also whether the test sshd was listening (port), a fresh ssh client probe (exit status + stderr tail), and the app's ssh hold / hand-over hops
        of this case - so 'ssh did not give a remote shell' says WHY the next time."""
        out = []
        for name in ("sshd.log", "fakesshd.stderr"):
            p = os.path.join(self.run, "ssh", name)
            try:
                tail = [l for l in open(p, errors="replace").read().splitlines() if l.strip()][-8:]
            except OSError:
                continue
            out.append("%s: %s" % (name, " | ".join(tail) if tail else "(empty)"))
        try:
            info = self.ssh or {}
            port = info.get("port")
            if port:
                import socket
                try:
                    socket.create_connection(("127.0.0.1", port), 1.0).close()
                    out.append("test sshd (%s) listening on 127.0.0.1:%d" % (info.get("kind"), port))
                except OSError as e:
                    out.append("test sshd (%s) NOT listening on 127.0.0.1:%d (%s)%s" % (info.get("kind"), port, e,
                               "; process EXITED rc=%s" % info["popen"].poll() if info.get("popen") is not None and info["popen"].poll() is not None else ""))
            if not self.win:
                out.append(self.ssh_client_probe())
            hops = self.ssh_app_hops(self.t_case0)
            out.append("app ssh hops: " + (" | ".join(hops) if hops else "none logged since the case started"))
        except Exception as e:      # diagnostics must never break the case
            out.append("ssh diag extras failed: %r" % e)
        if getattr(self.prof, "ssh_hint", None):
            out.append("PROBE: " + self.prof.ssh_hint)
        return "; ".join(out) or "no ssh server log"

    EXIT_WINDOW_S = 1.0       # agent_view/controller.rs ENTER_OR_EXIT_CONFIRMATION_WINDOW

    def _wrap_ctrlc(self, be):
        """record every Ctrl-C sent through be.key / be.keys (instance-level wrap; idempotent per backend object)"""
        if getattr(be, "_ctrlc_wrapped", None) is self:
            return
        ts = self.ctrlc_ts
        for name in ("key", "keys"):
            orig = getattr(be, name, None)
            if orig is None:
                continue

            def wrapped(spec, *a, _orig=orig, _nm=name, **k):
                specs = [spec] if _nm == "key" else list(spec)
                now = time.time()
                # key() may call keys() on the same backend: one physical Ctrl-C must not count twice
                if any(str(x).lower() == "ctrl+c" for x in specs) and not (ts and now - ts[-1] < 0.05):
                    ts.append(now)
                return _orig(spec, *a, **k)
            setattr(be, name, wrapped)
        be._ctrlc_wrapped = self

    def snapshot_log(self):
        """r18e9: a case that relaunches the app (ui_startup_time x5, remote_session_ext x2) loses the earlier launches' app log to the overwrite, so injector accounting
        counted only the last launch ("sent 331; app saw 164"). Copy the log (and its rotated chunks) aside BEFORE each stop; run.py accounts over all of them."""
        import glob
        try:
            lp = self.log_path
            if not lp or not os.path.exists(lp):
                return
            d = os.path.join(self.run, "logs", self.label, "acct")
            os.makedirs(d, exist_ok=True)
            base = os.path.join(d, "launch%d_%d.log" % (len(self.extra_logs), int(time.time())))
            shutil.copyfile(lp, base)
            for f in glob.glob(glob.escape(lp) + ".in_session.*"):
                shutil.copyfile(f, base + ".in_session." + f.rsplit(".", 1)[1])
            self.extra_logs.append(base)
        except Exception:
            pass

    def _wrap_type(self, be):
        """r18e9: remember what be.type() sent and when (the last few only), so a pane that vanished right after a typed `exit` is named as the case's own doing"""
        if getattr(be, "_type_wrapped", None) is self or not hasattr(be, "type"):
            return
        orig, log = be.type, self.typed_log

        def typed(text, *a, _orig=orig, **k):
            log.append((time.time(), str(text)))
            del log[:-20]
            return _orig(text, *a, **k)
        be.type = typed
        be._type_wrapped = self

    def last_typed_exit_since(self, t0):
        """True when the case's LAST typed text was `exit` (at or after t0)"""
        return bool(self.typed_log) and self.typed_log[-1][0] >= t0 and self.typed_log[-1][1].strip() == "exit"

    def in_subshell(self, base):
        """True/False/None: more shells under the app than `base` (a shell_count() taken before the subshell started). `exit` is typed only on True"""
        now = self.shell_count()
        return None if base is None or now is None else now > base

    def double_ctrlc_since(self, t0):
        """True when two Ctrl-C went out within the exit-confirmation window after t0: in an EMPTY agent conversation that is how the app is told to
        leave agent view (first press arms a confirmation, the second within 1 s exits; view.rs should_ctrl_c_exit_agent_view)"""
        t = [x for x in self.ctrlc_ts if x >= t0]
        return any(b - a < self.EXIT_WINDOW_S + 0.3 for a, b in zip(t, t[1:]))

    def reset(self):
        # r18d2 (Wayland): the agent pass left agent view after cadence/fast_typing because reset() sent Ctrl-C twice within the 1 s exit-confirmation window of an
        # empty agent conversation. By design that exits the view; so in agent mode a Ctrl-C is skipped when the previous one is that recent (the Ctrl-U still
        # clears the line; a running command is stopped by the cases that start one, which press Ctrl-C themselves).
        recent = self.ctrlc_ts and time.time() - self.ctrlc_ts[-1] < self.EXIT_WINDOW_S + 0.3
        if not (self.mode == "agent" and recent):
            self.be.key("ctrl+c")
        time.sleep(0.25)
        self.be.key("Escape" if getattr(self, "pane", None) == "cmd" else "ctrl+u")      # cmd.exe's line editor has no Ctrl-U; Escape clears its line
        time.sleep(0.15)

    def shot(self, tag):
        p = os.path.join(self.prof.shots, "%s_%s.png" % (self.label, tag))
        try:
            self.be.shot(p)
        except Exception:
            return None
        return p

    HARNESS_MARKER_RE = re.compile(r"(?:alive|proof)_[\w@.-]*\.txt")

    def cmds_since(self, mark):
        """what the shell logged after `mark`, minus the harness's OWN marker lines (the key-proof and wedge-probe `echo tok >> proof_*/alive_*.txt`): rec.log can
        log one of those late, after a case took its mark (r18d2 agent@gitbash fast_typing: 31 commands for 30 typed, every line shifted by that one)"""
        return [c for _, c in self.prof.commands()[mark:] if not self.HARNESS_MARKER_RE.search(c)]

    def cmd_mark(self):
        return len(self.prof.commands())

    def ps_history_path(self):
        """#1514: the PSReadLine history file of the PowerShell pane (the only record of the exact command line the shell received there: the pane has no rc
        hook, so no rec.log). None off Windows / when APPDATA is unknown."""
        if not (self.win and self.pane == "powershell"):
            return None
        env = dict(self.app.env()) if hasattr(getattr(self, "app", None), "env") else dict(os.environ)
        ad = env.get("APPDATA") or os.environ.get("APPDATA")
        return os.path.join(ad, "Microsoft", "Windows", "PowerShell", "PSReadLine", "ConsoleHost_history.txt") if ad else None

    def ps_history_lines(self):
        p = self.ps_history_path()
        try:
            return [l.rstrip("\r\n") for l in open(p, encoding="utf-8", errors="replace")] if p else []
        except Exception:
            return []

    def log_size(self):
        try:
            return os.path.getsize(self.log_path)
        except Exception:
            return 0

    def log_since(self, off):
        try:
            with open(self.log_path, "rb") as f:
                f.seek(off)
                return f.read().decode(errors="replace")
        except Exception:
            return ""

    def log_view_mode(self):
        """'agent' | 'terminal' | None from the LATEST `hop=view_mode mode=... pane=...` line of the app log (#1568): the app logs it when a pane
        is first shown and on every agent-view enter/exit, so no OCR is needed on any platform. Rotated chunks are read too (newest last).
        None when there is no log or no such line (a build older than #1568)."""
        import glob
        src = self.log_path
        if not src:
            return None
        parts = sorted(glob.glob(glob.escape(src) + ".in_session.*"), key=lambda f: int(f.rsplit(".", 1)[1]) if f.rsplit(".", 1)[1].isdigit() else -1, reverse=True) + [src]
        last = None
        for f in parts:
            try:
                txt = open(f, errors="replace").read()
            except OSError:
                continue
            for m in re.finditer(r"hop=view_mode mode=(agent|terminal)\b", txt):
                last = m.group(1)
        return last

    def mode_now(self, tag):
        """'agent' | 'terminal' | None. The app's own `hop=view_mode` log line first (#1568, works without OCR, e.g. on Windows); only when the
        log has none (older build) it falls back to OCR of a screenshot. Agent view: its header
        'New Phosphor Agent conversation', the 'ESC for terminal' bar, or the 'Ask anything' placeholder. Terminal session: the
        'new /agent conversation' hint or 'New terminal session' (r18cx: the agent pass had silently become a terminal session)."""
        logged = self.log_view_mode()
        if logged:
            return logged
        txt = self.ocr(tag)
        if txt is None:
            return None
        low = " ".join(txt.lower().split())
        agent = "new phosphor agent conversation" in low or "for terminal" in low or "ask anything" in low
        term = (("/agent" in low and "conversation" in low) or "new terminal session" in low
                or "run commands, or ask the agent" in low or "to continue conversation" in low)     # r18cz: the terminal prompt's other hints
        if agent and not term:
            return "agent"
        if term and not agent:
            return "terminal"
        return None

    def ocr(self, tag):
        mac_ocr = os.path.join(getattr(self.be, "TOOLS", "") or "", "ocr") if self.be.name == "mac" else ""
        if not shutil.which("tesseract") and not (mac_ocr and os.access(mac_ocr, os.X_OK)):
            return None
        p = self.shot(tag)
        if not p:
            return None
        if shutil.which("tesseract"):
            r = subprocess.run(["tesseract", p, "-"], capture_output=True, timeout=60)
        else:
            r = subprocess.run([mac_ocr, p], capture_output=True, timeout=60)      # Apple Vision (mac/tools/ocr.swift)
        return r.stdout.decode(errors="replace")


# ---------------------------------------------------------------- mode proof
@case("mode_proof", order=1, gui=True)
def c_mode_proof(ctx, r):
    """screenshot proof of the session mode before any key is typed (shots/<mode>_proof.png is the evidence), plus OCR when an engine exists.
    Agent mode = header 'New Phosphor Agent conversation' + 'Ask anything' placeholder (+ model chip with the CI model).
    The hint 'new /agent conversation' under a prompt means a TERMINAL session: it must not count as an agent marker."""
    # The proof is read from the PRE-KEY screenshot (shots/<mode>_proof_prekey.png): Escape in a fresh agent view LEAVES agent mode
    # (macOS CI run 3015802940 showed 'New terminal session' after the Escape in the agent pass), so the Escape is sent only after.
    if ctx.be.name == "mac":
        d = ctx.be.system_dialog(ocr=True)
        if d:
            raise ctx.be.needs_owner(d)
    txt = ctx.ocr("proof_prekey")      # taken before ANY key is sent
    logged = ctx.log_view_mode()       # #1568: the app's own view-mode log line, read before any key too (None on a build without it)
    if logged is not None:
        r.metric("log_view_mode", logged)
    # #1532: which destination logic the app under test runs (the fix tier's kill-switch pass runs with PHOSPHOR_QUEUE_DESTINATION=legacy)
    r.metric("queue_destination", dest_mode())
    if ctx.mode != "agent":
        # r18cx (macOS): this Escape is the agent view's own "esc: go back to terminal", so in the agent pass it left agent mode and every
        # "agent" case after it ran in a terminal session (owner photo). Only the terminal pass sends it.
        ctx.be.key("Escape")
        time.sleep(0.8)
    ctx.shot("proof")
    if logged is not None:
        r.check(logged == ctx.mode, "%s mode expected but the app's log says hop=view_mode mode=%s before any key was typed" % (ctx.mode, logged))
    if txt is None:
        if logged is not None:
            r.note("mode machine-verified from the app log (hop=view_mode mode=%s); no OCR engine" % logged)
            return
        r.note("no OCR engine: screenshot only (shots/%s_proof_prekey.png), mode not machine-verified" % ctx.label)
        return
    low = " ".join(txt.lower().split())
    hint = "/agent" in low and "conversation" in low                       # terminal-mode hint line
    header = "new phosphor agent conversation" in low
    placeholder = "ask anything" in low
    is_agent = (header or placeholder) and not (hint and not header)
    r.metric("ocr_agent_markers", is_agent)
    r.metric("ocr_terminal_hint", hint)
    if ctx.mode == "agent":
        r.check(is_agent, "agent mode expected but the screen shows no agent markers ('New Phosphor Agent conversation' / 'Ask anything'); terminal hint present: %s" % hint)
        if ctx.ollama:
            chip = "gpt-oss" in low
            r.metric("ocr_model_chip_gpt_oss", chip)
            r.check(chip, "model chip 'gpt-oss:20b' not visible before any key was typed (the CI Ollama model is not the active one)")
    else:
        r.check(not is_agent, "terminal mode expected but the screen shows agent markers ('New Phosphor Agent conversation' / 'Ask anything')")


# ---------------------------------------------------------------- cadence
@case("cadence", order=2, gui=True)
def c_cadence(ctx, r):
    """the input tool really types at 10-25 ms/key: arrival times measured at the pty by ts_reader.py"""
    out = os.path.join(ctx.W, "cad.txt")
    ctx.rm("cad.txt")
    n = 60
    t_start = time.time()
    mark = ctx.cmd_mark()
    prompts0 = ctx.prompts()
    reader = os.path.join(os.path.dirname(os.path.abspath(__file__)), "ts_reader.py")
    if ctx.win:      # the private interpreter by full path (it is not on the VM's PATH); msvcrt.getwch() in ts_reader.py reads the keys the ConPTY delivers
        py, reader = sys.executable.replace("\\", "/"), reader.replace("\\", "/")
        cmd = ("& '%s' '%s' cad.txt %d" if ctx.pane == "powershell" else '"%s" "%s" cad.txt %d') % (py, reader, n)
    else:
        cmd = "python3 %s %s %d" % (reader, "cad.txt", n)
    ctx.be.type(cmd, 12, enter=True)
    time.sleep(2.0)   # python3 start + READY
    delay = 15
    ctx.be.type("".join(chr(97 + i % 26) for i in range(n)), delay)
    ctx.until(lambda: os.path.exists(out), 15)
    try:
        rows = [l.split() for l in open(out)]
    except FileNotFoundError:
        # r18cj: a character of the path was lost in the Git Bash pane ("C:/Users/inters/..." for Winters), bash answered "No such file or directory", and this case
        # blamed "python3 missing". The shell records the typed command once its prompt is back, so judge it here: a lost key is reported as one.
        ctx.until(lambda: bool(ctx.cmds_since(mark)), 5)
        recv = ctx.cmds_since(mark)
        if recv and recv[0] != cmd:
            r.check(False, "the cadence command arrived corrupted, so ts_reader never ran (NOT a missing python): " + line_evidence(ctx, cmd, recv[0], t_start, None))
        else:
            r.check(False, "ts_reader produced no output (the reader did not start or did not receive %d keys; the command as the shell recorded it: %r)" % (n, recv[:1]))
        ctx.reset()
        return
    if ctx.prof.local_prompts:      # let the shell finish ts_reader and print its prompt: its command line is logged at that prompt, and would otherwise land after the next case's mark
        ctx.until(lambda: ctx.prompts() > prompts0, 10)
    ts = [float(b) for _, b in rows]
    gaps = [(b - a) * 1000 for a, b in zip(ts, ts[1:])]
    r.metric("gap_ms", stats(gaps))
    if gaps:
        med = statistics.median(gaps)
        r.check(5 <= med <= 40, "median inter-key gap %.1f ms is outside 5..40 ms for a requested %d ms (input tool cadence wrong)" % (med, delay))
    r.check(len(rows) == n, "only %d of %d characters arrived" % (len(rows), n))
    ctx.reset()


# ---------------------------------------------------------------- typing
def classify_line(want, have):
    """how the shell's received line `have` differs from the typed line `want`: one character lost / one extra / characters moved / other"""
    if len(have) == len(want) - 1 and any(want[:i] + want[i + 1:] == have for i in range(len(want))):
        return "one character LOST"
    if len(have) == len(want) + 1 and any(have[:i] + have[i + 1:] == want for i in range(len(have))):
        return "one EXTRA character"
    if len(have) == len(want) and sorted(have) == sorted(want):
        return "characters REORDERED (a character moved: %s)" % next(("%r came before %r" % (have[i], want[i]) for i in range(len(want)) if have[i] != want[i]), "?")
    return "differs (%d typed, %d received characters)" % (len(want), len(have))


PTY_WRITE_BYTES_RE = re.compile(r'input_order hop=pty_write_bytes\b[^\n]*? bytes="((?:[^"\\]|\\.)*)"[^\n]*? t=(\d+(?:\.\d+)?)')
# What the app writes to the shell's line that is never line text: the typeahead report requests (bash/zsh `ESC i`, fish `ESC 1`) and the
# bracketed-paste markers an editor command is wrapped in. Removed before the line is looked for, so a request written between two keys does not
# split it. The kill-line bytes (^P, ^K^U, ESC 2) stay: text written before one is not the line the shell runs.
PTY_NON_LINE_BYTES = ("\x1b[200~", "\x1b[201~", "\x1bi", "\x1b1")
PTY_LINE_VERDICT_TEXT = {
    "have": "the app's OWN pty writes (pty_write_bytes) carry the corrupted line: Phosphor wrote it that way (app-side: the hand-off / editor "
            "path moved or dropped the keys before the pty)",
    "want": "the app's own pty writes (pty_write_bytes) carry the line exactly as typed and not the corrupted one: it was changed after the "
            "app's pty write (ConPTY / the shell's tty)",
    "neither": "the app's own pty writes (pty_write_bytes) carry neither line contiguously (part of it went to the shell's line, part through "
               "an editor command): read the case log's pty_write_bytes / handoff_insert / input_report hops for the split",
    None: "no pty byte trace in the app log (pty_write_bytes with WARP_INPUT_ORDER_LOG_TEXT=1), so where it changed is not known",
}


def unescape_rust_ascii_bytes(s):
    """the inverse of Rust's `<[u8]>::escape_ascii` (what the app's input_order log prints), as BYTES: \\t \\r \\n \\' \\" \\\\ and \\xNN, where
    \\xNN is one byte (escape_ascii writes every non-ASCII byte that way, so a UTF-8 character is several of them)"""
    out, i = bytearray(), 0
    while i < len(s):
        ch = s[i]
        if ch == "\\" and i + 1 < len(s):
            nxt = s[i + 1]
            if nxt == "x" and re.fullmatch(r"[0-9a-fA-F]{2}", s[i + 2:i + 4]):
                out.append(int(s[i + 2:i + 4], 16))
                i += 4
                continue
            out += {"t": b"\t", "r": b"\r", "n": b"\n", "0": b"\0"}.get(nxt, nxt.encode("utf-8"))
            i += 2
            continue
        out += ch.encode("utf-8")
        i += 1
    return bytes(out)


def unescape_rust_ascii(s):
    """unescape_rust_ascii_bytes decoded as UTF-8 (invalid bytes replaced)"""
    return unescape_rust_ascii_bytes(s).decode("utf-8", errors="replace")


# Bytes that kill the shell's line: ^U, ^K, ^P (the bootstrap's kill-whole-line), PowerShell's ESC 2.
PTY_KILL_BYTES = ("\x15", "\x0b", "\x10", "\x1b2")


def pty_line_verdict(got, want, have):
    """#1490/#1393: 'have' when the corrupted line `have` is in the app's pty bytes `got` (and is not just a piece of the typed line), 'want' when the
    typed line is there instead and no kill byte (PTY_KILL_BYTES) follows it before its line ends (a line written and then killed did not reach
    the shell as typed), 'neither' otherwise, None without a trace. Report requests and bracketed-paste markers are removed first
    (PTY_NON_LINE_BYTES)."""
    if got is None:
        return None
    for b in PTY_NON_LINE_BYTES:
        got = got.replace(b, "")
    if have and have not in want and have in got:
        return "have"
    at = got.rfind(want)
    if at >= 0:
        rest = got[at + len(want):]
        ends = [i for i in (rest.find("\r"), rest.find("\n")) if i >= 0]
        rest = rest[:min(ends)] if ends else rest
        if not any(k in rest for k in PTY_KILL_BYTES):
            return "want"
    return "neither"


def line_evidence(ctx, want, have, t0, ctrl=None):
    """one sentence of proof for a corrupted line: what changed, whether the APP's own typed-character log shows the line arriving intact and in
    order (then the injector delivered it), whether the app's own pty writes carry the corrupted line or the typed one (#1490/#1393: app-side
    vs. after the pty write), and what the reference terminal did with the same injection in this session (a reference that kept every line
    exactly makes this Phosphor-side)."""
    parts = ["%s: typed %r, shell received %r" % (classify_line(want, have), want[:90], have[:90])]
    saw = ctx.app_saw_line(want, t0)
    if saw is True:
        # #1490/#1393: keys arriving in order say nothing about what the app WROTE: the hand-off moves keys between the shell's line and the editor
        # (a report, an editor command). Where the line changed is read from the app's own pty writes below, never from typed_characters (r18ct
        # said "changed after the app's pty write" from this line alone).
        parts.append("the app's log shows every character of the line arriving in order (typed_characters)")
    elif saw is False:
        parts.append("the app's log does NOT show the line's characters in order (never delivered to the app, or the log rotated it away)")
    else:
        parts.append("no key trace in the app log")
    wrote = ctx.app_pty_wrote_line(want, have, t0) if hasattr(ctx, "app_pty_wrote_line") else None
    parts.append(PTY_LINE_VERDICT_TEXT[wrote])
    if ctrl:
        parts.append("reference %s with the same injection: %d/%d lines missing, %s received exactly" % (
            ctrl.get("ref"), ctrl.get("bad", 0), ctrl.get("n", 0), "all" if not ctrl.get("exact_bad") else "NOT all (%d inexact)" % ctrl["exact_bad"]))
    return "; ".join(parts)


MAX_FORGIVEN_LOST = 1       # #1514: Phosphor may lose at most this many lines of a burst when the reference lost lines too


def forgiven_by_reference(n, got_lines, ctrl):
    """A Phosphor burst failure is forgiven as an injector/OS loss only when the reference terminal lost lines in the same session AND Phosphor
    lost no more than the reference and no more than MAX_FORGIVEN_LOST (#1514: it used to forgive ANY loss once the reference lost one)."""
    if not ctrl or not (ctrl.get("bad", 0) > 0 or (ctrl.get("exact_bad") or 0) > 0):
        return False
    lost = max(0, n - got_lines)
    return lost <= max(ctrl.get("bad", 0), 0) + (1 if (ctrl.get("exact_bad") or 0) > 0 and not ctrl.get("bad") else 0) and lost <= MAX_FORGIVEN_LOST


def effect_run(ctx, r, tag, n, gap, rollover=False, shell=None, attribute=None):
    """attribute: optional callable(gap) -> enter-control dict (run_enter_control). When given and the run fails with merged lines (a lost
    Enter), the failure is an APP bug only if the reference terminal kept every Enter in the same session (#1482)."""
    fname = "r_%s.txt" % tag
    shell = shell or ctx.pane_shell()
    ctx.rm(fname)
    t_start = time.time()
    mark = ctx.cmd_mark()
    hmark = len(ctx.ps_history_lines())
    k0 = len(ctx.be.calls) if ctx.be.calls is not None else None
    exp = []
    for i in range(n):
        tok = ctx.tok(10).lower()
        cmd = ctx.append_cmd(shell, tok, fname)
        exp.append(tok)
        if rollover:
            ctx.be.roll(cmd, overlap_ms=ctx.rng.choice([6, 10, 14]), enter=True)
        else:
            ctx.be.type(cmd, ctx.rng.randint(10, 25), enter=True)
        if gap:
            time.sleep(gap)
    ctx.until(lambda: len(ctx.lines(fname)) >= n, 20 + n * 0.5)
    time.sleep(0.5)
    got = ctx.lines(fname)
    ok = got == exp
    bad_i = next((i for i, (a, b) in enumerate(zip(exp + [None], got + [None])) if a != b), None)
    ev = ""
    if not ok and bad_i is not None and bad_i < n:
        # what the injector says about the Enter that should have ended the first bad line (its call is k0+bad_i: one injector call per line)
        v = ctx.be.inject_enter(k0 + bad_i) if k0 is not None else None
        ev = " [first bad line %d: %s]" % (bad_i, v[1] if v else "injector keeps no record on this backend")
    merged = False
    if ctx.prof.commands():
        want0 = [ctx.append_cmd(shell, t, fname) for t in exp]
        have0 = ctx.cmds_since(mark)
        merged = any(sum(1 for w in want0 if w in h) >= 2 for h in have0)
    elif not ok:
        merged = len(got) != n
    verdict = None
    ctrl = None
    if not ok and (merged or ctx.win) and attribute:      # Windows: a character moved/lost inside a line is attributed the same way as a lost Enter
        ctrl = attribute(gap)
        if ctrl is None:
            verdict = " (no reference terminal: lost Enter not attributable)"
        elif forgiven_by_reference(n, len(got), ctrl):
            r.note("%s: injector/OS lost Enter (reference too): reference %s merged/lost %d/%d lines with the same burst%s; Phosphor file has %d/%d lines" % (
                tag, ctrl["ref"], ctrl["bad"], ctrl["n"], ev, len(got), n))
            r.metric("injector_lost_enter_" + tag, dict(reference_bad=ctrl["bad"], n=ctrl["n"], phosphor_lines=len(got)))
            ok = True
            verdict = "skip"
        elif ctrl["bad"] > 0 or (ctrl.get("exact_bad") or 0) > 0:
            verdict = " -- the reference also lost %d/%d lines, but Phosphor lost %d: more than the reference or more than %d (#1514), so NOT forgiven" % (
                ctrl["bad"], ctrl["n"], max(0, n - len(got)), MAX_FORGIVEN_LOST)
        else:
            verdict = " -- the reference kept every line exactly in the same session (%d/%d lines): Phosphor-side (#1482)" % (ctrl["n"] - ctrl["bad"], ctrl["n"])
    if verdict != "skip":
        r.check(ok, "%s: file has %d/%d lines, exact match %s%s%s%s" % (tag, len(got), n, ok,
                "" if ok else " first diff: exp=%r got=%r" % next(((a, b) for a, b in zip(exp + [None], got + [None]) if a != b), (None, None)), ev, verdict or ""))
    if ctx.prof.commands():
        want = [ctx.append_cmd(shell, t, fname) for t in exp]
        have = ctx.cmds_since(mark)
        if have and verdict == "skip":
            pass
        elif have:
            pair = next(((h, w) for h, w in zip(have, want) if h != w), None)
            ev2 = ""
            if pair and have != want:
                ev2 = " -- " + line_evidence(ctx, pair[1], pair[0], t_start, ctrl)
            r.check(have == want, "%s: shell received %d commands, not the typed ones (first mismatch: %r)%s" % (
                tag, len(have), pair[0] if pair else "count differs %d vs %d" % (len(have), len(want)), ev2))
        else:
            r.note("%s: rec.log empty (PROMPT_COMMAND hook not run), effect file checked only" % tag)
    elif ctx.pane == "powershell" and verdict != "skip":
        # #1514: no rec.log in the PowerShell pane, so an extra character that still wrote the token passed. The exact line comes from PSReadLine's history file.
        want = [ctx.append_cmd(shell, t, fname) for t in exp]
        have = ctx.ps_history_lines()[hmark:]
        try:        # r18e9: the history slice this check compared, kept per case as evidence (the validator had no copy to settle a mismatch)
            ld = os.path.join(ctx.run, "logs", ctx.label)
            os.makedirs(ld, exist_ok=True)
            open(os.path.join(ld, "ps_history_%s.txt" % tag), "w", encoding="utf-8", errors="replace").write(
                "hmark=%d total=%d new=%d want=%d\n%s\n" % (hmark, hmark + len(have), len(have), len(want), "\n".join(repr(h) for h in have)))
        except Exception:
            pass
        if len(have) == len(want):
            bad = [(h, w) for h, w in zip(have, want) if h != w]
            r.check(not bad, "%s: PowerShell received %d of %d lines NOT exactly as typed (PSReadLine history; first: got %r, want %r)" % (
                tag, len(bad), len(want), bad[0][0] if bad else None, bad[0][1] if bad else None))
        else:
            r.metric("command_text_unverified_" + tag, dict(history_lines=len(have), typed=len(want)))
            r.note("%s: command text NOT verified in the PowerShell pane: PSReadLine history has %d new line(s) for %d typed (file missing, filtered or not yet written); the effect file (exact lines, no extras) was checked only" % (tag, len(have), len(want)))
    return ok


@case("fast_typing", order=10)
def c_fast(ctx, r):
    for gap in ctx.opts.get("ft_gaps", (0.0, 0.25)):
        attr = (lambda g: (run_enter_control(ctx, r, ctx.count(30)) or {}).get(g) or None) if ctx.opts.get("ref", True) else None
        effect_run(ctx, r, "ft%d" % int(gap * 100), ctx.count(30), gap, attribute=attr)


@case("rollover", order=11)
def c_roll(ctx, r):
    effect_run(ctx, r, "roll", ctx.count(20), 0.1, rollover=True)


@case("expert_fuzz", order=12)
def c_fuzz(ctx, r):
    """typos fixed with BackSpace / Left+insert / Ctrl-W, seeded; exact command verified"""
    fname = "fz.txt"
    ctx.rm(fname)
    exp = []
    N = ctx.count(25)
    for i in range(N):
        w = "".join(ctx.rng.choice("abcdefghijklmnopqrstuvwxyz") for _ in range(ctx.rng.randint(4, 9)))
        tok = "fz%d%s" % (i, w)
        exp.append(tok)
        k = ctx.rng.randrange(1, len(tok) - 1)
        mode = ctx.rng.choice(["bs", "left", "ctrlw", "plain"])
        dl = ctx.rng.randint(10, 25)
        if mode == "bs":
            ctx.be.type("echo " + tok[:k] + "q", dl)
            ctx.be.key("BackSpace")
            ctx.be.type(tok[k:] + " >> " + fname, dl, enter=True)
        elif mode == "left":
            ctx.be.type("echo " + tok[:k] + tok[k + 1:], dl)
            ctx.be.key("Left", repeat=len(tok) - k - 1, delay_ms=12)
            ctx.be.type(tok[k], dl)
            ctx.be.key("End")
            ctx.be.type(" >> " + fname, dl, enter=True)
        elif mode == "ctrlw":
            ctx.be.type("echo " + tok + " >> " + fname + " xyz", dl)
            ctx.be.key("ctrl+w")
            ctx.be.key("Return")
        else:
            ctx.be.type("echo " + tok + " >> " + fname, dl, enter=True)
        time.sleep(ctx.rng.choice([0, 0, 0.05, 0.3]))
    ctx.until(lambda: len(ctx.lines(fname)) >= N, 30)
    time.sleep(0.5)
    got = ctx.lines(fname)
    r.check(got == exp, "fuzz: %d/%d lines, mismatches %s" % (len(got), N, [(a, b) for a, b in zip(exp, got) if a != b][:3]))


@case("paste_instant_enter", order=13)
def c_paste_enter(ctx, r):
    ctx.rm("pe.txt")
    exp, sent = [], []
    mark = ctx.cmd_mark()
    for i in range(ctx.count(6)):
        tok = ctx.tok(8)
        pad = "w" * ctx.rng.choice([10, 500, 5000])
        if ctx.pane == "powershell":     # no md5sum: the pasted line writes tok+pad itself, compared exactly
            ctx.be.clip_set("Add-Content pe.txt %s%s" % (tok, pad))
            exp.append(tok + pad)
        else:
            ctx.be.clip_set("echo %s%s | md5sum >> pe.txt" % (tok, pad))
            exp.append(hashlib.md5((tok + pad + "\n").encode()).hexdigest())
            sent.append("echo %s%s | md5sum >> pe.txt" % (tok, pad))
        ctx.be.paste(enter=True)
        time.sleep(0.8)
    ctx.until(lambda: len(ctx.lines("pe.txt")) >= len(exp), 15)
    got = ctx.lines("pe.txt")
    if ctx.pane != "powershell":
        # compare the DIGEST: Git for Windows' md5sum prints "<hash> *-" (binary marker), GNU coreutils "<hash>  -" (r18cj: 0/6 in both Git Bash panes while the
        # shell's own rec.log showed all six pasted lines arrived byte-exact, 10/500/5000-character pads)
        got = [l.split()[0] if l.split() else l for l in got]
        recv = [c for c in ctx.cmds_since(mark) if "alive_" not in c] if (ctx.win and ctx.prof.commands()) else None     # a wedge probe's line can be logged late (r18co)
        if recv is not None:
            r.check(recv == sent, "pasted lines reached the shell exactly: %d/%d byte-exact (first mismatch: %r)" % (
                sum(a == b for a, b in zip(sent, recv)), len(sent), next(((b[:60], len(b)) for a, b in zip(sent, recv) if a != b), "count %d vs %d" % (len(recv), len(sent)))))
    r.check(got == exp, "paste + instant Enter: %d/%d %s lines exact%s" % (sum(a == b for a, b in zip(exp, got)), len(exp), "exact" if ctx.pane == "powershell" else "md5 digest",
            "" if got == exp else " (first got %r, expected %r)" % (got[:1], exp[:1])))


@case("arrows_behind_paste", order=14)
def c_paste_arrows(ctx, r):
    ctx.rm("pa.txt")
    exp = []
    for i in range(ctx.count(5)):
        tok = ctx.tok(6)
        pad = "q" * ctx.rng.choice([100, 3000])
        ctx.be.clip_set(tok + pad)
        if ctx.pane == "powershell":
            exp.append(tok + pad + "xzy")
            ctx.be.type("Add-Content pa.txt ", 12)
        else:
            exp.append(hashlib.md5((tok + pad + "xzy\n").encode()).hexdigest())
            ctx.be.type("echo ", 12)
        ctx.be.paste()
        ctx.be.keys(["x", "y", "Left", "z", "End"], delay_ms=8)
        if ctx.pane == "powershell":
            ctx.be.key("Return")
        else:
            ctx.be.type(" | md5sum >> pa.txt", 8, enter=True)
        time.sleep(1.0)
    ctx.until(lambda: len(ctx.lines("pa.txt")) >= len(exp), 15)
    got = ctx.lines("pa.txt")
    if ctx.pane != "powershell":
        got = [l.split()[0] if l.split() else l for l in got]      # digest only: Git for Windows' md5sum prints "<hash> *-"
    r.check(got == exp, "arrow keys behind a paste: %d/%d exact" % (sum(a == b for a, b in zip(exp, got)), len(exp)))


@case("big_paste", order=15, gui=True)
def c_big(ctx, r):
    mb = float(os.environ.get("APPCI_BIG_MB", "1"))
    rr = random.Random(5)
    out, tot, size = [], 0, int(mb * 1_000_000)
    while tot < size:
        l = "".join(rr.choice("abcdefghijklmnopqrstuvwxyz0123456789 ") for _ in range(rr.randint(20, 120)))
        out.append(l)
        tot += len(l) + 1
    data = "\n".join(out)[:size]
    ctx.rm("big.out")
    ctx.be.type("cat > big.out", 12, enter=True)
    time.sleep(1.0)
    ctx.be.clip_set(data)
    ui_off = ctx.log_size()
    ctx.be.paste()

    def stable():
        p = os.path.join(ctx.W, "big.out")
        a = os.path.getsize(p) if os.path.exists(p) else -1
        time.sleep(1.5)
        return a > 0 and a == (os.path.getsize(p) if os.path.exists(p) else -2)
    ctx.until(stable, 30 + 20 * mb, step=0.1)
    ctx.be.key("Return")
    time.sleep(0.5)
    ctx.be.key("ctrl+d")
    time.sleep(2)
    got = open(os.path.join(ctx.W, "big.out"), "rb").read() if os.path.exists(os.path.join(ctx.W, "big.out")) else b""
    r.metric("bytes_expected", len(data) + 1)
    r.metric("bytes_got", len(got))
    ui_item_metrics(ctx, r, ui_off, "big_paste")    # #1191: which main-thread items broke the 4 ms budget while the paste streamed
    r.check(got == (data + "\n").encode(), "%.0f MB paste not byte-exact (expected %d bytes, got %d)" % (mb, len(data) + 1, len(got)))
    ctx.reset()


@case("tab_enter", order=16)
def c_tab(ctx, r):
    """Tab then Enter must run the COMPLETED line (gap 0 / 50 / 150 ms); no stray file from the uncompleted line"""
    target = "tc/zzcompl_alpha_unique.txt"
    ctx.rm(target)
    open(os.path.join(ctx.W, target), "w").close()
    exp = []
    for gap in (0, 0.05, 0.15):
        for i in range(ctx.count(4)):
            tok = ctx.tok(6)
            exp.append(tok)
            ctx.be.type(("Add-Content -Value %s -Path tc/zzcompl_alpha_u" if ctx.pane == "powershell" else "echo %s >> tc/zzcompl_alpha_u") % tok, 15)
            ctx.be.key("Tab")
            if gap:
                time.sleep(gap)
            ctx.be.key("Return")
            time.sleep(0.5)
    ctx.until(lambda: len(ctx.lines(target)) >= len(exp), 10)
    got = ctx.lines(target)
    r.check(got == exp, "Tab+Enter: completed file has %d/%d expected lines" % (len(got), len(exp)))
    stray = [f for f in os.listdir(os.path.join(ctx.W, "tc")) if f != "zzcompl_alpha_unique.txt"]
    r.check(not stray, "Tab+Enter ran the UNCOMPLETED line, stray files: %s" % stray)


def ctrlc_line(ctx, fname, i, flood=False):
    """The line typed for one Ctrl-C sample. IDENTICAL in Phosphor and the reference terminal (same command, same timestamp source).
    macOS: zsh's $EPOCHREALTIME in the trap (no process start; the old ~/bin/now helper is a python3 start, ~30 ms of harness cost
    that inflated every absolute number). Linux: `date +%s.%N` (one fork of a tiny binary, the same in both terminals).
    flood=True (#1514): the interrupted command is `yes` writing to the terminal (an output flood) instead of `sleep 30`; unix panes only."""
    run = "yes" if flood else "sleep 30"
    if ctx.pane == "powershell":
        # Windows PowerShell: the finally block of the interrupted try stamps UTC ms (13 digits; the Windows PowerShell harness of #1487). Ctrl-C stops
        # the whole line, so there is no AFTER marker: the next sample's typed line proves the shell came back
        # .NET in the finally block, by absolute path: a cmdlet (Add-Content) run from the finally of a Ctrl-C-stopped pipeline took 3.5 s in Phosphor AND in
        # the conhost reference and left the pane wedged for good (r18cj), and .NET's cwd is not PowerShell's location, hence the absolute path
        # r18ct: each stamp carries its SAMPLE INDEX ("<i>:<ms>"). Pairing stamps to samples by order made a late stamp (one that landed after the 6 s wait) pair with the NEXT
        # sample's t0, which printed negative latencies (-2273 ms) and hid whether Ctrl-C ever interrupted the sleep.
        return ("try { Start-Sleep 30 } finally { [IO.File]::AppendAllText('%s', ('%d:' + [string]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()) + [char]10)) }"
                % (os.path.join(ctx.W, fname).replace("\\", "/"), i))
    if ctx.be.name == "mac":
        return "zsh -c 'zmodload zsh/datetime; trap \"print -r -- \\$EPOCHREALTIME >> %s; exit\" INT; %s'; echo AFTER%d >> %s" % (fname, run, i, fname)
    return "bash -c 'trap \"date +%%s.%%N >> %s; exit\" INT; %s'; echo AFTER%d >> %s" % (fname, run, i, fname)


def ctrlc_samples(ctx, n, fname, budget_s=None, flood=False, bail_after=None):
    """n x (start a trap-instrumented sleep, press Ctrl-C, read the trap's timestamp) -> (latencies ms, all delivered)"""
    ctx.rm(fname)
    lat_ms, typed, misses = [], [], []
    mark = ctx.cmd_mark()
    t_start = time.time()
    pw = ctx.pane == "powershell"
    t_end = time.time() + budget_s if budget_s else None
    for i in range(n):
        if t_end and time.time() > t_end:
            break
        if pw and bail_after and i == bail_after and not lat_ms:
            # r18d2: every one of the first samples ran its full 30 s sleep (each miss costs ~36 s; 33 of them held the pane for ~20 minutes). Nothing stops
            # the sleep: stop here, the caller asks the reference whether the INJECTED Ctrl-C can stop it at all.
            break
        typed.append(ctrlc_line(ctx, fname, i, flood))
        ctx.be.guard_ref()
        ctx.be.type(typed[-1], 10, enter=True)
        time.sleep(1.5 if pw else 1.2)
        t0 = time.time()
        ctx.be.key("ctrl+c")
        if pw:
            ctx.until(lambda: i in ps_stamps(ctx, fname), 6)
        else:
            ctx.until(lambda: sum(1 for l in ctx.lines(fname) if l.startswith("AFTER")) > i, 5)
        time.sleep(0.2)
        if pw:
            ts = ps_stamps(ctx, fname)
            if i in ts and ts[i] >= t0 - 0.05:
                lat_ms.append((ts[i] - t0) * 1000)
            elif i not in ts:
                # a miss: Ctrl-C did not stop the sleep within 6 s. Press it again, then WAIT for this sample's own stamp (the sleep ends by itself after 30 s) so the
                # next sample's line is not typed behind a still-running command: r18ct queued 33 x `Start-Sleep 30` that way and the pane stayed busy for ~16 minutes
                # (pane_wedged_after_ctrl_c_d).
                ctx.be.key("ctrl+c")
                time.sleep(0.8)
                ctx.be.key("ctrl+c")
                if not ctx.until(lambda: i in ps_stamps(ctx, fname), 36):
                    time.sleep(1)
                misses.append(i)
            else:
                misses.append(i)        # a stamp that PRECEDES this sample's Ctrl-C: the finally ran without our key (not counted as a delivery)
            continue
        ts = stamps(ctx, fname)
        if len(ts) > len(lat_ms):
            lat_ms.append((ts[len(lat_ms)] - t0) * 1000)
    after = [l for l in ctx.lines(fname) if l.startswith("AFTER")]
    if ctx.prof.commands():
        # a sample that never fired is either a Ctrl-C that did not arrive or a typed LINE that arrived corrupted (no trap, no stamp): tell them apart
        have = ctx.cmds_since(mark)
        pairs = [(w, h) for w, h in zip(typed, have) if w != h]
        ctx.cc_integrity[fname] = dict(n=len(typed), got=len(have), bad=len(pairs), examples=[line_evidence(ctx, w, h, t_start, None) for w, h in pairs[:2]])
    if pw and misses:
        ctx.cc_integrity.setdefault("misses", {})[fname] = misses
    return lat_ms, len(lat_ms) == n and (pw or len(after) == n)


def ps_stamps(ctx, fname):
    """PowerShell pane: {sample index: epoch seconds} from the `<i>:<13-digit UTC ms>` lines the finally block writes (see ctrlc_line)"""
    out = {}
    for l in ctx.lines(fname):
        m = re.fullmatch(r"(\d+):(\d{13})", l.strip().replace("\x00", ""))
        if m:
            out.setdefault(int(m.group(1)), int(m.group(2)) / 1000.0)
    return out


def stamps(ctx, fname):
    """epoch seconds written by the trap / finally block: `1699999999.123456789` (date, EPOCHREALTIME) or 13-digit UTC milliseconds (PowerShell)"""
    out = []
    for l in ctx.lines(fname):
        l = l.strip().replace("\x00", "")
        if re.fullmatch(r"\d+\.\d+", l):
            out.append(float(l))
        elif re.fullmatch(r"\d{13}", l):
            out.append(int(l) / 1000.0)
    return out


def ctrlc_flood(ctx, r, n, warm):
    """#1514: the owner budget (docs/testing.md) is Ctrl-C "under flood", the plain-sleep measurement above is the control. Same trap/stamp line, but the
    interrupted command is `yes` writing to the terminal. Phosphor vs the reference terminal, key 'ctrl_c_flood'. Unix panes only.
    r18e0 follow-up: Phosphor and the reference are measured in A/B/A/B blocks (run_interleaved, like latency_enter); a reference measured AFTER Phosphor
    confounded run order / drift with the overhead. When the reference cannot be kept open next to Phosphor the row is NOREF (Phosphor's own numbers
    only, reason recorded), never a verdict against a sequentially measured reference."""
    ctx.reset()
    t_flood0 = time.time()
    ref = ref_terminal(ctx)
    fl, rs, okf, mode, fallback, blocks = [], [], False, "sequential", None, None
    if ref and ref.switch and lat.interleave_enabled():
        seq = {"n": 0}
        oks = []

        def sampler(side, cnt):
            seq["n"] += 1       # a fresh sample file per block: ctrlc_samples indexes the trap stamps by position in the file it was given
            out, ok = ctrlc_samples(ctx, cnt, "ccf_%s_%d.txt" % ("p" if side == "phosphor" else "r", seq["n"]), budget_s=max(120, cnt * 5), flood=True)
            oks.append(ok)
            return out
        try:
            time.sleep(2.5)
            fl, rs, blocks = run_interleaved(ctx, ref, n, warm, sampler)
            mode = "interleaved"
        except RuntimeError as e:
            fl, rs, blocks = [], [], None
            fallback = "interleaving failed (%s)" % str(e)[:160]
        finally:
            close_ref(ref)
        time.sleep(1.0)
        okf = all(oks) and bool(oks)
    else:
        fallback = ("no reference terminal available%s" % (": " + ctx.ref_reason if getattr(ctx, "ref_reason", None) else "")) if not ref else \
            "this backend cannot keep both windows, or interleaving is disabled (APPCI_LAT_INTERLEAVE=0)"
        if ref:
            ref.close()
            time.sleep(1.0)
    if mode != "interleaved":
        # NOREF: Phosphor alone, so the delivery check still means something; the reason is in the row
        ctx.reset()
        fl, okf = ctrlc_samples(ctx, n + warm, "ccf.txt", flood=True)
        fl = fl[warm:]
        rs = []
        ctx.reset()
    record_interleave(r, mode, blocks, fallback)
    r.metric("ctrl_c_flood_ms", stats(fl))
    cs = ctrl_c_app_split(ctx.app_log_lines(), t_flood0)          # #1606: where Phosphor's share of the flood Ctrl-C goes
    if cs:
        r.metric("ctrl_c_flood_app_split", cs)
        r.note("Ctrl-C under flood, app side (#1606): " + "; ".join("%s p50 %s max %s (n=%d)" % (k[:-3], v["p50"], v["max"], v["n"]) for k, v in cs.items()))
    if rs:
        r.metric("reference_ctrl_c_flood_ms", stats(rs))
        r.metric("reference", ref.name)
    if fl:
        rec = lat.evaluate("%s/ctrl_c_d/ctrl_c_flood" % ctx.label, fl, rs, float(os.environ.get("APPCI_CTRLC_MAX_MS", "20")), "overhead", plat(),
                           remeasure=None, ref_name=ref.name if ref and rs else None, floor=ctx.be.key_floor_ms, expected=n,
                           pane_shell=pane_shell_id(ctx), ref_shell=getattr(ctx, "ref_shell", None) if ref else None, mode=mode,
                           notes=["Ctrl-C while `yes` floods the terminal; the plain-sleep ctrl_c measurement is the control; " +
                                  ("reference interleaved with Phosphor in blocks of %d (A/B/A/B, same session)" % lat.interleave_block() if mode == "interleaved"
                                   else "NOREF: the reference was not interleaved (%s), so no overhead is judged" % fallback)])
        r.metrics.setdefault("lat", {})["ctrl_c_flood"] = rec
        lat_checks(r, rec, "Ctrl-C under flood")
    r.check(okf and bool(fl), "Ctrl-C under flood: fewer than %d samples stopped `yes` and ran the trap" % (n + warm))


_CC_KD_RE = re.compile(r'input_order keydown key="?[cC]"? ctrl=true\b.* t=(\d+(?:\.\d+)?)')
_CC_HOP_RE = re.compile(r"input_order hop=(ctrl_c_keydown|ctrl_c_to_event_loop|ctrl_c_received|ctrl_c_pty_write)\b.* t=(\d+(?:\.\d+)?)")
_CC_STAGES = ("keydown", "ctrl_c_keydown", "ctrl_c_to_event_loop", "ctrl_c_received", "ctrl_c_pty_write")


def ctrl_c_segments(txt, skip=0):
    """#1408: the APP's share of Ctrl-C, from its own input_order lines, per press: `keydown key=c ctrl=true` (presenter) -> hop=ctrl_c_keydown
    (TerminalView::ctrl_c) -> hop=ctrl_c_to_event_loop (PtyController) -> hop=ctrl_c_received (pty event loop) -> hop=ctrl_c_pty_write (write(2)
    returned). -> dict(n, <segment>: stats ms, keydown_to_write: stats ms). A press is closed by its pty write; a stage logged twice before the write
    keeps the first stamp; the first `skip` presses (warm-up) are dropped. Whatever the reference terminal gap is beyond keydown_to_write is outside the app (injector, X server, signal delivery)."""
    samples, cur = [], None
    for line in txt.splitlines():
        m = _CC_KD_RE.search(line)
        if m:
            cur = {"keydown": float(m.group(1))}
            continue
        m = _CC_HOP_RE.search(line)
        if not m:
            continue
        stage, t = m.group(1), float(m.group(2))
        if cur is None:
            cur = {}
        cur.setdefault(stage, t)
        if stage == "ctrl_c_pty_write":
            samples.append(cur)
            cur = None
    samples = samples[skip:]                    # the warm-up presses the end-to-end latency (lat_ms[warm:]) drops too
    out = {"n": len(samples)}
    for a, b in zip(_CC_STAGES, _CC_STAGES[1:]):
        out["%s->%s" % (a, b)] = stats([x[b] - x[a] for x in samples if a in x and b in x])
    out["keydown_to_write"] = stats([x["ctrl_c_pty_write"] - x["keydown"] for x in samples if "keydown" in x])
    return out


@case("ctrl_c_d", order=17)
def c_ctrl(ctx, r):
    ctx.rm("cd.txt", "cd2.txt")
    n, warm = ctx.count_lat(30), lat.warmup()
    cc_off = ctx.log_size()
    lat_ms, ok1 = ctrlc_samples(ctx, n + warm, "cc.txt", bail_after=3 if ctx.pane == "powershell" else None)
    lat_ms = lat_ms[warm:]
    if ctx.win and ctx.pane == "powershell":
        # #1401: the gated samples above ran with the hook timing marks OFF; the ^C split comes from a separate, ungated batch with them on,
        # and only when Ctrl-C works at all (a miss costs a 36 s wait).
        r.metric("pwsh_hook_timing_during_samples", False)
        import pwsh_timing
        cs, hook_marks, t_cc0 = {}, None, time.time()
        if lat_ms:
            hook_marks = ctx.pwsh_hook_timing(True)
        if hook_marks:
            try:
                ctrlc_samples(ctx, 5, "cc_marks.txt", budget_s=60, bail_after=2)      # 5: the first may run before the shell reads `.on`
            finally:
                ctx.pwsh_hook_timing(False)
            time.sleep(0.5)
            try:
                marks_text = open(hook_marks, errors="replace").read()
            except OSError:
                marks_text = ""
            cs = pwsh_timing.ctrl_c_segments(marks_text, ctx.app_log_all_lines(), t_cc0 * 1000.0)
        r.metric("ctrl_c_segments_ms", cs or dict(unavailable=True, why="Ctrl-C stopped nothing" if not lat_ms else "no marks"))
        if cs:
            r.note("^C split (#1401, separate ungated batch with the marks on, medians ms): ^C write -> PowerShell prompt %s, -> LineEditorReading %s; app: line editor ready %s, active %s, "
                   "next command written %s, its Preexec %s"
                   % tuple(cs.get(k, "-") for k in ("ctrl_c_to_prompt", "ctrl_c_to_reading", "line_editor_ready_after_ctrl_c",
                                                     "line_editor_active_after_ctrl_c", "command_written_after_ctrl_c", "preexec_after_ctrl_c")))
    r.metric("ctrl_c_ms", stats(lat_ms))
    if ctx.log_path and os.path.exists(ctx.log_path):
        r.metric("ctrl_c_app_segments", ctrl_c_segments(ctx.log_since(cc_off), skip=warm))     # #1408: where the app's share of each Ctrl-C goes
    r.metric("harness_floor_ms", ctx.be.key_floor_ms)
    if ctx.win:
        # #1570: whether this harness (and so the app it started) inherited "ignore Ctrl-C" from the runner. The app clears it for its shells
        # (PHOSPHOR_CONPTY_CTRL_C_RESET); with it set and the app's reset off, no PowerShell in the pane can be interrupted.
        import winapi
        ign = winapi.ctrl_c_ignored()
        r.metric("harness_ctrl_c_ignored", ign)
        r.metric("app_conpty_ctrl_c_reset", os.environ.get("PHOSPHOR_CONPTY_CTRL_C_RESET", "default(on)"))
        if ign:
            r.note("#1570: the harness process ignores Ctrl-C (inherited from the runner's process group); the app inherits it and must clear it for its shells")
    # (the delivered-count verdict is below, after the reference ran, so it can say whether the reference lost any sample line)
    # Same measurement in the reference terminal, same session (#1486). Both numbers include the input tool's spawn and the timestamp
    # command's cost, which cancel in the overhead; the owner budget (Ctrl-C <= 20 ms) is judged on that overhead.
    rs, ref = [], None
    short = ctx.opts.get("cc_short", False)
    pw_all_missed = ctx.pane == "powershell" and not lat_ms
    ps_no_control = False
    if lat_ms or pw_all_missed:
        # r18cz: when Ctrl-C stopped NOTHING in the Phosphor PowerShell pane the reference never ran, so "does conhost stop it?" stayed unanswered. Run a few
        # reference samples anyway (each miss costs a 36 s wait) and say what the reference did.
        ctx.reset()
        ref = None if short else ref_terminal(ctx)
        if ref:
            try:
                time.sleep(2.5)
                if pw_all_missed:
                    rs = ctrlc_samples(ctx, 4, "cc_ref.txt", budget_s=300)[0]
                    nref = len(ctx.cc_integrity.get("misses", {}).get("cc_ref.txt", []))
                    r.note("Phosphor stopped the sleep on 0 samples; reference %s: Ctrl-C stopped it on %d of 4 samples" % (ref.name, 4 - nref))
                    if nref >= 4:
                        ps_no_control = True
                else:
                    rs = ctrlc_samples(ctx, n + warm, "cc_ref.txt", budget_s=max(120, (n + warm) * 5))[0][warm:]
                ctx.reset()
            finally:
                ref.close()
            time.sleep(1.0)
            r.metric("reference_ctrl_c_ms", stats(rs))
            r.metric("reference", ref.name)
            if ctx.win:
                r.metric("reference_ctrl_c_flag", getattr(ctx, "ref_ctrl_c", None))
        elif not short:
            r.note("no reference: absolute only (Ctrl-C): no reference terminal available%s" % (": " + ctx.ref_reason if getattr(ctx, "ref_reason", None) else ""))
        if short:
            r.note("fix tier, agent mode (winmap.FIX_TIER): Ctrl-C delivery and line integrity only; the overhead budget is judged in terminal mode")
        rec = None if (short or not lat_ms) else lat.evaluate("%s/ctrl_c_d/ctrl_c" % ctx.label, lat_ms, rs, float(os.environ.get("APPCI_CTRLC_MAX_MS", "20")), "overhead", plat(),
                           remeasure=None if short else (lambda: ctrlc_samples(ctx, n + warm, "cc2.txt")[0][warm:]), ref_name=ref.name if ref else None, floor=ctx.be.key_floor_ms,
                           expected=n,
                           notes=["same trap/timestamp command typed in both terminals; reference measured after Phosphor in the same session; "
                                  "absolute Phosphor median %s ms includes the input-tool spawn (harness floor %s ms)" % (stats(lat_ms).get("p50"), ctx.be.key_floor_ms)])
        if rec:
            r.metrics.setdefault("lat", {})["ctrl_c"] = rec
            lat_checks(r, rec, "Ctrl-C")
    if lat_ms and not short and not ctx.win and ctx.pane != "powershell":
        ctrlc_flood(ctx, r, n, warm)
    why = ""
    ig, igr = ctx.cc_integrity.get("cc.txt"), ctx.cc_integrity.get("cc_ref.txt")
    if ig:
        why = " -- Phosphor: %d/%d sample lines reached the shell NOT exactly as typed%s" % (ig["bad"], ig["n"], ("; e.g. " + ig["examples"][0]) if ig["examples"] else "")
        if igr:
            why += "; reference: %d/%d" % (igr["bad"], igr["n"])
            if ig["bad"] and not igr["bad"]:
                why += " (the reference kept every line: the corruption is Phosphor-side, the same character loss/move as fast_typing)"
        r.metric("ctrl_c_line_integrity", dict(phosphor=ig, reference=igr))
    mp = ctx.cc_integrity.get("misses", {})
    if mp.get("cc.txt"):
        why += " -- PowerShell: Ctrl-C did not stop the sleep (no stamp within 6 s, or one that predates the key) on %d of %d samples (indexes %s)" % (len(mp["cc.txt"]), n + warm, mp["cc.txt"][:12])
    if mp.get("cc_ref.txt"):
        why += "; reference: %d misses" % len(mp["cc_ref.txt"])
    if ps_no_control:
        # r18d2 (also r18cz): the injected Ctrl-C stopped the sleep in NEITHER Phosphor nor the conhost reference (0 of 4), and the screenshot shows each sleep ran its
        # full 30 s. A control that fails the same way is the injector / PowerShell-under-SendInput, not Phosphor: no verdict, and the reason is in the row.
        r.skip("NO-CONTROL: the injected Ctrl-C stopped the PowerShell sleep in neither Phosphor (0/%d tried) nor the conhost reference (0/4); this harness cannot judge Ctrl-C in PowerShell%s" % (
            len(ctx.cc_integrity.get("misses", {}).get("cc.txt", [])), why))
    r.check(ok1, "Ctrl-C / command-after-Ctrl-C delivered fewer than %d times%s" % (n + warm, why))
    if ctx.win:
        r.note("Ctrl-D half not run on Windows: Ctrl-D is not end-of-input in PowerShell, and under ConPTY an msys cat/bash has not been validated to see it as EOF (docs/testing.md 2.8); the Ctrl-C half above ran")
        ctx.reset()
        return
    # Ctrl-D: ends `cat` input, and exits a nested bash back to the outer shell.
    # SAFETY (#1458): a Ctrl-D that reaches the pane's OWN top-level shell exits it and closes the last window, which quits the app.
    # So a Ctrl-D is sent only after the case has PROVEN the thing it is meant to end is running (cat: its output file exists, i.e. the
    # shell executed the line; bash: SHLVL inside it is one more than outside). If a setup line was lost, fail without sending the Ctrl-D.
    ctx.be.type("cat > cd.txt", 12, enter=True)
    cat_up = ctx.until(lambda: os.path.exists(os.path.join(ctx.W, "cd.txt")), 5)
    r.check(bool(cat_up), "`cat > cd.txt` never started (its Enter was lost?): Ctrl-D NOT sent (it would hit the top-level shell)")
    if cat_up:
        ctx.be.type("hello", 12, enter=True)
        time.sleep(0.5)
        ctx.be.key("ctrl+d")
        time.sleep(1.5)
        ctx.be.type("echo done1 >> cd.txt", 12, enter=True)
        time.sleep(1)
        r.check(ctx.lines("cd.txt") == ["hello", "done1"], "Ctrl-D in cat: file %r" % ctx.lines("cd.txt"))
    ctx.be.type("echo L$SHLVL >> cd2.txt", 12, enter=True)
    ctx.until(lambda: len(ctx.lines("cd2.txt")) >= 1, 5)
    ctx.be.type("bash", 12, enter=True)
    time.sleep(2)
    ctx.be.type("echo L$SHLVL >> cd2.txt", 12, enter=True)
    ctx.until(lambda: len(ctx.lines("cd2.txt")) >= 2, 5)
    l = ctx.lines("cd2.txt")
    nested = len(l) >= 2 and l[0].startswith("L") and l[1].startswith("L") and l[0][1:].isdigit() and l[1][1:].isdigit() \
        and int(l[1][1:]) == int(l[0][1:]) + 1
    r.check(nested, "nested bash not entered (SHLVL lines %r): Ctrl-D NOT sent (it would exit the pane's own shell and quit the app)" % l)
    if nested:
        ctx.be.key("ctrl+d")
        time.sleep(1.5)
        ctx.be.type("echo L$SHLVL >> cd2.txt", 12, enter=True)
        ctx.until(lambda: len(ctx.lines("cd2.txt")) >= 3, 5)
        l = ctx.lines("cd2.txt")
        ok = len(l) == 3 and l[0] == l[2]
        r.check(ok, "Ctrl-D leaves the nested shell: SHLVL lines %r" % l)
    ctx.reset()


@case("read_x", order=18)
def c_read(ctx, r):
    ctx.rm("rd.txt")
    tok = ctx.tok()
    ctx.be.type("read x", 10, enter=True)
    time.sleep(0.8)
    ctx.be.type("%s with  spaces ; $HOME 'q'" % tok, 12, enter=True)
    time.sleep(0.8)
    ctx.be.type("printf '%s\\n' \"$x\" >> rd.txt", 12, enter=True)
    ctx.until(lambda: ctx.lines("rd.txt"), 8)
    r.check(ctx.lines("rd.txt") == ["%s with  spaces ; $HOME 'q'" % tok], "read x received %r" % ctx.lines("rd.txt"))


@case("subshells", order=20)
def c_sub(ctx, r):
    ran = 0
    for sh, gap in (("bash", 1.5), ("bash", 0.25), ("zsh", 2.0), ("fish", 2.0), ("nu", 2.5)):
        if not shutil.which(sh):
            r.note("%s not installed: skipped" % sh)
            continue
        ran += 1
        f = "sub_%s_%d.txt" % (sh, int(gap * 100))
        ctx.rm(f)
        tok = ctx.tok(6)
        ctx.be.type(sh, 12, enter=True)
        time.sleep(gap)
        ctx.be.type(ctx.append_cmd(sh, tok, f), 12, enter=True)
        time.sleep(0.8)
        ctx.be.type("sh -c 'ps -o comm= -p $PPID' >> " + f, 12, enter=True)
        ctx.until(lambda: len(ctx.lines(f)) >= 2, 6)
        ctx.be.type("exit", 12, enter=True)
        time.sleep(2.0)
        ctx.be.type(ctx.append_cmd("bash", "back_" + tok, f), 12, enter=True)
        ctx.until(lambda: len(ctx.lines(f)) >= 3, 6)
        l = ctx.lines(f)
        r.check(len(l) == 3 and l[0] == tok and l[2] == "back_" + tok and l[1].startswith(sh[:3]),
                "%s subshell (setup gap %.2fs): lines %r" % (sh, gap, l))
        ctx.reset()
    if not ran:
        r.skip("no subshell binaries installed")


@case("alt_screen", order=21)
def c_alt(ctx, r):
    ctx.rm("al.txt")
    if shutil.which("less"):
        ctx.be.type("seq 1 500 | less", 12, enter=True)
        time.sleep(1.2)
        ctx.be.type(" ", 20)
        ctx.be.type("q", 20)
        time.sleep(0.8)
    if shutil.which("vim"):
        ctx.be.type("vim -u NONE al_edit.txt", 12, enter=True)
        time.sleep(1.5)
        ctx.be.type("ihello", 15)
        ctx.be.key("Escape")
        ctx.be.type(":wq", 15, enter=True)
        time.sleep(1.0)
    ctx.be.type("echo AFTER_ALT >> al.txt", 12, enter=True)
    ctx.until(lambda: ctx.lines("al.txt"), 6)
    r.check(ctx.lines("al.txt") == ["AFTER_ALT"], "shell usable after less/vim: %r" % ctx.lines("al.txt"))
    if shutil.which("vim"):
        r.check(ctx.lines("al_edit.txt") == ["hello"], "vim received typed text: %r" % ctx.lines("al_edit.txt"))


# ---------------------------------------------------------------- ssh (local server)
SSH_GAPS = (0.0, 0.1, 0.2, 0.3)


def ssh_trial_plan(gaps, per_gap, total=0):
    """#1127: -> [(gap, trials)]. `total` (--ssh-trials / APPCI_SSH_TOTAL_TRIALS) spreads that many trials over the gaps (the first `total % len` gaps get one more);
    else `per_gap` trials at every gap (APPCI_SSH_TRIALS, default 3 = 12 trials)."""
    if total and total > 0:
        base, extra = divmod(total, len(gaps))
        return [(g, base + (1 if i < extra else 0)) for i, g in enumerate(gaps)]
    return [(g, per_gap) for g in gaps]


def loss_report(lost, n, conf=0.95):
    """#1127: observed loss rate and its one-sided upper confidence bound, and how many clean trials a 1/40 rate would need to be excluded"""
    ub = lat.loss_rate_bound(lost, n, conf)
    need = lat.trials_to_exclude(1 / 40.0, conf)
    text = ("%d of %d typed-ahead commands lost (observed %.1f %%); %d %% upper confidence bound %.1f %%%s" % (
        lost, n, 100.0 * lost / n if n else 0.0, int(conf * 100), 100.0 * ub if ub is not None else float("nan"),
        "" if ub is None or ub < 1 / 40.0 else "; this n cannot exclude a 1/40 (2.5 %%) rate: needs %d clean trials" % need))
    return dict(lost=lost, n=n, rate=round(lost / n, 4) if n else None, upper_bound=round(ub, 4) if ub is not None else None, conf=conf, text=text)


def ssh_trial_note(ctx, r, verdicts, label, t0, line, r0, enters_before=1):
    """one ssh typeahead trial's verdict from the app log (ssh_trial_facts): every trial that was not exact is noted with the hop that went
    wrong; diagnostics never break the case"""
    try:
        v = ctx.ssh_trial_verdict(t0, line, r0, enters_before)
    except Exception as e:
        r.note("%s: trial verdict failed: %r" % (label, e))
        return None
    verdicts.append(v)
    if not v["exact"]:
        r.note("%s: %s" % (label, v["text"]))
    return v


def ssh_trial_summary(r, verdicts):
    """per-case tally of the trial verdicts (exact / the issue each failure names) + the compact per-trial facts as a metric"""
    if not verdicts:
        return
    tally = {}
    for v in verdicts:
        k = "exact" if v["exact"] else v["text"].split(":", 1)[0] if v["text"].startswith("#") else "other"
        tally[k] = tally.get(k, 0) + 1
    r.note("ssh trial verdicts: " + ", ".join("%s=%d" % kv for kv in sorted(tally.items())))
    r.metric("ssh_trial_verdicts", [{k: v[k] for k in ("exact", "held_keys", "held_bytes", "deferred_busy", "handovers", "finish", "refused",
                                                      "handed_back", "enter_handed_back", "app_saw_line", "missing")} for v in verdicts])


@case("ssh_connect_typing", order=30)
def c_ssh(ctx, r):
    if not ctx.ssh:
        r.skip(ctx.ssh_why or "no local ssh server (needs /usr/sbin/sshd + ssh-keygen, or python3-paramiko for fakesshd.py)")
    plan = ssh_trial_plan(SSH_GAPS, int(os.environ.get("APPCI_SSH_TRIALS", "3")), int(os.environ.get("APPCI_SSH_TOTAL_TRIALS", "0") or 0))
    ctx.rm("rl.txt")
    for f in ("rl.txt",):
        try:
            os.remove(os.path.join(ctx.prof.rhome, f))
        except FileNotFoundError:
            pass
    exp, miss, base, stop = [], 0, ctx.shell_count(), False
    verdicts = []
    for gap, trials in plan:
        for i in range(trials):
            tok = ctx.tok(6)
            exp.append(tok)
            line = "echo %s >> ~/rl.txt" % tok
            t0, r0 = time.time(), len(ctx.prof.commands(remote=True))
            ctx.be.type("ssh fake", 12, enter=True)
            time.sleep(gap)
            ctx.be.type(line, 15, enter=True)
            if not ctx.until(lambda: tok in ctx.lines("rl.txt", remote=True), 20):
                miss += 1
            ssh_trial_note(ctx, r, verdicts, "ssh_connect_typing gap %.1f s trial %d" % (gap, i + 1), t0, line, r0)
            if not ctx.leave_remote(r, "ssh_connect_typing (gap %.1f s)" % gap, "rl.txt", tok, base):
                stop = True
                break
        if stop:
            break
    got = ctx.lines("rl.txt", remote=True)
    r.metric("trials", len(exp))
    lost = sum(1 for t in exp if t not in got)
    r.metric("loss", loss_report(lost, len(exp)))
    r.note("ssh_connect_typing loss: " + loss_report(lost, len(exp))["text"])
    ssh_trial_summary(r, verdicts)
    r.check(got == exp, "remote shell received %d/%d typed-ahead commands exactly (missing %d)" % (sum(1 for t in exp if t in got), len(exp), miss))
    r.check(not ctx.local_lines("rl.txt"), "typed-ahead command ran LOCALLY: %r" % ctx.local_lines("rl.txt"))
    rem = [c for _, c in ctx.prof.commands(remote=True)]
    if rem:
        r.check(all("echo %s >> ~/rl.txt" % t in rem for t in exp), "remote rec.log lacks the exact typed command for some trials")
    ctx.reset()


@case("ssh_enter_then_immediate_command", order=30)
def c_ssh_enter_immediate(ctx, r):
    """#1415: `ssh <host>` Enter immediately followed by `echo <tok>` Enter, no pause at all. The
    second line is typed while the first Enter still waits at its gate; released into the new
    connect it must be held for the remote shell (it used to be dropped with a log line)."""
    if not ctx.ssh:
        r.skip(ctx.ssh_why or "no local ssh server (needs /usr/sbin/sshd + ssh-keygen, or python3-paramiko for fakesshd.py)")
    trials = int(os.environ.get("APPCI_SSH_TRIALS", "3"))
    try:
        os.remove(os.path.join(ctx.prof.rhome, "rh.txt"))
    except FileNotFoundError:
        pass
    ctx.rm("rh.txt")
    exp, base = [], ctx.shell_count()
    verdicts = []
    for n in range(trials):
        tok = ctx.tok(6)
        exp.append(tok)
        line = "echo %s >> ~/rh.txt" % tok
        t0, r0 = time.time(), len(ctx.prof.commands(remote=True))
        ctx.be.type("ssh fake", 12, enter=True)
        ctx.be.type(line, 12, enter=True)
        ctx.until(lambda: tok in ctx.lines("rh.txt", remote=True), 20)
        ssh_trial_note(ctx, r, verdicts, "ssh_enter_then_immediate_command trial %d" % (n + 1), t0, line, r0)
        if not ctx.leave_remote(r, "ssh_enter_then_immediate_command", "rh.txt", tok, base):
            break
    ssh_trial_summary(r, verdicts)
    got = ctx.lines("rh.txt", remote=True)
    r.check(got == exp, "remote shell received %d/%d immediately-typed commands exactly: %r" % (sum(1 for t in exp if t in got), len(exp), got))
    r.check(not ctx.local_lines("rh.txt"), "immediately-typed command ran LOCALLY: %r" % ctx.local_lines("rh.txt"))
    ctx.reset()


@case("ssh_nested_typeahead", order=30)
def c_ssh_nested(ctx, r):
    """#1421: `ssh fake` Enter, then `ssh fake` Enter and `echo <tok>` Enter typed during that first
    connect. At the hand-over the nested ssh is submitted; the echo behind it is held for the
    SECOND connect, so it runs exactly once, remotely, and never locally."""
    if not ctx.ssh:
        r.skip(ctx.ssh_why or "no local ssh server (needs /usr/sbin/sshd + ssh-keygen, or python3-paramiko for fakesshd.py)")
    try:
        os.remove(os.path.join(ctx.prof.rhome, "rn.txt"))
    except FileNotFoundError:
        pass
    ctx.rm("rn.txt")
    base = ctx.shell_count()
    tok = ctx.tok(6)
    line = "echo %s >> ~/rn.txt" % tok
    t0, r0 = time.time(), len(ctx.prof.commands(remote=True))
    ctx.be.type("ssh fake", 12, enter=True)
    ctx.be.type("ssh fake", 12, enter=True)
    ctx.be.type(line, 12, enter=True)
    ctx.until(lambda: tok in ctx.lines("rn.txt", remote=True), 30)
    time.sleep(1.0)
    verdicts = []
    ssh_trial_note(ctx, r, verdicts, "ssh_nested_typeahead", t0, line, r0, enters_before=2)
    ssh_trial_summary(r, verdicts)
    got = ctx.lines("rn.txt", remote=True)
    r.check(got == [tok], "the line typed behind a nested ssh ran exactly once remotely: %r" % got)
    r.check(not ctx.local_lines("rn.txt"), "the line typed behind a nested ssh ran LOCALLY: %r" % ctx.local_lines("rn.txt"))
    # leave both ssh levels, each `exit` only while the remote proof holds (a missing proof means no `exit`, so the local shell survives)
    if ctx.leave_remote(r, "ssh_nested_typeahead (inner)", "rn.txt", tok, base, level=True):
        ctx.leave_remote(r, "ssh_nested_typeahead (outer)", "rn.txt", tok, base)
    ctx.reset()


class HangingTcpServer:
    """A local TCP listener that ACCEPTS and never speaks (no SSH banner). `ssh` to it connects, then waits for the banner until its ConnectTimeout, then fails
    with `Connection timed out during banner exchange`: an ssh that is provably still CONNECTING for ~ConnectTimeout seconds, and then fails (r18d8: the old
    `.invalid` host could fail in tens of ms, before the first typed character, making 'ran locally' correct)."""
    def __init__(self):
        import socket
        self.srv = socket.socket()
        self.srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.srv.bind(("127.0.0.1", 0))
        self.srv.listen(4)
        self.srv.setblocking(False)
        self.port = self.srv.getsockname()[1]
        self.conns, self.accepted_at = [], []

    def poll(self):
        """accept anything pending; -> (connections accepted so far, connections still open: the peer (ssh) has not closed them)"""
        import socket
        while True:
            try:
                c, _ = self.srv.accept()
            except (BlockingIOError, OSError):
                break
            c.setblocking(False)
            self.conns.append(c)
            self.accepted_at.append(time.time())
        alive = 0
        for c in self.conns:
            try:
                if c.recv(1, socket.MSG_PEEK) != b"":
                    alive += 1          # the peer sent something: still a live conversation
            except BlockingIOError:
                alive += 1              # nothing to read and not closed: open
            except OSError:
                pass
        return len(self.conns), alive

    def close(self):
        for c in self.conns + [self.srv]:
            try:
                c.close()
            except OSError:
                pass


SSH_GATE_RX = re.compile(r"hop=kh_hold_(?:entry|exit)\b|hop=kh_iq_(?:hold|release)\b.*Q7_ssh_typeahead|ssh hold:")


@case("ssh_failing_not_local", order=31, gui=True)
def c_ssh_fail(ctx, r):
    """#983/#1292: text typed while an ssh is CONNECTING and then FAILS is never run locally (the app hands it back unexecuted). The target is a local listener
    that accepts and never speaks, so the ssh is provably still connecting (TCP accepted, connection open) when the line is typed and fails only after its
    ConnectTimeout; the case checks that proof first (a harness failure if the ssh was not connecting), and logs the app's ssh gate state / release time next to
    the first MUSTNOT key so a pass or a fail is unambiguous."""
    ctx.rm("sf.txt")
    ct = 6
    srv = HangingTcpServer()
    try:
        t_ssh = time.time()
        ctx.be.type("ssh -p %d -o ConnectTimeout=%d -o BatchMode=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null 127.0.0.1" % (srv.port, ct), 12, enter=True)
        ok = ctx.until(lambda: srv.poll()[0] >= 1, 8)
        if not ok:
            r.check(False, "HARNESS: the ssh never connected to the hanging listener on 127.0.0.1:%d within 8 s (no TCP accept): not a test of typing behind a connecting ssh" % srv.port)
            ctx.reset()
            return
        time.sleep(0.3)
        n, alive = srv.poll()
        t_type = time.time()
        connecting = alive >= 1
        ctx.be.type(ctx.append_cmd(ctx.pane_shell(), "MUSTNOT", "sf.txt"), 15, enter=True)
        t_typed = time.time()
        n2, alive2 = srv.poll()
        r.note("ssh_failing_not_local: ssh started %.2f s before the first MUSTNOT key; the hanging listener had accepted %d connection(s), %d still open when typing began and %d when it ended "
               "(ConnectTimeout %d s)" % (t_type - t_ssh, n, alive, alive2, ct))
        # wait for the ssh to fail (its connection closes), then give the hand-back time to land
        ctx.until(lambda: srv.poll()[1] == 0, ct + 8)
        t_failed = time.time()
        time.sleep(6)
        ev = ctx.app_hop_lines(SSH_GATE_RX, t_type - 0.5)
        first = next(((t, l) for t, l in ev if t is not None and t >= t_type and ("kh_hold_entry" in l or "kh_holding_set holding=true" in l or "gate_open kind=ssh_connect" in l)), None)      # r18e9: other hops that prove the same hold
        rel = [(t, l) for t, l in ev if t is not None and ("kh_hold_exit" in l or "kh_iq_release" in l) and t >= t_type]
        r.note("ssh gate at the first MUSTNOT key: %s; first gate release/exit after it: %s; ssh failed (connection closed) %.2f s after typing began; listener-side typing window %.2f s" % (
            ("t=+%.3fs %s" % (first[0] - t_type, first[1]) if first else "no kh_hold_entry logged (app key trace off or rotated)"),
            ("t=+%.3fs %s" % (rel[0][0] - t_type, rel[0][1]) if rel else "none logged"), t_failed - t_type, t_typed - t_type))
        if not first:
            alive_trace = bool(ctx.app_hop_lines(re.compile(r"input_order "), t_type - 0.5))
            r.unverifiable("ssh_failing_not_local: no kh_hold_entry in the app log after the first MUSTNOT key (%s; log %s): the ssh gate was not observed, so \"the line did not run locally\" "
                           "is not shown to be the app's hold; ssh started +%.2f s before the key, connection closed +%.2f s after typing began" % (
                               "input_order trace is ON (other lines present) but the gate hop is absent" if alive_trace else "NO input_order lines at all: the key trace is OFF",
                               ctx.log_path, t_type - t_ssh, t_failed - t_type))
        r.check(connecting, "HARNESS: the ssh connection was already closed when MUSTNOT was typed (not connecting): the case proves nothing; ssh gate: %s" % (first[1] if first else "n/a"))
        r.check(not ctx.lines("sf.txt"), "line typed behind a FAILING ssh ran locally: %r (ssh was connecting when typed: %s; ssh gate at first key: %s)" % (
            ctx.lines("sf.txt"), connecting, first[1] if first else "not logged"))
    finally:
        srv.close()
        ctx.reset()


# ---------------------------------------------------------------- perf
@case("idle_cpu", order=40, gui=True)
def c_idle(ctx, r):
    ctx.reset()
    time.sleep(3)
    t0, rss0 = ctx.app.cpu_ticks()
    w0 = time.time()
    time.sleep(20)
    t1, rss1 = ctx.app.cpu_ticks()
    clk = ctx.app.clk_tck() if hasattr(ctx.app, "clk_tck") else os.sysconf("SC_CLK_TCK")      # Windows: FILETIME 100 ns units
    pct_ = (t1 - t0) / clk / (time.time() - w0) * 100
    r.metric("idle_cpu_pct_of_one_core", pct_)
    r.metric("rss_mb", rss1 / 1024)
    limit = float(os.environ.get("APPCI_IDLE_CPU_MAX", "15"))
    r.check(pct_ <= limit, "idle CPU %.1f%% of one core exceeds CI limit %.0f%% (compositor/GPU path differs from a desktop: compare trends)" % (pct_, limit))


class RefTerm:
    """A reference terminal running the same scratch rc, for same-session comparison. name says how it was reached (XWayland etc.)."""
    def __init__(self, name, proc=None, close=None):
        self.name, self.proc, self._close = name, proc, close
        self.switch = None      # #1514: callable(side) with side 'phosphor'|'reference' that makes that window the injection target (focus + guard + ctl dir); None = the backend cannot keep both windows, so no interleaving

    def close(self):
        if self._close:
            self._close()
        elif self.proc:
            try:
                os.killpg(self.proc.pid, 15)
            except Exception:
                pass


def ref_note(ctx, r):
    """#1485: when Terminal.app was already running, its reference windows are left open (never quit someone's Terminal)"""
    if ctx.win:
        return
    import refterm_mac
    n = refterm_mac.left_open_note()
    if n:
        r.note(n)


def ref_terminal_windows(ctx, rcfile=None):
    """Windows: the SAME shell as the pane in a plain console window (conhost: `powershell.exe -NoLogo -NoProfile`, as the pane, or Git Bash `--rcfile` for the Git Bash
    pane), driven through the same SendInput path under a guard that accepts only THAT window (its PID or its hwnd), never the job's own console.
    APPCI_WIN_REF=wt uses Windows Terminal (wt.exe, when present) instead of conhost. None (with ctx.ref_reason) when the window cannot be identified.
    rcfile: the Git Bash reference's rc instead of the pane's ~/.bashrc (#1583 G2: prof.ref_boot_rc, Phosphor's bash bootstrap sourced)."""
    import winapi
    ctx.ref_reason = None
    want_wt = os.environ.get("APPCI_WIN_REF", "conhost").lower() == "wt"
    home = ctx.prof.home
    env = dict(ctx.app.env()) if hasattr(ctx.app, "env") else dict(os.environ)
    env["HOME"] = home
    env.pop("PHOSPHOR_PWSH_HOOK_TIMING", None)          # the app's pane only (#1401)
    if ctx.pane == "gitbash":
        sh, shname = [winapi.git_bash(), "--rcfile", rcfile or (home + "/.bashrc"), "-i"], "bash"
    else:
        # r18e9: the reference conhost PowerShell shared PSReadLine's ConsoleHost_history.txt with the app's pane, so its own `exit` (+ a leading Ctrl-U) landed in
        # the pane's history slice and took the first slot of fast_typing. Give the reference its OWN HistorySavePath (and SaveNothing where that cannot be set).
        ref_hist = os.path.join(ctx.W, ".appci_ref_ps_history.txt").replace("'", "''")
        ctx.ref_ps_history = ref_hist
        pre = "try{Set-PSReadLineOption -HistorySavePath '%s'}catch{try{Set-PSReadLineOption -HistorySaveStyle SaveNothing}catch{}}" % ref_hist
        sh, shname = ["powershell.exe", "-NoLogo", "-NoProfile", "-NoExit", "-Command", pre], "powershell.exe"      # -NoProfile like the pane (#1514: the control must be symmetric)
    if want_wt:
        wt_exe = shutil.which("wt.exe") or shutil.which("wt")
        if not wt_exe:
            ctx.ref_reason = "APPCI_WIN_REF=wt but wt.exe (Windows Terminal) is not installed"
            return None
        argv, name, flags = [wt_exe, "-d", ctx.W, "new-tab"] + sh, "Windows Terminal (%s)" % shname, 0
    else:
        argv, name, flags = sh, "conhost (%s)" % shname, 0x00000010           # CREATE_NEW_CONSOLE
    before = {w[0] for w in winapi.top_windows(None)}
    # #1570: a process started in a new process group (this job, by the runner) ignores Ctrl-C and every child inherits that, so the conhost reference
    # could not be interrupted either (r18cz, r18e4: NO-CONTROL). A real user's conhost receives Ctrl-C: start the reference with the flag cleared
    # (restored right after), so the control measures conhost, not the runner. The app clears it for its own shells (PHOSPHOR_CONPTY_CTRL_C_RESET).
    # APPCI_WIN_REF_CTRL_C=inherit keeps the inherited flag (the old behaviour).
    ignored = winapi.ctrl_c_ignored()
    ctx.harness_ctrl_c_ignored = ignored
    clear = bool(ignored) and os.environ.get("APPCI_WIN_REF_CTRL_C", "enable").lower() != "inherit"
    if clear:
        winapi.set_ctrl_c_ignored(False)
    try:
        proc = subprocess.Popen(argv, cwd=ctx.W, env=env, creationflags=flags, stdin=subprocess.DEVNULL if want_wt else None)
    finally:
        if clear:
            winapi.set_ctrl_c_ignored(True)
    ctx.ref_ctrl_c = "cleared" if clear else ("inherited-ignored" if ignored else ("not-ignored" if ignored is False else "unknown"))
    if want_wt and clear and any(winapi.proc_name(w[1]) == "windowsterminal.exe" for w in winapi.top_windows(None) if w[0] in before):
        # wt.exe hands the tab to an already running Windows Terminal, whose process (and the shell it starts) never saw the cleared flag
        ctx.ref_ctrl_c = "unknown (wt reused)"
    ctx.ref_proc = proc
    ctx.ref_shell = shname
    hwnd, wpid = None, None
    for _ in range(60):
        time.sleep(0.4)
        ws = [w for w in winapi.top_windows(None) if w[0] not in before and w[2][2] > 100 and w[2][3] > 50]
        mine = [w for w in ws if w[1] == proc.pid] or [w for w in ws if winapi.proc_name(w[1]) in ("conhost.exe", "openconsole.exe", "windowsterminal.exe", "powershell.exe", "bash.exe")]
        if mine:
            hwnd, wpid = mine[-1][0], mine[-1][1]
            break
    def kill():
        subprocess.run(["taskkill", "/PID", str(proc.pid), "/T", "/F"], capture_output=True, timeout=30)
    if not hwnd:
        ctx.ref_reason = "the reference console window (%s) never appeared" % name
        kill()
        return None
    time.sleep(4)          # the shell prints its banner / loads its rc
    winapi.focus_window(hwnd)
    time.sleep(0.5)
    ctx.be.guard_override = dict(pids={proc.pid, wpid}, hwnds={hwnd})

    def close():
        try:
            ctx.be.type("exit", 12, enter=True)
        except Exception:
            pass
        time.sleep(1.0)
        ctx.be.guard_override = None
        if proc.poll() is None:
            kill()
        ctx.be.refocus()
    return RefTerm(name, close=close)


def pane_shell(ctx):
    """The shell program running in Phosphor's pane (e.g. '/bin/zsh'), asked of the pane once per session: `ps -o comm= -p $$`
    typed into it. None if it cannot be learned (the caller falls back). Cached on ctx."""
    if getattr(ctx, "_pane_shell", None) is not None:
        return ctx._pane_shell or None
    f = os.path.join(ctx.W, ".appci_shell")
    try:
        if os.path.exists(f):
            os.remove(f)
        ctx.be.guard()
        ctx.be.type("ps -o comm= -p $$ > .appci_shell", 12, enter=True)
        ctx.until(lambda: os.path.exists(f) and os.path.getsize(f) > 1, 5)
        name = open(f, errors="replace").read().strip().lstrip("-")
    except Exception:
        name = ""
    path = {"zsh": "/bin/zsh", "bash": "/bin/bash"}.get(os.path.basename(name), name if name.startswith("/") else "")
    ctx._pane_shell = path
    return path or None


def pane_shell_id(ctx):
    """The shell in Phosphor's pane for the verdict's shell-identity check: Windows by pane kind, elsewhere asked of the pane (pane_shell). None = unknown."""
    if ctx.win:
        return {"powershell": "powershell", "gitbash": "bash"}.get(ctx.pane)
    try:
        return pane_shell(ctx)
    except Exception:
        return None


def x11_dir_problem():
    """#1567: ' (/tmp/.X11-unix is owned by uid N, not root: XWayland refuses to start; the owner must `sudo chown root: /tmp/.X11-unix && sudo chmod 1777 /tmp/.X11-unix`, or install foot)'
    when that directory has the wrong owner, else ''. Never changes the box."""
    try:
        st = os.stat("/tmp/.X11-unix")
    except OSError:
        return ""
    if st.st_uid in (0, os.getuid()):
        return ""
    return " (/tmp/.X11-unix is owned by uid %d, not root: XWayland refuses to start; the owner must chown it to root with mode 1777, or install foot)" % st.st_uid


def ref_terminal(ctx):
    """Linux: foot (Wayland) when installed, else xterm (via XWayland under the Wayland session, native under X11).
    macOS: Terminal.app, driven by the same cliclick/osascript path as Phosphor. None when there is no reference."""
    home = ctx.prof.home
    ctx.ref_shell = None        # set by whichever path launches a reference: the shell it runs (recorded in the latency metrics, #1583)
    if ctx.win:
        return ref_terminal_windows(ctx)
    if ctx.be.name == "mac":
        # NO Apple Events: `tell application "Terminal" ... do script` needs an Automation grant (a TCC prompt nobody answers in CI; it hung
        # 30 s per sample in run 3015802940). A .command file opened with LaunchServices needs none (and a .terminal document would import
        # a profile into com.apple.Terminal's prefs: not allowed). Driven by htype2 under the frontmost guard. Windows are cleaned up by
        # quitting a Terminal.app this run launched, by exact PID (#1485, refterm_mac).
        import refterm_mac
        ctx.ref_reason = None
        refterm_mac.snapshot(ctx.run)   # normally already done at harness start; idempotent
        cmdf = os.path.join(ctx.prof.home, "ref_terminal.command")
        with open(cmdf, "w") as f:
            # The SAME shell as Phosphor's pane, asked of the pane itself (r18cz: I assumed bash from $SHELL, but the app's log shows the pane
            # ran /bin/zsh; guessing either way can compare two different shells). Unknown -> zsh, as before 736a0f9e0.
            login = pane_shell(ctx) or "/bin/zsh"
            if os.path.basename(login) == "bash":
                # its OWN rc + ctl dir (#1550, as Linux #1500): the reference logs to refctl/rec.log, never the app pane's ctl/rec.log
                f.write("#!/bin/zsh\ncd '%s'\nexec env HOME='%s' PA_CTL='%s' '%s' --rcfile '%s' -i\n" % (ctx.W, home, ctx.prof.refctl, login, ctx.prof.ref_rc))
            else:
                f.write("#!/bin/zsh\ncd '%s'\nexec env HOME='%s' ZDOTDIR='%s' PA_CTL='%s' zsh -i\n" % (ctx.W, home, ctx.prof.ref_zdot, ctx.prof.refctl))
            ctx.ref_shell = login
        os.chmod(cmdf, 0o755)
        subprocess.run(["xattr", "-c", cmdf], capture_output=True)
        rr = subprocess.run(["open", "-a", "Terminal", cmdf], capture_output=True, timeout=20)
        if rr.returncode != 0:
            ctx.ref_reason = "open -a Terminal failed: %s" % rr.stderr.decode().strip()[:120]
            return None
        refterm_mac.note_launched(ctx.run)
        for _ in range(16):             # <= 8 s for Terminal to become frontmost
            if ctx.be._front()[0].lower() == "terminal":
                break
            time.sleep(0.5)
        else:
            ctx.ref_reason = "Terminal.app did not become frontmost within 8 s"
            return None
        d = ctx.be.system_dialog()
        if d:
            raise ctx.be.needs_owner(d)
        time.sleep(1.5)
        ctx.be.guard_override = "Terminal"
        ctx.prof.ref_active = True          # commands()/read() now read refctl (#1550)

        def close():
            ctx.prof.ref_active = False
            # never Cmd-W/Q: `exit` ends the shell; the window itself goes when a Terminal.app this run launched is quit by exact PID at
            # the end of the run (refterm_mac), which also removes a shell whose exit was lost
            try:
                ctx.be.type("exit", 12, enter=True)
            except Exception:
                pass
            time.sleep(0.8)
            ctx.be.guard_override = None
            ctx.be.refocus()
        rt = RefTerm("Terminal.app (.command via open)", close=close)

        def switch(side):
            if side == "reference":
                ctx.be.guard_override = "Terminal"
                ctx.prof.ref_active = True
                ctx.be._focus_pid(pid_of("Terminal"))
            else:
                ctx.be.guard_override = None
                ctx.prof.ref_active = False
                ctx.be.refocus(verify=True)
            ctx.be.guard()          # frontmost process verified for the side now targeted; raises RuntimeError otherwise
        rt.switch = switch
        return rt
    disp, wl = os.environ.get("DISPLAY"), os.environ.get("WAYLAND_DISPLAY")
    # r18d2: the reference shell starts in the case work dir like the pane does (the rc cds to $APPCI_W). Without it foot/xterm inherited the harness's cwd, so every
    # effect file the reference shell wrote (el_ref*.txt, lat_N, cc_ref.txt) landed OUTSIDE ctx.W where the case reads: enter_loss_control saw 30/30 lines
    # "missing" while the shell's own rec.log had all 30 exact (and the same misplacement would starve the reference latency samples).
    env = dict(os.environ, HOME=home, APPCI_W=ctx.W)
    # its OWN rc and ctl dir (#1500): the reference logs to refctl/rec.log, never the app pane's ctl/rec.log, so a burst that reached the wrong
    # window can never be read back as reference data
    rc = ctx.prof.ref_rc
    if wl and shutil.which("foot") and ctx.be.name == "wayland":
        argv, name, names = ["foot", "-e", "bash", "--rcfile", rc, "-i"], "foot", {"foot"}
    elif shutil.which("xterm") and disp:
        argv = ["xterm", "-geometry", "120x30+0+0", "-e", "bash", "--rcfile", rc, "-i"]
        name = "xterm (via XWayland)" if wl and ctx.be.name == "wayland" else "xterm"
        names = {"xterm"}
    else:
        # #1567: say WHY (the box's /tmp/.X11-unix not being root-owned stops XWayland, so no DISPLAY and no foot = no reference)
        ctx.ref_reason = ("no reference terminal: no foot and %s%s" % ("no xterm" if not shutil.which("xterm") else "no DISPLAY (XWayland did not start)", x11_dir_problem()))
        return None
    proc = subprocess.Popen(argv, env=env, cwd=ctx.W, start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    ctx.ref_shell = "bash"
    ctx.prof.ref_active = True
    ctx.be.guard_override = dict(pids={proc.pid}, names=names)

    def close():
        ctx.be.guard_override = None
        ctx.prof.ref_active = False
        try:
            os.killpg(proc.pid, 15)
        except Exception:
            pass
    try:
        ctx.be.guard()          # refuse up front when the reference never took the focus
    except RuntimeError as e:
        close()
        ctx.ref_reason = str(e)
        return None
    rt = RefTerm(name, proc=proc, close=close)
    override = dict(pids={proc.pid}, names=names)

    def switch(side):
        if side == "reference":
            ctx.be.guard_override = override
            ctx.prof.ref_active = True
        else:
            ctx.be.guard_override = None
            ctx.prof.ref_active = False
            ctx.be.refocus(verify=True)     # sway: focus Phosphor by PID and wait for it; X11: no-op, its guard() focuses Phosphor
        ctx.be.guard()                      # reference: focus + verify its window; Phosphor on X11: focus its window
    rt.switch = switch
    return rt


LAT_SETTLE_S = 0.4


def run_interleaved(ctx, ref, n, warm, sampler, block=None, lead=None):
    """#1514: A/B/A/B interleaving of Phosphor and the reference for a latency case, in blocks (lat.block_schedule), never per sample. Before every block
    ref.switch(side) moves the focus and runs the backend guard (the frontmost-window check) for THAT side; sampler(side, count) -> list of samples
    for that side. -> (phosphor samples, reference samples, [(side, kept samples)]). RuntimeError (a guard or focus failure) propagates: the caller
    discards the partial data and falls back to the sequential mode, recording why."""
    block = block or lat.interleave_block()
    lead = lat.interleave_lead() if lead is None else lead
    out = {"phosphor": [], "reference": []}
    blocks = []
    for side, cnt, disc in lat.block_schedule(n, block, warm, lead):
        ref.switch(side)
        time.sleep(LAT_SETTLE_S)
        kept = sampler(side, cnt)[disc:]
        out[side] += kept
        blocks.append((side, kept))
        ctx.reset()
    return out["phosphor"], out["reference"], blocks


def record_interleave(r, mode, blocks=None, fallback=None):
    """metrics for how a latency case was measured: mode ('interleaved' | 'sequential'), the block size, per-block medians, and why it fell back"""
    r.metric("measurement_mode", mode)
    if fallback:
        r.metric("interleave_fallback", fallback)
        r.note("measurement mode: sequential (%s)" % fallback)
    if blocks:
        r.metric("interleave_block", lat.interleave_block())
        r.metric("block_medians_ms", [[side, round(statistics.median(xs), 2)] for side, xs in blocks if xs])
        r.note("measurement mode: interleaved, blocks of %d kept samples (+%d discarded lead per block), %d blocks" % (lat.interleave_block(), lat.interleave_lead(), len(blocks)))


def close_ref(ref):
    """close an interleaved reference: aim the guard at it first (mac types `exit` into it under the guard), tolerate a failed switch"""
    if ref.switch:
        try:
            ref.switch("reference")
        except RuntimeError:
            pass
    ref.close()


@case("key_loss_control", modes=("terminal",), order=3, gui=True)
def c_key_loss(ctx, r):
    """#1414: does the FIRST character of a wtype-injected line reach the client? The same injection sequences go into Phosphor AND a
    reference terminal in the same session: per-line calls (the harness default), per-line with the harness priming off, and ONE
    call for all lines. A Phosphor variant fails only when it loses first characters (or lines) the reference keeps with the
    SAME injection. The loss rates land in results.json (metrics.key_loss) and summary.md. Only the Wayland backend injects with a
    per-call keymap; other backends run it as a plain typing check."""
    n = ctx.count(12)
    is_wl = ctx.be.name == "wayland"
    prime_default = getattr(ctx.be, "prime_ms", 0)

    def variant(target, inject):
        if inject == "perline-unprimed":
            ctx.be.prime_ms = 0
        elif is_wl:
            ctx.be.prime_ms = prime_default
        fname = "kl_%s_%s.txt" % (target[:3], inject.replace("-", ""))
        ctx.rm(fname)
        mark = ctx.cmd_mark()
        want = []
        for _ in range(n):
            want.append("echo %s >> %s" % (ctx.tok(10).lower(), fname))
        try:
            ctx.be.guard_ref()
            if inject.startswith("oneshot"):
                ctx.be.type_lines(want, 12, 350)
            else:
                for l in want:
                    ctx.be.guard_ref()
                    ctx.be.type(l, 12, enter=True)
                    time.sleep(0.35)
        finally:
            if is_wl:
                ctx.be.prime_ms = prime_default
        ctx.until(lambda: len(ctx.cmds_since(mark)) >= n, 15 + n * 0.5)
        time.sleep(0.5)
        have = ctx.cmds_since(mark)
        lost = bad = 0
        for w, h in zip(want, have):
            if h == w:
                continue
            if h.endswith(w[1:]) and not h.endswith(w):
                lost += 1
            else:
                bad += 1
        missing = max(0, n - len(have))
        v = dict(target=target, inject=inject, n=n, lost_first=lost, other_bad=bad, missing=missing)
        if lost:
            v["example"] = next((w, h) for w, h in zip(want, have) if h != w and h.endswith(w[1:]))
        return v

    injections = ["perline-unprimed", "perline", "oneshot"] if is_wl else ["perline"]
    out = []
    for inj in injections:
        out.append(variant("phosphor", inj))
        ctx.reset()
    ref = ref_terminal(ctx)
    if ref:
        ref_note(ctx, r)
    if ref:
        try:
            time.sleep(2.5)
            for inj in injections:
                out.append(variant("reference", inj))
                ctx.reset()
        finally:
            ref.close()
        time.sleep(1.0)
    else:
        r.note("no reference terminal available: loss is reported but no variant can be attributed to Phosphor or the injector")
    inj_mode = getattr(ctx.be, "injector_mode", None)
    r.metric("key_loss", dict(variants=out, reference=ref.name if ref else None, injector_mode=inj_mode))
    r.note("injector mode: %s (persistent = one session-long virtual keyboard, so 'unprimed' measures Phosphor itself; percall = a keyboard per wtype call, the #1414 artifact)" % inj_mode)
    byk = {(v["target"], v["inject"]): v for v in out}
    for v in out:
        r.note("%s / %s: first char lost on %d/%d lines, other bad %d, missing %d" % (v["target"], v["inject"], v["lost_first"], v["n"], v["other_bad"], v["missing"]))
    for inj in injections:
        p, q = byk.get(("phosphor", inj)), byk.get(("reference", inj))
        bad_p = p["lost_first"] + p["other_bad"] + p["missing"]
        if q is None:
            continue
        bad_q = q["lost_first"] + q["other_bad"] + q["missing"]
        r.check(q["missing"] < q["n"], "the reference terminal's own log recorded NONE of the %d lines typed at it (%s): the keys did not reach it, so it is no control (#1500)" % (q["n"], inj))
        r.check(bad_p <= bad_q, "Phosphor lost keys the reference keeps with the SAME injection (%s): Phosphor %d/%d bad lines (first char lost %d), %s %d/%d -- a Phosphor-side drop (#1414)"
                % (inj, bad_p, n, p["lost_first"], ref.name, bad_q, n))
        if bad_q > bad_p:
            r.note("%s: the reference lost more than Phosphor (%d vs %d): injector-side loss" % (inj, bad_q, bad_p))
    # the harness's own default (primed, per-line) must deliver every line, whatever the reference does: otherwise every other case is unreliable
    d = byk.get(("phosphor", "perline"))
    if d:
        r.check(d["lost_first"] + d["other_bad"] + d["missing"] == 0, "harness default injection (per-line%s) still loses lines in Phosphor: %s"
                % (", primed" if is_wl and prime_default else "", {k: d[k] for k in ("lost_first", "other_bad", "missing")}))


def run_enter_control(ctx, r, n):
    """#1482: the fast_typing burst (same cadence 10-25 ms/key, instant Enter, same gaps) typed into the REFERENCE terminal's shell in this
    session. Returns {gap: dict(ref, n, bad, posted)} (bad = lines of the burst missing from the file, i.e. merged or lost; posted =
    ENTERs the injector reports posting, None when it keeps no record), or None when there is no reference terminal. Cached on ctx."""
    if ctx.enter_control is not None:
        return ctx.enter_control or None
    ref = ref_terminal(ctx)
    if ref:
        ref_note(ctx, r)
    if not ref:
        ctx.enter_control = {}
        r.note("enter control: no reference terminal (%s): a lost Enter in Phosphor cannot be attributed" % (getattr(ctx, "ref_reason", None) or "unavailable"))
        return None
    out = {}
    try:
        time.sleep(2.5)
        for gap in (0.0, 0.25):
            fname = "el_ref%d.txt" % int(gap * 100)
            ctx.rm(fname)
            k0 = len(ctx.be.calls) if ctx.be.calls is not None else None
            mark = ctx.cmd_mark()
            exp = []
            for _ in range(n):
                tok = ctx.tok(10).lower()
                exp.append(tok)
                ctx.be.guard_ref()
                ctx.be.type(ctx.append_cmd(ctx.pane_shell(), tok, fname), ctx.rng.randint(10, 25), enter=True)
                if gap:
                    time.sleep(gap)
            ctx.until(lambda: len(ctx.lines(fname)) >= n, 20 + n * 0.5)
            time.sleep(0.5)
            got = set(ctx.lines(fname))
            bad = sum(1 for t in exp if t not in got)
            # the same burst, judged on the EXACT command line the reference shell recorded (its rc writes the same rec.log): a character moved or
            # lost inside a line does not always remove the line from the file, so the file check alone under-counts what the reference kept
            exact_bad = None
            if ctx.prof.commands():
                want = [ctx.append_cmd(ctx.pane_shell(), t, fname) for t in exp]
                have = ctx.cmds_since(mark)
                exact_bad = sum(1 for w, h in zip(want, have) if w != h) + abs(len(want) - len(have))
            posted = None
            if k0 is not None:
                posted = sum(1 for c in ctx.be.calls[k0:k0 + n] if c["enters_posted"])
            out[gap] = dict(ref=ref.name, n=n, bad=bad, posted=posted, lines=len(ctx.lines(fname)), exact_bad=exact_bad)
            ctx.reset()
    finally:
        ref.close()
    time.sleep(1.0)
    ctx.enter_control = out
    return out


@case("enter_loss_control", modes=("terminal",), order=4, gui=True)
def c_enter_loss(ctx, r):
    """#1482: does an instant Enter after a fast-typed line survive in the REFERENCE terminal? Reports the control metric (metrics.enter_control)
    for fast_typing's verdict: its lost-Enter failure is an app bug only if this control kept every Enter in the same session. A separate
    case (not an extension of key_loss_control) because it needs the Phosphor-independent 30-line burst at fast_typing's exact cadence, and
    because key_loss_control's Wayland variants measure first-character loss, a different injector fault. Fails only when the injector itself
    reports it did not post an Enter (harness bug); reference loss is reported, not failed (it is the injector/OS, not the app)."""
    n = ctx.count(30)
    res = run_enter_control(ctx, r, n)
    if not res:
        r.note("no reference terminal available: Enter loss is not controlled")
        return
    r.metric("enter_control", dict(reference=next(iter(res.values()))["ref"], runs={str(g): v for g, v in res.items()}))
    for g, v in res.items():
        r.note("reference %s gap %.2fs: %d/%d lines missing (merged/lost Enter), %s lines not received exactly, injector posted %s Enters" % (
            v["ref"], g, v["bad"], v["n"], v.get("exact_bad"), v["posted"]))
        if v["posted"] is not None:
            r.check(v["posted"] == v["n"], "injector posted only %d of %d Enters in the control (gap %.2fs): harness/injector fault" % (v["posted"], v["n"], g))
    if any(v["bad"] or v.get("exact_bad") for v in res.values()):
        r.note("the injector/OS loses instant Enters even in the reference: fast_typing reports such a loss as 'injector/OS lost Enter (reference too)', not an app bug")


def stamp_line(ctx):
    """The timestamp command typed in BOTH terminals (a '%d' for the sample index). macOS: BSD date has no %N. When the pane shell is zsh (r18cz: it
    was, not bash 3.2) the stamp is the shell's own builtin, `$EPOCHREALTIME` (no fork/exec, no 50 ms perl start-up inside the measurement; the
    `zmodload` is a no-op after the first line and is typed before the stamp, so it is outside the stamped interval's tail; it also keeps the
    line structured so agent-mode autodetect does not send it to the AI). Only bash 3.2 (no $EPOCHREALTIME) keeps perl, which is in the base
    system. The reference terminal runs the same shell (pane_shell) and so the same line. Linux: `date +%s.%N`. PowerShell: UTC ms."""
    if ctx.pane == "powershell":
        return "[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() | Out-File lat_%d -Encoding ascii"
    if ctx.be.name == "mac":
        if lat.shell_base(pane_shell(ctx)) == "zsh":
            # #1606: the rc's preexec / zle-line-finish times follow the stamp (enter_to_start reads the first field as the stamp)
            return "zmodload zsh/datetime; print -r -- $EPOCHREALTIME ${__pa_px:-0} ${__pa_lf:-0} > lat_%d"
        return "perl -MTime::HiRes=time -e 'printf \"%%.6f\\n\", time' > lat_%d"
    # #1492, OPT-IN (APPCI_ENTER_SPLIT=1): in a bash/zsh pane (Git Bash, or a Linux pane already identified by pane_shell) the rc's
    # line-accepted time ($__pa_lf: bash PS0, in integer microseconds; zsh zle-line-finish, in seconds) is appended after the stamp. This is
    # NOT free: the line is 23 characters longer, and Phosphor writes the whole line in one burst at Enter, so the shell's ingest of those
    # characters falls INSIDE Phosphor's Enter -> start interval (the reference ingested them key by key before Enter). Samples taken with
    # it are therefore not comparable with the default line or older baselines; the default stays the old line.
    sh = "bash" if ctx.pane == "gitbash" else lat.shell_base(getattr(ctx, "_pane_shell", None) or None)
    if sh in ("bash", "zsh") and os.environ.get("APPCI_ENTER_SPLIT", "0") == "1":
        return "date +%%s.%%N > lat_%d;echo $__pa_lf>>lat_%d"
    return "date +%%s.%%N > lat_%d"


def enter_to_start(ctx, n, budget_s=None, key_delay_ms=12, stamp=None, d=None, split=None):
    """stamp/d (#1532 latency_enter_ssh): the stamp line and the directory its lat_<i> files land in, when they are not the local pane's
    (stamp_line(ctx), ctx.W): over ssh the REMOTE shell writes them into the remote home.
    split (#1606): a list that receives one dict per captured sample, epoch seconds: t0 (harness, before the injector starts), post (the
    injector's own post time of the Return, macOS htype2 only), stamp, and from the zsh rc lf (zle-line-finish) and px (last preexec hook)."""
    ds = []
    t_end = time.time() + budget_s if budget_s else None
    for i in range(n):
        if t_end and time.time() > t_end:
            break
        f = os.path.join(d or ctx.W, "lat_%d" % i)
        if os.path.exists(f):
            os.remove(f)
        stamp = stamp or stamp_line(ctx)
        ctx.be.guard_ref()
        ctx.be.type(stamp % ((i,) * stamp.count("%d")), key_delay_ms)       # #1492: the bash/zsh line names its file twice
        time.sleep(0.3)
        t0 = time.time()
        ctx.be.key("Return")
        ctx.until(lambda: os.path.exists(f) and os.path.getsize(f) > 5, 3, step=0.001)
        if "__pa_lf>>" in stamp:
            # #1492 split line: `date` and the `echo` append are separate writes (and Git Bash reaps `date` slowly), so wait briefly for the
            # second field; a sample without it keeps its stamp and is counted (ctx.lf_missing -> metric lf_missing), never silently dropped
            if not ctx.until(lambda: len(_read_fields(f)) >= 2, 1.0, step=0.002):
                ctx.lf_missing = getattr(ctx, "lf_missing", 0) + 1
        try:
            fields = _read_fields(f)
            v = float(fields[0])
            ds.append(((v / 1000.0 if v > 1e11 else v) - t0) * 1000)       # PowerShell stamps UTC milliseconds
            if split is not None:
                # zsh (macOS): stamp px lf; bash/zsh stamp_line (#1492): stamp lf
                px, lf = (None, _float_or_none(fields, 1)) if len(fields) == 2 else (_float_or_none(fields, 1), _float_or_none(fields, 2))
                split.append(dict(t0=t0, stamp=v / 1000.0 if v > 1e11 else v, post=injector_post_wall(ctx), px=_epoch_s(px), lf=_epoch_s(lf)))
        except Exception:
            pass
        if i == 2 and not ds:
            # three stamps in a row never ran: the line is not reaching a shell (wrong shell, wrong destination, wedged pane). Stop
            # rather than type 30 more lines into whatever took them (an agent queue, a pager); the caller reports 0/n captured.
            ctx.be.key("ctrl+c")
            break
        time.sleep(0.2)
    return ds


def _read_fields(f):
    """whitespace-separated fields of a stamp file (NULs from a UTF-16 PowerShell write removed); [] when it cannot be read yet"""
    try:
        return open(f, errors="replace").read().replace("\x00", "").split()
    except OSError:
        return []


def _float_or_none(fields, i):
    """fields[i] as a float; None when it is missing, not a number, or 0 (the stamp line's `${__pa_px:-0}` for a hook that never ran)"""
    try:
        v = float(fields[i])
    except (IndexError, ValueError):
        return None
    return v or None


def _epoch_s(v):
    """a shell time as epoch seconds: bash's PS0 time (#1492) is integer microseconds (> 1e14), zsh's $EPOCHREALTIME is already seconds"""
    if v is None:
        return None
    return v / 1e6 if v > 1e14 else v


def injector_post_wall(ctx):
    """#1606: wall time (epoch s) at which the injector itself posted the last special key of its last call (macOS htype2 K-line, converted
    with its T0 mach/epoch pair), or None (another backend, or no record)"""
    calls = getattr(ctx.be, "calls", None)
    if not calls:
        return None
    w = calls[-1].get("post_wall") or []
    return w[-1] if w else None


def _med(xs):
    xs = [x for x in xs if x is not None]
    return round(statistics.median(xs), 2) if xs else None


def enter_split_summary(samples):
    """#1606: one side's enter_to_start split dicts -> medians in ms (+ n) of
      t0_to_post        harness start -> the injector's post of Return (the input-tool spawn: the same for both sides; sanity only)
      post_to_lf        the post -> zle-line-finish: key delivery, the terminal's own work, the pty write and the line editor reading and
                        accepting the line (Phosphor writes the whole line at Enter; a plain terminal's shell already has it)
      lf_to_px          zle-line-finish -> the rc's preexec hook: the preexec hooks that run before it (Phosphor's warp_preexec, which
                        sends the Preexec message; none in the reference). Phosphor's title hook runs AFTER it and lands in px_to_stamp
      px_to_stamp       the rc's preexec hook -> the stamp: any later preexec hook (Phosphor's title hook) and the command itself
      post_to_stamp     the whole of it without the input-tool spawn
      t0_to_lf          (#1492, every platform with a bash/zsh pane, no injector post time needed) harness Return -> the line accepted: key
                        delivery, the terminal's work, the pty write and the line editor ingesting the line (a burst in Phosphor)
      lf_to_stamp       the line accepted -> the stamp: every preexec hook (Phosphor's DEBUG-trap preexec and title hook in bash; none in
                        the reference) and the `date` start
    A key is absent when no sample had both ends (no post time off macOS, no lf/px from a non-zsh shell)."""
    def d(a, b):
        return [(x[b] - x[a]) * 1000.0 for x in samples if x.get(a) is not None and x.get(b) is not None and 0 <= x[b] - x[a] < 1]
    out = {"n": len(samples)}
    for k, a, b in (("t0_to_post_ms", "t0", "post"), ("post_to_lf_ms", "post", "lf"), ("lf_to_px_ms", "lf", "px"), ("px_to_stamp_ms", "px", "stamp"),
                    ("post_to_stamp_ms", "post", "stamp"), ("post_to_px_ms", "post", "px"), ("t0_to_lf_ms", "t0", "lf"), ("lf_to_stamp_ms", "lf", "stamp")):
        v = _med(d(a, b))
        if v is not None:
            out[k] = v
    return out


def host_load():
    """#1583 W4: the host's logical CPU count and, where the OS reports it, the 1-minute load average (contention on a shared VM/box is
    the first suspect for a latency gap). {cpus, load1?, load_per_cpu?}"""
    out = {"cpus": os.cpu_count()}
    try:
        l1 = os.getloadavg()[0]
        out["load1"] = round(l1, 2)
        if out["cpus"]:
            out["load_per_cpu"] = round(l1 / out["cpus"], 2)
    except (AttributeError, OSError):        # Windows: no load average
        pass
    return out


def gitbash_fork_ms(n=40):
    """#1583 G1: Git Bash's fork+exec cost on this host, ms per `/usr/bin/true` (median of 3 runs of n), measured with Git for Windows' own
    bash.exe outside the app. msys fork is emulated and slow; when it is tens of ms, Phosphor's shell hooks (which fork) explain the Git Bash
    gap by themselves. None off Windows / without Git Bash / on error."""
    import winapi
    gb = winapi.git_bash()
    if not gb:
        return None
    runs = []
    for _ in range(3):
        t0 = time.time()
        try:
            p = subprocess.run([gb, "--noprofile", "--norc", "-c", "for i in $(seq %d); do /usr/bin/true; done" % n], capture_output=True, timeout=120)
        except (OSError, subprocess.TimeoutExpired):
            return None
        if p.returncode:
            return None
        runs.append((time.time() - t0) * 1000.0 / n)
    return round(statistics.median(runs), 2)


def reference_with_bootstrap(ctx, r, n, warm, rs):
    """#1583 G2 (Windows, Git Bash pane): a SECOND reference console whose bash sources Phosphor's own bash bootstrap (the bash-preexec DEBUG trap, the
    precmd/preexec hooks that fork, the title hook) after the plain reference rc, measured with the same stamp line and cadence. The difference to the
    plain reference is what the shell hooks alone cost; what is left between this reference and the pane is the app + ConPTY. Metrics
    `reference_bootstrap_enter_to_start_ms` and `reference_bootstrap_minus_plain_ms`, one note; never a verdict input (the verdict stays against the plain
    reference, which is the owner's definition of overhead). A note, not a failure, when the bootstrap cannot be composed or the window will not open."""
    if not os.path.exists(ctx.prof.ref_boot_rc):
        r.note("G2: reference with the bootstrap sourced not run: app/assets/bundled/bootstrap/bash.sh + bash_body.sh were not found next to the harness")
        return
    saved = getattr(ctx, "ref_shell", None)
    ref2 = None
    try:
        ref2 = ref_terminal_windows(ctx, rcfile=ctx.prof.ref_boot_rc.replace("\\", "/"))
        if not ref2:
            r.note("G2: reference with the bootstrap sourced not run: %s" % getattr(ctx, "ref_reason", "no window"))
            return
        time.sleep(2.5)
        nb = max(10, n // 2)
        bs = enter_to_start(ctx, nb + warm, budget_s=max(60, (nb + warm) * 2.5))[warm:]
        ctx.reset()
        if not bs:
            r.note("G2: the bootstrap-sourced reference captured no Enter timing (its rc may have failed: see the screenshot)")
            return
        r.metric("reference_bootstrap_enter_to_start_ms", stats(bs))
        mb, mp = statistics.median(bs), statistics.median(rs)
        r.metric("reference_bootstrap_minus_plain_ms", round(mb - mp, 2))
        r.note("G2: reference bash with Phosphor's bootstrap sourced %.1f ms vs the plain reference %.1f ms (%+.1f ms): that is what the shell hooks cost on this host, outside the app" % (mb, mp, mb - mp))
    finally:
        if ref2:
            ref2.close()
        ctx.ref_shell = saved
        time.sleep(1.0)


M3_UNSET = "unset zle_bracketed_paste; echo M3_$((${+zle_bracketed_paste})) >> m3_state.txt"
M3_RESTORE = "zle_bracketed_paste=( $'\\e[?2004h' $'\\e[?2004l' ); echo M3_$((${#zle_bracketed_paste})) >> m3_state.txt"


def unset_bracketed_paste_batch(ctx, r, n, warm, ds):
    """#1583 M3 (macOS zsh pane, terminal mode): size the cost of the paste wrap. zsh's line editor turns bracketed paste on (zle_bracketed_paste), so
    Phosphor writes Enter's line wrapped in ESC[200~ ... ESC[201~ and zle runs its bracketed-paste widget on it. `unset zle_bracketed_paste` makes zle
    leave the mode off from the next prompt on; a second batch of the same stamp line is then measured and compared with the first. Metrics
    `unset_bracketed_paste_enter_to_start_ms` and `unset_bracketed_paste_minus_default_ms`, one note. The parameter is put back afterwards."""
    sh = pane_shell_id(ctx)
    if lat.shell_base(sh) != "zsh":
        r.note("M3: not run: the pane shell is %s, not zsh (zle_bracketed_paste is a zsh parameter)" % sh)
        return
    ctx.rm("m3_state.txt")
    ctx.be.type(M3_UNSET, 12, enter=True)
    ctx.until(lambda: ctx.lines("m3_state.txt"), 6)
    if "M3_0" not in ctx.lines("m3_state.txt"):
        r.note("M3: not run: `unset zle_bracketed_paste` did not take effect in the pane (state file %r)" % ctx.lines("m3_state.txt"))
        return
    time.sleep(1.0)
    try:
        nb = max(10, n // 2)
        bs = enter_to_start(ctx, nb + warm, budget_s=max(60, (nb + warm) * 2.5))[warm:]
    finally:
        ctx.rm("m3_state.txt")
        ctx.be.type(M3_RESTORE, 12, enter=True)
        ctx.until(lambda: ctx.lines("m3_state.txt"), 6)
        if "M3_2" not in ctx.lines("m3_state.txt"):
            r.note("M3: could not confirm that zle_bracketed_paste was restored (state %r): the next cases may run without bracketed paste in this pane" % ctx.lines("m3_state.txt"))
        time.sleep(0.5)
        ctx.reset()
    if not bs:
        r.note("M3: no Enter timing captured with bracketed paste off")
        return
    r.metric("unset_bracketed_paste_enter_to_start_ms", stats(bs))
    mb, md = statistics.median(bs), statistics.median(ds)
    r.metric("unset_bracketed_paste_minus_default_ms", round(mb - md, 2))
    r.note("M3: Enter -> start with zle_bracketed_paste unset %.1f ms vs default %.1f ms (%+.1f ms): that is the cost of the paste wrap in zle (a negative or ~0 value means the wrap is not the gap)" % (mb, md, mb - md))


def record_enter_split(r, splits, segs):
    """#1606: the per-hop split of Enter -> command start for both sides, as the metric `enter_split_ms` {phosphor, reference, delta} and one
    note. The SAME shell clock (the rc's zle-line-finish / last-preexec times, enter_split_summary) on both sides, so a gap in post_to_lf
    is key delivery + terminal + line-editor ingest, a gap in lf_to_px is preexec hooks, a gap in px_to_stamp would be the command itself.
    Phosphor's post_to_lf is then split further by the app log (segs: event_age, event_to_enter, enter_to_write). The shell-side medians
    include each interleaved block's discarded lead sample (the sampler cannot see which one run_interleaved drops)."""
    sp = {side: enter_split_summary(v) for side, v in splits.items() if v}
    if not sp:
        return
    if "phosphor" in sp and "reference" in sp:
        sp["delta"] = {k: round(sp["phosphor"][k] - sp["reference"][k], 2) for k in sp["phosphor"]
                       if k.endswith("_ms") and k in sp["reference"]}
    app = {k: segs[k] for k in ("event_age_ms", "event_to_enter_ms", "enter_to_write_ms", "submit_bookkeeping_ms", "effect_flush_ms",
                                "pty_lock_wait_ms") if k in segs}
    if app:
        sp["phosphor_app"] = app
    r.metric("enter_split_ms", sp)
    if "delta" in sp:
        r.note("Enter split (Phosphor - reference, medians ms, #1606): " + ", ".join("%s %+.2f" % (k[:-3], v) for k, v in sp["delta"].items()) +
               ("; Phosphor app: " + ", ".join("%s %.2f" % (k[:-3], v) for k, v in app.items()) if app else ""))


@case("latency_enter", order=41, gui=True)
def c_lat_enter(ctx, r):
    """Enter -> command start, Phosphor vs a reference terminal in the SAME session with the SAME keys/cadence/timestamp command.
    Verdict is on OVERHEAD (Phosphor median - reference median) against the owner's 2 ms budget; both include one input-tool spawn,
    which cancels. No reference terminal -> verdict 'no-reference' (absolute numbers only). Also checked build-to-build (lat.py)."""
    n, warm = ctx.count_lat(60), lat.warmup()      # 60: the SE of a 30-sample median difference (~2.3 ms) is larger than the 2 ms budget
    pane_sh = pane_shell_id(ctx)        # asked once, before the first sample (Linux/macOS), so the stamp line below is chosen for the right shell
    r.metric("pane_shell", pane_sh or "unknown")
    r.metric("stamp_line", stamp_line(ctx))       # #1492: which stamp line ran (APPCI_ENTER_SPLIT=1 opts into the split line)
    ctx.lf_missing = 0
    r.metric("host_load", host_load())          # #1583 W4
    if ctx.win and ctx.pane == "gitbash":
        fk = gitbash_fork_ms()                   # #1583 G1
        if fk is not None:
            r.metric("gitbash_fork_ms", fk)
            r.note("G1: Git Bash fork+exec costs %.1f ms per process on this host (outside the app); every forking shell hook adds about that" % fk)
    t_lat0 = time.time()
    # #1514: A/B/A/B interleaving in blocks when the reference can stay open next to Phosphor (Linux, macOS); otherwise (Windows, no reference, a focus
    # failure) the sequential mode below. Which one ran is recorded (metrics.measurement_mode, lat record measurement_mode).
    ds, rs, ref, mode, fallback, blocks, tried = None, [], None, "sequential", None, None, False
    splits = {"phosphor": [], "reference": []}      # #1606: per-sample shell-side split (enter_to_start split=)
    if not ctx.win and lat.interleave_enabled():
        ref, tried = ref_terminal(ctx), True
        if ref and ref.switch:
            try:
                time.sleep(2.5)
                ds, rs, blocks = run_interleaved(ctx, ref, n, warm, lambda side, cnt: enter_to_start(ctx, cnt, split=splits[side]))
                mode = "interleaved"
            except RuntimeError as e:
                ds, rs, blocks = None, [], None
                splits = {"phosphor": [], "reference": []}
                fallback = "interleaving failed, fell back to sequential: %s" % str(e)[:160]
        elif ref:
            fallback = "this backend cannot keep both windows"
        if ds is None and ref and ref.switch:
            try:
                ref.switch("phosphor")
            except RuntimeError:
                pass
    if ds is None:
        ds = enter_to_start(ctx, n + warm, split=splits["phosphor"])[warm:]
    record_interleave(r, mode, blocks, fallback)
    r.metric("enter_to_start_ms", stats(ds))
    segs = ctx.app_enter_segments(t_lat0)
    share = segs.get("enter_to_write_ms")
    if share is not None:
        # r18cj (Windows Git Bash): Phosphor 95-100 ms vs the conhost reference 69 ms for the SAME bash.exe/rc/`date +%s.%N`/injector. The app's own part is this number
        # (Enter -> the pty write of the whole line); the rest of the overhead is after the write: the shell reading the line (the reference's keys were echoed as typed)
        # plus the shell hooks Phosphor's bootstrap adds. Reported so the overhead can be split instead of argued.
        r.metric("app_enter_to_pty_write_ms", share)
        r.note("app share of Enter->start: enter_begin -> pty_write median %.2f ms (from the app's input_order log); the remaining overhead is shell/ConPTY side" % share)
    if not segs:
        r.metric("app_segments_ms", dict(unavailable=True, log_path=ctx.log_path, log_bytes=ctx.log_size(), platform=plat()))        # r18e9: say WHY the metric is absent
        r.note("app segment metrics NOT available: no enter_begin -> pty_write pair in the app log %r (%d bytes) since the case started (stale log path after a relaunch, input-order logging off, or the Enters never reached the editor)" % (ctx.log_path, ctx.log_size()))
    if segs:
        # #1583 W3: the three segments per platform, as metrics (enter_begin->write, write->first pty_read, write->preexec)
        r.metric("app_segments_ms", dict(segs, platform=plat()))
        if "write_to_first_read_ms" in segs:
            r.note("shell side: pty_write -> first pty_read median %.2f ms (n=%s)" % (segs["write_to_first_read_ms"], segs.get("write_to_first_read_n")))
        if "write_to_preexec_ms" not in segs:
            r.note("write -> preexec NOT measurable: no `hop=preexec_received` line in the app log (a build older than #1583 W3, input-order logging off, or the shell sent no preexec)")
    if ctx.win and ctx.pane == "powershell":
        # #1401: the gated samples (and the app segments above) ran with the hook timing marks OFF (the `.on` switch file never existed). The per-hop split comes
        # from a SEPARATE, ungated batch with the marks on, so the marks' own cost never reaches a verdict.
        r.metric("pwsh_hook_timing_during_samples", False)
        nm = max(10, n // 3)
        t_marks0 = time.time()
        hook_marks = ctx.pwsh_hook_timing(True)
        if hook_marks:
            try:
                enter_to_start(ctx, nm + warm + 1, budget_s=max(45, (nm + warm + 1) * 2.5))     # +1: the shell reads `.on` at a prompt, so the first may be unmarked
            finally:
                ctx.pwsh_hook_timing(False)
            time.sleep(0.5)                              # the last command's marks are written out at its next prompt
            stamps = []
            for i in range(nm + warm + 1):
                try:
                    v = float(open(os.path.join(ctx.W, "lat_%d" % i), errors="replace").read().replace("\x00", "").strip())
                    stamps.append(v if v > 1e11 else v * 1000.0)
                except (OSError, ValueError):
                    pass
            hs = ctx.pwsh_hook_segments(hook_marks, t_marks0, stamps)
            r.metric("pwsh_hook_segments_ms", hs or dict(unavailable=True, marks_file=hook_marks))
            if hs:
                r.note("PowerShell side of Enter->start (#1401, separate ungated batch of %d with the marks on, medians ms): app write->chord handler %s, "
                       "pipe read %s, insert %s, AcceptLine %s, line->Preexec OSC %s, OSC->app %s, Preexec tail %s, hook->command start %s; next prompt: "
                       "precmd %s, render %s, maintenance+marker %s"
                       % ((nm,) + tuple(hs.get(k, "-") for k in ("app_write_to_handler", "handler_pipe_read", "handler_insert", "accept_line", "readline_to_preexec",
                                                         "preexec_osc_to_app", "preexec_tail", "hook_to_command_start", "prompt_precmd", "prompt_render",
                                                         "readline_maintenance"))))
    r.metric("harness_floor_ms", ctx.be.key_floor_ms)
    r.check(len(ds) >= n * 0.8, "only %d/%d Enter timings captured" % (len(ds), n))      # hard floor; a PASS verdict needs 90 % (lat.evaluate expected=)
    ctx.reset()
    if ctx.be.name == "mac" and ctx.mode == "terminal" and ds:
        unset_bracketed_paste_batch(ctx, r, n, warm, ds)           # #1583 M3
    if ref is None and not tried:
        ref = ref_terminal(ctx)
    if ref:
        ref_note(ctx, r)
    if ref:
        try:
            if mode != "interleaved":
                if ref.switch:
                    ref.switch("reference")
                time.sleep(2.5)
                rs = enter_to_start(ctx, n + warm, budget_s=max(60, (n + warm) * 2.5), split=splits["reference"])[warm:]      # overall cap: a dead reference cannot hang the case
            ctx.reset()
            if ctx.win and rs:
                # #1583 W1: the SAME stamp line delivered to the reference as ONE burst (0 ms between characters), then Enter. Phosphor writes the whole
                # line in one pty write; the per-key reference had the line echoed and parsed already. If the burst reference is much slower than the
                # per-key one, the Windows gap is the shell ingesting a burst (conhost/PSReadLine/bash), not Phosphor's work.
                nb = max(10, n // 2)
                bs = enter_to_start(ctx, nb + warm, budget_s=max(45, (nb + warm) * 2.5), key_delay_ms=0)[warm:]
                ctx.reset()
                if bs:
                    r.metric("reference_burst_enter_to_start_ms", stats(bs))
                    mb, mk = statistics.median(bs), statistics.median(rs)
                    r.metric("reference_burst_minus_per_key_ms", round(mb - mk, 2))
                    if mb - mk > 5:
                        r.note("W1: reference burst %.1f ms vs reference per-key %.1f ms (+%.1f ms): the gap is shell ingestion of a burst, not Phosphor-specific" % (mb, mk, mb - mk))
                    else:
                        r.note("W1: reference burst %.1f ms vs per-key %.1f ms (%+.1f ms): burst ingestion does not explain the gap" % (mb, mk, mb - mk))
        finally:
            close_ref(ref) if mode == "interleaved" or fallback else ref.close()
        time.sleep(1.0)   # focus returns to Phosphor before any re-measure
        if ctx.win and ctx.pane == "gitbash" and rs:
            reference_with_bootstrap(ctx, r, n, warm, rs)          # #1583 G2
        r.metric("reference_enter_to_start_ms", stats(rs))
        r.metric("reference", ref.name)
        r.metric("reference_shell", getattr(ctx, "ref_shell", None) or "unknown")
        if lat.shell_base(pane_sh) and lat.shell_base(getattr(ctx, "ref_shell", None)) and lat.shell_base(pane_sh) != lat.shell_base(ctx.ref_shell):
            r.note("NO-COMPARE: pane shell %s != reference shell %s; the overhead is not judged" % (pane_sh, ctx.ref_shell))
    else:
        r.note("no reference terminal available (need xterm+DISPLAY / foot / Terminal.app)%s" % (": " + ctx.ref_reason if getattr(ctx, "ref_reason", None) else ""))
    if "__pa_lf>>" in stamp_line(ctx):
        r.metric("lf_missing", getattr(ctx, "lf_missing", 0))     # #1492: split-line samples (both sides) whose line-accepted time never arrived
    record_enter_split(r, splits, segs)
    if ds:
        rec = lat.evaluate("%s/latency_enter/enter_to_start" % ctx.label, ds, rs, float(os.environ.get("APPCI_ENTER_BUDGET_MS", "2")), "overhead", plat(),
                           remeasure=lambda: enter_to_start(ctx, n + warm)[warm:], ref_name=ref.name if ref else None, floor=ctx.be.key_floor_ms,
                           pane_shell=pane_sh, ref_shell=getattr(ctx, "ref_shell", None) if ref else None, expected=n, mode=mode,
                           notes=[("reference interleaved with Phosphor in blocks of %d (A/B/A/B, same session)" % lat.interleave_block() if mode == "interleaved"
                                   else "reference measured after Phosphor (sequential, same session)" + ("; " + fallback if fallback else "")) +
                                  "; Linux headless-GPU numbers compare builds, not absolute vs hand-measured baselines"])
        r.metrics.setdefault("lat", {})["enter_to_start"] = rec
        lat_checks(r, rec, "Enter to command start")
        if not rs and ds:
            r.check(statistics.median(ds) <= 80, "Enter to command start median %.1f ms > 80 ms CI sanity bound" % statistics.median(ds))
    ctx.reset()


def parse_t(line):
    m = re.search(r"\bt=(\d+\.\d+)", line)
    return float(m.group(1)) if m else None


KEYDOWN_RE = re.compile(r"input_order keydown key=\S+ ctrl=(true|false) alt=\S+ shift=\S+ cmd=\S+ t=(\d+\.\d+)(?: seq=(\d+))?")
FRAME_RE = re.compile(r"input_order frame layout=\S+ t=(\d+\.\d+)")


def key_frame_pairs(txt):
    """Pair every plain (no ctrl) `keydown` with the first `frame` line after it (formats: crates/warpui_core/src/input_order.rs,
    `keydown key=m ctrl=false alt=false shift=false cmd=false t=<ms> seq=N`, `frame layout=<bool> t=<ms>`; t is ms since the epoch).
    A frame is logged only for the first frame after pty output that followed a pty write (#1092), so a key that never reaches the
    pty (typed into the input editor) has no frame at all: keys that pile up before one frame all pair with that frame, and keys
    after the last frame stay unpaired. -> (latencies ms, counts dict)"""
    cnt = dict(keydown=0, keydown_ctrl=0, frame=0, pty_write=0, pty_read=0)
    pending, ds = [], []
    for l in txt.splitlines():
        m = KEYDOWN_RE.search(l)
        if m:
            cnt["keydown"] += 1
            if m.group(1) == "true":
                cnt["keydown_ctrl"] += 1
            else:
                pending.append(float(m.group(2)))
            continue
        m = FRAME_RE.search(l)
        if m:
            cnt["frame"] += 1
            tf = float(m.group(1))
            ds.extend(tf - t for t in pending if tf >= t)
            pending = []
        elif "input_order pty_write" in l:
            cnt["pty_write"] += 1
        elif "input_order pty_read" in l:
            cnt["pty_read"] += 1
    return ds, cnt


PTY_WRITE_RE = re.compile(r"input_order pty_write bytes=\d+ t=(\d+(?:\.\d+)?)")


def key_write_pairs(txt):
    """#1176: key -> the pty write(2) that carries it. Pairs every plain (no ctrl) `keydown` with the first `input_order pty_write bytes=N t=<ms>`
    line after it (input_order::log_pty_write, logged on the pty event-loop thread right after the write; app/src/terminal/local_tty/event_loop.rs).
    Keys that the event loop coalesces into one write all pair with that write; a key typed into the input editor (no pty write) pairs with the
    next write that does happen, so callers only use this where every key goes to the pty (a running `cat`). Hop lines (`hop=pty_write_partial`,
    `hop=ctrl_c_pty_write`) are not writes of this kind and are ignored. -> [ms]"""
    pending, ds = [], []
    for l in txt.splitlines():
        m = KEYDOWN_RE.search(l)
        if m:
            if m.group(1) != "true":
                pending.append(float(m.group(2)))
            continue
        m = PTY_WRITE_RE.search(l)
        if m:
            tw = float(m.group(1))
            ds.extend(tw - t for t in pending if tw >= t)
            pending = []
    return ds


@case("latency_key_log", order=42, gui=True)
def c_lat_key(ctx, r):
    """key -> first frame after the echo, from the app's own input_order log (keydown ... pty_write -> pty_read -> frame);
    needs RUST_LOG input_order (set by the launcher). Phosphor-side only: no paint-readback through Wayland."""
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.skip("no app log at %s (set APPCI_APP_LOG)" % ctx.log_path)
    n, warm = ctx.count_lat(60), lat.warmup()
    SLOW = re.compile(r"slow ([\w .:/-]+?): (\d+) ms")

    def run_keys(count):
        # Typing at a shell prompt goes into Phosphor's input editor and reaches the pty only on Enter, so it has no pty echo and
        # the app logs no `frame` line for it (#1486). Keys are typed into a running `cat` instead: they go straight to the pty,
        # the tty echoes them, and the first frame after that echo is logged.
        ctx.be.type("cat > /dev/null", 10, enter=True)
        time.sleep(1.0)
        off = ctx.log_size()
        for i in range(count):
            ctx.be.type("m", 10)
            time.sleep(ctx.rng.uniform(0.08, 0.18))
        time.sleep(1)
        txt = ctx.log_since(off)
        ctx.reset()                  # Ctrl-C ends the cat
        ds, cnt = key_frame_pairs(txt)
        run_keys.counts = cnt
        run_keys.txt = txt
        return ds[warm:], [(m.group(1), int(m.group(2))) for m in SLOW.finditer(txt)]

    ui_off = ctx.log_size()          # the UI-work window: the typed keys of the first run (the `cat` start is a handful of items; the 5 % allowance covers it)
    ds, slow = run_keys(n + warm)
    ui_txt = ctx.log_since(ui_off)       # the window ends here: lat.evaluate's remeasure below types more keys
    r.metric("key_to_frame_ms", stats(ds))
    # #1176 (owner 2026-11-03: measure first, then decide): the app's own key -> pty write(2) share, whose target is <= 1 ms median at idle
    # (keys here are 80-180 ms apart), and the main-thread items >= 4 ms and model-lock holds >= 2 ms during the same typing. Informational:
    # metrics and a note, never a check, until the owner sets the verdict from a real-GPU run.
    txt = getattr(run_keys, "txt", "")
    kw = key_write_pairs(txt)[warm:]
    r.metric("key_to_pty_write_ms", stats(kw))
    if kw:
        kw50 = statistics.median(kw)
        r.note("#1176 key -> pty write median %.2f ms (p95 %.2f, n=%d): %s the 1 ms target" % (kw50, pct(kw, 0.95), len(kw), "within" if kw50 <= 1.0 else "OVER"))
    else:
        r.note("#1176 key -> pty write: no keydown -> `input_order pty_write` pair in the measurement window (input-order logging off, or no key reached the pty)")
    r.metric("typing_ui_items", ui_item_audit(txt))
    r.metric("typing_lock_holds", lock_hold_audit(txt))
    if len(ds) < n * 0.5:
        cnt = getattr(run_keys, "counts", {})
        why = ("no keydown line at all: input_order logging off, wrong log path, or the keys never reached the app" if not cnt.get("keydown") else
               "keydowns but no `frame` line: no key reached the pty (the `cat` did not start, or this mode routes typing elsewhere), and the app only logs "
               "a frame after pty output that followed a pty write" if not cnt.get("frame") else
               "frames exist but few follow a keydown: check for a stuck Control modifier (ctrl=true keydowns)")
        r.check(False, "only %d key->frame pairs from the app log %s (expected ~%d); in the measurement window: %s. Diagnosis: %s"
                % (len(ds), ctx.log_path, n, ", ".join("%s=%d" % kv for kv in sorted(cnt.items())), why))
        return
    # Phosphor-side clock only (keydown -> first frame after the echo); there is no paint read-back for a reference terminal through
    # Wayland / ScreenCaptureKit at this resolution, so the 2 ms overhead budget cannot be judged: verdict 'no-reference'.
    rec = lat.evaluate("%s/latency_key_log/key_to_frame" % ctx.label, ds, None, float(os.environ.get("APPCI_KEY_BUDGET_MS", "2")), "overhead", plat(),
                       remeasure=lambda: run_keys(n + warm)[0], notes=["Phosphor-side keydown to first frame; no same-method reference exists"])
    r.metrics.setdefault("lat", {})["key_to_frame"] = rec
    lat_checks(r, rec, "key to frame")
    # UI work (#1548): the app's `hop=ui_item` line (input_order::log_ui_item_if_over_budget, main thread, an event/action/frame at or over the 4 ms
    # budget) is the real ≤4 ms line; the older slow-operation log (#1146) only emits >= 8 ms and stays as a second source. Judged over the typed keys of
    # the FIRST run. `event` / `action` items are UI logic: more than max(2, 5 % of the keys) over 4 ms FAIL. `frame` items include rasterisation, which
    # llvmpipe/Xvfb and a VM's software GL put over 4 ms by themselves (ui_item_metrics says so): they are WARN and a metric, never a failure.
    items = ui_items(ui_txt)
    logic = [ms for _, ms, k in items if k != "frame"]
    frames = [ms for _, ms, k in items if k == "frame"]
    worst_ms = max([m for _, m in slow] + [ms for _, ms, _ in items], default=0)
    r.metric("ui_slow_ops", dict(count=len(slow), worst_ms=worst_ms, ops=sorted({w for w, _ in slow})[:8]))
    allowed = max(2, int(0.05 * (n + warm)))
    uiv = "fail" if len(logic) > allowed else ("warn" if items or worst_ms >= 16 else "pass")
    r.metric("ui_items_over_4ms", dict(logic_count=len(logic), logic_max_ms=max(logic, default=0.0), frame_count=len(frames), frame_max_ms=max(frames, default=0.0),
                                       allowed=allowed, source="hop=ui_item" if items else "none logged (nothing over 4 ms, or a build older than #1191)"))
    r.metrics["lat"]["ui_work"] = dict(phosphor=dict(n=len(items), p50=None, p95=None, max=float(worst_ms)), reference=None, budget_ms=4, budget_kind="ui-item-log",
                                       overhead_ms=None, prev=None, baseline={"status": "n/a"}, budget_verdict=uiv, verdict=uiv,
                                       notes=["hop=ui_item logs every main-thread item >= 4 ms: pass = none while typing; warn = frame items (raster-bound on software GL) or a few logic items; "
                                              "fail = more than max(2, 5 %% of the keys) event/action items; >= 16 ms is a dropped frame"])
    r.check(uiv != "fail", "UI work: %d event/action item(s) >= 4 ms during %d typed keys (allowed %d, worst %.1f ms)" % (len(logic), n + warm, allowed, max(logic, default=0.0)))
    if frames:
        r.note("UI work: %d frame item(s) >= 4 ms while typing (max %.1f ms); frame items include rasterisation, so this is a WARN on software GL" % (len(frames), max(frames)))
    if worst_ms >= 16:
        r.note("WARN UI work: item(s) up to %.1f ms during typing (>= 16 ms drops a frame): %s" % (worst_ms, ", ".join(sorted({w for w, _ in slow})[:5]) or "see hop=ui_item lines"))
    p50, p99 = statistics.median(ds), pct(ds, 0.99)
    r.check(p50 <= 20, "key->frame p50 %.1f ms > 20 ms CI bound (owner target: next refresh, own work <= 2 ms; software GL here)" % p50)
    r.check(p99 <= 60, "key->frame p99 %.1f ms > 60 ms CI bound" % p99)
    ctx.reset()


KL3_QUANTUM_MS = 1000.0 / 240.0      # ScreenCaptureKit frame times at 240 Hz: 4.17 ms against a 2 ms budget, so a pass cannot be claimed (#1514)


def kl_paint_ms(out):
    """one kl3 `K` answer line -> ms from the key post to the first changed pixel, or None when kl3 saw no change"""
    m0 = re.search(r"post0=([\d.]+)", out)
    ch = [float(x.split(":")[0]) for x in (re.search(r"ch=(\S*)", out).group(1) if re.search(r"ch=(\S*)", out) else "").split(",") if x]
    if m0 and ch:
        t0 = float(m0.group(1))
        after = [c for c in ch if c >= t0 - 1]
        if after:
            return min(after) - t0
    return None


def paint_samples(ctx, rect, n):
    """n x (post 'm' or Delete through kl3, ScreenCaptureKit 240 Hz) -> key->first-changed-pixel ms in the window at `rect`.
    The same call for Phosphor and the reference terminal. Alternates 'm' and Delete so every key changes the screen."""
    ds = []
    ctx.be.kl_start(rect)
    try:
        for i in range(n):
            d = kl_paint_ms(ctx.be.kl("K 400 %s" % ("m" if i % 2 == 0 else "DEL")))
            if d is not None:
                ds.append(d)
    finally:
        ctx.be.kl_stop()
    return ds


def pid_of(name):
    """newest exact PID of Terminal.app (the only name used): from `ps` by full executable path, never pgrep (no name matching)"""
    if name != "Terminal":
        return None
    import refterm_mac
    pids = refterm_mac.terminal_pids()
    return max(pids) if pids else None


@case("latency_paint", order=43, gui=True)
def c_lat_paint(ctx, r):
    """key -> first changed pixel in the terminal window, measured with kl3 (ScreenCaptureKit, 240 Hz frame times; macOS only), for
    Phosphor AND for the reference terminal (Terminal.app) with the same kl3 command in the same session. The verdict is on OVERHEAD
    (Phosphor median - reference median) against the owner's 2 ms key->paint budget; both include the same capture/injection cost
    (kl3 post + 240 Hz quantisation), which cancels. No reference -> 'no reference: absolute only' note."""
    if ctx.be.name != "mac" or not hasattr(ctx.be, "kl_start"):
        r.skip("kl3 (ScreenCaptureKit) is macOS only; no key->paint read-back exists on this platform, so there is nothing to compare")
    rect = ctx.be.window_rect()
    if not rect or rect[2] < 100:
        r.skip("no Phosphor window rect from System Events")
    ctx.be.guard()
    n, warm = ctx.count_lat(30), lat.warmup()
    t_paint0 = time.time()
    # #1514: A/B/A/B blocks with Terminal.app kept open next to Phosphor (each window sampled at its own rect); sequential when interleaving is
    # off, there is no reference/rect, or a focus guard fails. The mode that ran is recorded.
    ds, rs, ref, mode, fallback, blocks, tried = None, [], None, "sequential", None, None, False
    if lat.interleave_enabled():
        ref, tried = ref_terminal(ctx), True
        if ref and ref.switch:
            try:
                time.sleep(2.5)
                rrect = ctx.be.window_rect(pid=pid_of("Terminal"))
                if not rrect or rrect[2] < 100:
                    fallback = "no Terminal.app window rect"
                else:
                    ds, rs, blocks = run_interleaved(ctx, ref, n, warm, lambda side, cnt: paint_samples(ctx, rect if side == "phosphor" else rrect, cnt))
                    mode = "interleaved"
            except RuntimeError as e:
                ds, rs, blocks = None, [], None
                fallback = "interleaving failed, fell back to sequential: %s" % str(e)[:160]
            if ds is None:
                try:
                    ref.switch("phosphor")
                except RuntimeError:
                    pass
    if ds is None:
        ds = paint_samples(ctx, rect, n + warm)[warm:]
    record_interleave(r, mode, blocks, fallback)
    ctx.reset()
    r.metric("key_to_paint_ms", stats(ds))
    # #1606/#1491: how long Phosphor's key-downs queued before its main thread took them (the app's share of key->paint that is not its own
    # handling), and the nextDrawable waits in the same window
    kl = ctx.app_log_lines()
    ka = age_stats(os_key_ages(kl, t_paint0, enter=False, ctrl=False))
    if ka:
        r.metric("key_event_age_ms", ka)
    dw = age_stats([float(m.group(1)) for m in re.finditer(r"hop=drawable_wait wait_ms=(\d+(?:\.\d+)?) key_frame=\S+ t=(\d+)", "\n".join(kl))
                    if float(m.group(2)) / 1000.0 >= t_paint0])
    if dw:
        r.metric("drawable_wait_ms", dw)
    r.check(len(ds) >= 20, "only %d key->paint samples from kl3 (capture region or Screen Recording permission wrong)" % len(ds))
    if ref is None and not tried:
        ref = ref_terminal(ctx)
    if ref:
        try:
            if mode != "interleaved":
                if ref.switch:
                    ref.switch("reference")
                time.sleep(2.5)
                rrect = ctx.be.window_rect(pid=pid_of("Terminal"))
                if not rrect or rrect[2] < 100:
                    r.note("no reference: absolute only (key->paint): no Terminal.app window rect")
                else:
                    rs = paint_samples(ctx, rrect, n + warm)[warm:]
            ctx.reset()
        finally:
            close_ref(ref) if mode == "interleaved" or fallback else ref.close()
        time.sleep(1.0)
        r.metric("reference_key_to_paint_ms", stats(rs))
        r.metric("reference", ref.name)
    else:
        r.note("no reference: absolute only (key->paint): no reference terminal available%s" % (": " + ctx.ref_reason if getattr(ctx, "ref_reason", None) else ""))
    if ds:
        rec = lat.evaluate("%s/latency_paint/key_to_paint" % ctx.label, ds, rs, float(os.environ.get("APPCI_KEY_BUDGET_MS", "2")), "overhead", plat(),
                           remeasure=lambda: paint_samples(ctx, rect, n + warm)[warm:], ref_name=ref.name if ref and rs else None, floor=ctx.be.key_floor_ms,
                           expected=n, resolution_ms=KL3_QUANTUM_MS, mode=mode,
                           notes=["kl3 measures post-to-first-changed-pixel at 240 Hz in each terminal's own window; " +
                                  ("reference interleaved with Phosphor in blocks of %d (A/B/A/B, same session)" % lat.interleave_block() if mode == "interleaved"
                                   else "reference measured after Phosphor in the same session" + ("; " + fallback if fallback else ""))])
        r.metrics.setdefault("lat", {})["key_to_paint"] = rec
        lat_checks(r, rec, "Key to paint")
        if not rs:
            r.check(statistics.median(ds) <= 120, "key->paint median %.1f ms > 120 ms sanity bound (no reference measured)" % statistics.median(ds))


# ---------------------------------------------------------------- keydown count == keys sent (#1159)
KEYDOWN_KEY_RE = re.compile(r"input_order keydown key=(\S+) ctrl=(true|false) alt=(true|false) shift=(true|false) cmd=(true|false) t=(\d+(?:\.\d+)?)")
MODIFIER_KEYS = {"shift", "control", "ctrl", "alt", "option", "meta", "super", "command", "cmd", "capslock", "fn"}


def keydown_audit(txt, dup_ms=2.0):
    """#1159: the app log's own count of plain key presses. -> dict(count, dups): `count` = `input_order keydown` lines without ctrl/alt/cmd and not for a
    bare modifier key (Shift of a capital is not a typed key); `dups` = [(key, t_ms, dt_ms)] consecutive presses of the SAME key less than `dup_ms`
    apart (the #1159 signature: the extra keydown arrived ~0.4 ms after the real one; a human or injector at >= 12 ms/key never repeats a key that fast)."""
    ks = []
    for m in KEYDOWN_KEY_RE.finditer(txt):
        key, ctrl, alt, _shift, cmd, t = m.groups()
        if ctrl == "true" or alt == "true" or cmd == "true" or key.lower() in MODIFIER_KEYS:
            continue
        ks.append((key, float(t)))
    dups = [(k2, t2, round(t2 - t1, 2)) for (k1, t1), (k2, t2) in zip(ks, ks[1:]) if k1 == k2 and 0 <= t2 - t1 < dup_ms]
    return dict(count=len(ks), dups=dups)


_UI_ITEM_RE = re.compile(r"input_order hop=ui_item kind=(\w+) ms=(\d+(?:\.\d+)?)")


def ui_item_audit(txt):
    """#1191/#1219/#1301: the app's own `hop=ui_item kind=<frame|event|action> ms=<x>` lines (input_order::log_ui_item_if_over_budget, written for
    every main-thread item at or over the owner's 4 ms UI budget) -> dict(over_4ms, over_16ms, max_ms, by_kind={kind: [count, max_ms]}). Builds
    before the hop log nothing, which reads exactly like 'no item over budget': callers say so (ui_item_metrics)."""
    by_kind, ms_all = {}, []
    for m in _UI_ITEM_RE.finditer(txt):
        kind, ms = m.group(1), float(m.group(2))
        ms_all.append(ms)
        k = by_kind.setdefault(kind, [0, 0.0])
        k[0] += 1
        k[1] = max(k[1], ms)
    return dict(over_4ms=len(ms_all), over_16ms=sum(1 for v in ms_all if v >= 16.0), max_ms=max(ms_all) if ms_all else 0.0, by_kind=by_kind)


_LOCK_HOLD_RE = re.compile(r"model lock held (\d+(?:\.\d+)?)(ns|µs|us|ms|s) by (\S+)")
_DUR_MS = {"ns": 1e-6, "µs": 1e-3, "us": 1e-3, "ms": 1.0, "s": 1000.0}


def lock_hold_audit(txt):
    """#1301: the app's `model lock held <Duration> by <scope>` lines (app/src/terminal/model_lock_trace.rs HoldTimer: every hold >= 2 ms, needs
    RUST_LOG warp::terminal::model_lock_trace=debug) -> {scope: [count, max_ms]}. TerminalView::render and BlockListElement::layout are the
    holds #1301 is about; an empty dict is 'no hold over 2 ms' only on a build and RUST_LOG that log them."""
    out = {}
    for m in _LOCK_HOLD_RE.finditer(txt):
        ms = float(m.group(1)) * _DUR_MS[m.group(2)]
        k = out.setdefault(m.group(3), [0, 0.0])
        k[0] += 1
        k[1] = max(k[1], round(ms, 3))
    return out


def ui_item_metrics(ctx, r, off, label):
    """Records the ui_item audit of the app log written since byte offset `off` as metric `<label>_ui_items`, and the model-lock holds over 2 ms
    as `<label>_lock_holds` (#1301). Informational: llvmpipe/Xvfb frames
    are raster bound and over 4 ms by themselves, so this is a metric for the before/after comparison, never a check."""
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.note("%s: no app log, ui_item audit not taken" % label)
        return None
    txt = ctx.log_since(off)
    a = ui_item_audit(txt)
    r.metric("%s_ui_items" % label, a)
    r.metric("%s_lock_holds" % label, lock_hold_audit(txt))
    if not a["over_4ms"]:
        r.note("%s: no hop=ui_item line (nothing over 4 ms, or a build older than the ui_item hop)" % label)
    return a


_KD_SEQ_RE = re.compile(r"input_order keydown key=(\S+) ctrl=(true|false) alt=(true|false) shift=(?:true|false) cmd=(true|false) t=\d+(?:\.\d+)? seq=(\d+)")
_TYPED_SEQ_RE = re.compile(r"input_order typed_characters\b.* seq=(\d+)")
_ENTER_HOP_RE = re.compile(r"input_order hop=enter_begin deferred=(\w+)")


def key_event_audit(txt):
    """What the app actually logs per injected key (r18d8 validation: the premise 'one plain keydown per character' is false). A physical key reaches the
    app as a `keydown` (named keys, letters), as a `typed_characters` WITHOUT a keydown (wtype-typed symbols: the compositor delivers the text, not a key
    event), or - for an Enter the input editor handles - as neither: only the `hop=enter_begin deferred=false` line. -> dict(count, keydown, typed_only,
    enter_hops, enter_keydowns): count = distinct seq numbers of non-modifier keydowns (no ctrl/alt/cmd) and typed_characters events (a keydown and its
    typed_characters share a seq), plus the Enter hops that have no keydown (an Enter that logs a keydown is not counted twice)."""
    kd, enter_kd = set(), 0
    for m in _KD_SEQ_RE.finditer(txt):
        key, ctrl, alt, cmd, seq = m.groups()
        if ctrl == "true" or alt == "true" or cmd == "true" or key.lower() in MODIFIER_KEYS:
            continue
        kd.add(int(seq))
        if key.lower() in ("enter", "return", "kp_enter"):
            enter_kd += 1
    typed = {int(m.group(1)) for m in _TYPED_SEQ_RE.finditer(txt)}
    hops = sum(1 for m in _ENTER_HOP_RE.finditer(txt) if m.group(1) != "true")
    enter_only = max(hops - enter_kd, 0)
    return dict(count=len(kd | typed) + enter_only, keydown=len(kd), typed_only=len(typed - kd), enter_hops=hops, enter_keydowns=enter_kd, enter_only=enter_only)


@case("keydown_count", order=14, gui=True)
def c_keydown_count(ctx, r):
    """#1159: N known lines typed at 12-30 ms/key (alternating plain typing and the injector's rollover, where it has one: ctx.be.roll = overlapping key-down/key-up on
    macOS htype2 `r:`, the backend's own approximation elsewhere), then (1) the app's key events must equal the keys sent (every character plus Enter): a
    plain `keydown`, or a `typed_characters` with no keydown (wtype-typed symbols), or - for an Enter the input editor handles, which logs no keydown - the
    `hop=enter_begin` line (key_event_audit), with no same-key repeat < 2 ms apart, and (2) the shell must have received every line exactly (effect file; rec.log where the pane has one).
    Full tier only. A doubled keydown is an app/OS-delivery fault; a SHORT count with exact text would mean logging missed a key, a short count with wrong text is key loss."""
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.skip("no app log at %s (set APPCI_APP_LOG): the keydown count is read from the app's input_order log" % ctx.log_path)
    shell = ctx.pane_shell()
    fname = "kd.txt"
    ctx.rm(fname)
    n = ctx.count(20)
    ctx.reset()
    time.sleep(0.5)
    mark = ctx.cmd_mark()
    off = ctx.log_size()
    exp, cmds, sent, how = [], [], 0, {"roll": 0, "type": 0}
    for i in range(n):
        tok = "".join(ctx.rng.choice("abcdefghijklmnopqrstuvwxyz") for _ in range(12))
        cmd = ctx.append_cmd(shell, tok, fname)
        exp.append(tok)
        cmds.append(cmd)
        gap = ctx.rng.randint(12, 30)
        if i % 2:
            ctx.be.roll(cmd, overlap_ms=gap, enter=True)
            how["roll"] += 1
        else:
            ctx.be.type(cmd, gap, enter=True)
            how["type"] += 1
        sent += len(cmd) + 1
        time.sleep(ctx.rng.choice([0.05, 0.2]))
    ctx.until(lambda: len(ctx.lines(fname)) >= n, 20 + n * 0.5)
    time.sleep(1.0)
    got = ctx.lines(fname)
    logtxt = ctx.log_since(off)
    aud = keydown_audit(logtxt)                 # dups: consecutive same-key keydowns < 2 ms apart
    ev = key_event_audit(logtxt)               # count: what the app logs per key, whichever way it arrives (see key_event_audit)
    aud["count"] = ev["count"]
    r.metric("keys_sent", sent)
    r.metric("keydowns_logged", ev["keydown"])
    r.metric("key_events_logged", ev)
    r.metric("injection", how)
    if aud["count"] == 0:
        r.check(False, "no `input_order keydown` line at all in %s for %d keys sent: input_order logging off, wrong log path, or the keys never reached the app" % (ctx.log_path, sent))
        return
    r.check(aud["count"] == sent, "app key events %d != keys sent %d (%+d; %d keydown, %d typed_characters-only, %d Enter via enter_begin only): %s" % (
            aud["count"], sent, aud["count"] - sent, ev["keydown"], ev["typed_only"], ev["enter_only"],
            "EXTRA key events (the #1159 doubled-delivery signature)" if aud["count"] > sent else "keys missing from the app log (lost before the app)"))
    r.check(not aud["dups"], "same key pressed twice < 2 ms apart %d time(s): %s" % (len(aud["dups"]), aud["dups"][:5]))
    r.check(got == exp, "received text not exact: %d/%d lines; first mismatch %s" % (
        len(got), n, next(((a, b) for a, b in zip(exp + [None], got + [None]) if a != b), None)))
    if ctx.prof.commands():
        have = ctx.cmds_since(mark)
        r.check(have == cmds, "shell rec.log: %d/%d commands exact; first mismatch %s" % (
            sum(1 for a, b in zip(cmds, have) if a == b), n, next(((a, b) for a, b in zip(cmds + [None], have + [None]) if a != b), None)))


# ---------------------------------------------------------------- first key after idle (#1160)
IDLE_GAPS = (1.0, 2.0, 5.0)
IDLE_BLOCK = 10      # samples per side before the A/B order flips (macOS: a focus switch between Phosphor and Terminal.app costs ~1 s, so not per sample)


def idle_key_budget_ms():
    return float(os.environ.get("APPCI_IDLE_KEY_BUDGET_MS", "5"))


def idle_paint_block(ctx, rect, gap, count):
    """macOS: `count` x (sleep `gap` s with no input, then ONE key through kl3) -> key->first-changed-pixel ms of that first key alone"""
    ds = []
    ctx.be.kl_start(rect)
    try:
        for i in range(count):
            time.sleep(gap)
            d = kl_paint_ms(ctx.be.kl("K 400 %s" % ("m" if i % 2 == 0 else "DEL")))
            if d is not None:
                ds.append(d)
    finally:
        ctx.be.kl_stop()
    return ds


def idle_log_block(ctx, gap, count):
    """Linux / Windows Git Bash: `count` x (sleep `gap`, one key, 0.3 s later a second key) typed into a running `cat > /dev/null` -> (first-key, second-key)
    key->first-frame ms from the app's input_order log. The second key (0.3 s after the first, no idle) is the in-app control."""
    ctx.be.type("cat > /dev/null", 10, enter=True)
    time.sleep(1.0)
    first, second = [], []
    off = ctx.log_size()
    try:
        for i in range(count + 1):
            if i:       # the previous sample's two keys are in the log by now
                ds, _ = key_frame_pairs(ctx.log_since(off))
                if len(ds) >= 2:
                    first.append(ds[0]); second.append(ds[1])
            if i == count:
                break
            time.sleep(gap)
            off = ctx.log_size()
            ctx.be.type("m", 10)
            time.sleep(0.3)
            ctx.be.type("m", 10)
    finally:
        time.sleep(0.5)
        ctx.reset()
    return first, second


@case("latency_first_key_after_idle", order=44, gui=True)
def c_lat_first_key_idle(ctx, r):
    """#1160: the FIRST key after an idle gap of 1, 2 and 5 s, measured alone (full tier only; ~9 min on macOS). n = ctx.count_lat(30) kept samples per gap (never < 30),
    a few discarded warm-up samples per side and gap.
    macOS: key -> first changed pixel (kl3, 240 Hz) for Phosphor AND Terminal.app in the same session, A/B by blocks of 10 samples (the side that goes first
    alternates per gap); the verdict per gap is lat.evaluate on OVERHEAD (Phosphor median - Terminal.app median) against APPCI_IDLE_KEY_BUDGET_MS (5 ms, owner 2026-11-03; the
    issue's 'next refresh + 4 ms'), SE-aware PASS / FAIL / INCONCLUSIVE. Other platforms: no paint read-back, so key -> first frame from the app log, and
    the control is the SAME session's second key (0.3 s after the first, not idle): the verdict is the idle penalty (first - second median) against the
    same budget. Not interleaved per sample: a macOS focus switch between the two windows is itself an activation, so it happens between blocks."""
    mac = ctx.be.name == "mac" and hasattr(ctx.be, "kl_start")
    n, warm = ctx.count_lat(30), lat.warmup()
    budget = idle_key_budget_ms()
    if not mac:
        if not ctx.log_path or not os.path.exists(ctx.log_path):
            r.skip("no app log at %s (set APPCI_APP_LOG): key -> frame is read from the app's input_order log" % ctx.log_path)
        for g in IDLE_GAPS:
            first, second = [], []
            for gap, _side, cnt, disc in lat.idle_schedule([g], n, block=n, warm=warm, two_sided=False):
                a, b = idle_log_block(ctx, gap, cnt)
                first += a[disc:]
                second += b[disc:]
            tag = "gap%gs" % g
            r.metric("first_key_" + tag, stats(first))
            r.metric("second_key_" + tag, stats(second))
            r.check(len(first) >= 0.9 * n, "idle %g s: only %d of %d first-key key->frame pairs in the app log (keys not reaching the `cat`, or no input_order logging)" % (g, len(first), n))
            if len(first) >= 2:
                rec = lat.evaluate("%s/latency_first_key_after_idle/%s" % (ctx.label, tag), first, second, budget, "overhead", plat(), ref_name="second key 0.3 s later (same session)",
                                   expected=n, notes=["key -> first frame from the app's input_order log (Phosphor-side); the control is the following non-idle key, so the figure is the idle penalty, not an overhead vs another terminal"])
                r.metrics.setdefault("lat", {})[tag] = rec
                lat_checks(r, rec, "First key after %g s idle" % g)
        return
    rect = ctx.be.window_rect()
    if not rect or rect[2] < 100:
        r.skip("no Phosphor window rect")
    ctx.be.guard()
    ref = ref_terminal(ctx)
    rrect = None

    def close_ref():
        ctx.be.guard_override = "Terminal"          # RefTerm.close types `exit` under the guard: it must be aimed at Terminal.app, never Phosphor
        ctx.be._focus_pid(pid_of("Terminal"))
        ref.close()
    if ref:
        time.sleep(2.5)
        rrect = ctx.be.window_rect(pid=pid_of("Terminal"))
        if not rrect or rrect[2] < 100:
            r.note("no reference: absolute only (first key after idle): no Terminal.app window rect")
            close_ref()
            ref, rrect = None, None
    else:
        r.note("no reference: absolute only (first key after idle): no reference terminal available%s" % (": " + ctx.ref_reason if getattr(ctx, "ref_reason", None) else ""))
    ph, rs = {g: [] for g in IDLE_GAPS}, {g: [] for g in IDLE_GAPS}
    try:
        for g, side, cnt, disc in lat.idle_schedule(IDLE_GAPS, n, IDLE_BLOCK, warm, two_sided=bool(ref)):
            if side == "reference":
                ctx.be.guard_override = "Terminal"
                ctx.be._focus_pid(pid_of("Terminal"))
                rs[g] += idle_paint_block(ctx, rrect, g, cnt)[disc:]
            else:
                ctx.be.guard_override = None
                ctx.be.refocus()
                ph[g] += idle_paint_block(ctx, rect, g, cnt)[disc:]
            ctx.reset()
    finally:
        if ref:
            close_ref()
    for g in IDLE_GAPS:
        tag = "gap%gs" % g
        r.metric("first_key_paint_" + tag, stats(ph[g]))
        if ref:
            r.metric("reference_first_key_paint_" + tag, stats(rs[g]))
        r.check(len(ph[g]) >= 0.9 * n, "idle %g s: only %d of %d first-key key->paint samples from kl3" % (g, len(ph[g]), n))
        if ph[g]:
            rec = lat.evaluate("%s/latency_first_key_after_idle/%s" % (ctx.label, tag), ph[g], rs[g] if ref else [], budget, "overhead", plat(),
                               ref_name=ref.name if ref and rs[g] else None, floor=ctx.be.key_floor_ms, expected=n,
                               pane_shell=pane_shell_id(ctx), ref_shell=getattr(ctx, "ref_shell", None) if ref else None,
                               mode="interleaved" if ref and rs[g] else "sequential", implausible_neg_ms=float(os.environ.get("APPCI_IMPLAUSIBLE_NEG_MS", "5")),
                               notes=["first key after %g s idle, kl3 post-to-first-changed-pixel at 240 Hz (%.2f ms quantum, not applied as a resolution veto: the SE of the median difference is); "
                                      "Phosphor and Terminal.app in blocks of %d, the first side alternating per gap" % (g, KL3_QUANTUM_MS, IDLE_BLOCK)])
            r.metrics.setdefault("lat", {})[tag] = rec
            lat_checks(r, rec, "First key after %g s idle" % g)


# ---------------------------------------------------------------- agent
@case("approval_card_pause", modes=("agent",), order=45, gui=True)
def c_approval(ctx, r):
    """Enter pressed >1 s after an approval card appears must NOT approve it (#1350/#1362). Needs a provider."""
    if not ctx.ollama:
        r.skip("no provider: configure APPCI_OLLAMA_URL (env or the machine-local ~/.config/phosphor-ci/ci.env) to run agent cases that need a model")
    if not shutil.which("tesseract"):
        r.skip("tesseract missing: cannot detect the approval card on screen")
    ctx.rm("ap.txt")
    ctx.be.type("Please run exactly this shell command and nothing else: echo APPROVAL_TOKEN >> ap.txt", 15, enter=True)
    seen = ctx.until(lambda: re.search(r"(?i)\b(reject|approve|run command|accept)\b", ctx.ocr("card") or ""), 120, step=3)
    if not seen:
        r.skip("no approval card appeared within 120 s (model did not propose the command)")
    time.sleep(1.5)
    ctx.be.key("Return")
    time.sleep(2)
    r.check(not ctx.lines("ap.txt"), "Enter after a >1 s pause approved the card (command ran)")
    ctx.shot("card_after_enter")
    ctx.be.key("ctrl+c")


# ---------------------------------------------------------------- tabs and windows (last: leaves the app busy)
# ---------------------------------------------------------------- code editor memory (#1340)
def editor_mem_file(path, kind, target_bytes):
    """a generated ~7 MB source file: 'txt' = plain prose-like lines (~85k lines), 'rs' = Rust-looking code with long lines"""
    words = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega".split()
    rng = random.Random(1340)
    size, i = 0, 0
    with open(path, "w") as f:
        while size < target_bytes:
            if kind == "rs":
                a, b = rng.choice(words), rng.choice(words)
                line = ("    pub fn %s_%d(&self, %s: &str, count: usize) -> Result<Vec<String>, Error> { let mut out = Vec::with_capacity(count); "
                        "for n in 0..count { out.push(format!(\"{}-{}\", %s, n * %d)); } Ok(out) } // %s %s\n" % (a, i, b, b, i % 97, a, b))
            else:
                line = "line %d: %s\n" % (i, " ".join(rng.choice(words) for _ in range(rng.randint(6, 14))))
            f.write(line)
            size += len(line)
            i += 1


def mac_footprint_kib(pid):
    """macOS: (phys_footprint, resident) of `pid` in KiB from libproc's proc_pid_rusage(RUSAGE_INFO_V0), or None. phys_footprint is the
    figure Activity Monitor shows as Memory. `ps` RSS also counts pages malloc already freed (MADV_FREE_REUSABLE) until the kernel takes them
    back, so it swings with system memory pressure: r18e4's Rust half read RSS -110 MB while a 7 MB file opened (#1607). rusage_info_v0:
    uuid[16], then u64 user_time, system_time, pkg_idle_wkups, interrupt_wkups, pageins, wired_size, resident_size (offset 64),
    phys_footprint (offset 72). The buffer is oversized so a newer struct version can never overrun it."""
    import ctypes, struct
    for name in (None, "/usr/lib/libSystem.B.dylib", "/usr/lib/system/libsystem_kernel.dylib"):
        try:
            fn = ctypes.CDLL(name).proc_pid_rusage
        except (OSError, AttributeError):
            continue
        buf = ctypes.create_string_buffer(512)
        try:
            if fn(ctypes.c_int(int(pid)), ctypes.c_int(0), buf) != 0:
                return None
        except (TypeError, ValueError, OSError):
            return None
        resident, footprint = struct.unpack_from("<QQ", buf.raw, 64)
        return footprint // 1024, resident // 1024
    return None


def mem_sample(ctx):
    """{metric: MB} of the app process: 'rss' everywhere (Windows: working set), plus 'phys_footprint' on macOS when libproc answers
    (see mac_footprint_kib). Owner 2026-11-03: on macOS BOTH are judged; either one over a budget fails the case."""
    out = {"rss": ctx.app.cpu_ticks()[1] / 1024.0}
    if ctx.be.name == "mac":
        pid = ctx.app.pid()
        fp = mac_footprint_kib(pid) if pid else None
        if fp:
            out["phys_footprint"] = fp[0] / 1024.0
    return out


def rss_mb(ctx):
    return mem_sample(ctx)["rss"]


def settle_rss(ctx, quiet_s=3.0, timeout=150):
    """wait until every memory figure has stayed within +-2 MB for quiet_s seconds (file load is asynchronous and slow); returns mem_sample"""
    t0 = time.time()
    last, since = mem_sample(ctx), time.time()
    while time.time() - t0 < timeout:
        time.sleep(0.75)
        cur = mem_sample(ctx)
        if any(abs(cur[k] - last.get(k, cur[k])) > 2.0 for k in cur):
            last, since = cur, time.time()
        elif time.time() - since >= quiet_s:
            break
    return mem_sample(ctx)


def mem_metrics(r, prefix, sample, base=None):
    """record every figure of a mem_sample: <prefix>_mb (RSS, the historical name), <prefix>_<metric>_mb for the others, and *_delta_mb vs base"""
    for k, v in sample.items():
        name = "%s_mb" % prefix if k == "rss" else "%s_%s_mb" % (prefix, k)
        r.metric(name, v)
        if base is not None and k in base:
            r.metric(name[:-3] + "_delta_mb", v - base[k])


def mem_judge(r, sample, base, limit, what):
    """one check per figure (owner: macOS judges RSS AND phys_footprint): `what % (delta, metric)` describes a failure"""
    ok = True
    for k, v in sample.items():
        if k in base:
            ok &= r.check(v - base[k] <= limit, what % (v - base[k], k))
    return ok


def editor_close(ctx, r):
    """close the focused code pane; returns how ('key ...' / 'palette ...') or None when this platform has no usable way. APPCI_KEY_CLOSE first
    (Linux/Windows ctrl+shift+w: PaneGroupAction::RemoveActive, `pane_group:close_current_session`). macOS has no usable chord (Phosphor's is
    cmd-shift-W, which the frontmost-app guard refuses), so there the COMMAND palette (APPCI_KEY_CMD_PALETTE, mac cmd+p: CustomAction::CommandPalette
    = cmd_or_ctrl_shift("p"); cmd+shift+p is the navigation palette) runs the SAME action by its palette name APPCI_CLOSE_PALETTE_TEXT
    ('Close Current Session', app/i18n/en/warp.ftl keybinding-desc-pane-group-close-current-session). The caller proves a palette close by the
    app's buffer_dropped log line (Return could have run a different item)."""
    close = os.environ.get("APPCI_KEY_CLOSE", "ctrl+shift+w")
    if close:
        try:
            ctx.be.key(close)
            return "key " + close
        except (RuntimeError, ValueError) as e:  # the macOS guard refuses some chords
            r.note("close key %r refused by the backend (%s); trying the command palette" % (close, e))
    pal = os.environ.get("APPCI_KEY_CMD_PALETTE", "")
    text = os.environ.get("APPCI_CLOSE_PALETTE_TEXT", "Close Current Session")
    if not pal or not text:
        return None
    try:
        ctx.be.key(pal)
        time.sleep(1.5)
        ctx.be.type(text, 12)
        time.sleep(1.2)
        ctx.be.key("Return")
    except (RuntimeError, ValueError) as e:
        r.note("command palette close (%s, %r) refused by the backend: %s" % (pal, text, e))
        ctx.be.key("escape")
        return None
    return "palette %s + %r" % (pal, text)


def editor_open(ctx, palette, path):
    ctx.be.key(palette)
    time.sleep(1.5)
    ctx.be.type(path, 12)
    time.sleep(1.0)
    ctx.be.key("Return")   # the keysym name every backend maps (wtype refused "enter": r18e0)


def editor_mem_log(ctx, off):
    """the app's own breakdown lines (target editor_memory, debug: app/src/code/editor/model.rs after_large_layout and the buffer-drop log in
    global_buffer_model.rs) written since log offset `off`, as {'open': [dict...], 'dropped': [dict...]}"""
    out = {"open": [], "dropped": []}
    if not getattr(ctx, "log_path", None):
        return out
    for line in ctx.log_since(off).splitlines():
        m = re.search(r"editor_memory (open|buffer_dropped) (.*)$", line)
        if not m:
            continue
        kv = dict(re.findall(r"(\w+)=(\S+)", m.group(2)))
        out["open" if m.group(1) == "open" else "dropped"].append({k: (int(v) if v.isdigit() else v) for k, v in kv.items()})
    return out


def ui_items(text):
    """[(epoch_s, ms, kind)] of the app's `input_order hop=ui_item kind=<frame|event|action> ms=<ms> ... t=<epoch ms>` lines
    (input_order::log_ui_item_if_over_budget: main-thread items at or over the 4 ms budget). Same regex as ui_item_audit, plus each line's t="""
    out = []
    for line in text.splitlines():
        m = _UI_ITEM_RE.search(line)
        t = re.search(r" t=(\d+(?:\.\d+)?)", line) if m else None
        if m and t:
            out.append((float(t.group(1)) / 1000.0, float(m.group(2)), m.group(1)))
    return out


def ui_window_max(items, t0, seconds=5.0, after_largest=False, t_end=None):
    """(max ms, count >= 4 ms budget, skipped load-frame ms) over the UI items in [t0, min(t0 + seconds, t_end)]. after_largest: the window
    starts after the largest item in that window (the load's own layout frame after an open). t_end bounds the window so items of a later phase
    (the close) can neither enter it nor be taken for the load frame. A trim's own stall is judged separately (ui_trim_max), since it can be the
    largest item."""
    end = t0 + seconds if t_end is None else min(t0 + seconds, t_end)
    xs = sorted(i for i in items if t0 <= i[0] <= end)
    skipped = None
    if after_largest and xs:
        big = max(xs, key=lambda i: i[1])
        skipped = big[1]
        xs = [i for i in xs if i[0] > big[0]]
    return (max((i[1] for i in xs), default=0.0), sum(1 for i in xs if i[1] >= UI_BUDGET_MS), skipped)


_HEAP_TRIM_RE = re.compile(r"input_order hop=heap_trim why=(\w+) ms=(\d+(?:\.\d+)?)")


def heap_trims(text):
    """[(end epoch_s, ms, why)] of the app's `input_order hop=heap_trim why=<open|close> ms=<ms> t=<end>` lines (app/src/alloc.rs
    trim_free_memory_timed: one per heap trim, logged when it finishes)"""
    out = []
    for line in text.splitlines():
        m = _HEAP_TRIM_RE.search(line)
        t = re.search(r" t=(\d+(?:\.\d+)?)", line) if m else None
        if m and t:
            out.append((float(t.group(1)) / 1000.0, float(m.group(2)), m.group(1)))
    return out


def ui_trim_max(items, trims, why, pad_s=0.25):
    """max ms of the UI items (each logged when it ENDS) that ended while a `why` trim ran, or within pad_s after it: [end - ms, end + pad_s].
    0.0 when none did (no UI item over 4 ms during the trim)"""
    worst = 0.0
    for end, ms, w in trims:
        if w != why:
            continue
        lo, hi = end - ms / 1000.0, end + pad_s
        worst = max([worst] + [i[1] for i in items if lo <= i[0] <= hi])
    return worst


UI_BUDGET_MS = 4.0     # owner perf budget: UI work <= 4 ms (warn 16)


def editor_cycle_growth(close_mb):
    """MB the post-close figure grew from the first open/close cycle to the last (#1407: 'repeated open/close grows to ~2 GB')"""
    return (close_mb[-1] - close_mb[0]) if len(close_mb) >= 2 else 0.0


def editor_wait_dropped(ctx, off, timeout=15.0):
    """the buffer_dropped lines since `off`, waiting up to `timeout` for the first (the drop is event-driven but follows the close's effects)"""
    t0 = time.time()
    while True:
        d = editor_mem_log(ctx, off)["dropped"]
        if d or time.time() - t0 >= timeout:
            return d
        time.sleep(0.5)


def editor_mem_relaunch(ctx, env):
    """restart the app with `env` set ({} restores the default): Linux/Windows inherit os.environ, macOS gets the keys through
    app.PASSTHROUGH_ENV. Returns whether the new instance reached a prompt."""
    for k in ("PHOSPHOR_EDITOR_TRIM_AFTER_OPEN", "PHOSPHOR_HEAP_TRIM"):
        os.environ.pop(k, None)
    os.environ.update(env)
    n = ctx.prompts()
    ctx.snapshot_log()                # injector accounting spans the relaunch (r18e9)
    ctx.app.stop()
    time.sleep(1.0)
    ctx.app.start()
    up = ctx.until(lambda: ctx.prompts() > n, 90, step=0.5)
    time.sleep(3)
    ctx.reset()
    return bool(up)


def editor_trim_ab(ctx, r, palette, path):
    """#1607 perf rule: UI-thread cost of the heap trims, A/B by relaunch. One open/close of the plain-text file per variant: default (trim on
    close only), PHOSPHOR_EDITOR_TRIM_AFTER_OPEN=1 (opt-in trim after the load) and PHOSPHOR_HEAP_TRIM=0 (no trim at all, the control for the
    close-time trim, which is new on macOS). Per variant: the max hop=ui_item (main's log_ui_item_if_over_budget, items >= 4 ms; kind frame|event|action) in the 5 s after the load's layout frame and in the 5 s after the
    close key, against the 4 ms budget. A trim variant fails only if it goes over the budget AND more than 1 ms over its control, so a build that
    is already over budget there is reported (metrics, notes) without blaming the trim."""
    if not getattr(ctx, "log_path", None):
        r.note("trim A/B not run: no app log (hop=ui_item is read from it)")
        return
    variants = (("default", {}), ("trim_after_open", {"PHOSPHOR_EDITOR_TRIM_AFTER_OPEN": "1"}), ("no_trim", {"PHOSPHOR_HEAP_TRIM": "0"}))
    res = {}
    try:
        for name, env in variants:
            if not editor_mem_relaunch(ctx, env):
                r.note("trim A/B %s: the relaunched app reached no prompt; A/B incomplete" % name)
                break
            off = ctx.log_size()
            settle_rss(ctx)
            t_open = time.time()
            editor_open(ctx, palette, path)
            settle_rss(ctx)
            time.sleep(5.5)
            t_close = time.time()
            how = editor_close(ctx, r)
            if not how:
                r.note("trim A/B %s: no way to close the pane on this platform; A/B incomplete" % name)
                break
            time.sleep(5.5)
            txt = ctx.log_since(off)
            items, trims = ui_items(txt), heap_trims(txt)
            if not items:
                r.note("trim A/B %s: no hop=ui_item line (nothing over 4 ms, a build older than the ui_item hop, or input_order logging off)" % name)
            o_max, o_over, load_ms = ui_window_max(items, t_open, after_largest=True, t_end=t_close)
            c_max, c_over, _ = ui_window_max(items, t_close)
            res[name] = dict(open_max_ms=o_max, open_over=o_over, load_frame_ms=load_ms, close_max_ms=c_max, close_over=c_over,
                             trim_open_max_ms=ui_trim_max(items, trims, "open"), trim_close_max_ms=ui_trim_max(items, trims, "close"),
                             trims=[(w, ms) for _, ms, w in trims], audit=ui_item_audit(txt))
            ctx.reset()
    finally:
        if not editor_mem_relaunch(ctx, {}):
            r.check(False, "trim A/B: the app reached no prompt after relaunching with the default environment")
    r.metric("trim_ab", res)
    if "trim_after_open" in res and not any(w == "open" for w, _ in res["trim_after_open"]["trims"]):
        r.check(False, "trim A/B: PHOSPHOR_EDITOR_TRIM_AFTER_OPEN=1 logged no 'heap_trim why=open' (the opt-in trim did not run, or the build predates it)")
    # each trim is judged by the UI items that ran DURING it, against the same phase of the variant without that trim
    for name, ctrl, key, ctrl_key, what in (("trim_after_open", "default", "trim_open_max_ms", "open_max_ms", "the open trim"),
                                           ("default", "no_trim", "trim_close_max_ms", "close_max_ms", "the close trim")):
        if name not in res or ctrl not in res:
            continue
        got, base = res[name][key], res[ctrl][ctrl_key]
        if base > UI_BUDGET_MS:
            r.note("trim A/B: the control %s is itself over the %.0f ms UI budget after %s (%.1f ms)" % (ctrl, UI_BUDGET_MS, ctrl_key.split("_")[0], base))
        r.check(not (got > UI_BUDGET_MS and got > base + 1.0), "trim A/B: %s put a %.1f ms UI item on screen while it ran (> %.0f ms budget; control %s: %.1f ms)" % (
            what, got, UI_BUDGET_MS, ctrl, base))


@case("editor_memory_7mb", modes=("terminal",), order=85, gui=True)
def c_editor_mem(ctx, r):
    """#1340 / #1607 / #1407: open a generated 7 MB file in the code editor (command palette + absolute path), record memory after open, close
    the pane, record memory right after close and after the undo-close grace window, and repeat open/close APPCI_EDITOR_MEM_CYCLES times
    (default 3, plain-text file only) to measure growth per cycle. Thresholds are the #1340 targets (7 MB plain text <= ~100 MB over baseline,
    7 MB Rust <= ~150 MB, <= 60 MB held after close, APPCI_EDITOR_MEM_CYCLE_GROWTH_MAX_MB = 30 MB growth from the first to the last cycle).
    Judged: RSS (Windows: working set) everywhere, and on macOS ALSO phys_footprint (owner 2026-11-03: either over a budget fails; both are
    reported). A close through the command palette must be followed by the app's buffer_dropped log line. The app's own breakdown (log target
    editor_memory: buffer tree, render tree, parked chunks, hydrated frames) is attached as metrics. Then the heap-trim UI A/B (editor_trim_ab;
    APPCI_EDITOR_MEM_TRIM_AB=0 skips it). Skips (APPCI_EDITOR_MEM_STRICT=1: fails) when the file does not open, since the palette / close key
    sequence is platform specific: APPCI_KEY_PALETTE (default ctrl+shift+p; mac cmd+p), APPCI_KEY_CLOSE (default ctrl+shift+w; empty on macOS)
    and, where the close key is empty, APPCI_KEY_CMD_PALETTE + APPCI_CLOSE_PALETTE_TEXT (mac: cmd+p, 'Close Current Session')."""
    try:
        mem_sample(ctx)
    except Exception as e:  # noqa: BLE001 - no ps / no pid on this platform
        r.skip("process memory is not readable on this backend: %s" % e)
    palette = os.environ.get("APPCI_KEY_PALETTE", "ctrl+shift+p")
    grace = float(os.environ.get("APPCI_UNDO_CLOSE_WAIT_S", "75"))   # general.undo_close.grace_period is 60 s
    strict = os.environ.get("APPCI_EDITOR_MEM_STRICT", "") not in ("", "0")
    limits = {"txt": float(os.environ.get("APPCI_EDITOR_MEM_TXT_MAX_MB", "100")), "rs": float(os.environ.get("APPCI_EDITOR_MEM_RS_MAX_MB", "150"))}
    close_max = float(os.environ.get("APPCI_EDITOR_MEM_CLOSE_MAX_MB", "60"))
    growth_max = float(os.environ.get("APPCI_EDITOR_MEM_CYCLE_GROWTH_MAX_MB", "30"))
    cycles = max(1, int(os.environ.get("APPCI_EDITOR_MEM_CYCLES", "3")))
    kinds = [k for k in os.environ.get("APPCI_EDITOR_MEM_KINDS", "txt,rs").split(",") if k in limits]
    size = int(float(os.environ.get("APPCI_EDITOR_MEM_MB", "7")) * 1024 * 1024)
    ctx.reset()
    opened_any = False
    metrics = sorted(mem_sample(ctx))
    r.metric("mem_metrics", metrics)
    if len(metrics) > 1:
        r.note("judged memory figures: %s (owner: each one is checked; macOS ps RSS also counts freed-but-reusable malloc pages)" % ", ".join(metrics))
    ab_path = None
    for kind in kinds:
        path = os.path.join(ctx.W, "editor_mem_%s.%s" % (kind, "rs" if kind == "rs" else "txt"))
        editor_mem_file(path, kind, size)
        n_cycles = cycles if kind == kinds[0] else 1
        base = settle_rss(ctx)
        mem_metrics(r, "%s_rss_base" % kind, base)
        opens, closes, how = [], [], None
        for cyc in range(n_cycles):
            off = ctx.log_size() if getattr(ctx, "log_path", None) else 0
            pre = settle_rss(ctx) if cyc else base
            editor_open(ctx, palette, path)
            opened = settle_rss(ctx)
            tag = kind if cyc == 0 else "%s_c%d" % (kind, cyc + 1)
            ctx.shot("editor_mem_%s_open" % tag)
            mem_metrics(r, "%s_rss_open" % tag, opened, pre)
            grew = max(opened[k] - pre[k] for k in opened if k in pre)
            if grew < 8:
                msg = "%s: memory grew only %.1f MB after opening %s through the palette (%s): the file did not open; set APPCI_KEY_PALETTE" % (tag, grew, path, palette)
                if strict:
                    r.check(False, msg)
                else:
                    r.note("SKIPPED " + msg)
                ctx.be.key("escape")
                ctx.reset()
                break
            opened_any = True
            opens.append(opened)
            if cyc == 0:
                mem_judge(r, opened, pre, limits[kind], "%s: opening a %d MB file cost %%.0f MB of %%s (> %.0f MB target)" % (kind, size >> 20, limits[kind]))
            how = editor_close(ctx, r)
            if not how:
                r.note("%s: release on close NOT MEASURED: no close key and no command-palette close on this platform (APPCI_KEY_CLOSE / APPCI_KEY_CMD_PALETTE empty)" % kind)
                ctx.reset()
                break
            time.sleep(1.0)
            ctx.shot("editor_mem_%s_after_close" % tag)
            dropped = editor_wait_dropped(ctx, off) if getattr(ctx, "log_path", None) else []
            if how.startswith("palette") and getattr(ctx, "log_path", None) and not dropped:
                r.check(False, "%s: the palette close dropped no buffer: Return ran a different palette item or the editor is still referenced (#1407)" % tag)
            elif getattr(ctx, "log_path", None) and not dropped:
                r.note("%s: no 'editor_memory buffer_dropped' line in the app log after close (the buffer may still be referenced, #1407)" % tag)
            closed = settle_rss(ctx, quiet_s=4.0, timeout=60)
            closes.append(closed)
            mem_metrics(r, "%s_rss_closed_now" % tag, closed, base)
            br = editor_mem_log(ctx, off)
            if br["open"]:
                r.metric("%s_app_breakdown_open" % tag, br["open"][-1])
            if dropped:
                r.metric("%s_app_buffer_dropped" % tag, dropped[-1])
            ctx.reset()
        if not opens or not how:
            continue
        r.metric("%s_close_method" % kind, how)
        if n_cycles > 1 and len(closes) == n_cycles:
            for k in sorted(closes[0]):
                series = [c[k] for c in closes if k in c]
                growth = editor_cycle_growth(series)
                sfx = "" if k == "rss" else "_" + k
                r.metric("%s_cycle_close%s_mb" % (kind, sfx), [round(x, 1) for x in series])
                r.metric("%s_cycle_open%s_mb" % (kind, sfx), [round(o[k], 1) for o in opens if k in o])
                r.metric("%s_cycle_growth%s_mb" % (kind, sfx), growth)
                r.check(growth <= growth_max, "%s: %d open/close cycles grew the post-close %s by %.0f MB (> %.0f MB): %s" % (
                    kind, n_cycles, k, growth, growth_max, ", ".join("%.0f" % x for x in series)))
        time.sleep(grace)
        after = settle_rss(ctx)
        mem_metrics(r, "%s_rss_close" % kind, after, base)
        ctx.shot("editor_mem_%s_closed" % kind)
        mem_judge(r, after, base, close_max, "%s: %%.0f MB of %%s still held %.0f s after closing the editor (> %.0f MB over the pre-open baseline; close via %s)" % (
            kind, grace, close_max, how))
        if kind == "txt":
            ab_path = path
        else:
            try:
                os.remove(path)
            except OSError:
                pass
        ctx.reset()
    if not opened_any and not strict:
        r.skip("the generated file never opened in the code editor on this backend (see notes)")
    if ab_path and os.environ.get("APPCI_EDITOR_MEM_TRIM_AB", "1") not in ("", "0"):
        editor_trim_ab(ctx, r, palette, ab_path)
    if ab_path:
        try:
            os.remove(ab_path)
        except OSError:
            pass
    r.check(ctx.app.alive(), "app died during the editor memory case")


@case("gitbash_subshell", order=22)
def c_gitbash_subshell(ctx, r):
    """Windows (#1464): Git for Windows' bash.exe started from the pane's PowerShell as a subshell: 20 fast-typed lines, 4 pasted lines of 10-5000
    bytes with an instant Enter, 3 Ctrl-C of a trapped `sleep 30`, `exit` back to PowerShell and the next PowerShell command runs there."""
    if not ctx.win:
        r.skip("Windows only: Git for Windows' bash.exe as a subshell of the pane's PowerShell (POSIX hosts: the subshells case)")
    if ctx.pane != "powershell":
        r.skip("the pane IS Git Bash in this pass; bash inside bash is covered by ctrl_c_d / subshells")
    import winapi
    gb = winapi.git_bash()
    if not gb:
        r.skip("Git for Windows not installed (no bash.exe under Program Files\\Git\\bin)")
    names = ("gb_probe", "gb_ft", "gb_pe", "gb_cc", "gb_back")
    ctx.rm(*[n + ".txt" for n in names])
    ctx.be.type("& '%s' --noprofile --norc -i" % gb.replace("\\", "/"), 12, enter=True)
    time.sleep(4)
    ready = False
    for _ in range(4):
        ctx.be.type("echo probe >> gb_probe.txt", 12, enter=True)
        if ctx.until(lambda: ctx.lines("gb_probe.txt"), 4):
            ready = True
            break
    ctx.shot("gitbash_started")
    if not ready:
        r.check(False, "Git Bash never executed a probe line (cwd %s)" % ctx.W)
        ctx.reset()
        return
    rl = lambda n: [l.rstrip("\r") for l in ctx.lines(n)]       # noqa: E731
    exp = []
    for _ in range(20):
        tok = ctx.tok(8).lower()
        exp.append(tok)
        ctx.be.type("echo %s >> gb_ft.txt" % tok, ctx.rng.randint(10, 25), enter=True)
    ctx.until(lambda: len(rl("gb_ft.txt")) >= 20, 30)
    r.check(rl("gb_ft.txt") == exp, "Git Bash fast typing: %d/20 lines, exact match false (expected first=%s got first=%s)" % (len(rl("gb_ft.txt")), exp[0], (rl("gb_ft.txt") or [None])[0]))
    exp = []
    for i in range(4):
        tok, pad = ctx.tok(8).lower(), "w" * (10, 500, 5000, 50)[i]
        exp.append(tok + pad)
        ctx.be.clip_set("echo %s%s >> gb_pe.txt" % (tok, pad))
        ctx.be.paste(enter=True)
        time.sleep(0.9)
    ctx.until(lambda: len(rl("gb_pe.txt")) >= 4, 20)
    r.check(rl("gb_pe.txt") == exp, "Git Bash paste + instant Enter: %d/4 exact lines" % sum(a == b for a, b in zip(exp, rl("gb_pe.txt"))))
    c = 0
    for i in range(3):
        ctx.be.type("bash -c 'trap \"echo INT%d >> gb_cc.txt; exit\" INT; sleep 30'; echo AFTER%d >> gb_cc.txt" % (i, i), 12, enter=True)
        time.sleep(1.8)
        ctx.be.key("ctrl+c")
        if ctx.until(lambda: len(rl("gb_cc.txt")) >= 2 * (i + 1), 8):
            c += 1
    r.check(c == 3, "Git Bash Ctrl-C: the trap and the next line ran %d/3 times: %s" % (c, ",".join(rl("gb_cc.txt"))))
    ctx.be.type("exit", 12, enter=True)
    time.sleep(2)
    tok = ctx.tok(8).lower()
    ctx.be.type("Add-Content gb_back.txt %s" % tok, 12, enter=True)
    ctx.until(lambda: rl("gb_back.txt"), 8)
    r.check(rl("gb_back.txt") == [tok], "after exiting Git Bash the pane's PowerShell did not run the next command: %s" % rl("gb_back.txt"))


def stream_cmd(ctx, i):
    """a command that prints a line every ~20 ms until Ctrl-C, for the pane's own shell"""
    if ctx.pane == "powershell":
        return "while ($true) { 'streaming_%d_line'; Start-Sleep -Milliseconds 20 }" % i
    return "while :; do echo streaming_%d_line; sleep 0.02; done" % i


@case("tabs_streaming", order=91, gui=True)
def c_tabs_streaming(ctx, r):
    """12 tabs each streaming a line every 20 ms, then ONE more tab: 10 fast-typed commands in it arrive exactly, in order, and the app survives
    (the Windows tabs_streaming case of app_test.ps1, now on every platform; tabs_two_windows adds a second window and a paste)"""
    ntabs = int(os.environ.get("APPCI_TABS", "12"))
    newtab = os.environ.get("APPCI_KEY_NEWTAB", "ctrl+shift+t")
    ctx.reset()
    for i in range(ntabs):
        ctx.be.key(newtab)
        time.sleep(0.7)
        ctx.be.type(stream_cmd(ctx, i), 10, enter=True)
    ctx.rm("tw_s.txt")
    ctx.be.key(newtab)
    time.sleep(1.0)
    exp = []
    for _ in range(ctx.count(10)):
        tok = ctx.tok(8).lower()
        exp.append(tok)
        ctx.be.type(ctx.append_cmd(ctx.pane_shell(), tok, "tw_s.txt"), ctx.rng.randint(10, 25), enter=True)
    ctx.until(lambda: len(ctx.lines("tw_s.txt")) >= len(exp), 40)
    time.sleep(0.5)
    ctx.shot("tabs_streaming")
    r.check(ctx.app.alive(), "app died with %d streaming tabs" % ntabs)
    got = ctx.lines("tw_s.txt")
    r.check(got == exp, "typing under %d streaming tabs: %d/%d exact lines" % (ntabs, len(got), len(exp)))


@case("tabs_two_windows", order=90, gui=True)
def c_tabs(ctx, r):
    ntabs = int(os.environ.get("APPCI_TABS", "12"))
    newwin = os.environ.get("APPCI_KEY_NEWWIN", "ctrl+shift+n")
    newtab = os.environ.get("APPCI_KEY_NEWTAB", "ctrl+shift+t")
    ctx.reset()
    for i in range(ntabs):
        ctx.be.key(newtab)
        time.sleep(0.5)
        ctx.be.type(stream_cmd(ctx, i), 10, enter=True)
        time.sleep(0.2)
    ctx.shot("tabs_open")
    alive = ctx.app.alive()
    r.check(alive, "app died while opening %d streaming tabs" % ntabs)
    ctx.be.key(newwin)
    time.sleep(3)
    w = ctx.be.windows()
    r.metric("windows", len(w))
    if w:
        r.check(len(w) >= 2, "second window did not open (%d windows; key %s)" % (len(w), newwin))
    ctx.rm("tw.txt")
    exp = []
    mark = ctx.cmd_mark()
    for i in range(ctx.count(15)):
        tok = ctx.tok(8).lower()
        exp.append(tok)
        ctx.be.type(ctx.append_cmd(ctx.pane_shell(), tok, "tw.txt"), ctx.rng.randint(10, 25), enter=True)
    ctx.be.clip_set(ctx.append_cmd(ctx.pane_shell(), "PASTED_ok", "tw.txt"))
    ctx.be.paste(enter=True)
    exp.append("PASTED_ok")
    ctx.until(lambda: len(ctx.lines("tw.txt")) >= len(exp), 30)
    r.check(ctx.lines("tw.txt") == exp, "typing/paste in window 2 under %d streaming tabs: %d/%d exact lines" % (ntabs, len(ctx.lines("tw.txt")), len(exp)))
    ctx.shot("two_windows")
    r.check(ctx.app.alive(), "app not alive at the end of the tabs/windows case")


# ---------------------------------------------------------------- markdown / mermaid viewer (#1460)
MD_SMALL = """# PhosphorMdProbe

Intro paragraph with `inline code` and a [link](https://example.com).

| Fruit | Count |
|-------|-------|
| ZEBRAAPPLE | 42 |
| QUINCEBERRY | 17 |

```mermaid
graph TD
    A[AlphaNodeStart] --> B[BetaNodeMiddle]
    B --> C[GammaNodeEnd]
```

Closing paragraph.
"""
MD_SMALL_TEXT = ("phosphormdprobe", "zebraapple", "quinceberry")           # heading + table cells (lower-cased, spaces stripped)
MD_SMALL_DIAGRAM = ("alphanodestart", "betanodemiddle", "gammanodeend")    # mermaid node labels, drawn from the SVG


def _squash(txt):
    return "".join((txt or "").lower().split())


def windows_file_url(path):
    """file:///C:/Users/x/a%20b.txt for a Windows path (what the shell's file-open hands the app as a startup URL)"""
    import pathlib
    return pathlib.PureWindowsPath(path).as_uri()


def open_file_in_windows_app(ctx, path):
    """#1544: Windows hand-over. `phosphor.exe <file:// url>` is the same startup-args path the OS file association uses: the second process takes the
    per-session single-instance mutex check (app_services/windows/single_instance_manager.rs, `Local\\Zap<Channel>_SingleInstance`), finds the
    sole running instance, sends the URLs over its named pipe (`pass_startup_args_to_existing_instance`, app_services/windows/mod.rs) and exits. The test
    build is the SAME channel and executable as the app under test, so it is that instance. If the hand-over does not happen the second process would
    be a second app on the same data dir: it is killed after 30 s (exact pid, tree) and reported. Returns a failure string or None."""
    env = dict(ctx.app.env()) if hasattr(ctx.app, "env") else dict(os.environ)
    p = subprocess.Popen([ctx.app.bin, windows_file_url(path)], env=env, cwd=os.path.dirname(ctx.app.bin), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    with open(os.path.join(ctx.run, "pids"), "a") as f:
        f.write("file-open %d\n" % p.pid)
    try:
        p.wait(timeout=30)
    except subprocess.TimeoutExpired:
        subprocess.run(["taskkill", "/PID", str(p.pid), "/T", "/F"], capture_output=True, timeout=30)
        return "the forwarding invocation did not exit within 30 s (killed): the build did not hand the file to the running instance, or a second app started"
    if not ctx.app.alive():
        return "the app under test is gone after the file hand-over (exit %s)" % getattr(ctx.app.proc, "returncode", None)
    return None


def open_file_in_app(ctx, path):
    """Hand a local file to the RUNNING app the way the OS would (file:// URL -> uri::open_file -> markdown viewer pane
    when code.editor.prefer_markdown_viewer, default true). Linux: a second invocation of the same binary inside the app's own
    D-Bus session forwards the URL to the running instance (app_services/linux). macOS: `open -a <bundle> <file>`
    (LaunchServices open-URL event to the running instance). Returns a failure string or None."""
    if ctx.be.name == "mac":
        r = subprocess.run(["open", "-a", ctx.app.bundle, path], capture_output=True, timeout=30)
        return None if r.returncode == 0 else "open -a failed: %s" % r.stderr.decode(errors="replace")[:200]
    if ctx.win:
        return open_file_in_windows_app(ctx, path)
    pid = ctx.app.pid()
    try:
        raw = open("/proc/%d/environ" % pid, "rb").read().split(b"\0")
    except Exception as e:
        return "cannot read the app environment (%r)" % (e,)
    env = dict(x.decode(errors="replace").split("=", 1) for x in raw if b"=" in x)
    if not env.get("DBUS_SESSION_BUS_ADDRESS"):
        return "app has no DBUS_SESSION_BUS_ADDRESS: cannot forward a second invocation"
    p = subprocess.Popen([ctx.app.bin, "file://" + path], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    with open(os.path.join(ctx.run, "pids"), "a") as f:
        f.write("md-open %d\n" % p.pid)
    try:
        p.wait(timeout=30)
    except subprocess.TimeoutExpired:
        os.killpg(p.pid, 9)          # exactly the helper we started, never by name
        return "the forwarding invocation did not exit within 30 s"
    return None


def wait_for_text(ctx, tag, needles, timeout):
    """Poll OCR until every needle (space-less, lower-case) is on screen. Returns (seconds, found_text) ; seconds None on timeout."""
    t0 = time.time()
    txt = ""
    while time.time() - t0 < timeout:
        got = ctx.ocr(tag)
        if got is None:
            return None, None
        txt = _squash(got)
        if all(n in txt for n in needles):
            return time.time() - t0, txt
        time.sleep(0.7)
    return None, txt


@case("md_viewer", order=80, gui=True)
def c_md_viewer(ctx, r):
    """Opens a small .md (table + mermaid flowchart) in the app's markdown viewer pane; screenshot proof, OCR of the table text
    and the diagram's node labels. Agent-mode AI answers are covered by md_viewer_ai_answer. Windows: not covered (harness is
    app_test.ps1; no OCR engine there)."""
    path = os.path.join(ctx.W, "probe.md")
    open(path, "w").write(MD_SMALL)
    off = ctx.log_size()
    err = open_file_in_app(ctx, path)
    r.check(err is None, "could not hand the file to the running app: %s" % err)
    if err:
        return
    time.sleep(2)
    secs, txt = wait_for_text(ctx, "md_small", MD_SMALL_TEXT, 20)
    if txt is None:
        ctx.shot("md_small_noocr")
        r.note("no OCR engine (tesseract / Apple Vision): screenshot shots/%s_md_small_noocr.png only, table text and diagram labels NOT machine-verified" % ctx.label)
    else:
        r.metric("md_small_text_visible_s", round(secs, 2) if secs is not None else None)
        r.check(secs is not None, "markdown viewer: heading/table text %s not visible within 20 s (OCR saw %r)" % (list(MD_SMALL_TEXT), txt[:300]))
        _, txt2 = wait_for_text(ctx, "md_small_diagram", MD_SMALL_DIAGRAM, 15)
        missing = [n for n in MD_SMALL_DIAGRAM if n not in (txt2 or "")]
        r.check(not missing, "mermaid flowchart node labels not visible (blank / failed diagram block?): missing %s; OCR saw %r" % (missing, (txt2 or "")[:300]))
    new = ctx.log_since(off)
    r.check("panicked at" not in new, "app log shows a panic while opening the markdown file")
    r.check(ctx.app.alive(), "app died while opening the markdown file")


@case("md_viewer_big", modes=("terminal",), order=81, gui=True)
def c_md_viewer_big(ctx, r):
    """2 MB markdown file (full tier only; SMOKE keeps the small file): time until its first heading is on screen and the app's RSS growth."""
    if os.environ.get("APPCI_TIER") == "smoke":
        raise Skip("smoke tier: only the small file")
    rr = random.Random(11)
    out, size = ["# BIGDOCSTART", ""], 0
    i = 0
    while size < 2_000_000:
        i += 1
        para = " ".join("".join(rr.choice("abcdefghijklmnopqrstuvwxyz") for _ in range(rr.randint(2, 9))) for _ in range(rr.randint(20, 60)))
        out += ["## Section %d" % i, "", para, "", "- item %d a" % i, "- item %d b" % i, "", "| k | v |", "|---|---|", "| row%d | %d |" % (i, i), ""]
        size += len(para) + 120
    path = os.path.join(ctx.W, "big.md")
    open(path, "w").write("\n".join(out))
    r.metric("md_bytes", os.path.getsize(path))
    rss0 = ctx.app.cpu_ticks()[1]
    t0 = time.time()
    err = open_file_in_app(ctx, path)
    r.check(err is None, "could not hand the file to the running app: %s" % err)
    if err:
        return
    secs, txt = wait_for_text(ctx, "md_big", ("bigdocstart",), 90)
    if txt is None:
        raise Skip("no OCR engine: cannot time the render (screenshot only)")
    rss1 = ctx.app.cpu_ticks()[1]
    r.metric("md_big_render_s", round(time.time() - t0, 2) if secs is not None else None)
    r.metric("md_big_rss_delta_mb", round((rss1 - rss0) / 1024.0, 1))
    # Budgets are first guesses (no measurement yet): tighten after the first CI numbers (docs/testing.md).
    r.check(secs is not None, "2 MB markdown file: first heading not visible within 90 s")
    r.check((rss1 - rss0) / 1024.0 < 3000, "2 MB markdown file grew the app by %.0f MB RSS (budget 3000 MB)" % ((rss1 - rss0) / 1024.0))
    r.check(ctx.app.alive(), "app died rendering the 2 MB markdown file")


@case("md_viewer_ai_answer", modes=("agent",), order=82, gui=True)
def c_md_ai_answer(ctx, r):
    """Agent mode: ask the CI model for a markdown table + mermaid flowchart and look at the rendered answer. Needs the CI Ollama
    model (ctx.ollama); there is no fake-model path in the app. Model output is nondeterministic, so the OCR result is a NOTE and a
    metric, never a failure; a failure is only an app crash / panic."""
    if not ctx.ollama:
        raise Skip("no local/CI model configured (APPCI_OLLAMA_URL): no deterministic AI-answer path exists, so agent-mode markdown rendering is only covered by the unit corpus")
    off = ctx.log_size()
    ctx.be.type("Reply with only this markdown and nothing else: a table with header Fruit and Count and one row ZEBRAAPPLE and 42, then a fenced mermaid block containing graph TD with A[AlphaNodeStart] --> B[BetaNodeMiddle]", 12, enter=True)
    secs, txt = wait_for_text(ctx, "md_ai", ("zebraapple",), 120)
    if txt is None:
        raise Skip("no OCR engine: screenshot only")
    r.metric("ai_table_visible", secs is not None)
    _, txt2 = wait_for_text(ctx, "md_ai_diagram", ("alphanodestart",), 10)
    r.metric("ai_diagram_labels_visible", bool(txt2 and "alphanodestart" in txt2))
    if secs is None:
        r.note("model answer did not show ZEBRAAPPLE within 120 s (model nondeterminism or no answer); see shots/agent_md_ai.png")
    r.check("panicked at" not in ctx.log_since(off), "app log shows a panic while rendering the AI markdown answer")
    r.check(ctx.app.alive(), "app died rendering the AI markdown answer")
    ctx.be.key("ctrl+c")
import cases_caseagent  # noqa: E402,F401  (#1463: agent_stream_cancel, agent_approval, routing_table)
import cases_ui  # noqa: E402,F401  (#1462: completions/history, mouse/panes/resize, startup + live settings cases)
import cases_robust  # noqa: E402,F401  (#1468: session restore, quit with 24 tabs, floods, soak, code editor)
import cases_caseremote  # noqa: E402,F401  (issue #1464: remote work over ssh + per-shell variants)
import cases_caseintl  # noqa: E402,F401  (group 8: international input, #1466)
import cases_coverage_a  # noqa: E402,F401  (#1535-#1541: paste, alt-screen programs, sudo/su, ssh drop+reconnect, kill -9 + corrupt-db restore)
import cases_coverage_b  # noqa: E402,F401  (g10: CI-review coverage gaps #1542-#1548: agent pickers, settings UI/migration, state relaunch, no-autoupdate, UI-action budgets, mac editor)
import cases_shells  # noqa: E402,F401  (#1534: startup_shell_override passes: cmd.exe and pwsh 7 as the pane shell)
