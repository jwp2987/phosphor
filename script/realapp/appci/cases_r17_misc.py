# appci/cases_r17_misc.py: round 17 misc (docs/design/r17-misc.md). Registered with the same @case decorator as cases.py (one import line at its
# end). The lost-device arm of gpu_real_render (#2091) lives with its case in cases_r15_w5c.py; its pure parts are self-tested from
# selftest_r17_misc.py together with this file's.
#
#   md_viewer_first_frame   #2028. A markdown file opened in a viewer tab used to be laid out BEFORE the pane had a width: the render model starts at
#                           0 x 0 and laid the file out when it arrived, so the first frame showed one glyph per line and an image block with a
#                           content width of 0 minus its padding (the app logged `image paint with suspicious size ... element_size=(-20, ..`),
#                           until the debounced resize re-laid it out. The viewer now holds its layout until its element has given the model a
#                           width and logs `rich text: first size <w>x<h> released <n> held layout edit(s)` when it does. The case opens a file
#                           with a heading, an image and a paragraph and judges, from the app log since the open:
#                             1. the first-size line is there and its width is above zero;
#                             2. no `image paint with suspicious size` line.
#                           A frame cannot be caught by a screenshot reliably (the broken picture lasted about 150 ms), so the log is the oracle;
#                           the heading is also read by OCR where there is an engine (a note without one). Control, where the launcher can relaunch
#                           with an environment (Linux): the same file with PHOSPHOR_MD_FIRST_LAYOUT_HOLD=0 must NOT log the first-size line (the
#                           switch works and the line belongs to the hold); whether the old build's warning shows there is a metric, not a verdict
#                           (it depends on whether the file arrives before the first frame).
import os, re, struct, sys, time, zlib

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402  (cases.py imports this module near its end, so everything below is already defined)
import cases_robust as R  # noqa: E402
from cases import case  # noqa: E402

# ================================================================ pure parts (selftest_r17_misc.py)
# crates/editor/src/render/model/mod.rs release_first_size_hold: log::info!("rich text: first size {}x{} released {} held layout edit(s)", ..)
FIRST_SIZE_RE = re.compile(r"rich text: first size (-?\d+(?:\.\d+)?)x(-?\d+(?:\.\d+)?) released (\d+) held layout edit\(s\)")
SUSPICIOUS = "image paint with suspicious size"        # crates/warpui_core/src/elements/image.rs paint (WARN)
HOLD_SWITCH = "PHOSPHOR_MD_FIRST_LAYOUT_HOLD"            # app/src/notebooks/file/mod.rs first_layout_hold_enabled: exactly "0" turns the hold off


def first_sizes(log):
    """pure: every first-size release in `log`, in order: [(width, height, held edits)]"""
    return [(float(w), float(h), int(n)) for w, h, n in FIRST_SIZE_RE.findall(log or "")]


def suspicious_lines(log):
    """pure: the app's `image paint with suspicious size` lines in `log`"""
    return [ln.strip() for ln in (log or "").splitlines() if SUSPICIOUS in ln]


def first_frame_verdict(log):
    """pure: (ok, text) for the app log since the file was handed over. ok needs a first-size line with a width above zero and no suspicious image
    paint. A release of 0 held edits passes: the file arrived after the first frame and was laid out at the width the model already had."""
    sizes, bad = first_sizes(log), suspicious_lines(log)
    if bad:
        return False, "%d `%s` line(s) since the file was opened: a block was painted with no width (first: %s)" % (len(bad), SUSPICIOUS, bad[0][-220:])
    if not sizes:
        return False, ("no `rich text: first size` line since the file was opened: the viewer did not hold its layout for the pane width (a build before "
                       "#2028, %s=0 in the app's environment, or the log level hides the line)" % HOLD_SWITCH)
    zero = [s for s in sizes if s[0] <= 0]
    if zero:
        return False, "the viewer's layout was released at a width of %g: the held file was laid out with no width" % zero[0][0]
    w, h, n = sizes[-1]
    return True, "the viewer laid its file out at its first size %gx%g (%d held edit(s)); no block was painted without a width" % (w, h, n)


def control_verdict(log):
    """pure: (ok, text) for the control launch with the hold switched off: no first-size line may be logged"""
    sizes = first_sizes(log)
    if sizes:
        return False, "with %s=0 the app still logged a first-size release (%r): the kill switch does not switch the hold off" % (HOLD_SWITCH, sizes[0])
    return True, "with %s=0 no first-size line is logged (%d suspicious image paint line(s) there: metric only)" % (HOLD_SWITCH, len(suspicious_lines(log)))


def png_bytes(width, height, rgb):
    """pure: a valid one-colour 8-bit RGB PNG (no Pillow needed to MAKE the fixture)"""
    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)
    row = b"\x00" + bytes(rgb) * width
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(row * height)) + chunk(b"IEND", b""))


def fixture_markdown(tok):
    """pure: the file the case opens: a heading OCR can find, an image block (the element that logged the negative width) and a paragraph long
    enough to wrap at any pane width"""
    para = " ".join("word%02d" % i for i in range(1, 121))
    return "# FIRSTFRAME%s\n\n![probe picture](ff_probe.png)\n\n%s\n\n- item one\n- item two\n" % (tok, para)


# ================================================================ the case
def _open_and_read(ctx, r, path, what):
    """hand the file over, wait for the viewer tab and for the log to settle; -> (log since the open, offset) or (None, offset)"""
    off = ctx.log_size()
    err = cases.open_file_in_app(ctx, path)
    if not r.check(err is None, "%s: could not hand the file to the running app: %s" % (what, err)):
        return None, off
    opened = ctx.until(lambda: cases.file_viewer_opened(ctx.log_since(off)), 15, step=0.3)
    if not r.check(opened, "%s: no '%s' line within 15 s: the app did not open the file in a viewer tab" % (what, cases.FILE_PANE_OPENED)):
        return None, off
    ctx.until(lambda: bool(first_sizes(ctx.log_since(off))), 10, step=0.3)      # the release line, where the build has it
    time.sleep(2.0)                                                            # the image load and the frames after it
    return ctx.log_since(off), off


@case("md_viewer_first_frame", order=82, gui=True)
def c_md_viewer_first_frame(ctx, r):
    """#2028 (see the header): a markdown file with an image opened in a viewer tab is laid out at the pane width from its first content frame: the
    app's first-size line has a width above zero and no image block is painted without a width. Control (Linux launcher): with
    PHOSPHOR_MD_FIRST_LAYOUT_HOLD=0 the line is absent."""
    tok = "".join(ctx.rng.choice("ABCDEFHKMNPRTUVWXY") for _ in range(4))
    path = os.path.join(ctx.W, "first_frame_%s.md" % tok)
    with open(os.path.join(ctx.W, "ff_probe.png"), "wb") as f:
        f.write(png_bytes(160, 80, (40, 120, 200)))
    with open(path, "w") as f:
        f.write(fixture_markdown(tok))
    log, off = _open_and_read(ctx, r, path, "md_viewer_first_frame")
    try:
        if log is None:
            return
        ok, text = first_frame_verdict(log)
        r.metric("first_sizes", first_sizes(log))
        r.metric("suspicious_image_paints", len(suspicious_lines(log)))
        r.check(ok, text)
        if ok:
            r.note(text)
        secs, txt = cases.wait_for_text(ctx, "md_first_frame", ("firstframe%s" % tok.lower(),), 15)
        if txt is None:
            ctx.shot("md_first_frame_noocr")
            r.note("no OCR engine: the heading is not machine-read (screenshot shots/%s_md_first_frame_noocr.png)" % ctx.label)
        else:
            r.check(secs is not None, "the viewer's heading FIRSTFRAME%s is not on screen within 15 s (OCR saw %r)" % (tok, (txt or "")[:200]))
        r.check("panicked at" not in ctx.log_since(off), "app log shows a panic while opening the markdown file")
        r.check(ctx.app.alive(), "app died while opening the markdown file")
    finally:
        cases.close_file_viewer(ctx, r, off, "md_viewer_first_frame")
    # ---- control: the hold switched off (needs a launcher that can relaunch with an environment)
    if ctx.win or ctx.be.name == "mac" or ctx.app is None or not hasattr(ctx.app, "extra_env") or not hasattr(ctx.app, "proc"):
        r.note("control arm (the same file with %s=0) not run: this platform's launcher cannot relaunch the app with an extra environment" % HOLD_SWITCH)
        return
    back, why_back = False, "not relaunched"
    try:
        tmp = cases.Result(r.name, r.mode)
        ctx.app.extra_env = {HOLD_SWITCH: "0"}
        _n, up = R.start_app(ctx, tmp, "md_viewer_first_frame control (%s=0)" % HOLD_SWITCH)
        if not up:
            r.unverifiable("control arm: the app did not reach a prompt with %s=0 (%s): the switch is not judged" % (HOLD_SWITCH, "; ".join(tmp.fails)))
        else:
            ctx.reset()
            # the root shell of THIS instance, before the viewer tab opens: ctx.root_pid is the instance's before the relaunch (run.py records it
            # per launch it makes). None (no probe answer) = "not recorded": the close then still needs a shell to answer.
            croot = ctx.shell_pid() if cases.depth_probe_cmd(getattr(ctx, "pane", None), "x") is not None else None
            clog, coff = _open_and_read(ctx, r, path, "md_viewer_first_frame control")
            try:
                if clog is not None:
                    cok, ctext = control_verdict(clog)
                    r.metric("control_suspicious_image_paints", len(suspicious_lines(clog)))
                    r.check(cok, ctext)
                    if cok:
                        r.note(ctext)
            finally:
                cases.close_file_viewer(ctx, r, coff, "md_viewer_first_frame control", root_pid=croot)
    finally:
        tmp = cases.Result(r.name, r.mode)
        ctx.app.extra_env = {}
        _n, back = R.start_app(ctx, tmp, "md_viewer_first_frame: relaunch with the default environment")
        why_back = "; ".join(tmp.fails)
        ctx.reset()
    r.check(back, "the app reached no prompt after the final relaunch with the default environment: %s" % (why_back if not back else ""))
