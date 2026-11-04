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

usage: slim_appci_run.py <run_dir> [--max-mib N]   (default N=900; warns when the result is bigger)
"""
import gzip, json, os, shutil, subprocess, sys

LOG_LIMIT = 16 * 1024 * 1024


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


def jpeg_pillow(src, dst):
    from PIL import Image
    im = Image.open(src)
    im.thumbnail((1600, 1600))
    im.convert("RGB").save(dst, "JPEG", quality=60, optimize=True)


def jpeg_sips(src, dst):
    subprocess.run(["sips", "-s", "format", "jpeg", "-s", "formatOptions", "60", "-Z", "1600", src, "--out", dst],
                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    run = sys.argv[1].rstrip("/")
    max_mib = int(sys.argv[sys.argv.index("--max-mib") + 1]) if "--max-mib" in sys.argv else 900
    out = run + "_slim"
    if not os.path.isdir(run):
        print("slim: no run dir %s (nothing to do)" % run)
        return 0
    shutil.rmtree(out, ignore_errors=True)
    os.makedirs(out)
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
            if os.path.getsize(src) > LOG_LIMIT:
                with open(src, "rb") as i, gzip.open(dst + ".gz", "wb") as o:
                    shutil.copyfileobj(i, o)
            else:
                shutil.copy2(src, dst)
    shots = os.path.join(run, "shots")
    if os.path.isdir(shots):
        os.makedirs(os.path.join(out, "shots"))
        conv = None
        try:
            import PIL  # noqa: F401
            conv = jpeg_pillow
        except ImportError:
            if shutil.which("sips"):
                conv = jpeg_sips
        keep = None if conv else failing_shots(run)
        omitted = []
        for root, _, files in os.walk(shots):
            rel = os.path.relpath(root, run)
            os.makedirs(os.path.join(out, rel), exist_ok=True)
            for f in files:
                src = os.path.join(root, f)
                if conv and f.lower().endswith(".png"):
                    try:
                        conv(src, os.path.join(out, rel, f[:-4] + ".jpg"))
                        continue
                    except Exception as e:
                        print("slim: %s not converted (%r); copied as is" % (f, e))
                if keep is None or f in keep or not f.lower().endswith(".png"):
                    shutil.copy2(src, os.path.join(out, rel, f))
                else:
                    omitted.append(os.path.join(rel, f))
        if omitted:
            open(os.path.join(out, "shots", "OMITTED.txt"), "w").write(
                "No Pillow or sips: shots of passing cases were left out (%d):\n%s\n" % (len(omitted), "\n".join(omitted)))
    size = dir_size(out)
    print("slim: %s -> %s: %.1f MiB (was %.1f MiB)" % (run, out, size / 2**20, dir_size(run) / 2**20))
    if size > max_mib * 2**20:
        print("slim: WARNING still over %d MiB; the upload may be rejected" % max_mib)
    return 0


if __name__ == "__main__":
    sys.exit(main())
