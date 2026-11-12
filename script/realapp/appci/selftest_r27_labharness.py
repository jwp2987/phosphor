# appci/selftest_r27_labharness.py: offline self-tests of the r27 lab-harness fixes (r25a Linux lab: nu rejected `kill -9 $nu.pid`, the ssh_drop kill arm never
# dropped and typed "local" commands into the live remote nu). Per-shell line builders and the drop-not-ended handling. No display, no lab.
import os, sys, types

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


@check
def t_kill_line_per_shell():
    import cases_lab as CL
    assert CL.kill_self_line({"shell": "nu"}) == "kill -s 9 $nu.pid"
    assert CL.kill_self_line("nu") == "kill -s 9 $nu.pid"
    assert CL.kill_self_line("fish") == "kill -9 $fish_pid"
    for sh in ("bash", "zsh"):
        assert CL.kill_self_line(sh) == "kill -9 $$", sh
    assert "-9" not in CL.kill_self_line("nu"), "nu rejects the -9 shorthand"


@check
def t_ls_home_line():
    import cases_lab as CL
    assert CL.ls_home_line("bash", ".agents/skills") == "ls $HOME/.agents/skills"
    assert CL.ls_home_line("fish", ".agents/skills") == "ls $HOME/.agents/skills"
    nu = CL.ls_home_line({"shell": "nu"}, ".agents/skills")
    assert "$HOME" not in nu and "$env.HOME" in nu and ".agents/skills" in nu, nu


@check
def t_no_bash_only_redirect_left():
    # a typed `echo X > file` is wrong on nu (a comparison); the lab cases build appends through append_line
    src = open(os.path.join(HERE, "cases_lab.py")).read()
    assert 'ctx.be.type("echo %s > %s"' not in src
    sk = open(os.path.join(HERE, "cases_lab_skills.py")).read()
    assert 'type("ls $HOME' not in sk


class _Ctx:
    """a fake ctx for ssh_drop_body: the drop line does nothing (no new prompt), the pane stays remote"""
    def __init__(self):
        self.typed, self.resets, self.arm = [], 0, {"shell": "nu", "user": "u_nu"}
        self.left_unproven = ""
        self.be = types.SimpleNamespace(type=lambda t, *a, **k: self.typed.append(t), key=lambda k: None)

    def log_size(self): return 0
    def log_since(self, off): return ""
    def prompts(self): return 1
    def until(self, f, *a, **k): return False
    def shot(self, *a): pass
    def reset(self): self.resets += 1
    def ssh_diag(self): return ""
    def where_now(self): return "remote"
    def count(self, n): return n
    def cmd_mark(self): return 0
    def cmds_since(self, mark): return []


@check
def t_kill_arm_not_ended_types_nothing_more():
    import cases_coverage_a as CA
    ctx = _Ctx()
    notes, checks = [], []
    r = types.SimpleNamespace(check=lambda ok, m="": checks.append((ok, m)), note=lambda m: notes.append(m), metric=lambda *a: None,
                              unverifiable=lambda m: None)
    CA.CR.connect = lambda ctx, host, t: 0.1
    CA.CR.leave = lambda ctx: True
    CA.remote_lines_exact = lambda *a, **k: None
    CA.local_cmd_exact = lambda *a, **k: (_ for _ in ()).throw(AssertionError("typed a local command into the remote shell"))
    CA.ssh_drop_body(ctx, r, "h", ("kill",))
    assert ctx.typed == ["kill -s 9 $nu.pid"], ctx.typed
    assert any(not ok and "no local prompt" in m for ok, m in checks), checks
    assert ctx.resets >= 1 and any("proven" in n for n in notes), notes
