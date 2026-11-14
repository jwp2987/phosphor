# appci/selftest_r30_ctlreader.py: offline self-tests of #2213 (r30): the lab ctl reader pages the WHOLE file and fails loudly, the zsh bootstrap line is an
# internal command, and the zsh recorder never logs an empty C line. Pure Python, no display, no zsh needed.
import os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


class ChunkLab:
    """what the CURRENT labd does: size + at most 1,000,000 BYTES from `since`, decoded with errors replaced"""

    def __init__(self, raw, size=None):
        self.raw, self.size, self.calls = raw, size, 0

    def ctl(self, user, name, since=0):
        self.calls += 1
        return {"size": len(self.raw) if self.size is None else self.size,
                "data": self.raw[min(since, len(self.raw)):][:1_000_000].decode("utf-8", "replace")}


def _lines(n, pad=0):
    return "".join("C\t%d.0\tcmd %d %s\n" % (i, i, "é" * pad) for i in range(n))


@check
def r30_ctl_reader_pages_a_2_5mb_file_whole_2213():
    import lab as L
    text = _lines(100000, pad=3)
    raw = text.encode("utf-8")
    assert len(raw) > 2_400_000, len(raw)
    fake = ChunkLab(raw)
    got = L.read_ctl_all(fake, "u_zsh", "rec.log")
    assert got == text and fake.calls >= 3, fake.calls
    assert got.splitlines() == text.splitlines()
    rem = L.LabRemote(ChunkLab(raw), "u_zsh", "zsh")
    cmds = rem.commands()
    assert len(cmds) == 100000 and cmds[-1][1].startswith("cmd 99999")
    assert L.read_ctl_all(ChunkLab(b""), "u", "x") == ""


@check
def r30_ctl_reader_size_mismatch_raises_2213():
    import lab as L
    raw = _lines(60000).encode()
    for fake in (ChunkLab(raw, size=len(raw) + 7), ChunkLab(raw, size=len(raw) - 7)):
        try:
            L.read_ctl_all(fake, "u_zsh", "rec.log")
        except L.LabError as e:
            assert "ctl u_zsh/rec.log" in str(e), e
        else:
            raise AssertionError("a size mismatch returned a file")
    bad = _lines(100).encode() + b"C\t1\t\xff\xfe\xff\n" + _lines(100).encode()
    try:
        L.read_ctl_all(ChunkLab(bad), "u_zsh", "rec.log")
    except L.LabError:
        pass
    else:
        raise AssertionError("invalid UTF-8 shifted the offsets silently")
    try:
        L.read_ctl_all(ChunkLab(b"x" * 1_500_000), "u", "n")
    except L.LabError:
        pass
    else:
        raise AssertionError("a chunk without a newline was accepted")


@check
def r30_zsh_bootstrap_line_is_internal_bash_untouched_2213():
    import lab as L
    boot = "read -r -d '' WARP_BOOTSTRAP_VAR << 'EOM'\\n" + "x" * 64000
    keep = "echo read -r -d '' WARP_BOOTSTRAP_VAR << 'EOM'"
    cmds = [(1.0, "echo a"), (2.0, boot), (3.0, keep), (4.0, "echo b")]
    for sh in ("zsh", "zshc"):
        assert [c for _, c in L.drop_internal_commands(cmds, sh)] == ["echo a", keep, "echo b"], sh
    assert L.drop_internal_commands(cmds, "bash") == cmds
    assert L.drop_internal_commands(cmds, "fish") == cmds
    # no case depends on seeing the lab line: the only users of the name are the local-pane filter in cases.py (a different, `command -p stty` form)
    import glob
    for p in glob.glob(os.path.join(HERE, "cases*.py")):
        t = open(p, encoding="utf-8").read()
        assert "read -r -d '' WARP_BOOTSTRAP_VAR" not in t, p


@check
def r30_zsh_hook_logs_no_empty_c_line_2213():
    import profile as P
    pre = [l for l in P.ZSHRC.splitlines() if "$HISTCMD != $__pa_n || -n $__pa_ran" in l]
    assert len(pre) == 1 and "[[ -n $c ]] && print -r" in pre[0], pre
    # bash: the first prompt only adopts the history number, so it cannot log an empty first line
    assert 'if [ -n "$__pa_first" ]; then __pa_n=$n' in P.BASHRC


@check
def r30_profile_commands_remote_filters_via_lab_remote_2213():
    import lab as L
    import profile as P
    boot = "read -r -d '' WARP_BOOTSTRAP_VAR << 'EOM'\\n" + "x" * 64000
    raw = ("C\t1.0\techo a\nC\t2.0\t" + boot + "\nC\t3.0\techo b\n").encode()
    pr = P.Profile.__new__(P.Profile)
    pr.remote = L.LabRemote(ChunkLab(raw), "u_zsh", "zsh")
    assert [c for _, c in pr.commands(remote=True)] == ["echo a", "echo b"]
    pr.remote = L.LabRemote(ChunkLab(raw), "u_bash", "bash")
    assert [c for _, c in pr.commands(remote=True)] == ["echo a", boot, "echo b"]


@check
def selftest_main_guard_is_last_2214():
    # #2214: every line after `if __name__ == "__main__":` in selftest.py is dead under `python3 selftest.py` (the CI form); r26-r30 registered there and never ran
    import ast
    tree = ast.parse(open(os.path.join(HERE, "selftest.py"), encoding="utf-8").read())
    last = tree.body[-1]
    assert isinstance(last, ast.If) and "__name__" in ast.unparse(last.test), "selftest.py: the __main__ guard is not the last statement"
    guards = [n for n in tree.body if isinstance(n, ast.If) and "__name__" in ast.unparse(n.test)]
    assert len(guards) == 1, "selftest.py has %d __main__ guards" % len(guards)
