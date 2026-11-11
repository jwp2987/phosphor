#!/usr/bin/env python3
"""Make an appci run directory small enough to upload as a GitLab job artifact.

r18u8 (2026-11-04): the macOS app-test job's artifacts (467 PNG screenshots + 102 logs) were
rejected with "413 Payload Too Large" and the whole run's evidence was lost. This copies a run
into <run>_slim with:
  - results.json, summary.md, junit.xml as they are;
  - logs/: files up to 16 MiB copied, bigger ones gzip-compressed;
  - shots/: every PNG re-encoded as JPEG (quality 60, longest side at most 1600 px) with Pillow,
    else with macOS `sips`; if neither is available, only the shots referenced by a non-passing
    case in results.json are kept (as PNG) and the rest are listed in shots/OMITTED.txt.
It never modifies the original run directory.

#2123 (round 14: the Windows job's after_script ran into GitLab's 5 min after_script limit before this script had started, and the failing run
kept no run_slim logs or shots). The app-test job scripts now run this themselves on every exit path (script/ci/appci_artifacts.py), where only
the job timeout applies; the after_script pass is the net for a job that was cancelled or killed. For that:
  - <run>_slim/SLIM_DONE.json records a finished pass and what it was made from (file count, bytes and newest mtime of logs/ and shots/);
    is_current(run) is true while the run still looks like that, so the net does not redo a finished pass;
  - --budget-s N (the net only): logs first, then the shots of non-passing cases, then the rest; when N seconds are used up the shots not yet
    converted are left out and listed in shots/OMITTED.txt (a pass without a budget, i.e. the job script's, leaves nothing out).

usage: slim_appci_run.py <run_dir> [--max-mib N] [--budget-s N]   (default N=900 MiB; warns when the result is bigger)
"""
import gzip, json, os, shutil, subprocess, sys, time

LOG_LIMIT = 16 * 1024 * 1024
STAMP = "SLIM_DONE.json"
PART = ".part-"


def fingerprint(run):
    """what a slim pass was made from: [files, bytes, newest mtime_ns] of logs/ and shots/ (the two trees run_slim uploads)"""
    n = size = newest = 0
    for sub in ("logs", "shots"):
        for root, _, files in os.walk(os.path.join(run, sub)):
            for f in files:
                try:
                    st = os.stat(os.path.join(root, f))
                except OSError:
                    continue
                n, size, newest = n + 1, size + st.st_size, max(newest, st.st_mtime_ns)
    return [n, size, newest]


def is_current(run):
    """True when <run>_slim holds a FINISHED pass (nothing left out for time) made from logs/ and shots/ as they are now"""
    try:
        d = json.load(open(os.path.join(run.rstrip("/\\") + "_slim", STAMP)))
        return bool(d.get("complete")) and d.get("fingerprint") == fingerprint(run)
    except Exception:
        return False


def dir_size(p):
    t = 0
    for root, _, files in os.walk(p):
        for f in files:
            try:
                t += os.path.getsize(os.path.join(root, f))
            except OSError:
                pass
    return t


def failing_shots(run):
    keep = set()
    try:
        rows = json.load(open(os.path.join(run, "results.json")))
        rows = rows.get("cases", rows) if isinstance(rows, dict) else rows
        for r in rows:
            if isinstance(r, dict) and r.get("status") not in ("pass", "skip"):
                for s in r.get("shots") or []:
                    if s:
                        keep.add(os.path.basename(s))
    except Exception as e:
        print("slim: results.json unreadable (%r): keeping no shots by status" % e)
    return keep


def gzip_file(src, dst):
    with open(src, "rb") as i, gzip.open(dst, "wb") as o:
        shutil.copyfileobj(i, o)


def jpeg_pillow(src, dst):
    from PIL import Image
    with Image.open(src) as im:
        im.thumbnail((1600, 1600))
        im.convert("RGB").save(dst, "JPEG", quality=60, optimize=True)


def jpeg_sips(src, dst):
    subprocess.run(["sips", "-s", "format", "jpeg", "-s", "formatOptions", "60", "-Z", "1600", src, "--out", dst],
                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def slim(run, max_mib=900, budget_s=None, clock=time.monotonic):
    """write <run>_slim; returns dict(complete=..., omitted=N, mib=...). budget_s: see the module text."""
    run = run.rstrip("/\\")
    out = run + "_slim"
    t0 = clock()
    if not os.path.isdir(run):
        print("slim: no run dir %s (nothing to do)" % run)
        return dict(complete=True, omitted=0, mib=0.0)
    fp = fingerprint(run)
    # The budgeted pass (the after_script net) keeps what an earlier pass already made and only adds what is missing or older than its source: a
    # finished pass that merely went stale must not be thrown away for one that may run out of time. Every file is written under a temporary name
    # and renamed, so a file a killed pass was in the middle of is never taken for a finished one.
    keep_old = budget_s is not None
    if not keep_old:
        shutil.rmtree(out, ignore_errors=True)
    os.makedirs(out, exist_ok=True)
    for root, _, files in os.walk(out):         # what a killed pass was in the middle of
        for f in files:
            if f.startswith(PART):
                os.remove(os.path.join(root, f))

    def fresh(src, dst):
        try:
            return keep_old and os.path.getmtime(dst) >= os.path.getmtime(src)
        except OSError:
            return False

    def put(fn, src, dst):
        if fresh(src, dst):
            return
        tmp = os.path.join(os.path.dirname(dst), PART + os.path.basename(dst))    # the extension stays last: sips reads it
        fn(src, tmp)
        os.replace(tmp, dst)
    for f in ("results.json", "summary.md", "junit.xml"):
        if os.path.exists(os.path.join(run, f)):
            shutil.copy2(os.path.join(run, f), os.path.join(out, f))
    logs = os.path.join(run, "logs")
    for root, _, files in os.walk(logs):
        rel = os.path.relpath(root, run)
        os.makedirs(os.path.join(out, rel), exist_ok=True)
        for f in files:
            src = os.path.join(root, f)
            dst = os.path.join(out, rel, f)
            try:
                if os.path.getsize(src) > LOG_LIMIT:
                    put(gzip_file, src, dst + ".gz")
                else:
                    put(shutil.copy2, src, dst)
            except OSError as e:            # one unreadable log (still held open on Windows) must not cost the rest of the run's evidence
                print("slim: %s not copied (%r)" % (os.path.join(rel, f), e))
    t_logs = clock()
    shots = os.path.join(run, "shots")
    late = []
    if os.path.isdir(shots):
        os.makedirs(os.path.join(out, "shots"), exist_ok=True)
        conv = None
        try:
            import PIL  # noqa: F401
            conv = jpeg_pillow
        except ImportError:
            if shutil.which("sips"):
                conv = jpeg_sips
        failing = failing_shots(run) if (conv is None or budget_s is not None) else set()
        keep = None if conv else failing
        todo = []
        for root, _, files in os.walk(shots):
            rel = os.path.relpath(root, run)
            os.makedirs(os.path.join(out, rel), exist_ok=True)
            todo += [(rel, f) for f in sorted(files)]
        if budget_s is not None:            # the evidence of what failed first; sorted() is stable, so the rest keeps its order
            todo.sort(key=lambda x: x[1] not in failing)
        omitted = []
        for rel, f in todo:
            src = os.path.join(run, rel, f)
            if budget_s is not None and clock() - t0 > budget_s:
                late.append(os.path.join(rel, f))
                continue
            if conv and f.lower().endswith(".png"):
                try:
                    put(conv, src, os.path.join(out, rel, f[:-4] + ".jpg"))
                    continue
                except Exception as e:
                    print("slim: %s not converted (%r); copied as is" % (f, e))
            if keep is None or f in keep or not f.lower().endswith(".png"):
                try:
                    put(shutil.copy2, src, os.path.join(out, rel, f))
                except OSError as e:
                    print("slim: %s not copied (%r)" % (os.path.join(rel, f), e))
            else:
                omitted.append(os.path.join(rel, f))
        note = ""
        if omitted:
            note += "No Pillow or sips: shots of passing cases were left out (%d):\n%s\n" % (len(omitted), "\n".join(omitted))
        if late:
            note += ("The %d s budget of the after_script pass ran out (the job script's own pass did not finish): %d shot(s) were left out, "
                     "shots of non-passing cases were done first:\n%s\n" % (budget_s, len(late), "\n".join(late)))
        op = os.path.join(out, "shots", "OMITTED.txt")
        if note:
            open(op, "w").write(note)
        elif os.path.exists(op):
            os.remove(op)
    size = dir_size(out)
    complete = not late
    json.dump(dict(complete=complete, fingerprint=fp, omitted_for_time=len(late)), open(os.path.join(out, STAMP), "w"))
    print("slim: %s -> %s: %.1f MiB (was %.1f MiB); logs %.0f s, shots %.0f s%s" % (
        run, out, size / 2**20, dir_size(run) / 2**20, t_logs - t0, clock() - t_logs,
        "" if complete else "; INCOMPLETE: %d shot(s) left out when the %d s budget ran out (shots/OMITTED.txt)" % (len(late), budget_s)))
    if size > max_mib * 2**20:
        print("slim: WARNING still over %d MiB; the upload may be rejected" % max_mib)
    return dict(complete=complete, omitted=len(late), mib=size / 2**20)


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    max_mib = int(sys.argv[sys.argv.index("--max-mib") + 1]) if "--max-mib" in sys.argv else 900
    budget = float(sys.argv[sys.argv.index("--budget-s") + 1]) if "--budget-s" in sys.argv else None
    slim(sys.argv[1], max_mib, budget)
    return 0


if __name__ == "__main__":
    sys.exit(main())
