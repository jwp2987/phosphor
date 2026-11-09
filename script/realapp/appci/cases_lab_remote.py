# appci/cases_lab_remote.py: lab cases for the "same features without a helper" parity work (Wave H parity, docs/design/remote-helper-parity.md).
# One case per family, each run as `nohelper` (the shell backend over the out-of-band channel) and `upload` (the helper the app uploads), asserting the SAME
# thing from the user's side. Registered with cases_lab.lab_case, so they are lab_<case>.<user>.<lan|150>.<arm> and run only when --cases names them.
#
# [hook] lines are the app's own log lines: `hop=file_tree event=snapshot|update source=helper|shell removed=N added=M added_hash=[..]` (manager.rs
# log_file_tree_event; counts and the FNV-1a hash of each added leaf name, never the names). Until the app logs them a check is UNVERIFIABLE, never a pass.
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402  (cases.py imports this module after cases_lab)
import cases_coverage_a as CA  # noqa: E402
import cases_lab as CL  # noqa: E402
import cases_lab_helper as CLH  # noqa: E402

# families this file covers (the coverage guard in cases_lab_helper counts them): the file tree, and the ListDirectory the link validation uses
CLH.declare_external("repo_metadata", helper_case="lab_file_tree_remote", nohelper_case="lab_file_tree_remote")
CLH.declare_external("fs_basic", helper_case="lab_file_tree_remote", nohelper_case="lab_file_tree_remote")

HOOK_FILE_TREE = re.compile(r"hop=file_tree event=(snapshot|update) source=(helper|shell) removed=(\d+) added=(\d+) added_hash=\[([0-9a-f,]*)\]")


def fnv1a32(name):
    """the hash manager.rs log_file_tree_event logs for a leaf name (32-bit FNV-1a over the UTF-8 bytes), 8 hex digits"""
    h = 2166136261
    for b in name.encode():
        h = ((h ^ b) * 16777619) & 0xFFFFFFFF
    return "%08x" % h


def tree_events(text):
    """[(event, source, removed, added, [hash...])] in log order"""
    return [(m.group(1), m.group(2), int(m.group(3)), int(m.group(4)), [h for h in m.group(5).split(",") if h]) for m in HOOK_FILE_TREE.finditer(text)]


def saw_added(text, name, event="update"):
    want = fnv1a32(name)
    return any(e == event and want in hashes for e, _s, _r, _a, hashes in tree_events(text))


@CL.lab_case("file_tree_remote", shells=("bash", "zsh", "fish", "nu"), lat=("lan", "150"), helper=("nohelper", "upload"), modes=("terminal",))
def c_file_tree_remote(ctx, r, arm):
    """The remote file tree, with and without the helper. In a git repository on the server the user `cd`s into a subdirectory: the app gets the repo's tree
    (a snapshot from the helper, or from `git ls-files` over the out-of-band channel). Then a file is created OUTSIDE the terminal (a separate ssh connection,
    the way an editor or a build would):
      upload   -- the helper's watcher pushes the update on its own, no prompt needed (live);
      nohelper -- nothing is seen while no prompt returns (no timer, no polling); the next prompt re-reads the tree and the new file arrives.
    The tree event is the app's own log line (hop=file_tree); the user-visible tree panel shares the same events."""
    lab, u, rt = ctx.lab, arm["user"], CL.rtt_s(arm)
    uid = CL.unique(ctx)
    fx = "tree_%s" % uid
    repo = CL.work(arm, fx)
    lab.tree(u, {"%s/src/lib.rs" % fx: "", "%s/a.txt" % fx: "", "%s/.gitignore" % fx: "*.log\n"}, git=True)
    off = ctx.log_size()
    if CL.connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    ctx.be.type("cd %s/src" % repo, CL.type_pace(ctx), enter=True)
    snap = ctx.until(lambda: any(e == "snapshot" for e, *_ in tree_events(ctx.log_since(off))), 20 + 6 * rt, step=0.1)
    if not snap:
        if arm["helper"] == "nohelper":
            r.unverifiable("[hook] no hop=file_tree snapshot in the app log: the shell-mode file tree needs the session's out-of-band channel (the ControlMaster adapter), which this build may not register yet")
        else:
            r.check(False, "upload arm: the helper's file-tree snapshot never reached the app (no hop=file_tree event=snapshot in the log)")
        ctx.reset()
        CL.finish_remote(ctx, r)
        return
    sources = {s for e, s, *_ in tree_events(ctx.log_since(off)) if e == "snapshot"}
    want_source = "helper" if arm["helper"] == "upload" else "shell"
    r.check(sources == {want_source}, "the snapshot came from %s, expected %s" % (sorted(sources), want_source))
    ctx.shot("lab_file_tree_snapshot_%s" % arm["helper"])

    # a file appears outside the terminal
    new = "outside_%s.txt" % uid
    mark = ctx.log_size()
    lab.ssh(u, "touch %s/%s" % (repo, new))
    if arm["helper"] == "upload":
        live = ctx.until(lambda: saw_added(ctx.log_since(mark), new), 10 + 4 * rt, step=0.1)
        r.check(bool(live), "the helper did not push the new file without a prompt (no hop=file_tree update naming it)")
    else:
        # no prompt has come back: the shell backend has nothing to react to, and a quiet period shows it neither polls nor guesses
        time.sleep(2.0 + 2 * rt)
        r.check(not saw_added(ctx.log_since(mark), new), "the file was reported before any prompt returned: the shell backend must refresh on prompt events only")
        ctx.be.type("true", CL.type_pace(ctx), enter=True)
        seen = ctx.until(lambda: saw_added(ctx.log_since(mark), new), 15 + 6 * rt, step=0.1)
        r.check(bool(seen), "the next prompt did not bring the new file into the tree (no hop=file_tree update naming it)")
    ctx.shot("lab_file_tree_update_%s" % arm["helper"])

    # and it goes away again the same way
    mark = ctx.log_size()
    lab.ssh(u, "rm -f %s/%s" % (repo, new))
    if arm["helper"] == "nohelper":
        ctx.be.type("true", CL.type_pace(ctx), enter=True)
    gone = ctx.until(lambda: any(e == "update" and rem >= 1 for e, _s, rem, _a, _h in tree_events(ctx.log_since(mark))), 15 + 6 * rt, step=0.1)
    r.check(bool(gone), "the removal of the file was never reported (no hop=file_tree update with removed>=1)")
    CL.finish_remote(ctx, r)
