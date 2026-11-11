# appci/cases_r15_w5c.py: round 15 wave 5c (docs/design/r15-w5c.md; r14-test-gaps T48). Registered with the same @case decorator as cases.py
# (one import line at its end). The other three items of the wave live where their cases already were: the privilege boundary in
# priv_boundary.py + cases_coverage_a.py / cases_subshell_ready.py (T47), the soak in procsample.py + cases_robust.py (T49), and
# agent_code_recycle in cases_caseagent.py + fake_model.py (#2071).
#
#   gpu_real_render   Linux. Every committed run renders through software somewhere (llvmpipe under Xvfb; under headless sway nobody ever read which
#                     adapter the app took), so no run has said "the app drew on the GPU and the picture was right". The case launches the app twice
#                     on the same profile: once as the machine gives it (the render node, where there is one) and once forced onto Mesa's software
#                     rasteriser, draws the same screen in both, and judges
#                       1. which adapter each launch used, from the app's own `Surface configured: backend= adapter="" ... software_renderer=` line
#                          (crates/warpui/src/rendering/wgpu/resources.rs surface_config_summary; the wording is pinned by a unit test there);
#                       2. the GPU launch's frame times, from `hop=frame_drawn total_ms= acquire_ms= present_ms=`: the app's own share of a frame
#                          (total less the wait for a swapchain image and less the present call) against the owner's UI budget;
#                       3. the two screenshots, by changed-pixel fraction, with a control pair (an empty pane against the drawn one) that MUST differ.
#                     No render node, or one the CI user cannot open: SKIP with NEEDS: and the machine change. A render node but no hardware adapter
#                     the session can present to (Xvfb: no DRI3): SKIP with NEEDS: naming the session. A presentable hardware adapter listed and a
#                     software one used: FAIL. The spec's third oracle, "a lost device recovers", is its own case since round 17 (#2091):
#   gpu_lost_device_recovers   Linux, whatever adapter the launch draws on (the render node where there is one, the software rasteriser under
#                     Xvfb: the rebuild is the app's own code either way, and the adapter is named on the row). The app is relaunched with
#                     PHOSPHOR_TEST_GPU_LOSE_DEVICE_FILE=<work dir>/phosphor-test-lose-gpu-device (crates/warpui .. event_loop/draw_retry.rs
#                     TestDeviceLoss: the redraw that finds and removes that file reports the device as lost instead of drawing, once per file;
#                     the handling after that is the app's real lost-device path: DrawRetryState decides, the renderer is dropped and rebuilt).
#                     The case draws the pattern, takes the picture, creates the file and judges from the app log and a second picture:
#                       1. the hook's line, then `Recreating the renderer`, then a new `Surface configured:` line (the rebuilt renderer);
#                       2. `hop=frame_drawn` lines after the rebuild (the window draws again) and the app alive, with no panic;
#                       3. the same screen within the picture tolerance, with the control pair (cleared pane against the pattern) that must differ.
#                     How many frames were drawn after the rebuild BEFORE the next input is a metric and a note: the lost-device decision asks
#                     for no retry (retry_after None), so the frame that hit the loss waits for the next event (#2101).
import glob, os, re, statistics, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402  (cases.py imports this module near its end, so everything below is already defined)
import cases_caseagent as CA  # noqa: E402
import cases_robust as R  # noqa: E402
import cases_ui as UI  # noqa: E402
from cases import case  # noqa: E402

# ================================================================ pure parts (selftest_r15_w5c.py)
SURFACE_RE = re.compile(r'Surface configured: backend=(\w+) adapter="(.*?)" present_mode=(.*?) frame_latency=(\d+) alpha_mode=(\w+) software_renderer=(true|false)')
# select_adapter logs every adapter it considered, one block each (resources.rs is_supported_adapter): "<type>: <name>\n\tBackend: ..\n\tDriver: ..\n\tCan present: .."
LISTED_HEAD_RE = re.compile(r"\b(IntegratedGpu|DiscreteGpu|VirtualGpu|Cpu|Other): (.+?)\s*$")
FRAME_RE = re.compile(r"hop=frame_drawn total_ms=(\d+(?:\.\d+)?) acquire_ms=(\d+(?:\.\d+)?)(?: present_ms=(\d+(?:\.\d+)?))?")
SOFTWARE_NAMES = ("llvmpipe", "lavapipe", "softpipe", "swiftshader", "swrast")
# Mesa's software rasteriser through the GL backend: WGPU_BACKEND is read by the app (wgpu::Backends::from_env in rendering/wgpu/mod.rs), LIBGL_ALWAYS_SOFTWARE
# by Mesa. No Vulkan driver is asked for, so lavapipe need not be installed.
SOFTWARE_ENV = {"WGPU_BACKEND": "gl", "LIBGL_ALWAYS_SOFTWARE": "1"}
FRAME_P50_MS, FRAME_P95_MS = 4.0, 16.0     # owner's UI budget: 4 ms of the app's own work per frame; one 60 Hz refresh as the tail
SHOT_DIFF_MAX = 0.02                       # fraction of pixels that may differ between the GPU and the software picture of the same screen
MIN_FRAMES = 30
PATTERN_ROWS = 24
# The third oracle of T48, "a lost device recovers", is the case gpu_lost_device_recovers below (#2091, round 17). This text is on every row of
# gpu_real_render so that a reader of that row knows where the oracle went.
LOST_DEVICE_ELSEWHERE = ("'a lost device recovers' (the third oracle of r14-test-gaps T48; #2091) is judged by its own case, gpu_lost_device_recovers "
                         "(a renderer test hook reports the loss; the kernel's reset switches under /sys/kernel/debug/dri need root)")
LOSS_ENV = "PHOSPHOR_TEST_GPU_LOSE_DEVICE_FILE"          # crates/warpui/src/windowing/winit/event_loop/draw_retry.rs TEST_DEVICE_LOSS_ENV
LOSS_FILE = "phosphor-test-lose-gpu-device"              # .. TEST_DEVICE_LOSS_FILE_NAME: the variable is ignored unless the path ends in this name
LOSS_ARMED = "TEST HOOK armed (%s)" % LOSS_ENV           # logged once when the event loop is built with the variable set
LOSS_FIRED = "TEST HOOK (%s): reporting the GPU device as lost for this frame" % LOSS_ENV
RECREATING = "Recreating the renderer in an attempt to recover"     # event_loop/mod.rs redraw_window, the real handling of any lost device


def surfaces(text):
    """pure: every surface the app configured in `text`, in order: [{backend, adapter, present_mode, software}]"""
    return [dict(backend=m.group(1), adapter=m.group(2), present_mode=m.group(3), software=m.group(6) == "true") for m in SURFACE_RE.finditer(text or "")]


def is_software(surface):
    """pure: the app's own flag (device type Cpu), or a software rasteriser's name on an adapter that reports another type"""
    return bool(surface["software"] or any(n in surface["adapter"].lower() for n in SOFTWARE_NAMES))


def listed_adapters(text):
    """pure: the adapters the app listed while choosing: [{type, name, backend, can_present}] (backend / can_present None when the block is cut short)"""
    out, cur = [], None
    for ln in (text or "").splitlines():
        m = LISTED_HEAD_RE.search(ln)
        if m and "Surface configured" not in ln:
            cur = dict(type=m.group(1), name=m.group(2), backend=None, can_present=None)
            out.append(cur)
            continue
        s = ln.strip()
        if cur is not None and s.startswith("Backend:"):
            cur["backend"] = s.split(":", 1)[1].strip()
        elif cur is not None and s.startswith("Can present:"):
            cur["can_present"] = s.split(":", 1)[1].strip() == "true"
        elif cur is not None and not ln.startswith(("\t", " ")):
            cur = None
    return out


def presentable_hardware(adapters):
    """pure: the listed adapters that are real hardware AND can present to this session's surface"""
    return [a for a in adapters if a["type"] in ("IntegratedGpu", "DiscreteGpu") and a["can_present"] and not any(n in a["name"].lower() for n in SOFTWARE_NAMES)]


def frame_stats(text):
    """pure: the frames drawn in `text` -> {n, work_p50, work_p95, total_p50, acquire_p50, present_p50} in ms, or {n: 0}. work = total - acquire -
    present: the scene build, the encode and the submit, i.e. what the app itself spent, without the wait for a swapchain image (the compositor's
    pace) and without the present call (the driver's)."""
    rows = [(float(m.group(1)), float(m.group(2)), float(m.group(3) or 0)) for m in FRAME_RE.finditer(text or "")]
    if not rows:
        return dict(n=0)
    work = sorted(max(0.0, t - a - p) for t, a, p in rows)
    med = lambda xs: round(statistics.median(xs), 2)       # noqa: E731
    return dict(n=len(rows), work_p50=med(work), work_p95=round(work[min(len(work) - 1, int(len(work) * 0.95))], 2),
                total_p50=med([t for t, _, _ in rows]), acquire_p50=med([a for _, a, _ in rows]), present_p50=med([p for _, _, p in rows]))


def frame_verdict(st, p50_max=FRAME_P50_MS, p95_max=FRAME_P95_MS, min_frames=MIN_FRAMES):
    """pure: ('pass' | 'fail' | 'few', text) for the GPU launch's frames"""
    if st.get("n", 0) < min_frames:
        return "few", "only %d hop=frame_drawn line(s) during the workload (need %d): the frame times are not judged" % (st.get("n", 0), min_frames)
    text = "the app's own share of a frame: p50 %.2f ms (budget %g), p95 %.2f ms (budget %g) over %d frames; whole redraw p50 %.2f, swapchain wait p50 %.2f, present p50 %.2f" % (
        st["work_p50"], p50_max, st["work_p95"], p95_max, st["n"], st["total_p50"], st["acquire_p50"], st["present_p50"])
    return ("fail" if st["work_p50"] > p50_max or st["work_p95"] > p95_max else "pass"), text


def loss_timeline(text):
    """pure: where the hook and the recovery stand in an app-log excerpt -> dict(fired, fired_count, recreated, surface_after, frames_after).
    `fired`: the hook's line; `recreated`: `Recreating the renderer` AFTER it; `surface_after`: the last surface configured after that (the
    rebuilt renderer's adapter) or None; `frames_after`: hop=frame_drawn lines after the rebuild."""
    text = text or ""
    i = text.find(LOSS_FIRED)
    j = text.find(RECREATING, i) if i >= 0 else -1
    tail = text[j:] if j >= 0 else ""
    after = surfaces(tail)
    return dict(fired=i >= 0, fired_count=text.count(LOSS_FIRED), recreated=j >= 0, surface_after=after[-1] if after else None,
                frames_after=frame_stats(tail).get("n", 0))


def loss_verdict(tl, alive, panicked, trigger_left):
    """pure: (ok, text) for the recovery, from loss_timeline() and three facts: the app is alive, its log has a panic, the trigger file still
    exists. ok None: the hook never fired, so nothing is judged."""
    if not tl["fired"]:
        return None, "the hook never reported the loss (no `%s` line): no redraw ran while the trigger file existed, or the build has no hook" % LOSS_FIRED
    if panicked:
        return False, "the app log has a panic after the device was reported lost"
    if not alive:
        return False, "the app is gone after the device was reported lost (the crash-recovery exit needs three failures in a row; one was reported)"
    if not tl["recreated"]:
        return False, "the device was reported lost and the app did not log `%s`: the renderer was not rebuilt" % RECREATING
    if tl["surface_after"] is None:
        return False, "`%s` was logged but no `Surface configured:` line follows it: the rebuilt renderer configured no surface" % RECREATING
    if tl["frames_after"] < 1:
        return False, "the renderer was rebuilt (%s) but no hop=frame_drawn line follows: the window never drew again" % tl["surface_after"]["adapter"]
    if tl["fired_count"] != 1 or trigger_left:
        return False, "one loss was requested and the hook reported %d (trigger file still there: %s): it is not once per request" % (tl["fired_count"], trigger_left)
    return True, "device reported lost once; renderer rebuilt on %s (%s); %d frame(s) drawn after the rebuild" % (
        tl["surface_after"]["adapter"], tl["surface_after"]["backend"], tl["frames_after"])


def node_state(nodes, access=os.access):
    """pure given access: ('none' | 'denied' | 'ok', usable nodes) for the DRM render nodes of this machine"""
    if not nodes:
        return "none", []
    ok = [n for n in nodes if access(n, os.R_OK | os.W_OK)]
    return ("ok" if ok else "denied"), ok


def pattern_script(tok, rows=PATTERN_ROWS):
    """pure: a sh script that draws the same `rows` lines every time it runs: text across the pane in the 8 ANSI colours and bold, so a renderer that
    drops colour, glyphs or a row changes thousands of pixels"""
    out = ["#!/bin/sh"]
    for i in range(1, rows + 1):
        out.append("printf '\\033[%s3%dm%s\\033[0m\\n' 'GPU%s row %02d the quick brown fox jumps over the lazy dog 0123456789 []{}<>|/\\\\ @#$%%'" % (
            "1;" if i % 2 else "", i % 8, "%s", tok, i))
    return "\n".join(out) + "\n"


# ================================================================ the case
def _relaunch(ctx, r, label, env):
    """relaunch on the same profile with `env` added; -> (up, why). start_app's own failure text is returned, not recorded: the caller decides what a
    launch that did not come up means for that arm."""
    tmp = cases.Result(r.name, r.mode)
    ctx.app.extra_env = dict(env)
    _n, up = R.start_app(ctx, tmp, label)
    return up, "; ".join(tmp.fails)


def _arm(ctx, r, arm, script, env):
    """one launch: relaunch, read the adapter, draw the workload, take the still picture. -> dict(up, why, surface, listed, frames, blank, shot)"""
    up, why = _relaunch(ctx, r, "gpu_real_render %s" % arm, env)
    res = dict(up=up, why=why, surface=None, listed=[], frames=dict(n=0), blank=None, shot=None)
    if not up:
        return res
    ctx.reset()
    launch_log = "\n".join(ctx.app_log_lines())           # this launch's log (the relaunch starts a new file)
    s = surfaces(launch_log)
    res["surface"] = s[-1] if s else None
    res["listed"] = listed_adapters(launch_log)
    ctx.be.guard()
    ctx.be.type("clear", 12, enter=True)
    time.sleep(1.0)
    CA.settled(ctx, "gpu_%s_blank" % arm, quiet=3, step=0.7, timeout=20)
    res["blank"] = ctx.shot("gpu_%s_blank" % arm)
    off = ctx.log_size()
    ctx.be.type("sh %s" % script, ctx.rng.randint(10, 25), enter=True)      # key frames, then 24 coloured rows
    time.sleep(1.0)
    ctx.be.type("seq 1 4000", ctx.rng.randint(10, 25), enter=True)          # scrolling output: many frames in a row
    time.sleep(4.0)
    ctx.be.type("clear; sh %s" % script, ctx.rng.randint(10, 25), enter=True)
    time.sleep(1.0)
    CA.settled(ctx, "gpu_%s_still" % arm, quiet=3, step=0.7, timeout=30)
    res["frames"] = frame_stats(ctx.log_since(off))
    res["shot"] = ctx.shot("gpu_%s_pattern" % arm)
    ctx.reset()
    return res


@case("gpu_real_render", order=88, gui=True)
def c_gpu_real_render(ctx, r):
    """T48 (see the header): the app on the machine's render node against a forced software launch: the adapter each launch used (app log), the GPU
    launch's frame times against the UI budget, and the two pictures of the same screen within APPCI_GPU_SHOT_DIFF (2 % of the pixels), with a
    control pair that must differ. Budgets: APPCI_GPU_FRAME_P50_MS (4), APPCI_GPU_FRAME_P95_MS (16). Linux only."""
    if ctx.win or ctx.be.name == "mac" or not cases.plat().startswith("linux"):
        r.skip("Linux only by design (T48): the comparison is the DRM render node against Mesa's software rasteriser; macOS draws on Metal on the machine's "
               "GPU in every run and has no software arm, the Windows VM has no GPU (this platform: %s)" % cases.plat())
    if ctx.app is None or not hasattr(ctx.app, "extra_env") or not hasattr(ctx.app, "proc"):
        r.skip("needs the Linux app launcher (a relaunch with an extra environment), not the sim backend")
    state, nodes = node_state(sorted(glob.glob("/dev/dri/renderD*")))
    if state == "none":
        r.skip("no GPU render node on this machine (/dev/dri/renderD* does not exist). NEEDS: a test machine with a GPU whose kernel driver exposes a render "
               "node (the box has /dev/dri/renderD128, Intel UHD 630), or a GPU passed through to this runner. " + LOST_DEVICE_ELSEWHERE)
    if state == "denied":
        r.skip("the render node(s) %s exist but the CI user cannot open them. NEEDS: the CI user in the `render` and `video` groups "
               "(`sudo usermod -aG render,video <ci-user>`, then restart the runner service). %s" % (sorted(glob.glob("/dev/dri/renderD*")), LOST_DEVICE_ELSEWHERE))
    UI.need_pillow(r)
    p50_max = float(os.environ.get("APPCI_GPU_FRAME_P50_MS", FRAME_P50_MS))
    p95_max = float(os.environ.get("APPCI_GPU_FRAME_P95_MS", FRAME_P95_MS))
    diff_max = float(os.environ.get("APPCI_GPU_SHOT_DIFF", SHOT_DIFF_MAX))
    script = "gpu_pattern.sh"
    with open(os.path.join(ctx.W, script), "w") as f:
        f.write(pattern_script("".join(ctx.rng.choice("ABCDEFHKMNPRTUVWXY") for _ in range(4))))
    r.metric("render_nodes", nodes)
    gpu = soft = None
    back = False
    try:
        gpu = _arm(ctx, r, "gpu", script, {})
        if gpu["up"] and gpu["surface"] and not is_software(gpu["surface"]):
            soft = _arm(ctx, r, "software", script, SOFTWARE_ENV)
    finally:
        back, why_back = _relaunch(ctx, r, "gpu_real_render: relaunch with the default environment", {})
        ctx.reset()
    r.check(back, "the app reached no prompt after the final relaunch with the default environment: %s" % (why_back if not back else ""))
    r.note(LOST_DEVICE_ELSEWHERE)
    if not r.check(gpu["up"], "the default launch reached no prompt: %s" % gpu["why"]):
        return
    r.metric("gpu_surface", gpu["surface"])
    r.metric("gpu_listed_adapters", gpu["listed"][:8])
    if gpu["surface"] is None:
        r.unverifiable("the app log of the default launch has no `Surface configured:` line (a build before that line, or the log level hides it): which adapter "
                       "drew is unknown, so nothing is judged")
        return
    if is_software(gpu["surface"]):
        hw = presentable_hardware(gpu["listed"])
        if not r.check(not hw, "the machine has render node(s) %s and the app listed hardware it can present to (%s), yet it drew on the software adapter %r "
                               "(backend %s)" % (nodes, ["%s via %s" % (a["name"], a["backend"]) for a in hw], gpu["surface"]["adapter"], gpu["surface"]["backend"])):
            return
        r.skip("the render node exists but this session offers the app no hardware adapter it can present to (it listed %s and drew on %r). NEEDS: the app test "
               "in a session the GPU can present to: the headless-sway job with WLR_RENDERER=gles2 on the render node (Xvfb has no DRI3), and Mesa's "
               "driver for the GPU (packages mesa-vulkan-drivers, libgl1-mesa-dri). %s" % (
                   [(a["type"], a["name"], a["backend"], a["can_present"]) for a in gpu["listed"][:6]] or "no adapter", gpu["surface"]["adapter"], LOST_DEVICE_ELSEWHERE))
    # --- the GPU launch's frame times
    r.metric("gpu_frames", gpu["frames"])
    v, text = frame_verdict(gpu["frames"], p50_max, p95_max)
    if v == "few":
        r.unverifiable("GPU launch (%s): %s" % (gpu["surface"]["adapter"], text))
    else:
        r.check(v == "pass", "GPU launch (%s, %s): %s" % (gpu["surface"]["adapter"], gpu["surface"]["backend"], text))
        r.note("GPU launch (%s, %s, present %s): %s" % (gpu["surface"]["adapter"], gpu["surface"]["backend"], gpu["surface"]["present_mode"], text))
    # --- the software launch: the reference picture
    if soft is None or not soft["up"]:
        r.unverifiable("the forced software launch (%s) reached no prompt (%s): the GPU picture has no reference" % (SOFTWARE_ENV, soft["why"] if soft else "not started"))
        return
    r.metric("software_surface", soft["surface"])
    r.metric("software_frames", soft["frames"])
    if soft["surface"] is None or not is_software(soft["surface"]):
        r.unverifiable("the launch forced onto software (%s) drew on %r: Mesa's software rasteriser was not taken (no llvmpipe on this machine, or the "
                       "variables did not reach the app): the GPU picture has no reference" % (SOFTWARE_ENV, soft["surface"]["adapter"] if soft["surface"] else None))
        return
    if soft["frames"].get("n") and gpu["frames"].get("n"):
        r.note("software launch (%s): own share of a frame p50 %.2f ms, p95 %.2f ms over %d frames (metric only: software is expected to be slower)" % (
            soft["surface"]["adapter"], soft["frames"]["work_p50"], soft["frames"]["work_p95"], soft["frames"]["n"]))
    # --- the pictures
    if not (gpu["shot"] and soft["shot"] and gpu["blank"]):
        r.unverifiable("a screenshot is missing (gpu %s, software %s, blank %s): the pictures are not compared" % (bool(gpu["shot"]), bool(soft["shot"]), bool(gpu["blank"])))
        return
    control = UI.img_diff(gpu["blank"], gpu["shot"])
    d = UI.img_diff(gpu["shot"], soft["shot"])
    r.metric("shot_diff_gpu_vs_software", d)
    r.metric("shot_diff_control_blank_vs_pattern", control)
    if control is None or d is None or control < 5 * diff_max:
        r.unverifiable("the control pair (the cleared pane against the drawn pattern, same launch) differs by %s, under 5x the tolerance %g: the comparison could "
                       "not see the pattern, so equal pictures would prove nothing" % (control, diff_max))
        return
    r.check(d <= diff_max, "the GPU picture (%s) and the software picture (%s) of the same screen differ in %.2f %% of the pixels (tolerance %.2f %%; the cleared "
                           "pane against the pattern differs in %.2f %%): see shots gpu_gpu_pattern / gpu_software_pattern" % (
                               gpu["surface"]["adapter"], soft["surface"]["adapter"], d * 100, diff_max * 100, control * 100))


@case("gpu_lost_device_recovers", order=89, gui=True)
def c_gpu_lost_device_recovers(ctx, r):
    """T48's third oracle (#2091; see the header): the app is told once, through its renderer test hook, that the GPU device is lost; it must rebuild
    the renderer, draw again and show the same screen. Linux, on whatever adapter the launch uses (named on the row). Tolerance:
    APPCI_GPU_SHOT_DIFF (2 % of the pixels)."""
    if ctx.win or ctx.be.name == "mac" or not cases.plat().startswith("linux"):
        r.skip("Linux only by design (#2091): the hook is in the winit event loop and the case needs a launcher that can relaunch the app with an extra "
               "environment; macOS draws through its own loop (no hook there), the Windows launcher has no per-launch environment (this platform: %s)" % cases.plat())
    if ctx.app is None or not hasattr(ctx.app, "extra_env") or not hasattr(ctx.app, "proc"):
        r.skip("needs the Linux app launcher (a relaunch with an extra environment), not the sim backend")
    UI.need_pillow(r)
    diff_max = float(os.environ.get("APPCI_GPU_SHOT_DIFF", SHOT_DIFF_MAX))
    script = "gpu_loss_pattern.sh"
    with open(os.path.join(ctx.W, script), "w") as f:
        f.write(pattern_script("".join(ctx.rng.choice("ABCDEFHKMNPRTUVWXY") for _ in range(4))))
    trigger = os.path.join(os.path.abspath(ctx.W), LOSS_FILE)
    if os.path.exists(trigger):
        os.remove(trigger)
    back, why_back = False, ""
    try:
        up, why = _relaunch(ctx, r, "gpu_lost_device_recovers", {LOSS_ENV: trigger})
        if not r.check(up, "the launch with %s reached no prompt: %s" % (LOSS_ENV, why)):
            return
        ctx.reset()
        launch_log = "\n".join(ctx.app_log_lines())
        s0 = surfaces(launch_log)
        r.metric("surface_before", s0[-1] if s0 else None)
        if LOSS_ARMED not in launch_log:
            r.unverifiable("the app log of the launch with %s has no `%s` line: this build has no lost-device test hook (before #2091), or the log level "
                           "hides it; nothing is judged" % (LOSS_ENV, LOSS_ARMED))
            return
        # the screen: a cleared pane (control picture), then the pattern (the picture the recovery must give back)
        ctx.be.guard()
        ctx.be.type("clear", 12, enter=True)
        time.sleep(1.0)
        CA.settled(ctx, "gpu_loss_blank", quiet=3, step=0.7, timeout=20)
        blank = ctx.shot("gpu_loss_blank")
        ctx.be.type("clear; sh %s" % script, ctx.rng.randint(10, 25), enter=True)
        time.sleep(1.0)
        CA.settled(ctx, "gpu_loss_before", quiet=3, step=0.7, timeout=30)
        before = ctx.shot("gpu_loss_before")
        # the loss: the next redraw takes the file. An idle window may not redraw by itself, so one key is typed if none came within 1.5 s.
        off = ctx.log_size()
        with open(trigger, "w") as f:
            f.write("")
        typed = 0
        fired = ctx.until(lambda: LOSS_FIRED in ctx.log_since(off), 1.5, step=0.1)
        if not fired:
            ctx.be.guard()
            ctx.be.type("x", 12)
            typed += 1
            fired = ctx.until(lambda: LOSS_FIRED in ctx.log_since(off), 5, step=0.1)
        r.metric("loss_needed_a_key", bool(typed))
        # what the app does by itself after the rebuild, before any further input (#2101: the decision asks for no retry)
        ctx.until(lambda: RECREATING in ctx.log_since(off), 5, step=0.1)
        time.sleep(2.0)
        quiet = loss_timeline(ctx.log_since(off))
        r.metric("frames_after_rebuild_before_input", quiet["frames_after"])
        # the next input: one key and its removal, so the screen ends as it was
        ctx.be.guard()
        ctx.be.type("y", 12)
        typed += 1
        time.sleep(0.5)
        ctx.be.key("BackSpace", repeat=typed, delay_ms=40)
        time.sleep(1.0)
        CA.settled(ctx, "gpu_loss_after", quiet=3, step=0.7, timeout=30)
        after = ctx.shot("gpu_loss_after")
        log = ctx.log_since(off)
        tl = loss_timeline(log)
        r.metric("loss_timeline", tl)
        ok, text = loss_verdict(tl, ctx.app.alive(), "panicked at" in log, os.path.exists(trigger))
        if ok is None:
            r.unverifiable(text)
            return
        r.check(ok, text)
        if not ok:
            return
        r.note(text)
        if quiet["fired"] and quiet["recreated"]:
            r.note("%d frame(s) were drawn after the rebuild before the next input%s" % (
                quiet["frames_after"], "" if quiet["frames_after"] else ": the frame that hit the loss waited for the next event (#2101; not a verdict here)"))
        # the picture
        if not (blank and before and after):
            r.unverifiable("a screenshot is missing (blank %s, before %s, after %s): the pictures are not compared" % (bool(blank), bool(before), bool(after)))
            return
        control = UI.img_diff(blank, before)
        d = UI.img_diff(before, after)
        r.metric("shot_diff_before_vs_after_loss", d)
        r.metric("shot_diff_control_blank_vs_pattern", control)
        if control is None or d is None or control < 5 * diff_max:
            r.unverifiable("the control pair (the cleared pane against the drawn pattern) differs by %s, under 5x the tolerance %g: the comparison could not see "
                           "the pattern, so equal pictures would prove nothing" % (control, diff_max))
            return
        r.check(d <= diff_max, "the screen after the rebuilt renderer drew differs from the screen before the loss in %.2f %% of the pixels (tolerance %.2f %%; "
                               "the cleared pane against the pattern differs in %.2f %%): see shots gpu_loss_before / gpu_loss_after" % (d * 100, diff_max * 100, control * 100))
    finally:
        if os.path.exists(trigger):
            os.remove(trigger)
        back, why_back = _relaunch(ctx, r, "gpu_lost_device_recovers: relaunch with the default environment", {})
        ctx.reset()
        r.check(back, "the app reached no prompt after the final relaunch with the default environment: %s" % (why_back if not back else ""))

