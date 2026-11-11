# appci/selftest_w4core.py: offline self-tests of cases_w4core.py (round 15 wave 4 core, #2075: T34 grid_snapshot_render, T36 freeze_and_wake).
# Pure Python: no app, display or network. selftest.py appends CHECKS to its own list, so `run.py --self-test` and `selftest.py` both run these.
import copy, json, os, random, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, HERE)
import cases, winmap  # noqa: E402
import lab as labmod  # noqa: E402
import cases_w4core as W  # noqa: E402

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


# ---------------------------------------------------------------- a model of what a correct app paints for a script (the hook's file format)
def _paint(data, cols=80, rows=24):
    """a tiny terminal for the sequences the scripts use (SGR, CUP, ED 2, 1049, DECSCUSR, LF) -> the snapshot a correct app writes, as a dict
    in the hook's format. It is the self-test's independent statement of the expected grid: built from the BYTES, not from the expectation table."""
    import re
    import unicodedata
    text = data.decode("utf-8")
    grid, cur, shape = {}, [0, 0], "block"
    fg, bg, attrs = W.FG, W.BG, set()
    names = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"]
    i = 0
    while i < len(text):
        ch = text[i]
        if ch == "\x1b":
            m = re.match(r"\x1b\[([?0-9;:]*)( ?)([A-Za-z])", text[i:])
            assert m, "unknown sequence at %r" % text[i:i + 12]
            arg, sp, fin = m.groups()
            i += m.end()
            if fin == "m":
                p = arg.split(";")
                if arg == "0":
                    fg, bg, attrs = W.FG, W.BG, set()
                elif p[0] == "38" and p[1] == "5":
                    fg = "idx:%s" % p[2]
                elif p[0] == "48" and p[1] == "5":
                    bg = "idx:%s" % p[2]
                elif p[0] == "38" and p[1] == "2":
                    fg = "rgb:%02x%02x%02x" % tuple(int(x) for x in p[2:5])
                elif p[0] == "48" and p[1] == "2":
                    bg = "rgb:%02x%02x%02x" % tuple(int(x) for x in p[2:5])
                else:
                    for q in p:
                        if q == "4:2":
                            attrs.add("U")
                        elif q in ("1", "2", "3", "4", "7", "8", "9"):
                            attrs.add({"1": "b", "2": "d", "3": "i", "4": "u", "7": "r", "8": "h", "9": "s"}[q])
                        elif 30 <= int(q) <= 37:
                            fg = "named:" + names[int(q) - 30]
                        elif 40 <= int(q) <= 47:
                            bg = "named:" + names[int(q) - 40]
                        elif 90 <= int(q) <= 97:
                            fg = "named:bright" + names[int(q) - 90]
                        elif 100 <= int(q) <= 107:
                            bg = "named:bright" + names[int(q) - 100]
                        else:
                            raise AssertionError("SGR %s" % q)
            elif fin == "H":
                rc = [int(x) for x in arg.split(";")] if arg else [1, 1]
                cur = [rc[0] - 1, rc[1] - 1]
            elif fin == "J":
                grid.clear()
            elif fin == "q" and sp == " ":
                shape = {"0": "block", "1": "block", "2": "block", "3": "underline", "4": "underline", "5": "beam", "6": "beam"}[arg]
            elif fin in ("h", "l"):
                assert arg == "?1049"
                grid.clear()
                cur = [0, 0]
            else:
                raise AssertionError("sequence %r" % m.group(0))
            continue
        i += 1
        if ch == "\n":
            cur = [cur[0] + 1, 0]
            continue
        order = "bdiuUsrh"
        a = "".join(c for c in order if c in attrs)
        if unicodedata.combining(ch):
            key = (cur[0], cur[1] - 1)
            t = grid[key]
            grid[key] = (t[0] + ch, t[1], t[2], t[3])
            continue
        wide = unicodedata.east_asian_width(ch) in ("W", "F")
        grid[(cur[0], cur[1])] = (ch, fg, bg, a + ("W" if wide else ""))
        if wide:
            grid[(cur[0], cur[1] + 1)] = ("", fg, bg, a + "S")
        cur[1] += 2 if wide else 1
    out_rows = []
    for rr in range(rows):
        cells = [[c, t[0], t[1], t[2], t[3]] for (r_, c), t in sorted(grid.items()) if r_ == rr
                 and not (t[0] == " " and t[2] == W.BG and not set(t[3]) & set("uUsr"))]
        line, nxt = "", 0
        for c in cells:
            line += " " * (c[0] - nxt) + c[1]
            nxt = c[0] + 1
        out_rows.append(dict(row=rr, text=line.rstrip(" "), cells=cells))
    return dict(v=1, token="x", cols=cols, cursor=dict(row=cur[0], col=cur[1], shape=shape), rows=out_rows)


@check
def w4core_grid_scripts_and_judge():
    """T34: the expectation tables agree with an independent reading of the script BYTES; every class of defect the case is for makes a difference
    (a colour lost, an attribute that outlives its reset, a wide cell painted narrow, a combining mark dropped, a cursor in the wrong place or shape,
    no cursor); a snapshot file that is half-written, not JSON or another format version is not read"""
    tok = W.grid_token(random.Random(7), "gs")
    assert len(tok) == 12 and tok.isalnum() and W.end_marker(tok) != tok and tok not in W.end_marker(tok)
    data, exp = W.normal_script(tok)
    snap = _paint(data)
    assert W.row_of(snap, tok) == 0 and W.row_of(snap, W.end_marker(tok)) == 6 and W.row_of(snap, "nosuchtext") is None
    assert W.judge_grid(snap, 0, exp) == [], W.judge_grid(snap, 0, exp)[:3]
    classes = {e["klass"] for e in exp}
    assert {"colour16", "colour256", "truecolor", "attribute", "wide", "combining", "emoji", "box", "reset"} <= classes, classes
    assert len([e for e in exp if e["klass"] == "attribute"]) == 9 and any(e.get("attrs") == "U" for e in exp), "double underline is in the script"
    # the same grid scrolled down by a prompt line: rows are relative to the token's row
    moved = copy.deepcopy(snap)
    for row in moved["rows"]:
        row["row"] += 3
    assert W.row_of(moved, tok) == 3 and W.judge_grid(moved, 3, exp) == [] and W.judge_grid(moved, 0, exp)

    def broken(fn):
        s = copy.deepcopy(snap)
        fn(s)
        return W.judge_grid(s, 0, exp)

    def cell(s, row, col):
        return next(c for c in s["rows"][row]["cells"] if c[0] == col)
    d = broken(lambda s: cell(s, 1, 0).__setitem__(2, W.FG))                       # red lost
    assert len(d) == 1 and "[colour16] row 1 col 0" in d[0] and "named:red" in d[0]
    d = broken(lambda s: cell(s, 2, 4).__setitem__(2, "rgb:ff8700"))               # truecolor rounded to the palette
    assert len(d) == 1 and "[truecolor]" in d[0]
    d = broken(lambda s: s["rows"][3]["cells"].append([1, " ", W.FG, W.BG, "u"]))  # the underline outlives ESC[0m
    assert len(d) == 1 and "[reset] row 3 col 1" in d[0] and "expected a blank cell" in d[0]
    d = broken(lambda s: s["rows"][4]["cells"].remove(cell(s, 4, 1)))              # the wide cell has no spacer
    assert len(d) == 1 and "[wide] row 4 col 1" in d[0] and "nothing painted" in d[0]
    d = broken(lambda s: cell(s, 4, 5).__setitem__(1, "e"))                        # the combining accent dropped
    assert len(d) == 1 and "[combining]" in d[0]
    d = broken(lambda s: cell(s, 4, 7).__setitem__(4, ""))                         # the emoji painted one column wide
    assert len(d) == 1 and "[emoji] row 4 col 7" in d[0]
    d = broken(lambda s: cell(s, 3, 8).__setitem__(4, "u"))                        # double underline painted as single
    assert len(d) == 1 and "[attribute] row 3 col 8" in d[0]
    assert len(broken(lambda s: s["rows"].__setitem__(5, dict(row=5, text="", cells=[])))) == 6, "a box row that is not painted: six differences"
    # ---- the alternate screen and its two cursor states
    atok = W.grid_token(random.Random(8), "ga")
    alt, alt_exp = W.alt_scripts(atok)
    s1 = _paint(alt[0])
    assert W.judge_grid(s1, 0, alt_exp) == [] and W.row_of(s1, atok) == 2
    assert W.judge_cursor(s1, W.ALT_CURSORS[0]) is None and W.judge_cursor(s1, W.ALT_CURSORS[1]) is not None
    s2 = _paint(alt[0] + alt[1])
    assert W.row_of(s2, W.ALT_STEP2) == 8 and W.judge_cursor(s2, W.ALT_CURSORS[1]) is None and W.judge_cursor(s2, W.ALT_CURSORS[0]) is not None
    assert W.ALT_CURSORS[0] != W.ALT_CURSORS[1] and W.ALT_CURSORS[0][2] != W.ALT_CURSORS[1][2], "one fixed cursor cannot satisfy both states"
    assert "no cursor in the snapshot" in W.judge_cursor(dict(s1, cursor=None), W.ALT_CURSORS[0])
    assert "expected" in W.judge_cursor(dict(s1, cursor=dict(row=6, col=9, shape="block")), W.ALT_CURSORS[0])
    s3 = _paint(alt[0] + alt[1] + alt[2])
    assert s3["cursor"]["shape"] == "block" and not any(r_["cells"] for r_ in s3["rows"]), "the third file leaves the alternate screen and resets the shape"
    # ---- the file: request list and snapshot reading
    tmp = tempfile.mkdtemp()
    d_ = W.grid_dir(tmp)
    os.makedirs(d_)
    stale = os.path.join(d_, tok + ".grid")
    open(stale, "w").write(json.dumps(snap))
    W.grid_request(tmp, tok, atok)
    assert not os.path.exists(stale), "a snapshot left by an earlier request of the same token is removed before the token is listed"
    assert open(os.path.join(d_, W.GRID_REQUEST)).read() == tok + "\n" + atok + "\n"
    W.grid_request(tmp, "gsother0001")
    assert open(os.path.join(d_, W.GRID_REQUEST)).read().splitlines() == [tok, atok, "gsother0001"], "tokens are appended"
    p = os.path.join(d_, "x.grid")
    assert W.load_snapshot(p) is None
    for bad in ("", "{\"v\": 1, \"rows\": [", json.dumps(dict(v=2, rows=[])), json.dumps([1, 2]), json.dumps(dict(v=1))):
        open(p, "w").write(bad)
        assert W.load_snapshot(p) is None, bad
    open(p, "w", encoding="utf-8").write(json.dumps(snap))
    assert W.load_snapshot(p) == snap


@check
def w4core_grid_hook_two_sides_agree():
    """T34: the names the harness uses are the ones the app's hook reads and writes (a rename on either side fails here), the launcher sets the
    directory on Linux, macOS and Windows, and the Rust formatter's field names are the ones the reader takes"""
    rs = open(os.path.join(ROOT, "app", "src", "terminal", "appci_grid.rs"), encoding="utf-8").read()
    assert 'pub const ENV_GRID_DIR: &str = "%s";' % W.GRID_ENV in rs
    assert 'pub const REQUEST_FILE: &str = "%s";' % W.GRID_REQUEST in rs
    assert "pub const FORMAT_VERSION: u32 = %d;" % W.GRID_VERSION in rs
    assert 'format!("{token}.grid")' in rs and 'format!("{token}.grid.tmp")' in rs, "the snapshot's file name, written whole by rename"
    for key in ('"v": FORMAT_VERSION', '"token": token', '"cols": cols', '"cursor": cursor', '"rows": rows', '"row": row.row', '"text": row_text(&row.cells)',
                '"cells": cells', '"shape": shape_name(cursor.shape)'):
        assert key in rs, "appci_grid.rs no longer writes %s" % key
    assert "json!([cell.col, cell.text, cell.fg, cell.bg, cell.attrs])" in rs, "a cell is [col, text, fg, bg, attrs]"
    for name in ('"beam"', '"underline"', '"block"', '"named:{}"', '"idx:{index}"', '"rgb:{:02x}{:02x}{:02x}"'):
        assert name in rs, name
    for letter in "bdiuUsrhWSL":
        assert "'%s')" % letter in rs, "attribute letter %s" % letter
    renderer = open(os.path.join(ROOT, "app", "src", "terminal", "grid_renderer.rs"), encoding="utf-8").read()
    assert "appci_grid::active_tokens()" in renderer and renderer.count("appci_grid::report_painted_grid(") == 2, "render_grid calls the hook"
    src = open(os.path.join(HERE, "app.py"), encoding="utf-8").read()
    import app as A
    assert 'export %s="%%(home)s/%s"' % (W.GRID_ENV, W.GRID_DIRNAME) in A.INNER
    assert src.count('"%s"' % W.GRID_DIRNAME) + src.count("/%s" % W.GRID_DIRNAME) >= 4 and src.count(W.GRID_ENV) >= 3, "Linux, macOS and Windows launchers"
    assert W.grid_dir("/h") == os.path.join("/h", W.GRID_DIRNAME)


@check
def w4core_freeze_pure_parts():
    """T36: the freeze proof, the typing tab, the ssh tab and the agent turn, each with the input a defect would produce"""
    # the process state
    assert W.stat_state("1234 (phosphor) T 1 1234 1234 0 -1 4194304") == "T"
    assert W.stat_state("1234 (a b) c) S 1 1234") == "S" and W.stat_state("") is None and W.stat_state("garbage") is None
    assert W.ps_state_stopped("T\n") is True and W.ps_state_stopped(" T+ \n") is True and W.ps_state_stopped("S\n") is False and W.ps_state_stopped("") is None
    assert W.freeze_proof(True, True, 100, 100)[0] is True
    assert W.freeze_proof(True, True, 100, 180) == (False, "the app's log grew by 80 bytes while it was reported stopped")
    assert W.freeze_proof(True, False, 5, 5)[0] is False and W.freeze_proof(False, True, 5, 5)[0] is False
    assert W.freeze_proof(None, True, 5, 5)[0] is None, "an unreadable state is not a proof and not a failure"
    # the typing tab
    assert W.judge_local(["held1", "aft1"], "held1", "aft1")[0] == "ok"
    assert W.judge_local(["aft1"], "held1", "aft1")[0] == "dropped"
    for bad in ([], ["held1"], ["aft1", "held1"], ["held1", "held1", "aft1"], ["heldaft1"], ["hel", "aft1"], ["held1", "aft"], ["held1", "aft1", "aft1"]):
        assert W.judge_local(bad, "held1", "aft1")[0] == "fail", bad
    assert W.judge_received(["echo h >> f", "echo a >> f"], "echo h >> f", "echo a >> f", "ok") is None
    assert W.judge_received(["echo a >> f"], "echo h >> f", "echo a >> f", "dropped") is None
    assert "expected exactly" in W.judge_received(["echo h >> fecho a >> f"], "echo h >> f", "echo a >> f", "ok")      # two lines merged into one
    assert W.judge_received(["echo h >> f", "echo a >> f"], "echo h >> f", "echo a >> f", "dropped") is not None
    # the ssh tab
    assert W.judge_ssh(["rp"], [], "rp", False)[0] == "alive"
    assert W.judge_ssh([], ["rp"], "rp", True)[0] == "dropped"
    v, text = W.judge_ssh([], ["rp"], "rp", False)
    assert v == "fail" and "did not say the session dropped" in text
    assert W.judge_ssh([], [], "rp", True) == ("fail", "the line reached neither the host nor a local shell: the tab is dead")
    assert W.judge_ssh(["rp"], ["rp"], "rp", True)[0] == "fail" and W.judge_ssh(["r", "p"], [], "rp", False)[0] == "fail" and W.judge_ssh(["rp", "rp"], [], "rp", False)[0] == "fail"
    # the agent turn
    stream = dict(total=1200, sent=1200, finished=True, broken=False)
    good = dict(verdict="ok", dup=0, foreign=[], user_text="Please stream long reply FWX chunks 1200 gap 100\nPlease follow up check FWX", chunks_seen=1200, chunks_expected=1200)
    assert W.judge_agent(stream, good, ("held1", "rp")) == (True, "the turn ended: every chunk and the END marker once")
    ok, text = W.judge_agent(stream, None, ())
    assert ok is False and "neither ended nor errored" in text and "1200/1200" in text
    assert W.judge_agent(None, good, ())[0] is False
    ok, text = W.judge_agent(dict(stream, finished=False, broken=True, sent=400), dict(good, verdict="chunks 400/1200; no END", chunks_seen=400), ())
    assert ok is True and "errored or was cut" in text and "400/1200" in text, "an errored turn that takes the follow-up is allowed, and said"
    assert W.judge_agent(stream, dict(good, verdict="dup 3", dup=3), ())[0] is False
    assert W.judge_agent(stream, dict(good, foreign=["S2"]), ())[0] is False
    ok, text = W.judge_agent(stream, dict(good, user_text=good["user_text"] + "\necho held1 >> fw.txt"), ("held1", "rp"))
    assert ok is False and "held1" in text, "a line typed in the typing tab that reached the conversation"
    # misrouting
    assert W.misrouted(("held1", "aft1"), {"the host's file": "rpre\nrp", "a model request": "hello"}) == []
    assert W.misrouted(("held1", "aft1"), {"the host's file": "rpre\nheld1", "a model request": "x aft1"}) == [("held1", "the host's file"), ("aft1", "a model request")]
    assert W.FREEZE_S >= 1 and W.STALL_S > 0 and W.WAKE_S > 0


@check
def w4core_registry_and_docs():
    """both cases are registered once, PROVING only (never SMOKE); freeze_and_wake is agent-only and in the fake pass; the lab arm is a PROVING_LAB
    glob that matches a registered arm; Windows panes are classified with a reason that names what is missing; docs/testing.md describes them"""
    reg = {c["name"]: c for c in cases.CASES}
    g, f = reg["grid_snapshot_render"], reg["freeze_and_wake"]
    assert [c["name"] for c in cases.CASES].count("grid_snapshot_render") == 1 and [c["name"] for c in cases.CASES].count("freeze_and_wake") == 1
    assert g["modes"] == ("terminal", "agent") and g["gui"] and f["modes"] == ("agent",) and f["gui"]
    for n in ("grid_snapshot_render", "freeze_and_wake"):
        assert n in cases.PROVING and n not in cases.SMOKE, n
    assert "freeze_and_wake" in cases.PROVING_FAKE and "grid_snapshot_render" not in cases.PROVING_FAKE
    assert "lab_freeze_and_wake.*" in cases.PROVING_LAB
    arms = [c for c in cases.CASES if c.get("lab") and labmod.name_matches("lab_freeze_and_wake.*", c["name"])]
    assert [c["name"] for c in arms] == ["lab_freeze_and_wake.bash.lan.nohelper"] and arms[0]["modes"] == ("agent",)
    assert winmap.skip_reason("grid_snapshot_render", "gitbash") is None and "Git Bash pane" in winmap.skip_reason("grid_snapshot_render", "powershell")
    for pane in ("powershell", "gitbash"):
        assert "NEEDS:" in winmap.skip_reason("freeze_and_wake", pane) and "NtSuspendProcess" in winmap.skip_reason("freeze_and_wake", pane)
    assert not winmap.unclassified(["grid_snapshot_render", "freeze_and_wake"])
    assert "NEEDS:" in W.NEEDS_FAKE
    doc = open(os.path.join(ROOT, "docs", "testing.md"), encoding="utf-8").read()
    for word in ("`grid_snapshot_render`", "`freeze_and_wake`", "`lab_freeze_and_wake.*`", "PHOSPHOR_APPCI_GRID_DIR", "selftest_w4core.py", "appci_grid_tests.rs",
                 "APPCI_FW_FREEZE_S", "APPCI_FW_WAKE_S", "APPCI_FW_STALL_S"):
        assert word in doc, "docs/testing.md does not mention %s" % word
