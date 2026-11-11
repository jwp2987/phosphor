# appci/selftest_r16_settings.py: offline self-tests of the test settings model (round 16: case_settings.py, cases.requires, run.py's wiring, the
# tables of docs/testing.md). Pure Python: no app, display or network. The seed files these checks build are synthetic (written into a temp
# directory by the checks themselves): no value of any real owner seed is in this file.
# selftest.py appends CHECKS to its own list, so `run.py --self-test` and `selftest.py` both run these.
import os, shutil, sys, tempfile, types

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, HERE)
import cases  # noqa: E402
import case_settings as CS  # noqa: E402
import profile  # noqa: E402
import toml_text as T  # noqa: E402

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


# a seed in the SHAPE the app writes (tables, a multi-line inline list, a comment), with made-up values
SEED = '''[appearance.vertical_tabs]
enabled = true

[agents.warp_agent]
providers = [
  {
    id = "x",
    models = [ { name = "m", id = "m" } ],
  },
]

[agents.warp_agent.input]
ai_auto_detection_enabled = false
nld_in_terminal_enabled = false

[text_editing]
vim_mode_enabled = %(vim)s

[general]
default_session_mode = "terminal"
restore_session = true
# a comment the harness keeps
'''


class _Env:
    """a temp run directory and APPCI_OWNER_SETTINGS / APPCI_SSH_WRAPPER as given, put back on exit"""
    def __init__(self, seed_text=None):
        self.seed_text = seed_text

    def __enter__(self):
        self.d = tempfile.mkdtemp(prefix="r16s")
        self.old = {k: os.environ.get(k) for k in ("APPCI_OWNER_SETTINGS", "APPCI_SSH_WRAPPER", "APPCI_FAKE_EMBEDDINGS_URL")}
        self.note = profile.Profile.owner_note
        for k in ("APPCI_SSH_WRAPPER", "APPCI_FAKE_EMBEDDINGS_URL"):
            os.environ.pop(k, None)
        seed = os.path.join(self.d, "no-such-seed.toml")
        if self.seed_text is not None:
            seed = os.path.join(self.d, "seed.toml")
            with open(seed, "w", encoding="utf-8") as f:
                f.write(self.seed_text)
        os.environ["APPCI_OWNER_SETTINGS"] = seed
        return self

    def __exit__(self, *a):
        for k, v in self.old.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        profile.Profile.owner_note = self.note
        shutil.rmtree(self.d, ignore_errors=True)

    def ctx(self, mode, ollama=None):
        """a profile built as run.py builds it (win False: the POSIX layout on every OS this self-test runs on) and the pieces of Ctx requires() reads"""
        prof = profile.Profile(os.path.join(self.d, "run_" + mode))
        prof.win, prof.pane = False, None
        prof.build(mode, ollama)
        return types.SimpleNamespace(prof=prof, app=None, win=False, mode=mode)


def _settings(ctx):
    return os.path.join(ctx.prof.home, ".config", "phosphor", "settings.toml")


def _raises(f, exc):
    try:
        f()
    except exc as e:
        return e
    raise AssertionError("expected %s" % exc.__name__)


@check
def r16_requires_matching_and_mismatching_profile():
    """requires() on a profile the harness built: a matching profile returns, a mismatching one is UNVERIFIABLE with 'SETTINGS: <name>=<actual>, case
    needs <expected>' (run.py: status noref, never pass, never fail for that reason), a live rewrite by the case is what the next call reads"""
    import run
    with _Env() as env:                                    # bare profile (no seed)
        for mode in ("terminal", "agent"):
            ctx = env.ctx(mode)
            r = cases.Result("t", mode)
            assert cases.requires(ctx, r, session_mode=CS.PASS_MODE, restore_session=False, vim_mode=False, vertical_tabs=False, copy_on_select=True,
                                  ssh_extension_install="never_install", keybindings_file="absent", font_size=CS.APP_DEFAULT, startup_shell=CS.ABSENT) is True
            assert cases.requires_declared(ctx, r, "fast_typing")["session_mode"] == CS.PASS_MODE and not r.noref and not r.fails
            # r16-declare: a lab arm is checked by its family's declaration, and its helper arm's install mode is NOT part of the check (the lab
            # harness writes it after this point): an `upload` arm passes on a profile that still says never_install
            for arm in ("lab_tab_remote_dir.bash.lan.nohelper", "lab_helper_install.bash.lan.upload"):
                if mode in next(c["modes"] for c in cases.CASES if c["name"] == arm):
                    got = cases.requires_declared(ctx, r, arm)
                    assert got["ssh_warpification"] is True and "ssh_extension_install" not in got and not r.noref
        # mismatch: the case needs the horizontal tab bar, the profile (after a live rewrite) has the panel
        T.edit_file(_settings(ctx), "appearance.vertical_tabs", "enabled", "true")
        e = _raises(lambda: cases.requires(ctx, r, vertical_tabs=False, vim_mode=False), cases.SettingsMismatch)
        assert str(e) == "SETTINGS: vertical_tabs=true, case needs false", str(e)
        assert isinstance(e, cases.Skip)                   # cleanup code that re-raises Skip handles it
        r = cases.Result("t", "agent")
        assert run.skip_status(r, e) == "noref" and not r.fails
        assert r.noref == ["UNVERIFIABLE: SETTINGS: vertical_tabs=true, case needs false"] and r.notes[0].startswith("NO-REFERENCE: UNVERIFIABLE: SETTINGS:")
        r = cases.Result("t", "agent")                     # a check that failed before stands (#1498): the mismatch never hides it and never causes it
        r.check(False, "earlier failure")
        assert run.skip_status(r, e) == "fail" and r.fails == ["earlier failure"]
        assert cases.requires(ctx, r, vertical_tabs=True)  # and the live value satisfies a case that needs it
        # r16-declare: a full-tier case and a lab arm are UNVERIFIABLE on a mismatch like any other (they were not checked at all before)
        T.edit_file(_settings(ctx), "terminal.input", "right_click_behavior", '"paste"')
        e = _raises(lambda: cases.requires_declared(types.SimpleNamespace(prof=ctx.prof, app=None, win=False, mode="terminal"), r, "ui_right_click_paste"), cases.SettingsMismatch)
        assert "right_click=paste, case needs context_menu" in str(e), str(e)
        T.edit_file(_settings(ctx), "agents.warp_agent.active_ai", "natural_language_autosuggestions_in_ssh_enabled", "true")
        e = _raises(lambda: cases.requires_declared(ctx, r, "lab_agent_nld_predict_ssh.bash.lan.nohelper"), cases.SettingsMismatch)
        assert str(e) == "SETTINGS: nld_predict_in_ssh=true, case needs false", str(e)
        T.edit_file(_settings(ctx), "terminal.input", "right_click_behavior", '"context_menu"')
        T.edit_file(_settings(ctx), "agents.warp_agent.active_ai", "natural_language_autosuggestions_in_ssh_enabled", "false")
        # two settings off: both named, sorted, one line
        T.edit_file(_settings(ctx), "terminal", "copy_on_select", "false")
        e = _raises(lambda: cases.requires(ctx, r, vertical_tabs=False, copy_on_select=True, session_mode="agent"), cases.SettingsMismatch)
        assert str(e) == "SETTINGS: copy_on_select=false, case needs true; vertical_tabs=true, case needs false", str(e)
        # a keybindings file beside the settings file
        open(os.path.join(os.path.dirname(_settings(ctx)), "keybindings.yaml"), "w").close()
        e = _raises(lambda: cases.requires(ctx, r, keybindings_file="absent"), cases.SettingsMismatch)
        assert str(e) == "SETTINGS: keybindings_file=present, case needs absent", str(e)
        # no readable file: nothing is known, so nothing is judged
        os.remove(_settings(ctx))
        e = _raises(lambda: cases.requires(ctx, r, vim_mode=False), cases.SettingsMismatch)
        assert "could not be read" in str(e) and "vim_mode" in str(e)


@check
def r16_harness_overrides_hold_on_a_seeded_profile():
    """the three layers: a seed that says the opposite of every forced key still gives a pass profile that meets the baseline (profile.py forces
    them), keeps what the harness does not touch (vertical tabs: the seed's), and a seed value the harness leaves alone that a case cannot run
    with (vim keys) makes the declared check UNVERIFIABLE for every declared case"""
    with _Env(SEED % {"vim": "false"}) as env:
        for mode in ("terminal", "agent"):
            ctx = env.ctx(mode, "http://127.0.0.1:9")
            assert profile.Profile.owner_note.startswith("sha256 ")
            text = open(_settings(ctx)).read()
            assert "# a comment the harness keeps" in text
            assert CS.mismatches(text, CS.baseline(mode), mode) == [], (mode, CS.mismatches(text, CS.baseline(mode), mode))
            eff = {n: CS.effective(text, n)[0] for n in ("session_mode", "restore_session", "ssh_extension_install", "vertical_tabs", "autodetect", "nld_in_terminal")}
            assert eff["session_mode"] == '"%s"' % mode and eff["restore_session"] == "false" and eff["ssh_extension_install"] == '"never_install"'
            assert eff["vertical_tabs"] == "true"                                   # not forced: the seed's value
            assert (eff["autodetect"], eff["nld_in_terminal"]) == (("true", "true") if mode == "agent" else ("false", "false"))      # forced in the agent pass only
            assert CS.effective(text, "providers")[0] == '"set"' and CS.effective(text, "subshell_commands")[0] == '"set"'
            assert "appci-ollama" in text                                           # the CI endpoint replaced the seed's provider list
            r = cases.Result("t", mode)
            assert cases.requires_declared(ctx, r, "tab_enter") and cases.requires_declared(ctx, r, "no_such_case") is None
            if mode == "terminal":       # the terminal pass keeps the seed's detection switches: a case that needs them on is not judged on this seed
                e = _raises(lambda: cases.requires_declared(ctx, r, "routing_plain_installed_head"), cases.SettingsMismatch)
                assert str(e) == "SETTINGS: autodetect=false, case needs true; nld_in_terminal=false, case needs true", str(e)
            else:
                assert cases.requires_declared(ctx, r, "routing_plain_installed_head")
        # the pass's mode is part of the baseline: a profile built for the other mode is a mismatch
        other = types.SimpleNamespace(prof=ctx.prof, app=None, win=False, mode="terminal")
        e = _raises(lambda: cases.requires_declared(other, cases.Result("t", "terminal"), "fast_typing"), cases.SettingsMismatch)
        assert str(e) == "SETTINGS: session_mode=agent, case needs terminal", str(e)
    with _Env(SEED % {"vim": "true"}) as env:
        ctx = env.ctx("agent")
        e = _raises(lambda: cases.requires_declared(ctx, cases.Result("t", "agent"), "fast_typing"), cases.SettingsMismatch)
        assert str(e) == "SETTINGS: vim_mode=true, case needs false", str(e)
        assert cases.requires_declared(ctx, cases.Result("t", "agent"), "first_run_defaults") is None      # settings-independent: not checked


@check
def r16_settings_reader_forms_and_values():
    """the reader sees a key in each form a settings file can write it; values compare as values; a typo or a wrong kind of requirement raises"""
    table = '[appearance.vertical_tabs]\nenabled = true\n'
    dotted = '[appearance]\nvertical_tabs.enabled = true\n'
    inline = '[appearance]\nvertical_tabs = { display_granularity = "panes", enabled = true }\n'
    root = 'appearance.vertical_tabs.enabled = true\n'
    for text in (table, dotted, inline, root):
        assert CS.effective(text, "vertical_tabs") == ("true", "true"), text
        assert CS.mismatches(text, dict(vertical_tabs=False), "terminal") == [("vertical_tabs", "true", "false")], text
    assert CS.effective("", "vertical_tabs") == ("false", None)                                   # absent: the app default is in force
    assert CS.lookup(T.flat(inline), "appearance.vertical_tabs.display_granularity") == '"panes"'
    # strings: either quote; numbers: 13 is 13.0; alternatives; the sentinels
    assert CS.mismatches("[terminal.input]\nright_click_behavior = 'paste'\n", dict(right_click="context_menu"), "terminal") == [("right_click", "paste", "context_menu")]
    assert CS.mismatches("[appearance.text]\nfont_size = 13\n", dict(font_size=13.0), "terminal") == []
    assert CS.mismatches("[appearance.text]\nfont_size = 13.0\n", dict(font_size=CS.APP_DEFAULT), "terminal") == []
    assert CS.mismatches("[appearance.text]\nfont_size = 15.0\n", dict(font_size=CS.APP_DEFAULT), "terminal") == [("font_size", "15.0", "the app default (13.0)")]
    assert CS.mismatches("", dict(font_size=CS.APP_DEFAULT, extra_meta_keys=CS.APP_DEFAULT, startup_shell=CS.ABSENT), "terminal") == []
    assert CS.mismatches("[terminal.input.extra_meta_keys]\nleft_alt = true\n", dict(extra_meta_keys=CS.APP_DEFAULT), "terminal") == [("extra_meta_keys", "set", "the app default")]
    assert CS.mismatches("[terminal.input]\ncompletions_open_while_typing = true\n", dict(completions_while_typing=CS.ABSENT), "terminal") == [
        ("completions_while_typing", "true", "the key absent")]
    pi = dict(vertical_tabs_primary_info=("command", "working_directory"))
    assert CS.mismatches("", pi, "terminal") == [] and CS.mismatches('[appearance.vertical_tabs]\nprimary_info = "working_directory"\n', pi, "terminal") == []
    assert CS.mismatches('[appearance.vertical_tabs]\nprimary_info = "branch"\n', pi, "terminal") == [("vertical_tabs_primary_info", "branch", "command or working_directory")]
    assert CS.mismatches('[general]\ndefault_session_mode = "agent"\n', dict(session_mode=CS.PASS_MODE), "agent") == []
    assert CS.mismatches("", dict(session_mode=CS.PASS_MODE), "agent") == [("session_mode", "terminal", "agent")]
    assert CS.message([("a", "1", "2"), ("b", "x", "y")]) == "SETTINGS: a=1, case needs 2; b=x, case needs y"
    _raises(lambda: CS.mismatches("", dict(vertical_tab=False), "terminal"), KeyError)             # a misspelt name is never 'satisfied'
    _raises(lambda: CS.mismatches("", dict(providers="x"), "terminal"), ValueError)                # a list setting cannot be compared with a value
    # the note of a case that left the file changed names keys, never values
    note = cases.settings_left_changed('[a]\nx = "secret-one"\ny = 1\n', '[a]\nx = "secret-two"\ny = 1\n[b]\nz = true\n')
    assert note and "a.x" in note and "b.z" in note and "secret" not in note and "a.y" not in note
    assert cases.settings_left_changed("x = 1\n", "x = 1\n") is None and cases.settings_left_changed(None, "x = 1\n") is None
    assert cases.settings_left_changed("x = 1\n", "x  =  1\n") is None                             # the same values re-spaced are not a change


@check
def r16_effective_settings_path_per_platform():
    """the file requires() reads is the one the RUNNING app reads on each platform (the same rule as cases_ui.settings_path)"""
    import cases_ui as U
    prof = types.SimpleNamespace(home=os.path.join("h"))
    lin = types.SimpleNamespace(win=False, prof=prof, app=types.SimpleNamespace())
    mac = types.SimpleNamespace(win=False, prof=prof, app=types.SimpleNamespace(settings=os.path.join("h", ".phosphor", "settings.toml")))
    win = types.SimpleNamespace(win=True, prof=prof, app=types.SimpleNamespace(config=os.path.join("C:", "cfg")))
    sim = types.SimpleNamespace(win=False, prof=prof, app=None)
    assert cases.effective_settings_path(lin) == os.path.join("h", ".config", "phosphor", "settings.toml") == cases.effective_settings_path(sim)
    assert cases.effective_settings_path(mac) == os.path.join("h", ".phosphor", "settings.toml")
    assert cases.effective_settings_path(win) == os.path.join("C:", "cfg", "settings.toml")
    for c in (lin, mac, win):
        assert cases.effective_settings_path(c) == U.settings_path(c)


@check
def r16_every_case_declares_its_settings():
    """every REGISTERED case (fix tier, full tier, every lab arm) declares what it needs (case_settings.CASE_SETTINGS), is listed as
    settings-independent with a reason, or (a lab arm) matches exactly ONE family glob of case_settings.FAMILIES; no declaration names a case that
    does not exist, no glob matches nothing, no setting is undefined; every declared need can be evaluated and holds on a bare profile's text
    for the modes the case runs in; no lab family needs a setting the lab harness itself rewrites per arm; run.py checks the declaration before
    the case function; docs/testing.md carries the generated tables"""
    import fnmatch
    fix = list(cases.SMOKE) + list(cases.PROVING)
    reg = {c["name"]: c for c in cases.CASES}
    names = [c["name"] for c in cases.CASES]
    assert len(fix) == len(set(fix)) and len(names) == len(reg) and len(names) > len(fix) + 100          # the registry, not only the fix tier
    missing = [n for n in names if not CS.declared(n)]
    assert not missing, ("registered cases with no settings declaration (add them to case_settings.py: decl(...), independent(...), or for a lab "
                         "family lab(\"lab_<family>.*\", ...)): %s" % missing)
    assert not set(CS.CASE_SETTINGS) & set(CS.INDEPENDENT)
    stale = [n for n in list(CS.CASE_SETTINGS) + list(CS.INDEPENDENT) if n not in reg]
    assert not stale, "case_settings.py names cases that are not registered: %s" % stale
    for n, why in CS.INDEPENDENT.items():
        assert len(why) > 40, "%s: 'settings-independent' needs its reason" % n
    # the lab: one registered case per arm, one declaration per family
    lab_names = [n for n in names if reg[n].get("lab")]
    assert len(lab_names) > 300 and all(n.startswith("lab_") for n in lab_names)
    for n in names:
        globs, exact = CS.family_globs(n), (n in CS.CASE_SETTINGS or n in CS.INDEPENDENT)
        assert len(globs) + exact == 1, "%s: declared %d times (exact: %s, family globs: %s); a case has exactly one declaration" % (n, len(globs) + exact, exact, globs)
        assert bool(globs) == bool(reg[n].get("lab")), "%s: a family glob declares lab arms only, and a lab arm is declared by its family (%s)" % (n, globs)
    dead = [g for g, _ in CS.FAMILIES if not any(fnmatch.fnmatchcase(n, g) for n in names)]
    assert not dead, "case_settings.py family globs that match no registered case: %s" % dead
    assert {n.split(".")[0] for n in lab_names} == {g.split(".")[0] for g, _ in CS.FAMILIES}, "one glob per lab family, `lab_<family>.*`"
    for g in cases.PROVING_LAB:                              # what the fix tier (and the Mac / Windows lab jobs) play is declared
        hit = [n for n in lab_names if fnmatch.fnmatchcase(n, g)]
        assert hit and all(CS.declaration(n) for n in hit), g
    for g, d in CS.FAMILIES:
        # cases_lab.lab_prepare rewrites these per helper arm AFTER run.py's check: a need would be judged against the PREVIOUS arm's value
        assert not set(d["needs"]) & set(CS.LAB_SETS), "%s: a lab family cannot need %s (the lab harness writes it per arm)" % (g, sorted(set(d["needs"]) & set(CS.LAB_SETS)))
        assert set(CS.LAB_SETS) <= set(d["sets"]), g
    lab_src = open(os.path.join(HERE, "cases_lab.py"), encoding="utf-8").read()
    assert "CR.set_ext_mode(ctx, HELPER_ARM_MODE[arm[\"helper\"]])" in lab_src and "set_denied_hosts(ctx," in lab_src        # what LAB_SETS describes
    decls = [(n, d) for n, d in CS.CASE_SETTINGS.items()] + list(CS.FAMILIES)
    for n, d in decls:
        for k in list(d["needs"]) + list(d["sets"]) + list(d["reads"]):
            assert k in CS.SETTINGS, "%s: %r is not a setting of case_settings.SETTINGS" % (n, k)
    for n in names:
        d = CS.declaration(n)
        if d is None:
            assert n in CS.INDEPENDENT
            continue
        for mode in reg[n]["modes"]:
            wants = dict(CS.baseline(mode))
            wants.update(d["needs"])
            CS.mismatches("", wants, mode)                 # raises on a value the setting cannot be compared with
            # every need is one a bare pass profile meets (app defaults + what the harness forces): a declaration that cannot hold there would make
            # the case UNVERIFIABLE on every machine. Only the keys the harness forces are written here.
            bare = '[general]\ndefault_session_mode = "%s"\nrestore_session = false\n[warpify.ssh]\nssh_extension_install_mode = "never_install"\n' % mode
            if mode == "agent":
                bare += "[agents.warp_agent.input]\nai_auto_detection_enabled = true\nnld_in_terminal_enabled = true\n"
            bad = CS.mismatches(bare, wants, mode)
            assert not bad, "%s (%s pass): its declared needs do not hold on a bare pass profile: %s" % (n, mode, CS.message(bad))
    # the declaration run.py checks for a lab arm is its family's; an unknown name and a settings-independent case are not checked
    arm = next(n for n in lab_names if n.startswith("lab_tab_remote_dir."))
    assert CS.declaration(arm) is dict(CS.FAMILIES)["lab_tab_remote_dir.*"] and CS.declaration(arm)["needs"]["ssh_warpification"] is True
    assert CS.declaration("no_such_case") is None and CS.declaration("first_run_defaults") is None and not CS.declared("lab_no_such_family.bash.lan.nohelper")
    for n in CS.HARNESS_FORCED:
        assert CS.SETTINGS[n]["forced"], n
    assert [n for n, d in CS.SETTINGS.items() if d["forced"]] == [n for n in CS.SETTINGS if n in CS.HARNESS_FORCED]
    # the defaults this file states are the app's: each simple setting's key is defined in the Rust source with that toml_path
    src = ""
    for base in ("app/src", "crates"):
        for dp, _dn, fns in os.walk(os.path.join(ROOT, base)):
            for fn in fns:
                if fn.endswith(".rs") and ("settings" in fn or "settings" in dp or fn == "alt_screen_reporting.rs"):
                    try:
                        src += open(os.path.join(dp, fn), encoding="utf-8", errors="replace").read()
                    except OSError:
                        pass
    if src:      # a harness copied out of the repo has no app source: nothing to compare with
        for n, d in CS.SETTINGS.items():
            if d["path"].startswith("<"):
                continue
            assert 'toml_path: "%s"' % d["path"] in src, "%s: no setting with toml_path %s in the app source (renamed?)" % (n, d["path"])
    run_src = open(os.path.join(HERE, "run.py"), encoding="utf-8").read()
    i, j = run_src.index('cases.requires_declared(ctx, r, c["name"])'), run_src.index('c["fn"](ctx, r)')
    assert i < j and "be.guard()" in run_src[i:j], "run.py must check the declared settings before the case's first key"
    doc = open(os.path.join(ROOT, "docs", "testing.md"), encoding="utf-8").read()
    assert "### 3.1b Test settings model" in doc
    order = CS.table_order(names, fix)
    assert [n for n, _ in order][:len(fix)] == fix and len(order) == len(names) - len(lab_names) and {t for _, t in order} == {"fix", "full"}
    table = CS.table_markdown(order)
    assert table.count("\n| `lab_") == len(CS.FAMILIES) and "| `lab_tab_remote_dir.*` | lab |" in table and "| `fast_typing` | fix |" in table
    assert table in doc, ("docs/testing.md: the per-case settings table is not the one case_settings.py generates; regenerate it with "
                                             "`python3 script/realapp/appci/case_settings.py --write`")
    assert CS.settings_markdown() in doc, "docs/testing.md: the settings table is stale; regenerate it with `python3 script/realapp/appci/case_settings.py --write`"
    for n in CS.HARNESS_FORCED:
        assert "| %s |" % n in doc


# a key profile.py writes that is part of another named setting (the active model belongs to the provider list, and so on)
FORCED_ALIASES = {"agents.byop.last_used_model_id": "providers", "code.indexing.agent_mode_codebase_context_auto_indexing": "codebase_index",
                  "session.new_session_shell_override": "startup_shell"}


def harness_written_keys(src):
    """the dotted settings keys profile.py's write_settings / apply_fake_embeddings write or delete: `T.set_key(t, "<table>", "<key>", ...)`,
    `T.del_key(...)`, and the `for k in (...)` loops over one table"""
    import re
    keys = set()
    for m in re.finditer(r'T\.(?:set_key|del_key|get_key)\(t, "([^"]+)", "([^"]+)"', src):
        keys.add("%s.%s" % (m.group(1), m.group(2)))
    for m in re.finditer(r'for k in \(([^)]*)\):[^\n]*\n\s+t = T\.(?:set_key|del_key)\(t, "([^"]+)", k', src):
        for k in re.findall(r'"([^"]+)"', m.group(1)):
            keys.add("%s.%s" % (m.group(2), k))
    return keys


@check
def r16_every_harness_override_is_a_named_forced_setting():
    """the 'harness' column of docs/testing.md is complete: every settings key profile.py writes or deletes belongs to a setting of
    case_settings.SETTINGS that says `forced`, so a new override cannot be added without a line that tells the case authors"""
    import inspect
    src = inspect.getsource(profile.Profile.write_settings) + inspect.getsource(profile.Profile.apply_fake_embeddings)
    keys = harness_written_keys(src)
    assert {"general.default_session_mode", "general.restore_session", "warpify.ssh.ssh_extension_install_mode", "warpify.ssh.enable_legacy_ssh_wrapper",
            "session.startup_shell_override", "agents.warp_agent.providers", "agents.warp_agent.input.nld_in_terminal_enabled"} <= keys, sorted(keys)
    forced = {d["path"]: n for n, d in CS.SETTINGS.items() if d["forced"]}
    for k in sorted(keys):
        name = forced.get(k) or FORCED_ALIASES.get(k)
        assert name and CS.SETTINGS[name]["forced"], "profile.py writes %s and no setting of case_settings.SETTINGS says so (`forced=`)" % k
    # the guard sees an override nobody named
    assert "x.y" in harness_written_keys('t = T.set_key(t, "x", "y", "1")') and "x.y" not in forced
    assert harness_written_keys('for k in ("a", "b"):     # c\n    t = T.del_key(t, "s", k)') == {"s.a", "s.b"}
