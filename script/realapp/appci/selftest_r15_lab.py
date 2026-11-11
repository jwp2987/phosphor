# appci/selftest_r15_lab.py: offline self-tests of the round 15 lab work (#2062 the helper artifact, #2063 the lab families and their drivers, #2005 the
# keys kl3 posts). Pure Python: no app, lab, display or network. selftest.py appends CHECKS to its own list, so `run.py --self-test` runs these.
import json, os, re, shutil, sys, tempfile, types

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, HERE)
import cases  # noqa: E402
import backend, injacct  # noqa: E402
import lab as L  # noqa: E402
import cases_lab as CL  # noqa: E402
import cases_lab_buffers as CLB  # noqa: E402
import cases_lab_diff as CLD  # noqa: E402
import cases_lab_git as CLG  # noqa: E402

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


def _read(*parts):
    return open(os.path.join(ROOT, *parts), encoding="utf-8").read()


def _job(ci, name):
    """the text of one top-level job of .gitlab-ci.yml (to the next blank line)"""
    j = ci[ci.index("\n%s:\n" % name) + 1:]
    return j[:j.index("\n\n")]


@check
def r15_lab_helper_from_the_artifact():
    """#2062: a lab job on a machine with no helper cache (the Mac, the Windows VM) takes the `lab-helper` job's artifact, ./helper-artifacts/. The resolver
    uses it only as THIS checkout's helper, prefers an explicit APPCI_LAB_HELPER_BIN, falls back to the cache, and when the box could not build the helper it
    gives the recorded `NEEDS: ...` line as the helper arm's reason (the row is UNVERIFIABLE, never a pass, and says which machine change is missing)."""
    LH = CL.lab_helper_cache()
    d = tempfile.mkdtemp()
    try:
        art, cache = os.path.join(d, "helper-artifacts"), os.path.join(d, "cache")
        env = {"APPCI_LAB_HELPER_ARTIFACTS": art, "PHOSPHOR_CI_HELPER_CACHE": cache, "CI_PIPELINE_ID": "4242"}
        # nothing anywhere: the job is named; a NEEDS line appears only when something recorded one (or zig is absent where the build looks, on Linux)
        path, why = CL.resolve_helper_bin(dict(env, HOME=d, PATH=""))
        assert path == "" and "lab-helper" in why and "helper-artifacts" in why, why
        # the box could not build: the artifact holds the NEEDS record
        LH.write_needs(art, ["zig", "cargo-zigbuild"], "k", "2026-11-11T00:00:00Z")
        path, why = CL.resolve_helper_bin(env)
        assert path == "" and "NEEDS: zig, cargo-zigbuild installed" in why and "owner: machine change" in why and "could not build" in why, why
        assert why.count("NEEDS:") == 1, why
        shutil.rmtree(art)
        # the cache root holds the record (the Linux lab job, same machine)
        LH.write_needs(cache, ["zig"], "k")
        path, why = CL.resolve_helper_bin(env)
        assert path == "" and "NEEDS: zig installed" in why and why.count("NEEDS:") == 1, why
        LH.clear_needs(cache)
        # a real entry, exported as the job exports it
        try:
            key = LH.key_of(LH.key_inputs(LH.ROOT, LH.helper_tag(LH.ROOT)))
        except LH.NoKey:
            key = None
        tag = LH.helper_tag(LH.ROOT)
        line = "phosphor-remote %s protocol 1" % tag
        fake = os.path.join(d, "fake")
        open(fake, "wb").write(b"\x7fELF\x02\x01" + bytes(12) + (62).to_bytes(2, "little") + b" not a real helper")
        entry = LH.store(os.path.join(d, "elsewhere"), key or "nokey", fake, line, {"tag": tag})
        LH.export(entry, art, {"CI_PIPELINE_ID": "4242", "CI_JOB_ID": "9"})
        path, why = CL.resolve_helper_bin(env)
        assert why is None and path == os.path.join(art, "phosphor-remote"), (path, why)
        assert CL.helper_version_line(path) == line, "a helper this machine cannot start is read from the .version file the artifact carries"
        assert CL.helper_arch(path) == "x86_64"
        # explicit still wins; another pipeline's artifact with no key match is refused
        assert CL.resolve_helper_bin(dict(env, APPCI_LAB_HELPER_BIN=fake)) == (fake, None)
        meta = json.load(open(os.path.join(art, "meta.json")))
        meta["key"] = "0" * 24
        json.dump(meta, open(os.path.join(art, "meta.json"), "w"))
        path, why = CL.resolve_helper_bin(dict(env, CI_PIPELINE_ID="1"))
        if key is not None:
            assert path == "" and "another commit's helper" in why, (path, why)
        else:
            assert path == "" and "cannot be tied to this checkout" in why, (path, why)

        # lab_prepare: the helper arm stops with that reason, before anything is put on the lab host
        class FakeLab:
            info = {"flavor": "full"}
            helpers = []

            def helper(self, user, files=None):
                self.helpers.append(user)

        class R:
            msg = None

            def unverifiable(self, msg):
                self.msg = msg

            def skip(self, msg):
                raise cases.Skip(msg)
        import profile as P
        shutil.rmtree(art)
        LH.write_needs(art, ["zig"], "k")
        keep = {k: os.environ.get(k) for k in ("APPCI_LAB_HELPER_BIN", "APPCI_LAB_HELPER_ARTIFACTS", "PHOSPHOR_CI_HELPER_CACHE", "APPCI_APP_PINNED_DIGESTS")}
        try:
            os.environ.pop("APPCI_LAB_HELPER_BIN", None)
            os.environ.pop("APPCI_APP_PINNED_DIGESTS", None)
            os.environ["APPCI_LAB_HELPER_ARTIFACTS"], os.environ["PHOSPHOR_CI_HELPER_CACHE"] = art, cache
            prof = P.Profile(d)
            lab = FakeLab()
            ctx = types.SimpleNamespace(lab=lab, lab_why=None, prof=prof, remote=L.LocalRemote(prof), arm=None, lab_helper_mode="nohelper", run=d)
            arm = dict(name="lab_x.bash.lan.helper", user="u_bash", shell="bash", helper="helper", lat_ms=0, flavor="full")
            r = R()
            try:
                ok = CL.lab_prepare(ctx, r, arm)
            finally:
                CL.lab_cleanup(ctx, arm)
            assert ok is False and r.msg.startswith("helper arm: ") and "NEEDS: zig installed" in r.msg and lab.helpers == [], (ok, r.msg, lab.helpers)
        finally:
            for k, v in keep.items():
                if v is None:
                    os.environ.pop(k, None)
                else:
                    os.environ[k] = v
    finally:
        shutil.rmtree(d, ignore_errors=True)
    # the CI wiring
    ci = _read(".gitlab-ci.yml")
    helper = _job(ci, "lab-helper")
    assert "when: always" in helper[helper.index("artifacts:"):] and "helper-artifacts/" in helper, "lab-helper must upload helper-artifacts/ when it fails too (NEEDS.txt)"
    for name, build in (("app-test-lab-macos", "build-macos-arm64"), ("app-test-lab-windows", "build-windows-x86_64")):
        job = _job(ci, name)
        assert re.search(r"needs:\n    - job: %s\n      artifacts: false\n    - job: lab-helper\n      artifacts: true\n      optional: true\n" % build, job), job
        assert 'APPCI_APP_PINNED_DIGESTS: "0"' in job and "APPCI_LAB: fixed" in job, job
    linux = _job(ci, "app-test-lab-linux")
    assert "job: lab-helper" not in linux, "the Linux lab job reads the box's cache; a `needs` on the manual lab-helper job would make it unplayable until that job is played"
    assert "lab_helper.py artifact" in linux and "lab_helper.py path" in linux
    doc = _read("docs", "testing.md")
    assert "helper-artifacts/NEEDS.txt" in doc and "APPCI_LAB_HELPER_ARTIFACTS" in doc and "needs: lab-helper" in doc
