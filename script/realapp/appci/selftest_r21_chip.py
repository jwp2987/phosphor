# appci/selftest_r21_chip.py: offline self-tests of round 21, #2138 (docs/design/r21-chip.md): cases_chip.py, the scripted model's tagged prompt
# suggestion (fake_model.py) and the app strings the cases read. Pure Python: no app, no display.
# selftest.py appends CHECKS to its own list (two lines at its end), so `run.py --self-test` runs them.
# Every judge is fed a right input AND the wrong ones it exists to catch: an oracle that cannot fail is not a test.
import inspect, json, os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.join(HERE, "..", "..", "..")
sys.path.insert(0, HERE)

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


def _src(*parts):
    return open(os.path.join(REPO, *parts), encoding="utf-8").read()


def _find(token, y, x0=120.0, x1=260.0, h=32.0):
    return "t [DEBUG] appci_find token=%s x0=%.1f y0=%.1f x1=%.1f y1=%.1f\n" % (token, x0, y, x1, y + h)


def _chip(shown, hidden=0, place="chip", drawn=None, width=None):
    drawn = (1 if shown and not hidden and place == "chip" else 0) if drawn is None else drawn
    width = (400 if drawn else 0) if width is None else width
    return "t [DEBUG] input_order hop=prompt_chip shown=%d hidden=%d place=%s drawn=%d width=%d t=1\n" % (shown, hidden, place, drawn, width)


def _accept(source="Keybinding"):
    return "t [DEBUG] input_order hop=prompt_chip_accept source=%s t=1\n" % source


@check
def r21_chip_cases_registered_proving_and_classified():
    """both cases are registered once, PROVING (never SMOKE), the scripted one in PROVING_FAKE and agent-only, declared in the settings model with
    prompt suggestions required (the scripted one also with the default chords: it presses the accept key), classified for Windows; the banner
    cases of the unmerged fix/r21-banner are not registered; docs/testing.md names the cases, the switch and this file"""
    import cases, case_settings, winmap
    reg = {c["name"]: c for c in cases.CASES}
    both, scripted = "suggestion_chip_keeps_output", "agent_suggestion_chip_keeps_output"
    for n in (both, scripted):
        assert [c["name"] for c in cases.CASES].count(n) == 1 and n in cases.PROVING and n not in cases.SMOKE, n
        d = case_settings.declaration(n)
        assert d and d["needs"].get("prompt_suggestions") is True, "%s must require prompt suggestions on: with them off no suggestion is asked for" % n
        assert winmap.skip_reason(n, "gitbash") is None and "#2138" in winmap.skip_reason(n, "powershell"), n
    assert case_settings.declaration(scripted)["needs"].get("keybindings_file") == "absent", "the accept key is pressed as its default chord"
    assert tuple(reg[both]["modes"]) == ("terminal", "agent") and tuple(reg[scripted]["modes"]) == ("agent",)
    assert scripted in cases.PROVING_FAKE and both not in cases.PROVING_FAKE, "the both-modes case runs on the pass's own model, the scripted one in agent@fake"
    assert len(set(cases.PROVING)) == len(cases.PROVING), "PROVING lists a case twice"
    for old in ("suggestion_banner_keeps_output", "agent_suggestion_banner_keeps_output"):
        assert old not in reg and old not in cases.PROVING, "%s is the unmerged banner-slot case" % old
    assert case_settings.SETTINGS["prompt_suggestions"]["path"] == "agents.warp_agent.active_ai.agent_mode_query_suggestions_enabled"
    assert 'toml_path: "agents.warp_agent.active_ai.agent_mode_query_suggestions_enabled"' in _src("app", "src", "settings", "ai.rs")
    doc = _src("docs", "testing.md")
    sec = doc[doc.index("## Round 21: the prompt suggestion is a chip"):]
    for word in ("`suggestion_chip_keeps_output`", "`agent_suggestion_chip_keeps_output`", "PHOSPHOR_PROMPT_SUGGESTION_CHIP", "hop=prompt_chip", "hop=prompt_chip_accept",
                 "selftest_r21_chip.py", "suggestion_chip_tests.rs", "suggestion_chip_view_tests.rs", "suggestiontag_", "APPCI_CHIP_WAIT_S", "#2138"):
        assert word in sec, "docs/testing.md (Round 21) does not mention %s" % word
    table = doc[doc.index("### 2.6 Environment switches"):doc.index("### 2.7")]
    assert "PHOSPHOR_PROMPT_SUGGESTION_CHIP=0" in table, "docs/testing.md 2.6 does not list the kill switch"


@check
def r21_chip_app_strings_are_the_ones_the_case_reads():
    """the hops' names and fields, the switch's name, the accept chord and the find hook's line are the app's own (a renamed field would turn every
    run UNVERIFIABLE, a changed chord would fail the accept)"""
    import cases_chip as B, cases_ui as U
    rs = _src("app", "src", "terminal", "input.rs")
    assert 'log_hop("prompt_chip"' in rs and '"shown={} hidden={} place={} drawn={} width={:.0}"' in rs, "input.rs no longer logs hop=prompt_chip shown= hidden= place= drawn= width="
    view = _src("app", "src", "terminal", "view.rs")
    assert 'log_hop("prompt_chip_accept"' in view and 'format!("source={interaction_source:?}")' in view, "view.rs no longer logs hop=prompt_chip_accept source="
    assert "    Button,\n    Keybinding,\n" in _src("app", "src", "server", "telemetry.rs"), "InteractionSource no longer prints as Button / Keybinding"
    chip = _src("app", "src", "terminal", "input", "suggestion_chip.rs")
    assert '"PHOSPHOR_PROMPT_SUGGESTION_CHIP"' in chip and '=> "chip"' in chip and '=> "banner"' in chip
    init = _src("app", "src", "terminal", "view", "init.rs")
    at = init.index("ACCEPT_PROMPT_SUGGESTION_KEYBINDING,\n")
    binding = init[at:at + 900]
    assert '"ctrl-enter"' in binding and '"alt-shift-enter"' in binding, "the default accept chord changed: cases_chip.accept_key presses the old one"
    assert B.accept_key("mac") == "ctrl+Return" and B.accept_key("x11") == B.accept_key("win") == "alt+shift+Return"
    events = B.chip_events(_chip(1) + _chip(1, 1) + _chip(1, 0, "banner") + _chip(1, 0, "chip", 0, 0) + _chip(0))
    assert [e[1:] for e in events] == [(True, False, "chip", True, 400), (True, True, "chip", False, 0), (True, False, "banner", False, 0),
                                       (True, False, "chip", False, 0), (False, False, "chip", False, 0)], events
    assert events[0][0] < events[1][0] < events[4][0] and B.first_shown(events) == events[0] and B.first_gone(events, events[0][0]) == events[4]
    assert B.first_shown(events, events[0][0]) == events[2], "the next shown event after an offset is the next chip"
    assert B.first_shown(B.chip_events(_chip(1, 1) + _chip(0))) is None, "a hidden suggestion (a coding suggestion waiting for its diff) is not on screen"
    assert B.accept_sources(_accept() + _chip(0) + _accept("Button")) == ["Keybinding", "Button"] and B.accept_sources("") == []
    assert B.find_rows(_find("bnabc", 1276.0) + _find("other", 5.0) + _find("bnabc", 1198.0), "bnabc") == [(120.0, 1276.0, 260.0, 1308.0), (120.0, 1198.0, 260.0, 1230.0)]
    assert B.CHIP_RE.pattern.startswith("hop=prompt_chip ") and B.ACCEPT_RE.pattern.startswith("hop=prompt_chip_accept ") and U.FIND_RE.pattern.startswith("appci_find token=")


@check
def r21_chip_judge_passes_a_still_row_and_fails_each_move():
    """judge(): the round 15 macOS numbers (y 1276 -> 1198 when the suggestion appeared) FAIL; a row that stays passes only with a control that saw
    the hook report a move; the suggestion going is judged in terminal mode; every missing piece of evidence is UNVERIFIABLE, never a pass"""
    import cases_chip as B
    tok = "bnabc"
    ctl = _find(tok, 1180.0)
    still = _find(tok, 1276.0) + _chip(1) + _chip(0)
    v = B.judge(still, ctl, tok, "terminal", 15.0)
    assert v["status"] == "pass" and v["metrics"]["place"] == "chip" and v["metrics"]["drawn"] is True and v["metrics"]["width_px"] == 400, v
    assert v["metrics"]["control_moves"] == 1 and v["metrics"]["row_gone"][1] == 1276.0, v
    # #2138 as filed (the kill switch's layout): the block moved up 78 px in the frame of the banner
    old = _find(tok, 1276.0) + _chip(1, 0, "banner") + _find(tok, 1198.0)
    v = B.judge(old, ctl, tok, "terminal", 15.0)
    assert v["status"] == "fail" and "-78.0 px" in v["why"] and "appeared" in v["why"] and "place=banner" in v["why"], v
    v = B.judge(old, "", tok, "agent", 15.0)
    assert v["status"] == "fail", "a move fails whatever the control says: %r" % (v,)
    # a chip that moved the row fails all the same
    v = B.judge(_find(tok, 1276.0) + _chip(1) + _find(tok, 1260.0), ctl, tok, "agent", 15.0)
    assert v["status"] == "fail" and "-16.0 px" in v["why"] and "place=chip" in v["why"], v
    # the suggestion going moves the row back: judged in terminal mode
    back = _find(tok, 1276.0) + _chip(1) + _chip(0) + _find(tok, 1354.0)
    v = B.judge(back, ctl, tok, "terminal", 15.0)
    assert v["status"] == "fail" and "+78.0 px" in v["why"] and "went" in v["why"], v
    # both halves fail together
    v = B.judge(_find(tok, 1276.0) + _chip(1) + _find(tok, 1198.0) + _chip(0) + _find(tok, 1276.0), ctl, tok, "terminal", 15.0)
    assert v["status"] == "fail" and "appeared" in v["why"] and "went" in v["why"], v
    # another copy of the word (another x, another height) is not the row
    quoted = _find(tok, 1276.0) + _chip(1) + _find(tok, 1300.0, x0=40.0, x1=180.0, h=24.0)
    v = B.judge(quoted, ctl, tok, "agent", 15.0)
    assert v["status"] == "pass" and v["metrics"]["row_showing"][1] == 1276.0 and any("agent view" in n for n in v["notes"]), v
    # a sighting logged BEFORE the suggestion's line is the command's own layout settling, not a move
    settle = _find(tok, 1400.0) + _find(tok, 1276.0) + _chip(1)
    assert B.judge(settle, ctl, tok, "agent", 15.0)["status"] == "pass"
    # no evidence is never a pass
    v = B.judge(_find(tok, 1276.0), "", tok, "terminal", 45.0)
    assert v["status"] == "unverifiable" and "no prompt suggestion" in v["why"] and "45 s" in v["why"], v
    v = B.judge(_find(tok, 1276.0) + _chip(1, 1), "", tok, "terminal", 45.0)
    assert v["status"] == "unverifiable" and "1 prompt_chip line(s)" in v["why"], "a suggestion that stayed hidden was never on screen: %r" % (v,)
    v = B.judge(_chip(1), ctl, tok, "terminal", 1.0)
    assert v["status"] == "unverifiable" and "find hook logged no position" in v["why"], v
    v = B.judge(still, "", tok, "terminal", 15.0)
    assert v["status"] == "unverifiable" and "control" in v["why"], "a hook that reports no move for a new block proves nothing: %r" % (v,)
    v = B.judge(still, _find(tok, 1276.0), tok, "terminal", 15.0)
    assert v["status"] == "unverifiable", "a control sighting at the same place is not a move: %r" % (v,)
    # Escape that did not drop the suggestion: the appearing half still stands, with a note
    v = B.judge(_find(tok, 1276.0) + _chip(1), ctl, tok, "terminal", 15.0)
    assert v["status"] == "pass" and any("Escape did not drop" in n for n in v["notes"]) and "row_gone" not in v["metrics"], v
    assert B.moved((0, 10.0, 5, 20.0), (0, 10.4, 5, 20.4)) == 0.0 and B.moved((0, 10.0, 5, 20.0), (0, 11.0, 5, 21.0)) == 1.0


@check
def r21_chip_text_and_accept_judges():
    """judge_text(): the chip's text read from the screen passes; a text OCR did not read, a missing engine, a chip the app did not draw and the
    banner placement are UNVERIFIABLE (never a pass, never a fail) and say what the same read made of the printed row. judge_accept(): only the
    app's own `source=Keybinding` line after the key passes; a click's line, or none, fails. worst(): fail > unverifiable > pass."""
    import cases_chip as B
    label, tok = B.chip_label("acdef234"), "bnabc"
    assert label == "Explain chip acdef234"
    shown = B.first_shown(B.chip_events(_chip(1)))
    screen = "~/w $ cat suggestiontag_acdef234.txt error[E0432] see https://192.0.2.7/acdef234 suggestiontag_acdef234 row bnabc"
    assert B.judge_text(shown, screen + " Explain chip acdef234 Alt Shift Enter", label, tok) == ("pass", "")
    assert B.judge_text(shown, screen + " Exp1ain chip acdef234", label, tok)[0] == "pass", "one misread glyph is still the chip's text"
    s, why = B.judge_text(shown, screen, label, tok)
    assert s == "unverifiable" and "found the printed row's word" in why and "screen was readable" in why, "the tag in the file's name is not the chip: %r" % (why,)
    s, why = B.judge_text(shown, "no words here", label, tok)
    assert s == "unverifiable" and "did not find" in why and "OCR cannot read this screen" in why, why
    s, why = B.judge_text(shown, None, label, tok)
    assert s == "unverifiable" and "no OCR engine" in why, why
    s, why = B.judge_text(B.first_shown(B.chip_events(_chip(1, 0, "chip", 0, 0))), screen + " Explain chip acdef234", label, tok)
    assert s == "unverifiable" and "drawn=0" in why, "the app said it drew no chip: nothing read on screen can be the chip (%r)" % (why,)
    s, why = B.judge_text(B.first_shown(B.chip_events(_chip(1, 0, "banner"))), screen, label, tok)
    assert s == "unverifiable" and "place=banner" in why and "PHOSPHOR_PROMPT_SUGGESTION_CHIP" in why, why
    assert B.judge_accept(_chip(0) + _accept(), "alt+shift+Return") == ("pass", "")
    s, why = B.judge_accept(_chip(0), "alt+shift+Return")
    assert s == "fail" and "alt+shift+Return" in why and "accept lines: []" in why, why
    s, why = B.judge_accept(_accept("Button"), "ctrl+Return")
    assert s == "fail" and "['Button']" in why, "a click's accept line is not the key's: %r" % (why,)
    assert B.worst("pass", "pass") == "pass" and B.worst("pass", "unverifiable") == "unverifiable" and B.worst("unverifiable", "fail", "pass") == "fail"


@check
def r21_chip_case_body_reads_the_log_in_order():
    """the body waits for the suggestion by the app's line, reads the chip's text only with the scripted model, takes the main text before the control
    command, plays the going half in terminal mode only, presses the accept key on a second chip after the control, and turns the worst verdict
    into the right kind of row"""
    import cases_chip as B
    src = inspect.getsource(B.play)
    assert src.index("P.print_file(") < src.index("first_shown(chip_events(ctx.log_since(seen)))") < src.index('ctx.ocr("chip_shown")') < src.index("ctl_off = ctx.log_size()")
    assert src.index("ctl_off = ctx.log_size()") < src.index('U.run_cmd(ctx, "echo bnctl_%s" % tag)') < src.index("play_accept(ctx, r, fname, tag, token, limit)")
    assert 'if ctx.mode == "terminal":' in src and 'ctx.be.key("escape")' in src and "if scripted and up:" in src
    assert src.index('if status == "fail":') < src.index('r.check(False, "; ".join(whys))') < src.index('r.unverifiable("; ".join(whys))')
    acc = inspect.getsource(B.play_accept)
    assert acc.index('U.run_cmd(ctx, "cat %s" % fname)') < acc.index("first_shown(chip_events(ctx.log_since(off)))") < acc.index("ctx.be.key(key)") < acc.index("judge_accept(ctx.log_since(key_off), key)")
    assert acc.count('return "unverifiable"') == 2, "no second chip, or a cat that did not reach the shell, is not a failed accept"
    if not os.environ.get("APPCI_CHIP_WAIT_S"):
        assert B.wait_s(False) == B.DEFAULT_WAIT_S > B.wait_s(True) == B.SCRIPTED_WAIT_S > 0
    text = B.fixture_text("TAGx1", "bnabc")
    assert text.endswith("row bnabc\n") and "suggestiontag_TAGx1\n" in text and text.count("bnabc") == 2 and "Explain" not in text
    assert "fake = FM.ACTIVE" in inspect.getsource(B.c_agent_suggestion_chip_keeps_output) and "r.skip(" in inspect.getsource(B.c_agent_suggestion_chip_keeps_output)


@check
def r21_fake_model_suggests_only_for_the_tagged_last_block():
    """fake_model: a prompt-suggestions request whose LAST block names suggestiontag_<TAG> gets a "simple" suggestion, late; a tag in an older block,
    no tag, or another purpose gets what it always got. The request text is shaped like the app's template. The suggestion's text is the one the
    case reads from the screen."""
    import fake_model as FM
    import cases_chip as B
    assert "\n- $ {{ b.command }}" in _find_template("prompt_suggestions_user.j2"), "the app's request template no longer starts a block with `- $ <command>`: suggestion_tag reads the last one"

    def user(blocks):
        return "# Recent command(s)" + "".join("\n- $ %s\n  pwd: /w\n  exit: 0\n  output:\n    %s" % b for b in blocks) + "\n\nLast exit code: 0\n\nWhat follow-up?"
    sysm = {"role": "system", "content": "You are inside a terminal emulator. The user just finished a command.\nPredict the single most useful follow-up"}

    def msgs(blocks):
        return [sysm, {"role": "user", "content": user(blocks)}]
    tagged = msgs([("ls", "a b"), ("cat suggestiontag_Ab12.txt", "row x")])
    assert FM.classify_purpose("/api/chat", {"messages": tagged}) == "prompt_suggestions"
    assert FM.suggestion_tag(user([("ls", "a"), ("cat suggestiontag_Ab12.txt", "row x")])) == "Ab12"
    assert FM.suggestion_tag(user([("cat f.txt", "suggestiontag_Zz9\n    row x")])) == "Zz9", "the tag in the block's output counts too"
    assert FM.suggestion_tag(user([("cat suggestiontag_Ab12.txt", "row x"), ("echo next", "next")])) is None, "a tag of an OLDER block must not put a chip into every later command"
    assert FM.suggestion_tag(user([("ls", "a")])) is None and FM.suggestion_tag("") is None and FM.suggestion_tag(None) is None
    got = json.loads(FM.purpose_reply("prompt_suggestions", tagged)["text"])
    assert got == {"kind": "simple", "query": B.chip_label("Ab12"), "should_plan_task": False}, got
    assert FM.purpose_reply("prompt_suggestions", msgs([("ls", "a")]))["text"] == '{"kind":"none"}' and FM.purpose_reply("prompt_suggestions")["text"] == '{"kind":"none"}'
    assert FM.purpose_reply("next_command", tagged)["text"] == "echo fake-next"
    assert FM.suggestion_delay("prompt_suggestions", tagged) == FM.SUGGESTION_DELAY_S > 0 and FM.suggestion_delay("prompt_suggestions", msgs([("ls", "a")])) == 0.0
    assert FM.suggestion_delay("chat", tagged) == 0.0 and FM.suggestion_delay("next_command", tagged) == 0.0
    # the app's own schema names the kind the fake answers with
    assert '{"kind":"simple","query":"…","should_plan_task":<bool>}' in _find_template("prompt_suggestions_system.j2")


def _find_template(name):
    for root, _dirs, files in os.walk(os.path.join(REPO, "app")):
        if name in files and os.sep + "target" + os.sep not in root:
            return open(os.path.join(root, name), encoding="utf-8").read()
    raise AssertionError("the app's %s was not found" % name)
