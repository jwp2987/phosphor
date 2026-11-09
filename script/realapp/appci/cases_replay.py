# appci/cases_replay.py: the REAL-USE REPLAY (Wave 1.5 #3, docs/testing.md "Real-use replay"): one long, seeded "day of use" case per session mode, registered
# through lab_case as lab_replay.bash.150.nohelper (the fake ssh lab at 150 ms netem, the no-helper default).
#   terminal pass: local commands, cd, Tab completion, ssh sessions (remote commands, cd, Tab, Ctrl-C, big output), Ctrl-C at random points, seq 1 200000,
#                  a large file cat, a bash / dash subshell, sudo -n where it works. Typing at 10-25 ms/key, instant Enter.
#   agent pass:    prose, an approved command, a rejected command, Ctrl-C mid-stream, against the scripted fake model (fake_model.py, APPCI_FAKE_MODEL=1).
# Oracles: the exact typed lines reach the shell (local / remote rec.log, in order); every effect file holds its token exactly once (no lost, no doubled key;
# a Ctrl-C'd or rejected step leaves nothing); no ERROR line outside the allowlist (replay.ERROR_ALLOW_DEFAULT + APPCI_REPLAY_ALLOW); the agent hop= invariants
# (replay.judge_agent); latency per step kind is RECORDED (metric), never a verdict. Length: APPCI_REPLAY_MINUTES (default 15; the nightly is 60); the seed is the
# run's --seed. The lab / fake model missing is a SKIP with the reason (lab_prepare, need_model).
import os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402
import cases_caseagent as AG  # noqa: E402
import cases_caseremote as CR  # noqa: E402
import cases_lab as CL  # noqa: E402
import replay as RP  # noqa: E402

TYPE_MS = (10, 25)


def _fwd(p):
    return p.replace("\\", "/")


def need_model(ctx, r):
    if os.environ.get("APPCI_FAKE_MODEL_ACTIVE") != "1":
        r.skip("the fake model is not running (APPCI_FAKE_MODEL=1 starts fake_model.py as the agent provider; a real provider is never used for the replay)")
    AG.need_provider(ctx, r)


def sudo_ok():
    try:
        return subprocess.run(["sudo", "-n", "true"], capture_output=True, timeout=10).returncode == 0
    except Exception:
        return False


def type_line(ctx, text):
    ctx.be.type(text, ctx.rng.randint(*TYPE_MS), enter=True)


class Run:
    """what one replay did: the lines the shell must have received (in order, per side), the effect files, the latency samples"""
    def __init__(self):
        self.local_lines, self.remote_lines = [], []
        self.ran, self.samples, self.notes = set(), [], []

    def sample(self, kind, t0):
        self.samples.append((kind, time.time() - t0))


def wait_eff(ctx, name, tok, timeout, remote=False):
    return ctx.until(lambda: tok in ctx.lines(name, remote=remote), timeout, step=0.05)


# ---------------------------------------------------------------- terminal steps (local side)
def local_step(ctx, r, run, s, base, rt):
    k, eff, tok = s["kind"], os.path.join(ctx.W, s["eff"]), s["tok"]
    abs_eff = _fwd(eff)
    t0 = time.time()
    if k == "local_cmd":
        line = "echo %s >> %s" % (tok, abs_eff)
        type_line(ctx, line)
        run.local_lines.append(line)
        ok = wait_eff(ctx, s["eff"], tok, 15)
    elif k == "cd":
        tgt = _fwd(ctx.W) if s["dir"] == "." else s["dir"]
        if s["dir"].startswith("rpd_"):
            os.makedirs(os.path.join(ctx.W, s["dir"]), exist_ok=True)
            tgt = _fwd(os.path.join(ctx.W, s["dir"]))
        line = "cd %s && echo %s >> %s" % (tgt, tok, abs_eff)
        type_line(ctx, line)
        run.local_lines.append(line)
        ok = wait_eff(ctx, s["eff"], tok, 15)
    elif k == "tab":
        os.makedirs(os.path.join(ctx.W, s["dir"]), exist_ok=True)
        pre = s["dir"][:len("rptab%s_" % s["idx"]) + 2]
        ctx.be.type("cd %s/%s" % (_fwd(ctx.W), pre), ctx.rng.randint(*TYPE_MS))
        ctx.be.key("Tab")
        time.sleep(0.5)
        rest = " && echo %s >> %s" % (tok, abs_eff)
        type_line(ctx, rest)
        run.local_lines.append("cd %s/%s/%s" % (_fwd(ctx.W), s["dir"], rest))       # normalised: bash adds the trailing slash (see norm_line)
        ok = wait_eff(ctx, s["eff"], tok, 15)
    elif k == "ctrlc":
        line = "sleep 30; echo %s >> %s" % (tok, abs_eff)
        type_line(ctx, line)
        run.local_lines.append(line)
        time.sleep(s["delay_ms"] / 1000.0)
        ctx.be.key("ctrl+c")
        time.sleep(1.0)
        run.ran.add(s["idx"])
        run.sample("ctrlc_return", t0)
        _clear(ctx)
        return
    elif k in ("big_seq", "big_cat"):
        if k == "big_seq":
            line = "seq 1 %d; echo %s >> %s" % (s["n"], tok, abs_eff)
        else:
            f = _fwd(os.path.join(ctx.W, "rpbig_%s.txt" % s["idx"]))
            line = "seq 1 200000 > %s; cat %s; echo %s >> %s" % (f, f, tok, abs_eff)
        type_line(ctx, line)
        run.local_lines.append(line)
        ok = wait_eff(ctx, s["eff"], tok, 120)
        time.sleep(0.5)
    elif k == "subshell":
        sh = "dash" if s["shell"] == "dash" and ctx.which("dash") else "bash --norc"
        type_line(ctx, sh)
        run.local_lines.append(sh)
        time.sleep(1.5)
        type_line(ctx, "echo %s >> %s" % (tok, abs_eff))          # typed into the subshell: it is not in the outer rec.log
        ok = wait_eff(ctx, s["eff"], tok, 15)
        type_line(ctx, "exit")
        time.sleep(1.0)
    elif k == "sudo":
        if not run.sudo:
            run.notes.append("step %s sudo skipped: `sudo -n` does not work on this machine" % s["idx"])
            return
        line = "sudo -n true && echo %s >> %s" % (tok, abs_eff)
        type_line(ctx, line)
        run.local_lines.append(line)
        ok = wait_eff(ctx, s["eff"], tok, 15)
    else:
        return
    run.ran.add(s["idx"])
    if ok:
        run.sample(k, t0)


def _clear(ctx):
    ctx.be.key("ctrl+u")
    time.sleep(0.2)


# ---------------------------------------------------------------- the ssh session (remote side)
def ssh_step(ctx, r, run, s, arm, rt):
    lab, u, base = ctx.lab, arm["user"], CL.work(arm)
    secs = CL.connect_or_fail(ctx, r, arm, "step %s: " % s["idx"])
    if secs is None:
        ctx.reset()
        return
    run.local_lines.append("ssh %s" % CL.host_of(arm))
    run.samples.append(("ssh_connect", secs))
    time.sleep(1.0 + 2 * rt)
    rl = []
    for i in s["inner"]:
        k, tok, t0 = i["kind"], i["tok"], time.time()
        eff = base + i["eff"]
        if k == "remote_cmd":
            line = "echo %s >> %s" % (tok, eff)
        elif k == "remote_cd":
            line = "cd %s && echo %s >> %s" % (i["dir"] if i["dir"] != "." else base, tok, eff)
        elif k == "remote_tab":
            lab.tree(u, {"%s/x" % i["dir"]: ""})
            pre = i["dir"][:len("rptab%s_" % i["idx"]) + 2]
            ctx.be.type("cd %s%s" % (base, pre), ctx.rng.randint(*TYPE_MS))
            ctx.be.key("Tab")
            time.sleep(0.5 + 2 * rt)
            rest = " && echo %s >> %s" % (tok, eff)
            type_line(ctx, rest)
            rl.append("cd %s%s/%s" % (base, i["dir"], rest))
            if not wait_eff(ctx, i["eff"], tok, 15 + 6 * rt, remote=True):
                run.notes.append("remote step %s: the Tab-completed line left no token" % i["idx"])
            else:
                run.sample("remote_tab", t0)
            run.ran.add(i["idx"])
            continue
        elif k == "remote_ctrlc":
            line = "sleep 30; echo %s >> %s" % (tok, eff)
            type_line(ctx, line)
            rl.append(line)
            time.sleep(i["delay_ms"] / 1000.0 + rt)
            ctx.be.key("ctrl+c")
            time.sleep(1.0 + 2 * rt)
            run.ran.add(i["idx"])
            run.sample("remote_ctrlc_return", t0)
            _clear(ctx)
            continue
        elif k == "remote_big":
            line = "seq 1 %d; echo %s >> %s" % (i["n"], tok, eff)
        else:
            continue
        type_line(ctx, line)
        rl.append(line)
        if wait_eff(ctx, i["eff"], tok, (120 if k == "remote_big" else 15) + 6 * rt, remote=True):
            run.sample(k, t0)
        run.ran.add(i["idx"])
    CL.finish_remote(ctx, r, "step %s: " % s["idx"])
    run.local_lines.append("exit")
    run.remote_lines.extend(rl)
    run.ran.add(s["idx"])


def norm_line(line):
    """bash completes a directory with a trailing slash: compare `cd a/b/ && ...` and `cd a/b && ...` as one"""
    return line.replace("/ && ", " && ")


def first_divergence(want, got):
    """None when equal, else a one-line description of the first difference"""
    want, got = [norm_line(x) for x in want], [norm_line(x) for x in got]
    for i, (w, g) in enumerate(zip(want, got)):
        if w != g:
            return "line %d: typed %r, the shell received %r" % (i + 1, w, g)
    if len(want) != len(got):
        return "typed %d lines, the shell received %d (first extra/missing: %r)" % (len(want), len(got), (want[len(got):] or got[len(want):])[:1])
    return None


# ---------------------------------------------------------------- agent steps
def agent_step(ctx, r, run, s):
    tok, k = s["tok"], s["kind"]
    eff = _fwd(os.path.join(ctx.W, s["eff"]))
    ask = s["ask"].replace(" %s" % s["eff"], " %s" % eff)
    s["ask"] = ask
    off = ctx.log_size()
    streams0 = AG.count_in_log(ctx, off, "[byop] opening stream")
    ctx.rm(s["eff"])
    t0 = time.time()
    AG.type_fast(ctx, ask)
    if k == "agent_ctrlc":
        AG.wait_marker(ctx, off, "[byop] opening stream", 1, 30)
        time.sleep(s["delay_ms"] / 1000.0 * 0.5)
        ctx.be.key("ctrl+c")
        time.sleep(4.0)               # the SLOW turn ends on its own after 3 s; nothing may be typed before the conversation is idle again
        run.sample("agent_ctrlc_return", t0)
    elif k == "agent_prose":
        AG.wait_marker(ctx, off, "[byop] opening stream", 1, 60)
        run.sample("agent_request", t0)
        time.sleep(1.5)
    else:
        if not AG.wait_marker(ctx, off, "queue_actions", 1, 90):
            run.notes.append("step %s: no approval card within 90 s" % s["idx"])
            ctx.be.key("ctrl+c")
            return
        run.sample("agent_card", t0)
        time.sleep(0.8)
        ctx.be.key("Return")          # focus the card (#1195/#1350)
        time.sleep(0.6)
        if k == "agent_approve":
            ctx.be.key("Return")
            wait_eff(ctx, s["eff"], tok, 30)
            AG.wait_marker(ctx, off, "[byop] opening stream", 2, 60)      # the tool result goes back to the model
            run.sample("agent_approve", t0)
        else:
            ctx.be.key("ctrl+c")      # Reject
        time.sleep(2.5)
    run.ran.add(s["idx"])


# ---------------------------------------------------------------- the case
@CL.lab_case("replay", shells=("bash",), lat=("150",), helper=("nohelper",), modes=("terminal", "agent"))
def c_replay(ctx, r, arm):
    try:
        minutes = RP.replay_minutes()
    except ValueError as e:
        r.skip("bad configuration: %s" % e)
    mode, rt, seed = ctx.mode, CL.rtt_s(arm), getattr(ctx, "seed", 1)
    steps = RP.gen_scenario(seed, minutes, mode)
    r.metric("replay", dict(seed=seed, minutes=minutes, mode=mode, steps=len(steps), kinds=sorted({s["kind"] for s in RP.flatten(steps)})))
    run = Run()
    run.sudo = sudo_ok()
    deadline = time.time() + minutes * 60 * 1.5 + 300         # the scenario is an estimate; the real run may take longer, never unbounded
    if mode == "agent":
        need_model(ctx, r)
        AG.fresh_conversation(ctx, r)
    off0, mark = ctx.log_size(), ctx.cmd_mark()          # after the fresh conversation: its Escape / /agent are not part of the replay
    done = []
    try:
        for s in steps:
            if time.time() > deadline:
                run.notes.append("deadline: stopped after %d of %d steps" % (len(done), len(steps)))
                break
            if mode == "agent":
                agent_step(ctx, r, run, s)
            elif s["kind"] == "ssh_session":
                ssh_step(ctx, r, run, s, arm, rt)
            else:
                local_step(ctx, r, run, s, CL.work(arm), rt)
            done.append(s)
            if mode == "terminal" and s["kind"] != "ctrlc":
                ctx.be.key("ctrl+u")
    finally:
        if mode == "agent":
            ctx.be.key("ctrl+c")
        ctx.reset()
    # --- oracles
    exp = RP.expected_effects(done, run.ran)
    remote_effs = {i["eff"] for st in done for i in st.get("inner", [])}
    local_exp = {f: v for f, v in exp.items() if f not in remote_effs}
    remote_exp = {f: v for f, v in exp.items() if f in remote_effs}
    bad = RP.judge_effects(local_exp, {f: ctx.lines(f) for f in local_exp})
    bad += ["remote " + b for b in RP.judge_effects(remote_exp, {f: ctx.remote.lines(f) for f in remote_exp})]
    r.check(not bad, "lost / doubled / misrouted input: %s" % "; ".join(bad[:6]))
    if mode == "terminal" and ctx.prof.commands():
        d = first_divergence(run.local_lines, ctx.cmds_since(mark))
        r.check(d is None, "the exact typed lines did not arrive (local rec.log): %s" % d)
        if run.remote_lines:
            rc = [c for _, c in ctx.remote.commands()]
            tail = [c for c in rc if not cases.Ctx.HARNESS_MARKER_RE.search(c)]
            d = first_divergence(run.remote_lines, tail[-len(run.remote_lines):])
            r.check(d is None, "the exact typed lines did not arrive (remote rec.log): %s" % d)
    text = ctx.log_since(off0)
    errs = RP.error_lines(text, extra_allow=[p for p in os.environ.get("APPCI_REPLAY_ALLOW", "").split(",") if p])
    r.check(not errs, "%d ERROR line(s) outside the allowlist, first: %s" % (len(errs), errs[:3]))
    if mode == "agent":
        hops = RP.agent_hops(text)
        probs = RP.judge_agent(hops, [s for s in done if s.get("ask")])
        r.check(not probs, "agent hop= invariants: %s" % "; ".join(probs[:5]))
    r.metric("replay_latency_s", RP.latency_stats(run.samples))       # recorded, not gating
    r.metric("replay_steps_done", "%d/%d" % (len(done), len(steps)))
    for n in run.notes[:20]:
        r.note(n)
    ctx.shot("replay_end")
