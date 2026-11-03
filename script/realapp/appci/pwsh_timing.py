# appci/pwsh_timing.py (#1401, #1402, #1280): PowerShell's own hook timing marks, placed on the app's timeline.
#
# The app's input_order log ends at the pty: `pwsh_command_via_pipe` (the insert chord about to be written) and `preexec_received` (the Preexec OSC
# parsed). Everything between, and from the command's end to the next prompt, happens inside PowerShell/ConPTY and was not attributable (r18e4:
# write -> preexec 15.8 ms, preexec -> command start ~30 ms, nothing finer). With PHOSPHOR_PWSH_HOOK_TIMING=<file> in the app's environment and
# `<file>.on` present, pwsh.ps1 appends `<mark> <Stopwatch ticks>` lines (marks: see the comment at $script:warpDiagPath in pwsh.ps1):
#   k0 k1 k2   the insert chord's handler: start, pipe read, before AcceptLine
#   r          PSReadLine returned the line        p   Preexec OSC written        q   Preexec done (the command starts after this)
#   c          prompt function entered             m   Precmd sent                x   prompt function done
#   i          PSConsoleHostReadLine entered       e   LineEditorReading sent
# Stopwatch.GetTimestamp() is QueryPerformanceCounter; the app logs the same counter as `qpc=<ticks> qpf=<freq>` on those two hops, next to its
# epoch `t=`, so every app line with both is a calibration pair. Pure functions: no I/O beyond what the caller passes in.
import re
import statistics

_QPC = re.compile(r" qpc=(-?\d+) qpf=(\d+)")
_T = re.compile(r" t=(\d+(?:\.\d+)?)")


def parse_marks(text):
    """-> (frequency or None, [(name, ticks)]) in file order"""
    freq, marks = None, []
    for line in (text or "").splitlines():
        parts = line.split()
        if len(parts) != 2:
            continue
        if parts[0] == "freq":
            try:
                freq = int(parts[1])
            except ValueError:
                pass
            continue
        try:
            marks.append((parts[0], int(parts[1])))
        except ValueError:
            continue
    return freq, marks


def calibration(lines):
    """app log lines -> ms to add to a QPC time (ticks * 1000 / freq) to get epoch ms (median over every line carrying both t= and qpc=), or None"""
    offs = []
    for l in lines:
        q, t = _QPC.search(l), _T.search(l)
        if q and t and int(q.group(2)) > 0:
            offs.append(float(t.group(1)) - int(q.group(1)) * 1000.0 / int(q.group(2)))
    return statistics.median(offs) if offs else None


def app_times(lines, hop):
    """epoch ms of every app log line with `hop=<hop>`"""
    out = []
    for l in lines:
        if "hop=%s " % hop in l:
            t = _T.search(l)
            if t:
                out.append(float(t.group(1)))
    return out


def _first_at_or_after(ts, t, within):
    c = [x for x in ts if t <= x <= t + within]
    return min(c) if c else None


def _last_at_or_before(ts, t, within):
    c = [x for x in ts if t - within <= x <= t]
    return max(c) if c else None


def segments(marks_text, lines, stamps_ms=(), t0_ms=0.0):
    """The per-hop split of each Phosphor command (medians in ms, plus _n counts), {} when there are no usable marks or no calibration.
      app_write_to_handler   app: chord about to be written -> k0 (ConPTY input + PSReadLine reading the key)
      handler_pipe_read      k0 -> k1        handler_insert   k1 -> k2        accept_line   k2 -> r (AcceptLine render, history)
      readline_to_preexec    r -> p (Preexec JSON + OSC write)                   preexec_osc_to_app   p -> app preexec_received (ConPTY output)
      preexec_tail           p -> q (window title)                               hook_to_command_start   q -> the command's own stamp (epoch ms)
      prompt_precmd          c -> m          prompt_render    m -> x             prompt_to_readline   x -> i
      readline_maintenance   i -> e (Warp-Idle-Maintenance + LineEditorReading)
    stamps_ms: the commands' own start stamps (epoch ms), optional. t0_ms: ignore marks before this epoch time."""
    freq, marks = parse_marks(marks_text)
    off = calibration(lines)
    if not freq or off is None or not marks:
        return {}
    ms = lambda ticks: ticks * 1000.0 / freq + off          # noqa: E731
    writes, preexecs = app_times(lines, "pwsh_command_via_pipe"), app_times(lines, "preexec_received")
    stamps = sorted(stamps_ms or ())
    seg = {k: [] for k in ("app_write_to_handler", "handler_pipe_read", "handler_insert", "accept_line", "readline_to_preexec",
                           "preexec_osc_to_app", "preexec_tail", "hook_to_command_start", "prompt_precmd", "prompt_render",
                           "prompt_to_readline", "readline_maintenance")}
    cyc = {}

    def close(c):
        def d(key, a, b, limit=1000.0):
            if a in c and b in c and 0 <= c[b] - c[a] <= limit:
                seg[key].append(c[b] - c[a])
        if "k0" in c:
            w = _last_at_or_before(writes, c["k0"], 500.0)
            if w is not None:
                seg["app_write_to_handler"].append(c["k0"] - w)
        d("handler_pipe_read", "k0", "k1")
        d("handler_insert", "k1", "k2")
        d("accept_line", "k2", "r")
        d("readline_to_preexec", "r", "p")
        d("preexec_tail", "p", "q")
        if "p" in c:
            a = _first_at_or_after(preexecs, c["p"] - 1.0, 500.0)     # 1 ms of slack: two clocks, one calibration
            if a is not None:
                seg["preexec_osc_to_app"].append(max(0.0, a - c["p"]))
        if "q" in c and stamps:
            s = _first_at_or_after(stamps, c["q"] - 1.0, 2000.0)
            if s is not None:
                seg["hook_to_command_start"].append(s - c["q"])
        d("prompt_precmd", "c", "m")
        d("prompt_render", "m", "x")
        d("prompt_to_readline", "x", "i")
        d("readline_maintenance", "i", "e")

    for name, ticks in marks:
        t = ms(ticks)
        if t < t0_ms:
            continue
        # a new command: k0 (the insert chord) starts one; r is the same command's when its k0 came first, else a line PSReadLine read some
        # other way (typed keys) and starts one too. Marks before the first command (a prompt's c..e) form a prompt-only cycle.
        if (name == "k0" or (name == "r" and not ("k0" in cyc and "r" not in cyc))) and cyc:
            close(cyc)
            cyc = {}
        cyc.setdefault(name, t)
    if cyc:
        close(cyc)
    out = {}
    for k, v in seg.items():
        if v:
            out[k] = round(statistics.median(v), 2)
            out[k + "_n"] = len(v)
    return out


def ctrl_c_segments(marks_text, lines, t0_ms=0.0):
    """#1401: after each ^C the app wrote (`hop=ctrl_c_pty_write`): -> medians ms of
      ctrl_c_to_prompt     ^C written -> PowerShell's prompt function entered (c): the pipeline stopping, its finally blocks
      ctrl_c_to_reading    ^C written -> LineEditorReading sent (e)
    plus the app's own legs from its log (since_ctrl_c_ms): line_editor_ready_after_ctrl_c, line_editor_active_after_ctrl_c,
    command_written_after_ctrl_c, preexec_after_ctrl_c. {} when nothing applies."""
    out, seg = {}, {"ctrl_c_to_prompt": [], "ctrl_c_to_reading": []}
    freq, marks = parse_marks(marks_text)
    off = calibration(lines)
    cc = [t for t in app_times(lines, "ctrl_c_pty_write") if t >= t0_ms]
    if freq and off is not None and marks and cc:
        cs = sorted(t * 1000.0 / freq + off for n, t in marks if n == "c")
        es = sorted(t * 1000.0 / freq + off for n, t in marks if n == "e")
        for t in cc:
            c = _first_at_or_after(cs, t, 10000.0)
            e = _first_at_or_after(es, t, 10000.0)
            if c is not None:
                seg["ctrl_c_to_prompt"].append(c - t)
            if e is not None:
                seg["ctrl_c_to_reading"].append(e - t)
    for hop in ("line_editor_ready_after_ctrl_c", "line_editor_active_after_ctrl_c", "command_written_after_ctrl_c", "preexec_after_ctrl_c"):
        v = []
        for l in lines:
            if "hop=%s " % hop in l:
                t, m = _T.search(l), re.search(r"since_ctrl_c_ms=(\d+(?:\.\d+)?)", l)
                if m and t and float(t.group(1)) >= t0_ms:
                    v.append(float(m.group(1)))
        if v:
            seg[hop] = v
    for k, v in seg.items():
        if v:
            out[k] = round(statistics.median(v), 2)
            out[k + "_n"] = len(v)
    return out
