#!/usr/bin/env python3
"""script/ci/results_trend.py: one-page trend of the committed test runs (test-results/*/manifest.json).

WHAT IT ANSWERS (owner, 2026-11-10): "are we getting closer?" Three numbers per platform and run, on the SAME cases:
  1. failing cases (FAIL), listed with their reason and how many runs in a row they have failed;
  2. untrustworthy cases: NOREF, split into "injector" (the harness says keys were lost between the injector and the app,
     so no pass can be trusted) and "other" (no reference terminal, within noise, ...);
  3. regressions: cases that passed in the previous run and fail now, and fixes: the reverse.
Plus the DANGER classes (lost/reordered keys, a secret in the wrong place, a wrong command run), which must be zero FAIL
whatever else is open, and the unit-test failures of the newest run.

Comparisons use only (case, mode) pairs judged pass/fail in BOTH runs: the raw pass rate misled us once (the suite grew 3x
between two runs and the rate fell although nothing regressed).

  python3 script/ci/results_trend.py                    # writes test-results/TREND.md from test-results/*/manifest.json
  python3 script/ci/results_trend.py --last 5 --stdout
  python3 script/ci/results_trend.py --from <dir with manifest.json> [--from ...]   # a run that is not committed yet
  python3 script/ci/results_trend.py --self-test

Generated file: never edit test-results/TREND.md by hand.
"""
import argparse, glob, json, os, re, sys, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# cases whose failure is a release blocker whatever else is open (budgets note: lost, duplicated or reordered keys, a wrong
# command executed, a secret leaking). Matched against the case name.
DANGER = re.compile(r"key_loss|enter_loss|fast_typing|cadence|giveup_keys|secret|password|keys_follow|typeahead|ordering|"
                    r"out_of_order|no_keys_lost|paste_exact|big_paste|exact", re.I)


def _text(v):
    return v if isinstance(v, str) else json.dumps(v) if v is not None else ""


def load_runs(dirs):
    runs = []
    for d in dirs:
        m = os.path.join(d, "manifest.json")
        try:
            man = json.load(open(m))
        except (OSError, ValueError):
            continue
        name = os.path.basename(os.path.normpath(d))
        cases = {}
        for plat, v in (man.get("app") or {}).items():
            if not isinstance(v, dict):
                continue
            cs = v.get("cases")
            cs = cs if isinstance(cs, list) else list((cs or {}).values())
            for c in cs:
                if isinstance(c, dict) and c.get("name"):
                    cases[(plat, c["name"], str(c.get("mode") or "-"))] = (
                        str(c.get("status") or "").lower(), _text(c.get("detail")))
        unit = {}
        for job, u in (man.get("unit") or {}).items():
            if isinstance(u, dict):
                unit[job] = (u.get("failed") or 0, [f.get("test", "") for f in (u.get("failures") or []) if isinstance(f, dict)])
        runs.append({"name": name, "commit": str(man.get("commit") or "")[:9], "date": str(man.get("date") or ""),
                     "created": str(man.get("created_at") or ""), "cases": cases, "unit": unit})
    runs.sort(key=lambda r: (r["created"] or r["date"], r["name"]))
    return runs


def noref_kind(detail):
    if "TEST HOST MISSING" in detail:      # #2043: a case lost to a test host that did not start is never filed under the injector explanation
        return "other"
    return "injector" if "injector accounting" in detail and ("never reached" in detail or "UNVERIFIABLE" in detail) else "other"


def counts(run, plat):
    c = {"cases": 0, "pass": 0, "fail": 0, "noref_inj": 0, "noref_other": 0, "skip": 0}
    for (p, _n, _m), (st, det) in run["cases"].items():
        if p != plat:
            continue
        c["cases"] += 1
        if st == "noref":
            c["noref_inj" if noref_kind(det) == "injector" else "noref_other"] += 1
        elif st in c:
            c[st] += 1
    return c


def compare(prev, cur, plat):
    """-> (fixed, regressed) lists of (case, mode) judged pass/fail in both runs."""
    fixed, reg = [], []
    for k, (st, _d) in cur["cases"].items():
        if k[0] != plat or k not in prev["cases"]:
            continue
        ps = prev["cases"][k][0]
        if ps in ("pass", "fail") and st in ("pass", "fail"):
            if ps == "fail" and st == "pass":
                fixed.append((k[1], k[2]))
            elif ps == "pass" and st == "fail":
                reg.append((k[1], k[2]))
    return sorted(fixed), sorted(reg)


def streak(runs, key):
    n = 0
    for r in reversed(runs):
        v = r["cases"].get(key)
        if v is None:
            continue
        if v[0] != "fail":
            break
        n += 1
    return n


def short(detail, n=150):
    t = re.sub(r"\s+", " ", detail).strip(" []'\"")
    t = re.sub(r"injector accounting:.*?(;|$)", "", t).strip(" ;|")
    return (t[:n] + "...") if len(t) > n else t


def render(runs, last):
    runs = runs[-last:] if last else runs
    out = ["# Test trend", "",
           "Generated by `script/ci/results_trend.py`; never edit by hand. Source: `test-results/*/manifest.json` (committed runs only).",
           "Numbers compare the SAME cases (judged pass/fail in both runs). See the script header for what each column means.", ""]
    if not runs:
        return "\n".join(out + ["No runs found."]) + "\n"
    out += ["Runs shown (oldest first): " + ", ".join("`%s`" % r["name"] for r in runs), ""]
    plats = sorted({k[0] for r in runs for k in r["cases"]})
    latest = runs[-1]
    for plat in plats:
        out += ["## %s" % plat, "", "| run | commit | cases | PASS | FAIL | NOREF injector | NOREF other | SKIP | fixed vs previous | regressed vs previous |",
                "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|"]
        prev = None
        for r in runs:
            if not any(k[0] == plat for k in r["cases"]):
                continue
            c = counts(r, plat)
            fx, rg = compare(prev, r, plat) if prev else ([], [])
            out.append("| `%s` | %s | %d | %d | %d | %d | %d | %d | %s | %s |" % (
                r["name"], r["commit"], c["cases"], c["pass"], c["fail"], c["noref_inj"], c["noref_other"], c["skip"],
                len(fx) if prev else "-", len(rg) if prev else "-"))
            prev = r
        out.append("")
        if not any(k[0] == plat for k in latest["cases"]):
            out += ["(newest run `%s` has no %s results)" % (latest["name"], plat), ""]
            continue
        fails = sorted((k, v) for k, v in latest["cases"].items() if k[0] == plat and v[0] == "fail")
        out.append("**Failing in `%s` (%d):**" % (latest["name"], len(fails)))
        out.append("")
        for k, (st, det) in fails:
            tag = " **[DANGER]**" if DANGER.search(k[1]) else ""
            out.append("- `%s` (%s)%s, failing %d run(s) in a row: %s" % (k[1], k[2], tag, streak(runs, k), short(det)))
        if not fails:
            out.append("- none")
        out.append("")
        un = sorted((k, v) for k, v in latest["cases"].items() if k[0] == plat and v[0] == "noref" and noref_kind(v[1]) == "other")
        if un:
            out.append("**NOREF without the injector explanation (%d):** " % len(un) + ", ".join("`%s` (%s)" % (k[1], k[2]) for k, _ in un))
            out.append("")
        if len(runs) >= 2:
            before = None
            for r in runs[:-1]:
                if any(k[0] == plat for k in r["cases"]):
                    before = r
            if before:
                fx, rg = compare(before, latest, plat)
                out.append("**Regressions in `%s` vs `%s` (%d):** %s" % (latest["name"], before["name"], len(rg),
                           ", ".join("`%s` (%s)" % x for x in rg) or "none"))
                out.append("")
                out.append("**Fixed in `%s` vs `%s` (%d):** %s" % (latest["name"], before["name"], len(fx),
                           ", ".join("`%s` (%s)" % x for x in fx) or "none"))
                out.append("")
    dang = sorted((k, v) for k, v in latest["cases"].items() if v[0] == "fail" and DANGER.search(k[1]))
    out += ["## Danger classes (must be zero FAIL)", "",
            "Cases whose name matches lost/reordered keys, secrets, typeahead or exactness. Newest run `%s`:" % latest["name"], ""]
    out += ["- **%s** `%s` (%s): %s" % (k[0], k[1], k[2], short(v[1], 110)) for k, v in dang] or ["- none failing"]
    out += ["", "## Unit tests (newest run)", ""]
    uf = [(j, n) for j, (cnt, names) in latest["unit"].items() for n in names]
    out += ["- `%s`: `%s`" % x for x in uf] or ["- no unit failures recorded"]
    return "\n".join(out) + "\n"


def self_test():
    def mk(name, created, rows, unit=None):
        d = tempfile.mkdtemp(prefix="trend_")
        cases = [{"name": n, "mode": m, "status": s, "detail": det} for (n, m, s, det) in rows]
        json.dump({"commit": "abc123456", "created_at": created, "app": {"macos": {"cases": cases}}, "unit": unit or {}},
                  open(os.path.join(d, "manifest.json"), "w"))
        return d
    a = mk("a", "2026-11-01", [("x", "terminal", "fail", "boom"), ("y", "terminal", "pass", ""), ("key_loss_control", "terminal", "pass", ""),
                               ("z", "terminal", "noref", "injector accounting: 3 of 9 never reached the app")])
    b = mk("b", "2026-11-02", [("x", "terminal", "pass", ""), ("y", "terminal", "fail", "bad"), ("key_loss_control", "terminal", "fail", "lost 2"),
                               ("z", "terminal", "noref", "within noise"), ("new", "terminal", "fail", "n")],
           unit={"test-linux": {"failed": 1, "failures": [{"test": "mod::t"}]}})
    runs = load_runs([a, b])
    assert [r["created"] for r in runs] == ["2026-11-01", "2026-11-02"]
    fx, rg = compare(runs[0], runs[1], "macos")
    assert fx == [("x", "terminal")] and sorted(rg) == [("key_loss_control", "terminal"), ("y", "terminal")], (fx, rg)
    assert counts(runs[0], "macos")["noref_inj"] == 1 and counts(runs[1], "macos")["noref_other"] == 1
    # #2043: a case lost to a missing test host is listed by name, whatever the injector accounting appended to its row
    lost = "NO-REFERENCE: TEST HOST MISSING: the ssh test host of this pass did not start; injector accounting: UNVERIFIABLE: no key trace"
    assert noref_kind(lost) == "other" and noref_kind("injector accounting: UNVERIFIABLE: no key trace") == "injector"
    h = load_runs([mk("h", "2026-11-03", [("ssh_connect_typing", "terminal", "noref", lost), ("ssh_test_host", "terminal", "fail", "TEST HOST MISSING")])])
    hm = render(h, 5)
    assert "NOREF without the injector explanation (1):** `ssh_connect_typing` (terminal)" in hm and "`ssh_test_host` (terminal)" in hm, hm
    assert streak(runs, ("macos", "y", "terminal")) == 1 and streak(runs, ("macos", "new", "terminal")) == 1 and streak(runs, ("macos", "x", "terminal")) == 0
    md = render(runs, 5)
    assert "[DANGER]" in md and "key_loss_control" in md and "`mod::t`" in md and "Regressions in `" in md, md
    assert "new" in md and "failing 1 run(s) in a row" in md
    assert render([], 3).strip().endswith("No runs found.")
    print("results_trend self-test ok")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--last", type=int, default=4, help="newest N runs (default 4)")
    ap.add_argument("--from", dest="extra", action="append", default=[], help="extra run directory holding manifest.json (not committed yet)")
    ap.add_argument("--out", default=os.path.join(ROOT, "test-results", "TREND.md"))
    ap.add_argument("--stdout", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    dirs = sorted(glob.glob(os.path.join(ROOT, "test-results", "*", ""))) + a.extra
    md = render(load_runs(dirs), a.last)
    if a.stdout:
        sys.stdout.write(md)
    else:
        open(a.out, "w").write(md)
        print("wrote %s (%d runs read)" % (os.path.relpath(a.out, ROOT), len(load_runs(dirs))))


if __name__ == "__main__":
    main()
