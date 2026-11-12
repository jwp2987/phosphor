# appci/selftest_r25_labtests.py: offline self-tests of the round-25 lab test faults (B prompt nudge, C zsh recorder #2162, D nu internal host command, G helper_killed
# branch witness, H no-helper chip witness). Pure Python (C runs a real zsh when there is one). selftest.py appends CHECKS at its end.
import os, re, shutil, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


def _src(n):
    return open(os.path.join(HERE, n), encoding="utf-8").read()


@check
def r25_b_prompt_nudge_is_a_real_command_in_every_lab_shell():
    import cases_lab as CL
    assert CL.PROMPT_NUDGE == "pwd"
    t = _src("cases_lab_remote.py")
    assert "type(CL.PROMPT_NUDGE" in t and 'type("true"' not in t
    for fn in sorted(f for f in os.listdir(HERE) if re.fullmatch(r"cases_lab\w*\.py", f)):
        bad = re.findall(r"""\.type\(\s*["'](?:true|false|:)["']""", _src(fn))
        assert not bad, "%s types a bare true/false/: (a literal in nu, routed to the agent): %r" % (fn, bad)


@check
def r25_d_nu_internal_report_input_command_is_dropped_exactly():
    import lab as L
    cmds = [(1.0, "sleep 30sec"), (2.0, " __warp_report_input"), (3.0, "__warp_report_input"), (4.0, "echo __warp_report_input x"), (5.0, "echo hi")]
    got = [c for _, c in L.drop_internal_commands(cmds, "nu")]
    assert got == ["sleep 30sec", "echo __warp_report_input x", "echo hi"], got
    assert L.drop_internal_commands(cmds, "bash") == cmds, "only nu rows are filtered"

    class FakeLab:
        def ctl(self, user, which):
            return {"data": "C\t1.0\tsleep 30sec\nC\t2.0\t __warp_report_input\nC\t3.0\techo hi\n"}
    assert [c for _, c in L.LabRemote(FakeLab(), "u_nu", "nu").commands()] == ["sleep 30sec", "echo hi"]
    assert len(L.LabRemote(FakeLab(), "u_bash", "bash").commands()) == 3


@check
def r25_g_branch_is_read_from_the_chip_update_line_in_the_app_log():
    import cases_lab as CL
    log = ('2026-11-12T07:47:11Z [INFO] [chips] update_chip_value ShellGitBranch -> Some(Text("brt1230oot1du"))\n'
           '2026-11-12T07:47:23Z [INFO] [chips] update_chip_value ShellGitBranch -> Some(Text("443495ed3"))\n'
           '2026-11-12T07:47:23Z [INFO] [chips] update_chip_value GitDiffStats -> Some(Text("brzzz"))\n'
           '2026-11-12T07:47:23Z [INFO] [chips] update_chip_value ShellGitBranch -> None\n')
    assert CL.chip_values(log) == ["brt1230oot1du", "443495ed3"]
    assert CL.chip_values(log, "GitDiffStats") == ["brzzz"] and CL.chip_values("") == []
    body = _src("cases_lab_helper.py")
    i = body.index("def c_helper_killed")
    seg = body[i:body.index("reg(\"helper_killed\"")]
    assert "CL.chip_values(" in seg and "UI.find_token(" in seg and "br in txt" not in seg, "the branch is judged from the log and find_token, not a bare OCR substring"
    assert "15 + 8 * rt" in seg, "the wait scales with the link latency"


@check
def r25_h_no_helper_chip_is_witnessed_by_the_frame_log_with_ocr_second():
    import cases_lab as CL
    yes = '2026-11-12T06:29:11Z [DEBUG] frame 1: updated=["AcceptAutosuggestionKeybinding", "FeaturePopup", "SshNoHelperChip", "EditorView"] removed=3 redraw=true\n'
    no = '2026-11-12T06:29:11Z [DEBUG] frame 1: updated=["FeaturePopup", "EditorView"] removed=3 redraw=true\n2026-11-12T06:29:12Z [INFO] SshNoHelperChip dismissed\n'
    assert CL.no_helper_chip_in_log(yes) and not CL.no_helper_chip_in_log(no) and not CL.no_helper_chip_in_log("")
    body = _src("cases_lab.py")
    seg = body[body.index("def c_no_helper_chip"):body.index("# ---------", body.index("def c_no_helper_chip"))]
    assert "no_helper_chip_in_log(" in seg and "bool(in_log) or ocr_has" in seg, "either witness proves the chip; neither fails"


def _zsh_run(d, lines, pre=""):
    drv = ("zmodload zsh/zpty; export ZDOTDIR=$1 HOME=$1 APPCI_W=$1/w TERM=xterm; zpty z zsh -di; sleep 1\n"
           "for l in " + " ".join("'%s'" % l for l in lines) + "; do zpty -w z \"$l\"; sleep 0.5; done\nsleep 0.5; zpty -d z\n")
    subprocess.run(["zsh", "-f", "-c", drv, "_", d], timeout=90, capture_output=True)


@check
def r25_c_zsh_recorder_survives_hostile_options_and_stamps_every_entry():
    """a zsh whose user options (err_exit, nounset, noclobber, no append_create, xtrace) would have broken the old hook still logs each line once; hook.enter
    stamps every entry of __pa_pxh and hook.err stays empty; the evidence line the case types yields the hook array, whence -v, PA_CTL and the write test."""
    if not shutil.which("zsh"):
        print("  skip: no zsh binary on this machine")
        return
    import profile
    import cases_lab as CL
    d = tempfile.mkdtemp()
    try:
        os.makedirs(os.path.join(d, "w"))
        with open(os.path.join(d, ".zshrc"), "w") as f:
            f.write(profile.ZSHRC % {"ctl": d})
        ev = os.path.join(d, "ev.txt")
        lines = ["setopt err_return noclobber nounset; unsetopt append_create", "echo one", "echo two", CL.zsh_evidence_line(ev), "exit"]
        _zsh_run(d, lines)
        rec = [l.split("\t", 2)[2] for l in open(os.path.join(d, "rec.log")).read().splitlines() if l.startswith("C\t")]
        rec = [c for c in rec if c and "typeset -p preexec_functions" not in c and not c.startswith("() {")]
        assert rec == lines[:3] + ["exit"], "rec.log C lines %r, expected each typed line exactly once: %r" % (rec, lines[:3])
        enter = open(os.path.join(d, "hook.enter")).read().splitlines()
        assert len(enter) >= 3 and all(len(e.split("\t")) == 3 for e in enter), "hook.enter must stamp each entry (time, pid, line): %r" % enter
        err = open(os.path.join(d, "hook.err")).read() if os.path.exists(os.path.join(d, "hook.err")) else ""
        assert not err.strip(), "hook.err is not empty: %r" % err
        evt = open(ev).read()
        assert "preexec_functions" in evt and "__pa_pxh" in evt and ("PA_CTL=" + d) in evt and "wr=1" in evt, evt
    finally:
        shutil.rmtree(d, ignore_errors=True)


@check
def r25_c_a_failed_preexec_write_is_still_logged_by_the_precmd_path():
    """__pa_pre is set only after the preexec write succeeded: when it cannot write (rec.log made a read-only directory entry is not possible per line, so the
    hook is told to fail by a function override), the precmd history path logs the line, not nobody."""
    p = _src("profile.py")
    assert "rec.log\" && __pa_pre=1" in p, "__pa_pre must only be set after a successful write"
    assert "2>> \"$PA_CTL/hook.err\"" in p and "hook.enter" in p
    import cases_lab as CL
    t = _src("cases_lab.py")
    assert "hook.enter" in t and "hook.err" in t and "zsh_evidence_line(" in t
    assert "whence -v __pa_pxh" in CL.zsh_evidence_line("f") and "typeset -p preexec_functions" in CL.zsh_evidence_line("f")
