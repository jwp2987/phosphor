# appci/case_settings.py: what settings each case runs with (docs/testing.md "Test settings model"). Pure: no app, no backend, no import of cases.
#
# The app under test is launched on a settings.toml built in three layers:
#   1. the owner seed       a MACHINE-LOCAL file (profile.py OWNER_SEED, or APPCI_OWNER_SETTINGS); never in the repo, never printed. No seed = a bare profile.
#   2. harness overrides    profile.py write_settings: the keys every pass forces (HARNESS_FORCED below says which and why).
#   3. per-case overrides   a case that rewrites the file (with the app stopped, or live) and puts it back.
# A case's oracle is only as good as its knowledge of those values. This file is that knowledge, per case:
#   * SETTINGS        the named settings a case can depend on: TOML path, the app's own default, what the harness does with it;
#   * BASELINE        what every declared case needs before its first key (the pass contract);
#   * CASE_SETTINGS   per case: `needs` (checked before the case by cases.requires: a mismatch is UNVERIFIABLE, never a pass, never a failure),
#                     `sets` (the case writes them itself and puts them back), `reads` (the case judges by the value in force: it reads the file, or
#                     the value is one the harness forces in every pass);
#   * INDEPENDENT     cases that judge nothing on the pass's settings file, each with the reason.
#   * FAMILIES        the lab: one declaration per lab family, by glob (`lab_<family>.*`: the arms of a family share one body, so one oracle).
# Every registered case (cases.CASES: fix tier, full tier, every lab arm) is in CASE_SETTINGS or in INDEPENDENT, or matches exactly one glob of
# FAMILIES (self-test r16_every_case_declares_its_settings), and the table in docs/testing.md is table_markdown() of this file (same self-test).
# Names only: no value of the owner's seed is written here; the values in `needs` are the ones the CASE was written against.
import fnmatch, os, re, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import toml_text as T  # noqa: E402

PASS_MODE = "<the pass's session mode>"      # needs: general.default_session_mode equals the mode of the pass (terminal | agent)
APP_DEFAULT = "<app default>"                # needs: the key is absent from the file (the app's own default is in force), or holds that default
ABSENT = "<absent>"                          # needs: the key is not in the file at all
UNSET_SHOWN = "app default"                  # how an absent key with no known literal default is written in a message


def S(path, default, about, forced=None, simple=True):
    return dict(path=path, default=default, about=about, forced=forced, simple=simple)


# name -> the setting. `default` is the literal the APP uses when the key is absent (read from the Rust definition named in `about`), None when it is
# a table, a list or platform-dependent. `forced`: what profile.py does with the key in every pass (None = the seed's value, or the app default).
# `simple` False: a table or list; it can be named in `sets` / `reads`, and in `needs` only as APP_DEFAULT / ABSENT.
SETTINGS = {
    "session_mode": S("general.default_session_mode", '"terminal"', "the view a new session opens in (settings/ai.rs DefaultSessionMode)",
                      forced="set to the pass's mode: every case runs once per mode"),
    "restore_session": S("general.restore_session", "true", "tabs and sessions restored at launch (terminal/general_settings.rs)",
                         forced="false: a relaunch inside a pass must not bring back another case's tabs"),
    "vim_mode": S("text_editing.vim_mode_enabled", "false", "vim keys in the input editor (settings/editor.rs)"),
    "autodetect": S("agents.warp_agent.input.ai_auto_detection_enabled", "true", "shell-or-prompt detection of a typed line (settings/ai.rs)",
                    forced="true in the agent pass (a shell line typed in the agent view must reach the shell); the terminal pass keeps the seed's value"),
    "nld_in_terminal": S("agents.warp_agent.input.nld_in_terminal_enabled", "true", "natural-language detection on the plain terminal (settings/ai.rs)",
                         forced="true in the agent pass; the terminal pass keeps the seed's value"),
    "nld_predict_in_ssh": S("agents.warp_agent.active_ai.natural_language_autosuggestions_in_ssh_enabled", "false",
                            "query predictions from an ssh pane (settings/ai.rs, #1982)"),
    "next_command_in_ssh": S("agents.warp_agent.active_ai.intelligent_autosuggestions_in_ssh_enabled", "false",
                             "next-command suggestions requested from an ssh pane (settings/ai.rs, #2094)"),
    "prompt_suggestions_in_ssh": S("agents.warp_agent.active_ai.prompt_suggestions_in_ssh_enabled", "false",
                                   "prompt suggestions requested from an ssh pane (settings/ai.rs, #2094)"),
    "vertical_tabs": S("appearance.vertical_tabs.enabled", "false", "the vertical tabs panel instead of the horizontal tab bar (workspace/tab_settings.rs)"),
    "vertical_tabs_primary_info": S("appearance.vertical_tabs.primary_info", '"command"', "what a panel row shows first: command | working_directory | branch"),
    "vertical_tabs_view_mode": S("appearance.vertical_tabs.view_mode", '"compact"', "panel rows compact | expanded"),
    "vertical_tabs_granularity": S("appearance.vertical_tabs.display_granularity", '"panes"', "one panel row per pane | per tab"),
    "copy_on_select": S("terminal.copy_on_select", "true", "a selection is copied to the clipboard (settings/select.rs)"),
    "linux_selection_clipboard": S("system.linux_selection_clipboard", "true", "Linux PRIMARY selection (settings/select.rs)"),
    "middle_click_paste": S("terminal.input.middle_click_paste_enabled", "true", "middle click pastes (settings/select.rs)"),
    "right_click": S("terminal.input.right_click_behavior", '"context_menu"', "right click: context menu or paste (settings/select.rs)"),
    "honor_ps1": S("terminal.input.honor_ps1", "false", "the shell's own prompt instead of the app's (terminal/session_settings.rs)"),
    "secret_redaction": S("privacy.secret_redaction.enabled", "false", "secret redaction (terminal/safe_mode_settings.rs)"),
    "secret_regexes": S("privacy.custom_secret_regex_list", None, "the user's secret patterns", simple=False),
    "quit_warning": S("general.show_warning_before_quitting", "true", "the question before a quit / close with something running (terminal/general_settings.rs)"),
    "completions_while_typing": S("terminal.input.completions_open_while_typing", "true",
                                  "the completions menu opens while typing, local sessions (settings/input.rs)"),
    "autosuggestions": S("terminal.input.autosuggestions.enabled", "true", "ghost text from history (settings/editor.rs)"),
    "extra_meta_keys": S("terminal.input.extra_meta_keys", None, "Alt / Option acts as Meta (terminal/keys_settings.rs)", simple=False),
    "osc52_clipboard": S("terminal.osc52_clipboard_access", '"write_only"', "OSC 52 clipboard access (terminal/settings.rs)"),
    "mouse_reporting": S("terminal.mouse_reporting_enabled", "true", "mouse reports to a program that asked (terminal/alt_screen_reporting.rs)"),
    "scroll_reporting": S("terminal.scroll_reporting_enabled", "true", "wheel reports to a program that asked (terminal/alt_screen_reporting.rs)"),
    "focus_reporting": S("terminal.focus_reporting_enabled", "true", "focus in / out reports (terminal/alt_screen_reporting.rs)"),
    "ssh_warpification": S("warpify.ssh.enable_ssh_warpification", "true", "managed ssh sessions (terminal/warpify/settings.rs)",
                           forced="true only with APPCI_SSH_WRAPPER=1 (the Windows *_wrapper ssh cases, #1884); else the seed's value"),
    "legacy_ssh_wrapper": S("warpify.ssh.enable_legacy_ssh_wrapper", "true", "the ssh wrapper function in the pane's shell (terminal/warpify/settings.rs)",
                            forced="true only with APPCI_SSH_WRAPPER=1; else the seed's value"),
    "ssh_extension_install": S("warpify.ssh.ssh_extension_install_mode", '"always_ask"', "the remote helper: ask / install / never (terminal/warpify/settings.rs)",
                               forced='"never_install": no helper is the primary remote design, and no case may be stopped by the install question'),
    "reuse_control_master": S("warpify.ssh.reuse_existing_control_master", "false", "attach to the user's own ControlMaster (settings/ssh.rs)"),
    "helper_hosts": S("warpify.ssh.remote_helper_allowed_hosts", None, "hosts the helper may / may not be used on (and remote_helper_denied_hosts)", simple=False),
    "subshell_commands": S("warpify.subshells.added_subshell_commands", None, "commands that get the subshell banner",
                           forced="the seed's list plus profile.PROBE_SUBSHELL_COMMANDS (dash, busybox ash, bash --rcfile, tcsh -f, docker run -t)", simple=False),
    "startup_shell": S("session.startup_shell_override", None, "the pane's shell (and session.new_session_shell_override)",
                       forced="removed (the rc files that log prompts and commands belong to the login shell); set by the harness for the Windows Git Bash, "
                              "pwsh 7, cmd.exe and WSL passes", simple=False),
    "network_gate": S("network.gate_enabled", "true", "the network gate (settings/network.rs)"),
    "allowed_origins": S("network.allowed_origins", None, "origins the gate lets through", simple=False),
    "open_file_editor": S("code.editor.open_file_editor", '"system_default"', "what opens a clicked file (util/file/external_editor/settings.rs)"),
    "agent_file_read": S("agents.profiles.agent_mode_coding_permissions", '"always_ask_before_reading"', "the agent's file-read permission (settings/ai.rs)"),
    "agent_readonly_autorun": S("agents.profiles.agent_mode_execute_readonly_commands", "false", "the agent runs read-only commands unasked (settings/ai.rs)"),
    "safe_command_allowlist": S("agents.warp_agent.permissions.safe_command_allowlist_enabled", "false", "the safe allow list, auto-run (settings/ai.rs)"),
    "persist_conversations": S("general.persist_conversations", "true", "agent conversations kept across restarts (terminal/general_settings.rs)"),
    "agent_notifications": S("agents.warp_agent.other.show_agent_notifications", "true", "notifications of a waiting agent (settings/ai.rs)"),
    "providers": S("agents.warp_agent.providers", None, "the model providers and the active model (agents.byop.last_used_model_id)",
                   forced="with APPCI_OLLAMA_URL (ci.env): the CI endpoint alone, as the active model; in the agent@fake pass the scripted fake model. "
                          "Without either: the seed's list (a bare profile has none)", simple=False),
    "codebase_index": S("code.indexing.agent_mode_codebase_context", None, "codebase context and auto indexing",
                        forced="true only with APPCI_FAKE_EMBEDDINGS_URL (the lab's index case)"),
    "notifications": S("notifications.preferences", None, "desktop notification preferences", simple=False),
    "toast_seconds": S("notifications.toast_duration_secs", "8", "how long a toast stays (terminal/session_settings.rs)"),
    "system_theme": S("appearance.themes.system_theme", "false", "the theme follows the OS light / dark setting (settings/theme.rs)"),
    "theme": S("appearance.themes.theme", None, "the theme, when it does not follow the OS (settings/theme.rs; the default is the fork's own theme kind)"),
    "font_size": S("appearance.text.font_size", "13.0", "the terminal font size (settings/font.rs)"),
    "zoom": S("appearance.window.zoom_level", "100", "the window zoom (window_settings.rs)"),
    "mcp_config": S("<mcp config file>", None, "the app's global MCP server file, beside settings.toml (not a settings.toml key)", simple=False),
    # not a settings.toml key: the keybindings file beside it (the same directory). The harness never seeds one, so the app's default bindings are in force.
    "keybindings_file": S("<keybindings.yaml>", '"absent"', "the user's keybindings file beside settings.toml: absent | present",
                          forced="never seeded: every pass runs the app's default key bindings"),
}

# what profile.py write_settings forces, in the order it applies them (docs/testing.md "Test settings model" prints this list)
HARNESS_FORCED = tuple(n for n in ("session_mode", "restore_session", "ssh_extension_install", "subshell_commands", "ssh_warpification", "legacy_ssh_wrapper",
                                   "startup_shell", "autodetect", "nld_in_terminal", "providers", "codebase_index", "keybindings_file"))


def baseline(mode):
    """what every declared case needs before its first key: the pass contract. A case that judges nothing on the pass's settings is in INDEPENDENT."""
    b = dict(session_mode=PASS_MODE, restore_session=False, vim_mode=False)
    if mode == "agent":
        b.update(autodetect=True, nld_in_terminal=True)
    return b


# ------------------------------------------------------------------ reading a settings file
def _split_inline(body):
    """top-level `key=value` pairs of an inline table's body (whitespace already removed), string- and bracket-aware"""
    out, depth, cur, q = [], 0, "", None
    for ch in body:
        if q:
            cur += ch
            if ch == q:
                q = None
            continue
        if ch in "\"'":
            q = ch
        elif ch in "[{":
            depth += 1
        elif ch in "]}":
            depth -= 1
        elif ch == "," and depth == 0:
            out.append(cur)
            cur = ""
            continue
        cur += ch
    if cur:
        out.append(cur)
    return dict((p.split("=", 1)[0].strip('"'), p.split("=", 1)[1]) for p in out if "=" in p)


def lookup(flat, path):
    """the literal of `path` (a.b.c) in a toml_text.flat dict, or None. Reads the three forms a settings file can use: `[a.b]` + `c = v`, a dotted key
    under a shorter table (`[a]` + `b.c = v`, which flat already joins), and an inline table (`[a]` + `b = { c = v }`)."""
    if path in flat:
        return flat[path]
    parts = path.split(".")
    for i in range(len(parts) - 1, 0, -1):
        lit = flat.get(".".join(parts[:i]))
        rest = parts[i:]
        while lit is not None and lit.startswith("{") and lit.endswith("}") and rest:
            lit = _split_inline(lit[1:-1]).get(rest[0])
            rest = rest[1:]
        if lit is not None and not rest:
            return lit
    return None


def has_table(flat, path):
    """is anything written under `path` (a table or list setting: `[path]`, `[[path]]` sub-keys, or `path = ...`)?"""
    return lookup(flat, path) is not None or any(k.startswith(path + ".") for k in flat)


def norm(lit):
    """a TOML literal in one spelling: no whitespace, double quotes"""
    if lit is None:
        return None
    lit = re.sub(r"\s+", "", lit)
    if len(lit) >= 2 and lit[0] == "'" and lit[-1] == "'":
        lit = '"%s"' % lit[1:-1]
    return lit


def literal(v):
    """a Python value as the TOML literal a settings file holds for it"""
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, (int, float)):
        return repr(v)
    return '"%s"' % (v,)


def same(a, b):
    """two normalized literals name the same value (13 and 13.0 are one number)"""
    if a == b:
        return True
    try:
        return float(a) == float(b)
    except (TypeError, ValueError):
        return False


def shown(lit):
    """a literal as a message prints it: the quotes of a string dropped"""
    if lit is None:
        return UNSET_SHOWN
    return lit[1:-1] if len(lit) >= 2 and lit[0] == '"' and lit[-1] == '"' else lit


def effective(text, name, config_dir=None):
    """(literal in force, literal written in the file or None) for one named setting on a settings.toml TEXT: the file's value, else the app's default
    (None when the default is not one literal). keybindings_file is read from `config_dir` (the directory of the settings file)."""
    d = SETTINGS[name]
    if name == "keybindings_file":
        there = bool(config_dir) and os.path.isfile(os.path.join(config_dir, "keybindings.yaml"))
        return ('"present"' if there else '"absent"'), None
    flat = T.flat(text)
    if not d["simple"]:
        written = '"set"' if has_table(flat, d["path"]) else None
        return written, written
    written = norm(lookup(flat, d["path"]))
    return (written if written is not None else d["default"]), written


def mismatches(text, wants, mode, config_dir=None):
    """[(name, actual as shown, expected as shown)] for every setting of `wants` ({name: value | PASS_MODE | APP_DEFAULT | ABSENT | tuple of values})
    the settings text does not satisfy. An unknown name or a value a table setting cannot be compared with raises: a typo must not read as 'satisfied'."""
    out = []
    for name in sorted(wants):
        want = wants[name]
        if name not in SETTINGS:
            raise KeyError("requires(): %r is not a setting named in case_settings.SETTINGS" % name)
        d = SETTINGS[name]
        actual, written = effective(text, name, config_dir)
        if want is ABSENT or want == ABSENT:
            if written is not None:
                out.append((name, shown(written), "the key absent"))
            continue
        if want is APP_DEFAULT or want == APP_DEFAULT:
            if written is not None and not (d["default"] is not None and same(written, d["default"])):
                out.append((name, shown(written), "the app default" + ("" if d["default"] is None else " (%s)" % shown(d["default"]))))
            continue
        if not d["simple"]:
            raise ValueError("requires(): %s is a table or list setting; only APP_DEFAULT or ABSENT can be required of it" % name)
        alts = want if isinstance(want, (tuple, list, set, frozenset)) else (want,)
        lits = [literal(mode if (w is PASS_MODE or w == PASS_MODE) else w) for w in alts]
        if not any(actual is not None and same(actual, l) for l in lits):
            out.append((name, shown(actual), " or ".join(shown(l) for l in lits)))
    return out


def message(bad):
    """the one-line reason of an UNVERIFIABLE row: 'SETTINGS: <name>=<actual>, case needs <expected>' per setting"""
    return "SETTINGS: " + "; ".join("%s=%s, case needs %s" % (n, a, e) for n, a, e in bad)


def changed_keys(before, after):
    """the dotted keys whose literal differs between two settings texts (added and removed keys too). Names only: run.py notes them when a case leaves
    the settings file other than it found it."""
    a, b = T.flat(before or ""), T.flat(after or "")
    return sorted(k for k in set(a) | set(b) if a.get(k) != b.get(k))


# ------------------------------------------------------------------ per-case declarations
# Bundles of `needs`: the values the cases were written against. Every one of them is the app's own default, so a bare profile satisfies them too.
INPUT = dict(completions_while_typing=True, autosuggestions=True)       # a line typed into the input editor, judged by what the shell received
KEYS = dict(keybindings_file="absent")                                  # the app's default chords (palette, tabs, find, close, settings, quit)
PROMPT = dict(honor_ps1=False)                                          # the app draws the prompt: the bootstrap's hand-off is what is judged
SSH = dict(ssh_warpification=True, legacy_ssh_wrapper=True, ssh_extension_install="never_install", honor_ps1=False)
# reuse_control_master is the seed's value in every ssh case and no case reads it: the harness's ssh config names no ControlMaster to attach to
AGENT_ASK = dict(agent_readonly_autorun=False, safe_command_allowlist=False)      # a command the model proposes stops at an approval card
MODEL = ("providers",)                                                  # the provider the harness configured for the pass (CI endpoint or fake model)

CASE_SETTINGS = {}
INDEPENDENT = {}
FAMILIES = []          # [(glob, declaration)] in the order declared: the lab families (one registered case per arm, one declaration per family)


def _merge(*ds):
    out = {}
    for d in ds:
        out.update(d)
    return out


def decl(names, needs=None, sets=(), reads=(), note=""):
    for n in names.split():
        assert n not in CASE_SETTINGS and n not in INDEPENDENT, n
        CASE_SETTINGS[n] = dict(needs=dict(needs or {}), sets=tuple(sets), reads=tuple(reads), note=note)


def independent(names, why):
    for n in names.split():
        assert n not in CASE_SETTINGS and n not in INDEPENDENT, n
        INDEPENDENT[n] = why


def family(globs, needs=None, sets=(), reads=(), note=""):
    """one declaration for every case whose name matches a glob (`lab_tab_remote_dir.*`: every arm of that lab family)"""
    for g in globs.split():
        assert g not in dict(FAMILIES) and ("*" in g or "?" in g), g
        FAMILIES.append((g, dict(needs=dict(needs or {}), sets=tuple(sets), reads=tuple(reads), note=note)))


def family_globs(name):
    """the globs of FAMILIES that match a case name (the self-test wants exactly one for a case with no exact declaration)"""
    return [g for g, _ in FAMILIES if fnmatch.fnmatchcase(name, g)]


def declaration(name):
    """the declaration `run.py` checks before the case: the exact one, else the first family glob that matches; None for a settings-independent
    case and for a name nothing declares"""
    if name in CASE_SETTINGS:
        return CASE_SETTINGS[name]
    if name in INDEPENDENT:
        return None
    for g, d in FAMILIES:
        if fnmatch.fnmatchcase(name, g):
            return d
    return None


def declared(name):
    return name in CASE_SETTINGS or name in INDEPENDENT or bool(family_globs(name))


# ---- SMOKE
decl("mode_proof", reads=MODEL, note="the pass's mode itself: the view a new session opens in (baseline session_mode)")
decl("key_loss_control enter_loss_control cadence fast_typing paste_instant_enter", needs=INPUT,
     note="typed lines at the prompt, judged by the exact line the shell received")
decl("tab_enter", needs=_merge(INPUT, KEYS), note="Tab completion then Enter; the 'Completion took too long' give-up is read from the app log, not from the toast")
decl("ctrl_c_d", needs=_merge(INPUT, PROMPT))
decl("ssh_connect_typing ssh_enter_then_immediate_command ssh_nested_typeahead", needs=_merge(INPUT, SSH),
     note="managed ssh without the helper; in the agent pass the shell-or-prompt verdict of each line is part of the path. As in every ssh case, "
          "reuse_control_master is the seed's and is not read: the harness's ssh config names no ControlMaster to attach to")
decl("shell_fish shell_nu", needs=_merge(INPUT, PROMPT), note="fish / nu as a subshell of the pane's shell (built-in subshell commands)")
decl("latency_enter latency_key_log", needs=_merge(INPUT, PROMPT), note="budgets are measured with the input editor's default helpers on")
decl("latency_paint", needs=_merge(INPUT, PROMPT, dict(font_size=APP_DEFAULT, zoom=APP_DEFAULT)),
     note="key -> paint watches a screen rectangle: measured at the default text size; the theme (system_theme: the seed's) is not read")
decl("md_viewer", needs=KEYS, note="the file is opened through the app's own open-file path (not a click: open_file_editor is not involved); OCR of the viewer")
decl("routing_table", needs=_merge(INPUT, dict(autodetect=True)), reads=("nld_in_terminal",),
     note="prose goes to the agent in the terminal pass only when nld_in_terminal is not false: the case reads it and judges by it")
decl("agent_stream_cancel agent_input_exact", needs=INPUT, reads=MODEL)
decl("agent_mode_switch", needs=_merge(INPUT, KEYS, dict(autodetect=True, nld_in_terminal=True)),
     note="routing after each agent <-> terminal switch, in both passes")

# ---- PROVING: agent cases on the scripted model
decl("agent_edit_accept agent_edit_reject agent_edit_conflict agent_edit_touch_only agent_edit_endings", needs=KEYS, reads=MODEL,
     note="the diff card of an edit the scripted model proposes (no file read is asked for first); accept and reject are the card's default keys")
decl("agent_interactive_password agent_interactive_vim agent_interactive_less agent_noninteractive_no_takeover", needs=_merge(AGENT_ASK, PROMPT), reads=MODEL)
decl("agent_interactive_ssh agent_interactive_ssh_takeover", needs=_merge(AGENT_ASK, SSH), reads=MODEL)
decl("agent_latency_key agent_latency_enter agent_steer_fast", needs=INPUT, reads=MODEL)
decl("agent_mcp_faults mcp_basic", needs=KEYS, sets=("mcp_config",), reads=MODEL + ("agent_readonly_autorun",),
     note="the MCP call is approved at its card where the profile shows one; the case notes when none was shown")
decl("agent_multitab_streaming", needs=_merge(INPUT, KEYS), reads=MODEL, note="tabs by their number keys: the tab layout (vertical_tabs: the seed's) is not read")
decl("agent_gate_deny_allow agent_gate_off_live agent_gate_key_never_leaves", needs=KEYS, sets=("network_gate", "allowed_origins", "providers"),
     note="the gate and the origin list are written by the case (live and with the app stopped) and put back")
decl("network_gate_notice", needs=KEYS, sets=("network_gate", "allowed_origins"), note="the toast is found on screen while it is up (toast_seconds: the seed's, not read)")
decl("network_gate_switch", needs=KEYS, sets=("network_gate", "allowed_origins"),
     note="starts from a file with NO network.gate_enabled key (absent must mean on); a profile that already carries the key fails the case's own first check")
decl("agent_fault_429 agent_fault_500 agent_fault_disconnect agent_fault_down agent_fault_slow_cancel agent_fault_queued_rows", needs=INPUT, reads=MODEL)
decl("agent_restart_quit_stream agent_restart_quit_queued agent_restart_quit_blocked agent_restart_kill_stream agent_restart_kill_queued agent_restart_kill_blocked",
     needs=_merge(AGENT_ASK, KEYS, dict(persist_conversations=True)), sets=("restore_session",), reads=MODEL,
     note="restore_session is switched on for the restart and put back")
decl("agent_lrc_no_false_takeover agent_approval", needs=_merge(AGENT_ASK, INPUT), reads=MODEL)
decl("agent_question_answer agent_question_cut agent_question_nodone agent_question_trunc agent_question_empty agent_question_slow agent_question_ctrl_c "
     "agent_question_steer agent_question_steer_at_end", needs=INPUT, reads=MODEL)
decl("agent_question_after_command", needs=_merge(INPUT, AGENT_ASK), reads=MODEL)
decl("agent_model_switch agent_anthropic_wire", needs=_merge(KEYS, AGENT_ASK), sets=("providers",), note="its own provider list, written with the app stopped and put back")
decl("agent_autorun_permissions", needs=dict(agent_readonly_autorun=False), sets=("safe_command_allowlist",),
     note="the allow list off (the control: `ls` asks) and on, both written by the case")
decl("agent_conversation_resume", needs=_merge(KEYS, dict(persist_conversations=True)), sets=("restore_session",), reads=MODEL)
decl("agent_block_context", needs=KEYS, sets=("secret_redaction", "secret_regexes"), reads=MODEL, note="redaction off and on, both written by the case with its own pattern")
decl("agent_hidden_tab_attention", needs=_merge(KEYS, INPUT), sets=("vertical_tabs",), reads=MODEL,
     note="the panel is taken as it is when on, else switched on live and put back (Windows: only when the profile has it on)")
decl("agent_code_recycle", needs=_merge(KEYS, INPUT), reads=MODEL)
decl("freeze_and_wake", needs=_merge(SSH, INPUT), reads=MODEL)
decl("desktop_notification_agent_question", sets=("notifications",), reads=MODEL, note="as desktop_notification, with the agent's needs-attention notification on")

# ---- PROVING: input, routing, shells
decl("ctrl_c_no_esc_i_local ctrl_c_flood_ab giveup_keys_reach_shell", needs=_merge(INPUT, PROMPT))
decl("ui_completions_as_you_type_local", needs=dict(completions_while_typing=ABSENT),
     note="the option NOT written by the profile: an explicit value wins over the local-session default (the case skipped on one; now UNVERIFIABLE)")
decl("remote_tab_enter_no_helper remote_tab_no_helper ssh_password_prompt", needs=_merge(INPUT, SSH))
decl("remote_session_legacy", needs=_merge(INPUT, SSH), sets=("ssh_extension_install",))
decl("subshell_sh_probe subshell_ready_rc_questions subshell_ready_vim_typeahead subshell_ready_unsupported_tcsh subshell_ready_secret_after_prompt "
     "subshell_ready_relay_shape subshell_ready_exec_fish_rc subshell_ready_docker_t_without_i subshell_ready_zsh_newuser_install subshell_ready_rc_clear "
     "subshell_ready_secret_after_probe subshell_sh_probe_empty_ps1", needs=_merge(INPUT, PROMPT), reads=("subshell_commands",),
     note="the subshell banner needs the commands the harness adds to the seed's list (forced in every pass)")
decl("routing_typo_head", needs=_merge(INPUT, dict(autodetect=True)))
decl("routing_plain_installed_head", needs=_merge(INPUT, KEYS, dict(autodetect=True, nld_in_terminal=True)),
     note="with detection off on the plain terminal every line reaches the shell and the installed-head rule would be 'proven' by nothing")
decl("routing_table_powershell agent_basics_powershell", needs=_merge(INPUT, dict(autodetect=True)), reads=("nld_in_terminal",) + MODEL)
decl("latency_paint_ab latency_echo_ab", needs=_merge(INPUT, dict(font_size=APP_DEFAULT, zoom=APP_DEFAULT)))
decl("latency_key_views_ab", needs=INPUT)
decl("ssh_connect_typing_wrapper ssh_nested_typeahead_wrapper", needs=_merge(INPUT, SSH),
     note="Windows Git Bash; the job sets APPCI_SSH_WRAPPER=1 so the two wrapper switches are written, not defaulted")
decl("ssh_gitbash_managed ssh_gitbash_typeahead_managed ssh_gitbash_kill_switch ssh_gitbash_bootstrap_in_command", needs=_merge(INPUT, SSH))
decl("ps_ctrl_break ps_ctrl_scrolllock_break ps_fast_command_young_enter gitbash_long_osc_reaches_app gitbash_typeahead_reps", needs=_merge(INPUT, PROMPT))
decl("ps_read_host_secure gitbash_precmd_secret", needs=_merge(INPUT, PROMPT))
decl("precmd_secret_honor_ps1", needs=INPUT, sets=("honor_ps1", "startup_shell"), note="both arms written by the case: honor PS1 off (control) and on; macOS forces bash")
decl("precmd_question_echo", needs=_merge(INPUT, PROMPT), sets=("startup_shell",), note="macOS: the startup shell is set to bash for the case and put back")
decl("output_frames", needs=KEYS, note="frame counts from the app log with hidden tabs; the tab layout (vertical_tabs: the seed's) is not read")
decl("history_up_recall", needs=_merge(INPUT, PROMPT))
decl("job_control hostile_output_recovers", needs=_merge(INPUT, KEYS, PROMPT))
decl("meta_and_word_keys", needs=_merge(INPUT, KEYS), reads=("extra_meta_keys",),
     note="default key bindings: Ctrl-A / Ctrl-E / Ctrl-Y off macOS are the app's documented defaults, not readline (an open owner question); "
          "whether Alt acts as Meta is read from the file")
decl("ui_readline_keys", needs=_merge(INPUT, KEYS, dict(extra_meta_keys=APP_DEFAULT)))

# ---- PROVING: UI by keyboard
decl("cb_report_problem", needs=KEYS, note="the bundle's copy of the pass's settings is checked for the home path, host name and user name, whatever the settings are")
decl("editor_save_mixed_endings editor_memory_7mb", needs=KEYS)
decl("editor_daily", needs=KEYS, reads=("quit_warning",), note="quit_warning false also switches off the unsaved-changes question: the case says UNVERIFIABLE itself")
decl("flood_scrollback_find scroll_keys_position find_basic scroll_keys_foreground", needs=KEYS,
     note="positions come from the app's own geometry lines; where OCR is used the theme and text size are the seed's and are not read")
decl("tab_lifecycle_keys", needs=KEYS, reads=("quit_warning",), note="with quit_warning false the close key does not ask: the cancel half is not run (noted)")
decl("settings_ui_toggle_persists", needs=KEYS, sets=("vertical_tabs", "font_size"),
     note="toggles vertical tabs from whichever state the profile has and raises the font size one step from the file's value; the file's bytes are put back")
decl("osc52_write_read", needs=SSH, reads=("osc52_clipboard",), note="the expectation follows terminal.osc52_clipboard_access when the profile sets it")
decl("history_survives_restart", needs=_merge(INPUT, KEYS), note="the quit goes through the app's quit key; its question is answered when it comes (quit_warning is not read)")
decl("restore_shows_history", needs=_merge(KEYS, dict(persist_conversations=True)), sets=("restore_session",), reads=MODEL)
decl("session_restore crash_kill9_restore", needs=KEYS, sets=("restore_session",), note="restore_session on for the restart, put back; the quit goes through the app's quit key and its question")
decl("ui_resize_typing ui_split_panes resize_reflow_output minimize_fullscreen", needs=_merge(INPUT, KEYS),
     note="sizes are read from the pane's own `stty size`, never assumed, so the tab layout and text size (the seed's) are not read")
decl("ui_ctrl_r ui_ghost_accept ui_tab_menu", needs=_merge(INPUT, KEYS))
decl("big_paste paste_multiline_prompt", needs=_merge(INPUT, PROMPT))
decl("alt_screen_programs", needs=INPUT, note="vim, less, tmux by keyboard only")
decl("keybinding_rebind", needs=KEYS, sets=("keybindings_file",), note="starts from NO keybindings.yaml (the control), writes its own, removes it")
decl("grid_snapshot_render", needs=INPUT, note="cells are compared through the app's grid snapshot hook, not from pixels: theme, text size and tab layout are not read")
decl("gpu_real_render", note="two launches on the same profile are compared with each other, so the seed's theme and text size cancel out")
decl("gpu_lost_device_recovers", note="one launch compared with itself before and after the rebuilt renderer, so the seed's theme and text size cancel out; "
                                      "judged from the app log (the hook's line, the rebuild, frames) and two screenshots")
decl("soak", needs=_merge(INPUT, KEYS), reads=MODEL)
decl("startup_shell_pane", needs=INPUT, reads=("startup_shell",), note="the pass's own startup shell (set by the harness for the pwsh 7 / cmd / WSL passes)")

# ---- PROVING: pointer, window, desktop
decl("ui_copy_wrapped_output", needs=KEYS)
decl("ui_copy_wrapped_drag selection_modes", needs=KEYS, note="every copy is the copy key after the selection (a sentinel is on the clipboard first), so copy_on_select is not what is judged")
decl("ui_vertical_tabs_many", needs=KEYS, sets=("vertical_tabs",),
     note="taken as it is when the profile has the panel on (the owner's layout), else switched on live and put back")
decl("ui_vertical_tabs_panel_click", needs=_merge(KEYS, dict(vertical_tabs_primary_info=("command", "working_directory"))), sets=("vertical_tabs",),
     note="the rows are found by each tab's last command or directory name, so the panel must show one of them first; panel on as ui_vertical_tabs_many; "
          "the panel's view mode and granularity are the seed's and are not read")
decl("context_menu_actions", needs=dict(right_click="context_menu"), note="a right click that pastes has no menu to test (the case skipped on it; now UNVERIFIABLE)")
decl("tab_pointer_use", needs=KEYS, sets=("vertical_tabs",), note="both layouts: the one the profile shows first, then the other, written live and put back")
decl("focus_cycle", needs=_merge(INPUT, dict(focus_reporting=True)))
decl("link_and_path_click", reads=("open_file_editor",), note="a clicked path is judged by what opened it: the desktop opener or the app's own file pane, whichever the profile names")
decl("primary_and_middle_click", needs=dict(linux_selection_clipboard=True, middle_click_paste=True), sets=("copy_on_select",),
     note="copy on select off, then on, both written live by the case")
decl("primary_absent_clipboard_kept", sets=("copy_on_select",), note="copy on select off, then on, both written live by the case")
decl("wheel_scroll", reads=("mouse_reporting", "scroll_reporting"), note="where the profile turns reporting off the program must receive nothing")
decl("pane_resize_maximize", needs=_merge(INPUT, KEYS), note="pane sizes from each pane's own `stty size`")
decl("scale_matrix", needs=_merge(KEYS, dict(zoom=APP_DEFAULT)), note="the grid at a display scale is compared with the same profile at scale 1; the word is copied with the copy key")
decl("desktop_notification", sets=("notifications",), note="notifications enabled with its own long-running threshold, app relaunched, put back")
decl("drag_drop_file ime_compose", needs=INPUT)
decl("mac_pointer_chord_proof", needs=KEYS, note="proves the pointer tool and the app-only chords: default bindings")

# ---- startup, network, plugins
decl("keyring_unavailable_startup", needs=INPUT, reads=MODEL, note="relaunches on the pass profile without a secret service")
decl("zero_network_startup", needs=dict(network_gate=True), reads=MODEL,
     note="NOT default settings: the pass profile. With a provider configured the scripted session may reach that endpoint and only it")
decl("claude_plugin_offline agent_plugins_offline", needs=INPUT, note="the bundled plugins; the per-agent toolbar settings of the seed are not read by the checks")

independent("first_run_defaults", "stops the app, moves the config directory aside and launches with NO settings file: the app's own defaults are what is judged; "
                                  "the profile is put back at the end")
independent("installer_smoke", "installs the build's artefact and runs the installed copy on data of its own making; the pass's app is stopped meanwhile")
independent("upgrade_from_previous_release", "the previous release writes its own profile (or the checked-in fixture's) and the build under test upgrades that")
independent("keychain_startup_macos", "three launches judged on first frame, log and exact typing against keychains the harness creates; no setting of the profile is read")


# ================================================================== full tier only (round 16, r16-declare): the registered cases outside SMOKE / PROVING
# ---- typing, paste, programs at the prompt
decl("rollover expert_fuzz arrows_behind_paste read_x keydown_count flood_typed_line", needs=_merge(INPUT, PROMPT),
     note="typed (and pasted) lines at the prompt, judged by the exact line the shell received")
decl("subshells shell_zsh shell_pwsh gitbash_subshell", needs=_merge(INPUT, PROMPT),
     note="a shell started from the pane's shell (the built-in subshell commands; Git Bash from PowerShell on Windows)")
decl("alt_screen paste_bracketed_off", needs=INPUT, note="a program at the tty (less, vim, cat) by keyboard and paste; judged by the bytes it received")
decl("latency_first_key_after_idle", needs=_merge(INPUT, PROMPT, dict(font_size=APP_DEFAULT, zoom=APP_DEFAULT)),
     note="as latency_paint: macOS watches a screen rectangle (default text size); elsewhere key -> first frame from the app log")
decl("idle_cpu", note="CPU of the idle app on the pass profile as a whole (theme, tab layout, provider: the seed's); no single setting is its oracle")
decl("flood_typing_latency tabs_streaming tabs_two_windows", needs=_merge(INPUT, KEYS, PROMPT),
     note="new tab / new window by the default chords; the tab layout (vertical_tabs: the seed's) is not read")
decl("quit_24_tabs", needs=_merge(INPUT, KEYS), note="the quit goes through the app's quit key; its question is answered when it comes (quit_warning is not read)")
decl("intl_wide_type intl_wide_paste intl_wide_edit intl_wide_program intl_compose_linux intl_ime_japanese intl_ime_pinyin", needs=_merge(INPUT, PROMPT),
     note="judged by the UTF-8 bytes the shell or the program received; the rendering is a screenshot, not a check (the font is the seed's)")
decl("intl_deadkey_option intl_altgr_ps intl_altgr_bash", needs=_merge(INPUT, PROMPT, dict(extra_meta_keys=APP_DEFAULT)),
     note="Option dead keys (macOS) and AltGr (Windows, German layout): with Option / Alt acting as Meta the character is not composed")
# ---- ssh on the harness's own test host
decl("ssh_failing_not_local ssh_hanging_host_not_local", needs=_merge(INPUT, SSH),
     note="an ssh that fails or hangs while connecting: the typed line runs locally once (plain-terminal pass-through of the managed ssh path)")
decl("remote_heredoc_paste latency_enter_ssh ssh_drop_reconnect", needs=_merge(INPUT, SSH))
decl("remote_session_ext", needs=_merge(INPUT, SSH), sets=("ssh_extension_install",), note="always_install for the case (app relaunched), then never_install again")
decl("sudo_su_real_shell", needs=_merge(INPUT, PROMPT), note="sudo / su shells are built-in subshell matches; proof is `id -u` typed into the inner shell")
# ---- agent
decl("approval_card_pause", needs=_merge(INPUT, AGENT_ASK), reads=MODEL, note="the real CI model must propose the command and the app must stop at its card (OCR)")
decl("md_viewer_ai_answer", needs=INPUT, reads=MODEL, note="the real CI model's answer; OCR is a note, only a crash fails")
decl("paste_agent_input cb_agent_pickers cb_agent_file_mention", needs=INPUT,
     note="the agent input itself (paste, / and @ pickers), judged from the app's own enter lines; no model is needed (the baseline's agent mode is)")
# ---- UI by keyboard and pointer
decl("md_viewer_first_frame", needs=KEYS, note="as md_viewer: opened through the app's own open-file path; judged from the app log (the viewer's "
                                              "first-size line, no image painted without a width), OCR of the heading where there is an engine")
decl("md_viewer_big", needs=KEYS, note="as md_viewer: opened through the app's own open-file path, OCR of the viewer")
decl("ui_palette", needs=_merge(INPUT, KEYS), note="the palette chord and its action names; OCR where there is one, else the app log's frame lines")
decl("ui_mouse_select_copy", needs=KEYS, note="every copy is the copy key after the selection (a sentinel is on the clipboard first), so copy_on_select is not what is judged")
decl("ui_right_click_paste", needs=_merge(INPUT, dict(right_click="context_menu")), note="a right click that pastes has no menu with a Paste entry to click")
decl("ui_drag_drop_file", needs=INPUT)
decl("alt_screen_mouse_reporting", needs=_merge(INPUT, dict(mouse_reporting=True)), note="a click must reach the program that asked for SGR mouse reports")
decl("ui_startup_time cb_state_relaunch", needs=INPUT,
     note="relaunches on the pass profile (restore_session false: the baseline); launch times depend on the whole profile, typing after each launch is exact")
decl("ui_settings_live", needs=INPUT, sets=("system_theme", "theme", "font_size", "vertical_tabs"),
     note="each written live from whatever the profile has (following the system theme is switched off first); the file's bytes are put back")
decl("editor_edit_save editor_paste cb_editor_mac", needs=KEYS, note="the code editor's default keys (save, undo, redo, paste, close pane); judged by the file on disk")
decl("cb_settings_ui_open", needs=_merge(INPUT, KEYS), note="the settings page by its default shortcut; closed by Escape, else the close key or the palette")
decl("cb_settings_migration", needs=INPUT,
     note="replaces the WHOLE settings file with the app stopped (a legacy-shaped copy of the pass's file, then a truncated one: the app then runs "
          "on its defaults) and puts the original bytes back")
decl("cb_ui_action_latency", needs=_merge(INPUT, KEYS), note="new tab, next tab and the Tab menu timed from screenshots; either tab layout changes the screen")
# ---- state on disk
decl("crash_corrupt_db", needs=_merge(INPUT, KEYS), sets=("restore_session",), note="restore_session on for the relaunch on the corrupt database, put back")
decl("upgrade_over_previous", needs=INPUT, note="the previous release is launched on the pass's own profile, then the build under test on its data")
independent("upgrade_from_v0_1_17", "replaces the profile's settings.toml and keybindings.yaml with the checked-in v0.1.17 fixture, judges the upgrade of that, and puts "
                                    "the pass's own files back at the end")
independent("cb_no_autoupdate", "reads the app log of the whole run for updater lines; there is no setting that turns the updater on (DECLINED.md)")

# ================================================================== the ssh lab (round 16, r16-declare): one declaration per family, every arm
# The lab's own harness writes two settings per arm BEFORE the case body and AFTER run.py's check (cases_lab.lab_prepare): the helper install mode
# (never_install for the nohelper / helper arms, always_install for upload / upgrade / never) and the denied-host list, with a relaunch, whenever the
# arm's helper mode differs from the one the app was last launched with. They are NOT put back: the next arm of another mode writes its own. So no
# lab family may NEED a value of ssh_extension_install or helper_hosts (it would be judged against the previous arm's).
LAB = _merge(INPUT, dict(ssh_warpification=True, legacy_ssh_wrapper=True, honor_ps1=False))      # a managed ssh session, lines typed at its prompt
LAB_SETS = ("ssh_extension_install", "helper_hosts")
LAB_NOTE = "ssh_extension_install and helper_hosts are written per helper arm by the lab harness (not put back between arms)"


def lab(globs, needs=None, reads=(), note=""):
    family(globs, needs=_merge(LAB, needs or {}), sets=LAB_SETS, reads=reads, note=(note + "; " if note else "") + LAB_NOTE)


lab("lab_tab_remote_dir.* lab_nohelper_tab_hands_back.* lab_nohelper_listing_single_flight.*", needs=KEYS, note="Tab completion of remote paths by the default Tab key")
lab("lab_path_detect.* lab_enter_start.* lab_enter_no_network_wait.* lab_raw_key_arrival.* lab_ctrl_c_no_esc_i.* lab_ssh_drop_reconnect.* "
    "lab_password_prompt.* lab_nu_agent_pager_env.*", note="typed lines and keys in the ssh session, judged by the host's files, the sshd log and the app log")
lab("lab_chips.* lab_no_helper_chip.* lab_gh_chip_remote.*", note="chips by OCR or the app's chip hop lines; the theme and text size are the seed's and are not read")
lab("lab_helper_install.* lab_helper_quiet_fallback.* lab_helper_digest_mismatch.* lab_helper_missing_asset.* lab_helper_one_version.* lab_helper_idle_exit.* "
    "lab_helper_reconnect.* lab_helper_scp_fallback.* lab_helper_route_unsupported.* lab_helper_killed.* lab_nohelper_unavailable.*",
    note="what the app does about the helper, judged on the host's files, the sshd log and the app log")
lab("lab_helper_policy.*", note="also writes helper_hosts' allowed list (the host allowed under never_install) and relaunches; the lists are emptied at the end")
lab("lab_fam_fs_basic.* lab_fam_git_read.* lab_fam_run_command.* lab_file_tree_remote.* lab_skills_remote.*",
    note="a `cd` into a fixture on the host; judged by the routing lines of the app log and the sshd exec channels")
lab("lab_remote_edit.* lab_diff_panel_remote.* lab_git_write_remote.*", needs=KEYS,
    note="panels opened through the command palette by its default chord and the action's name; the editor's default save key")
lab("lab_codebase_index_remote.*", reads=("codebase_index",) + MODEL, note="needs the fake embeddings provider the harness configures (APPCI_FAKE_EMBEDDINGS)")
lab("lab_read_files_remote.* lab_diff_apply_remote.* lab_agent_remote_flow.* lab_agent_remote_errors.*", reads=MODEL,
    note="the scripted model's tool calls on the host; a card is approved when one shows, so the agent's file-read permission (the seed's) is not needed")
lab("lab_agent_remote_drop.*", needs=AGENT_ASK, reads=MODEL, note="the run_shell_command must stop at its approval card before the connection is reset")
lab("lab_fam_agent_edit_conflict.* lab_fam_agent_delete_conflict.* lab_fam_agent_edit_endings.* lab_fam_agent_edit_touch_only.*", needs=KEYS, reads=MODEL,
    note="as agent_edit_*: the diff card of an edit the scripted model proposes, accepted by the card's default keys")
lab("lab_agent_nld_predict_ssh.*", needs=dict(nld_predict_in_ssh=False), reads=MODEL,
    note="written against the default (no prediction request from an ssh pane, #1982); the local control makes the case UNVERIFIABLE itself when predictions are off")
lab("lab_agent_suggestions_ssh.*", needs=dict(next_command_in_ssh=False, prompt_suggestions_in_ssh=False), reads=MODEL,
    note="written against the defaults (no suggestion request from an ssh pane, #2094); each purpose is judged only against its own local control")
lab("lab_freeze_and_wake.*", reads=MODEL, note="as freeze_and_wake, against the lab host with its link stalled")
lab("lab_replay.*", reads=MODEL, note="the recorded-session replay: typed lines locally and over ssh (terminal pass), agent steps on the scripted model (agent pass)")


# ------------------------------------------------------------------ the table of docs/testing.md
DOC_BEGIN = "<!-- case-settings-table: generated by `python3 script/realapp/appci/case_settings.py --write`; do not edit by hand -->"
DOC_END = "<!-- /case-settings-table -->"


def _fmt_needs(needs):
    def one(n, v):
        if v is APP_DEFAULT or v == APP_DEFAULT:
            return "%s = app default" % n
        if v is ABSENT or v == ABSENT:
            return "%s absent" % n
        alts = v if isinstance(v, (tuple, list, set, frozenset)) else (v,)
        return "%s = %s" % (n, " or ".join("the pass's mode" if (a is PASS_MODE or a == PASS_MODE) else shown(literal(a)) for a in alts))
    return ", ".join(one(n, needs[n]) for n in sorted(needs)) or "-"


def table_order(case_names, fix_names):
    """the rows of the table for the registered case names: [(name, tier)]: the fix tier in its own order (SMOKE, PROVING), then every other exactly
    declared case in registry order (tier full); the lab arms are not rows (table_markdown appends one row per family glob)"""
    fix = list(fix_names)
    seen = set(fix)
    rest = [n for n in case_names if n not in seen and (n in CASE_SETTINGS or n in INDEPENDENT)]
    return [(n, "fix") for n in fix] + [(n, "full") for n in rest]


def table_markdown(order):
    """the per-case table: `order` is table_order(...) ([(case, tier)]), then one row per lab family glob (tier lab). Names and the values the
    CASES need; no value of any seed."""
    rows = ["| case | tier | needs (beyond the baseline) | sets itself, puts back | judges by the value in force | note |", "|---|---|---|---|---|---|"]

    def row(label, tier, d):
        rows.append("| `%s` | %s | %s | %s | %s | %s |" % (label, tier, _fmt_needs(d["needs"]), ", ".join(d["sets"]) or "-", ", ".join(d["reads"]) or "-", d["note"] or ""))
    for n, tier in order:
        if n in INDEPENDENT:
            rows.append("| `%s` | %s | settings-independent | - | - | %s |" % (n, tier, INDEPENDENT[n]))
            continue
        row(n, tier, CASE_SETTINGS[n])
    for g, d in FAMILIES:
        row(g, "lab", d)
    return "\n".join([DOC_BEGIN] + rows + [DOC_END])


SETTINGS_BEGIN = "<!-- settings-names-table: generated by `python3 script/realapp/appci/case_settings.py --write`; do not edit by hand -->"
SETTINGS_END = "<!-- /settings-names-table -->"


def settings_markdown():
    """the named settings: name, TOML key, app default, what the harness does"""
    rows = ["| name | settings.toml key | app default | harness |", "|---|---|---|---|"]
    for n, d in SETTINGS.items():
        rows.append("| %s | `%s` | %s | %s |" % (n, d["path"], shown(d["default"]) if d["default"] is not None else "(not one literal)", d["forced"] or "not touched: the seed's value, else the app default"))
    return "\n".join([SETTINGS_BEGIN] + rows + [SETTINGS_END])


def replace_block(doc, begin, end, block):
    """`doc` with the text from the `begin` marker through the `end` marker replaced by `block` (which carries both markers)"""
    i, j = doc.index(begin), doc.index(end)
    return doc[:i] + block + doc[j + len(end):]


if __name__ == "__main__":
    import cases
    order = table_order([c["name"] for c in cases.CASES], list(cases.SMOKE) + list(cases.PROVING))
    if "--write" in sys.argv:        # rewrite the two generated tables of docs/testing.md ("Test settings model") in place
        path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "docs", "testing.md")
        with open(path, encoding="utf-8") as f:
            doc = f.read()
        doc = replace_block(doc, SETTINGS_BEGIN, SETTINGS_END, settings_markdown())
        doc = replace_block(doc, DOC_BEGIN, DOC_END, table_markdown(order))
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            f.write(doc)
        print("docs/testing.md: both tables rewritten")
    else:
        print(settings_markdown() if "--settings" in sys.argv else table_markdown(order))
