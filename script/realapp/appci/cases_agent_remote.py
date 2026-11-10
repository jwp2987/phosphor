# appci/cases_agent_remote.py (#1827): the AGENT driving a remote host end to end in the ssh lab, with the uploaded helper and with none.
#   lab_agent_remote_flow    one agent turn of six scripted tool calls: read_files -> grep -> file_glob -> apply_file_diffs (edit of an EXISTING file) ->
#                            run_shell_command (cat the edited file, use its output) -> search_codebase; shells bash dash ash nu pwsh, helper nohelper|upload
#   lab_agent_remote_errors  the failure answers over ssh: a missing file, an edit whose search text is not in the file, a grep/glob with no hit; the model must
#                            be told (not "ok with the old content") and the host file must stay byte for byte
#   lab_agent_remote_drop    the connection RESETS while a run_shell_command is running on the host: the model gets a tool result that says the command did not
#                            complete (no hang), the pane is local again, and the agent still answers the next prompt
#   lab_agent_nld_predict_ssh (#1982) query predictions (the ghosted completion of an agent prompt; one model request per typing pause, carrying the input buffer)
#                            are requested on a LOCAL pane (control) and NOT on an ssh pane with the default settings. Oracle: the fake model's own count of
#                            `nld_predict` requests (fake_model.by_purpose); a control that sees no request makes the case UNVERIFIABLE, never a pass
# Agent mode only, against the scripted fake model (fake_model.py, APPCI_FAKE_MODEL=1; the multi-step "Please carry out these steps in order: a ;; b" turn).
# Oracles (never a screenshot alone):
#   * the file on the lab host (exact lines after the edit; unchanged after a refused edit);
#   * the app log's `hop=backend_route family=<capability> backend=<helper|shell|unavailable>`: nohelper -> shell, upload -> helper, per family
#     (file_context, ripgrep_search, fs_basic, file_write, codebase_chunks). file_context and file_write are REQUIRED to be logged; a family the build does not
#     route yet is reported UNVERIFIABLE (never a pass);
#   * the lab shell's rec.log: nothing the FEATURE did (the tokens only the read/grep/edit tools carry) was typed into the remote shell;
#   * the approved command's OWN effect file on the host (`cat <file> | /usr/bin/tee -a <ran file>`: one copy of the edited file = it ran exactly once; the
#     drop case's `/usr/bin/touch <marker>` = it started). Not rec.log (job 17885063245, both bash arms failed on it): an agent command is typed in its
#     wrapper, which bash keeps out of the history (`HISTIGNORE=*__phosphor_ap_*`, bash_body.sh, #904), and the bash hook logs `history 1` at the NEXT
#     prompt, so it can never show a command that is still running. No quote, `&&` or redirection in the flow command: nu and pwsh run it as written;
#   * the tool results the MODEL received (fake_model.FakeModel.tool_results_for): parsed JSON, status ok, the right paths / lines / new content.
# Platforms: Linux hosts drive the lab; the drop case is skipped on Windows panes (not ported, like lab_ssh_drop_reconnect). Needs the lab and APPCI_FAKE_MODEL=1:
# lab_prepare and need_model SKIP with the reason otherwise.
import json, os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases_caseagent as AG  # noqa: E402
import cases_lab as CL  # noqa: E402
import cases_lab_remote as LR  # noqa: E402
import cases_replay as CRP  # noqa: E402

SHELLS = ("bash", "dash", "ash", "nu", "pwsh")
PLAN_HEAD = "Please carry out these steps in order: "


def plan_text(steps):
    return PLAN_HEAD + " ;; ".join(steps)


def parse_result(text):
    """pure: a tool result the model received -> dict (the tools answer JSON); anything else -> {"_raw": text}"""
    try:
        v = json.loads(text)
        return v if isinstance(v, dict) else {"_raw": text}
    except (TypeError, ValueError):
        return {"_raw": text}


def result_paths(res, key="files"):
    """pure: the file paths of a grep/glob/read result: entries are {"path": ...} (grep, read) or plain strings (glob)"""
    out = []
    for e in res.get(key) or []:
        out.append(e.get("path") if isinstance(e, dict) else e)
    return out


def drop_command(shell, marker_path, tok):
    """pure: the plan step of the drop case: leave a marker file on the host (the command started), sleep, then print the final line the tool result must
    NOT show. bash/dash chain with `&&`; nu has no `&&` and its own `sleep` wants a duration, so there the externals are named by path and chained with `;`
    (nu stops a `;` chain at a failed command). No quote and no redirection in either form."""
    if shell == "nu":
        return "run /usr/bin/touch %s; /usr/bin/sleep 300; echo fin_%s" % (marker_path, tok)
    return "run /usr/bin/touch %s && sleep 300 && echo fin_%s" % (marker_path, tok)


def drive_plan(ctx, steps, timeout=150):
    """type the plan, approve each action card as it appears; returns (app log offset, finished) where finished = the model's final request (after the last tool
    result) arrived. Cards: Return focuses the card, the second Return approves (cases_replay agent_approve)."""
    import fake_model as FM
    fake = FM.ACTIVE
    base, off = fake.requests, ctx.log_size()
    AG.type_fast(ctx, plan_text(steps))
    handled, t_end = 0, time.time() + timeout
    while time.time() < t_end:
        if fake.requests >= base + len(steps) + 1:
            time.sleep(1.5)
            return off, True
        if AG.count_in_log(ctx, off, "queue_actions") > handled:
            handled += 1
            time.sleep(0.8)
            ctx.be.key("Return")
            time.sleep(0.6)
            ctx.be.key("Return")
        time.sleep(0.25)
    return off, False


def fake_model_or_skip(ctx, r):
    CRP.need_model(ctx, r)
    import fake_model as FM
    if FM.ACTIVE is None:
        r.skip("the fake model object is not reachable from the case process (fake_model.ACTIVE is unset), so the tool results the model received cannot be read")
    return FM.ACTIVE


def check_routes(ctx, r, arm, off, want_families, required):
    """per family: every backend_route line says the arm's backend; a family with no line is a failure when `required`, UNVERIFIABLE otherwise (the app logs a line for every family the agent uses: #1827)"""
    for fam in want_families:
        got = LR.routes(ctx, off, fam)
        if not got:
            if fam in required:
                r.check(False, "no hop=backend_route line for %s: the call never reached the router" % fam)
            else:
                r.unverifiable("no hop=backend_route family=%s line in the app log: this build does not route that family through the backend router (yet)" % fam)
            continue
        r.check(all(b == LR.expected_backend(arm) for b, _ in got), "%s routed to %r, expected %s" % (fam, got, LR.expected_backend(arm)))


def setup_tree(ctx, arm, fx, toks):
    """a small git tree on the host; returns the absolute directory"""
    ctx.lab.tree(arm["user"], {
        "%s/a.txt" % fx: "alpha %s\nsecond line\n" % toks["a"],
        "%s/b.txt" % fx: "bravo %s\n" % toks["n"],
        "%s/c.txt" % fx: "charlie nothing here\n",
        "%s/src/lib.rs" % fx: "pub fn needle_%s() {}\n" % toks["n"],
    }, git=True)
    return CL.work(arm, fx)


@CL.lab_case("agent_remote_flow", shells=SHELLS, lat=("lan",), helper=("nohelper", "upload"), modes=("agent",))
def c_agent_remote_flow(ctx, r, arm):
    """read -> grep -> glob -> edit an existing file -> run a command and use its output -> codebase search, as ONE agent turn on an ssh pane"""
    fake = fake_model_or_skip(ctx, r)
    uid = CL.unique(ctx, 6)
    toks = dict(a="tka%s" % uid, n="tkn%s" % uid, e="tke%s" % uid)
    fx = "agf_%s" % uid
    d = setup_tree(ctx, arm, fx, toks)
    a = "%s/a.txt" % d
    if not CL.agent_connect(ctx, r, arm):          # no remote prompt, or the #2019 plain-session limitation (UNVERIFIABLE)
        ctx.remote.rm("%s/a.txt" % fx, "%s/b.txt" % fx, "%s/c.txt" % fx, "%s/src/lib.rs" % fx)
        return
    ran = "ran_%s.txt" % uid         # the approved command's own effect file: one copy of the edited file per run
    steps = ["read %s" % a, "grep %s %s" % (toks["n"], d), "glob *.txt %s" % d,
             "edit %s :: alpha %s :: alpha %s" % (a, toks["a"], toks["e"]), "run cat %s | /usr/bin/tee -a %s" % (a, CL.work(arm, ran))]
    index_on = os.environ.get("APPCI_FAKE_EMBEDDINGS_ACTIVE") == "1"
    if index_on:
        steps.append("codebase needle_%s" % toks["n"])
    off, done = drive_plan(ctx, steps)
    ctx.shot("agent_remote_flow_%s_%s" % (arm["shell"], arm["helper"]))
    res = [parse_result(x) for x in fake.tool_results_for(toks["a"])]
    r.check(done, "the agent turn did not finish: the model received %d of %d tool results" % (len(res), len(steps)))
    if len(res) >= 1:
        r.check(res[0].get("status") == "ok" and any(toks["a"] in (f.get("content") or "") and "second line" in (f.get("content") or "") for f in res[0].get("files") or []),
                "read_files result is wrong: %.300r" % (res[0],))
    if len(res) >= 2:
        paths = result_paths(res[1])
        r.check(res[1].get("status") == "ok" and sorted(os.path.basename(p) for p in paths) == ["b.txt", "lib.rs"],
                "grep for the token found %r, expected exactly b.txt and src/lib.rs" % paths)
    if len(res) >= 3:
        names = {os.path.basename(p) for p in result_paths(res[2])}
        r.check(res[2].get("status") == "ok" and {"a.txt", "b.txt", "c.txt"} <= names and "lib.rs" not in names, "glob *.txt returned %r" % sorted(names))
    if len(res) >= 4:
        r.check(res[3].get("status") == "ok", "the edit's tool result is not ok: %.300r" % (res[3],))
    # the host file: exactly the edited lines
    r.check(ctx.until(lambda: ctx.remote.lines(a) == ["alpha %s" % toks["e"], "second line"], 30, step=0.5), "the edited file on the host is %r" % ctx.remote.lines(a))
    if len(res) >= 5:
        out = res[4].get("output") or res[4].get("_raw") or ""
        r.check(res[4].get("status") == "completed" and res[4].get("exit_code") == 0 and toks["e"] in out and toks["a"] not in out,
                "the run_shell_command result does not show the EDITED content: %.300r" % (res[4],))
    if index_on and len(res) >= 6:
        r.check(res[5].get("status") not in ("error", "cancelled"), "search_codebase answered with an error: %.300r" % (res[5],))
        r.metric("codebase_result", str(res[5])[:200])
    elif not index_on:
        r.note("codebase step not run: APPCI_FAKE_EMBEDDINGS=1 is off, so the host has no embeddings provider to index with")
    fams = ("file_context", "file_write", "ripgrep_search", "fs_basic") + (("codebase_chunks",) if index_on else ())
    check_routes(ctx, r, arm, off, fams, fams)
    # nothing feature-driven reached the remote shell; the one approved command did, once
    typed = [c for _, c in ctx.remote.commands()]
    leaked = [c for c in typed if any(toks[k] in c for k in ("a", "n", "e"))]
    r.check(not leaked, "the feature typed into the remote shell: %r" % leaked[:2])
    # the one approved command ran on the host exactly once: its effect file holds ONE copy of the edited file (two runs would hold two). The shell's own
    # record can only add to that: where the hook logs agent commands (not bash, see the header) a second typed copy is a failure too.
    edited = ["alpha %s" % toks["e"], "second line"]
    got_ran = ctx.remote.lines(ran)
    r.check(got_ran == edited, "the approved `cat` ran %s on the host (its effect file holds %r, expected exactly one copy of the edited file %r)" % (
        "0 times" if not got_ran else "%.1f times" % (len(got_ran) / float(len(edited))), got_ran[:6], edited))
    cats = [c for c in typed if "cat" in c and a in c]
    r.check(len(cats) <= 1, "the approved `cat` was typed into the remote shell %d times (%r), expected once" % (len(cats), cats[:3]))
    ctx.remote.rm("%s/a.txt" % fx, "%s/b.txt" % fx, "%s/c.txt" % fx, "%s/src/lib.rs" % fx, ran)
    CL.finish_remote(ctx, r)


@CL.lab_case("agent_remote_errors", shells=SHELLS, lat=("lan",), helper=("nohelper", "upload"), modes=("agent",))
def c_agent_remote_errors(ctx, r, arm):
    """the model is told the truth when a remote tool call fails or finds nothing; the host file is not touched"""
    fake = fake_model_or_skip(ctx, r)
    uid = CL.unique(ctx, 6)
    toks = dict(a="tka%s" % uid, n="tkn%s" % uid, e="tke%s" % uid)
    fx = "age_%s" % uid
    d = setup_tree(ctx, arm, fx, toks)
    a, missing = "%s/a.txt" % d, "%s/nope_%s.txt" % (d, uid)
    if not CL.agent_connect(ctx, r, arm):          # no remote prompt, or the #2019 plain-session limitation (UNVERIFIABLE)
        ctx.remote.rm("%s/a.txt" % fx, "%s/b.txt" % fx, "%s/c.txt" % fx, "%s/src/lib.rs" % fx)
        return
    steps = ["read %s" % missing, "edit %s :: not_in_file_%s :: whatever" % (a, uid), "grep zz%sqq %s" % (uid, d), "glob *.nothing_%s %s" % (uid, d)]
    off, done = drive_plan(ctx, steps)
    ctx.shot("agent_remote_errors_%s_%s" % (arm["shell"], arm["helper"]))
    res = [parse_result(x) for x in fake.tool_results_for("nope_%s" % uid)]
    r.check(done, "the agent turn did not finish: the model received %d of %d tool results" % (len(res), len(steps)))
    if len(res) >= 1:
        r.check(bool(res[0].get("failed_files")) or res[0].get("status") == "error", "a read of a missing file was not reported as failed: %.300r" % (res[0],))
        r.check(not any("nope_%s" % uid in (f.get("content") or "") for f in res[0].get("files") or []), "a missing file came back with content: %.300r" % (res[0],))
    if len(res) >= 2:
        r.check(res[1].get("status") != "ok" or not res[1].get("updated_files"), "an edit whose search text is not in the file was reported as applied: %.300r" % (res[1],))
    if len(res) >= 3:
        r.check(res[2].get("status") == "ok" and not result_paths(res[2]), "a grep with no hit returned %.300r" % (res[2],))
    if len(res) >= 4:
        r.check(res[3].get("status") == "ok" and not result_paths(res[3]), "a glob with no match returned %.300r" % (res[3],))
    r.check(ctx.remote.lines(a) == ["alpha %s" % toks["a"], "second line"], "the refused edit changed the host file: %r" % ctx.remote.lines(a))
    r.check(not ctx.remote.lines(missing), "the missing file exists on the host after the turn")
    check_routes(ctx, r, arm, off, ("file_context",), ("file_context",))
    leaked = [c for _, c in ctx.remote.commands() if uid in c]
    r.check(not leaked, "the feature typed into the remote shell: %r" % leaked[:2])
    ctx.remote.rm("%s/a.txt" % fx, "%s/b.txt" % fx, "%s/c.txt" % fx, "%s/src/lib.rs" % fx)
    CL.finish_remote(ctx, r)


@CL.lab_case("agent_remote_drop", shells=("bash", "dash", "nu"), lat=("lan",), helper=("nohelper", "upload"), modes=("agent",))
def c_agent_remote_drop(ctx, r, arm):
    """The connection is reset while the agent's run_shell_command is running on the host (the lab server resets the live connection, as a dropped link does).
    The model must get a tool result that says the command did NOT complete (no 'completed' with exit 0, no 'running' forever), within 60 s; the pane is local
    again and the agent answers the next prompt."""
    if ctx.win:
        r.skip("not yet ported to the Windows panes (see lab_ssh_drop_reconnect)")
    fake = fake_model_or_skip(ctx, r)
    tok = CL.unique(ctx, 8)
    if not CL.agent_connect(ctx, r, arm):          # no remote prompt, or the #2019 plain-session limitation (UNVERIFIABLE)
        return
    base, off = fake.requests, ctx.log_size()
    started_file = "started_%s" % tok
    AG.type_fast(ctx, plan_text([drop_command(arm["shell"], CL.work(arm, started_file), tok)]))
    if not AG.wait_marker(ctx, off, "queue_actions", 1, 90):
        r.check(False, "no approval card for the run_shell_command within 90 s")
        ctx.reset()
        return
    time.sleep(0.8)
    ctx.be.key("Return")
    time.sleep(0.6)
    ctx.be.key("Return")
    # the command is RUNNING on the host: its first step left the marker file (rec.log cannot say so: bash logs a command at the prompt AFTER it, and never
    # logs an agent command at all, see the header)
    started = ctx.until(lambda: ctx.remote.exists(started_file), 30 + 6 * CL.rtt_s(arm), step=0.25)
    r.check(bool(started), "the approved command never started on the host (no %s there), so the drop could not hit a running tool call" % CL.work(arm, started_file))
    if not started:
        ctx.remote.rm(started_file)
        ctx.reset()
        return
    time.sleep(1.0)
    t_drop = time.time()
    ctx.lab.drop(arm["user"], "rst")
    got = ctx.until(lambda: any("fin_%s" % tok in t for t in fake.tool_results_for("fin_%s" % tok)) or fake.requests >= base + 2, 60, step=0.25)
    r.metric("drop_to_tool_result_s", round(time.time() - t_drop, 1) if got else None)
    r.check(bool(got), "no tool result reached the model within 60 s of the connection reset: the tool call hangs")
    ctx.shot("agent_remote_drop_%s_%s" % (arm["shell"], arm["helper"]))
    results = [parse_result(x) for x in fake.tool_results_for("fin_%s" % tok)]
    if results:
        res = results[-1]
        status, code, out = res.get("status"), res.get("exit_code"), res.get("output") or ""
        r.check(not (status == "completed" and code == 0), "a command killed by a connection reset was reported as completed with exit 0: %.300r" % (res,))
        r.check(status != "running", "the tool result still says the command is running after the connection was lost: %.300r" % (res,))
        r.check(not re.search(r"^fin_%s\s*$" % tok, out, re.M), "the command's final output line appears although the connection was reset first: %.300r" % (res,))
    # the pane is usable again: local, and the agent answers a plain prompt
    r.check(ctx.until(lambda: ctx.where_now() == "local", 20, step=0.5), "after the reset the pane is %r, expected local" % (ctx.where_now(),))
    n = fake.requests
    AG.type_fast(ctx, "Reply with the word after_%s" % tok)
    r.check(ctx.until(lambda: fake.requests > n, 30, step=0.25), "the agent did not answer a new prompt after the connection was lost")
    ctx.remote.rm(started_file)
    ctx.reset()


def nld_requests(fake):
    return fake.by_purpose.get("nld_predict", 0)


def nld_verdict(local_delta, ssh_delta):
    """pure: (#1982) -> ("pass" | "fail" | "unverifiable", why). The local control must have produced a prediction request, or an ssh pane with none proves
    nothing (no provider for predictions, the flag off, no completed block yet); any request from the ssh pane is the defect."""
    if local_delta < 1:
        return "unverifiable", "the LOCAL control produced no nld_predict request, so none in the ssh session proves nothing"
    if ssh_delta > 0:
        return "fail", "%d nld_predict request(s) reached the model while typing in the ssh session (natural_language_autosuggestions_in_ssh_enabled is off by default)" % ssh_delta
    return "pass", ""


def type_partial_prompt(ctx, fake, text, wait_s, until_request):
    """type an agent prompt WITHOUT Enter and wait for the prediction debounce (250 ms) plus the request; returns how many nld_predict requests arrived. The
    line is cleared afterwards, and a request still in flight gets time to land before the next baseline."""
    base = nld_requests(fake)
    AG.type_fast(ctx, text, enter=False)
    if until_request:
        ctx.until(lambda: nld_requests(fake) > base, wait_s, step=0.25)
    else:
        time.sleep(wait_s)
    got = nld_requests(fake) - base
    ctx.reset()
    time.sleep(1.5)
    return got


@CL.lab_case("agent_nld_predict_ssh", shells=("bash",), lat=("lan",), helper=("nohelper",), modes=("agent",))
def c_agent_nld_predict_ssh(ctx, r, arm):
    """#1982: a typing pause in an agent prompt asks the model for a completion on a local pane, and asks for nothing on an ssh pane (default settings)"""
    fake = fake_model_or_skip(ctx, r)
    uid = CL.unique(ctx, 6)
    # a prediction needs a completed block in the pane (its context): run one on each side before typing the prompt
    ctx.reset()
    AG.type_fast(ctx, "echo nldpre_%s" % uid)
    time.sleep(2.0)
    local_delta = type_partial_prompt(ctx, fake, "please summarize what nldloc%s" % uid, 10, True)
    ctx.shot("agent_nld_predict_local_%s" % arm["shell"])
    r.metric("nld_predict_requests_local", local_delta)
    if CL.connect_or_fail(ctx, r, arm) is None:
        ctx.reset()
        return
    time.sleep(1.0 + 2 * CL.rtt_s(arm))
    AG.type_fast(ctx, "echo nldrem_%s" % uid)
    ran = ctx.until(lambda: any("nldrem_%s" % uid in c for _, c in ctx.remote.commands()), 20 + 6 * CL.rtt_s(arm), step=0.5)
    r.check(bool(ran), "`echo nldrem_%s` did not run in the remote shell, so the ssh pane has no completed block to predict from" % uid)
    ssh_delta = type_partial_prompt(ctx, fake, "please summarize what nldssh%s" % uid, 5, False)
    ctx.shot("agent_nld_predict_ssh_%s" % arm["shell"])
    r.metric("nld_predict_requests_ssh", ssh_delta)
    verdict, why = nld_verdict(local_delta, ssh_delta)
    if verdict == "unverifiable":
        r.unverifiable(why)
    else:
        r.check(verdict == "pass", why)
    leaked = [c for _, c in ctx.remote.commands() if "nldssh%s" % uid in c]
    r.check(not leaked, "the untyped-Enter prompt reached the remote shell: %r" % leaked[:2])
    CL.finish_remote(ctx, r)
