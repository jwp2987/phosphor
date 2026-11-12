# appci/cases_lab_skills.py: lab case for the remote agent context (Wave H parity P8, docs/design/remote-helper-parity.md family h), registered through lab_case.
#   lab_skills_remote   a skill and a global rules file on the remote host reach the app: pushed by the helper (upload arm) or read by the shell backend over the
#                       ControlMaster (nohelper arm). Bundled skills come from the app, never from the host.
# Shells: bash, zsh, fish, nu, dash, ash, pwsh (PowerShell source for the home/skills/rules reads, #1874). Terminal mode: the feature is about the remote host's context, not the agent loop. Oracle: the app's own log line
# `hop=agent_context source=<helper|shell> revision=N skills=S rules=R names_hash=[...]` (manager.rs log_agent_context; counts and the FNV-1a hash of each skill's directory name
# only), plus the host's files (made over a separate ssh connection, the way an editor or `git clone` would) and the remote shell's rec.log (the feature types nothing there).
#   upload   -- the helper's watcher pushes the new revision with no prompt (live);
#   nohelper -- nothing is read while no prompt returns (no timer); a command that names a skill location (here `ls ~/.agents/skills`) brings the new revision in.
import os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases_lab as CL  # noqa: E402
import cases_lab_remote as CLR  # noqa: E402

HOOK = re.compile(r"hop=agent_context source=(helper|shell) revision=(\d+) skills=(\d+) rules=(\d+) names_hash=\[([0-9a-f,]*)\]")


def events(text):
    """[(source, revision, skills, rules, [hash...])] in log order"""
    return [(m.group(1), int(m.group(2)), int(m.group(3)), int(m.group(4)), [h for h in m.group(5).split(",") if h]) for m in HOOK.finditer(text)]


def saw_skill(text, name):
    want = CLR.fnv1a32(name)
    return any(want in hashes for _s, _rev, _sk, _ru, hashes in events(text))


def make_skill(lab, user, name):
    """a valid SKILL.md under ~/.agents/skills/<name>, made over a separate ssh connection. `sh -c '...'` keeps the quoting identical for every login shell (bash, zsh, fish, nu)."""
    lab.ssh(user, 'sh -c \'mkdir -p $HOME/.agents/skills/%s && printf "---\\nname: %s\\ndescription: lab skill\\n---\\nbody\\n" > $HOME/.agents/skills/%s/SKILL.md\'' % (name, name, name))


def make_rules(lab, user):
    lab.ssh(user, 'sh -c \'mkdir -p $HOME/.agents && printf "lab rules\\n" > $HOME/.agents/AGENTS.md\'')


def clean(lab, user, names):
    lab.ssh(user, "sh -c 'rm -rf %s $HOME/.agents/AGENTS.md'" % " ".join("$HOME/.agents/skills/%s" % n for n in names))


@CL.lab_case("skills_remote", shells=("bash", "zsh", "fish", "nu", "dash", "ash", "pwsh"), lat=("lan",), helper=("nohelper", "upload"), modes=("terminal",))
def c_skills_remote(ctx, r, arm):
    """The host's skills and global rules reach the app, with and without the helper, and a skill added behind the app's back arrives (live with the helper, on the
    next command that names a skill location without it). The context is never typed into the remote shell."""
    lab, u, rt = ctx.lab, arm["user"], CL.rtt_s(arm)
    uid = CL.unique(ctx)
    first, second = "labsk-a-%s" % uid, "labsk-b-%s" % uid
    off = ctx.log_size()
    try:
        make_skill(lab, u, first)
        make_rules(lab, u)
        if CL.connect_or_fail(ctx, r, arm) is None:
            ctx.reset()
            return
        time.sleep(1.0 + 2 * rt)
        # the helper pushes on connect; the shell backend reads at the session's first navigation, which a cd triggers
        ctx.be.type("cd %s" % CL.work(arm), CL.type_pace(ctx), enter=True)
        got = ctx.until(lambda: saw_skill(ctx.log_since(off), first), 25 + 6 * rt, step=0.2)
        if not got and arm["helper"] == "nohelper" and not events(ctx.log_since(off)):
            r.unverifiable("[hook] no hop=agent_context line in the app log: the shell-mode agent context needs the session's out-of-band channel (the ControlMaster adapter), which this build may not register")
            ctx.reset()
            CL.finish_remote(ctx, r)
            return
        r.check(bool(got), "the host's skill never reached the app (no hop=agent_context line naming it)")
        seen = [e for e in events(ctx.log_since(off)) if CLR.fnv1a32(first) in e[4]]
        want_source = "helper" if arm["helper"] == "upload" else "shell"
        r.check(bool(seen) and {e[0] for e in seen} == {want_source}, "the context came from %s, expected %s" % (sorted({e[0] for e in seen}), want_source))
        r.check(bool(seen) and seen[-1][3] == 1, "the global rules file was not reported (rules=%s)" % (seen[-1][3] if seen else "?"))
        first_rev = seen[-1][1] if seen else 0
        ctx.shot("lab_skills_first_%s" % arm["helper"])

        # a second skill appears outside the terminal
        mark = ctx.log_size()
        make_skill(lab, u, second)
        if arm["helper"] == "upload":
            live = ctx.until(lambda: saw_skill(ctx.log_since(mark), second), 10 + 4 * rt, step=0.1)
            r.check(bool(live), "the helper did not push the new skill without a prompt (no hop=agent_context line naming it)")
        else:
            # no prompt has come back and no command names a skill location: nothing is read (no timer, no polling)
            time.sleep(2.0 + 2 * rt)
            r.check(not saw_skill(ctx.log_since(mark), second), "the skill was reported before any command named a skill location: the shell backend must not poll")
            ctx.be.type(CL.ls_home_line(arm, ".agents/skills"), CL.type_pace(ctx), enter=True)
            seen2 = ctx.until(lambda: saw_skill(ctx.log_since(mark), second), 15 + 6 * rt, step=0.1)
            r.check(bool(seen2), "the command naming the skills directory did not bring the new skill in (no hop=agent_context line naming it)")
        newer = [e for e in events(ctx.log_since(mark)) if CLR.fnv1a32(second) in e[4]]
        r.check(bool(newer) and newer[-1][1] > first_rev and newer[-1][2] >= 2, "the new snapshot must carry a higher revision and both skills: %r" % newer[-1:])
        ctx.shot("lab_skills_second_%s" % arm["helper"])
        typed = [cmd for _, cmd in ctx.remote.commands() if uid in cmd and "labsk" not in cmd and "ls " not in cmd]
        r.check(not typed, "the feature typed into the remote shell: %r" % typed[:2])
    finally:
        clean(lab, u, [first, second])
    CL.finish_remote(ctx, r)


def declare_coverage():
    """tell the coverage guard (cases_lab_helper.py) that this case covers the agent_context family on both arms. Idempotent. Called from here when cases_lab_helper is
    already loaded and from cases_lab_helper's end otherwise (the two import each other through cases.py)."""
    import cases_lab_helper as CLH
    CLH.declare_external("agent_context", helper_case="lab_skills_remote", nohelper_case="lab_skills_remote")


if hasattr(sys.modules.get("cases_lab_helper"), "declare_external"):
    declare_coverage()
