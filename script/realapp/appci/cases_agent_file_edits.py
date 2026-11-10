# appci/cases_agent_file_edits.py (#1835): the agent's file edits, end to end. The scripted fake model (fake_model.py, APPCI_FAKE_MODEL=1) proposes a multi-hunk
# apply_file_diffs ("Please edit exactly this file: <path> with these edits: <search>=><replace> ;; ..."); the case answers the diff card with the owner's keys
# (Enter focuses the card, Enter accepts, Ctrl-C rejects; code_diff_view.rs ACCEPT_KEY / "ctrl-c") and the FILE ON DISK is the oracle, byte for byte.
#   agent_edit_accept          a three-hunk diff, accepted: the file is exactly the expected bytes (and stays so: no second write)
#   agent_edit_reject          the same diff, rejected: the file is byte-identical to before (and the model still gets its answer)
#   agent_edit_conflict        the file is changed on disk AFTER the diff was proposed and BEFORE it is accepted: the save is refused, the user's change is intact,
#                              the conflict is reported (app log: "Failed to save file for accepted AgentMode diffs"; the toast text is warp_files "changed on disk")
#   agent_edit_endings         CRLF file / LF file with no trailing newline / CRLF with none / a file that MIXES CRLF and LF lines, with and without a final newline
#                              (#1956): after an accepted edit (including an edit of the LAST line) every line ending and the missing-or-present final newline are
#                              exactly what they were (#1925)
#   lab_fam_agent_edit_conflict   the accept + conflict pair on a REMOTE host (lab, arms nohelper = the shell backend over the ControlMaster, the primary design; upload =
#                              the helper). The accept re-reads the file, checks it against the proposal, and writes with a SHA-256 of the bytes it read
#                              (#1837): the shell script (nohelper) or the helper (`WriteFile.expect`, capability file_write_expect) compares atomically with the write
#                              and refuses with "the file changed on the host after it was read, so it was not overwritten" (matched by SAVE_FAILED); the host's
#                              file keeps the user's change
#   lab_fam_agent_delete_conflict  the agent proposes DELETING a file and the user edits it on the host before the accept: the file is kept, the conflict reported (#1837 follow-up;
#                              the delete carries the SHA-256 of what the accept read, `DeleteFile.expect` / capability file_delete_expect)
#   lab_fam_agent_edit_endings    CRLF and no-final-newline preserved over the same two arms
#   agent_edit_touch_only      mtime moved, bytes identical, then accept: succeeds, no false conflict (local: content compare in warp_files; remote leg in the lab pair)
# Remote writes must log hop=backend_route family=file_write (nohelper -> shell, upload -> helper); no line fails the lab cases (crates/remote_server router.rs).
# Partial accept: the diff card has NO per-hunk accept (Up/Down only navigate hunks, code_diff_view.rs NavigateToDiffHunk): accept is all hunks or none. Every case
# says so in a note rather than inventing a pass.
# Platforms: Linux and Mac (the prompt carries the file path, the file is read back by the harness). Not Windows (winmap NOT_WINDOWS): the path form typed to the model
# differs there and the CI model pass is Git Bash only. The lab cases run wherever the lab does (lab_* are never Windows-classified).
# Oracles are the file bytes and the app log; a case that cannot prove its precondition (no action card, first Enter already accepted) is UNVERIFIABLE, never a pass.
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cases import case  # noqa: E402
import cases_caseagent as AG  # noqa: E402
import cases_lab as CL  # noqa: E402
import cases_replay as CRP  # noqa: E402

EDIT_SEP = " ;; "
WORDS = ("alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa")
SAVE_FAILED = re.compile(r"Failed to save file for accepted AgentMode diffs|changed on (?:disk|the (?:remote )?host)|was not overwritten|was not deleted")
STREAM = "[byop] opening stream"


# ---------------------------------------------------------------- pure parts (selftest.py exercises these)
def make_file(tok, eol=b"\n", final_newline=True):
    """ten lines `L01 alpha <tok>` ... `L10 kappa <tok>` joined by `eol`; the last one ends with `eol` only when final_newline. `eol` is one line ending, or a
    tuple of them taken in turn line by line (a file with MIXED endings, #1956: line i ends with eol[i % len(eol)])"""
    lines = [("L%02d %s %s" % (i + 1, w, tok)).encode() for i, w in enumerate(WORDS)]
    eols = eol if isinstance(eol, tuple) else (eol,)
    out = b""
    for i, line in enumerate(lines):
        out += line + (eols[i % len(eols)] if final_newline or i < len(lines) - 1 else b"")
    return out


def three_edits(tok):
    """the multi-hunk diff: lines 2, 5 and 10 (the LAST line, which is the one a missing final newline sits on). Each search is a WHOLE line (without its line ending):
    the app's matcher works on whole lines (diff_validation fuzzy_match_file_diffs), and a search that is only a prefix of a line can fail to match (round 7: 1 unmatched,
    the action failed at once and no diff card was ever accepted), so the file's line ending is never part of a search string and no search is a partial line"""
    return [("L02 beta %s" % tok, "L02 BETA-EDITED %s" % tok), ("L05 epsilon %s" % tok, "L05 EPSILON-EDITED %s" % tok), ("L10 kappa %s" % tok, "L10 KAPPA-EDITED %s" % tok)]


def prompt_for(path, edits):
    return "Please edit exactly this file: %s with these edits: %s" % (path, EDIT_SEP.join("%s=>%s" % (a, b) for a, b in edits))


def apply_edits(content, edits):
    """what a correct accept writes: each search replaced once, in order, as bytes (no line-ending translation). ValueError when a search is absent or not unique"""
    out = content
    for old, new in edits:
        o, n = old.encode(), new.encode()
        if out.count(o) != 1:
            raise ValueError("search %r occurs %d times" % (old, out.count(o)))
        out = out.replace(o, n)
    return out


def ending_profile(b):
    """(crlf lines, bare-LF lines, bare-CR, ends with a newline)"""
    crlf = b.count(b"\r\n")
    return (crlf, b.count(b"\n") - crlf, b.count(b"\r") - crlf, b.endswith(b"\n"))


def judge_bytes(actual, expected, what):
    """[] when actual == expected, else one message that names the first differing byte and the line-ending profiles"""
    if actual == expected:
        return []
    n = next((i for i, (x, y) in enumerate(zip(actual, expected)) if x != y), min(len(actual), len(expected)))
    return ["%s: %d bytes on disk, %d expected; first difference at byte %d (%r vs %r); line endings (crlf, bare lf, bare cr, final newline) %r vs expected %r" % (
        what, len(actual), len(expected), n, actual[n:n + 24], expected[n:n + 24], ending_profile(actual), ending_profile(expected))]


def judge_conflict(actual, before, user, agent_expected):
    """the file after a save that must be refused: exactly the user's bytes. Names which way it went wrong"""
    if actual == user:
        return []
    if actual == agent_expected:
        return ["the agent's edit overwrote the file although it changed on disk after the proposal (the user's change is gone)"]
    if actual == before:
        return ["the file was put back to its pre-proposal bytes: the user's change is gone"]
    return judge_bytes(actual, user, "the refused save altered the file (neither the user's bytes nor the agent's)")


# ---------------------------------------------------------------- the two hosts the same scenario runs on
class LocalHost:
    """files under the app profile's work dir, on this machine"""

    def __init__(self, ctx):
        self.ctx = ctx
        self.made = []

    def path(self, name):
        return os.path.join(self.ctx.W, name)

    def write(self, name, data):
        os.makedirs(self.ctx.W, exist_ok=True)
        with open(self.path(name), "wb") as f:
            f.write(data)
        if name not in self.made:
            self.made.append(name)

    def touch(self, name):
        """move the mtime forward, bytes untouched"""
        t = time.time() + 10
        os.utime(self.path(name), (t, t))
        return True

    def read(self, name):
        try:
            with open(self.path(name), "rb") as f:
                return f.read()
        except FileNotFoundError:
            return None

    def cleanup(self):
        for n in self.made:
            try:
                os.remove(self.path(n))
            except OSError:
                pass


class LabHost:
    """files in /work/<user> on the lab host; written through the lab's tree API (shell-independent: the login shell may be nu or pwsh), read back raw"""

    def __init__(self, ctx, arm):
        self.ctx, self.arm, self.made = ctx, arm, []

    def path(self, name):
        return CL.work(self.arm, name)

    def write(self, name, data):
        self.ctx.lab.tree(self.arm["user"], {name: data})
        if name not in self.made:
            self.made.append(name)

    def touch(self, name):
        import shlex
        time.sleep(1.1)           # whole-second mtimes: make the new one differ
        res = self.ctx.lab.ssh(self.arm["user"], "sh -c %s" % shlex.quote("touch %s" % shlex.quote(self.path(name))))
        return getattr(res, "returncode", 1) == 0

    def read(self, name):
        try:
            return self.ctx.lab.file(self.path(name))
        except Exception:
            return None

    def cleanup(self):
        for n in self.made:
            self.ctx.lab.rm_file(self.path(n))


# ---------------------------------------------------------------- the scenario engine
def streams(ctx, off):
    return AG.count_in_log(ctx, off, STREAM)


def delete_prompt_for(path):
    return "Please delete exactly this file and nothing else: %s" % path


def propose(ctx, r, host, name, content, edits):
    """write the file, ask the agent for the edit (edits=None: for the file's DELETION), wait for the diff card; returns (off, streams_at_card) or None (UNVERIFIABLE already recorded)"""
    host.write(name, content)
    off = ctx.log_size()
    cards0 = AG.count_in_log(ctx, off, "queue_actions")
    AG.type_fast(ctx, prompt_for(host.path(name), edits) if edits is not None else delete_prompt_for(host.path(name)))
    if not ctx.until(lambda: AG.count_in_log(ctx, off, "queue_actions") > cards0, 60, step=0.25):
        r.unverifiable("no action card for the proposed edit within 60 s (no '[byop-diag] queue_actions' in the app log): the fake model's edit never reached the diff view, nothing to accept or reject")
        return None
    time.sleep(2.5 + (ctx.arm["lat_ms"] / 1000.0 * 4 if getattr(ctx, "arm", None) else 0))       # the diff view has loaded the file it will compare against at save
    ctx.shot("edit_card_%s" % name)
    return off, streams(ctx, off)


def answer(ctx, r, host, name, off, n_streams, action, content):
    """accept (Enter focuses, Enter accepts) or reject (Enter focuses, Ctrl-C rejects). Returns False when the precondition failed (UNVERIFIABLE recorded)"""
    ctx.be.key("Return")
    time.sleep(1.0)
    if action == "reject":
        if host.read(name) != content:
            r.unverifiable("reject: the first Enter already applied the edit (the card was focused), so the reject path could not be reached")
            return False
        ctx.be.key("ctrl+c")
    else:
        ctx.be.key("Return")
    if not ctx.until(lambda: streams(ctx, off) > n_streams, 40, step=0.25):
        r.note("%s: no follow-up model request within 40 s (the tool result may not have reached the model)" % action)
    time.sleep(2.0)
    ctx.shot("edit_%s_%s" % (action, name))
    return True


def stable(host, name, settle=2.0):
    """the bytes now and again after a pause: a second write (a duplicate apply) would show as a change"""
    a = host.read(name)
    time.sleep(settle)
    b = host.read(name)
    return a, b


def partial_note(r):
    r.note("partial accept: the diff card has no per-hunk accept (Up/Down only move between hunks, code_diff_view.rs); accept applies every hunk, reject none: not testable, not claimed")


def run_accept(ctx, r, host, label, eol=b"\n", final_newline=True):
    tok = CL.unique(ctx, 6)
    name = "ae_%s_%s.txt" % (label, tok)
    content, edits = make_file(tok, eol, final_newline), three_edits(tok)
    expected = apply_edits(content, edits)
    got = propose(ctx, r, host, name, content, edits)
    if got is None:
        return
    off, n = got
    r.check(host.read(name) == content, "%s: the file changed before the diff was accepted" % label)
    if not answer(ctx, r, host, name, off, n, "accept", content):
        return
    ctx.until(lambda: host.read(name) == expected, 30, step=0.5)
    a, b = stable(host, name)
    for msg in judge_bytes(a, expected, "%s: the accepted multi-hunk edit" % label):
        r.check(False, msg)
    r.check(a == b, "%s: the file changed again after the accept settled (a duplicate write)" % label)


def run_conflict(ctx, r, host, label):
    tok = CL.unique(ctx, 6)
    name = "ac_%s_%s.txt" % (label, tok)
    content, edits = make_file(tok), three_edits(tok)
    agent_expected = apply_edits(content, edits)
    got = propose(ctx, r, host, name, content, edits)
    if got is None:
        return
    off, n = got
    user = content + ("USER_EDIT_%s\n" % tok).encode()
    host.write(name, user)                           # the user (or a build) changes the file after the proposal, before the accept
    if host.read(name) != user:
        r.unverifiable("%s: could not change the file between proposal and accept" % label)
        return
    if not answer(ctx, r, host, name, off, n, "accept", user):
        return
    ctx.until(lambda: host.read(name) != user, 8, step=0.5)       # an unwanted write would land here; the wait is bounded, a refusal never changes the file
    a, b = stable(host, name)
    for msg in judge_conflict(a, content, user, agent_expected):
        r.check(False, "%s: %s" % (label, msg))
    r.check(a == b, "%s: the file changed again after the refused accept" % label)
    r.check(bool(SAVE_FAILED.search(ctx.log_since(off))), "%s: the conflict was not reported (no 'Failed to save file for accepted AgentMode diffs' / 'changed on disk' line in the app log)" % label)


def run_delete_conflict(ctx, r, host, label):
    """#1837 follow-up: the agent proposes DELETING a file and the user edits it on the host before the accept: the file must still be there with exactly the user's bytes and the
    refusal must be reported. (The host-side check closes the window between the app's read and the delete; the contract tests prove that; this proves the whole accept path.)"""
    tok = CL.unique(ctx, 6)
    name = "ad_%s_%s.txt" % (label, tok)
    content = make_file(tok)
    got = propose(ctx, r, host, name, content, None)
    if got is None:
        return
    off, n = got
    user = content + ("USER_EDIT_%s\n" % tok).encode()
    host.write(name, user)
    if host.read(name) != user:
        r.unverifiable("%s: could not change the file between the deletion proposal and the accept" % label)
        return
    if not answer(ctx, r, host, name, off, n, "accept", user):
        return
    ctx.until(lambda: host.read(name) != user, 8, step=0.5)       # an unwanted delete would land here; a refusal never changes the file
    a, b = stable(host, name)
    if a is None:
        r.check(False, "%s: the file was deleted although it changed on the host after the deletion was proposed (the user's edit is gone)" % label)
    else:
        for msg in judge_bytes(a, user, "%s: the refused delete altered the file" % label):
            r.check(False, msg)
    r.check(a == b, "%s: the file changed again after the refused delete" % label)
    r.check(bool(SAVE_FAILED.search(ctx.log_since(off))), "%s: the conflict was not reported (no 'Failed to save file for accepted AgentMode diffs' / 'changed on the remote host' / 'was not deleted' line in the app log)" % label)


def run_touch_only(ctx, r, host, label):
    """the file's mtime moves after the proposal but its BYTES are identical (a `touch`, a backup tool, a checkout of the same content). The guard compares content
    (warp_files::save_if_unchanged), so the accept must succeed: no false conflict, the expected bytes, no 'changed on disk'. Remote: the nohelper size+mtime Expect
    may report a false conflict here; that expectation belongs to branch fix/1837-remote-edit-conflict (until it lands this case FAILS on that arm, which is the defect)."""
    tok = CL.unique(ctx, 6)
    name = "at_%s_%s.txt" % (label, tok)
    content, edits = make_file(tok), three_edits(tok)
    expected = apply_edits(content, edits)
    got = propose(ctx, r, host, name, content, edits)
    if got is None:
        return
    off, n = got
    if not host.touch(name) or host.read(name) != content:
        r.unverifiable("%s: could not move the mtime with the bytes unchanged" % label)
        return
    if not answer(ctx, r, host, name, off, n, "accept", content):
        return
    ctx.until(lambda: host.read(name) == expected, 30, step=0.5)
    a, b = stable(host, name)
    for msg in judge_bytes(a, expected, "%s: an accept after a touch-only change (bytes identical)" % label):
        r.check(False, msg)
    r.check(a == b, "%s: the file changed again after the accept settled" % label)
    r.check(not SAVE_FAILED.search(ctx.log_since(off)), "%s: a touch-only change (same bytes) was reported as a conflict (false conflict)" % label)


def check_write_route(ctx, r, arm, off, label):
    """the app must log which arm took the remote write (hop=backend_route family=file_write): nohelper -> shell, upload -> helper. No line = the edit did not go through the router"""
    want = "shell" if arm["helper"] == "nohelper" else "helper"
    routes = re.findall(r"hop=backend_route family=file_write backend=(helper|shell|unavailable)", ctx.log_since(off))
    r.check(bool(routes), "%s: no hop=backend_route family=file_write line: the edit was not written through the backend router" % label)
    r.check(all(b == want for b in routes), "%s: file_write routed to %r, expected %s" % (label, sorted(set(routes)), want))


# ---------------------------------------------------------------- local cases (agent mode only)
def local_ready(ctx, r):
    CRP.need_model(ctx, r)
    AG.fresh_conversation(ctx, r)


@case("agent_edit_accept", modes=("agent",), order=52, gui=True)
def c_agent_edit_accept(ctx, r):
    """a three-hunk diff accepted with Enter, Enter: the file is exactly the expected bytes, written once"""
    local_ready(ctx, r)
    host = LocalHost(ctx)
    try:
        run_accept(ctx, r, host, "lf")
        partial_note(r)
    finally:
        host.cleanup()
        ctx.be.key("ctrl+c")


@case("agent_edit_reject", modes=("agent",), order=52, gui=True)
def c_agent_edit_reject(ctx, r):
    """the same diff rejected with Enter (focus), Ctrl-C: the file is byte-identical, and a following accept of a fresh proposal still works (the reject left no state)"""
    local_ready(ctx, r)
    host = LocalHost(ctx)
    try:
        tok = CL.unique(ctx, 6)
        name = "ar_%s.txt" % tok
        content, edits = make_file(tok), three_edits(tok)
        got = propose(ctx, r, host, name, content, edits)
        if got is None:
            return
        off, n = got
        if answer(ctx, r, host, name, off, n, "reject", content):
            a, b = stable(host, name, 3.0)
            for msg in judge_bytes(a, content, "the rejected edit changed the file"):
                r.check(False, msg)
            r.check(a == b, "the file changed after the reject")
        run_accept(ctx, r, host, "after_reject")
        partial_note(r)
    finally:
        host.cleanup()
        ctx.be.key("ctrl+c")


@case("agent_edit_conflict", modes=("agent",), order=52, gui=True)
def c_agent_edit_conflict(ctx, r):
    """the file changes on disk between the proposal and the accept: the save is refused, the user's bytes survive, the conflict is reported"""
    local_ready(ctx, r)
    host = LocalHost(ctx)
    try:
        run_conflict(ctx, r, host, "local")
    finally:
        host.cleanup()
        ctx.be.key("ctrl+c")


@case("agent_edit_touch_only", modes=("agent",), order=52, gui=True)
def c_agent_edit_touch_only(ctx, r):
    """the mtime moves between proposal and accept, the bytes do not: the accept succeeds (content compare, no false conflict)"""
    local_ready(ctx, r)
    host = LocalHost(ctx)
    try:
        run_touch_only(ctx, r, host, "local")
    finally:
        host.cleanup()
        ctx.be.key("ctrl+c")


# MIXED (#1956): lines 2, 5 and 8 end in LF, the other seven in CRLF. The edit touches two LF lines (2, 5) and the CRLF last line; every other line must keep
# its own ending (the editor buffer can only write one ending, so an accept that wrote the buffer text turned the three LF lines into CRLF)
MIXED = (b"\r\n", b"\n", b"\r\n")
ENDINGS = (("crlf", b"\r\n", True), ("lf_no_eol", b"\n", False), ("crlf_no_eol", b"\r\n", False), ("mixed", MIXED, True), ("mixed_no_eol", MIXED, False))


@case("agent_edit_endings", modes=("agent",), order=52, gui=True)
def c_agent_edit_endings(ctx, r):
    """CRLF files, files without a final newline and files with mixed endings keep every ending through an accepted edit that touches the last line"""
    local_ready(ctx, r)
    host = LocalHost(ctx)
    try:
        for label, eol, fin in ENDINGS:
            run_accept(ctx, r, host, label, eol, fin)
    finally:
        host.cleanup()
        ctx.be.key("ctrl+c")


# ---------------------------------------------------------------- lab cases (remote host; nohelper = shell backend, the primary design; upload = the helper)
def lab_ready(ctx, r, arm):
    CRP.need_model(ctx, r)
    if CL.connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return False
    time.sleep(1.0 + 2 * CL.rtt_s(arm))
    return True


@CL.lab_case("fam_agent_edit_conflict", shells=("bash", "dash", "nu", "pwsh"), lat=("lan",), helper=("nohelper", "upload"), modes=("agent",))
def c_lab_agent_edit_conflict(ctx, r, arm):
    """on the remote host: a multi-hunk diff accepted writes exactly the expected bytes; a second proposal whose file changes on the host before the accept is refused
    and the host's file keeps the change. Without the helper the guard is the size+mtime Expect on the shell write; with it, the helper's own protocol. A clobber on
    either arm is a failure, not a note."""
    if not lab_ready(ctx, r, arm):
        return
    host = LabHost(ctx, arm)
    off = ctx.log_size()
    try:
        run_accept(ctx, r, host, "remote")
        run_conflict(ctx, r, host, "remote")
        check_write_route(ctx, r, arm, off, "lab_fam_agent_edit_conflict")
        partial_note(r)
    finally:
        host.cleanup()
        CL.finish_remote(ctx, r)


@CL.lab_case("fam_agent_delete_conflict", shells=("bash", "dash", "pwsh"), lat=("lan",), helper=("nohelper", "upload"), modes=("agent",))
def c_lab_agent_delete_conflict(ctx, r, arm):
    """on the remote host: the agent proposes deleting a file, the user edits it on the host before the accept: the file is kept with the user's bytes and the conflict is reported.
    The delete carries the SHA-256 of what the accept read to the shell script (nohelper) or to the helper (`DeleteFile.expect`, capability file_delete_expect)"""
    if not lab_ready(ctx, r, arm):
        return
    host = LabHost(ctx, arm)
    off = ctx.log_size()
    try:
        run_delete_conflict(ctx, r, host, "remote")
        check_write_route(ctx, r, arm, off, "lab_fam_agent_delete_conflict")
    finally:
        host.cleanup()
        CL.finish_remote(ctx, r)


@CL.lab_case("fam_agent_edit_endings", shells=("bash", "pwsh"), lat=("lan",), helper=("nohelper", "upload"), modes=("agent",))
def c_lab_agent_edit_endings(ctx, r, arm):
    """CRLF, no-final-newline, both, and mixed endings, edited on the remote host: the bytes written back keep their endings"""
    if not lab_ready(ctx, r, arm):
        return
    host = LabHost(ctx, arm)
    off = ctx.log_size()
    try:
        for label, eol, fin in ENDINGS:
            run_accept(ctx, r, host, label, eol, fin)
        check_write_route(ctx, r, arm, off, "lab_fam_agent_edit_endings")
    finally:
        host.cleanup()
        CL.finish_remote(ctx, r)


@CL.lab_case("fam_agent_edit_touch_only", shells=("bash",), lat=("lan",), helper=("nohelper", "upload"), modes=("agent",))
def c_lab_agent_edit_touch_only(ctx, r, arm):
    """on the remote host: mtime moved, bytes identical, then accept: no false conflict. The nohelper arm's size+mtime Expect is expected to be fixed by fix/1837-remote-edit-conflict;
    until that lands a failure here is that defect, not a flaky case"""
    if not lab_ready(ctx, r, arm):
        return
    host = LabHost(ctx, arm)
    off = ctx.log_size()
    try:
        run_touch_only(ctx, r, host, "remote")
        check_write_route(ctx, r, arm, off, "lab_fam_agent_edit_touch_only")
    finally:
        host.cleanup()
        CL.finish_remote(ctx, r)
