# appci/cases_lab_index.py: lab case for remote codebase indexing (Wave H parity P9b), registered through lab_case.
#   lab_codebase_index_remote   a git repository on the ssh host is indexed by the APP: the host (the uploaded helper, or the app's shell path over the ControlMaster with
#                               no helper) scans, chunks, hashes and stores; the app embeds through the network gate with the user's own provider.
# The provider is script/fake_embeddings.py (APPCI_FAKE_EMBEDDINGS=1: run.py starts it on loopback, profile.py configures it as the BYOP embeddings provider and turns
# codebase context + auto indexing on; the provider's "key" is a sentinel header). Terminal mode. Oracles, all from logs and the host:
#   * the app log's `hop=remote_index_sync done ... embedded=N` (N>0 on the first sync), `hop=remote_index_embed ... via=network_gate`;
#   * the fake provider's own request log (it saw the sentinel, so the gate let the call through to the configured provider and to nothing else);
#   * the sentinel appears NOWHERE on the host (rec.log of the remote shell, the helper's index dir, the helper log): the key never leaves the user's machine;
#   * an outside edit + a HEAD-changing git command in the terminal re-syncs, and only the changed fragment is embedded (`embedded=1`);
#   * `hop=backend_route family=codebase_chunks backend=<helper|shell>` says which arm served it.
# No timer is involved: the sync fires on navigation (cd into the repo) and on the prompt after the git command.
import hashlib, os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases_lab as CL  # noqa: E402

SYNC = re.compile(r"hop=remote_index_sync done files=(\d+) fragments=(\d+) embedded=(\d+) already=(\d+) stale=(\d+)")
EMBED = re.compile(r"hop=remote_index_embed texts=(\d+) via=network_gate purpose=ai_endpoint")
ROUTE = re.compile(r"hop=backend_route family=codebase_chunks backend=(helper|shell|unavailable) reason=(\w+)")
SHELLS = ("bash", "zsh", "fish", "nu", "dash", "ash", "pwsh")


def syncs(text):
    """[(files, fragments, embedded, already, stale)] in log order"""
    return [tuple(int(g) for g in m.groups()) for m in SYNC.finditer(text)]


def sentinel():
    return os.environ.get("APPCI_EMBED_SENTINEL", "appci-embed-sentinel-0001")


def sentinel_sha():
    return hashlib.sha256(sentinel().encode()).hexdigest()[:8]


def provider_log(ctx):
    path = os.environ.get("APPCI_FAKE_EMBEDDINGS_LOG")
    try:
        return open(path).read().splitlines() if path else []
    except OSError:
        return []


@CL.lab_case("codebase_index_remote", shells=SHELLS, lat=("lan", "150"), helper=("nohelper", "upload"), modes=("terminal",))
def c_codebase_index_remote(ctx, r, arm):
    if os.environ.get("APPCI_FAKE_EMBEDDINGS_ACTIVE") != "1":
        r.skip("the fake embeddings provider is not running (APPCI_FAKE_EMBEDDINGS=1 starts script/fake_embeddings.py and configures it as the BYOP provider)")
        return
    lab, u, rt = ctx.lab, arm["user"], CL.rtt_s(arm)
    uid = CL.unique(ctx)
    fx = "idx_%s" % uid
    repo = CL.work(arm, fx)
    lab.tree(u, {"src/lib.rs": "pub fn one() {}\n", "a.txt": "alpha beta gamma\n", "notes.md": "# notes\nsome words here\n"}, dir=fx, git=True)
    off = ctx.log_size()
    n_provider = len(provider_log(ctx))
    if CL.connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * rt)
    if CL.plain_gate(ctx, r, arm, off, "the remote codebase index of the managed session"):             # #2160
        return
    ctx.be.type("cd %s" % repo, CL.type_pace(ctx), enter=True)
    first = ctx.until(lambda: bool(syncs(ctx.log_since(off))), 40 + 10 * rt, step=0.25)
    if not first:
        r.check(False, "no hop=remote_index_sync done after cd into the repository (the sync never ran or never finished)")
        ctx.reset()
        CL.finish_remote(ctx, r)
        return
    s1 = syncs(ctx.log_since(off))[0]
    r.check(s1[0] >= 3 and s1[2] >= 3 and s1[3] == 0, "first sync (files, fragments, embedded, already, stale) = %r: expected >=3 files embedded, none already" % (s1,))
    text = ctx.log_since(off)
    routes = {m.group(1) for m in ROUTE.finditer(text)}
    want = "helper" if arm["helper"] == "upload" else "shell"
    r.check(routes == {want}, "codebase_chunks was served by %s, expected %s" % (sorted(routes), want))
    r.check(bool(EMBED.search(text)), "no hop=remote_index_embed ... via=network_gate line: the embeddings did not go through the gate")
    seen = provider_log(ctx)[n_provider:]
    r.check(bool(seen) and all("key_sha=%s" % sentinel_sha() in l for l in seen), "the fake provider saw %r: expected requests carrying the user's key header" % seen[:3])

    # the key never reaches the host
    typed = [cmd for _, cmd in ctx.remote.commands() if sentinel() in cmd]
    r.check(not typed, "the sentinel key was typed into the remote shell")
    leaked = lab.ssh(u, "grep -rIl -- %s $HOME/.phosphor $HOME/%s 2>/dev/null | head -3" % (sentinel(), fx)).stdout
    r.check(not (leaked or "").strip(), "the sentinel key is on the host: %r" % leaked)

    # an outside edit, then a HEAD-changing command: only the changed fragment is embedded
    mark = ctx.log_size()
    lab.ssh(u, "echo 'delta epsilon' >> %s/a.txt" % repo)
    ctx.be.type("git checkout -q -b b_%s" % uid, CL.type_pace(ctx), enter=True)
    second = ctx.until(lambda: bool(syncs(ctx.log_since(mark))), 40 + 10 * rt, step=0.25)
    r.check(bool(second), "the prompt after a HEAD-changing git command did not re-sync the index")
    if second:
        s2 = syncs(ctx.log_since(mark))[0]
        r.check(s2[2] == 1 and s2[3] >= 2, "second sync (files, fragments, embedded, already, stale) = %r: expected exactly the one edited fragment embedded" % (s2,))
    ctx.shot("lab_codebase_index_%s" % arm["helper"])
    CL.finish_remote(ctx, r)


def declare_coverage():
    import cases_lab_helper as CLH
    CLH.declare_external("codebase_chunks", helper_case="lab_codebase_index_remote", nohelper_case="lab_codebase_index_remote")


if hasattr(sys.modules.get("cases_lab_helper"), "declare_external"):
    declare_coverage()
