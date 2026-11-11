# appci/winocr.py (#2060): OCR on the Windows runner through windows/lib/ocr.ps1 (Windows.Media.Ocr, part of Windows 10 1507+; no tesseract, nothing to
# install on the VM except, on a machine whose languages have no OCR pack, the en-US one). The Python side is pure where it can be: the command line,
# the parser of the script's output, the scaling and offset that turn the engine's image pixels into screen pixels. Only read()/available() start a
# process. Output of the same shapes the Linux/macOS readers give the cases: ctx.ocr -> one text, cases_ui.words -> (text, cx, cy, left, right, top, bottom).
import os
import subprocess

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.normpath(os.path.join(HERE, "..", "windows", "lib", "ocr.ps1"))
SCALE_ENV, DISABLE_ENV, LANG_ENV = "APPCI_WINOCR_SCALE", "APPCI_WINOCR", "APPCI_WINOCR_LANG"
DEFAULT_SCALE = 2          # Windows.Media.Ocr misses the terminal's small text at 1x as tesseract does; a 2x copy is read and the boxes scaled back
MAX_DIM = 10000            # OcrEngine.MaxImageDimension
EXIT_REASONS = {2: "the image file is missing", 3: "the image is larger than OcrEngine.MaxImageDimension", 4: "no OCR engine on this machine (no OCR language pack)",
                5: "the OCR call failed"}


def scale_from_env(environ=None):
    """pure: the upscale factor (1..4) from APPCI_WINOCR_SCALE, DEFAULT_SCALE when unset or not a number"""
    try:
        v = int((environ if environ is not None else os.environ).get(SCALE_ENV, DEFAULT_SCALE))
    except (TypeError, ValueError):
        return DEFAULT_SCALE
    return min(4, max(1, v))


def effective_scale(width, height, want):
    """pure: the largest factor <= want that keeps both sides within MAX_DIM (at least 1)"""
    s = int(want)
    while s > 1 and (width * s > MAX_DIM or height * s > MAX_DIM):
        s -= 1
    return max(1, s)


def ps_command(path, lang="", probe=False, script=None, exe="powershell.exe"):
    """pure: the argv that runs the OCR script on `path` (or only probes for an engine)"""
    cmd = [exe, "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", script or SCRIPT]
    if probe:
        cmd.append("-Probe")
    else:
        cmd += ["-Path", path]
    if lang:
        cmd += ["-Lang", lang]
    return cmd


def parse(out):
    """pure: ocr.ps1 stdout -> (lines, words); lines = [text], words = [(text, x, y, w, h)] in image pixels. Malformed records are dropped."""
    lines, words = [], []
    for rec in (out or "").splitlines():
        c = rec.rstrip("\r").split("\t")
        if c[0] == "L" and len(c) >= 2:
            lines.append(c[1].strip())
        elif c[0] == "W" and len(c) >= 6 and c[5].strip():
            try:
                x, y, w, h = (int(v) for v in c[1:5])
            except ValueError:
                continue
            words.append((c[5].strip(), x, y, w, h))
    return lines, words


def to_screen_words(words, scale=1, origin=(0, 0)):
    """pure: [(text, x, y, w, h)] in the pixels of an image upscaled by `scale` -> [(text, cx, cy, left, right, top, bottom)] in SCREEN pixels, the
    shape cases_ui.words gives. origin = the screen position of the original screenshot's top-left (the virtual desktop's left/top: the shot is
    ImageGrab of all monitors, so a monitor left of the primary makes it negative)."""
    ox, oy = origin
    res = []
    for t, x, y, w, h in words:
        l, tp = x // scale + ox, y // scale + oy
        rt, bt = (x + w) // scale + ox, (y + h) // scale + oy
        res.append((t, (l + rt) // 2, (tp + bt) // 2, l, rt, tp, bt))
    return res


def shot_to_screen(px, py, origin=(0, 0)):
    """pure: a pixel of the screenshot -> the screen point to hand the pointer"""
    return px + origin[0], py + origin[1]


def run(path, probe=False, timeout=90, runner=None, lang=None):
    """(exit code, stdout, stderr) of one ocr.ps1 run; runner(argv, timeout) -> CompletedProcess is replaceable for the self-test"""
    lang = os.environ.get(LANG_ENV, "") if lang is None else lang
    argv = ps_command(path, lang, probe)
    try:
        p = (runner or (lambda a, t: subprocess.run(a, capture_output=True, timeout=t)))(argv, timeout)
    except Exception as e:                                  # powershell.exe missing, timeout
        return 5, "", "could not run %s: %r" % (argv[0], e)
    return p.returncode, p.stdout.decode("utf-8", errors="replace"), p.stderr.decode("utf-8", errors="replace")


_probe = {}


def available(runner=None):
    """True when an OCR engine can be made here: cached after the first probe; APPCI_WINOCR=0 turns it off"""
    if os.environ.get(DISABLE_ENV, "1") == "0" or (os.name != "nt" and runner is None):
        return False
    if runner is not None:
        return run("", probe=True, runner=runner)[0] == 0
    if "ok" not in _probe:
        _probe["ok"] = run("", probe=True, timeout=60)[0] == 0
    return _probe["ok"]


def _upscaled(path, want):
    """(path to read, scale used): a 2x LANCZOS copy next to the shot when Pillow is there, else the shot itself"""
    try:
        from PIL import Image
        im = Image.open(path)
        s = effective_scale(im.width, im.height, want)
        if s <= 1:
            return path, 1
        out = os.path.splitext(path)[0] + ".winocr%dx.png" % s
        im.convert("L").resize((im.width * s, im.height * s), Image.LANCZOS).save(out)
        return out, s
    except Exception:
        return path, 1


def read(path, origin=(0, 0), runner=None):
    """OCR of a screenshot -> (text, words) with words in screen pixels (to_screen_words), or None when no engine / the read failed. The plain read
    and the upscaled read are both taken and ADDED (as cases.ocr does with tesseract on X11): nothing the plain read found is lost."""
    if not available(runner):
        return None
    texts, words = [], []
    sources = [(path, 1)]
    if scale_from_env() > 1:
        up = _upscaled(path, scale_from_env())
        if up[1] > 1:
            sources.append(up)
    for p, s in sources:
        code, out, _err = run(p, runner=runner)
        if code != 0:
            if s == 1:
                return None
            continue
        lines, ws = parse(out)
        texts.append("\n".join(lines))
        words += to_screen_words(ws, s, origin)
    return "\n".join(texts), words
