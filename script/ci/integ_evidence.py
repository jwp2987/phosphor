#!/usr/bin/env python3
# What an integration-linux job keeps so a failed scenario can be judged afterwards (#2122). Run as the job's last step:
#
#   python3 script/ci/integ_evidence.py finish <dir> [--max-file-mb 7] [--max-total-mb 150]
#   python3 script/ci/integ_evidence.py --self-test
#
# <dir> is $WARP_INTEGRATION_EVIDENCE_DIR of the run:
#   logs/<scenario>.<attempt>.log   the whole output of one start of a scenario that did NOT pass (the test shim, crates/integration/tests/common/
#                                   evidence.rs, removes the log of a start that passed). INFO log plus the `warp::input_order` debug target: every
#                                   injected event in order (`integ_event seq=`), every pty write, menu and hover state (`hop=`), the failure state
#                                   (`integ_failure_state`). Typed text is in there by its length only (#2040).
#   skips/<scenario>.skip           one `INTEG-SKIP <scenario> NEEDS: <reason>` line, written by the driver when the scenario's own predicate said no.
#                                   Such a scenario exits 0 and nextest prints PASS for it: these files are how it is not counted as a pass.
#   artifacts/<scenario>/<time>/    the driver's own files (recording.log = the action log, screenshots).
#
# `finish` makes that an artifact of bounded size and says what is in it:
#   1. artifacts/ of a scenario with no kept log are removed (it passed);
#   2. a file over --max-file-mb keeps its first MiB and its end, with a line between saying how much was cut;
#   3. over --max-total-mb in all, the largest files go first, each named in DROPPED.txt (never silently);
#   4. prints one INTEG-SKIP line per skipped scenario and one `INTEG-EVIDENCE ...` line of counts, and writes both to SUMMARY.txt.
#      script/ci/collect_results.py reads the INTEG-SKIP lines from the job trace: those scenarios are taken out of "passed".
# It never fails the job (exit 0 unless its arguments are wrong): the nextest exit status is the job's verdict.
import argparse
import os
import shutil
import sys

MIB = 1024 * 1024
SKIP_MARKER = "INTEG-SKIP"
CUT_LINE = "\n=== integ_evidence: %d bytes cut here (file was %d bytes; the first MiB and the end are kept) ===\n"


def scenario_of_log(name):
    """logs/<scenario>.<attempt>[.rerunN].log -> <scenario> (a scenario name has no dot)."""
    return name.split(".", 1)[0]


def files_under(root):
    out = []
    for base, _, names in os.walk(root):
        for n in names:
            p = os.path.join(base, n)
            if os.path.isfile(p) and not os.path.islink(p):
                out.append(p)
    return sorted(out)


def truncate_file(path, max_bytes):
    """Keep the first MiB and the end of a file over max_bytes. Returns the bytes cut (0 = untouched)."""
    size = os.path.getsize(path)
    if size <= max_bytes:
        return 0
    head = min(MIB, max_bytes // 2)
    tail = max_bytes - head
    with open(path, "rb") as f:
        first = f.read(head)
        f.seek(size - tail)
        last = f.read(tail)
    cut = size - head - tail
    with open(path, "wb") as f:
        f.write(first)
        f.write((CUT_LINE % (cut, size)).encode())
        f.write(last)
    return cut


def read_skips(root):
    """{scenario: reason line} from skips/*.skip. A file that does not hold the marker line still counts: the scenario did not run."""
    out = {}
    d = os.path.join(root, "skips")
    if not os.path.isdir(d):
        return out
    for n in sorted(os.listdir(d)):
        if not n.endswith(".skip"):
            continue
        scenario = n[:-len(".skip")]
        try:
            first = open(os.path.join(d, n), errors="replace").readline().strip()
        except OSError:
            first = ""
        if not first.startswith(SKIP_MARKER + " "):
            first = "%s %s NEEDS: (the skip marker file holds no reason)" % (SKIP_MARKER, scenario)
        out[scenario] = " ".join(first.split())
    return out


def finish(root, max_file_mb=7, max_total_mb=150):
    """Bound the evidence directory and return the summary lines (also written to SUMMARY.txt)."""
    os.makedirs(root, exist_ok=True)
    logs_dir, art_dir = os.path.join(root, "logs"), os.path.join(root, "artifacts")
    logs = [n for n in sorted(os.listdir(logs_dir)) if n.endswith(".log")] if os.path.isdir(logs_dir) else []
    failed = sorted({scenario_of_log(n) for n in logs})
    removed_artifacts = 0
    if os.path.isdir(art_dir):
        for n in sorted(os.listdir(art_dir)):
            if n not in failed:
                shutil.rmtree(os.path.join(art_dir, n), ignore_errors=True)
                removed_artifacts += 1
    cut_files = []
    for p in files_under(logs_dir) + files_under(art_dir):
        cut = truncate_file(p, max_file_mb * MIB)
        if cut:
            cut_files.append((os.path.relpath(p, root), cut))
    kept = [(os.path.getsize(p), p) for p in files_under(logs_dir) + files_under(art_dir)]
    total = sum(s for s, _ in kept)
    dropped = []
    for size, p in sorted(kept, reverse=True):
        if total <= max_total_mb * MIB:
            break
        os.remove(p)
        total -= size
        dropped.append((os.path.relpath(p, root), size))
    if dropped:
        with open(os.path.join(root, "DROPPED.txt"), "w") as f:
            f.write("removed to keep the artifact under %d MiB, largest first:\n" % max_total_mb)
            f.writelines("%s (%d bytes)\n" % d for d in dropped)
    skips = read_skips(root)
    left = [n for n in logs if os.path.exists(os.path.join(logs_dir, n))]
    lines = [skips[k] for k in sorted(skips)]
    lines += ["INTEG-EVIDENCE kept log: logs/%s" % n for n in left]
    lines += ["INTEG-EVIDENCE cut: %s (%d bytes)" % c for c in cut_files]
    lines += ["INTEG-EVIDENCE dropped: %s (%d bytes)" % d for d in dropped]
    lines.append("INTEG-EVIDENCE scenarios_not_passed=%d logs=%d bytes=%d skipped_themselves=%d cut=%d dropped=%d passed_artifacts_removed=%d" % (
        len(failed), len(left), total, len(skips), len(cut_files), len(dropped), removed_artifacts))
    if skips:
        lines.append("INTEG-EVIDENCE %d scenario(s) above did NOT run: nextest printed PASS for them (exit 0); they are skips, not passes" % len(skips))
    with open(os.path.join(root, "SUMMARY.txt"), "w") as f:
        f.write("\n".join(lines) + "\n")
    return lines


def self_test():
    import tempfile
    rows = []
    with tempfile.TemporaryDirectory(dir=os.environ.get("TMPDIR") or None) as root:
        def put(rel, data):
            p = os.path.join(root, rel)
            os.makedirs(os.path.dirname(p), exist_ok=True)
            with open(p, "wb") as f:
                f.write(data)
            return p
        put("logs/test_a.try1.log", b"A" * (3 * MIB))
        put("logs/test_a.try2.rerun1.log", b"small\n")
        put("logs/test_b.pid77.log", b"H" * MIB + b"M" * (2 * MIB) + b"TAIL")
        put("artifacts/test_a/2026-01-01T00-00-00/recording.log", b"[+00:00.000] Step started\n")
        put("artifacts/test_passed/2026-01-01T00-00-00/recording.log", b"x\n")
        put("skips/test_s.skip", b"INTEG-SKIP test_s NEEDS:   sshd   (the OpenSSH server)\nsecond line\n")
        put("skips/test_t.skip", b"")
        put("skips/README", b"not a marker\n")
        lines = finish(root, max_file_mb=2, max_total_mb=150)
        rows.append((lines[0] == "INTEG-SKIP test_s NEEDS: sshd (the OpenSSH server)" and lines[1].startswith("INTEG-SKIP test_t NEEDS: (the skip marker file holds no reason)"),
                     "one INTEG-SKIP line per skipped scenario, first, whitespace folded; an empty marker still counts"))
        rows.append((not os.path.exists(os.path.join(root, "artifacts/test_passed")) and os.path.exists(os.path.join(root, "artifacts/test_a/2026-01-01T00-00-00/recording.log")),
                     "the driver's files of a scenario that passed are removed, those of a failed one kept"))
        b = open(os.path.join(root, "logs/test_b.pid77.log"), "rb").read()
        rows.append((b.startswith(b"H" * MIB + b"\n=== integ_evidence: ") and (b"%d bytes cut here" % (MIB + 4)) in b and b.endswith(b"M" * 16 + b"TAIL") and len(b) < 2 * MIB + 200,
                     "a file over the per-file bound keeps its first MiB and its END (what the failing step printed) and says how much was cut"))
        rows.append((open(os.path.join(root, "logs/test_a.try2.rerun1.log"), "rb").read() == b"small\n" and not os.path.exists(os.path.join(root, "DROPPED.txt")), "a file under the bound is untouched; nothing is dropped under the total bound"))
        last = [l for l in lines if l.startswith("INTEG-EVIDENCE scenarios_not_passed=")]
        rows.append((len(last) == 1 and "scenarios_not_passed=2 logs=3 " in last[0] and "skipped_themselves=2 cut=2 dropped=0 passed_artifacts_removed=1" in last[0],
                     "the count line: failed scenarios, logs, skips, what was cut, dropped and removed"))
        rows.append((lines[-1].endswith("they are skips, not passes") and open(os.path.join(root, "SUMMARY.txt")).read().splitlines() == lines, "the summary says a skip is not a pass and is written to SUMMARY.txt as printed"))
        again = finish(root, max_file_mb=8, max_total_mb=3)
        total = sum(os.path.getsize(p) for p in files_under(os.path.join(root, "logs")) + files_under(os.path.join(root, "artifacts")))
        dropped = open(os.path.join(root, "DROPPED.txt")).read()
        rows.append((total <= 3 * MIB and len([l for l in again if l.startswith("INTEG-EVIDENCE dropped: logs/test_")]) == 1 and dropped.count("logs/test_") == 1
                     and os.path.exists(os.path.join(root, "logs/test_a.try2.rerun1.log")) and any("logs=2 " in l and "dropped=1 " in l for l in again),
                     "over the total bound the largest file goes first and is NAMED (DROPPED.txt and the trace); the small ones stay"))
    with tempfile.TemporaryDirectory(dir=os.environ.get("TMPDIR") or None) as root:
        empty = finish(os.path.join(root, "never-made"))
        rows.append((empty == ["INTEG-EVIDENCE scenarios_not_passed=0 logs=0 bytes=0 skipped_themselves=0 cut=0 dropped=0 passed_artifacts_removed=0"], "a run that kept nothing (all passed, or died before the first scenario) is one count line, not an error"))
    rows.append((scenario_of_log("test_x.try2.rerun3.log") == "test_x" and scenario_of_log("test_x.pid9.log") == "test_x", "a log name gives its scenario"))
    bad = 0
    for ok, what in rows:
        bad += 0 if ok else 1
        print("%s %s" % ("ok  " if ok else "FAIL", what))
    print("self-test: %s" % ("FAILED (%d)" % bad if bad else "all passed"))
    return 1 if bad else 0


def main():
    ap = argparse.ArgumentParser(description="bound and summarise an integration job's evidence directory (#2122)")
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("command", nargs="?", choices=["finish"])
    ap.add_argument("dir", nargs="?")
    ap.add_argument("--max-file-mb", type=int, default=7,
                    help="per file; under the 8 MiB above which results_scrub.py --tree leaves a file unscrubbed")
    ap.add_argument("--max-total-mb", type=int, default=150)
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    if a.command != "finish" or not a.dir:
        ap.error("usage: integ_evidence.py finish <dir>")
    try:
        for line in finish(a.dir, a.max_file_mb, a.max_total_mb):
            print(line)
    except Exception as e:                                   # the job's verdict is nextest's: a problem here is said, not raised
        print("INTEG-EVIDENCE ERROR could not finish %s: %r" % (a.dir, e))
    return 0


if __name__ == "__main__":
    sys.exit(main())
