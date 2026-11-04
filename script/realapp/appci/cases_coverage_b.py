# appci/cases_coverage_b.py: real-app cases for the CI-review coverage gaps #1542 (agent flows), #1544 (code editor on macOS), #1545 (settings UI and
# settings migration), #1546 (state surviving relaunches), #1547 (auto-update is off), #1548 (UI-action budgets). Same rules as cases.py: the exact
# bytes the shell / agent received (rec.log, the app log's `hop=enter_begin` line), typing at 10-25 ms/key with an instant Enter, screenshot proof
# per step, a case that cannot run SKIPS with the precise reason. Registered by the one import line at the end of cases.py. Every case is full tier.
# What is NOT here, and why, is in docs/testing.md 2.14 (upgrade-over-previous install, uninstall, `phosphor://` end to end, a real signed update).
import os, re, sqlite3, sys, tarfile, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases as C  # noqa: E402  (cases.py imports this module as its LAST lines, so everything below is already defined)
import cases_caseagent as CA  # noqa: E402
import cases_ui as U  # noqa: E402
import cases_robust as R  # noqa: E402
from cases import case  # noqa: E402


# ---------------------------------------------------------------- pure helpers (unit-tested in selftest.py)
def legacy_settings(orig, tok):
    """`orig` (the profile's settings.toml text) with what a settings file from another version carries: a comment, a top-level key and a whole table
    this build has never heard of, and a known table with an extra unknown key. Returns (text, markers): the strings that must still be in the
    file if the app writes it back without dropping the user's entries."""
    markers = ["# written by an older phosphor %s" % tok, "legacy_removed_key_%s = true" % tok, "[legacy_removed_table_%s]" % tok, "old_value = \"keep-me-%s\"" % tok]
    head = markers[0] + "\n" + markers[1] + "\n"
    tail = "\n" + markers[2] + "\n" + markers[3] + "\n"
    return head + (orig if orig.endswith("\n") or not orig else orig + "\n") + tail, markers


def corrupt_settings(orig):
    """a truncated, unparsable settings file (power loss in the middle of a write): the first half of the text plus an unterminated string"""
    return orig[: max(1, len(orig) // 2)] + '\n[broken\nkey = "unterminated\n'


def startup_log_problems(text):
    """lines of an app-log excerpt that mean a failed start/migration: a panic, or a migration / database / settings line at ERROR level"""
    out = []
    for line in text.splitlines():
        low = line.lower()
        if "panicked at" in line:
            out.append(line.strip()[:200])
        elif (" error " in low or "[error]" in low or " error:" in low) and any(k in low for k in ("migrat", "sqlite", "diesel", "database", "settings file", "settings.toml")):
            out.append(line.strip()[:200])
    return out


AUTOUPDATE_RE = re.compile(r"(?i)(auto[-_ ]?update|check(?:ing)? for updates?|update available|downloading update|releases/latest|SHA256SUMS)")


def autoupdate_lines(text):
    """app-log lines that show the updater doing something. Phosphor ships WITHOUT the GUI auto-updater (DECLINED.md, script/check_no_autoupdate):
    a release build must log none of these."""
    return [l.strip()[:200] for l in text.splitlines() if AUTOUPDATE_RE.search(l)]


def ai_submits(log_text):
    """The `hop=enter_begin` rows (buffer, input_type) of an app-log excerpt that SUBMITTED to the agent: input type AI and not taken by a menu.
    The app logs enter_begin for every Enter, including one the @ picker takes; that Enter is followed (before the next Enter) by
    `hop=enter_menu_accept` (#1633). Counting every enter_begin as a submit failed an Enter that had accepted the picker row."""
    out = []
    begins = [m.start() for m in re.finditer(r"hop=enter_begin\b", log_text)] + [len(log_text)]
    for i in range(len(begins) - 1):
        seg = log_text[begins[i]:begins[i + 1]]
        rows = CA.enter_buffers(seg)
        if not rows or rows[0][1] != "AI":
            continue
        if re.search(r"hop=enter_menu_accept\b", seg):
            continue
        out.append(rows[0])
    return out


def picker_verdict(open_diff, closed_diff, min_open=0.004):
    """(ok, why) for a picker that must appear on `/` or `@` and be gone after Escape. open_diff = fraction of changed pixels against the screenshot
    before the key, closed_diff = the same after Escape. None (no Pillow) is 'cannot judge'. The menu must change the screen by at least min_open and
    Escape must bring back at least 3/4 of the difference away."""
    if open_diff is None or closed_diff is None:
        return None, "no Pillow (python3-pil): the picker change cannot be measured, screenshots only"
    if open_diff < min_open:
        return False, "the picker did not appear: the screen changed by %.4f (< %.4f)" % (open_diff, min_open)
    if closed_diff > open_diff * 0.25:
        return False, "Escape did not close the picker: %.4f of the screen still differs (it differed %.4f while open)" % (closed_diff, open_diff)
    return True, ""


# ---------------------------------------------------------------- #1542 agent flows: slash / @ pickers, @-mention of a file
def _view(ctx):
    return ctx.log_view_mode()


def _back_to_agent(ctx):
    """leave the case in the agent view it started in"""
    for _ in range(2):
        if _view(ctx) == "agent":
            return
        ctx.be.key("cmd+Return" if ctx.be.name == "mac" else "ctrl+shift+Return")
        CA.wait_view_mode(ctx, "agent")


@case("cb_agent_pickers", modes=("agent",), order=51, gui=True)
def c_agent_pickers(ctx, r):
    """#1542: in the agent view `/` opens the slash-command picker and `@` the context picker; the picker is visible (screenshot differs from
    the one before the key), Escape closes it, nothing is sent to the shell, and a prose line typed afterwards reaches the agent byte for byte
    (hop=enter_begin ... input_type=AI). Escape on a picker must not leave the agent view when it had a picker to close (noted, not failed, if
    it does: the case then goes back in and says so). Needs no model."""
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.skip("no app log at %s: the agent input at Enter cannot be read" % ctx.log_path)
    if ctx.log_view_mode() != "agent":
        r.skip("the app log does not say hop=view_mode mode=agent (older build, or not in the agent view)")
    mark = ctx.cmd_mark()
    for trig, name in (("/", "slash"), ("@", "at")):
        ctx.reset()
        time.sleep(0.4)
        before = ctx.shot("cb_picker_%s_before" % name)
        ctx.be.type(trig, ctx.rng.randint(10, 25), enter=False)
        time.sleep(0.9)
        opened = ctx.shot("cb_picker_%s_open" % name)
        ctx.be.key("Escape")
        time.sleep(0.6)
        left = _view(ctx) != "agent"
        closed = ctx.shot("cb_picker_%s_closed" % name)
        od = U.img_diff(before, opened) if before and opened else None
        cd = U.img_diff(before, closed) if before and closed else None
        r.metric("%s_open_diff" % name, od if od is not None else -1)
        ok, why = picker_verdict(od, cd)
        if ok is None:
            r.note("%s picker: %s" % (name, why))
        else:
            r.check(ok, "%s picker (%s): %s" % (name, trig, why))
        if left:
            r.note("%s picker: Escape left the agent view (hop=view_mode); the case goes back in" % name)
            _back_to_agent(ctx)
    ctx.reset()
    r.check(not ctx.cmds_since(mark), "a picker key reached the shell: %r" % ctx.cmds_since(mark)[:3])
    r.check(_view(ctx) == "agent", "the view is not agent after the pickers (hop=view_mode says %s)" % _view(ctx))
    CA.submit_prose(ctx, r, "cb_picker_after", "please explain why the picker %s run is odd, it's fine" % ctx.tok(6), "prose after the pickers")
    ctx.reset()


@case("cb_agent_file_mention", modes=("agent",), order=52, gui=True)
def c_agent_file_mention(ctx, r):
    """#1542 (attachments): `@<prefix of a real file>` lists that file in the picker (OCR when an engine exists); Enter accepts the row
    WITHOUT submitting (every AI hop=enter_begin is followed by hop=enter_menu_accept, #1633; nothing at the shell); the file's name is then in the input; the line is
    cleared and a following prose line still arrives exactly. The agent request itself is not sent (no model needed)."""
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.skip("no app log at %s" % ctx.log_path)
    if ctx.log_view_mode() != "agent":
        r.skip("the app log does not say hop=view_mode mode=agent")
    tok = ctx.tok(6).lower()
    fname = "cbmention_%s.txt" % tok
    path = os.path.join(ctx.W, fname)
    open(path, "w").write("mention target %s\n" % tok)
    ctx.reset()
    mark, off = ctx.cmd_mark(), ctx.log_size()
    ctx.be.type("@cbmention_%s" % tok, ctx.rng.randint(10, 25), enter=False)
    time.sleep(1.5)
    ctx.shot("cb_mention_picker")
    seen = CA.ocr_has(ctx, "cb_mention_picker_ocr", fname)
    if seen is None:
        r.note("no OCR engine: whether the picker lists %s is in shots/ only" % fname)
    else:
        r.check(seen, "the @ picker does not list the file %s that exists in the working directory" % fname)
    ctx.be.key("Return")                       # accept the highlighted row; a picker must consume this Enter
    time.sleep(0.8)
    ctx.shot("cb_mention_accepted")
    submitted = [b for b, _t in ai_submits(ctx.log_since(off))]
    r.check(not submitted, "Enter on a picker row SUBMITTED to the agent instead of accepting the row: %r" % submitted[:1])
    r.check(not ctx.cmds_since(mark), "Enter on a picker row reached the shell: %r" % ctx.cmds_since(mark)[:2])
    kept = CA.ocr_has(ctx, "cb_mention_input_ocr", tok)
    if kept is not None:
        r.check(kept, "after accepting the row the input does not show the file mention (%s)" % tok)
    r.check(_view(ctx) == "agent", "the view left agent mode while accepting the mention")
    ctx.reset()
    CA.submit_prose(ctx, r, "cb_mention_after", "what is in the file that I just mentioned %s, it's short" % ctx.tok(5), "prose after the mention")
    ctx.reset()
    try:
        os.remove(path)
    except OSError:
        pass


# ---------------------------------------------------------------- #1545 settings: the settings UI, and a settings file from another version
SETTINGS_KEY = {"linux": "ctrl+comma", "mac": "cmd+,"}     # the backends spell the comma differently (xdotool/wtype keysym vs the macOS key table)


@case("cb_settings_ui_open", order=87, gui=True)
def c_settings_ui_open(ctx, r):
    """#1545: the settings page opens from its shortcut (Ctrl-, / Cmd-,): the screen changes and, with OCR, shows the page; it is closed again with
    Escape (then, if still open, the close-pane key); the shell pane behind it still takes typing and receives the EXACT line (no key leaked into
    the page, none swallowed by it). Terminal and agent mode."""
    if ctx.win:
        r.skip("Windows: the settings shortcut is not mapped in the Windows backend (Ctrl-, via SendKeys is not verified); covered by the live-reload case on Linux/macOS")
    key = SETTINGS_KEY["mac" if ctx.be.name == "mac" else "linux"]
    ctx.reset()
    time.sleep(0.3)
    before = ctx.shot("cb_settings_before")
    ctx.be.key(key)
    time.sleep(1.2)
    opened = ctx.shot("cb_settings_open")
    d = U.img_diff(before, opened) if before and opened else None
    r.metric("settings_open_diff", d if d is not None else -1)
    if d is not None:
        r.check(d >= 0.02, "the settings page did not open from %s: the screen changed by %.4f" % (key, d))
    else:
        r.note("no Pillow: the settings page opening is in shots/ only")
    seen = CA.ocr_has(ctx, "cb_settings_ocr", "appearance")
    if seen is not None:
        r.check(seen or CA.ocr_has(ctx, "cb_settings_ocr2", "settings") is True, "the settings page shows neither 'Appearance' nor 'Settings' (OCR)")
    ctx.be.key("Escape")
    time.sleep(0.8)
    closed = ctx.shot("cb_settings_closed")
    cd = U.img_diff(before, closed) if before and closed else None
    if cd is not None and cd > 0.02 and R.CLOSE_PANE:
        r.note("Escape left the settings page open (%.3f of the screen differs); closing it with the pane-close key" % cd)
        ctx.be.key(R.CLOSE_PANE)
        time.sleep(0.8)
    ctx.reset()
    if ctx.mode == "agent":
        CA.submit_prose(ctx, r, "cb_settings_prose", "please explain why the settings %s test is odd, it's fine" % ctx.tok(5), "prose after the settings page")
        ctx.reset()
    else:
        U.exact_burst(ctx, r, "cb_settings_after", 4)
    r.check(ctx.app.alive(), "app died while opening/closing the settings page")


def _relaunch_ok(ctx, r, tag):
    t = U.relaunch(ctx, False)
    r.metric("%s_start_s" % tag, t if t is not None else -1)
    return t is not None


@case("cb_settings_migration", order=88, gui=True)
def c_settings_migration(ctx, r):
    """#1545: a settings.toml written by another version (unknown comment/key/table, a known table with an unknown key), then a TRUNCATED, unparsable
    one: the app must start (a shell prompt within 90 s), take exact typing, log no panic and no migration/settings ERROR. Whether the unknown
    entries survive a write-back by the app is a metric (`legacy_entries_kept`), not a verdict: the file is only compared, never judged by a
    setting change the app made. The original file is put back at the end and the app restarted on it."""
    if ctx.app is None or not hasattr(ctx.app, "stop"):
        r.skip("sim backend: no app process to relaunch")
    p = U.settings_path(ctx)
    if not os.path.isfile(p):
        r.skip("settings file %s not found" % p)
    orig = open(p, "rb").read()
    text = orig.decode("utf-8", "replace")
    tok = ctx.tok(5)
    legacy, markers = legacy_settings(text, tok)
    try:
        for name, body in (("legacy", legacy), ("truncated", corrupt_settings(text))):
            open(p, "wb").write(body.encode("utf-8"))
            ctx.snapshot_log()
            ok = _relaunch_ok(ctx, r, name)
            if not r.check(ok, "%s settings file: the app reached no shell prompt within 90 s" % name):
                continue
            time.sleep(3)
            ctx.be.key("Escape")
            ctx.shot("cb_settings_%s_started" % name)
            U.exact_burst(ctx, r, "cb_settings_%s" % name, 4)
            probs = startup_log_problems(open(ctx.log_path, errors="replace").read() if ctx.log_path and os.path.exists(ctx.log_path) else "")
            r.check(not probs, "%s settings file: the app log shows a failed start: %s" % (name, probs[:2]))
            r.check(ctx.app.alive(), "app died on the %s settings file" % name)
            now = open(p, "rb").read().decode("utf-8", "replace")
            if name == "legacy":
                r.metric("legacy_entries_kept", sum(1 for m in markers if m in now))
            else:
                r.metric("truncated_file_unchanged", now == body)
    finally:
        open(p, "wb").write(orig)
        ctx.snapshot_log()
        if not _relaunch_ok(ctx, r, "restored"):
            r.check(False, "the app did not come back on the original settings file")
        else:
            time.sleep(2)
            ctx.be.key("Escape")
            U.exact_burst(ctx, r, "cb_settings_restored", 3)


# ---------------------------------------------------------------- #1546 state across relaunches (the database migration path every upgrade runs)
@case("cb_state_relaunch", modes=("terminal",), order=89, gui=True)
def c_state_relaunch(ctx, r):
    """#1546: three relaunches on the SAME state directories (the SQLite database and its migrations run on every start; an upgrade is the same code path
    with an older schema, which needs a fixture of an old build's data and is a recorded gap, docs/testing.md 2.14). Each launch must reach a prompt,
    type exactly, and log no panic / migration / database ERROR. A line typed before the first quit must be in the shell's history file afterwards is
    NOT assumed (shell-dependent); the app's own data files must still exist and not shrink."""
    if ctx.app is None or not hasattr(ctx.app, "stop"):
        r.skip("sim backend: no app process to relaunch")
    home = ctx.prof.home

    def sizes():
        out = {}
        for sub in (".local/share", ".local/state", "Library/Application Support"):
            base = os.path.join(home, sub)
            for root, _d, files in os.walk(base):
                for f in files:
                    if f.endswith((".sqlite", ".db", ".sqlite3")):
                        fp = os.path.join(root, f)
                        try:
                            out[fp] = os.path.getsize(fp)
                        except OSError:
                            pass
        return out
    first = sizes()
    r.metric("db_files", len(first))
    for i in range(3):
        ctx.snapshot_log()
        if not r.check(_relaunch_ok(ctx, r, "relaunch%d" % (i + 1)), "relaunch %d: no shell prompt within 90 s on the existing state" % (i + 1)):
            return
        time.sleep(2)
        ctx.be.key("Escape")
        U.exact_burst(ctx, r, "cb_state_%d" % (i + 1), 4)
        probs = startup_log_problems(open(ctx.log_path, errors="replace").read() if ctx.log_path and os.path.exists(ctx.log_path) else "")
        r.check(not probs, "relaunch %d: the app log shows a failed start / migration: %s" % (i + 1, probs[:2]))
    after = sizes()
    gone = [os.path.basename(k) for k in first if k not in after]
    shrunk = [os.path.basename(k) for k in first if k in after and after[k] < first[k] // 2 and first[k] > 65536]
    r.check(not gone, "database file(s) disappeared across relaunches: %s" % gone)
    r.check(not shrunk, "database file(s) shrank to under half across relaunches: %s" % shrunk)


# ---------------------------------------------------------------- #1546 upgrade over a previous build's data
STATE_SUBDIRS = (".local/share", ".local/state", "Library/Application Support")      # where the app keeps warp.sqlite and its state (Linux, macOS); Windows: the known folders


def pack_state(home, out):
    """tar.gz of the previous build's state directories under `home` (home-relative names), the fixture of option (a). -> number of files packed"""
    n = 0
    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
    with tarfile.open(out, "w:gz") as t:
        for sub in STATE_SUBDIRS:
            base = os.path.join(home, sub)
            for root, _d, files in os.walk(base):
                for f in files:
                    fp = os.path.join(root, f)
                    if os.path.islink(fp):
                        continue
                    t.add(fp, arcname=os.path.relpath(fp, home))
                    n += 1
    return n


def unpack_state(tar_path, home):
    """extract a fixture made by pack_state into `home`; refuses any member that is not a plain file/dir under one of STATE_SUBDIRS (no absolute path, no
    `..`, no link): the archive is data, not trusted. -> number of files extracted"""
    n = 0
    with tarfile.open(tar_path, "r:*") as t:
        for m in t.getmembers():
            name = m.name.replace("\\", "/")
            ok = (m.isfile() or m.isdir()) and not name.startswith("/") and ".." not in name.split("/") and any(name == sub or name.startswith(sub + "/") for sub in STATE_SUBDIRS)
            if not ok:
                raise ValueError("fixture member %r is outside the state directories or not a plain file" % m.name)
            dest = os.path.join(home, *name.split("/"))
            if m.isdir():
                os.makedirs(dest, exist_ok=True)
                continue
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            with t.extractfile(m) as src, open(dest, "wb") as dst:
                dst.write(src.read())
            n += 1
    return n


def db_migrations(path):
    """number of applied schema migrations in a SQLite file (diesel's `__diesel_schema_migrations`), opened read-only; None when it cannot be read"""
    try:
        con = sqlite3.connect("file:%s?mode=ro" % path.replace("\\", "/"), uri=True, timeout=2)
        try:
            return con.execute("select count(*) from __diesel_schema_migrations").fetchone()[0]
        finally:
            con.close()
    except sqlite3.Error:
        return None


def state_files(ctx):
    """{path: size} of every SQLite file in the app's state directories (home-relative dirs on Linux/macOS, the known folders on Windows)"""
    out = {}
    roots = R.data_roots(ctx) if ctx.win else [os.path.join(ctx.prof.home, sub) for sub in STATE_SUBDIRS]
    for base in roots:
        for root, _d, files in os.walk(base):
            for f in files:
                if f.endswith((".sqlite", ".db", ".sqlite3")):
                    fp = os.path.join(root, f)
                    try:
                        out[fp] = os.path.getsize(fp)
                    except OSError:
                        pass
    return out


def old_app_like(app, binary, bundle=None):
    """an App handle of the same kind as `app` that launches the PREVIOUS build on the same profile (same run dir, home, ctl)"""
    import app as appmod
    if isinstance(app, appmod.WindowsApp):
        return appmod.WindowsApp(app.run, app.home, binary, app.pane, app.ctl)
    if isinstance(app, appmod.MacApp):
        return appmod.MacApp(app.run, app.home, bundle, binary)
    return appmod.App(app.run, app.home, binary, app.display)


@case("upgrade_over_previous", modes=("terminal",), order=90, gui=True)
def c_upgrade_over_previous(ctx, r):
    """#1546 option (a): the NEW build starts on the data directory of the PREVIOUS release. The old data comes from one of (checked in this order):
    APPCI_PREV_FIXTURE=<state tarball> (made by an earlier run of this case, kept as a CI artifact), or APPCI_PREV_BIN=<previous release binary> (Windows: its
    exe; macOS also APPCI_PREV_BUNDLE=<its .app>), which is launched on the scratch profile, used (three exact lines), stopped, and its state packed to
    <run>/fixtures/prev-state.tar.gz (not on Windows: the state is in the user's known folders). Then the new build is launched: it must reach a prompt, take
    exact typing, log no panic / migration / database ERROR, keep every database file (not moved aside as warp.sqlite.corrupt-*, not shrunk to under half) and
    not have FEWER applied migrations than the old build left (the count is a metric). Where CI gets the previous release's artifact is the owner's
    decision: without either variable the case SKIPS with that reason."""
    fixture, prev_bin, prev_bundle = (os.environ.get(k, "") for k in ("APPCI_PREV_FIXTURE", "APPCI_PREV_BIN", "APPCI_PREV_BUNDLE"))
    if ctx.app is None or not hasattr(ctx.app, "stop"):
        r.skip("sim backend: no app process")
    if not (fixture or prev_bin):
        r.skip("no previous-release data: set APPCI_PREV_FIXTURE (state tarball) or APPCI_PREV_BIN (previous release binary; macOS also APPCI_PREV_BUNDLE) -- where CI gets the previous release's artifact is the owner's decision (#1546)")
    if fixture and not os.path.isfile(fixture):
        r.skip("APPCI_PREV_FIXTURE=%s does not exist" % fixture)
    if not fixture and not os.path.isfile(prev_bin):
        r.skip("APPCI_PREV_BIN=%s does not exist" % prev_bin)
    if not fixture and ctx.be.name == "mac" and not os.path.isdir(prev_bundle):
        r.skip("macOS needs APPCI_PREV_BUNDLE=<the previous release's .app> next to APPCI_PREV_BIN")
    new_app = ctx.app
    mig_old = None
    try:
        ctx.snapshot_log()
        new_app.stop()
        if ctx.win:
            new_app.prepare()                                  # the known-folder data of the new build's earlier cases goes; the harness backs both up (app_test.ps1)
        else:
            import shutil
            for sub in STATE_SUBDIRS:
                shutil.rmtree(os.path.join(ctx.prof.home, sub), ignore_errors=True)
        if fixture:
            r.metric("fixture_files", unpack_state(fixture, ctx.prof.home))
        else:
            ctx.app = old_app_like(new_app, prev_bin, prev_bundle)
            _n, up = R.start_app(ctx, r, "uo_old", restored_min=1)
            if not up:
                return
            ctx.be.key("Escape")
            U.exact_burst(ctx, r, "uo_old", 3)
            time.sleep(4.0)                                    # the database writer and the session writer run at least once
            ctx.app.stop()
            ctx.app = new_app
            if not ctx.win:
                out = os.path.join(ctx.run, "fixtures", "prev-state.tar.gz")
                r.metric("fixture_packed_files", pack_state(ctx.prof.home, out))
                r.note("the previous build's state is packed in %s (keep it as a CI artifact for APPCI_PREV_FIXTURE)" % out)
        before = state_files(ctx)
        mig_old = {k: db_migrations(k) for k in before}
        r.metric("prev_db_files", {os.path.basename(k): v for k, v in before.items()})
        r.check(bool(before), "the previous build left no SQLite file in %s: nothing to upgrade" % ("the known folders" if ctx.win else "the state directories"))
        n_new, up = R.start_app(ctx, r, "uo_new", restored_min=1)
        r.check(up, "the new build did not reach a shell on the previous build's data")
        if not up:
            return
        time.sleep(2)
        ctx.be.key("Escape")
        ctx.shot("uo_new_started")
        U.exact_burst(ctx, r, "uo_new", 4)
        probs = startup_log_problems(open(ctx.log_path, errors="replace").read() if ctx.log_path and os.path.exists(ctx.log_path) else "")
        r.check(not probs, "the app log shows a failed start / migration on the previous build's data: %s" % probs[:2])
        after = state_files(ctx)
        gone = [os.path.basename(k) for k in before if k not in after]
        aside = [os.path.basename(k) for k in after if ".corrupt-" in k]
        shrunk = [os.path.basename(k) for k in before if k in after and after[k] < before[k] // 2 and before[k] > 65536]
        r.check(not gone, "database file(s) of the previous build vanished: %s" % gone)
        r.check(not aside, "the previous build's database was moved aside as corrupt: %s" % aside)
        r.check(not shrunk, "database file(s) shrank to under half: %s" % shrunk)
        mig_new = {k: db_migrations(k) for k in after}
        r.metric("migrations_before_after", {os.path.basename(k): [mig_old.get(k), mig_new.get(k)] for k in after})
        worse = [os.path.basename(k) for k in after if mig_old.get(k) is not None and mig_new.get(k) is not None and mig_new[k] < mig_old[k]]
        r.check(not worse, "fewer applied migrations after the new build ran than the old build left: %s" % worse)
        r.check(ctx.app.alive(), "the new build died on the previous build's data")
    finally:
        ctx.app = new_app
        if not new_app.alive():
            R.start_app(ctx, r, "uo_end", restored_min=1)


# ---------------------------------------------------------------- #1547 auto-update
@case("cb_no_autoupdate", modes=("terminal",), order=92, gui=True)
def c_no_autoupdate(ctx, r):
    """#1547: Phosphor deliberately ships WITHOUT the GUI auto-updater (DECLINED.md `FeatureFlag::Autoupdate`; script/check_no_autoupdate guards the
    build flags). The real build must therefore never talk about updating: no updater line in the app log of this whole run, and no
    `releases/latest` / `SHA256SUMS` fetch. Where the updater itself is exercised (SHA256SUMS parsing, mismatch/missing refusal) is
    app/src/autoupdate/gitlab_tests.rs; running it against a real signed release needs the release pipeline (a recorded gap, docs 2.14)."""
    if not ctx.log_path or not os.path.exists(ctx.log_path):
        r.skip("no app log at %s" % ctx.log_path)
    txt = open(ctx.log_path, errors="replace").read()
    r.metric("log_bytes", len(txt))
    hits = autoupdate_lines(txt)
    r.check(not hits, "the app log mentions the updater although this build ships without it: %s" % hits[:3])


# ---------------------------------------------------------------- #1548 UI-action budgets (screenshot-observed: loose bounds, trend metrics)
def _until_changed(ctx, before, tag, thresh, timeout):
    """seconds from now until a screenshot differs from `before` by >= thresh; None on timeout, -1 without Pillow"""
    t0 = time.time()
    n = 0
    while time.time() - t0 < timeout:
        n += 1
        p = ctx.shot("%s_%d" % (tag, n))
        d = U.img_diff(before, p) if p else None
        if d is None and p:
            return -1.0
        if d is not None and d >= thresh:
            return time.time() - t0
    return None


def ui_work_metric(ctx, r, off, label):
    """#1548: the app's own `hop=ui_item` lines (main-thread items >= 4 ms, the owner's UI-work budget) written since log offset `off`: metric
    `<label>_ui_work` {count, max_ms, kinds}; a note when an item reaches 16 ms (a dropped frame). Never a failure here (a new tab starts a shell and
    paints a fresh surface; software GL puts frames over 4 ms by itself): latency_key_log judges the steady-typing window."""
    if not getattr(ctx, "log_path", None):
        return
    time.sleep(0.3)
    items = C.ui_items(ctx.log_since(off))
    r.metric("%s_ui_work" % label, dict(count=len(items), max_ms=max([ms for _, ms, _ in items], default=0.0), kinds=sorted({k for _, _, k in items})))
    worst = max([ms for _, ms, _ in items], default=0.0)
    if worst >= 16:
        r.note("WARN UI work: %s had a main-thread item of %.1f ms (>= 16 ms drops a frame)" % (label, worst))


@case("cb_ui_action_latency", modes=("terminal",), order=93, gui=True)
def c_ui_action_latency(ctx, r):
    """#1548: new tab, tab switch and the Tab completion menu are observed from screenshots (each costs 50-300 ms of its own, so the numbers are an
    UPPER bound on the app's time, not a ≤4 ms proof: the app log cannot see UI work under 8 ms, see docs/testing.md 3.6). Bounds are generous first
    guesses (APPCI_UI_ACTION_MAX_S, default 1.5 s each); the numbers are metrics for trend. The key->paint 2 ms budget stays in latency_paint (macOS)."""
    limit = float(os.environ.get("APPCI_UI_ACTION_MAX_S", "1.5"))
    newtab = os.environ.get("APPCI_KEY_NEWTAB", "ctrl+shift+t")
    nexttab = "cmd+shift+]" if ctx.be.name == "mac" else "ctrl+Next"      # ActivateNextTab (app/src/util/bindings.rs)
    ctx.reset()
    time.sleep(0.5)
    before = ctx.shot("cb_ui_new_before")
    off_new = ctx.log_size()
    ctx.be.key(newtab)
    s = _until_changed(ctx, before, "cb_ui_new", 0.002, 6)
    ui_work_metric(ctx, r, off_new, "new_tab")
    r.metric("new_tab_s", s if s is not None else -2)
    if s == -1.0:
        r.note("no Pillow: UI action timing needs python3-pil; screenshots only")
        return
    r.check(s is not None and s <= limit, "new tab: first changed frame after %s s (bound %.1f s)" % (None if s is None else round(s, 2), limit))
    time.sleep(1.5)
    before = ctx.shot("cb_ui_switch_before")
    off_sw = ctx.log_size()
    ctx.be.key(nexttab)
    s2 = _until_changed(ctx, before, "cb_ui_switch", 0.0005, 6)
    ui_work_metric(ctx, r, off_sw, "tab_switch")
    r.metric("tab_switch_s", s2 if s2 is not None else -2)
    # the tab strip changes little when two identical shells swap; a missing change is a note, not a fail
    if s2 is None:
        r.note("tab switch: no changed frame in 6 s (two identical shell tabs look alike); see shots/cb_ui_switch_*.png")
    else:
        r.check(s2 <= limit, "tab switch: first changed frame after %.2f s (bound %.1f s)" % (s2, limit))
    # close the throw-away tab; the original keeps the exact-typing proof
    R.close_tab_best_effort(ctx, r)
    time.sleep(1.0)
    ctx.reset()
    U.exact_burst(ctx, r, "cb_ui_after", 4)
    r.check(ctx.app.alive(), "app died during the UI action timing")


# ---------------------------------------------------------------- #1544 code editor on macOS
@case("cb_editor_mac", modes=("terminal",), order=99, gui=True)
def c_editor_mac(ctx, r):
    """#1544: the code-editor edit/save/undo flow on macOS (editor_edit_save is Linux-only: it opens the file over the D-Bus URL forward). The file
    is handed to the running app with `open -a <bundle>` (LaunchServices), a token is typed at 10-25 ms/key, Cmd-S saves, the file on disk is
    byte-exact, Cmd-Z x N + Cmd-S restores the original. Nothing may reach the shell. macOS has no usable keyboard close for the pane (Cmd-Shift-W
    is refused by the frontmost-app guard), so the pane is left open: this case runs last and run.py relaunches the app if the pane is not the
    shell afterwards. Linux has editor_edit_save; Windows has no verified file hand-over."""
    if ctx.be.name != "mac":
        r.skip("Linux: editor_edit_save/editor_paste/editor_memory_7mb cover the editor; Windows: editor_edit_save and editor_paste hand the file over the named pipe (#1544)")
    d = os.path.join(ctx.W, "ed")
    os.makedirs(d, exist_ok=True)
    path = os.path.join(d, "mac_edit_%s.txt" % ctx.mode)
    open(path, "wb").write(R.EDIT_ORIG.encode())
    mark = ctx.cmd_mark()
    err = C.open_file_in_app(ctx, path)
    if not r.check(err is None, "could not hand the file to the running app: %s" % err):
        return
    time.sleep(2.5)
    ctx.shot("cb_editor_mac_open")
    t1 = "EDIT_%s_" % ctx.tok(6)
    ctx.be.key("cmd+up")
    ctx.be.type(t1, ctx.rng.randint(10, 25))
    time.sleep(0.4)
    ctx.be.key("cmd+s")
    ctx.until(lambda: R.disk(path) != R.EDIT_ORIG, 6)
    ctx.shot("cb_editor_mac_saved")
    r.check(open(path, "rb").read() == (t1 + R.EDIT_ORIG).encode(), "save: disk has %r, expected %r (the editor may not have had focus; see shots)" % (R.disk(path)[:80], (t1 + R.EDIT_ORIG)[:80]))
    ctx.be.key("cmd+z", repeat=len(t1) + 8, delay_ms=40)
    time.sleep(0.4)
    ctx.be.key("cmd+s")
    ctx.until(lambda: R.disk(path) == R.EDIT_ORIG, 6)
    r.check(R.disk(path) == R.EDIT_ORIG, "undo + save: disk has %r, expected the original" % R.disk(path)[:80])
    r.check(not ctx.cmds_since(mark), "keys meant for the editor reached the shell: %r" % ctx.cmds_since(mark)[:3])
    r.check(ctx.app.alive(), "app died in the code editor")
