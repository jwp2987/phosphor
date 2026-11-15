# appci/selftest_r32_2155.py: offline self-test of #2155 (a remote ~/.bash_profile that runs `exec zsh -l`): the lab evidence line and its verdict, the
# place the case asks for it, and that the line the pattern reads is the one the app logs. Pure Python.
import inspect, os, re, sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, HERE)
CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


def _src(rel):
    return open(os.path.join(ROOT, rel), encoding="utf-8").read()


@check
def r32_2155_exec_replaced_shell_evidence_verdicts():
    """only p_exec is judged; the line must name bash -> zsh"""
    import cases  # noqa: F401  (cases.py imports cases_lab at its end: it goes first)
    import cases_lab as C
    line = "[INFO] InitShell from the shell that replaced the pending ssh session's (Bash -> Zsh, exec in the remote profile): it keeps the session's ssh identity"
    assert C.exec_replaced_shell_evidence("u_bash", line) is None and C.exec_replaced_shell_evidence("p_tinypath", "") is None
    ok, msg = C.exec_replaced_shell_evidence("p_exec", "x\n" + line + "\ny")
    assert ok and "Bash -> Zsh" in msg
    for text in ("", None, "Received ManagedSSHSessionStarting hook", line.replace("Bash -> Zsh", "Bash -> Bash"), line.replace("Bash -> Zsh", "Zsh -> Bash")):
        ok, msg = C.exec_replaced_shell_evidence("p_exec", text)
        assert not ok and "#2155" in msg, text


@check
def r32_2155_pattern_reads_the_line_the_app_logs():
    """the Rust log line is one string literal split over lines with trailing backslashes: joined, the lab pattern must match it"""
    import re
    import cases  # noqa: F401
    import cases_lab as C
    rs = _src("app/src/terminal/model/terminal_model.rs")
    start = rs.index("\"InitShell from the shell that replaced the pending ssh session's")
    literal = rs[start + 1:rs.index("\",\n", start)]
    joined = re.sub(r"\\\n\s*", "", literal).replace("{:?}", "%s")
    assert joined.count("%s") == 2, joined
    assert C.EXEC_REPLACED_SHELL_RE.search(joined % ("Bash", "Zsh")).groups() == ("Bash", "Zsh")
    # ShellType's Debug names, as the line prints them
    shell = _src("crates/warp_terminal/src/shell/mod.rs")
    enum = shell[shell.index("pub enum ShellType"):]
    enum = enum[:enum.index("}")]
    assert re.search(r"\bBash\b", enum) and re.search(r"\bZsh\b", enum)


@check
def r32_2155_path_detect_case_asks_for_the_evidence():
    """the p_exec arm exists, the case reads the connect's own log, and a missing listing on p_exec is a failure, not UNVERIFIABLE"""
    import cases
    import cases_lab as C
    body = inspect.getsource(C.c_path_detect)
    assert 'exec_replaced_shell_evidence(arm["user"], ctx.log_since(off0), one_conn)' in body
    assert body.index("exec_replaced_shell_evidence(") > body.index("connect_or_fail(")
    assert "elif proof is not None:" in body and "r.check(False, \"p_exec: no hop=cwd_listing" in body
    names = {c["name"] for c in cases.CASES}
    assert "lab_path_detect.p_exec.lan.nohelper" in names


@check
def r32_2155_lab_user_and_bootstrap_block_are_in_place():
    """the lab user's profile is the reported one, and the bash bootstrap carries the hand-over (markers the Rust unit test extracts by)"""
    users = _src("script/realapp/sshlab/rootfs/usr/local/lib/sshlab/users.tsv")
    assert "p_exec\tbash\texec\t" in users
    labd = _src("script/realapp/sshlab/rootfs/usr/local/sbin/labd.py")
    assert "'exec zsh -l\\n' if prof == \"exec\"" in labd
    boot = _src("app/assets/bundled/bootstrap/bash_body.sh")
    for marker in (": __warp_exec_zsh_begin", ": __warp_exec_zsh_end", ": __warp_exec_zsh_done_begin", ": __warp_exec_zsh_done_end"):
        assert boot.count(marker) == 1, marker
    block = boot[boot.index(": __warp_exec_zsh_begin"):boot.index(": __warp_exec_zsh_end")]
    assert "alias exec='__WARP_EXEC_ZSH=$(__warp_exec_zsh_info) ZDOTDIR=$(__warp_exec_zsh_zdotdir) exec'" in block
    assert "${WARP_IS_SSH-} == 1" in block and "${PHOSPHOR_SSH_EXEC_ZSH-} != 0" in block
    assert boot.index("source \"$HOME/.bash_profile\"") > boot.index(": __warp_exec_zsh_end")
    assert boot.index(": __warp_exec_zsh_done_begin") > boot.index("source \"$HOME/.profile\"")


@check
def r32_2230_one_connection_p_exec_is_a_plain_session():
    """#2230: on the Windows Git Bash pane p_exec must show the plain-login hop and NOT the #2155 takeover line; everywhere else the #2155 line is demanded"""
    import cases  # noqa: F401
    import cases_lab as C
    take = "InitShell from the shell that replaced the pending ssh session's (Bash -> Zsh, exec in the remote profile)"
    plain = "[DEBUG] input_order hop=ssh_wrapper replaced_shell_plain holding=false held_bytes=0 t=1.0"
    assert C.exec_replaced_shell_evidence("u_bash", plain, True) is None
    ok, msg = C.exec_replaced_shell_evidence("p_exec", "a\n" + plain + "\nb", True)
    assert ok and "#2230" in msg
    for text in ("", None, take, plain + "\n" + take, "hop=ssh_wrapper replaced_shell_plainer"):
        ok, msg = C.exec_replaced_shell_evidence("p_exec", text, True)
        assert not ok, text
    # the ControlMaster arms are unchanged: the plain hop is no proof there, the takeover line is
    assert not C.exec_replaced_shell_evidence("p_exec", plain, False)[0] and C.exec_replaced_shell_evidence("p_exec", take, False)[0]
    assert C.exec_replaced_shell_evidence("p_exec", take)[0]

    class Ctx:
        def __init__(self, win, pane):
            self.win, self.pane = win, pane
    assert C.one_connection_pane(Ctx(True, "gitbash"))
    assert not C.one_connection_pane(Ctx(True, "pwsh")) and not C.one_connection_pane(Ctx(False, "gitbash")) and not C.one_connection_pane(object())
    why = C.exec_replaced_shell_plain_limit("p_exec", True)
    assert why and "#2230" in why and C.EXEC_REPLACED_SHELL_PLAIN_DECISION in why
    assert os.path.exists(os.path.join(ROOT, C.EXEC_REPLACED_SHELL_PLAIN_DECISION.split(" ")[0]))
    assert C.exec_replaced_shell_plain_limit("p_exec", False) is None and C.exec_replaced_shell_plain_limit("u_bash", True) is None


@check
def r32_2230_path_detect_leaves_the_plain_arm_before_the_managed_checks():
    """the case returns through finish_remote on the plain arm, before it types the cd; and the hop the pattern reads is the one the view logs"""
    import cases  # noqa: F401
    import cases_lab as C
    body = inspect.getsource(C.c_path_detect)
    gate = body.index("plain_why = exec_replaced_shell_plain_limit(")
    assert body.index("exec_replaced_shell_evidence(") < gate < body.index('ctx.be.type("cd %spd%s"')
    block = body[gate:body.index("time.sleep(2.0 + 3 * rt)")]
    assert "r.unverifiable(plain_why)" in block and "finish_remote(ctx, r)" in block and "return" in block
    view = _src("app/src/terminal/view.rs")
    at = view.index("fn ssh_on_replaced_shell_plain(")
    assert 'log_hop("ssh_wrapper"' in view[at:at + 1600] and '"replaced_shell_plain holding={} held_bytes={} window={}"' in view[at:at + 1600]
    # review of 5341ccb97: the release has its own name and the delivery its own finish hop (no give-up, no failed connect in the log)
    phase = _src("app/src/terminal/input_queue/ssh_phase.rs")
    assert 'SshRelease::ReplacedShellPlain => "replaced_shell_plain"' in phase
    deliver = view[view.index("fn deliver_held_ssh_typeahead_to_plain_session("):]
    deliver = deliver[:deliver.index("\n    }\n")]
    assert '"source=replaced_shell_plain held_bytes={}"' in deliver and '"delivered=plain_session"' in deliver and "give_up" not in deliver
    # cases.py reads the finish hops by these shapes: the held count from kh_finish_begin, the outcome from kh_finish_end
    import cases
    assert cases._SSH_FINISH_RE.search("hop=kh_finish_begin source=replaced_shell_plain held_bytes=3").group(1) == "3"
    assert re.search(r"hop=kh_finish_(?:begin|end)\b.*?\b((?:delivered|give_up)=\w+)", "hop=kh_finish_end delivered=plain_session").group(1) == "delivered=plain_session"
    boot = _src("app/assets/bundled/bootstrap/bash_body.sh")
    assert "WARP_SSH_PLAIN_HOOK='$warp_plain_hex'" in boot and "if [[ $warp_single -eq 1 ]]; then" in boot
