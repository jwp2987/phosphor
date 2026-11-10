#!/usr/bin/env python3
"""script/ci/lab_helper.py: the fake ssh lab's `phosphor-remote` helper, built ONCE per set of inputs on the build box and kept in a
machine-local cache (owner decision 2026-11-10; docs/design/r12-policy.md section 3, docs/testing.md 2.15).

WHY. The lab's `helper` arms pre-install a real `phosphor-remote` on the lab host. It has to be a static musl binary (it runs inside the lab
container) and it has to carry the SAME version name as the app under test (`phosphor-remote-<version>`: `remote_server::naming::helper_version`
is GIT_RELEASE_TAG, and a dev build's tag is `v<app version>-dev`). No CI job built one: `build-remote-helper` is a release job, and building
it in every dev build would add an LTO cross build to each. So it is built by the manual `lab-helper` job when its inputs change, and reused
by every lab run until they change again.

THE CACHE. `${PHOSPHOR_CI_HELPER_CACHE:-$HOME/.cache/phosphor-ci/helper}/<key>/` (the runner user's home, next to cache_build.sh's builds):
    phosphor-remote                     the binary (0755)
    phosphor-remote.version             first line of `phosphor-remote --version` (read where the binary cannot run: macOS, Windows)
    phosphor-remote-linux-x86_64.gz     gzip -9 of it, the name the app's bundle uses
    phosphor-remote-linux-x86_64.sha256 SHA-256 of the DECOMPRESSED binary (what a client pins)
    meta.json                           key, inputs, tag, size, when, which job
An entry is immutable: a second build of the same key is refused (two LTO builds are not promised to be byte-identical, and a digest pinned
from the first one must stay true). Newest KEEP entries are kept.

THE KEY. sha256 over: the tag, the target, the profile, the features, this recipe's revision, and the git ids of everything the helper is
built from: the trees in KEY_TREES (the workspace crates in the helper's dependency tree: remote_server itself, `command`, and the vendored
`notify`) and the files Cargo.toml, Cargo.lock, rust-toolchain.toml, .cargo/config.toml. No cargo is needed to compute it, so a lab job
computes it in milliseconds, and a commit that touches none of those (most of them: `app/`, the other crates) reuses the entry. KEY_TREES is
a hand-kept list, so `build` PROVES it before every build: `cargo tree` for the helper's exact feature set names every local crate, and one
that lies outside KEY_TREES fails the job with the line to add (a stale list can never produce a helper cached under the wrong key).
A tree that is dirty under those paths has no key (nothing is read or stored).

COMMANDS
    key             print the key of this checkout
    path            print the cached binary's path; exit 3 (reason on stderr) when this checkout's helper is not in the cache
    build           the `lab-helper` job: build it unless cached, verify it (static, version line, size), store it, and copy the entry to
                    ./helper-artifacts/ for the job's artifact. Installs NOTHING: a missing musl target, zig or cargo-zigbuild is an error
                    naming what is missing (they are the tools the cli-linux release job leaves in the runner user's home).
    --self-test     the pure parts, in a temp dir

Exit: 0 ok, 1 failed, 2 usage, 3 not cached (path).
"""
import hashlib
import json
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import time

RECIPE = "1"                                    # bump when the build command below changes
TARGET = "x86_64-unknown-linux-musl"
ARCH = "x86_64"
PROFILE = "release-remote-helper"
FEATURES = "daemon"
BIN = "phosphor-remote"
GZ = "phosphor-remote-linux-%s.gz" % ARCH
SHA = "phosphor-remote-linux-%s.sha256" % ARCH
KEY_TREES = ("crates/remote_server", "crates/command", "lib/notify")      # proven complete by `build` (local_crates_outside_key)
KEY_FILES = ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml", ".cargo/config.toml")
MAX_BYTES = 20 * 1024 * 1024                    # the release job's size gate (build-remote-helper)
KEEP = 3
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


class NoKey(Exception):
    pass


def dev_tag(root):
    """the GIT_RELEASE_TAG the dev build jobs export: v<version of app/Cargo.toml>-dev (.gitlab-ci.yml build-linux-x86_64 / build-macos-arm64)"""
    for line in open(os.path.join(root, "app", "Cargo.toml"), encoding="utf-8"):
        if line.startswith('version = "'):
            return "v%s-dev" % line.split('"')[1]
    raise NoKey("no version line in app/Cargo.toml")


def helper_tag(root, env=None):
    env = os.environ if env is None else env
    return env.get("LAB_HELPER_TAG") or dev_tag(root)


def git_out(root, *args):
    p = subprocess.run(("git", "-C", root) + args, capture_output=True, text=True, stdin=subprocess.DEVNULL)
    return p.stdout.strip() if p.returncode == 0 else None


def key_inputs(root, tag, git=git_out):
    """[(name, value)] in a fixed order. Raises NoKey when the checkout cannot name them (not a git checkout, or dirty under the key paths)."""
    paths = list(KEY_TREES) + list(KEY_FILES)
    dirty = git(root, "status", "--porcelain", "--untracked-files=all", "--", *paths)
    if dirty is None:
        raise NoKey("not a git checkout (git status failed): the helper's inputs cannot be identified")
    if dirty:
        raise NoKey("the checkout has uncommitted changes under %s: the helper's inputs are not a commit's, so nothing is cached for them" % ", ".join(paths))
    out = [("recipe", RECIPE), ("tag", tag), ("target", TARGET), ("profile", PROFILE), ("features", FEATURES)]
    for p in paths:
        out.append((p, git(root, "rev-parse", "--verify", "--quiet", "HEAD:" + p) or "absent"))
    gone = [n for n, v in out if v == "absent" and n in KEY_TREES + ("Cargo.toml", "Cargo.lock")]
    if gone:
        raise NoKey("%s is not in this commit: KEY_TREES in script/ci/lab_helper.py no longer names the helper's crates" % ", ".join(gone))
    return out


def key_of(inputs):
    h = hashlib.sha256()
    for n, v in inputs:
        h.update(("%s=%s\n" % (n, v)).encode())
    return h.hexdigest()[:24]


def cache_root(env=None):
    env = os.environ if env is None else env
    return env.get("PHOSPHOR_CI_HELPER_CACHE") or os.path.join(os.path.expanduser("~"), ".cache", "phosphor-ci", "helper")


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def entry_problem(d):
    """None when directory `d` is a complete, self-consistent entry; else what is wrong with it"""
    for n in (BIN, BIN + ".version", GZ, SHA, "meta.json"):
        if not os.path.isfile(os.path.join(d, n)):
            return "%s is missing" % n
    want = open(os.path.join(d, SHA)).read().strip()
    got = sha256_file(os.path.join(d, BIN))
    if want != got:
        return "the binary's SHA-256 is %s, the entry records %s" % (got, want)
    if not version_line_ok(open(os.path.join(d, BIN + ".version")).readline().strip(), None):
        return "the recorded version line is not `phosphor-remote <version> protocol <n>`"
    return None


def cached_entry(root, env=None, git=git_out):
    """(dir, None) for this checkout's entry, or (None, reason)"""
    try:
        key = key_of(key_inputs(root, helper_tag(root, env), git))
    except NoKey as e:
        return None, str(e)
    d = os.path.join(cache_root(env), key)
    if not os.path.isdir(d):
        return None, "no cached helper for key %s under %s: play the manual `lab-helper` job on this ref (it builds the helper once and every later lab run reuses it)" % (key, cache_root(env))
    why = entry_problem(d)
    if why:
        return None, "the cached helper %s is damaged (%s): remove that directory and play `lab-helper` again" % (d, why)
    return d, None


def version_line_ok(line, tag):
    """`phosphor-remote <version> protocol <n>`; with `tag`, the version must be exactly it"""
    parts = line.split()
    return len(parts) == 4 and parts[0] == "phosphor-remote" and parts[2] == "protocol" and parts[3].isdigit() and (tag is None or parts[1] == tag)


def elf_is_static(data):
    """True for a 64-bit little-endian ELF with no PT_INTERP program header: it asks for no dynamic loader, so it runs in a container that has
    none of the box's libraries (a static musl link; a static-pie one keeps PT_DYNAMIC for its own relocations and still has no interpreter).
    False when it names an interpreter; None when `data` is not such an ELF at all"""
    if len(data) < 64 or data[:4] != b"\x7fELF" or data[4] != 2 or data[5] != 1:
        return None
    phoff, = struct.unpack_from("<Q", data, 32)
    phentsize, phnum = struct.unpack_from("<HH", data, 54)
    if phentsize < 4 or phoff + phentsize * phnum > len(data):
        return None
    types = [struct.unpack_from("<I", data, phoff + i * phentsize)[0] for i in range(phnum)]
    return 3 not in types                               # PT_INTERP = 3


def local_crates_outside_key(tree_output, root):
    """`cargo tree --prefix none --format '{p}'` prints a path dependency as `name vX.Y.Z (/abs/dir)`. Returns the local crate directories (relative
    to `root`) that are NOT inside one of KEY_TREES, sorted; [] proves the key covers the helper's sources. A path outside the checkout counts too."""
    out = set()
    for line in tree_output.splitlines():
        line = line.strip()
        if not line.endswith(")") or " (" not in line:
            continue
        inside = line[line.rindex(" (") + 2:-1]
        if not os.path.isabs(inside):
            continue                                    # (proc-macro), (*) and the like
        rel = os.path.relpath(inside, root).replace(os.sep, "/")
        if not any(rel == t or rel.startswith(t + "/") for t in KEY_TREES):
            out.add(rel)
    return sorted(out)


def store(cache, key, binary, version_line, meta):
    """put `binary` into the cache as entry `key`, atomically. Refuses an existing entry (immutable). Returns the entry directory."""
    import gzip
    d = os.path.join(cache, key)
    if os.path.exists(d):
        raise RuntimeError("entry %s already exists; an entry is never replaced (a digest pinned from it must stay true). Remove it by hand to rebuild." % d)
    os.makedirs(cache, exist_ok=True)
    tmp = tempfile.mkdtemp(prefix=".tmp-", dir=cache)
    try:
        shutil.copyfile(binary, os.path.join(tmp, BIN))
        os.chmod(os.path.join(tmp, BIN), 0o755)
        with open(os.path.join(tmp, BIN), "rb") as src, open(os.path.join(tmp, GZ), "wb") as raw:
            with gzip.GzipFile(filename="", mode="wb", compresslevel=9, fileobj=raw, mtime=0) as dst:
                shutil.copyfileobj(src, dst)
        digest = sha256_file(os.path.join(tmp, BIN))
        open(os.path.join(tmp, SHA), "w").write(digest + "\n")
        open(os.path.join(tmp, BIN + ".version"), "w").write(version_line + "\n")
        json.dump(dict(meta, key=key, sha256=digest, size=os.path.getsize(os.path.join(tmp, BIN)), version_line=version_line),
                  open(os.path.join(tmp, "meta.json"), "w"), indent=1, sort_keys=True)
        os.chmod(tmp, 0o755)
        os.rename(tmp, d)
    except BaseException:
        shutil.rmtree(tmp, ignore_errors=True)
        raise
    return d


def prune(cache, keep=KEEP, keep_dir=None):
    """keep the newest `keep` entries (by mtime; `keep_dir` always stays); stale .tmp-* directories older than two hours go too"""
    ents = []
    for n in os.listdir(cache):
        p = os.path.join(cache, n)
        if not os.path.isdir(p):
            continue
        if n.startswith(".tmp-"):
            if time.time() - os.path.getmtime(p) > 7200:
                shutil.rmtree(p, ignore_errors=True)
            continue
        ents.append((os.path.getmtime(p), p))
    ents.sort(reverse=True)
    gone = []
    for _, p in ents[keep:]:
        if keep_dir and os.path.samefile(p, keep_dir):
            continue
        shutil.rmtree(p, ignore_errors=True)
        gone.append(p)
    return gone


def export(entry, dest):
    """copy the entry's files to `dest` (the job's artifact directory)"""
    os.makedirs(dest, exist_ok=True)
    for n in (BIN, BIN + ".version", GZ, SHA, "meta.json"):
        shutil.copyfile(os.path.join(entry, n), os.path.join(dest, n))
    os.chmod(os.path.join(dest, BIN), 0o755)


def missing_tools(env):
    """the build's prerequisites that are absent, as messages. Nothing is installed here."""
    out = []
    for tool, why in (("cargo", "the rust toolchain"), ("cargo-zigbuild", "the cli-linux release job installs it into the runner user's home"),
                      ("zig", "the cli-linux release job installs it into $HOME/.local/zig-*")):
        if shutil.which(tool, path=env.get("PATH")) is None:
            out.append("%s is missing for this user (%s; nothing is installed by this job)" % (tool, why))
    if not out:
        p = subprocess.run(["rustup", "target", "list", "--installed"], capture_output=True, text=True, env=env, stdin=subprocess.DEVNULL)
        if p.returncode != 0 or TARGET not in p.stdout.split():
            out.append("the %s rust target is not installed (`rustup target add %s`, once, by the toolchain owner)" % (TARGET, TARGET))
    return out


def cmd_build(root):
    env = dict(os.environ)
    tag = helper_tag(root, env)
    try:
        inputs = key_inputs(root, tag)
    except NoKey as e:
        print("lab_helper: ERROR %s" % e, file=sys.stderr)
        return 1
    key = key_of(inputs)
    cache = cache_root(env)
    d = os.path.join(cache, key)
    if os.path.isdir(d):
        why = entry_problem(d)
        if why:
            print("lab_helper: ERROR the cached entry %s is damaged (%s); remove it by hand, then play this job again" % (d, why), file=sys.stderr)
            return 1
        os.utime(d, None)                       # used now: the prune keeps the entries in use
        print("lab_helper: key %s is already cached at %s (no build)" % (key, d))
    else:
        home = os.path.expanduser("~")
        extra = [os.path.join(env.get("CARGO_HOME") or os.path.join(home, ".cargo"), "bin"), os.path.join(home, ".local", "bin")]
        local = os.path.join(home, ".local")
        if os.path.isdir(local):
            extra += [os.path.join(local, n) for n in sorted(os.listdir(local), reverse=True) if n.startswith("zig-") and os.path.isfile(os.path.join(local, n, "zig"))]
        env["PATH"] = os.pathsep.join(extra + [env.get("PATH", "")])
        miss = missing_tools(env)
        if miss:
            for m in miss:
                print("lab_helper: ERROR %s" % m, file=sys.stderr)
            return 1
        # the helper must stay warpui-free; the same gate build-remote-helper runs first (metadata only)
        if subprocess.run([os.path.join(root, "script", "check_remote_helper_deps")], cwd=root, env=env).returncode != 0:
            return 1
        # the key must cover every local crate the helper is built from (normal and build dependencies of its exact feature set)
        tree = subprocess.run(["cargo", "tree", "-p", "remote_server", "--no-default-features", "--features", FEATURES, "--target", TARGET, "-e", "normal,build",
                               "--prefix", "none", "--format", "{p}"], cwd=root, env=env, capture_output=True, text=True, stdin=subprocess.DEVNULL)
        if tree.returncode != 0 or "remote_server" not in tree.stdout:
            print("lab_helper: ERROR cargo tree failed, the key cannot be proven complete:\n%s" % (tree.stderr or tree.stdout)[-2000:], file=sys.stderr)
            return 1
        outside = local_crates_outside_key(tree.stdout, root)
        if outside:
            print("lab_helper: ERROR the helper is built from local crates the cache key does not cover: %s. Add them to KEY_TREES in script/ci/lab_helper.py "
                  "(and bump RECIPE); nothing was built or cached." % ", ".join(outside), file=sys.stderr)
            return 1
        # same pkg-config isolation as cli-linux / build-remote-helper: the box's glibc dev libs must not leak into the musl link
        empty = os.path.join(root, ".musl-pkgconfig-empty")
        os.makedirs(empty, exist_ok=True)
        env.update({"PKG_CONFIG_LIBDIR_x86_64_unknown_linux_musl": empty, "PKG_CONFIG_PATH_x86_64_unknown_linux_musl": "", "PKG_CONFIG_ALLOW_CROSS": "1",
                    "GIT_RELEASE_TAG": tag, "PHOSPHOR_REMOTE_BUILD_ID": "lab-" + key[:12]})
        cmd = ["cargo", "zigbuild", "-p", "remote_server", "--profile", PROFILE, "--no-default-features", "--features", FEATURES, "--target", TARGET, "--bin", BIN]
        print("lab_helper: building key %s (GIT_RELEASE_TAG=%s): %s" % (key, tag, " ".join(cmd)), flush=True)
        if subprocess.run(cmd, cwd=root, env=env).returncode != 0:
            print("lab_helper: ERROR the helper build failed", file=sys.stderr)
            return 1
        target_dir = env.get("CARGO_TARGET_DIR") or os.path.join(root, "target")
        binary = os.path.join(target_dir, TARGET, PROFILE, BIN)
        if not os.path.isfile(binary):
            print("lab_helper: ERROR no helper at %s (is CARGO_TARGET_DIR set elsewhere?)" % binary, file=sys.stderr)
            return 1
        size = os.path.getsize(binary)
        if size >= MAX_BYTES:
            print("lab_helper: ERROR the helper is %d bytes, over the %d gate" % (size, MAX_BYTES), file=sys.stderr)
            return 1
        static = elf_is_static(open(binary, "rb").read())
        if static is not True:
            print("lab_helper: ERROR %s is not a static x86_64 ELF (%s): it would not run inside the lab container" % (binary, "it names a dynamic loader" if static is False else "not an ELF64"), file=sys.stderr)
            return 1
        p = subprocess.run([binary, "--version"], capture_output=True, text=True, stdin=subprocess.DEVNULL, timeout=20)
        line = (p.stdout.splitlines() or [""])[0].strip()
        if p.returncode != 0 or not version_line_ok(line, tag):
            print("lab_helper: ERROR `%s --version` printed %r, expected `phosphor-remote %s protocol <n>` (the app under test names the helper after that tag)" % (BIN, line, tag), file=sys.stderr)
            return 1
        meta = {"inputs": dict(inputs), "tag": tag, "created": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                "job_id": env.get("CI_JOB_ID", ""), "pipeline_id": env.get("CI_PIPELINE_ID", ""), "sha": env.get("CI_COMMIT_SHA", "")}
        d = store(cache, key, binary, line, meta)
        print("lab_helper: stored %s (%d bytes, %s)" % (d, size, line))
    for p in prune(cache, KEEP, d):
        print("lab_helper: pruned %s" % p)
    export(d, os.path.join(root, "helper-artifacts"))
    print("lab_helper: helper-artifacts/ holds key %s: %s" % (key, open(os.path.join(d, BIN + ".version")).readline().strip()))
    return 0


def self_test():
    ok = True

    def check(cond, what):
        nonlocal ok
        print("%s %s" % ("ok  " if cond else "FAIL", what))
        ok = ok and bool(cond)
    tmp = tempfile.mkdtemp(prefix="lab-helper-selftest-")
    try:
        root = os.path.join(tmp, "repo")
        os.makedirs(os.path.join(root, "app"))
        open(os.path.join(root, "app", "Cargo.toml"), "w").write('[package]\nname = "warp"\nversion = "0.1.18"\n')
        check(dev_tag(root) == "v0.1.18-dev" and helper_tag(root, {}) == "v0.1.18-dev" and helper_tag(root, {"LAB_HELPER_TAG": "v9"}) == "v9", "the tag is the dev build's, or LAB_HELPER_TAG")
        ids = {"HEAD:crates/remote_server": "c1", "HEAD:crates/command": "c2", "HEAD:lib/notify": "l1", "HEAD:Cargo.toml": "t1", "HEAD:Cargo.lock": "k1",
               "HEAD:rust-toolchain.toml": "r1"}
        state = {"dirty": ""}

        def fake_git(_root, *args):
            if args[0] == "status":
                return state["dirty"]
            return ids.get(args[-1])
        k1 = key_of(key_inputs(root, "v0.1.18-dev", fake_git))
        check(k1 == key_of(key_inputs(root, "v0.1.18-dev", fake_git)) and len(k1) == 24, "the key is stable")
        check(dict(key_inputs(root, "v0.1.18-dev", fake_git))[".cargo/config.toml"] == "absent", "an optional file that is absent is part of the key as such")
        check(key_of(key_inputs(root, "v0.1.19-dev", fake_git)) != k1, "another tag is another key (the app would name the helper differently)")
        for name in ("HEAD:crates/remote_server", "HEAD:crates/command", "HEAD:lib/notify", "HEAD:Cargo.lock", "HEAD:Cargo.toml", "HEAD:rust-toolchain.toml"):
            old, ids[name] = ids[name], "changed"
            check(key_of(key_inputs(root, "v0.1.18-dev", fake_git)) != k1, "a change of %s is another key" % name[5:])
            ids[name] = old
        ids["HEAD:app"] = "a-changed"
        ids["HEAD:crates/warpui"] = "w-changed"
        check(key_of(key_inputs(root, "v0.1.18-dev", fake_git)) == k1, "a change under app/ or in a crate the helper does not use keeps the key")
        for t in KEY_TREES:
            check(os.path.isfile(os.path.join(ROOT, t, "Cargo.toml")), "KEY_TREES entry %s is a crate (or a vendored workspace) of this checkout" % t)
        man = open(os.path.join(ROOT, "crates", "remote_server", "Cargo.toml"), encoding="utf-8").read()
        check('daemon = ["dep:grep", "dep:ignore", "dep:serde_json", "dep:sha2", "dep:notify"]' in man and "command.workspace = true" in man,
              "remote_server's daemon feature still has the local dependencies KEY_TREES was written for (command, notify); re-check KEY_TREES when this fails")
        tree = "remote_server v0.1.0 (%s/crates/remote_server)\nanyhow v1.0.86\ncommand v0.1.0 (%s/crates/command)\nnotify v8.0.0 (%s/lib/notify/notify)\n" \
               "notify-types v2.0.0 (%s/lib/notify/notify-types)\nprost-derive v0.13.1 (proc-macro)\nlog v0.4.22 (*)\n" % ((root,) * 4)
        check(local_crates_outside_key(tree, root) == [], "the helper's own crates are inside KEY_TREES")
        tree2 = tree + "warp_util v0.1.0 (%s/crates/warp_util)\nelsewhere v1.0.0 (/somewhere/else)\n" % root
        got = local_crates_outside_key(tree2, root)
        check(len(got) == 2 and "crates/warp_util" in got, "a local crate outside KEY_TREES is reported (%r)" % got)
        check(local_crates_outside_key("command v0.1.0 (%s/crates/command_extra)\n" % root, root) == ["crates/command_extra"], "a sibling directory with the same prefix is not covered")
        state["dirty"] = " M crates/remote_server/src/lib.rs"
        try:
            key_inputs(root, "v0.1.18-dev", fake_git)
            check(False, "a dirty checkout has no key")
        except NoKey as e:
            check("uncommitted" in str(e), "a dirty checkout has no key")
        state["dirty"] = ""
        old, ids["HEAD:Cargo.lock"] = ids["HEAD:Cargo.lock"], None
        try:
            key_inputs(root, "v0.1.18-dev", fake_git)
            check(False, "a commit without Cargo.lock has no key")
        except NoKey as e:
            check("Cargo.lock" in str(e), "a commit without Cargo.lock has no key")
        ids["HEAD:Cargo.lock"] = old
        try:
            key_inputs(root, "t", lambda *_a: None)
            check(False, "outside a git checkout there is no key")
        except NoKey:
            check(True, "outside a git checkout there is no key")
        # the cache
        cache = os.path.join(tmp, "cache")
        env = {"PHOSPHOR_CI_HELPER_CACHE": cache}
        d, why = cached_entry(root, env, fake_git)
        check(d is None and "lab-helper" in why and k1 in why, "a missing entry says which key and which job makes it")
        fake = os.path.join(tmp, "fake-helper")
        open(fake, "wb").write(b"\x7fELF fake helper bytes")
        stored = store(cache, k1, fake, "phosphor-remote v0.1.18-dev protocol 1", {"tag": "v0.1.18-dev"})
        d, why = cached_entry(root, env, fake_git)
        check(d == stored and why is None and entry_problem(stored) is None, "a stored entry is found by its key")
        check(os.access(os.path.join(stored, BIN), os.X_OK) and open(os.path.join(stored, SHA)).read().strip() == sha256_file(fake), "the entry holds an executable and its SHA-256")
        import gzip
        check(gzip.open(os.path.join(stored, GZ), "rb").read() == open(fake, "rb").read(), "the .gz decompresses to the binary the digest names")
        check(json.load(open(os.path.join(stored, "meta.json")))["key"] == k1, "meta.json names the key")
        try:
            store(cache, k1, fake, "phosphor-remote v0.1.18-dev protocol 1", {})
            check(False, "an entry is never replaced")
        except RuntimeError:
            check(True, "an entry is never replaced")
        check(not [n for n in os.listdir(cache) if n.startswith(".tmp-")], "no temp directory is left behind")
        open(os.path.join(stored, BIN), "ab").write(b"x")
        d, why = cached_entry(root, env, fake_git)
        check(d is None and "damaged" in why, "an entry whose binary no longer matches its digest is refused")
        shutil.rmtree(stored)
        # prune: newest KEEP stay, the entry in use always stays
        made = []
        for i in range(5):
            made.append(store(cache, "key%d" % i, fake, "phosphor-remote v1 protocol 1", {}))
            os.utime(made[-1], (1000 + i, 1000 + i))
        gone = prune(cache, 3, made[0])
        left = sorted(n for n in os.listdir(cache))
        check(left == ["key0", "key2", "key3", "key4"] and gone == [made[1]], "prune keeps the newest three and the entry in use (%r)" % left)
        dest = os.path.join(tmp, "helper-artifacts")
        export(made[4], dest)
        check(sorted(os.listdir(dest)) == sorted([BIN, BIN + ".version", GZ, SHA, "meta.json"]) and os.access(os.path.join(dest, BIN), os.X_OK), "export copies the whole entry")
        # version line and ELF checks
        check(version_line_ok("phosphor-remote v0.1.18-dev protocol 1", "v0.1.18-dev") and version_line_ok("phosphor-remote v1 protocol 12", None), "a version line is accepted")
        check(not version_line_ok("phosphor-remote v0.1.17 protocol 1", "v0.1.18-dev") and not version_line_ok("phosphor-oss v1 protocol 1", None)
              and not version_line_ok("phosphor-remote v1", None) and not version_line_ok("", None), "another tag, another program or a short line is refused")

        def elf(types):
            head = bytearray(64)
            head[:6] = b"\x7fELF\x02\x01"
            struct.pack_into("<Q", head, 32, 64)
            struct.pack_into("<HH", head, 54, 56, len(types))
            body = b"".join(struct.pack("<I", t) + bytes(52) for t in types)
            return bytes(head) + body
        check(elf_is_static(elf([6, 1, 1, 4])) is True and elf_is_static(elf([1, 2])) is True, "an ELF that names no interpreter is static (static-pie included)")
        check(elf_is_static(elf([6, 3, 1, 2])) is False, "PT_INTERP means it needs the host's loader")
        check(elf_is_static(b"MZ" + bytes(100)) is None and elf_is_static(b"\x7fELF") is None and elf_is_static(elf([1])[:70]) is None, "not an ELF64, or a truncated one, is neither")
        # the constants the app and the release job use
        ci = open(os.path.join(ROOT, ".gitlab-ci.yml"), encoding="utf-8").read()
        check('GIT_RELEASE_TAG="v$(sed -n \'s/^version = "\\(.*\\)"/\\1/p\' app/Cargo.toml | head -1)-dev"' in ci, "the dev build jobs still derive the tag the way dev_tag does")
        check("--profile %s --no-default-features --features %s" % (PROFILE, FEATURES) in ci, "the release job builds the helper with the same profile and features")
        setup = open(os.path.join(ROOT, "crates", "remote_server", "src", "setup.rs"), encoding="utf-8").read()
        check('DEV_MUSL_TARGET: &str = "%s"' % TARGET in setup and 'DEV_REMOTE_PROFILE: &str = "%s"' % PROFILE in setup, "target and profile are the ones remote_server::setup names")
        check(dev_tag(ROOT).startswith("v") and dev_tag(ROOT).endswith("-dev"), "this checkout's app/Cargo.toml has a version line")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    print("self-test: %s" % ("ok" if ok else "FAILED"))
    return 0 if ok else 1


def main(argv):
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    cmd = argv[1]
    if cmd == "--self-test":
        return self_test()
    if cmd == "key":
        try:
            print(key_of(key_inputs(ROOT, helper_tag(ROOT))))
        except NoKey as e:
            print("lab_helper: ERROR %s" % e, file=sys.stderr)
            return 1
        return 0
    if cmd == "path":
        d, why = cached_entry(ROOT)
        if d is None:
            print("lab_helper: %s" % why, file=sys.stderr)
            return 3
        print(os.path.join(d, BIN))
        return 0
    if cmd == "build":
        return cmd_build(ROOT)
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
