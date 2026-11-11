# appci/cases_w4core.py (#2075; docs/design/r14-test-gaps.md 7.1 wave 4, T34 and T36; design note docs/design/r15-w4core.md).
# Two harness capabilities and the cases that use them, all in cases.PROVING (the lab arm in PROVING_LAB):
#   grid_snapshot_render     T34  every platform (Windows: the Git Bash pane). The app's grid snapshot hook (app/src/terminal/appci_grid.rs,
#                                 PHOSPHOR_APPCI_GRID_DIR, set by app.py): the harness lists a token in <dir>/request, prints a fixed escape script that
#                                 starts with the token, and the app writes the grid it PAINTS to <dir>/<token>.grid (cells: text, colour as the sequence
#                                 gave it, attributes, wide cells and spacers; cursor row, column, shape). The case compares that with the expected grid:
#                                 16 colours, 256 colours, truecolor, bold / dim / italic / underline / double underline / strikeout / reverse / hidden,
#                                 wide and combining cells, an emoji, box drawing on the normal screen; cells plus the cursor's place and shape, twice,
#                                 on the alternate screen. No OCR, no screenshot in any verdict.
#   freeze_and_wake          T36  Linux and macOS, the fake-model pass. Three tabs: a local typing tab, a tab with a scripted agent reply streaming, a
#                                 tab in an ssh session on the test host. The app is frozen (SIGSTOP) for APPCI_FW_FREEZE_S (60 s) and woken (SIGCONT).
#                                 A line typed WHILE it is frozen and a line typed right after the wake must reach the local shell exactly, in order, the
#                                 second within APPCI_FW_WAKE_S; the ssh tab is alive (a typed line lands on the host) or says it dropped and is back at a
#                                 local prompt; the agent turn ends or errors (the conversation takes a follow-up), with nothing duplicated; no key of
#                                 any tab reaches another tab, the host or the model.
#   lab_freeze_and_wake.*    T36  the same body against the ssh lab with the lab's link stalled for APPCI_FW_STALL_S (90 s) across the freeze.
# Rules as in cases.py: typing at 10-25 ms per key with an instant Enter, the EXACT received line, a SKIP names what is missing ("NEEDS: ..."). The pure
# parts are in selftest_w4core.py. Registered by the import line at the end of cases.py.
import json, os, signal, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import cases as C  # noqa: E402
import cases_caseagent as CA  # noqa: E402
import cases_caseremote as CR  # noqa: E402
import cases_lab as LC  # noqa: E402
import cases_robust as R  # noqa: E402
import cases_wrap_vtabs as WV  # noqa: E402
from cases import case  # noqa: E402

# ================================================================= T34: the grid snapshot hook, harness side
GRID_ENV = "PHOSPHOR_APPCI_GRID_DIR"      # app/src/terminal/appci_grid.rs ENV_GRID_DIR (selftest pins both sides)
GRID_DIRNAME = "appci_grid"               # under the profile's home; app.py names the same directory on every platform
GRID_REQUEST = "request"                  # appci_grid.rs REQUEST_FILE
GRID_VERSION = 1                          # appci_grid.rs FORMAT_VERSION
ESC = "\x1b"
FG, BG = "named:foreground", "named:background"
ALNUM = "abcdefghijklmnopqrstuvwxyz0123456789"


def grid_dir(home):
    return os.path.join(home, GRID_DIRNAME)


def grid_token(rng, prefix):
    """a token the hook accepts (6 to 64 ASCII letters or digits; it is a file name on the app's side)"""
    return prefix + "".join(rng.choice(ALNUM) for _ in range(10))


def grid_request(home, *tokens):
    """list tokens in the hook's request file (appended: the app re-reads the file when its size or mtime changed, at most every 200 ms)"""
    d = grid_dir(home)
    os.makedirs(d, exist_ok=True)
    for t in tokens:
        try:
            os.remove(os.path.join(d, t + ".grid"))
        except FileNotFoundError:
            pass
    with open(os.path.join(d, GRID_REQUEST), "a", encoding="utf-8") as f:
        f.write("".join(t + "\n" for t in tokens))


def load_snapshot(path):
    """the snapshot the app wrote, or None (no file yet, or not format 1). The app writes a temporary file and renames it, so a file that exists is whole."""
    try:
        with open(path, encoding="utf-8") as f:
            snap = json.load(f)
    except (OSError, ValueError):
        return None
    if not isinstance(snap, dict) or snap.get("v") != GRID_VERSION or not isinstance(snap.get("rows"), list):
        return None
    return snap


def snap_cells(snap):
    """{(row, col): (text, fg, bg, attrs)} of every cell the snapshot lists (a cell that paints nothing is not listed)"""
    out = {}
    for row in snap.get("rows", []):
        for c in row.get("cells", []):
            if isinstance(c, list) and len(c) == 5:
                out[(row.get("row"), c[0])] = (c[1], c[2], c[3], c[4])
    return out


def snap_texts(snap):
    """{row: text}"""
    return {row.get("row"): row.get("text", "") for row in snap.get("rows", [])}


def row_of(snap, needle):
    """the first row whose text contains `needle`, or None"""
    for row, text in sorted(snap_texts(snap).items()):
        if needle in text:
            return row
    return None


def _sgr(code, ch):
    return "%s[%sm%s%s[0m" % (ESC, code, ch, ESC)


def _cell(row, col, text, klass, fg=FG, bg=BG, attrs=""):
    return dict(row=row, col=col, text=text, fg=fg, bg=bg, attrs=attrs, klass=klass)


def _blank(row, col, klass="reset"):
    return dict(row=row, col=col, blank=True, klass=klass)


def _sgr_line(row, items):
    """items = (sgr code, char, class, fg, bg, attrs): each char in its own SGR, reset after it, one plain space between -> (text, expected cells).
    The space after every reset must paint nothing: an attribute or colour that outlives `ESC[0m` shows there."""
    text, exp = "", []
    for i, (code, ch, klass, fg, bg, attrs) in enumerate(items):
        text += _sgr(code, ch) + " "
        exp.append(_cell(row, 2 * i, ch, klass, fg, bg, attrs))
        exp.append(_blank(row, 2 * i + 1))
    return text.rstrip(" "), exp[:-1]


def end_marker(token):
    return "fin" + token[2:]


def normal_script(token):
    """the fixed escape script of the normal screen -> (bytes, expected cells). Rows are relative to the row that shows the token; the last line is
    end_marker(token), so a snapshot that shows it shows the whole script."""
    lines, exp = [token], []
    t, e = _sgr_line(1, [("31", "R", "colour16", "named:red", BG, ""), ("92", "G", "colour16", "named:brightgreen", BG, ""),
                         ("44", "B", "colour16", FG, "named:blue", ""), ("103", "Y", "colour16", FG, "named:brightyellow", "")])
    lines.append(t)
    exp += e
    t, e = _sgr_line(2, [("38;5;196", "I", "colour256", "idx:196", BG, ""), ("48;5;21", "J", "colour256", FG, "idx:21", ""),
                         ("38;2;255;136;0", "T", "truecolor", "rgb:ff8800", BG, ""), ("48;2;0;51;102", "U", "truecolor", FG, "rgb:003366", "")])
    lines.append(t)
    exp += e
    t, e = _sgr_line(3, [("1", "b", "attribute", FG, BG, "b"), ("2", "d", "attribute", FG, BG, "d"), ("3", "i", "attribute", FG, BG, "i"),
                         ("4", "u", "attribute", FG, BG, "u"), ("4:2", "D", "attribute", FG, BG, "U"), ("9", "s", "attribute", FG, BG, "s"),
                         ("7", "r", "attribute", FG, BG, "r"), ("8", "h", "attribute", FG, BG, "h"), ("1;4;31", "x", "attribute", "named:red", BG, "bu")])
    lines.append(t)
    exp += e
    # two wide CJK cells, a blank, `e` with a combining acute accent (one cell, two code points), a blank, an emoji (two columns), a blank, `x`
    lines.append("世界 é \U0001F600 x")
    exp += [_cell(4, 0, "世", "wide", attrs="W"), _cell(4, 1, "", "wide", attrs="S"), _cell(4, 2, "界", "wide", attrs="W"),
            _cell(4, 3, "", "wide", attrs="S"), _blank(4, 4, "wide"), _cell(4, 5, "é", "combining"), _blank(4, 6, "combining"),
            _cell(4, 7, "\U0001F600", "emoji", attrs="W"), _cell(4, 8, "", "emoji", attrs="S"), _blank(4, 9, "emoji"), _cell(4, 10, "x", "emoji")]
    box = "┌─┐│└┘"
    lines.append(box)
    exp += [_cell(5, i, ch, "box") for i, ch in enumerate(box)]
    lines.append(end_marker(token))
    return ("\n".join(lines) + "\n").encode("utf-8"), exp


ALT_CURSORS = ((6, 9, "beam"), (1, 2, "underline"))      # after alt_scripts()[0] and after [1]: row, column (0-based), shape
ALT_STEP2 = "step2"


def alt_scripts(token):
    """the alternate screen in three files -> ([enter + first state, second state, leave], expected cells with ABSOLUTE rows).
    First state: the token in magenta at row 2 column 4, a wide cell and `x` on row 4, two box corners on row 5, the cursor a steady bar at (6, 9).
    Second state: `step2` on row 8, the cursor a steady underline at (1, 2). Leave: the default cursor shape, the normal screen."""
    first = "%s[?1049h%s[2J%s[H" % (ESC, ESC, ESC)
    first += "%s[3;5H%s[35m%s%s[0m" % (ESC, ESC, token, ESC)
    first += "%s[5;1H世x" % ESC
    first += "%s[6;3H┌┐" % ESC
    first += "%s[6 q%s[7;10H" % (ESC, ESC)
    second = "%s[9;1H%s%s[4 q%s[2;3H" % (ESC, ALT_STEP2, ESC, ESC)
    leave = "%s[0 q%s[?1049l" % (ESC, ESC)
    exp = [_cell(2, 4 + i, ch, "alt colour16", fg="named:magenta") for i, ch in enumerate(token)]
    exp += [_blank(2, 3, "alt position"), _blank(2, 4 + len(token), "alt reset"),
            _cell(4, 0, "世", "alt wide", attrs="W"), _cell(4, 1, "", "alt wide", attrs="S"), _cell(4, 2, "x", "alt wide"),
            _cell(5, 2, "┌", "alt box"), _cell(5, 3, "┐", "alt box")]
    return [s.encode("utf-8") for s in (first, second, leave)], exp


def judge_grid(snap, base_row, expected):
    """pure: the snapshot against the expected cells (rows relative to base_row) -> the list of differences, each naming its class, place, what the
    app painted and what the script asked for. A listed cell that must be blank and a missing cell that must not be are both differences."""
    cells, out = snap_cells(snap), []
    for e in expected:
        key = (base_row + e["row"], e["col"])
        got = cells.get(key)
        if e.get("blank"):
            if got is not None:
                out.append("[%s] row %d col %d: painted %r, expected a blank cell" % (e["klass"], key[0], key[1], got))
            continue
        want = (e["text"], e["fg"], e["bg"], e["attrs"])
        if got is None:
            out.append("[%s] row %d col %d: nothing painted, expected %r" % (e["klass"], key[0], key[1], want))
        elif tuple(got) != want:
            out.append("[%s] row %d col %d: painted %r, expected %r" % (e["klass"], key[0], key[1], tuple(got), want))
    return out


def judge_cursor(snap, want):
    """pure: None when the snapshot's cursor is at want = (row, col, shape); else what differs. No cursor in the snapshot is a difference."""
    cur = snap.get("cursor")
    if not isinstance(cur, dict):
        return "no cursor in the snapshot (the app did not draw this grid's cursor), expected %r" % (want,)
    got = (cur.get("row"), cur.get("col"), cur.get("shape"))
    return None if got == tuple(want) else "the cursor is %r, expected %r (row, column, shape)" % (got, tuple(want))


def wait_snapshot(ctx, token, pred, timeout):
    """poll the token's snapshot until pred(snapshot) -> (the last snapshot read or None, whether pred held)"""
    path = os.path.join(grid_dir(ctx.prof.home), token + ".grid")
    box = [None]

    def ready():
        s = load_snapshot(path)
        if s is not None:
            box[0] = s
        return s is not None and pred(s)
    ok = ctx.until(ready, timeout, step=0.2)
    return box[0], bool(ok)


NO_SNAPSHOT = ("%s: the app wrote no snapshot for the token within %d s (%s). The hook writes the grid it paints when %s is set and the token is on "
               "screen (app/src/terminal/appci_grid.rs); a build older than #2075 has no hook")
RESTORE_PRINTF = "printf '\\033[0 q\\033[?1049l'"


@case("grid_snapshot_render", order=64, gui=True)
def c_grid_snapshot_render(ctx, r):
    """T34 (r14-test-gaps H4). See the file header. Normal screen: `cat` of the script file, the snapshot must show the end marker, then every expected
    cell is compared. Alternate screen: `cat first; read; cat second; read; cat leave`, the cursor is judged in both states (two different places and
    shapes: a hook that reported one fixed cursor fails one of them), Return moves on, and the command list must finish (the keys reached `read`)."""
    if ctx.pane == "powershell":
        r.skip("`cat` of the script files and `read` are POSIX: runs in the Git Bash pane")
    home = ctx.prof.home
    tok, atok = grid_token(ctx.rng, "gs"), grid_token(ctx.rng, "ga")
    stem = tok[2:]
    fa, f1, f2, f3, done = ("gs_%s_%s" % (stem, n) for n in ("a.bin", "b1.bin", "b2.bin", "b3.bin", "done.txt"))
    data, exp = normal_script(tok)
    alt, alt_exp = alt_scripts(atok)
    for name, blob in ((fa, data), (f1, alt[0]), (f2, alt[1]), (f3, alt[2])):
        with open(os.path.join(ctx.W, name), "wb") as f:
            f.write(blob)
    ctx.rm(done)
    grid_request(home, tok, atok)
    time.sleep(0.5)                       # the app re-reads the request file at most every 200 ms
    in_alt = False
    try:
        # ---- the normal screen
        ctx.be.type("cat %s" % fa, ctx.rng.randint(10, 25), enter=True)
        snap, whole = wait_snapshot(ctx, tok, lambda s: row_of(s, end_marker(tok)) is not None, 20)
        ctx.shot("grid_normal")
        if snap is None:
            r.check(False, NO_SNAPSHOT % ("normal screen", 20, os.path.join(grid_dir(home), tok + ".grid"), GRID_ENV))
            return
        r.check(whole, "normal screen: the snapshot never showed the script's last line %r (rows painted: %r)" % (end_marker(tok), sorted(snap_texts(snap).items())[:10]))
        base = row_of(snap, tok)
        r.check(base is not None, "normal screen: no painted row shows the token %r" % tok)
        if base is not None:
            diffs = judge_grid(snap, base, exp)
            r.metric("grid_normal", dict(cols=snap.get("cols"), rows=len(snap["rows"]), token_row=base, cells_expected=len(exp), differences=len(diffs),
                                         cursor=snap.get("cursor")))
            for d in diffs[:16]:
                r.check(False, "normal screen " + d)
            if len(diffs) > 16:
                r.check(False, "normal screen: %d more differences (logs: the snapshot file is kept in %s)" % (len(diffs) - 16, grid_dir(home)))
        # ---- the alternate screen, two cursor states
        ctx.be.type("cat %s; read -r _; cat %s; read -r _; cat %s; echo fin >> %s" % (f1, f2, f3, done), ctx.rng.randint(10, 25), enter=True)
        in_alt = True
        c1, c2 = ALT_CURSORS
        snap, ok = wait_snapshot(ctx, atok, lambda s: row_of(s, atok) is not None and judge_cursor(s, c1) is None, 20)
        ctx.shot("grid_alt_1")
        if snap is None:
            r.check(False, NO_SNAPSHOT % ("alternate screen", 20, os.path.join(grid_dir(home), atok + ".grid"), GRID_ENV))
            return
        diffs = judge_grid(snap, 0, alt_exp)
        r.metric("grid_alt", dict(cols=snap.get("cols"), rows=len(snap["rows"]), cells_expected=len(alt_exp), differences=len(diffs), cursor_1=snap.get("cursor")))
        for d in diffs[:12]:
            r.check(False, "alternate screen " + d)
        r.check(ok, "alternate screen, first state: %s" % (judge_cursor(snap, c1) or "the token row was not painted"))
        ctx.be.key("Return")
        snap2, ok2 = wait_snapshot(ctx, atok, lambda s: row_of(s, ALT_STEP2) is not None and judge_cursor(s, c2) is None, 20)
        ctx.shot("grid_alt_2")
        snap2 = snap2 or snap
        r.metric("grid_alt_cursor_2", snap2.get("cursor"))
        r.check(row_of(snap2, ALT_STEP2) == 8, "alternate screen, second state: %r is on row %r, expected row 8 (the Return did not reach `read`, or the "
                "text was placed elsewhere)" % (ALT_STEP2, row_of(snap2, ALT_STEP2)))
        r.check(ok2, "alternate screen, second state: %s" % (judge_cursor(snap2, c2) or "`%s` was not painted" % ALT_STEP2))
        ctx.be.key("Return")
        fin = ctx.until(lambda: "fin" in ctx.lines(done), 15)
        in_alt = not fin
        r.check(bool(fin), "the command list did not finish within 15 s after the second Return (%s has no `fin`): the keys did not reach `read`" % done)
        r.check(ctx.app.alive() if ctx.app is not None else True, "the app is not alive after the escape scripts")
    finally:
        if in_alt:                        # cleanup only: end the reads, then leave the alternate screen and the cursor shape the script set
            ctx.reset()
            time.sleep(0.4)
            ctx.be.type(RESTORE_PRINTF, 12, enter=True)
            time.sleep(0.6)
        ctx.reset()
        ctx.rm(fa, f1, f2, f3, done)


# ================================================================= T36: freeze_and_wake
FREEZE_S = float(os.environ.get("APPCI_FW_FREEZE_S", "60"))
WAKE_S = float(os.environ.get("APPCI_FW_WAKE_S", "5"))            # SIGCONT -> the effect of the line typed right after it
STALL_S = int(float(os.environ.get("APPCI_FW_STALL_S", "90")))    # the lab arm: the link's stall, started just before the freeze
GAP_MS = 100
NEEDS_FAKE = ("NEEDS: the scripted fake model (APPCI_FAKE_MODEL=1; the fix tier's agent@fake pass and the lab job set it): the streaming agent turn "
              "that must be open across the freeze is a scripted long reply")


def stat_state(stat_text):
    """pure: the state letter of a Linux /proc/<pid>/stat line (the field after the parenthesised command, which may hold spaces and parentheses)"""
    tail = (stat_text or "").rsplit(")", 1)
    f = tail[-1].split() if len(tail) == 2 else []
    return f[0] if f else None


def ps_state_stopped(ps_text):
    """pure: is the `ps -o state=` answer a stopped process (macOS / BSD: the first letter is T)"""
    t = (ps_text or "").strip()
    return t[:1] == "T" if t else None


def proc_stopped(pid):
    """True / False from the OS (Linux: /proc; macOS: ps); None when it cannot be read"""
    try:
        if sys.platform.startswith("linux"):
            with open("/proc/%d/stat" % pid) as f:
                st = stat_state(f.read())
            return None if st is None else st in ("T", "t")
        out = subprocess.run(["ps", "-o", "state=", "-p", str(pid)], capture_output=True, text=True, timeout=5).stdout
        return ps_state_stopped(out)
    except (OSError, subprocess.SubprocessError):
        return None


def freeze_proof(stopped_start, stopped_end, log_start, log_end):
    """pure: (ok, text). The freeze is proven when the OS said 'stopped' at its start and its end AND the app's log did not grow in between."""
    if stopped_start is None or stopped_end is None:
        return None, "the process state could not be read (start %r, end %r)" % (stopped_start, stopped_end)
    if not (stopped_start and stopped_end):
        return False, "the OS did not report the app as stopped (start %r, end %r)" % (stopped_start, stopped_end)
    if log_start != log_end:
        return False, "the app's log grew by %d bytes while it was reported stopped" % (log_end - log_start)
    return True, "stopped at both ends, the log did not grow"


def judge_local(new_lines, held, after):
    """pure: ('ok' | 'dropped' | 'fail', text) for the effect-file lines added in the typing tab. `held` was typed while the app was frozen, `after`
    right after the wake. Exactly [held, after] is ok. [after] alone means the keys typed while frozen never reached the app (the display system did
    not queue them): not a misroute, reported as not judged. Anything else (a partial, merged, repeated or reordered line) fails."""
    if new_lines == [held, after]:
        return "ok", "both lines arrived exactly, in order"
    if new_lines == [after]:
        return "dropped", "the line typed while the app was frozen never arrived (the display system did not hold the keys); the line after the wake is exact"
    return "fail", "the typing tab's file gained %r, expected exactly %r" % (new_lines, [held, after])


def judge_received(cmds, held_line, after_line, verdict):
    """pure: None or what is wrong with the command lines the local shell received (rec.log) for the same two lines"""
    want = [held_line, after_line] if verdict == "ok" else [after_line]
    return None if cmds == want else "the local shell received %r, expected exactly %r" % (cmds, want)


def judge_ssh(remote_new, local_new, tok, ssh_gone):
    """pure: ('alive' | 'dropped' | 'fail', text) for the line typed in the ssh tab after the wake. remote_new / local_new = the lines the host's file
    and the local file of the same name gained; ssh_gone = the app logged that ssh left the foreground since the freeze began."""
    if remote_new == [tok] and not local_new:
        return "alive", "the session is alive: the host received the line exactly"
    if local_new == [tok] and not remote_new:
        if ssh_gone:
            return "dropped", "the session dropped and the pane is back at a local prompt (the app logged ssh leaving the foreground); the line ran locally, exactly"
        return "fail", "the line ran in the LOCAL shell but the app never logged the ssh client leaving the foreground: the pane did not say the session dropped"
    if not remote_new and not local_new:
        return "fail", "the line reached neither the host nor a local shell: the tab is dead"
    return "fail", "the host's file gained %r and the local file %r, expected exactly [%r] in one of them" % (remote_new, local_new, tok)


def judge_agent(stream, followup, foreign_tokens):
    """pure: (ok, text). stream = fake.streams[tag] (or None), followup = fake.followups[tag] (or None: no follow-up reached the model).
    The turn must END or ERROR: either way the conversation takes the follow-up. Hung = no follow-up. A reply that came back with a chunk twice, a
    marker of another conversation, or a token typed in another tab fails; an incomplete reply is an errored turn, which is allowed and reported."""
    if not stream:
        return False, "the model never received the request for the streaming reply"
    if not followup:
        return False, ("the turn neither ended nor errored: no follow-up reached the model (stream: %d/%d chunks written, finished=%s, broken=%s)"
                       % (stream.get("sent", 0), stream.get("total", 0), stream.get("finished"), stream.get("broken")))
    bad = []
    if followup.get("dup"):
        bad.append("%d chunk(s) are in the conversation twice" % followup["dup"])
    if followup.get("foreign"):
        bad.append("markers of another conversation: %r" % (followup["foreign"],))
    leaked = [t for t in foreign_tokens if t in (followup.get("user_text") or "")]
    if leaked:
        bad.append("text typed in another tab is in this conversation: %r" % leaked)
    if bad:
        return False, "; ".join(bad)
    if followup.get("verdict") == "ok":
        return True, "the turn ended: every chunk and the END marker once"
    return True, "the turn errored or was cut, and the conversation took the follow-up (%s; %s/%s chunks)" % (
        followup.get("verdict"), followup.get("chunks_seen"), followup.get("chunks_expected"))


def misrouted(tokens, places):
    """pure: [(token, place)] for every token found in a place it was not typed for. places = {name: text}"""
    return [(t, name) for t in tokens for name, text in sorted(places.items()) if t in (text or "")]


def _alnum_tok(ctx, n=8):
    return ctx.tok(n).lower().replace("_", "")


def _need_freeze_platform(ctx, r):
    if ctx.win or os.name == "nt":
        r.skip("Linux and macOS only (r14-test-gaps T36): the freeze is SIGSTOP / SIGCONT. NEEDS: a suspend / resume of the app's process in the "
               "Windows backend (NtSuspendProcess in winapi.py), which is not built")
    if ctx.app is None or not ctx.app.pid():
        r.skip("NEEDS: the real app process (the sim backend has none to freeze)")


def freeze_body(ctx, r, host, stall=None):
    """the T36 body. host = the ssh host name typed in the ssh tab. stall = None, or a function that stalls the link for STALL_S (the lab arm): it is
    called just before the freeze. Tab 1 is the typing tab; the agent tab is opened first and the ssh tab last, so the ssh tab is the LAST tab (tab
    key 9) and the agent tab the one before it (previous-tab key from there), whatever tabs earlier cases left open."""
    import fake_model
    _need_freeze_platform(ctx, r)
    if os.environ.get("APPCI_FAKE_MODEL_ACTIVE") != "1" or fake_model.ACTIVE is None:
        r.skip(NEEDS_FAKE)
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.skip("NEEDS: the app log (none at %s): the freeze proof and the ssh state are read from it" % ctx.log_path)
    fake = fake_model.ACTIVE
    t = _alnum_tok(ctx, 6)
    eff, rfile, tag = "fw_%s.txt" % t, "fw_r_%s.txt" % t, "FW%s" % t.upper()
    pre, held, after, rpre, rpost = ("%s%s" % (p, _alnum_tok(ctx, 8)) for p in ("pre", "held", "aft", "rpre", "rpost"))
    chunks = int((FREEZE_S + 60) * 1000 / GAP_MS)
    last, prev, first = R.tab_key(ctx, 9), WV.KX(ctx, "prev_tab"), R.tab_key(ctx, 1)
    pid = ctx.app.pid()
    ctx.rm(eff, rfile)
    CR.rm_remote(ctx, rfile)
    off_case = ctx.log_size()
    opened, frozen, ssh_in_front = 0, False, False

    def local_line(word):
        return "echo %s >> %s" % (word, eff)

    try:
        # ---- control: the typing tab works before anything else is open
        mark = ctx.cmd_mark()
        ctx.be.type(local_line(pre), ctx.rng.randint(10, 25), enter=True)
        ctx.until(lambda: pre in ctx.lines(eff), 10)
        if ctx.lines(eff) != [pre]:
            r.check(False, "control: before the freeze the typing tab's file is %r, expected [%r]: the case cannot judge a wake on a tab that does not "
                    "take a line" % (ctx.lines(eff), pre))
            return
        ctx.await_commands(mark, 1, 5.0)
        # ---- the agent tab: a scripted reply that streams across the whole freeze
        ctx.be.key(R.NEWTAB)
        opened += 1
        time.sleep(1.5)
        if ctx.log_view_mode() != "agent":
            ctx.be.type("/agent", 12, enter=True)
            if not ctx.until(lambda: ctx.log_view_mode() == "agent", 10):
                r.skip("NEEDS: an agent view in a new tab (`/agent` did not open one; hop=view_mode says %s)" % ctx.log_view_mode())
            time.sleep(0.8)
        CA.type_fast(ctx, "Please stream long reply %s chunks %d gap %d" % (tag, chunks, GAP_MS))
        if not ctx.until(lambda: tag in fake.streams, 30):
            r.check(False, "the app sent no request for the streaming reply within 30 s: no agent turn is open, the case cannot go on")
            return
        # ---- the ssh tab
        ctx.be.key(R.NEWTAB)
        opened += 1
        time.sleep(1.5)
        secs = CR.connect(ctx, host, 90)
        if secs is None:
            r.check(False, "no remote prompt after `ssh %s` in the ssh tab: no ssh session is open, the case cannot go on. Test host: %s" % (host, ctx.ssh_diag()))
            return
        ssh_in_front = True
        time.sleep(1.0)
        ctx.be.type("echo %s >> ~/%s" % (rpre, rfile), ctx.rng.randint(10, 25), enter=True)
        ctx.until(lambda: rpre in ctx.lines(rfile, remote=True), 20)
        if ctx.lines(rfile, remote=True) != [rpre]:
            r.check(False, "control: before the freeze the host's file is %r, expected [%r]" % (ctx.lines(rfile, remote=True), rpre))
            return
        # ---- back to the typing tab, then freeze
        ctx.be.key(first)
        time.sleep(1.5)
        st = fake.streams.get(tag) or {}
        r.check(not st.get("finished") and not st.get("broken"), "the streaming reply is already over before the freeze (%r): no agent turn is open across it" % st)
        mark = ctx.cmd_mark()
        if stall is not None:
            stall()
        t_freeze = time.time()
        os.kill(pid, signal.SIGSTOP)
        frozen = True
        time.sleep(1.0)
        s0, l0 = proc_stopped(pid), ctx.log_size()
        time.sleep(4.0)
        try:
            ctx.be.type(local_line(held), ctx.rng.randint(10, 25), enter=True)    # typed into the frozen window: the display system holds the keys
        except RuntimeError as e:                                                 # a backend's focus guard refusing the frozen window
            r.note("the injector refused to type while the app was frozen (%s): the held-keys half is not judged" % e)
        time.sleep(max(0.0, t_freeze + FREEZE_S - 1.0 - time.time()))
        s1, l1 = proc_stopped(pid), ctx.log_size()
        early = ctx.lines(eff)[1:]
        os.kill(pid, signal.SIGCONT)
        frozen = False
        t_wake = time.time()
        # ---- the wake: a line typed at once
        ctx.be.type(local_line(after), ctx.rng.randint(10, 25), enter=True)
        got_after = ctx.until(lambda: after in ctx.lines(eff), 30, step=0.05)
        wake_s = time.time() - t_wake
        ok, text = freeze_proof(s0, s1, l0, l1)
        r.metric("freeze", dict(seconds=round(t_wake - t_freeze, 1), proof=text, stream_at_freeze=dict(st), stream_at_wake=dict(fake.streams.get(tag) or {})))
        if ok is None:
            r.unverifiable("the freeze is not proven: %s" % text)
        else:
            r.check(ok, "the freeze did not happen as specified: %s" % text)
        r.check(not early, "a line typed while the app was frozen took effect BEFORE the wake (%r): the app was not frozen, or another process ran it" % early)
        time.sleep(1.5)
        new = ctx.lines(eff)[1:]
        verdict, text = judge_local(new, held, after)
        r.metric("wake", dict(wake_to_effect_s=round(wake_s, 2), bound_s=WAKE_S, local=verdict))
        if verdict == "fail":
            r.check(False, "typing tab after the wake: %s" % text)
        elif verdict == "dropped":
            r.no_reference("keys typed while frozen: %s" % text)
        r.check(bool(got_after) and wake_s <= WAKE_S, "the line typed right after the wake took effect after %.1f s (bound %.1f s, APPCI_FW_WAKE_S)%s" % (
            wake_s, WAKE_S, "" if got_after else ": it never did within 30 s"))
        if ctx.prof.commands():
            # rec.log is one file for every tab, and a tab's `ssh <host>` line is logged when ssh ends: were the session to drop inside this window,
            # that line is the ssh tab's own and is not counted against the typing tab
            mine = lambda: [c for c in ctx.cmds_since(mark) if not c.startswith("ssh ")]      # noqa: E731
            ctx.until(lambda: len(mine()) >= (2 if verdict == "ok" else 1), 5.0)
            err = judge_received(mine(), local_line(held), local_line(after), verdict) if verdict != "fail" else None
            r.check(err is None, "typing tab after the wake: %s" % err)
        # ---- the ssh tab: alive, or dropped back to a local prompt
        ctx.be.key(last)
        time.sleep(1.5)
        ctx.be.type("echo %s >> ~/%s" % (rpost, rfile), ctx.rng.randint(10, 25), enter=True)
        left = max(0.0, STALL_S - (time.time() - t_freeze)) if stall is not None else 0.0
        home_file = os.path.join(ctx.prof.home, rfile)

        def home_lines():
            try:
                return open(home_file, errors="replace").read().splitlines()
            except OSError:
                return []
        ctx.until(lambda: rpost in ctx.lines(rfile, remote=True) or rpost in home_lines(), 30 + left, step=0.25)
        gone = any("hop=ssh_pty_owner ssh=false" in l for l in ctx.app_log_window(t_freeze))
        sv, text = judge_ssh(ctx.lines(rfile, remote=True)[1:], home_lines(), rpost, gone)
        r.metric("ssh_after_wake", dict(state=sv, stall_s=STALL_S if stall is not None else 0))
        r.check(sv != "fail", "ssh tab after the wake: %s" % text)
        r.note("ssh tab after the wake: %s" % text)
        ssh_in_front = sv == "alive"
        ctx.shot("fw_ssh")
        # ---- the agent tab: the turn ends or errors
        ctx.be.key(prev)
        time.sleep(1.5)
        wait_s = chunks * GAP_MS / 1000.0 + 60
        ctx.until(lambda: fake.streams[tag]["finished"] or fake.streams[tag]["broken"], wait_s, step=1.0)
        time.sleep(2.0)
        CA.type_fast(ctx, "Please follow up check %s" % tag)
        ctx.until(lambda: tag in fake.followups, 60, step=0.5)
        ctx.shot("fw_agent")
        ok, text = judge_agent(fake.streams.get(tag), fake.followups.get(tag), (pre, held, after, rpre, rpost))
        r.metric("agent_after_wake", dict(stream=dict(fake.streams.get(tag) or {}), followup_verdict=(fake.followups.get(tag) or {}).get("verdict")))
        r.check(ok, "agent tab after the wake: %s" % text)
        if ok:
            r.note("agent tab after the wake: %s" % text)
        # ---- nothing went to the wrong place
        turns = "\n".join(t_ for _, t_ in fake.user_turns)
        remote_cmds = "\n".join(c for _, c in ctx.prof.commands(remote=True)) if getattr(ctx, "remote", None) is None else ""
        wrong = misrouted((pre, held, after), {"the host's file": "\n".join(ctx.lines(rfile, remote=True)), "the host's shell": remote_cmds, "a model request": turns})
        wrong += misrouted((rpre, rpost), {"the typing tab's file": "\n".join(ctx.lines(eff)), "a model request": turns})
        r.check(not wrong, "keys went to the wrong place: %r" % wrong)
        n_stream = sum(1 for _, t_ in fake.user_turns if "stream long reply %s " % tag in t_)
        r.check(n_stream == 1, "%d requests carried the stream prompt (expected 1: a replayed Enter would add one)" % n_stream)
        r.check(ctx.app.alive(), "the app died during the case")
        r.check(not [l for l in ctx.log_since(off_case).splitlines() if "panicked at" in l], "panic in the app log during the case")
    finally:
        if frozen:
            try:
                os.kill(pid, signal.SIGCONT)        # never leave the app stopped, whatever failed above
            except OSError:
                pass
            time.sleep(2.0)
        try:
            if opened >= 2:
                ctx.be.key(last)
                time.sleep(1.0)
                if ssh_in_front:
                    left_ok = CR.leave(ctx)
                    if not left_ok:
                        r.note("cleanup: the ssh tab was not proven remote (%s), `exit` was not typed" % getattr(ctx, "left_unproven", None))
                R.close_tab_best_effort(ctx, r)
                time.sleep(1.0)
                opened -= 1
            if opened >= 1:
                ctx.be.key(last)
                time.sleep(1.0)
                R.close_tab_best_effort(ctx, r)
                time.sleep(1.0)
            ctx.be.key(first)
            time.sleep(0.8)
        except Exception as e:              # cleanup never hides the case's own outcome
            r.note("cleanup after freeze_and_wake raised %s: %s" % (type(e).__name__, e))
        CA.cleanup_tmp(ctx)
        ctx.rm(eff, rfile)
        CR.rm_remote(ctx, rfile)
        try:
            os.remove(os.path.join(ctx.prof.home, rfile))
        except OSError:
            pass


@case("freeze_and_wake", modes=("agent",), order=98, gui=True)
def c_freeze_and_wake(ctx, r):
    """T36 (r14-test-gaps H6) against the harness's own ssh test host. See the file header and freeze_body."""
    _need_freeze_platform(ctx, r)
    C.need_ssh(ctx, r)
    freeze_body(ctx, r, "fake")


@LC.lab_case("freeze_and_wake", shells=("bash",), lat=("lan",), helper=("nohelper",), modes=("agent",))
def c_lab_freeze_and_wake(ctx, r, arm):
    """T36 against the ssh lab, with the lab's link to the session stalled for APPCI_FW_STALL_S (90 s) from just before the freeze: the stall outlives
    the freeze, so the line typed in the ssh tab after the wake waits for the link and must then land on the host exactly (a stall is not a drop)."""
    freeze_body(ctx, r, LC.host_of(arm), stall=lambda: ctx.lab.drop(arm["user"], "stall", STALL_S))
