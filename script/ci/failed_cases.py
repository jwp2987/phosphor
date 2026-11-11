#!/usr/bin/env python3
# script/ci/failed_cases.py: write the cases file of a TARGETED run (script/ci/ci_seq.py --tier targeted --cases-file F) from a run that was collected.
#
#   python3 script/ci/failed_cases.py <test-results/RUN dir | dir of job traces> [more dirs ...] [-o FILE]
#                                     [--add PLATFORM=name,name ...] [--noref] [--no-unverifiable] [--proving PLATFORM ...]
#                                     [--changed-since GIT_REF [--changed-platforms a,b]] [--self-test]
#
# Owner 2026-11-11: the fixes of a round are proven with a targeted run first (only the cases that failed, on the OS where they failed, plus the cases
# that touch the changed code), and only then the whole harness. This helper does the first half: per platform (linux-wayland, linux-x11, linux-lab,
# macos, macos-lab, windows, windows-lab) the names of the cases whose row was FAIL or UNVERIFIABLE, and per machine the suites to play (its three unit
# suites, unit tests first, plus every integration / usage suite that failed there). `--add` is for the second half, the related cases a person names:
#   --add windows=ssh_connect_typing,big_paste     more cases for an app platform (repeatable)
#   --add linux=integration-linux                  more suites for a machine (linux | macos | windows)
#
#   --noref                                        also every other NOREF row (not judged: a latency case with no comparison, a host that was not there)
#   --proving linux-x11                            a job with NO result in that round: its whole PROVING list (a -lab platform: the PROVING_LAB globs)
#   --changed-since <ref>                          every case ADDED or CHANGED since that commit (it has never played): the @case functions of
#                                                  script/realapp/appci/cases*.py whose own code, or a helper of the same file they use, differs from
#                                                  <ref>; added to --changed-platforms (default linux-wayland,macos,windows; a case skips itself where it
#                                                  cannot run). Generated names (lab arms, table rows in other files) are not derivable: use --add; the
#                                                  changed files that define no @case are listed under info.changed_files_not_mapped.
# Why each added name is in the file is kept under info.added (ci_seq.py --dry-run prints it).
#
# Inputs, per directory:
#   * a collected run (test-results/<date>_<ref>_<pipeline>/manifest.json, written by collect_results.py): every app row and every suite status;
#   * a directory of job traces, as saved by hand from a round: `<job>.txt` (the job's trace: the `[mode] case STATUS detail` rows run.py prints),
#     `<job>.fails` (one case name a line), `app-fails.txt` (`## <job>` then rows) and `failed-jobs.txt` (`<job id> <job name>` a line).
# What is left out, and said on stderr and under `info` in the file (never silently):
#   * follow-on rows (pane_not_local_after_<case>, pane_wedged_after_<case>, left_agent_mode_after_<case>): they are not cases, they come back by
#     themselves when their case runs again. The case they name IS added (the row says that case left the pane broken);
#   * rows that are not registered cases (preflight, launch, needs_owner, ...): run.py refuses an unknown name (#1508), and a machine matter is not
#     proven by re-running a case;
#   * the Wayland job's xwayland pass (a fixed list of its own; the targeted run leaves that pass out) and the replay job;
#   * an app job that failed without one case row (it never got to its cases: read its trace).
# A case that failed in a fake-model pass (`agent@fake`) is listed under info.fake_pass. Nothing to do for it: in a targeted run run.py plans the fake
# pass from the selected names, so a PROVING_FAKE case gets its scripted model as in the fix tier.
# The arm filters are the union of where the rows failed: `modes` (terminal / agent) and, on Windows, `win_shells` (the pane passes).
import argparse, glob, json, os, re, sys
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import ci_seq  # noqa: E402  (the platform, job and suite tables: one source)

JOB_PLATFORM = {job: key for key, (_, job) in ci_seq.TARGETED_APP.items()}
FOLLOW_ON_RX = re.compile(r"^(?:pane_not_local_after|pane_wedged_after|left_agent_mode_after)_(.+)$")
ANSI_RX = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")
# a row as run.py prints it; a GitLab trace line may carry a timestamp and stream prefix ("2026-11-11T06:05:17.364888Z 00O ")
ROW_RX = re.compile(r"^(?:\d{4}-\d\d-\d\dT\S+Z \S+ ?)?\[([A-Za-z0-9@+_.-]+)\]\s+([A-Za-z0-9_.-]+)\s+(PASS|FAIL|SKIP|NOREF)\b\s*(.*)$")
SHELL_ORDER = list(ci_seq.TARGETED_WIN_SHELLS)


def platform_of_job(job):
    """app job name (a -reuse variant too) -> targeted platform key, or None (the replay, anything that is not an app job)"""
    return JOB_PLATFORM.get(re.sub(r"-reuse$", "", job or ""))


def machine_of_suite(job):
    """suite job name -> (machine, the name ci_seq knows) or None"""
    for p in (ci_seq.LINUX, ci_seq.MAC, ci_seq.WIN):
        if job in ci_seq.targeted_suites_allowed(p):
            return p, job
    return None


def job_of_trace_file(name):
    """`integration-linux[2].txt` -> `integration-linux: [2]` (a matrix job's trace as saved to a file); other names unchanged"""
    base = re.sub(r"\.(txt|fails|log)$", "", os.path.basename(name))
    return re.sub(r"\[(\d+)\]$", r": [\1]", base)


def is_bad(status, detail, unverifiable=True, noref=False):
    """the rows a targeted run repeats: FAIL, (unless turned off) the rows the harness itself could not verify (status noref, detail UNVERIFIABLE), and
    with noref=True every NOREF row (nothing was judged)"""
    st = (status or "").lower()
    return st == "fail" or (st == "noref" and (noref or (unverifiable and "UNVERIFIABLE" in (detail or ""))))


def parse_rows(text):
    """the app rows of a trace or of app-fails.txt -> [(job or None, mode, name, STATUS, detail)]; `## <job>` lines switch the job"""
    out, job = [], None
    for line in text.splitlines():
        line = ANSI_RX.sub("", line).rstrip()
        if line.startswith("## "):
            job = line[3:].strip()
            continue
        m = ROW_RX.match(line)
        if m:
            out.append((job, m.group(1), m.group(2), m.group(3), m.group(4)))
    return out


def read_dir(d, unverifiable=True, noref=False):
    """-> (rows [(job, mode or None, name)] of FAIL / UNVERIFIABLE rows, failed suite job names, app jobs that failed with no row, notes)"""
    rows, suites, norow, notes = [], [], [], []
    man = os.path.join(d, "manifest.json")
    if os.path.isfile(man):
        m = json.load(open(man))
        for key, info in sorted((m.get("app") or {}).items()):
            job = info.get("job") or ""
            if key.endswith("xwayland"):
                bad = sorted({c["name"] for c in info.get("cases") or [] if is_bad(c.get("status"), " ".join(c.get("detail") or []), unverifiable, noref)})
                if bad:
                    notes.append("xwayland pass of %s (not targeted: it has its own fixed case list): %s" % (job, ", ".join(bad)))
                continue
            got = [(job, c.get("mode") or None, c["name"]) for c in info.get("cases") or [] if is_bad(c.get("status"), " ".join(c.get("detail") or []), unverifiable, noref)]
            rows += got
            if not got and info.get("status") == "failed" and not any(c.get("status") in ("pass", "fail", "noref") for c in info.get("cases") or []):
                norow.append(job)
        for n, j in sorted((m.get("jobs") or {}).items()):
            if j.get("status") == "failed" and machine_of_suite(n):
                suites.append(n)
            if j.get("status") == "failed" and n.startswith("app-test-") and "no results artifact" in (j.get("note") or ""):
                norow.append(n)
        return rows, suites, sorted(set(norow)), notes
    seen_jobs = set()
    for f in sorted(glob.glob(os.path.join(d, "*.txt"))):
        base = os.path.basename(f)
        text = open(f, errors="replace").read()
        if base == "failed-jobs.txt":
            for line in text.splitlines():
                parts = line.split(None, 1)
                name = parts[1].strip() if len(parts) == 2 and parts[0].isdigit() else line.strip()
                if machine_of_suite(name):
                    suites.append(name)
                elif name.startswith("app-test-"):
                    seen_jobs.add(name)
            continue
        file_job = None if base == "app-fails.txt" else job_of_trace_file(base)
        parsed = parse_rows(text)
        for job, mode, name, status, detail in parsed:
            if is_bad(status, detail, unverifiable, noref):
                rows.append((job or file_job, mode, name))
        if file_job and file_job.startswith("app-test-"):
            seen_jobs.add(file_job)
            if not parsed and re.search(r"ERROR: Job failed", text):
                norow.append(file_job)
        elif file_job and machine_of_suite(file_job) and re.search(r"ERROR: Job failed", text):
            suites.append(file_job)
    for f in sorted(glob.glob(os.path.join(d, "*.fails"))):
        job = job_of_trace_file(f)
        rows += [(job, None, n) for n in open(f).read().split()]
    have = {j for j, _, _ in rows}
    norow = sorted(set(norow) - have)
    return rows, sorted(set(suites)), norow, notes


def registered_cases():
    """the names run.py accepts (appci cases registry), or None when the harness cannot be imported here"""
    try:
        sys.path.insert(0, os.path.join(HERE, "..", "realapp", "appci"))
        import cases
        return {c["name"] for c in cases.CASES}
    except Exception as e:                                # any import problem: fall back to the pattern rules, and say so
        sys.stderr.write("failed_cases: the appci case registry could not be imported (%s): unregistered rows are told by name pattern only\n" % str(e)[:160])
        return None


def proving_lists():
    """(PROVING, PROVING_LAB) as the harness holds them; when it cannot be imported, the literal statements of cases.py (ci_seq.fold_tuple_assignments)"""
    try:
        sys.path.insert(0, os.path.join(HERE, "..", "realapp", "appci"))
        import cases
        return list(cases.PROVING), list(cases.PROVING_LAB)
    except Exception:
        src = open(os.path.join(HERE, "..", "realapp", "appci", "cases.py")).read()
        return ci_seq.fold_tuple_assignments(src, "PROVING"), ci_seq.fold_tuple_assignments(src, "PROVING_LAB")


def case_defs(source):
    """-> ({case name: dump of its function}, {other top-level name: dump}, {case name: names it refers to}, {helper name: names it refers to}) of one
    cases*.py source. A case is a top-level function with a `@case("name", ...)` (or `@x.case(...)`) decorator."""
    import ast
    cs, helpers, crefs, hrefs = {}, {}, {}, {}
    for node in ast.parse(source).body:
        refs = {n.id for n in ast.walk(node) if isinstance(n, ast.Name)} | {n.attr for n in ast.walk(node) if isinstance(n, ast.Attribute)}
        names = []
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            for dec in node.decorator_list:
                f = getattr(dec, "func", None)
                if isinstance(dec, ast.Call) and (getattr(f, "id", None) == "case" or getattr(f, "attr", None) == "case") and dec.args \
                        and isinstance(dec.args[0], ast.Constant) and isinstance(dec.args[0].value, str):
                    names.append(dec.args[0].value)
        if names:
            for n in names:
                cs[n], crefs[n] = ast.dump(node), refs
            continue
        tops = [node.name] if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)) else \
            [t.id for t in (node.targets if isinstance(node, ast.Assign) else [getattr(node, "target", None)]) if isinstance(t, ast.Name)] \
            if isinstance(node, (ast.Assign, ast.AugAssign, ast.AnnAssign)) else []
        for t in tops:
            helpers[t] = helpers.get(t, "") + ast.dump(node)      # a name assigned twice (X = ..., X += ...): all of its statements
            hrefs[t] = hrefs.get(t, set()) | (refs - {t})
    return cs, helpers, crefs, hrefs


def changed_cases(old_src, new_src):
    """-> {case name: why} for the cases of new_src that are new, whose own code differs from old_src, or that use (directly, or through helpers of the
    same file) a top-level helper that is new or differs. old_src None = the file is new."""
    ncs, nh, ncr, nhr = case_defs(new_src)
    ocs, oh, _, _ = case_defs(old_src) if old_src is not None else ({}, {}, {}, {})
    dirty = {h for h in nh if oh.get(h) != nh[h]} if old_src is not None else set()
    grew = True
    while grew:                                           # a helper that uses a changed helper is changed too
        more = {h for h in nh if h not in dirty and nhr[h] & dirty}
        grew = bool(more)
        dirty |= more
    out = {}
    for n in ncs:
        if n not in ocs:
            out[n] = "new"
        elif ocs[n] != ncs[n]:
            out[n] = "changed"
        elif ncr[n] & dirty:
            out[n] = "uses changed %s" % ", ".join(sorted(ncr[n] & dirty)[:3])
    return out


def changed_since(ref, repo=None):
    """-> ({case: "file: why"}, [changed appci files that define no @case]) between git `ref` and the working tree, for script/realapp/appci/"""
    import subprocess
    repo = repo or os.path.join(HERE, "..", "..")
    sub = "script/realapp/appci"
    r = subprocess.run(["git", "-C", repo, "diff", "--name-only", ref, "--", sub], capture_output=True, text=True)
    if r.returncode:
        raise ValueError("git diff %s failed: %s" % (ref, r.stderr.strip()[:200]))
    out, unmapped = {}, []
    for f in [l for l in r.stdout.splitlines() if l.endswith(".py")]:
        path = os.path.join(repo, f)
        base = os.path.basename(f)
        if not os.path.isfile(path) or base.startswith("selftest"):
            continue
        new_src = open(path).read()
        if not case_defs(new_src)[0]:
            unmapped.append(f)
            continue
        o = subprocess.run(["git", "-C", repo, "show", "%s:%s" % (ref, f)], capture_output=True, text=True)
        for n, why in changed_cases(o.stdout if o.returncode == 0 else None, new_src).items():
            out[n] = "%s: %s" % (base, why)
    return out, unmapped


def split_label(label):
    """a row's mode label -> (session mode, Windows pane pass or None, fake pass?): `agent@gitbash+fake` -> ("agent", "gitbash", True)"""
    mode, _, rest = (label or "").partition("@")
    parts = [x for x in rest.split("+") if x]
    return mode, next((x for x in parts if x != "fake"), None), "fake" in parts


def build(rows, suites, norow=(), notes=(), registry=None, adds=()):
    """-> the cases file (dict). rows: [(job, mode label or None, name)]; suites: failed suite job names; adds: [(platform or machine, [names])] or
    [(platform, [names], why)]: `why` is kept under info.added[platform]."""
    plats, info = {}, {"dropped": {}, "follow_on": {}, "fake_pass": {}, "not_targeted": list(notes), "jobs_without_rows": sorted(set(norow))}

    def entry(key):
        return plats.setdefault(key, {"cases": [], "modes": [], "shells": [], "with_mode": set()})

    def known(name):
        return (name in registry) if registry is not None else not re.match(r"^(preflight|launch|harness|restore|needs_owner|case_selection|startup_shell_pass)$", name)
    for job, label, name in rows:
        key = platform_of_job(job)
        if key is None:
            info["not_targeted"].append("%s: %s (no targeted platform for this job)" % (job, name))
            continue
        m = FOLLOW_ON_RX.match(name)
        if m and (registry is None or not known(name)):
            base = m.group(1)
            info["follow_on"].setdefault(key, [])
            if name not in info["follow_on"][key]:
                info["follow_on"][key].append(name)
            if not known(base) or FOLLOW_ON_RX.match(base):
                continue
            name = base                                   # the case the follow-on row blames
        elif not known(name):
            info["dropped"].setdefault(key, [])
            if name not in info["dropped"][key]:
                info["dropped"][key].append(name)
            continue
        e = entry(key)
        if name not in e["cases"]:
            e["cases"].append(name)
        if label is None or label == "-":
            continue                                      # a bare name (a .fails list): no arm known from this row
        e["with_mode"].add(name)
        mode, pane, fake = split_label(label)
        if mode in ci_seq.TARGETED_MODES and mode not in e["modes"]:
            e["modes"].append(mode)
        if key.startswith("windows"):
            pane = pane or "powershell"
            if pane in SHELL_ORDER and pane not in e["shells"]:
                e["shells"].append(pane)
        if fake:
            info["fake_pass"].setdefault(key, [])
            if name not in info["fake_pass"][key]:
                info["fake_pass"][key].append(name)
    extra_suites = {}
    for where, names, *why in adds:
        if where in ci_seq.TARGETED_APP:
            e = entry(where)
            fresh = [n for n in names if n not in e["cases"]]
            e["cases"] += fresh
            if why:                                       # names a failed row already put there are listed too: both reasons hold
                info.setdefault("added", {}).setdefault(where, {})[why[0]] = sorted(names)
        elif where in (ci_seq.LINUX, ci_seq.MAC, ci_seq.WIN):
            extra_suites.setdefault(where, [])
            extra_suites[where] += names
        else:
            raise ValueError("--add %s=...: not a platform (%s) nor a machine (linux, macos, windows)" % (where, ", ".join(ci_seq.TARGETED_APP)))
    out = {"platforms": {}, "suites": {}}
    for key in ci_seq.TARGETED_APP:                       # the file in play order
        e = plats.get(key)
        if not e or not e["cases"]:
            continue
        o = {"cases": sorted(e["cases"])}
        arms_known = set(e["cases"]) <= e["with_mode"]      # a case no row gave a mode for (a bare name, --add) may need any arm: no filter then
        if e["modes"] and arms_known:
            o["modes"] = [m for m in ci_seq.TARGETED_MODES if m in e["modes"]]
        if e["shells"] and arms_known:
            o["win_shells"] = [x for x in SHELL_ORDER if x in e["shells"]]
        out["platforms"][key] = o
    failed = {}
    for n in suites:
        ms = machine_of_suite(n)
        if ms:
            failed.setdefault(ms[0], []).append(ms[1])
    for p in (ci_seq.LINUX, ci_seq.MAC, ci_seq.WIN):
        has_app = any(ci_seq.TARGETED_APP[k][0] == p for k in out["platforms"])
        want = (ci_seq.unit_suites(p) if has_app else []) + failed.get(p, []) + extra_suites.get(p, [])
        allowed = ci_seq.targeted_suites_allowed(p)
        lst = []
        for n in want:
            for x in ([a for a in allowed if a.startswith("integration-linux: [")] if n == "integration-linux" else [n]):
                if x not in allowed:
                    raise ValueError("--add %s=%s: not a suite of that machine (known: %s)" % (p, x, ", ".join(allowed)))
                if x not in lst:
                    lst.append(x)
        if lst:
            out["suites"][p] = sorted(lst, key=allowed.index)
    info = {k: v for k, v in info.items() if v}
    if info:
        out["info"] = info
    return out


_OLD = '''
TABLE = (1,)
def _h1(): return 1
def _h2(): return _h1()
def _h3(): return 3
@case("c_body")
def a(ctx, r): r.ok(1)
@case("c_helper", modes=("agent",))
def b(ctx, r): _h2()
@case("c_table")
def c(ctx, r): return TABLE
@cases.case("c_same")
def d(ctx, r): _h3()
'''
_NEW = _OLD.replace("def _h1(): return 1", "def _h1(): return 2").replace("r.ok(1)", "r.ok(2)").replace("TABLE = (1,)", "TABLE = (1,)\nTABLE += (2,)") + '''
@case("c_new")
def e(ctx, r): pass
'''


def self_test():
    reg = {"tab_enter", "big_paste", "md_viewer", "soak", "ssh_connect_typing", "lab_raw_key_arrival.bash.lan.nohelper", "fake_case"}
    trace = "\n".join([
        "2026-11-11T06:05:17.364888Z 00O [terminal] tab_enter                FAIL the shell received []",
        "[terminal] big_paste                PASS ok",
        "\x1b[0;31m[agent] ui_x  NOREF NO-REFERENCE: UNVERIFIABLE: injector accounting: 1 of 215\x1b[0m",
        "[agent] latency_enter NOREF NO-REFERENCE: within noise: INCONCLUSIVE",
        "[terminal] skipme SKIP Windows only",
        "not a row [x] y FAIL",
    ])
    pr = parse_rows(trace)
    rows = [("app-test-linux-wayland", "terminal", "tab_enter"), ("app-test-linux-wayland", "agent", "pane_wedged_after_tab_enter"),
            ("app-test-windows", "terminal@gitbash", "pane_not_local_after_md_viewer"), ("app-test-windows", "agent", "big_paste"),
            ("app-test-windows", "agent@gitbash+fake", "fake_case"), ("app-test-windows", "terminal", "left_agent_mode_after_soak"),
            ("app-test-macos", "terminal", "needs_owner"), ("app-test-macos-reuse", "terminal", "tab_enter"), ("app-test-lab-linux", "agent", "lab_raw_key_arrival.bash.lan.nohelper"),
            ("app-test-replay-linux", "terminal", "lab_replay_day"), ("app-test-linux-x11", None, "tab_enter")]
    f = build(rows, ["test-linux", "integration-linux: [2]", "test-workspace-rest-windows", "usage-test-macos"], ["app-test-lab-macos"], registry=reg,
              adds=[("linux", ["integration-linux"]), ("macos-lab", ["lab_raw_key_arrival.*"])])
    spec_ok = True
    try:
        plan = ci_seq.targeted_plan(ci_seq.targeted_spec(f))
    except ValueError as e:
        spec_ok, plan = False, {"machines": {}}
        print("     " + str(e))
    checks = [
        ([(r[1], r[2], r[3]) for r in pr] == [("terminal", "tab_enter", "FAIL"), ("terminal", "big_paste", "PASS"), ("agent", "ui_x", "NOREF"), ("agent", "latency_enter", "NOREF"), ("terminal", "skipme", "SKIP")],
         "rows are read from a trace: a timestamp prefix and colour codes do not hide one, other lines are not rows"),
        ([n for _, _, n, s, d in pr if is_bad(s, d)] == ["tab_enter", "ui_x"] and [n for _, _, n, s, d in pr if is_bad(s, d, False)] == ["tab_enter"],
         "FAIL and UNVERIFIABLE rows are taken; an inconclusive latency NOREF, a PASS and a SKIP are not; --no-unverifiable keeps FAIL only"),
        (f["platforms"]["linux-wayland"] == {"cases": ["tab_enter"], "modes": ["terminal", "agent"]},
         "a follow-on row (pane_wedged_after_*) is stripped; the case it names is in the list once, and its mode counts"),
        (f["platforms"]["windows"] == {"cases": ["big_paste", "fake_case", "md_viewer", "soak"], "modes": ["terminal", "agent"], "win_shells": ["powershell", "gitbash"]},
         "Windows: pane passes become win_shells (no pane = powershell), a follow-on row adds its case"),
        (f["platforms"]["macos"] == {"cases": ["tab_enter"], "modes": ["terminal"]} and f["info"]["dropped"] == {"macos": ["needs_owner"]}, "a row that is no registered case is left out and listed; a -reuse job is its platform"),
        (f["platforms"]["linux-x11"] == {"cases": ["tab_enter"]}, "a name with no known mode (a .fails list) gives no mode filter"),
        (f["platforms"]["linux-lab"]["cases"] == ["lab_raw_key_arrival.bash.lan.nohelper"] and f["platforms"]["macos-lab"] == {"cases": ["lab_raw_key_arrival.*"]}
         and any("replay" in x for x in f["info"]["not_targeted"]), "lab rows go to the lab platform; the replay is not targeted and is listed"),
        (f["suites"] == {"linux": ["test-linux", "test-warpui-core", "test-workspace-rest", "integration-linux: [1]", "integration-linux: [2]", "integration-linux: [3]"],
                         "macos": ["test-macos", "test-warpui-core-macos", "test-workspace-rest-macos", "usage-test-macos"],
                         "windows": ["test-windows", "test-warpui-core-windows", "test-workspace-rest-windows"]},
         "suites: the machine's three unit suites first, then the integration / usage suites that failed or were added"),
        (f["info"]["fake_pass"] == {"windows": ["fake_case"]} and f["info"]["jobs_without_rows"] == ["app-test-lab-macos"]
         and f["info"]["follow_on"]["windows"] == ["pane_not_local_after_md_viewer", "left_agent_mode_after_soak"], "fake-pass cases, row-less failed jobs and the stripped follow-on rows are listed under info"),
        (spec_ok and list(plan["machines"]) == ["linux", "macos", "windows"], "the file this helper writes is a valid cases file for ci_seq.py --tier targeted"),
        (build(rows, [], registry=reg, adds=[("windows", ["ssh_connect_typing", "big_paste"])])["platforms"]["windows"] == {"cases": ["big_paste", "fake_case", "md_viewer", "soak", "ssh_connect_typing"]},
         "--add adds related cases once; a case with no known arm lifts the mode and pane filters (it may need any pass)"),
        (is_bad("NOREF", "within noise", True, True) and not is_bad("SKIP", "x", True, True) and not is_bad("PASS", "", True, True), "--noref takes every NOREF row, never a PASS or a SKIP"),
        (build([], [], registry=reg, adds=[("linux-x11", ["tab_enter", "soak"], "why x"), ("linux-lab", ["lab_raw_key_arrival.*"], "why lab")])
         == {"platforms": {"linux-x11": {"cases": ["soak", "tab_enter"]}, "linux-lab": {"cases": ["lab_raw_key_arrival.*"]}}, "suites": {"linux": ci_seq.unit_suites("linux")},
             "info": {"added": {"linux-x11": {"why x": ["soak", "tab_enter"]}, "linux-lab": {"why lab": ["lab_raw_key_arrival.*"]}}}},
         "a whole list added for a job with no result is in the file with its reason under info.added"),
        (changed_cases(_OLD, _NEW) == {"c_new": "new", "c_body": "changed", "c_helper": "uses changed _h2", "c_table": "uses changed TABLE"} and changed_cases(None, _NEW).keys() == {"c_new", "c_body", "c_helper", "c_table", "c_same"},
         "--changed-since: a new case, a case whose code changed, and a case that uses a changed helper or table of its file (also through another helper); an untouched case is not listed; a new file = all its cases"),
        (build([], ["test-macos"]) == {"platforms": {}, "suites": {"macos": ["test-macos"]}}, "a machine with only a failed suite plays that suite only"),
        (build([("app-test-macos", "terminal", "pane_wedged_after_x"), ("app-test-macos", "terminal", "preflight"), ("app-test-macos", "agent", "real_case")], [])["platforms"]["macos"]["cases"] == ["real_case", "x"],
         "without the registry the follow-on and harness rows are still told by name"),
        (job_of_trace_file("/x/integration-linux[2].txt") == "integration-linux: [2]" and platform_of_job("app-test-lab-windows") == "windows-lab" and platform_of_job("app-test-replay-linux") is None,
         "trace file names and job names map to jobs and platforms"),
    ]
    bad = 0
    for ok, what in checks:
        bad += 0 if ok else 1
        print("%s %s" % ("ok  " if ok else "FAIL", what))
    print("self-test: %s" % ("FAILED (%d)" % bad if bad else "all passed"))
    return 1 if bad else 0


def main():
    ap = argparse.ArgumentParser(description="Write the cases file of a targeted run from a collected run; see the header of this file and docs/testing.md 2.9")
    ap.add_argument("dirs", nargs="*", help="test-results/<run> directories (manifest.json) or directories of saved job traces")
    ap.add_argument("-o", "--out", default="", help="write the file here (default: stdout)")
    ap.add_argument("--add", action="append", default=[], metavar="PLATFORM=NAMES", help="related cases for an app platform, or suites for a machine (comma separated; repeatable)")
    ap.add_argument("--no-unverifiable", action="store_true", help="FAIL rows only (default: also the rows the harness reported UNVERIFIABLE)")
    ap.add_argument("--noref", action="store_true", help="also every other NOREF row (not judged at all)")
    ap.add_argument("--proving", action="append", default=[], metavar="PLATFORM", help="a job with no result: add its whole PROVING list (a -lab platform: the PROVING_LAB globs); repeatable")
    ap.add_argument("--changed-since", default="", metavar="GIT_REF", help="add every appci case that is new or changed since this commit (it has never played)")
    ap.add_argument("--changed-platforms", default="linux-wayland,macos,windows", help="where --changed-since adds its cases (default: %(default)s)")
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    if not (a.dirs or a.add or a.proving or a.changed_since):
        ap.error("give at least one directory (or --add / --proving / --changed-since)")
    rows, suites, norow, notes = [], [], [], []
    for d in a.dirs:
        if not os.path.isdir(d):
            sys.stderr.write("failed_cases: %s is not a directory\n" % d)
            return 2
        r, s, n, t = read_dir(d, not a.no_unverifiable, a.noref)
        rows += r; suites += s; norow += n; notes += t
    adds = []
    for x in a.add:
        if "=" not in x:
            ap.error("--add wants PLATFORM=name,name")
        adds.append((x.split("=", 1)[0], [n for n in x.split("=", 1)[1].split(",") if n]))
    unmapped = []
    try:
        for key in a.proving:
            if key not in ci_seq.TARGETED_APP:
                raise ValueError("--proving %s: not a platform (%s)" % (key, ", ".join(ci_seq.TARGETED_APP)))
            proving, proving_lab = proving_lists()
            adds.append((key, proving_lab if key.endswith("-lab") else proving,
                         "the job had no result in the round: its whole %s list" % ("PROVING_LAB" if key.endswith("-lab") else "PROVING")))
        if a.changed_since:
            ch, unmapped = changed_since(a.changed_since)
            for key in [k for k in a.changed_platforms.split(",") if k]:
                if key not in ci_seq.TARGETED_APP or key.endswith("-lab"):
                    raise ValueError("--changed-platforms %s: not a non-lab platform" % key)
                adds.append((key, sorted(ch), "added or changed since %s (never played)" % a.changed_since))
        reg = registered_cases()
        if reg is not None:                               # an added name the harness does not know would make run.py refuse the whole job (#1508)
            import fnmatch
            unknown = sorted({n for where, names, *_ in adds if where in ci_seq.TARGETED_APP for n in names if not fnmatch.filter(reg, n)})
            if unknown:
                raise ValueError("added name(s) that match no registered case: %s" % ", ".join(unknown))
        doc = build(rows, suites, norow, notes, reg, adds)
        for key in a.proving:                             # the PROVING_LAB globs are what the fix tier's lab job plays: with its quick matrix
            if key.endswith("-lab") and key in doc["platforms"]:
                doc["platforms"][key].setdefault("lab_matrix", "quick")
        if a.changed_since:
            doc.setdefault("info", {})["changed_cases"] = {n: ch[n] for n in sorted(ch)}
            if unmapped:
                doc["info"]["changed_files_not_mapped"] = unmapped
        if doc["platforms"] or doc["suites"]:
            ci_seq.targeted_spec(doc)                     # never hand over a file the driver would refuse
    except ValueError as e:
        sys.stderr.write("failed_cases: %s\n" % e)
        return 2
    for k, v in sorted((doc.get("info") or {}).items()):
        sys.stderr.write("failed_cases: %s: %s\n" % (k, json.dumps(v, ensure_ascii=False)))
    if not doc["platforms"] and not doc["suites"]:
        sys.stderr.write("failed_cases: no FAIL / UNVERIFIABLE row and no failed suite found: there is nothing to target\n")
        return 1
    text = json.dumps(doc, indent=1, ensure_ascii=False) + "\n"
    if a.out:
        with open(a.out, "w") as f:
            f.write(text)
        sys.stderr.write("failed_cases: wrote %s; check the plan with: python3 script/ci/ci_seq.py --tier targeted --cases-file %s --dry-run\n" % (a.out, a.out))
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
