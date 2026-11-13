# appci/cases_r14_state.py: round 14, wave 2b (docs/design/r14-test-gaps.md 7.1, T16-T20; design note docs/design/r14-w2b.md). Five cases, all in
# cases.PROVING (owner rule 2026-11-09; no code moves a case out of it).
#   agent_hidden_tab_attention   T16, agent mode, scripted fake model. Tab 2 gets a question card while tab 1 is shown: the vertical tabs panel's row of
#                                tab 2 shows the waiting state (the app's `hop=vtab_row_status ... status=blocked`, logged from what the row draws), with
#                                nothing typed; typing in tab 1 is exact and does not answer the card; tab 2's key and Enter answer it; the row clears.
#   settings_ui_toggle_persists  T17. Settings opened by its key; vertical tabs and font size changed by keyboard (command palette): exactly those two keys
#                                change in settings.toml, the pane's `stty size` moves, both survive a relaunch, a hand edit is seen, and the hand edit
#                                that puts back the app's own last bytes is seen too (#2026).
#   osc52_write_read             T18. A program's OSC 52 write sets the clipboard; its OSC 52 read query gets no reply (the reader's DSR 5 control does);
#                                the app's `hop=osc52_clipboard` says the query arrived and was refused. Again over ssh.
#   history_survives_restart     T19. Commands typed before a quit and before a kill -9 are recalled after the relaunch by Up, by prefix + Up and by
#                                Ctrl-R: the shell receives exactly the original line, once. An agent prompt never enters the shell's history.
#   restore_shows_history        T20. restore_session on: after a stop and relaunch Find matches a token that only an earlier command's OUTPUT holds
#                                (and, in agent mode, the conversation's text); a token never printed does not match; nothing re-runs.
# Same rules as cases.py: the EXACT received line (rec.log; PSReadLine history in the PowerShell pane), typing at 10-25 ms/key with instant Enter, a case
# that cannot run SKIPS naming the missing capability, and every oracle has a control that a wrong result trips. Pure parts: selftest.py r14_w2b_*.
import base64, os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases as C  # noqa: E402  (cases.py imports this module near its end, so everything below is already defined)
import cases_agent_question as AQ  # noqa: E402
import cases_agent_resilience as AR  # noqa: E402
import cases_caseagent as AG  # noqa: E402
import cases_caseremote as RM  # noqa: E402
import cases_robust as R  # noqa: E402
import cases_ui as U  # noqa: E402
import cases_wrap_vtabs as WV  # noqa: E402
import toml_text as T  # noqa: E402
from cases import case  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
SENTINEL = "SENTINEL_NOT_SET_BY_OSC52"

# ================================================================= pure helpers (unit-tested in selftest.py)
# ---- T16: the vertical tabs panel's row status
ROW_STATUS_RE = re.compile(r"hop=vtab_row_status view=(\d+) status=(\w+)")       # app/src/workspace/view/vertical_tabs.rs log_row_status_hop
WAITING = "blocked"                                                              # ConversationStatus::Blocked: the agent waits for the user


def row_statuses(text):
    """[(view id, status name)] of every `hop=vtab_row_status` line, in order: what the panel drew as a terminal row's status badge, each change"""
    return ROW_STATUS_RE.findall(text or "")


def row_last(text, view):
    """the last status name logged for `view`, or None"""
    last = None
    for v, s in row_statuses(text):
        if v == view:
            last = s
    return last


def waiting_views(text):
    """the views whose row showed the waiting state, in first-seen order"""
    out = []
    for v, s in row_statuses(text):
        if s == WAITING and v not in out:
            out.append(v)
    return out


def attention_verdict(before, hidden):
    """-> (view | None, problems). `before`: the app log from the question prompt until tab 2 was hidden; `hidden`: the log since then (tab 1 shown,
    nothing typed after the probe). Pass: no row showed the waiting state before the question arrived, and exactly one view's row shows it now.
    A hop that never says `blocked` fails the second half, one that says it for every row or too early fails the first or the count."""
    probs = []
    early = waiting_views(before)
    if early:
        probs.append("a panel row showed the waiting state BEFORE the question arrived (view %s): the status is not the hidden tab's question" % ", ".join(early))
    now = waiting_views(hidden)
    if not now:
        seen = sorted({s for _v, s in row_statuses(hidden)})
        probs.append("no panel row shows the waiting state while the tab with the question is hidden (row statuses logged since it was hidden: %s)"
                     % (seen or "none: no hop=vtab_row_status line at all"))
    elif len(now) > 1:
        probs.append("%d panel rows show the waiting state (views %s); one tab has a question" % (len(now), ", ".join(now)))
    return (now[0] if len(now) == 1 else None), probs


# ---- T17: the settings file as flat keys
def toml_flat(text):
    """{'table.key': literal} for every `key = value` of a settings.toml TEXT (toml_text.flat, where the reader lives since round 16)"""
    return T.flat(text)


def changed_keys(a, b):
    """sorted keys whose literal differs between two toml_flat dicts (added and removed keys included)"""
    return sorted(k for k in set(a) | set(b) if a.get(k) != b.get(k))


VT_KEY = "appearance.vertical_tabs.enabled"
FONT_KEY = "appearance.text.font_size"


def num(lit):
    """a TOML number literal as float, or None"""
    try:
        return float(lit)
    except (TypeError, ValueError):
        return None


def stty_size(lines):
    """(rows, cols) from the output of `stty size`, or None"""
    m = re.match(r"\s*(\d+)\s+(\d+)\s*$", (lines or [""])[0])
    return (int(m.group(1)), int(m.group(2))) if m else None


def settings_delta_problems(before, after, want_vt):
    """what is wrong with the settings file after the two keyboard changes: `before` / `after` are toml_flat dicts, want_vt the literal the vertical
    tabs key must now hold. Exactly the two keys may differ; the font size must be a number other than the old one (old + 1 when the file had one)."""
    probs = []
    diff = changed_keys(before, after)
    other = [k for k in diff if k not in (VT_KEY, FONT_KEY)]
    if other:
        probs.append("keys other than the two that were changed differ in settings.toml: %s" % ", ".join(other))
    if after.get(VT_KEY) != want_vt:
        probs.append("%s is %r in the file, expected %s" % (VT_KEY, after.get(VT_KEY), want_vt))
    old, new = num(before.get(FONT_KEY)), num(after.get(FONT_KEY))
    if new is None:
        probs.append("%s is %r in the file after 'Increase font size': not written" % (FONT_KEY, after.get(FONT_KEY)))
    elif old is not None and abs(new - (old + 1.0)) > 1e-6:
        probs.append("%s went from %s to %s, expected one step up (%s)" % (FONT_KEY, old, new, old + 1.0))
    elif old is None and FONT_KEY not in diff:
        probs.append("%s did not change" % FONT_KEY)
    return probs


# ---- T18: OSC 52
OSC52_HOP_RE = re.compile(r"hop=osc52_clipboard op=(write|read) access=(\w+) allowed=(true|false)")       # Osc52ClipboardAccess::hop_detail


def osc52_hops(text):
    """[(op, access, allowed)] of every `hop=osc52_clipboard` line"""
    return [(op, acc, al == "true") for op, acc, al in OSC52_HOP_RE.findall(text or "")]


def osc52_expect(setting_line):
    """(write allowed, read allowed, setting name) for the profile's `terminal.osc52_clipboard_access` line (toml_text.get_key; None = not set: the
    default since #1963 is write_only). An owner seed that sets it is judged by what it says."""
    v = "write_only"
    if setting_line:
        m = re.search(r'=\s*"(\w+)"', setting_line)
        if m:
            v = m.group(1)
    return v in ("write_only", "read_write"), v == "read_write", v


def osc52_write_line(token):
    """the shell line that makes a program write `token` to the clipboard with OSC 52 (printf: bash, zsh, Git Bash; local and over ssh)"""
    return "printf '\\033]52;c;%s\\a'" % base64.b64encode(token.encode()).decode()


OSC52_QUERY_LINE = "printf '\\033]52;c;?\\a'"


def judge_reader(lines, secret):
    """what osc52_reader.py received: `lines` are its 'epoch hex' rows. -> dict(dsr: the DSR 5 control was answered, osc52: an OSC 52 reply arrived,
    leak: the base64 of `secret` (the clipboard's content) is in the bytes, raw: the bytes)"""
    raw = b""
    for l in lines or []:
        p = l.split()
        if len(p) == 2:
            try:
                raw += bytes.fromhex(p[1])
            except ValueError:
                pass
    b64 = base64.b64encode(secret.encode())
    return dict(dsr=b"\x1b[0n" in raw, osc52=b"\x1b]52;" in raw, leak=bool(secret) and (b64 in raw or secret.encode() in raw), raw=raw)


# ---- T19: history
def hist_line(shell, ident, tok, fname):
    """-> (line, prefix): the append-command of this pane's shell whose token starts with `ident`, and the start of that line up to and including
    `ident` (the prefix a user types before Up; unique to the line when `ident` is)"""
    line = C.Ctx.append_cmd(None, shell, ident + tok, fname)
    return line, line[:line.index(ident) + len(ident)]


def recall_problem(got, want):
    """None when the shell received exactly [want] for one recall; else what it received instead"""
    if got == [want]:
        return None
    if not got:
        return "the shell received nothing (the recalled line was not found, or not run)"
    if got.count(want) > 1:
        return "the shell received the line %d times: %r" % (got.count(want), got)
    if want in got:
        return "the shell received other lines besides it: %r" % (got,)
    return "the shell received %r, expected exactly [%r]" % (got, want)


def effect_twice(ran):
    """True when the effect lines of one round are three distinct tokens, typed in order and then each run exactly once more"""
    return len(ran) == 6 and len(set(ran[:3])) == 3 and sorted(ran[3:]) == sorted(ran[:3])


def count_in(lines, needle):
    return sum(1 for l in lines if needle in l)


# ---- T20: Find
def find_hops(text):
    """the payloads of the `hop=find_scroll` lines (#1775): 'result=scroll block=N moved=..' / 'result=scroll rich_content ..' / 'result=no_match ..'"""
    return [l.split("hop=find_scroll", 1)[1].strip()[:300] for l in (text or "").splitlines() if "hop=find_scroll" in l]


def find_result(hops):
    """'scroll' when the app's Find focused a match in the block list, 'no_match' when it ran and had none, 'none' when it logged nothing"""
    if any(h.startswith("result=scroll") for h in hops):
        return "scroll"
    return "no_match" if hops else "none"


def store_note(counts):
    """pure: what the stored block rows say about a restore that does not show the earlier output (#2110). counts = {'after_stop': (rows, rows that
    hold the token) | None, 'after_relaunch': the same}; None = the store could not be read."""
    a, b = counts.get("after_stop"), counts.get("after_relaunch")
    if a is None:
        return "the block store could not be read after the stop: whether the output was stored is not known"
    if a[1] == 0:
        return ("after the stop the store held %d block row(s), none with the token: the output was not stored, or the stop dropped it (the view "
                "has nothing to restore)" % a[0])
    if b is not None and b[1] == 0:
        return "the store held the token after the stop (%d row(s)) and not after the relaunch (%d row(s) left): the launch dropped the rows" % (a[1], b[0])
    return "the store holds the token (%d row(s) after the stop%s): the rows are there and the restored view does not show them" % (
        a[1], "" if b is None else ", %d after the relaunch" % b[1])


def stored_blocks_with(ctx, token):
    """(block rows, block rows whose command or output bytes hold `token`) over every warp.sqlite of the profile, read-only; None when no store
    or no `blocks` table can be read. A diagnostic for #2110, never a verdict: the output is stored as styled bytes, and a token that a style run
    happens to split would not be found."""
    import sqlite3
    import cases_coverage_a as CA2
    needle, rows, hits, seen = token.encode(), 0, 0, False
    for p in CA2.find_db(ctx):
        try:
            con = AR.sqlite_ro(p)
        except sqlite3.Error:
            continue
        try:
            for cmd, out in con.execute("SELECT stylized_command, stylized_output FROM blocks").fetchall():
                seen, rows = True, rows + 1
                blob = b"".join(x if isinstance(x, bytes) else str(x or "").encode() for x in (cmd, out))
                hits += 1 if needle in blob else 0
            seen = True
        except sqlite3.Error:
            pass
        finally:
            con.close()
    return (rows, hits) if seen else None


def split_token(tok):
    """(first half, second half): `printf '%s%s\\n' A B` prints the token while the command's own text never holds it"""
    k = len(tok) // 2
    return tok[:k], tok[k:]


# ================================================================= shared steps
def letters(ctx, n):
    return "".join(ctx.rng.choice("ABCDEFGHJKLMNPQRSTUVWXYZ") for _ in range(n))


def received(ctx, mark, hmark):
    """the lines the pane's shell received since the marks: rec.log, or the PSReadLine history in a pane with no rc hook"""
    if ctx.prof.local_prompts:
        return ctx.cmds_since(mark)
    return ctx.ps_history_lines()[hmark:]


def after_relaunch(ctx):
    """the first-run overlay of a relaunched app is dismissed in terminal mode only (Escape in a fresh agent view leaves it: cases.py mode_proof)"""
    if ctx.mode != "agent":
        ctx.be.key("Escape")
        time.sleep(0.5)


FIND_KEY = lambda ctx: os.environ.get("APPCI_KEY_FIND", "cmd+f" if ctx.be.name == "mac" else "ctrl+shift+f")      # noqa: E731  (terminal:find, as flood_scrollback_find)


def find_in_pane(ctx, tok, wait):
    """one Find for `tok` (cases_r14_w1a.find_enter: Find key, the field emptied, the token, Enter, Escape) -> the find_scroll hops logged AFTER
    the Enter. Round 14: the hops since the Find key were read; opening the bar re-runs the previous query, so the control for a token never
    printed saw the earlier token's `result=scroll`."""
    import cases_r14_w1a as W1A          # imported here: cases.py loads that module before this one
    return W1A.find_enter(ctx, tok, wait, shot="find_%s" % tok[:6].lower())[1]


def palette_run(ctx, text):
    """the command palette by keyboard: open, type the action's name, Enter"""
    ctx.be.guard()
    ctx.be.key(U.K(ctx, "palette"))
    time.sleep(1.0)
    ctx.be.type(text, 15)
    time.sleep(0.9)
    ctx.be.key("Return")
    time.sleep(0.6)


def pane_size(ctx, tag):
    """the pane's `stty size` as (rows, cols), or None when the shell did not answer"""
    f = "su_size_%s.txt" % tag
    ctx.rm(f)
    ctx.be.type("stty size > %s" % f, ctx.rng.randint(10, 25), enter=True)
    ctx.until(lambda: ctx.lines(f), 8)
    time.sleep(0.3)
    return stty_size(ctx.lines(f))


def tab_count_now(ctx, key_n):
    """press a tab key and read the window's tab count from the app's hop=tab_activate line; None without the line"""
    off = ctx.log_size()
    ctx.be.guard()
    ctx.be.key(R.tab_key(ctx, key_n))
    time.sleep(0.7)
    return WV.window_tab_count(ctx, off)


# ================================================================= T16 agent_hidden_tab_attention
def panel_on(ctx, r):
    """the vertical tabs panel for this case -> an object with restore(). Linux / macOS: WV.VTabs (taken as it is, or switched on live and restored).
    Windows: the live settings rewrite is not ported, so the case runs only when the profile already has the panel on."""
    if not ctx.win:
        return WV.VTabs(ctx, r)
    p = os.path.join(getattr(ctx.app, "config", "") or "", "settings.toml")
    text = open(p, encoding="utf-8", errors="replace").read() if os.path.isfile(p) else ""
    st = WV.vtabs_state(text)
    if st != "on":
        r.skip("Windows: vertical tabs are %s in this profile's settings and the harness cannot switch them on while the app runs there (the live "
               "settings rewrite of cases_ui.settings_path is not ported to the Windows config path); the row status hop needs the panel drawn" % st)

    class _Keep:
        how = "on in the profile's settings (Windows: taken as it is)"

        def restore(self):
            pass
    return _Keep()


@case("agent_hidden_tab_attention", modes=("agent",), order=54, gui=True)
def c_agent_hidden_tab_attention(ctx, r):
    """T16 (gap G24): an agent that waits for the user in a HIDDEN tab. Tab 2 asks the scripted model for a question that arrives 12 s later; tab 1 is
    shown meanwhile. With nothing typed, the vertical tabs panel's row of tab 2 shows the waiting state (hop=vtab_row_status status=blocked, after the
    switch, for exactly one view, none before). Typing in tab 1 is exact and leaves the card unanswered. Tab 2's key, then Enter, answers it."""
    AR.need_fake(ctx, r)
    if not WV.first_tab_in_front(ctx, r, "attn"):
        return
    vt = panel_on(ctx, r)
    r.note("vertical tabs: %s" % vt.how)
    f = AR.fake()
    opened = False
    try:
        f.clear_faults()
        # ---- tab 2, agent view, the question on its way
        p0 = ctx.prof.prompts()
        ctx.be.guard()
        ctx.be.key(R.NEWTAB)
        opened = True
        time.sleep(1.2)
        if ctx.prof.local_prompts:
            ctx.until(lambda: ctx.prof.prompts() > p0, 10, step=0.1)
        else:
            time.sleep(3.0)
        if ctx.log_view_mode() != "agent":
            ctx.be.type("/agent", 12, enter=True)
            if not ctx.until(lambda: ctx.log_view_mode() == "agent", 10):
                r.skip("tab 2 did not open in the agent view (hop=view_mode says %s): the question cannot be asked there" % ctx.log_view_mode())
            time.sleep(0.8)
        tok = "H" + ctx.tok(6).upper().replace("_", "")
        f.inject("ask_slow", times=1, match=tok, secs=12)
        off_q, tools0 = ctx.log_size(), len(f.tool_results)
        sent = "Please ask me a question %s" % tok
        AG.type_fast(ctx, sent)
        ctx.until(lambda: AG.enter_buffers(ctx.log_since(off_q)), 6)
        err = AG.judge_agent_input(sent, AG.enter_buffers(ctx.log_since(off_q)))
        r.check(err is None, "the question prompt typed in tab 2: %s" % err)
        if not AG.wait_marker(ctx, off_q, "[byop] opening stream", timeout=60):
            r.check(False, "tab 2: no model request within 60 s of the prompt (the question was never asked)")
            return
        r.check(ctx.until(lambda: bool(f.fired) and f.fired[-1] == "ask_slow", 10), "the slow-question script never reached the fake model")
        # ---- hide tab 2: tab 1 is shown and proven to answer, before the question arrives
        before = ctx.log_since(off_q)
        ctx.be.guard()
        ctx.be.key(R.tab_key(ctx, 1))
        time.sleep(0.8)
        off_hidden = ctx.log_size()
        got = WV.tab_answer(ctx, "attn_t1")
        if not r.check(got == R.norm_path(ctx.W), "the tab 1 key did not show tab 1 (the pane that answers is in %r): tab 2 is not hidden" % got):
            return
        if AQ.ask_blocked_ids(ctx.log_since(off_q)):
            r.unverifiable("the question card was already up before tab 2 was hidden and tab 1 had answered (the 12 s script was too short on this "
                           "runner): 'the row shows it while hidden, with nothing typed' is not judged")
            return
        # ---- nothing is typed from here until the row is judged
        t0 = time.time()
        if not ctx.until(lambda: bool(AQ.ask_blocked_ids(ctx.log_since(off_q))), 60, step=0.25):
            r.check(False, "the question never became answerable in the hidden tab within 60 s (ask actions %r, last turn state %r)"
                    % (AQ.ask_actions(ctx.log_since(off_q)), AR.last_turn_state(ctx.log_since(off_q))))
            return
        ctx.until(lambda: bool(waiting_views(ctx.log_since(off_hidden))), 5, step=0.1)
        r.metric("row_waiting_after_card_s", round(time.time() - t0, 2))
        hidden = ctx.log_since(off_hidden)
        view, probs = attention_verdict(before, hidden)
        r.metric("row_statuses_while_hidden", row_statuses(hidden)[-8:])
        for p in probs:
            r.check(False, p)
        ctx.shot("attn_tab2_waiting")
        seen = AG.ocr_has(ctx, "attn_panel_ocr", tok)
        r.note("the waiting state is an icon badge on the row (no text): the verdict is the row's own hop=vtab_row_status. OCR %s"
               % ("read the row's title %s in the panel (shots/*attn_tab2_waiting.png shows the badge)" % tok if seen
                  else "did not read the row's title" if seen is False else "is not available on this runner"))
        # ---- typing in tab 1: exact, and none of it answers the card
        U.exact_burst(ctx, r, "attn_tab1", 6)
        AG.submit_prose(ctx, r, "attn_tab1_prose", "please explain why build %s is odd, it's fine" % ctx.tok(5), "prose typed in tab 1 while tab 2 waits")
        time.sleep(0.8)
        log = ctx.log_since(off_q)
        r.check(bool(AQ.ask_open_ids(log)) and len(f.tool_results) == tools0,
                "typing in tab 1 answered or closed the question of the hidden tab 2 (open ask actions %r, tool results the model got %d)"
                % (AQ.ask_open_ids(log), len(f.tool_results) - tools0))
        if view:
            r.check(row_last(ctx.log_since(off_hidden), view) == WAITING,
                    "the row left the waiting state while the question was still unanswered (now %r)" % row_last(ctx.log_since(off_hidden), view))
        # ---- jump to tab 2, Enter answers
        ctx.be.guard()
        ctx.be.key(R.tab_key(ctx, 2))
        time.sleep(1.0)
        off_a = ctx.log_size()
        if r.check(AQ.answer_card(ctx, r, off_q, "attn"), "tab 2's key, then Enter, did not answer the card (no ask_user_question action reached done)"):
            state = AR.wait_turn(ctx, off_a, ("success", "error", "cancelled"), 60)
            r.check(state == "success", "after the answer the turn ended %r, expected `success`" % state)
            r.check(len(f.tool_results) > tools0 and any("alpha" in t or "ok" in t.lower() for _t, t in f.tool_results[-1:]),
                    "the model's next request carried no answer to the question")
        # ---- tab 1 again: the row of tab 2 no longer waits
        ctx.be.guard()
        ctx.be.key(R.tab_key(ctx, 1))
        time.sleep(0.8)
        if view:
            ctx.until(lambda: row_last(ctx.log_since(off_hidden), view) == "success", 10, step=0.25)
            last = row_last(ctx.log_since(off_hidden), view)
            r.check(last == "success", "after the answer the row of tab 2 shows %r, expected `success` (the waiting state did not clear)" % last)
        for p in AR.judge_errors(ctx.log_since(off_q), limit=2):
            r.check(False, p)
        r.check(ctx.app.alive(), "the app died in the hidden-tab attention case")
    finally:
        try:
            f.clear_faults()
            if opened:
                WV.close_extra_tabs(ctx, r, 2)
        finally:
            vt.restore()
            AG.cleanup_tmp(ctx)
    got = WV.tab_answer(ctx, "attn_end")
    r.check(got == R.norm_path(ctx.W), "after tab 2 was closed the pane that answers is in %r, expected the first tab (%r)" % (got, R.norm_path(ctx.W)))
    U.exact_burst(ctx, r, "attn_closed", 4)


# ================================================================= T17 settings_ui_toggle_persists
def wait_flat(ctx, path, pred, timeout):
    """poll the settings file until pred(toml_flat) holds -> the flat dict last read"""
    def read():
        try:
            return toml_flat(open(path, encoding="utf-8", errors="replace").read())
        except OSError:
            return {}
    ctx.until(lambda: pred(read()), timeout, step=0.25)
    return read()


def wait_size(ctx, tag, pred, timeout=10):
    """poll `stty size` until pred(size) holds -> the last size read (each poll types one probe line)"""
    end, i, s = time.time() + timeout, 0, None
    while True:
        i += 1
        s = pane_size(ctx, "%s_%d" % (tag, i))
        if (s is not None and pred(s)) or time.time() >= end:
            return s
        time.sleep(1.0)


@case("settings_ui_toggle_persists", order=88, gui=True)
def c_settings_ui_toggle_persists(ctx, r):
    """T17 (gap G25, #2026): a setting changed in the app by keyboard sticks and applies. Settings is opened by its key; 'vertical tab layout' and
    'Increase font size' are run from the command palette: exactly those two keys change in settings.toml and the pane's `stty size` moves. After a
    relaunch both are still in the file and in force. Then a hand edit is seen, and the hand edit that puts back the app's own last bytes is seen too."""
    if ctx.win:
        r.skip("Windows: the settings shortcut is not mapped in the Windows backend and the case rewrites the settings file while the app runs (not ported there)")
    R.need_relaunch_app(ctx, "settings_ui_toggle_persists")
    import cases_coverage_b as CB
    p = U.settings_path(ctx)
    if not os.path.isfile(p):
        r.skip("settings file %s not found" % p)
    orig = open(p, "rb").read()
    text0 = orig.decode("utf-8", errors="replace")
    vt0 = WV.vtabs_state(text0)
    if vt0 == "unknown" or not WV.vtabs_editable(text0):
        r.skip("appearance.vertical_tabs.enabled is written in a form the harness cannot read key by key (an inline table or a dotted key): "
               "'exactly those keys change' cannot be judged on this profile")
    flat0 = toml_flat(text0)
    want_vt = "false" if vt0 == "on" else "true"
    settings_open = False

    def close_settings(tag, n_before):
        how = C.editor_close(ctx, r)
        time.sleep(1.0)
        left = tab_count_now(ctx, 1)
        if n_before is not None and left is not None:
            r.check(left == n_before, "%s: %s tab(s) open after the settings tab was closed with %s, expected %d" % (tag, left, how, n_before))
        return how

    try:
        s0 = pane_size(ctx, "s0")
        if not r.check(s0 is not None, "precondition: the pane's shell did not answer `stty size` before anything was changed"):
            return
        # ---- A. settings by its key: one more tab
        n0 = tab_count_now(ctx, 1)
        ctx.be.guard()
        ctx.be.key(CB.SETTINGS_KEY["mac" if ctx.be.name == "mac" else "linux"])
        settings_open = True
        time.sleep(1.5)
        n1 = tab_count_now(ctx, 9)
        ctx.shot("su_settings_open")
        if n0 is None or n1 is None:
            r.note("no hop=tab_activate line in the app log: that the settings key opened a tab is not checked (shots/*su_settings_open.png)")
        else:
            r.check(n1 == n0 + 1, "the settings key did not open the settings tab: %d tab(s) before, %d after" % (n0, n1))
        # ---- B, C. the two changes, by keyboard
        palette_run(ctx, "vertical tab layout")
        flat = wait_flat(ctx, p, lambda d: d.get(VT_KEY) == want_vt, 10)
        r.check(flat.get(VT_KEY) == want_vt, "palette 'vertical tab layout' + Enter: %s is %r in settings.toml 10 s later, expected %s (it was %s)"
                % (VT_KEY, flat.get(VT_KEY), want_vt, vt0))
        f_old = num(flat0.get(FONT_KEY))
        palette_run(ctx, "Increase font size")
        flat = wait_flat(ctx, p, lambda d: num(d.get(FONT_KEY)) not in (None, f_old), 6)
        if num(flat.get(FONT_KEY)) in (None, f_old):
            r.note("palette 'Increase font size' + Enter did not change %s within 6 s (another palette row has the same text); trying the key ctrl-shift->" % FONT_KEY)
            try:
                ctx.be.key("ctrl+shift+greater")
            except Exception as e:
                r.note("the backend refused ctrl+shift+greater: %s" % e)
            flat = wait_flat(ctx, p, lambda d: num(d.get(FONT_KEY)) not in (None, f_old), 6)
        time.sleep(1.0)
        ctx.shot("su_changed")
        w_bytes = open(p, "rb").read()
        flat_w = toml_flat(w_bytes.decode("utf-8", errors="replace"))
        r.metric("settings_keys_changed", changed_keys(flat0, flat_w))
        for prob in settings_delta_problems(flat0, flat_w, want_vt):
            r.check(False, "after the two keyboard changes: %s" % prob)
        f_w = num(flat_w.get(FONT_KEY))
        if f_w is None or flat_w.get(VT_KEY) != want_vt:
            return                                          # nothing was written: the rest would judge a change that did not happen
        # ---- E. the screen changed: the pane has another size
        close_settings("after the changes", n0)
        settings_open = False
        s1 = pane_size(ctx, "s1")
        r.metric("stty_size", dict(before=s0, after_changes=s1))
        if not r.check(s1 is not None, "the shell pane did not answer `stty size` after the settings tab was closed (the settings page still has the keyboard?)"):
            return
        r.check(s1 != s0, "the pane is still %r after vertical tabs went %s and the font size went up: the changes are in the file but not on screen" % (s1, want_vt))
        # ---- F. relaunch: still in the file, and in force
        q = R.stop_for_restart(ctx, r, 20, "su")
        if not r.check(q["exited"], "the app did not exit for the relaunch (%r)" % q):
            return
        R.keep_log(ctx, "su_stop")
        _, up = R.start_app(ctx, r, "su_relaunch")
        if not up:
            return
        after_relaunch(ctx)
        flat_r = toml_flat(open(p, encoding="utf-8", errors="replace").read())
        r.check(flat_r.get(VT_KEY) == want_vt and num(flat_r.get(FONT_KEY)) == f_w,
                "after the relaunch settings.toml holds %s = %r and %s = %r, expected %s and %s (the changes did not survive)"
                % (VT_KEY, flat_r.get(VT_KEY), FONT_KEY, flat_r.get(FONT_KEY), want_vt, f_w))
        s1r = pane_size(ctx, "s1r")
        r.metric("stty_size_after_relaunch", s1r)
        # ---- G. the app writes once more in THIS process: one font step down. A process running on the file's value writes that value - 1;
        # one that ignored the file writes its own default - 1.
        palette_run(ctx, "Decrease font size")
        flat = wait_flat(ctx, p, lambda d: num(d.get(FONT_KEY)) == f_w - 1.0, 6)
        if num(flat.get(FONT_KEY)) != f_w - 1.0:
            try:
                ctx.be.key("ctrl+shift+less")
            except Exception as e:
                r.note("the backend refused ctrl+shift+less: %s" % e)
            flat = wait_flat(ctx, p, lambda d: num(d.get(FONT_KEY)) == f_w - 1.0, 6)
        if not r.check(num(flat.get(FONT_KEY)) == f_w - 1.0,
                       "after the relaunch 'Decrease font size' wrote %s = %r, expected %s (one step under the %s the file held: the relaunched app "
                       "was not running on the saved value, or the key did nothing)" % (FONT_KEY, flat.get(FONT_KEY), f_w - 1.0, f_w)):
            return
        time.sleep(1.0)
        w2 = open(p, "rb").read()
        s2 = wait_size(ctx, "s2", lambda s: s != s1r, 6)
        r.check(s2 is not None and s2 != s1r, "the pane is still %r after the font size went down by keyboard" % (s2,))
        # ---- H. a hand edit is seen (the control for I)
        U.set_setting(ctx, "appearance.text", "font_size", "%.1f" % (f_w + 6.0))
        s3 = wait_size(ctx, "s3", lambda s: s != s2, 10)
        seen_edit = r.check(s3 is not None and s3 != s2, "a hand edit of %s (to %.1f) was not applied within 10 s: the pane is still %r" % (FONT_KEY, f_w + 6.0, s3))
        # ---- I. #2026: the hand edit that puts back exactly the bytes the app wrote last
        with open(p, "wb") as fh:
            fh.write(w2)
        s4 = wait_size(ctx, "s4", lambda s: s == s2, 10)
        if seen_edit:
            r.check(s4 == s2, "#2026: the file was put back to the bytes the app itself wrote last (font size %.1f) and the app did not reload it: "
                              "the pane is %r, it was %r with those bytes" % (f_w - 1.0, s4, s2))
        # ---- J. the next write by the app keeps the undo, and vertical tabs alone move the pane
        back_vt = "true" if want_vt == "false" else "false"
        palette_run(ctx, "vertical tab layout")
        flat = wait_flat(ctx, p, lambda d: d.get(VT_KEY) == back_vt, 10)
        r.check(flat.get(VT_KEY) == back_vt, "palette 'vertical tab layout' + Enter (back): %s is %r, expected %s" % (VT_KEY, flat.get(VT_KEY), back_vt))
        r.check(num(flat.get(FONT_KEY)) == f_w - 1.0, "#2026: the app's next write put %s = %r into the file, expected %s: the hand edit that was undone "
                                                       "came back with the app's write" % (FONT_KEY, flat.get(FONT_KEY), f_w - 1.0))
        s5 = wait_size(ctx, "s5", lambda s: s != s4, 6)
        r.check(s5 is not None and s5 != s4, "the pane is still %r after vertical tabs went back to %s" % (s5, back_vt))
        ctx.shot("su_end")
        r.check(ctx.app.alive(), "the app died in the settings case")
    finally:
        try:
            if settings_open:
                C.editor_close(ctx, r)
                time.sleep(0.8)
        except Exception:
            pass
        try:
            if open(p, "rb").read() != orig:
                with open(p, "wb") as fh:
                    fh.write(orig)
                time.sleep(2.0)
        except OSError:
            pass
    U.exact_burst(ctx, r, "su_restored", 4)


# ================================================================= T18 osc52_write_read
def osc52_send(ctx, r, label, line, remote=False):
    """type one printf line at the owner rate and check the shell (local or remote) received exactly it -> the app-log offset before it"""
    off = ctx.log_size()
    mark = len(ctx.prof.commands(remote=True)) if remote else ctx.cmd_mark()
    ctx.be.type(line, ctx.rng.randint(10, 25), enter=True)
    got = lambda: [c for _t, c in ctx.prof.commands(remote=True)[mark:]] if remote else ctx.cmds_since(mark)      # noqa: E731
    ctx.until(lambda: line in got(), 8)
    r.check(got() == [line], "%s: the shell received %r, expected exactly [%r]" % (label, got(), line))
    return off


def osc52_write_arm(ctx, r, label, can_write, remote=False):
    tok = "osc" + ctx.tok(8).replace("_", "")
    ctx.be.clip_set(SENTINEL)
    time.sleep(0.3)
    off = osc52_send(ctx, r, label + " write", osc52_write_line(tok), remote)
    ctx.until(lambda: any(op == "write" for op, _a, _al in osc52_hops(ctx.log_since(off))), 6, step=0.1)
    time.sleep(0.6)
    clip = WV.norm_clip(ctx.be.clip_get()).strip("\n")
    hops = [h for h in osc52_hops(ctx.log_since(off)) if h[0] == "write"]
    r.metric("osc52_%s_write_hops" % label.replace(" ", "_"), hops)
    if not r.check(len(hops) == 1, "%s: the app logged %d OSC 52 write request(s) for one printed sequence (hop=osc52_clipboard op=write): %s"
                   % (label, len(hops), "the sequence never reached the app" if not hops else hops)):
        if not hops:
            r.check(clip != tok, "%s: the clipboard holds the token although the app logged no OSC 52 write" % label)
        return
    r.check(hops[0][2] == can_write, "%s: the app %s the OSC 52 write, the setting in force says it must be %s"
            % (label, "allowed" if hops[0][2] else "refused", "allowed" if can_write else "refused"))
    if can_write:
        r.check(clip == tok, "%s: after the program's OSC 52 write the clipboard holds %r, expected the token %r" % (label, clip[:40], tok))
    else:
        r.check(clip == SENTINEL, "%s: OSC 52 writes are denied by the setting, yet the clipboard changed to %r" % (label, clip[:40]))


def osc52_query_arm(ctx, r, label, can_read, remote=False):
    """the read query sent with printf: the hop says it arrived and what the app decided; no reply may land in the shell's input (the lines typed
    right after stay exact)"""
    secret = "sec" + ctx.tok(8).replace("_", "")
    ctx.be.clip_set(secret)
    time.sleep(0.3)
    off = osc52_send(ctx, r, label + " query", OSC52_QUERY_LINE, remote)
    ctx.until(lambda: any(op == "read" for op, _a, _al in osc52_hops(ctx.log_since(off))), 6, step=0.1)
    time.sleep(1.0)
    hops = [h for h in osc52_hops(ctx.log_since(off)) if h[0] == "read"]
    if r.check(len(hops) == 1, "%s: the app logged %d OSC 52 read request(s) for one query (hop=osc52_clipboard op=read): %s"
               % (label, len(hops), "the query never reached the app, so 'no reply' proves nothing" if not hops else hops)):
        r.check(hops[0][2] == can_read, "%s: the app %s the OSC 52 clipboard READ, the setting in force says it must be %s"
                % (label, "answered" if hops[0][2] else "refused", "answered" if can_read else "refused"))
    return secret


@case("osc52_write_read", order=64, gui=True)
def c_osc52_write_read(ctx, r):
    """T18 (gap G26, #1963): OSC 52 write allowed, read denied. A program's write sets the clipboard (a sentinel is there first); its read query gets
    no reply while the DSR 5 it sends with it IS answered; the app's hop=osc52_clipboard shows the query arrived and was refused. The same over ssh.
    The expectation follows terminal.osc52_clipboard_access when the profile sets it."""
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.skip("no app log at %s: the app's hop=osc52_clipboard lines cannot be read" % ctx.log_path)
    p = (os.path.join(getattr(ctx.app, "config", "") or "", "settings.toml") if ctx.win else U.settings_path(ctx))
    text = open(p, encoding="utf-8", errors="replace").read() if os.path.isfile(p) else ""
    can_write, can_read, setting = osc52_expect(T.get_key(text, "terminal", "osc52_clipboard_access"))
    r.metric("osc52_setting", setting)
    r.note("terminal.osc52_clipboard_access is %s in this profile: write %s, read %s"
           % (setting, "allowed" if can_write else "denied", "allowed" if can_read else "denied"))
    ctx.reset()
    try:
        # ---- local write
        osc52_write_arm(ctx, r, "local", can_write)
        # ---- local read: a raw reader where the harness Python has termios, else the printf query
        py = None if ctx.win else ctx.which("python3")
        if py:
            secret = "sec" + ctx.tok(8).replace("_", "")
            ctx.be.clip_set(secret)
            time.sleep(0.3)
            out = "o52_read_%s.txt" % ctx.mode
            ctx.rm(out, out + ".done")
            off = ctx.log_size()
            mark = ctx.cmd_mark()
            line = "python3 %s %s 6" % (os.path.join(HERE, "osc52_reader.py"), R.fwd(os.path.join(ctx.W, out)))
            ctx.be.type(line, ctx.rng.randint(10, 25), enter=True)
            done = ctx.until(lambda: os.path.exists(os.path.join(ctx.W, out + ".done")), 15, step=0.1)
            ctx.until(lambda: line in ctx.cmds_since(mark), 6)
            r.check(ctx.cmds_since(mark) == [line], "the reader line: the shell received %r, expected exactly [%r]" % (ctx.cmds_since(mark), line))
            if r.check(bool(done), "the OSC 52 reader did not finish within 15 s (it did not start, or the pane hangs): the read is not judged"):
                j = judge_reader(ctx.lines(out), secret)
                r.metric("osc52_reader", dict(dsr_answered=j["dsr"], osc52_reply=j["osc52"], bytes=len(j["raw"])))
                if r.check(j["dsr"], "control: the reader got no answer to DSR 5 (received %r): replies do not reach it, so 'no OSC 52 reply' proves nothing" % j["raw"][:40]):
                    if can_read:
                        r.check(j["osc52"] and j["leak"], "OSC 52 read is allowed by the setting, yet the reader got no reply holding the clipboard")
                    else:
                        r.check(not j["osc52"], "the program received a reply to its OSC 52 clipboard READ query although reads are denied: %r" % j["raw"][:60])
                        r.check(not j["leak"], "the clipboard's content reached the program that sent the OSC 52 read query")
                hops = [h for h in osc52_hops(ctx.log_since(off)) if h[0] == "read"]
                if r.check(len(hops) == 1, "the app logged %d OSC 52 read request(s) for the reader's one query (hop=osc52_clipboard op=read): %s"
                           % (len(hops), "the query never reached the app" if not hops else hops)):
                    r.check(hops[0][2] == can_read, "the app %s the OSC 52 read, the setting says it must be %s"
                            % ("answered" if hops[0][2] else "refused", "answered" if can_read else "refused"))
        else:
            r.note("no termios Python in this pane (%s): the read query is sent with printf; the app's hop is the verdict and the exact lines typed "
                   "right after it prove no reply landed in the shell's input" % ("Windows Git Bash" if ctx.win else "python3 not on PATH"))
            osc52_query_arm(ctx, r, "local", can_read)
        U.exact_burst(ctx, r, "o52_local", 4)
        # ---- the same from a remote shell
        if not ctx.ssh:
            why = getattr(ctx, "ssh_why", None) or C.NO_SSH_HOST
            if getattr(ctx, "ssh_required", False):
                r.no_reference("the ssh half was NOT judged: TEST HOST MISSING (%s)" % why)
            else:
                r.note("the ssh half was not run: %s" % why)
            return
        took = RM.connect(ctx)
        if not r.check(took is not None, "ssh fake: no remote shell prompt within 90 s; the ssh half is not judged"):
            ctx.return_to_local()
            return
        time.sleep(1.0)
        try:
            osc52_write_arm(ctx, r, "over ssh", can_write, remote=True)
            osc52_query_arm(ctx, r, "over ssh", can_read, remote=True)
            mark = len(ctx.prof.commands(remote=True))
            tok = ctx.tok(8)
            ctx.be.type("echo %s >> o52_remote.txt" % tok, ctx.rng.randint(10, 25), enter=True)
            ctx.until(lambda: len(ctx.prof.commands(remote=True)) > mark, 8)
            got = [c for _t, c in ctx.prof.commands(remote=True)[mark:]]
            r.check(got == ["echo %s >> o52_remote.txt" % tok], "the line typed in the remote shell right after the OSC 52 query: received %r "
                                                                 "(a reply to the query landed in the shell's input?)" % got)
        finally:
            if not RM.leave(ctx):
                r.note("the remote shell was not left with `exit` (the probe ran in %r); returning to the local shell by probes" % ctx.left_unproven)
                ctx.return_to_local()
        U.exact_burst(ctx, r, "o52_after_ssh", 3)
    finally:
        try:
            ctx.be.clip_set("")
        except Exception:
            pass


# ================================================================= T19 history_survives_restart
def hist_files(ctx):
    """the shell's own history file(s) of this pane: PSReadLine's in the PowerShell pane, else bash / zsh in the scratch home"""
    if ctx.win and ctx.pane == "powershell":
        return [x for x in [ctx.ps_history_path()] if x]
    return [os.path.join(ctx.prof.home, n) for n in (".bash_history", ".zsh_history")]


def hist_lines(ctx):
    out = []
    for f in hist_files(ctx):
        try:
            out += open(f, encoding="utf-8", errors="replace").read().splitlines()
        except OSError:
            pass
    return out


def recall(ctx, r, label, want, keys):
    """one recall gesture: `keys()` presses what brings the line back, then Enter (a second Enter when the first only filled the input, as ui_ctrl_r).
    The shell must receive exactly [want]."""
    ctx.reset()
    time.sleep(0.4)
    mark, hmark = ctx.cmd_mark(), len(ctx.ps_history_lines())
    keys()
    time.sleep(0.02)
    ctx.be.key("Return")
    if not ctx.until(lambda: received(ctx, mark, hmark), 1.5):
        ctx.be.key("Return")
    ctx.until(lambda: received(ctx, mark, hmark), 8)
    time.sleep(0.6)
    got = received(ctx, mark, hmark)
    prob = recall_problem(got, want)
    if not r.check(prob is None, "%s: %s" % (label, prob)):
        ctx.shot("hs_%s" % re.sub(r"\W+", "_", label)[:40])
    ctx.reset()


def history_round(ctx, r, how, fname, prompt_tok):
    """three commands, the stop (`how`: 'quit' | 'kill'), the relaunch, three recalls -> False when the round could not go on"""
    shell = ctx.pane_shell()
    # #2204: the idents are unique per RUN (ptok is unseeded; tok is seeded, so every run on a machine whose history file persists typed identical
    # commands and the file's earlier runs read as "typed more than once"), and the PowerShell history file is judged only from this round's lines
    rid = "hs" + ctx.ptok(8).replace("_", "")[-8:] + ("q" if how == "quit" else "k")
    hist_base = len(hist_lines(ctx)) if (ctx.win and ctx.pane == "powershell") else 0
    cmds = []
    mark, hmark = ctx.cmd_mark(), len(ctx.ps_history_lines())
    for i in (1, 2, 3):
        line, prefix = hist_line(shell, "%s%d" % (rid, i), ctx.tok(6).replace("_", ""), fname)
        cmds.append((line, prefix, "%s%d" % (rid, i)))
        ctx.be.type(line, ctx.rng.randint(10, 25), enter=True)
        time.sleep(0.3)
    ctx.until(lambda: count_in(ctx.lines(fname), rid) >= 3, 12)
    ctx.until(lambda: len(received(ctx, mark, hmark)) >= 3, 6)
    got = received(ctx, mark, hmark)
    if not r.check(got == [c[0] for c in cmds], "%s round, setup: the shell received %r, expected the three typed lines %r" % (how, got, [c[0] for c in cmds])):
        return False
    time.sleep(3.0)                                      # the app's own store has the commands before a hard stop (a quit flushes it itself)
    # ---- the stop
    if how == "quit":
        q = R.stop_for_restart(ctx, r, 20, "hs_quit")
        if q.get("how"):
            r.note("no quit key on this platform: the app was stopped by pid (%s); this round proves history after a stop, the next after kill -9" % q["how"])
        if not r.check(q["exited"], "the app did not exit for the relaunch (%r)" % q):
            return False
        R.keep_log(ctx, "hs_quit")
    elif not R.hard_kill(ctx, r, "hs_kill"):
        return False
    time.sleep(1.5)                                      # a shell that got SIGHUP writes its history file now
    hl = hist_lines(ctx)[hist_base:]
    counts = {ident: count_in(hl, ident) for _l, _p, ident in cmds}
    r.metric("history_file_counts_after_%s" % how, counts)
    r.check(all(n <= 1 for n in counts.values()), "after the %s the shell's history file holds a typed command more than once: %r" % (how, counts))
    if prompt_tok:
        r.check(not count_in(hl, prompt_tok), "the agent prompt is in the SHELL's history file after the %s (%d line(s))" % (how, count_in(hl, prompt_tok)))
    _, up = R.start_app(ctx, r, "hs_%s_relaunch" % how)
    if not up:
        return False
    after_relaunch(ctx)
    ctx.shot("hs_%s_relaunched" % how)
    # ---- three recalls, one per command, three gestures
    c1, c2, c3 = cmds
    if ctx.win:
        # the Windows harness types a key-proof line into the pane after every relaunch (run.py prove_keys), so the newest line is not c3:
        # the newest line that STARTS like c3 is
        recall(ctx, r, "%s round, c3 prefix + Up + Enter (the newest such line)" % how, c3[0],
               lambda: (ctx.be.type(c3[1], ctx.rng.randint(10, 25)), time.sleep(0.02), ctx.be.key("Up")))
    else:
        recall(ctx, r, "%s round, Up + Enter (the last command before the %s)" % (how, how), c3[0], lambda: ctx.be.key("Up"))
    recall(ctx, r, "%s round, typed prefix + Up + Enter" % how, c1[0],
           lambda: (ctx.be.type(c1[1], ctx.rng.randint(10, 25)), time.sleep(0.02), ctx.be.key("Up")))
    recall(ctx, r, "%s round, Ctrl-R + fragment + Enter" % how, c2[0],
           lambda: (ctx.be.key("ctrl+r"), time.sleep(0.4), ctx.be.type(c2[2], ctx.rng.randint(10, 25)), time.sleep(0.6)))
    ctx.until(lambda: count_in(ctx.lines(fname), rid) >= 6, 6)
    ran = [l for l in ctx.lines(fname) if rid in l]
    r.check(effect_twice(ran),
            "%s round: the effect file has %d line(s) of this round (%r), expected each of the three tokens twice (typed once, recalled once)" % (how, len(ran), ran))
    return True


@case("history_survives_restart", order=90, gui=True)
def c_history_survives_restart(ctx, r):
    """T19 (gap G27): commands typed before a quit, and before a kill -9, come back after the relaunch: Up + Enter runs the last one, a typed prefix +
    Up + Enter and Ctrl-R + a fragment run the others, each exactly as typed and exactly once. The shell's history file holds no command twice, and
    never the agent prompt typed between the two rounds (agent mode)."""
    R.need_relaunch_app(ctx, "history_survives_restart")
    fname = "hs_%s.txt" % ctx.mode
    ctx.rm(fname)
    ctx.reset()
    prompt_tok = None
    if not history_round(ctx, r, "quit", fname, None):
        return
    if ctx.mode == "agent" and ctx.log_path and os.path.exists(ctx.log_path) and ctx.log_view_mode() == "agent":
        prompt_tok = "hsp" + ctx.tok(6).replace("_", "")
        AG.submit_prose(ctx, r, "hs_agent_prompt", "please explain what %s means for the build, it's odd" % prompt_tok, "the agent prompt between the two rounds")
        time.sleep(0.8)
    elif ctx.mode == "agent":
        r.note("the pane is not in the agent view after the relaunch (hop=view_mode says %s): no agent prompt was typed, so 'the prompt is not in the "
               "shell's history' is not judged" % ctx.log_view_mode())
    if not history_round(ctx, r, "kill", fname, prompt_tok):
        return
    if prompt_tok:
        hl = hist_lines(ctx)
        r.check(not count_in(hl, prompt_tok), "the agent prompt is in the SHELL's history file (%d line(s))" % count_in(hl, prompt_tok))
        r.check(not [c for _t, c in ctx.prof.commands() if prompt_tok in c], "the agent prompt reached the shell as a command")
    r.check(ctx.app.alive(), "the app died in the history case")


# ================================================================= T20 restore_shows_history
@case("restore_shows_history", order=86, gui=True)
def c_restore_shows_history(ctx, r):
    """T20 (gap G28): restore_session on. A token that only a command's OUTPUT holds, and in agent mode one agent turn; stop, relaunch: Find matches the
    token in the restored view (the app's hop=find_scroll result=scroll), a token never printed does not, the conversation's text matches too, and
    nothing re-ran (effect file, rec.log)."""
    R.need_relaunch_app(ctx, "restore_shows_history")
    if not ctx.log_path:
        r.skip("no app log: the app's hop=find_scroll lines cannot be read")
    import fake_model
    eff = "rs_eff_%s.txt" % ctx.mode
    try:
        q = R.stop_for_restart(ctx, r, 20, "rs_setup")
        if not r.check(q["exited"], "rs_setup: the app did not exit within 20 s (%r)" % q):
            return
        R.keep_log(ctx, "rs_setup")
        R.set_restore(ctx, True)
        _, up = R.start_app(ctx, r, "rs_first")
        if not up:
            return
        after_relaunch(ctx)
        ctx.rm(eff)
        # ---- the block: the token is in the OUTPUT only
        tok, never = letters(ctx, 14), letters(ctx, 14)
        a, b = split_token(tok)
        etok = ctx.tok(6)
        cmd = "printf '%%s%%s\\n' %s %s; echo %s >> %s" % (a, b, etok, R.fwd(os.path.join(ctx.W, eff)))
        mark = ctx.cmd_mark()
        ctx.be.type(cmd, ctx.rng.randint(10, 25), enter=True)
        ctx.until(lambda: ctx.lines(eff), 10)
        ctx.await_commands(mark, 1)
        if not r.check(ctx.cmds_since(mark) == [cmd] and ctx.lines(eff) == [etok],
                       "setup: the shell received %r (effect file %r), expected exactly the printf line once" % (ctx.cmds_since(mark), ctx.lines(eff))):
            return
        time.sleep(0.8)
        # ---- agent mode: one agent turn
        conv_tok, conv_why = None, None
        if ctx.mode == "agent":
            if not ctx.ollama:
                conv_why = "no provider is configured (APPCI_OLLAMA_URL)"
            elif ctx.log_view_mode() != "agent":
                conv_why = "the pane is not in the agent view (hop=view_mode says %s)" % ctx.log_view_mode()
            else:
                off = ctx.log_size()
                if os.environ.get("APPCI_FAKE_MODEL_ACTIVE") == "1" and fake_model.ACTIVE is not None:
                    tag = "RS" + letters(ctx, 8)
                    AG.type_fast(ctx, "Please stream long reply %s chunks 3 gap 20" % tag)
                    conv_tok = "%s:END" % tag                 # only the REPLY holds it
                else:
                    conv_tok = "RS" + letters(ctx, 10)
                    AG.type_fast(ctx, "Reply with the word %s" % conv_tok)
                AG.wait_marker(ctx, off, "[byop] opening stream", timeout=60)
                state = AR.wait_turn(ctx, off, ("success", "error", "cancelled"), 120)
                if state != "success":
                    conv_why, conv_tok = "the agent turn ended %r, not `success`" % state, None
                time.sleep(1.5)
        # ---- preconditions: Find sees both now
        pre = find_in_pane(ctx, tok, 10)
        if not r.check(find_result(pre) == "scroll", "precondition: BEFORE the stop, Find for the output token gave %s (%r): the restore half cannot be "
                                                     "judged" % (find_result(pre), pre[-1:])):
            return
        if conv_tok:
            pre_c = find_in_pane(ctx, conv_tok, 10)
            if find_result(pre_c) != "scroll":
                conv_why, conv_tok = "before the stop, Find for the conversation's text gave %s (%r): Find does not reach it in this view" % (find_result(pre_c), pre_c[-1:]), None
        r.check(not [c for c in ctx.cmds_since(mark) if c != cmd], "the Find text was typed into the shell: %r" % ctx.cmds_since(mark)[1:])
        time.sleep(3.0)                                      # the session writer has run before a stop by pid
        eff_before = ctx.lines(eff)
        ctx.shot("rs_before_stop")
        # ---- stop, relaunch
        q = R.stop_for_restart(ctx, r, 20, "rs")
        if q.get("how"):
            r.note("no quit key on this platform: the app was stopped by pid (%s); this proves the restore after a stop, not after a quit" % q["how"])
        if not r.check(q["exited"], "the app did not exit for the restore (%r)" % q):
            return
        R.keep_log(ctx, "rs_stop")
        store = {"after_stop": stored_blocks_with(ctx, tok)}             # #2110: what the store holds, before the next launch touches it
        mark2 = ctx.cmd_mark()
        newp, up = R.start_app(ctx, r, "rs_restored", restored_min=1)
        if not up:
            return
        ctx.shot("rs_restored")
        store["after_relaunch"] = stored_blocks_with(ctx, tok)
        r.metric("rs_store", store)
        # ---- nothing re-ran
        r.check(ctx.lines(eff) == eff_before, "RE-RUN: the effect file changed across the restore: before %r, after %r" % (eff_before, ctx.lines(eff)))
        ran = [c for c in ctx.cmds_since(mark2)]
        r.check(not ran, "RE-RUN: the restored shell received %d command(s) before any key was sent: %r" % (len(ran), ran[:4]))
        # ---- the restored view holds the output: Find matches it; the control token does not match
        hops = find_in_pane(ctx, tok, 12)
        r.metric("find_after_restore", hops[-2:])
        r.check(find_result(hops) == "scroll", "after the restore Find for the token the earlier command printed gave %s (%s): the restored view does "
                                               "not show the earlier output. Store (#2110): %s" % (find_result(hops), hops[-1:] or "no hop=find_scroll line after Enter",
                                                                                                    store_note(store)))
        ctl = find_in_pane(ctx, never, 3)
        r.check(find_result(ctl) != "scroll", "control: after Enter, Find for a token that was never printed went to a match (%r): the Find oracle "
                                              "cannot tell a restored block from anything" % [h for h in ctl if h.startswith("result=scroll")][-1:])
        if find_result(ctl) == "none":
            r.note("control: Find for a token never printed logged no hop=find_scroll line after its Enter")
        if ctx.mode == "agent":
            if conv_tok:
                hc = find_in_pane(ctx, conv_tok, 12)
                r.metric("find_conversation_after_restore", hc[-2:])
                r.check(find_result(hc) == "scroll", "after the restore Find for the conversation's text gave %s (%s): the earlier agent conversation "
                                                     "is not shown in the restored view" % (find_result(hc), hc[-1:] or "no hop=find_scroll line"))
            else:
                r.no_reference("the conversation half was NOT judged: %s" % conv_why)
        r.check(not ctx.cmds_since(mark2), "the Find text was typed into the restored shell: %r" % ctx.cmds_since(mark2)[:3])
        r.check(ctx.lines(eff) == eff_before, "RE-RUN: the effect file changed while the restored view was searched: %r" % ctx.lines(eff))
        r.check(ctx.app.alive(), "the app died in the restore case")
    finally:
        R.fresh_app(ctx, r, "rs")
        after_relaunch(ctx)
