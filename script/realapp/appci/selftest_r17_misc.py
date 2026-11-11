# appci/selftest_r17_misc.py: offline self-tests of round 17 misc (docs/design/r17-misc.md): md_viewer_first_frame (#2028, cases_r17_misc.py) and
# gpu_lost_device_recovers (#2091, cases_r15_w5c.py) with the app strings they read. Pure Python: no app, no display.
# selftest.py appends CHECKS to its own list (two lines at its end), so `run.py --self-test` runs them.
# Every judge is fed a right input AND the wrong ones it exists to catch: an oracle that cannot fail is not a test.
import os, struct, sys, zlib

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.join(HERE, "..", "..", "..")
sys.path.insert(0, HERE)

CHECKS = []


def check(f):
    CHECKS.append(f)
    return f


def _src(*parts):
    return open(os.path.join(REPO, *parts), encoding="utf-8").read()


@check
def r17_cases_registered_proving_and_classified():
    """both cases are registered, PROVING (never SMOKE, never PROVING_FAKE), declared in the settings model and classified for Windows; docs/testing.md
    names them, their switches and this file"""
    import cases, case_settings, winmap
    reg = {c["name"]: c for c in cases.CASES}
    for n in ("md_viewer_first_frame", "gpu_lost_device_recovers"):
        assert n in reg and reg[n]["gui"] and n in cases.PROVING and n not in cases.SMOKE and n not in cases.PROVING_FAKE, n
        assert case_settings.declared(n), n
    assert len(set(cases.PROVING)) == len(cases.PROVING), "PROVING lists a case twice"
    assert winmap.skip_reason("md_viewer_first_frame", "powershell") is None and winmap.skip_reason("md_viewer_first_frame", "gitbash") is None
    why = winmap.skip_reason("gpu_lost_device_recovers", "gitbash")
    assert why and "by design" in why and "PHOSPHOR_TEST_GPU_LOSE_DEVICE_FILE" in why, why
    doc = _src("docs", "testing.md")
    sec = doc[doc.index("## Round 17 misc"):]
    for word in ("`md_viewer_first_frame`", "`gpu_lost_device_recovers`", "PHOSPHOR_MD_FIRST_LAYOUT_HOLD", "PHOSPHOR_TEST_GPU_LOSE_DEVICE_FILE",
                 "PHOSPHOR_BASH_HOOK_HEX_CACHE", "selftest_r17_misc.py", "#2101"):
        assert word in sec, "docs/testing.md (Round 17 misc) does not mention %s" % word
    table = doc[doc.index("### 2.6 Environment switches"):doc.index("### 2.7")]
    for word in ("PHOSPHOR_MD_FIRST_LAYOUT_HOLD=0", "PHOSPHOR_TEST_GPU_LOSE_DEVICE_FILE", "PHOSPHOR_BASH_HOOK_HEX_CACHE=0"):
        assert word in table, "docs/testing.md 2.6 does not list %s" % word


@check
def r17_md_first_frame_judges():
    """the first-frame verdict passes the fixed build's log and fails each thing it exists to catch (a block painted without a width, no hold line, a
    release at width 0); the control verdict fails when the kill switch does not switch the hold off; the strings are the app's"""
    import cases_r17_misc as M
    good = ("t [INFO] dispatching action for \"root_view:add_file_pane\"\n"
            "t [INFO] rich text: first size 1183.5x702 released 1 held layout edit(s)\n")
    ok, text = M.first_frame_verdict(good)
    assert ok and "1183.5x702" in text and M.first_sizes(good) == [(1183.5, 702.0, 1)], text
    late = "t [INFO] rich text: first size 900x600 released 0 held layout edit(s)\n"
    assert M.first_frame_verdict(late)[0], "a file that arrives after the first frame is laid out at the width the model already has: not a failure"
    warn = ("t [WARN] image paint with suspicious size source=Async { id: AsyncAssetId { .. } } element_size=(-20, 208.00002) bounds=(-20, 208) "
            "fit_type=Contain cache_option=BySize\n")
    ok, text = M.first_frame_verdict(good + warn)
    assert not ok and "painted with no width" in text and "element_size=(-20" in text, "the issue's own log line must fail the case even with the hold line present: %s" % text
    ok, text = M.first_frame_verdict("t [INFO] dispatching action for \"root_view:add_file_pane\"\n")
    assert not ok and "no `rich text: first size` line" in text and M.HOLD_SWITCH in text, "a build without the hold (or with it switched off) fails: %s" % text
    ok, text = M.first_frame_verdict("t rich text: first size 0x0 released 3 held layout edit(s)\n")
    assert not ok and "width of 0" in text, text
    assert M.first_frame_verdict("")[0] is False and M.first_frame_verdict(None)[0] is False
    assert len(M.suspicious_lines(warn * 3)) == 3 and M.suspicious_lines(good) == []
    # control
    assert M.control_verdict("t [INFO] something else\n" + warn)[0], "with the switch off the old warning may show: a metric, not a failure"
    ok, text = M.control_verdict(good)
    assert not ok and "kill switch does not" in text, text
    # the strings are the app's own
    model = _src("crates", "editor", "src", "render", "model", "mod.rs")
    assert '"rich text: first size {}x{} released {} held layout edit(s)"' in model, "release_first_size_hold's log line changed: update FIRST_SIZE_RE"
    assert "fn hold_layout_until_first_size" in model and "fn release_first_size_hold" in model
    assert M.SUSPICIOUS in _src("crates", "warpui_core", "src", "elements", "image.rs"), "the image element's warning changed: update SUSPICIOUS"
    viewer = _src("app", "src", "notebooks", "file", "mod.rs")
    assert '"%s"' % M.HOLD_SWITCH in viewer and 'value != Some("0")' in viewer, "the viewer's kill switch changed: update HOLD_SWITCH"
    assert "model.hold_layout_until_first_size(ctx)" in viewer, "the markdown viewer no longer holds its layout for the first size"
    src = _src("script", "realapp", "appci", "cases_r17_misc.py")
    body = src[src.index("def c_md_viewer_first_frame"):]
    assert body.index("first_frame_verdict(log)") < body.index("close_file_viewer(ctx, r, off") < body.index("extra_env = {HOLD_SWITCH: \"0\"}") < body.index("extra_env = {}")
    assert body.count("close_file_viewer(") == 2, "each arm closes the viewer tab it opened (a tab left in front takes the next case's keys)"


@check
def r17_md_first_frame_fixture():
    """the fixture: a valid PNG made without Pillow (signature, IHDR size, CRCs, the pixel data inflates to the right length) and a markdown file with
    the heading, the image block and a paragraph that wraps"""
    import cases_r17_misc as M
    png = M.png_bytes(160, 80, (40, 120, 200))
    assert png[:8] == b"\x89PNG\r\n\x1a\n"
    pos, kinds, idat = 8, [], b""
    while pos < len(png):
        n = struct.unpack(">I", png[pos:pos + 4])[0]
        kind, data, crc = png[pos + 4:pos + 8], png[pos + 8:pos + 8 + n], struct.unpack(">I", png[pos + 8 + n:pos + 12 + n])[0]
        assert crc == zlib.crc32(kind + data) & 0xFFFFFFFF, kind
        kinds.append(kind)
        if kind == b"IHDR":
            assert struct.unpack(">IIBBBBB", data) == (160, 80, 8, 2, 0, 0, 0)
        if kind == b"IDAT":
            idat += data
        pos += 12 + n
    assert kinds == [b"IHDR", b"IDAT", b"IEND"] and len(zlib.decompress(idat)) == 80 * (1 + 160 * 3)
    try:
        import io
        from PIL import Image
        im = Image.open(io.BytesIO(png))
        assert im.size == (160, 80) and im.convert("RGB").getpixel((5, 5)) == (40, 120, 200)
    except ImportError:
        pass
    md = M.fixture_markdown("ABCD")
    assert md.startswith("# FIRSTFRAMEABCD\n") and "![probe picture](ff_probe.png)" in md and md.count("word") == 120 and md != M.fixture_markdown("ABCE")


SURF = 'x [INFO] Surface configured: backend=%s adapter="%s" present_mode=Fifo frame_latency=2 alpha_mode=Opaque software_renderer=%s\n'
FRAME = "t input_order hop=frame_drawn total_ms=3.0 acquire_ms=0.5 present_ms=0.5 t=1000.0\n"


@check
def r17_gpu_lost_device_judges():
    """the recovery verdict passes the full sequence (hook line, rebuild, new surface, frames) and fails or refuses each broken one: the hook never
    fired (not judged), no rebuild, no surface after it, no frame after it, two losses for one request, the trigger left behind, a dead app, a panic;
    a rebuild or frames logged BEFORE the hook's line do not count"""
    import cases_r15_w5c as G
    fired = "x [WARN] %s\n" % G.LOSS_FIRED
    rec = "x [WARN] %s...\n" % G.RECREATING
    surf = SURF % ("Vulkan", "Intel(R) UHD Graphics 630 (CFL GT2)", "false")
    good = FRAME * 3 + fired + "x [WARN] Failed to render frame: Device was lost\n" + rec + surf + FRAME * 4
    tl = G.loss_timeline(good)
    assert tl["fired"] and tl["fired_count"] == 1 and tl["recreated"] and tl["frames_after"] == 4 and tl["surface_after"]["adapter"].startswith("Intel"), tl
    ok, text = G.loss_verdict(tl, True, False, False)
    assert ok is True and "4 frame(s)" in text and "Intel" in text, text
    soft = G.loss_timeline(fired + rec + SURF % ("Gl", "llvmpipe (LLVM 20.1.2, 256 bits)", "true") + FRAME)
    assert G.loss_verdict(soft, True, False, False)[0] is True, "a software adapter rebuilds the same way (the Xvfb job)"
    # not fired: nothing to judge, and what came before the hook does not count
    early = G.loss_timeline(rec + surf + FRAME * 5)
    assert not early["fired"] and not early["recreated"] and early["frames_after"] == 0, early
    ok, text = G.loss_verdict(early, True, False, True)
    assert ok is None and "never reported the loss" in text, text
    before_only = G.loss_timeline(rec + surf + FRAME * 5 + fired)
    assert before_only["fired"] and not before_only["recreated"], "a rebuild logged before the hook's line is not this loss's rebuild"
    assert G.loss_verdict(before_only, True, False, False)[0] is False
    # each broken recovery
    for log, alive, panicked, left, needle in (
            (fired + FRAME * 3, True, False, False, "renderer was not rebuilt"),
            (fired + rec + FRAME * 3, True, False, False, "configured no surface"),
            (fired + rec + surf, True, False, False, "never drew again"),
            (fired + rec + surf + FRAME + fired + rec + surf + FRAME, True, False, False, "not once per request"),
            (good, True, False, True, "not once per request"),
            (good, False, False, False, "app is gone"),
            (good, True, True, False, "panic")):
        ok, text = G.loss_verdict(G.loss_timeline(log), alive, panicked, left)
        assert ok is False and needle in text, (needle, text)
    assert G.loss_timeline(None) == G.loss_timeline("") and not G.loss_timeline("")["fired"]


@check
def r17_gpu_lost_device_hook_strings_and_case_order():
    """the names and lines the case reads are the ones the Rust hook defines and logs, the hook is taken before anything is drawn, and the case takes its
    reference picture before it requests the loss and removes the trigger file whatever happens"""
    import cases_r15_w5c as G
    dr = _src("crates", "warpui", "src", "windowing", "winit", "event_loop", "draw_retry.rs")
    assert 'TEST_DEVICE_LOSS_ENV: &str = "%s"' % G.LOSS_ENV in dr and 'TEST_DEVICE_LOSS_FILE_NAME: &str = "%s"' % G.LOSS_FILE in dr
    assert '"TEST HOOK armed ({TEST_DEVICE_LOSS_ENV})' in dr and G.LOSS_ARMED == "TEST HOOK armed (%s)" % G.LOSS_ENV
    assert "std::fs::remove_file(path).is_ok()" in dr and "path.is_absolute()" in dr, "the hook fires once per file and only for an absolute path"
    ev = _src("crates", "warpui", "src", "windowing", "winit", "event_loop", "mod.rs")
    assert '"TEST HOOK ({}): reporting the GPU device as lost for this frame"' in ev and G.LOSS_FIRED.startswith("TEST HOOK (%s): " % G.LOSS_ENV)
    assert '"%s..."' % G.RECREATING in ev, "the rebuild's log line changed: update RECREATING"
    redraw = ev[ev.index("let render_result = (|| {"):]
    assert redraw.index("self.test_device_loss.take()") < redraw.index("window.update_size_if_needed()?") < redraw.index("window.render(new_scene"), \
        "the hook must be asked before the frame is sized, built or drawn"
    assert "return Err(crate::rendering::wgpu::renderer::Error::DeviceLost);" in redraw
    tests = _src("crates", "warpui", "src", "windowing", "winit", "event_loop", "draw_retry_tests.rs")
    for name in ("test_device_loss_hook_is_off_unless_the_variable_names_its_own_file", "test_device_loss_hook_fires_once_per_trigger_file",
                 "the_error_the_test_hook_reports_rebuilds_the_renderer"):
        assert "fn %s()" % name in tests, name
    src = _src("script", "realapp", "appci", "cases_r15_w5c.py")
    body = src[src.index("def c_gpu_lost_device_recovers"):]
    order = [body.index(x) for x in ('{LOSS_ENV: trigger}', 'LOSS_ARMED not in launch_log', 'ctx.shot("gpu_loss_before")', 'with open(trigger, "w")',
                                     'frames_after_rebuild_before_input', 'ctx.be.key("BackSpace", repeat=typed', 'ctx.shot("gpu_loss_after")', "loss_verdict(tl,",
                                     "UI.img_diff(before, after)")]
    assert order == sorted(order), "arm, check the hook is there, reference picture, request the loss, quiet wait, input, second picture, verdict, pictures: %r" % order
    fin = body[body.index("    finally:"):]
    assert "os.remove(trigger)" in fin and "_relaunch(ctx, r" in fin and "{})" in fin, "the trigger file is removed and the app relaunched without the variable whatever happened"
    assert os.path.basename(os.path.join("/w", G.LOSS_FILE)) == "phosphor-test-lose-gpu-device"
