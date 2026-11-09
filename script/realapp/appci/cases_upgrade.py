# appci/cases_upgrade.py: `upgrade_from_v0_1_17` (#1546 "coverage gap: upgrades/migrations/installer"; docs/testing.md 2.18).
# A profile in the shape the v0.1.17 release writes (owner-like setup, every value made up, every endpoint loopback) is upgraded by the CURRENT build and every
# row of upgrade_table.py is asserted. Registered by the last import line of cases.py; full tier (candidate + release), none in SMOKE. Linux first: the seed paths,
# the keyring helper and the launcher are the Linux ones (macOS / Windows skip with their reason; the table and the fixture are platform-neutral).
#
# Where the old profile comes from, in order: the real v0.1.17 build (APPCI_PREV_RELEASE_BIN, the cache, or a download of the release AppImage:
# prev_release.py) launched on the seeded profile so it writes its own state (sqlite, user_preferences, keyring access); else the checked-in fixture alone
# (fixtures/upgrade_v0_1_17/: settings, keybindings, a user theme, the Claude plugin registration, plus the keyring items when secret-tool exists).
# The case SKIPS with the reason only when neither a build nor the fixture is usable.
import hashlib, json, os, re, shutil, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases as C  # noqa: E402  (cases.py imports this module as its LAST lines)
import cases_coverage_b as CB  # noqa: E402
import cases_robust as R  # noqa: E402
import prev_release  # noqa: E402
import toml_text as T  # noqa: E402
import upgrade_table as U  # noqa: E402
from cases import case  # noqa: E402

KEYRING_SERVICE = os.environ.get("APPCI_KEYRING_SERVICE", "dev.phosphor.Phosphor")      # ChannelState::data_domain() = AppId "dev.phosphor.Phosphor"
SECRETS = {                                                                              # made-up values; the keys are the SECURE_STORAGE_KEYs in the app
    "ProxyPassword": "upgrade-probe-not-a-secret-1",
    "AgentProviderSecrets": json.dumps({"upg-ollama": "upgrade-probe-key-a", "upg-claude-proxy": "upgrade-probe-key-b"}, sort_keys=True),
}
REMOVED_ENV = {"PHOSPHOR_BOOTSTRAP_TIMED": "1", "PHOSPHOR_SSH_BOOTSTRAP_GIVEUP_DELIVERS": "1", "PHOSPHOR_SUBSHELL_READY": "0"}
VOLATILE = re.compile(r"(\.log|\.lock|\.sock|\.pid|-wal|-shm|\.tmp|/cache/|/Cache/|\.in_session\.|crash|keyrings/)")


# ---------------------------------------------------------------- pure helpers (unit-tested in selftest.py `upgrade_case_pure_helpers`)
def render_seed(text, bash, home):
    """a fixture text with its placeholders filled: @BASH@ the shell path, @HOME@ the scratch home"""
    return text.replace("@BASH@", bash.replace("\\", "/")).replace("@HOME@", home.replace("\\", "/"))


def tree_hashes(root, skip=VOLATILE):
    """{relative path: sha256} of every regular file under `root` that is not volatile (logs, locks, sockets, caches, the keyring files)"""
    out = {}
    for base, _d, files in os.walk(root):
        for f in files:
            fp = os.path.join(base, f)
            rel = os.path.relpath(fp, root).replace("\\", "/")
            if os.path.islink(fp) or skip.search("/" + rel):
                continue
            try:
                out[rel] = hashlib.sha256(open(fp, "rb").read()).hexdigest()
            except OSError:
                pass
    return out


def lost_files(before, after):
    """paths the old build left that the new build deleted"""
    return sorted(set(before) - set(after))


UPGRADE_ERROR_WORDS = ("settings", "keybinding", "theme", "plugin", "marketplace", "secure", "keyring", "provider", "migrat", "sqlite", "diesel", "database", "network gate", "denied")
ERROR_LINE = re.compile(r"(?<![A-Za-z_])ERROR(?![A-Za-z_])|\[error\]", re.I)


def upgrade_log_problems(text):
    """-> lines of ONE launch's app log that mean the upgrade failed: a panic, a migration / database / settings ERROR (startup_log_problems), or an ERROR line
    that names one of UPGRADE_ERROR_WORDS (a settings, keybindings, theme, plugin, keyring or provider failure). A loopback model server that is not
    running is not one of them: connection errors do not name those words."""
    out = list(CB.startup_log_problems(text))
    for line in text.splitlines():
        if ERROR_LINE.search(line) and any(w in line.lower() for w in UPGRADE_ERROR_WORDS):
            s = line.strip()[:200]
            if s not in out:
                out.append(s)
    return out


def downgrade_report(pre_text, post_text, prompts_ok, log_text):
    """what v0.1.17 did when opened on the migrated profile, as data (recorded, never a failure): did it reach a prompt, which seeded keys the old build lost,
    which problem lines it logged"""
    lost = []
    for table, key in sorted(U.seeded_keys()):
        if U.norm_value(T.get_key(pre_text, table, key)) is not None and U.norm_value(T.get_key(post_text, table, key)) is None:
            lost.append("%s.%s" % (table, key))
    return dict(reached_prompt=bool(prompts_ok), seeded_keys_lost=lost, problem_lines=upgrade_log_problems(log_text)[:5])


# ---------------------------------------------------------------- the profile
def paths(home):
    cfg, data = os.path.join(home, ".config", "phosphor"), os.path.join(home, ".local", "share", "phosphor")
    return dict(cfg=cfg, data=data, settings=os.path.join(cfg, "settings.toml"), keybindings=os.path.join(cfg, "keybindings.yaml"),
                theme=os.path.join(data, "themes", "upgrade_probe.yaml"), claude=os.path.join(home, ".claude"),
                claude_files=[os.path.join(home, ".claude", "plugins", n) for n in ("known_marketplaces.json", "installed_plugins.json")])


def read(path):
    with open(path, "rb") as f:
        return f.read()


def seed_files(home, bash):
    """writes the fixture into the scratch profile (the app must be stopped). -> the settings text written"""
    p = paths(home)
    fx = U.FIXTURE_DIR
    for d in (p["cfg"], os.path.dirname(p["theme"]), os.path.join(p["claude"], "plugins")):
        os.makedirs(d, exist_ok=True)
    text = render_seed(open(os.path.join(fx, "settings.toml"), encoding="utf-8").read(), bash, home)
    with open(p["settings"], "w", newline="\n") as f:
        f.write(text)
    os.chmod(p["settings"], 0o600)
    shutil.copyfile(os.path.join(fx, "keybindings.yaml"), p["keybindings"])
    shutil.copyfile(os.path.join(fx, "themes", "upgrade_probe.yaml"), p["theme"])
    for name, dest in zip(("known_marketplaces.json", "installed_plugins.json"), p["claude_files"]):
        with open(dest, "w", newline="\n") as f:
            f.write(render_seed(open(os.path.join(fx, "claude_plugins", name), encoding="utf-8").read(), bash, home))
    return text


def keyring(ctx, ops):
    """runs secret-tool operations inside the same kind of private D-Bus session + unlocked keyring daemon the launcher gives the app (app.py INNER), on the
    profile's HOME so the keyring files are the app's. ops: [("store", key, value) | ("lookup", key)] -> {key: value or None} for the lookups. None-valued
    result (the whole dict None) when secret-tool or dbus-run-session is missing."""
    if not (shutil.which("secret-tool") and shutil.which("dbus-run-session") and shutil.which("gnome-keyring-daemon")):
        return None
    home, run = ctx.prof.home, ctx.run
    lines = ["#!/bin/bash", "unset GNOME_KEYRING_CONTROL SSH_AUTH_SOCK",
             'export HOME="%s" XDG_CONFIG_HOME="%s/.config" XDG_STATE_HOME="%s/.local/state" XDG_DATA_HOME="%s/.local/share"' % ((home,) * 4),
             'mkdir -p "$XDG_DATA_HOME" "$XDG_STATE_HOME"', 'printf "" | gnome-keyring-daemon --unlock --components=secrets >/dev/null 2>&1']
    n = 0
    for op in ops:
        if op[0] == "store":
            lines.append("printf %%s %s | secret-tool store --label='appci upgrade probe' service '%s' key '%s'" % (shell_q(op[2]), KEYRING_SERVICE, op[1]))
        else:
            n += 1
            lines.append("echo \"KEY:%s\"; secret-tool lookup service '%s' key '%s'; echo; echo \"END\"" % (op[1], KEYRING_SERVICE, op[1]))
    lines.append("sleep 1")
    script = os.path.join(run, "upgrade_keyring.sh")
    with open(script, "w", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
    os.chmod(script, 0o755)
    proc = subprocess.run(["dbus-run-session", "--", script], capture_output=True, text=True, timeout=90, start_new_session=True)
    return parse_lookups(proc.stdout)


def shell_q(s):
    return "'" + s.replace("'", "'\\''") + "'"


def parse_lookups(out):
    """the KEY:/END framing of keyring()'s output -> {key: value}; an empty value is None (not found)"""
    res = {}
    for m in re.finditer(r"KEY:(\S+)\n(.*?)\nEND", out, re.S):
        v = m.group(2).rstrip("\n")
        res[m.group(1)] = v or None
    return res


def secrets_equal(key, a, b):
    if a is None or b is None:
        return a == b
    if key == "AgentProviderSecrets":
        try:
            return json.loads(a) == json.loads(b)
        except ValueError:
            return False
    return a == b


# ---------------------------------------------------------------- the case
@case("upgrade_from_v0_1_17", modes=("terminal",), order=91, gui=True)
def c_upgrade_from_v0_1_17(ctx, r):
    """a v0.1.17-shaped profile (owner-like settings, a custom keybinding and theme, a Claude plugin registered from a remote source, keyring items, a
    SQLite state when the real v0.1.17 build is available) is upgraded by the current build: launch 1 reaches a prompt and takes exact typing, the app log has no
    panic / migration / settings / plugin / keyring ERROR, and every row of upgrade_table.py holds (docs/testing.md 2.18); launch 2 (with the removed env
    switches set) changes nothing (idempotent); then v0.1.17 is opened on the migrated profile and what it did is RECORDED (downgrade note, never a failure)."""
    if ctx.app is None or not hasattr(ctx.app, "stop") or not hasattr(ctx.app, "keyring"):
        r.skip("needs the Linux App launcher (seed paths, keyring helper and launcher are the Linux ones; macOS and Windows: the table and fixture are platform-neutral, the seeding is not ported)")
    if ctx.be.name == "mac" or ctx.win or C.plat().startswith(("macos", "windows")):
        r.skip("Linux first: macOS and Windows need their own seed paths (~/.phosphor, %%LOCALAPPDATA%%) and keyring helper")
    bash = shutil.which("bash")
    if not bash:
        r.skip("no bash on PATH: the fixture's session.startup_shell_override needs one")
    prev_bin, why = prev_release.locate()
    r.metric("old_profile_source", ("real v0.1.17 build (%s)" % why) if prev_bin else ("fixture only (%s)" % why))
    if not os.path.isdir(U.FIXTURE_DIR):
        r.skip("fixture directory %s is missing" % U.FIXTURE_DIR)
    home = ctx.prof.home
    p = paths(home)
    new_app = ctx.app
    backup = os.path.join(ctx.run, "upgrade_backup")
    os.makedirs(backup, exist_ok=True)
    saved = {}
    for name in ("settings", "keybindings"):
        if os.path.isfile(p[name]):
            saved[name] = read(p[name])
    claude_bak = p["claude"] + ".upgrade_bak"
    had_claude = os.path.isdir(p["claude"])
    old_env = {k: os.environ.get(k) for k in REMOVED_ENV}
    try:
        ctx.snapshot_log()
        new_app.stop()
        for sub in CB.STATE_SUBDIRS:
            shutil.rmtree(os.path.join(home, sub), ignore_errors=True)
        if had_claude:
            shutil.rmtree(claude_bak, ignore_errors=True)
            os.rename(p["claude"], claude_bak)
        for f in (p["keybindings"],):
            if os.path.isfile(f):
                os.remove(f)
        seed_text = seed_files(home, bash)
        # keyring items the v0.1.17 build would have written (secret-tool; skipped with a note where it is missing)
        stored = keyring(ctx, [("store", k, v) for k, v in SECRETS.items()])
        have_keyring = stored is not None
        if not have_keyring:
            r.note("NOT VERIFIED here: the keyring rows U22/U23 (secret-tool / dbus-run-session / gnome-keyring-daemon missing on this runner)")
        # ---- the old build writes its own state on the seeded profile
        old_app = None
        pre_text = seed_text
        if prev_bin:
            old_app = CB.old_app_like(new_app, prev_bin)
            ctx.app = old_app
            _n, up = R.start_app(ctx, r, "uf_old", restored_min=1)
            if not up:
                r.check(False, "the v0.1.17 build did not reach a prompt on the seeded profile: the old profile is not usable (%s)" % why)
                return
            time.sleep(4.0)                                     # the database writer and the settings watcher run at least once
            ctx.app.stop()
            ctx.app = new_app
            pre_text = open(p["settings"], encoding="utf-8", errors="replace").read()
            dropped = [("%s.%s" % k) for k in sorted(U.seeded_keys()) if U.norm_value(T.get_key(pre_text, *k)) is None]
            r.metric("seed_keys_dropped_by_v0_1_17_itself", dropped)
            if dropped:
                r.note("v0.1.17 itself did not keep %s: those rows are judged against what it left" % dropped)
        pre_cfg = {**tree_hashes(p["cfg"]), **{"claude/" + k: v for k, v in tree_hashes(p["claude"]).items()}}
        pre_files = {f: hashlib.sha256(read(f)).hexdigest() for f in [p["keybindings"], p["theme"]] + p["claude_files"] if os.path.isfile(f)}
        pre_data = tree_hashes(p["data"])
        db_before = CB.state_files(ctx)
        mig_old = {k: CB.db_migrations(k) for k in db_before}
        r.metric("old_db_files", {os.path.basename(k): v for k, v in db_before.items()})
        pre_secrets = {}
        if have_keyring:
            pre_secrets = keyring(ctx, [("lookup", k) for k in SECRETS]) or {}
            r.check(all(secrets_equal(k, pre_secrets.get(k), SECRETS[k]) for k in SECRETS), "the seeded keyring items could not be read back before the upgrade: %r" % {k: bool(v) for k, v in pre_secrets.items()})
        # ---- launch 1: the current build
        n1, up = R.start_app(ctx, r, "uf_new1", restored_min=1)
        r.check(up, "the new build did not reach a shell on the v0.1.17 profile")
        if not up:
            return
        time.sleep(2)
        ctx.be.key("Escape")
        ctx.shot("uf_new1_started")
        UI = CB.U
        UI.exact_burst(ctx, r, "uf_new1", 4)
        log1 = open(ctx.log_path, errors="replace").read() if ctx.log_path and os.path.exists(ctx.log_path) else ""
        probs = upgrade_log_problems(log1)
        r.check(not probs, "launch 1: the app log shows a failed upgrade: %s" % probs[:3])
        r.check(ctx.app.alive(), "launch 1: the new build died on the v0.1.17 profile")
        R.keep_log(ctx, "uf_new1")
        ctx.app.stop()
        post1 = open(p["settings"], encoding="utf-8", errors="replace").read()
        for rid, msg in U.after_problems(pre_text, post1, T):
            r.check(False, "%s: %s" % (rid, msg))
        _check_files(r, p, pre_cfg, pre_files, "launch 1")
        _check_db(r, ctx, db_before, mig_old)
        r.check(not lost_files(pre_data, tree_hashes(p["data"])), "launch 1: files the v0.1.17 build left in the data directory are gone: %s" % lost_files(pre_data, tree_hashes(p["data"]))[:5])
        if have_keyring:
            got1 = keyring(ctx, [("lookup", k) for k in SECRETS]) or {}
            for k in SECRETS:
                r.check(secrets_equal(k, pre_secrets.get(k), got1.get(k)), "U22/U23: keyring item %s changed or vanished after launch 1" % k)
        # ---- launch 2: idempotent, with the removed environment switches set
        for k, v in REMOVED_ENV.items():
            os.environ[k] = v
        snap1 = {"settings": read(p["settings"]), "cfg": tree_hashes(p["cfg"]), "claude": tree_hashes(p["claude"])}
        n2, up2 = R.start_app(ctx, r, "uf_new2", restored_min=1)
        r.check(up2, "launch 2 (removed env switches set): the new build did not reach a shell")
        if up2:
            time.sleep(2)
            ctx.be.key("Escape")
            UI.exact_burst(ctx, r, "uf_new2", 4)
            log2 = open(ctx.log_path, errors="replace").read() if ctx.log_path and os.path.exists(ctx.log_path) else ""
            probs2 = upgrade_log_problems(log2)
            r.check(not probs2, "U70-U72 / launch 2: the app log shows a failure on the second launch: %s" % probs2[:3])
            R.keep_log(ctx, "uf_new2")
            ctx.app.stop()
            snap2 = {"settings": read(p["settings"]), "cfg": tree_hashes(p["cfg"]), "claude": tree_hashes(p["claude"])}
            r.check(snap1["settings"] == snap2["settings"], "launch 2 is not idempotent: settings.toml changed between launch 1 and launch 2 (first difference near byte %d)" % first_diff(snap1["settings"], snap2["settings"]))
            r.check(snap1["cfg"] == snap2["cfg"] and snap1["claude"] == snap2["claude"], "launch 2 is not idempotent: files changed under the config / .claude directories: %s" % sorted(
                k for k in set(snap1["cfg"]) | set(snap2["cfg"]) if snap1["cfg"].get(k) != snap2["cfg"].get(k))[:5])
            if have_keyring:
                got2 = keyring(ctx, [("lookup", k) for k in SECRETS]) or {}
                for k in SECRETS:
                    r.check(secrets_equal(k, pre_secrets.get(k), got2.get(k)), "U22/U23: keyring item %s changed or vanished after launch 2" % k)
        for k, v in old_env.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        # ---- downgrade note: v0.1.17 on the migrated profile (recorded, never a failure)
        if prev_bin and up2:
            migrated = open(p["settings"], encoding="utf-8", errors="replace").read()
            ctx.app = CB.old_app_like(new_app, prev_bin)
            _n, reached = R.start_app(ctx, r, "uf_down", restored_min=1, timeout=60)
            time.sleep(2)
            log_d = open(ctx.log_path, errors="replace").read() if ctx.log_path and os.path.exists(ctx.log_path) else ""
            ctx.app.stop()
            ctx.app = new_app
            after_down = open(p["settings"], encoding="utf-8", errors="replace").read()
            rep = downgrade_report(migrated, after_down, reached, log_d)
            r.metric("downgrade_v0_1_17_on_migrated_profile", rep)
            r.note("DOWNGRADE (recorded): v0.1.17 on the migrated profile %s; seeded keys it lost: %s; problem lines: %d" % (
                "reached a prompt" if rep["reached_prompt"] else "did NOT reach a prompt", rep["seeded_keys_lost"] or "none", len(rep["problem_lines"])))
        elif not prev_bin:
            r.note("downgrade note not recorded: no real v0.1.17 build (%s)" % why)
    finally:
        for k, v in old_env.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        ctx.app = new_app
        try:
            new_app.stop()
            for sub in CB.STATE_SUBDIRS:
                shutil.rmtree(os.path.join(home, sub), ignore_errors=True)
            for f in (p["keybindings"],):
                if os.path.isfile(f):
                    os.remove(f)
            shutil.rmtree(p["claude"], ignore_errors=True)
            if had_claude and os.path.isdir(claude_bak):
                os.rename(claude_bak, p["claude"])
            for name, data in saved.items():
                with open(p[name], "wb") as f:
                    f.write(data)
            if "settings" not in saved and os.path.isfile(p["settings"]):
                os.remove(p["settings"])
            R.start_app(ctx, r, "uf_end", restored_min=1)
        except Exception as e:                                  # never mask the case's own verdict
            r.check(False, "could not leave a fresh app on the harness profile: %r" % e)


def first_diff(a, b):
    for i, (x, y) in enumerate(zip(a, b)):
        if x != y:
            return i
    return min(len(a), len(b))


def _check_files(r, p, pre_cfg, pre_files, label):
    """U81 / U82 / U83: whole files, byte for byte as the old build left them (the fixture's bytes when there was no old build), and nothing in the config dir lost"""
    names = {p["keybindings"]: "U81 keybindings.yaml", p["theme"]: "U82 user theme"}
    names.update({fp: "U83 %s (a plugin registration is only moved by a user action, never at launch)" % os.path.basename(fp) for fp in p["claude_files"]})
    for path, name in names.items():
        now = hashlib.sha256(read(path)).hexdigest() if os.path.isfile(path) else None
        r.check(now is not None and now == pre_files.get(path), "%s: %s after %s" % (name, "missing" if now is None else "changed", label))
    cfg_now = tree_hashes(p["cfg"])
    gone = [k for k in pre_cfg if not k.startswith("claude/") and k not in cfg_now]
    r.check(not gone, "%s: files the v0.1.17 build left in the config directory are gone: %s" % (label, gone[:5]))


def _check_db(r, ctx, before, mig_old):
    """U80: the same rule as upgrade_over_previous (files kept, not moved aside, not shrunk, applied migrations never fewer)"""
    after = CB.state_files(ctx)
    gone = [os.path.basename(k) for k in before if k not in after]
    aside = [os.path.basename(k) for k in after if ".corrupt-" in k]
    shrunk = [os.path.basename(k) for k in before if k in after and after[k] < before[k] // 2 and before[k] > 65536]
    r.check(not gone, "U80: database file(s) of the previous build vanished: %s" % gone)
    r.check(not aside, "U80: the previous build's database was moved aside as corrupt: %s" % aside)
    r.check(not shrunk, "U80: database file(s) shrank to under half: %s" % shrunk)
    mig_new = {k: CB.db_migrations(k) for k in after}
    r.metric("migrations_before_after", {os.path.basename(k): [mig_old.get(k), mig_new.get(k)] for k in after})
    worse = [os.path.basename(k) for k in after if mig_old.get(k) is not None and mig_new.get(k) is not None and mig_new[k] < mig_old[k]]
    r.check(not worse, "U80: fewer applied migrations after the new build ran than the old build left: %s" % worse)
