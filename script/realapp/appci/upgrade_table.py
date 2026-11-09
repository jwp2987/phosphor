# appci/upgrade_table.py: the v0.1.17 -> current upgrade table (#1546): EVERY setting / state key that was added, renamed, removed, or whose default or
# meaning changed between the tag v0.1.17 and merge/w0, and what an upgrade must do with it. One list, three users:
#   - cases_upgrade.py (`upgrade_from_v0_1_17`) seeds the profile from fixtures/upgrade_v0_1_17/ and asserts each row after the first and second launch,
#   - docs/testing.md 2.18 prints the table (`python3 upgrade_table.py --markdown`),
#   - selftest `upgrade_table_matches_fixture_and_docs` fails when a row, the fixture and the docs drift apart.
# Pure Python, no I/O at import (selftest-able). The "now" column is the behaviour of merge/w0 READ FROM THE SOURCE (file:line in `src`), not measured:
# the case measures the file effects, the rest is verified on the box (docs/testing.md 2.18 "Verified only on the box").
import os, re

HERE = os.path.dirname(os.path.abspath(__file__))
FIXTURE_DIR = os.path.join(HERE, "fixtures", "upgrade_v0_1_17")

# action vocabulary (the owner's four, plus the two ways a new key can be handled)
KEEP = "keep"                      # the value (or file) survives byte for byte
MIGRATE = "migrate"                # rewritten to another key / value by the app
DROP_SILENTLY = "drop silently"    # the app stops reading it; the line stays in the file (unknown keys are preserved, #1545), no notice
DROP_NOTICE = "drop with a notice" # the app removes it and tells the user
NEW = "new key"                    # absent in a v0.1.17 profile; the upgrade must not invent a value

# check kinds (what the case asserts about the row)
C_KEPT = "kept"                    # TOML key present after launch 1 AND 2 with the value the v0.1.17 build left
C_UNKNOWN_KEPT = "unknown_kept"    # the key is unknown to the new build: the line is still in the file, unchanged
C_ABSENT = "absent_or_default"     # the key is not written by the upgrade (absent, or equal to its default)
C_FILE_SAME = "file_same"          # a whole file is byte-identical after launch 1 and 2
C_SECRET = "secret_same"           # the keyring item is unchanged
C_ENV = "env_inert"                # a removed environment switch is set for launch 2: the app starts and logs no failure
C_DB = "db_migrations"             # the SQLite files are kept and the applied migrations never go down
C_UNIT = "unit_only"               # nothing in a seeded file can show it (an UNSET key whose default moved): pinned by a Rust unit test, named in `src`


def row(rid, area, key, kind, v117, now, action, check, src, seed=None, default=None, note=""):
    return dict(id=rid, area=area, key=key, kind=kind, v117=v117, now=now, action=action, check=check, src=src, seed=seed, default=default, note=note)


# `seed` = (toml_table, key) of the line in fixtures/upgrade_v0_1_17/settings.toml that carries the row (None: nothing is seeded for it).
# `default` = the TOML literal of the new default for C_ABSENT rows (a list: one literal per key of the row).
ROWS = [
    # ---- owner-shape keys that did NOT change: the "no settings reset" half. Each must come through byte for byte.
    row("U01", "general", "general.default_session_mode", "unchanged", '"agent"', "same", KEEP, C_KEPT, "app/src/settings/ai.rs:315", ("general", "default_session_mode"),
        note="agent mode stays the default after the upgrade"),
    row("U02", "agent input", "agents.warp_agent.input.ai_auto_detection_enabled / nld_in_terminal_enabled", "unchanged", "true / true", "same", KEEP, C_KEPT,
        "app/src/settings/ai.rs:1920,1940", [("agents.warp_agent.input", "ai_auto_detection_enabled"), ("agents.warp_agent.input", "nld_in_terminal_enabled")], note="autodetect + natural-language detection stay on"),
    row("U03", "agent providers", "agents.warp_agent.providers", "unchanged shape; now also feeds the network gate", "two inline-table providers (local model, local Claude proxy)",
        "same list; each provider's base_url is a configured origin of the gate", KEEP, C_KEPT, "app/src/settings/ai.rs:2669, app/src/settings/init.rs:apply_network_gate_configured_origins",
        ("agents.warp_agent", "providers"), note="the providers are loopback, so the gate admits them without network.allowed_origins; a non-loopback provider is admitted because it is configured"),
    row("U04", "agent providers", "agents.byop.last_used_model_id", "unchanged", '"byop:upg-ollama:probe-model"', "same", KEEP, C_KEPT, "app/src/settings/ai.rs", ("agents.byop", "last_used_model_id")),
    row("U05", "appearance", "appearance.vertical_tabs.enabled", "unchanged", "true", "same", KEEP, C_KEPT, "app/src/workspace/tab_settings.rs:549", ("appearance.vertical_tabs", "enabled")),
    row("U06", "appearance", "appearance.themes.theme / appearance.text.font_size", "unchanged", '"tokyo_night" / 14.0', "same", KEEP, C_KEPT,
        "app/src/settings/theme.rs:32, app/src/settings/font.rs:56", [("appearance.themes", "theme"), ("appearance.text", "font_size")]),
    row("U07", "shell", "session.startup_shell_override", "unchanged", '"<bash>"', "same", KEEP, C_KEPT, "app/src/terminal/session_settings.rs:287", ("session", "startup_shell_override")),
    row("U08", "privacy", "privacy.custom_secret_regex_list", "unchanged", "two patterns (one named)", "same", KEEP, C_KEPT, "app/src/settings/privacy.rs:127", ("privacy", "custom_secret_regex_list")),
    row("U09", "privacy", "privacy.secret_redaction.enabled", "unchanged", "false", "same", KEEP, C_KEPT, "app/src/terminal/safe_mode_settings.rs:80", ("privacy.secret_redaction", "enabled"),
        note="redaction patterns set but redaction OFF stays OFF; an upgrade must never switch it on"),
    row("U10", "privacy", "privacy.telemetry_enabled", "unchanged", "false", "same", KEEP, C_KEPT, "app/src/settings/privacy.rs:106", ("privacy", "telemetry_enabled")),
    row("U11", "general", "general.restore_session", "unchanged", "false", "same", KEEP, C_KEPT, "app/src/terminal/general_settings.rs:37", ("general", "restore_session")),
    # ---- network gate (Wave N)
    row("U20", "network", "network.proxy_mode / proxy_url / proxy_username / proxy_no_proxy", "meaning changed for system and off",
        '"custom" + http://127.0.0.1:3128 + user + no_proxy list (the default "system" followed OS/env proxies)',
        "custom is used as before; system and off now both mean NO proxy (the gate ignores system and environment proxies)", KEEP, C_KEPT,
        "crates/http_client/src/proxy.rs:set_global_proxy_config, crates/network_gate/src/lib.rs", [("network", "proxy_mode"), ("network", "proxy_url"), ("network", "proxy_username"), ("network", "proxy_no_proxy")],
        note="values are kept; only a user whose proxy_mode is system/unset and who needs the OS proxy sees a change (their provider calls go direct). Seeded: custom, which the gate uses"),
    row("U21", "network", "network.allowed_origins", NEW, "(absent)", 'default "" (loopback + configured AI endpoints always allowed)', NEW, C_ABSENT, "app/src/settings/network.rs:139", None, '""'),
    row("U22", "keyring", "ProxyPassword (keyring item)", "unchanged key; read moved off the UI thread (#1805)", "stored in the OS keyring", "same item, read on the secure-storage worker", KEEP, C_SECRET,
        "app/src/settings/network_secrets.rs", note="the case seeds it with secret-tool and reads it back after both launches; needs secret-tool on the runner"),
    row("U23", "keyring", "AgentProviderSecrets (keyring item: per-provider API keys, JSON)", "unchanged", "stored in the OS keyring", "same item", KEEP, C_SECRET,
        "app/src/ai/agent_providers/secrets.rs:16", note="made-up key values only"),
    # ---- models.dev
    row("U30", "agent providers", "agents.warp_agent.models_dev_fetch", "default changed true -> false", "true (explicit in the seed)",
        "explicit true is honoured (auto-download when the Providers page opens); UNSET is now false (built-in catalog, Refresh button only)", KEEP, C_KEPT,
        "app/src/settings/ai.rs:2677", ("agents.warp_agent", "models_dev_fetch"),
        note="the unset-key default is pinned by the unit test models_dev_fetch_defaults_off_and_a_v0_1_17_provider_list_still_parses"),
    row("U31", "agent providers", "agents.warp_agent.models_dev_url", "meaning changed (a configured origin of the gate)", "a mirror URL (loopback)", "same URL, added to the gate's configured origins", KEEP, C_KEPT,
        "app/src/settings/ai.rs:2690, app/src/settings/init.rs", ("agents.warp_agent", "models_dev_url")),
    # ---- ssh / remote helper
    row("U40", "ssh", "warpify.ssh.use_ssh_tmux_wrapper", "REMOVED (the tmux option is deleted, DECLINED.md)", "true (explicit)", "no setting by that name; the key is unknown", DROP_SILENTLY, C_UNKNOWN_KEPT,
        "app/src/terminal/warpify/settings.rs (UseSshTmuxWrapper deleted)", ("warpify.ssh", "use_ssh_tmux_wrapper"),
        note="no migration and no notice: the line stays in settings.toml like any unknown key (the #1545 legacy-key case) and the ssh session takes the primary (no-helper) path"),
    row("U41", "ssh", "warpify.ssh.ssh_extension_install_mode", "meaning changed (the app-wide default for a host with no own decision)", '"always_install" (what "don\'t ask again" + Install wrote in v0.1.17)',
        "same value; a host in neither list still follows it, so the old global choice keeps applying to every host", KEEP, C_KEPT,
        "app/src/terminal/warpify/settings.rs:142, crates/remote_server/src/policy.rs:decide", ("warpify.ssh", "ssh_extension_install_mode"),
        note="the new per-host lists are NOT filled from it (no host name exists in a v0.1.17 profile). The tick now writes remote_helper_allowed_hosts / _denied_hosts for the one host and leaves this mode alone"),
    row("U42", "ssh", "warpify.ssh.remote_helper_allowed_hosts", NEW, "(absent)", "default []", NEW, C_ABSENT, "app/src/terminal/warpify/settings.rs:156", None, "[]"),
    row("U43", "ssh", "warpify.ssh.remote_helper_denied_hosts", NEW, "(absent)", "default []", NEW, C_ABSENT, "app/src/terminal/warpify/settings.rs:166", None, "[]"),
    row("U44", "ssh", "terminal.remote.show_no_helper_chip / no_helper_chip_dismissed_hosts", NEW, "(absent)", "default true / []", NEW, C_ABSENT,
        "app/src/settings/input.rs (show_no_helper_chip)", None, ["true", "[]"]),
    # ---- input / profile
    row("U50", "input", "terminal.input.completions_open_while_typing", "default changed false -> true (local sessions only unless set)", "false (explicit in the seed)",
        "explicit false wins everywhere; UNSET is now on for local sessions, off for remote", KEEP, C_KEPT, "app/src/settings/input.rs:55,completions_while_typing_effective", ("terminal.input", "completions_open_while_typing"),
        note="an unset v0.1.17 profile starts completing while typing in local panes: a deliberate default change (unit tests default_is_on_for_local_only, explicit_setting_wins_everywhere)"),
    row("U51", "profile", "appearance.profile.display_name / appearance.profile.avatar", NEW, "(absent)", 'default "" / ""', NEW, C_ABSENT, "app/src/settings/input.rs", None, '""'),
    # ---- agent approvals
    row("U60", "agent permissions", "agents.warp_agent.permissions.safe_command_allowlist_enabled / safe_command_allowlist", NEW, "(absent)",
        "default false / the built-in read-only list (no git)", NEW, C_ABSENT, "app/src/settings/ai.rs:2558", None, "false", note="the allowlist must never be switched on by an upgrade"),
    # ---- removed environment switches
    row("U70", "env switch", "PHOSPHOR_BOOTSTRAP_TIMED", "REMOVED (the events-not-timers rework, #1354/#1815)", "=1 selected the old fixed-delay ssh bootstrap", "ignored", DROP_SILENTLY, C_ENV,
        "app/src/terminal/view.rs (switch deleted)", note="inert only as far as 'the app starts and logs no failure' can show"),
    row("U71", "env switch", "PHOSPHOR_SSH_BOOTSTRAP_GIVEUP_DELIVERS", "REMOVED (#1815)", "=1 delivered typed text after a give-up", "ignored (a give-up drops the keys with a count toast, #1661)", DROP_SILENTLY, C_ENV,
        "app/src/terminal/view.rs:25120"),
    row("U72", "env switch", "PHOSPHOR_SUBSHELL_READY", "never in a release (added and removed between the tags)", "(not in v0.1.17)", "ignored", DROP_SILENTLY, C_ENV, "docs/testing.md 2.6",
        note="listed because the owner asked; v0.1.17 never read it"),
    # ---- state
    row("U80", "state", "warp.sqlite (and every other SQLite file in the data/state dirs)", "schema migrations run at open", "the v0.1.17 schema", "same file, migrations applied in place", KEEP, C_DB,
        "app/src/persistence/sqlite.rs (setup_database_with_deadline, #1772)", note="not moved aside as corrupt, not shrunk under half, applied migrations never fewer (reuses upgrade_over_previous' checks)"),
    row("U81", "state", "keybindings.yaml", "unchanged", "one custom binding", "same file", KEEP, C_FILE_SAME, "app/src/keyboard.rs:35,161"),
    row("U82", "state", "themes/<user theme>.yaml", "unchanged", "one user theme", "same file", KEEP, C_FILE_SAME, "crates/warp_core/src/paths.rs:256"),
    row("U83", "state", "~/.claude/plugins/known_marketplaces.json + installed_plugins.json (Claude Code plugin registered from Warp's GitHub)", "bundled plugin replaces remote registrations (Wave N)",
        "marketplace registered from a remote repository", "NOT touched at launch: the app offers a 'move to the bundled copy' action (needs_source_migration) and never fetches the plugin", KEEP, C_FILE_SAME,
        "app/src/terminal/cli_agent_sessions/plugin_manager/claude.rs:349,460", note="two files; the case also checks the launch made no network connection to the old source only through zero_network_startup (separate case)"),
]

assert len({r["id"] for r in ROWS}) == len(ROWS)


def seeds_of(r):
    """the fixture lines a row carries, as a list of (table, key)"""
    s = r["seed"]
    if not s:
        return []
    return [s] if isinstance(s, tuple) else list(s)


def seeded_keys():
    """{(table, key)} of every fixture line a row accounts for"""
    return {k for r in ROWS for k in seeds_of(r)}


# ---------------------------------------------------------------- TOML helpers (textual: the owner-shaped files use multi-line inline tables that tomllib rejects)
def norm_literal(text):
    """a TOML value as text, compared across rewrites: comments cut, whitespace collapsed, trailing commas inside brackets dropped, single quotes -> double"""
    if text is None:
        return None
    out = []
    for line in str(text).splitlines():
        out.append(re.sub(r"\s+#.*$", "", line).strip())
    t = " ".join(x for x in out if x)
    t = re.sub(r",\s*([\]}])", r"\1", t)
    t = re.sub(r"\s+", " ", t)
    t = re.sub(r"([\[{])\s+", r"\1", t)
    t = re.sub(r"\s+([\]}])", r"\1", t)
    return t.replace("'", '"').strip()


def fixture_keys(text):
    """[(table, key)] of every `key = value` line of a settings text (multi-line values count once, by their first line)"""
    out, table, depth = [], "", 0
    for line in text.splitlines():
        s = line.strip()
        if depth <= 0:
            m = re.match(r"^\[([^\[\]]+)\]\s*(#.*)?$", s)
            if m:
                table = m.group(1).replace(" ", "")
                continue
            m = re.match(r"^([A-Za-z0-9_\"-]+)\s*=\s*(.*)$", s)
            if m:
                out.append((table, m.group(1).strip('"')))
                rest = m.group(2)
            else:
                continue
        else:
            rest = s
        rest = re.sub(r'"(?:[^"\\]|\\.)*"', '""', rest)
        depth += rest.count("[") + rest.count("{") - rest.count("]") - rest.count("}")
    return out


def fixture_problems(text):
    """-> list of drift messages between the fixture's keys and the rows' seeds"""
    have = set(fixture_keys(text))
    want = seeded_keys()
    out = ["fixture has %s.%s which no table row accounts for" % k for k in sorted(have - want)]
    out += ["table row seeds %s.%s which the fixture does not contain" % k for k in sorted(want - have)]
    return out


# ---------------------------------------------------------------- verdicts (pure: the case feeds them text)
def after_problems(pre, post, T):
    """-> [(row id, message)] for the TOML rows, comparing the settings text the v0.1.17 build left (`pre`) with the text after the new build ran (`post`).
    `T` is toml_text (get_key). A row with several keys is judged on its first (the `seed` key) and the extra keys of the same table that the fixture carries."""
    out = []
    for r in ROWS:
        if r["check"] == C_KEPT or r["check"] == C_UNKNOWN_KEPT:
            for table, key in seeds_of(r):
                a, b = norm_value(T.get_key(pre, table, key)), norm_value(T.get_key(post, table, key))
                if a is None:
                    out.append((r["id"], "the seed line %s.%s is missing from the file the old build left (the old build dropped it?)" % (table, key)))
                elif b is None:
                    out.append((r["id"], "%s.%s was removed from settings.toml by the upgrade" % (table, key)))
                elif norm_literal(a) != norm_literal(b):
                    out.append((r["id"], "%s.%s changed: %r -> %r" % (table, key, norm_literal(a)[:80], norm_literal(b)[:80])))
        elif r["check"] == C_ABSENT:
            defaults = r["default"] if isinstance(r["default"], list) else [r["default"]] * len(_absent_keys(r))
            for (table, key), dflt in zip(_absent_keys(r), defaults):
                v = norm_value(T.get_key(post, table, key))
                if v is not None and norm_literal(v) != norm_literal(dflt):
                    out.append((r["id"], "%s.%s was written by the upgrade with a non-default value %r" % (table, key, norm_literal(v)[:80])))
    return out


def norm_value(raw):
    """toml_text.get_key returns `key = value` text (or None): the value part"""
    if raw is None:
        return None
    return raw.split("=", 1)[1].strip() if "=" in raw else raw.strip()


def _absent_keys(r):
    """[(table, key)] the C_ABSENT row watches: its dotted key(s), 'a.b.key / c' meaning the last segment of the first path and a sibling name"""
    out = []
    first = r["key"].split(" / ")[0].strip()
    table, _, key = first.rpartition(".")
    out.append((table, key))
    for sib in r["key"].split(" / ")[1:]:
        sib = sib.strip()
        if "." in sib:
            t2, _, k2 = sib.rpartition(".")
            out.append((t2, k2))
        else:
            out.append((table, sib))
    return out


# ---------------------------------------------------------------- docs
def markdown():
    lines = ["| id | area | key | change | v0.1.17 | now | upgrade must | check |", "|---|---|---|---|---|---|---|---|"]
    for r in ROWS:
        cell = lambda s: str(s).replace("|", "\\|").replace("\n", " ")
        lines.append("| %s | %s | `%s` | %s | %s | %s | **%s** | %s |" % (r["id"], cell(r["area"]), cell(r["key"]), cell(r["kind"]), cell(r["v117"]), cell(r["now"]), r["action"], r["check"]))
    return "\n".join(lines)


def doc_problems(doc_text):
    """-> messages when docs/testing.md's table does not carry every row id with its key"""
    out = []
    for r in ROWS:
        if not re.search(r"\|\s*%s\s*\|" % r["id"], doc_text):
            out.append("docs/testing.md has no table row %s (%s)" % (r["id"], r["key"]))
        elif "`%s`" % r["key"] not in doc_text:
            out.append("docs/testing.md row %s does not name `%s`" % (r["id"], r["key"]))
    return out


if __name__ == "__main__":
    import sys
    if "--markdown" in sys.argv:
        print(markdown())
    else:
        print("%d rows, %d seeded keys" % (len(ROWS), len(seeded_keys())))
