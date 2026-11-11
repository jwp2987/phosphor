# appci/cases_desk.py: round 15, wave 4 desktop integration (docs/design/r14-test-gaps.md 7.1, T35 / T37 / T38 / T39; design note
# docs/design/r15-w4desk.md; #2075, #2078). Five cases, all in cases.PROVING (owner rule 2026-11-09; no code moves a case out of it).
#   scale_matrix                         T35 (Wayland, X11). The display scale is 1.25, 1.5 and 2 (sway `output ... scale`; X resources Xft.dpi): at each
#                                        the pty's `stty size` is the app's own grid and equals, within a cell or two, the grid of the SAME logical window
#                                        size at scale 1 (the control measured just before); a double click on a word copies exactly that word (the
#                                        pointer is placed from the app's own draw position). Wayland: the window is then moved to a second headless output
#                                        with another scale and both hold there.
#   desktop_notification                 T37. Notifications enabled in the profile, long-running threshold 3 s. A 12 s command that ends while Phosphor
#                                        has the focus: none. The same command ending while the reference terminal has the focus: exactly one, naming the
#                                        command. A 2 s command ending unfocused: none. Linux: a recording org.freedesktop.Notifications service on the
#                                        launcher's private bus (notify_recorder.py) is the oracle and the app's hop=desktop_notification must agree;
#                                        Windows (and macOS once its permission prompt is answered): the hop alone.
#   desktop_notification_agent_question  T37, scripted fake model. A question card that arrives while the reference terminal has the focus: exactly one
#                                        notification; answering it with the focus back sends no second one.
#   drag_drop_file                       T38 (X11, then Wayland). A file whose name has a space is dragged from a real drag source (dragsrc.py, GTK; or
#                                        dragon-drop) onto the terminal after `ls -d ` was typed: the shell receives ONE argument equal to the path and
#                                        `ls` prints it. Wayland: the native backend's drop target (#2085, vendored winit, kill switch PHOSPHOR_WAYLAND_DND=0)
#                                        is judged like X11, and the source must also be told the drop succeeded.
#   ime_compose                          T39. Pinyin through a real input method (Linux: fcitx5 or ibus in the app's session; Windows: Microsoft Pinyin
#                                        selected for the Phosphor window, keys by SendInput virtual keys): nothing reaches a raw reader before the
#                                        commit, the committed text is exact there and in the input editor. macOS: intl_ime_japanese / intl_ime_pinyin.
# Same rules as cases.py: the exact received text, typing at 10-25 ms/key with instant Enter, a case that cannot run SKIPS with `NEEDS: <the exact
# machine change>`, a half that cannot be judged is UNVERIFIABLE (status noref), never a pass. Pure parts: selftest_desk.py.
import json, os, re, shlex, shutil, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases as C  # noqa: E402  (cases.py imports this module near its end, so everything below is already defined)
import cases_agent_question as AQ  # noqa: E402
import cases_agent_resilience as AR  # noqa: E402
import cases_caseagent as AG  # noqa: E402
import cases_caseintl as INTL  # noqa: E402
import cases_pointer as P  # noqa: E402
import cases_r14_state as S  # noqa: E402
import cases_robust as R  # noqa: E402
import cases_ui as U  # noqa: E402
import cases_window_use as WU  # noqa: E402
import cases_wrap_vtabs as WV  # noqa: E402
import dragsrc  # noqa: E402
import notify_recorder as NR  # noqa: E402
import toml_text as T  # noqa: E402
from cases import case  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
SESSION_PRE_ENV = "APPCI_SESSION_PRE"       # app.py INNER sources the file this names inside the app's private D-Bus session, before the app starts


# ================================================================= pure helpers (unit-tested in selftest_desk.py)
# ---- T35 scale
SCALES = (1.25, 1.5, 2.0)
GRID_FRAC, GRID_FLOOR = 0.08, 2      # a grid at scale s may differ from the scale-1 control by 8 % or two cells (cell sizes snap to device pixels)
WIN_SCALE_NEEDS = ("NEEDS: a display scale the harness can set on the Windows VM (the VM's basic display adapter offers no per-monitor DPI call; a "
                   "system DPI change needs a sign-out) and Windows pointer synthesis for the word click (mouse SendInput, r14-test-gaps T31)")


def fmt_scale(s):
    return "%g" % s


def scale_tag(s):
    """1.25 -> '125' (file and screenshot names)"""
    return "%d" % round(s * 100)


def logical_size(mode_w, mode_h, scale):
    """the logical size of a window that fills a mode_w x mode_h output at `scale`"""
    return int(mode_w / scale), int(mode_h / scale)


def xft_dpi(scale):
    """the Xft.dpi value of a display scale (96 dpi = 1)"""
    return int(round(96 * scale))


def last_grid(text):
    """(rows, cols) of the last hop=grid_resize line of `text`, or None"""
    hits = WU.GRID_HOP_RE.findall(text or "")
    return (int(hits[-1][0]), int(hits[-1][1])) if hits else None


def grid_scale_verdict(control, got, hop, scale, frac=GRID_FRAC, floor=GRID_FLOOR):
    """-> (problems, weak). control = `stty size` at scale 1 for the same logical window size, got = at `scale`, hop = the app's last grid (or None).
    A correct app keeps the grid (the logical size did not change); one that lays the grid out in device pixels has about control * scale, one that
    ignores the scale about control / scale. weak = the dimensions too small for that difference to exceed the tolerance (not judged)."""
    if control is None:
        return ["the shell did not answer `stty size` at scale 1 (the control)"], []
    if got is None:
        return ["the shell did not answer `stty size` at scale %s" % fmt_scale(scale)], []
    probs, weak = [], []
    if hop is not None and tuple(hop) != tuple(got):
        probs.append("the pty reports %d rows x %d columns but the app's last grid (hop=grid_resize) is %d x %d: the resize of the scale change did not "
                     "reach the pty" % (got[0], got[1], hop[0], hop[1]))
    for name, c, g in (("rows", control[0], got[0]), ("columns", control[1], got[1])):
        tol = max(floor, int(round(c * frac)))
        if min(abs(c * scale - c), abs(c / scale - c)) <= tol:
            weak.append(name)
            continue
        if abs(g - c) > tol:
            probs.append("%s: %d at scale %s, %d at scale 1 for the same logical window size (allowed +-%d; a grid laid out in device pixels would "
                         "have about %d, one that ignored the scale about %d)" % (name, g, fmt_scale(scale), c, tol, round(c * scale), round(c / scale)))
    return probs, weak


def sway_outputs(raw):
    """[{name, scale, rect (x, y, w, h), mode (w, h)}] of the active outputs in `swaymsg -t get_outputs -r` output (bytes or str); [] when unreadable"""
    try:
        data = json.loads(raw)
    except (TypeError, ValueError):
        return []
    out = []
    for o in data if isinstance(data, list) else []:
        if not o.get("active", True) or not o.get("rect"):
            continue
        rc, md = o["rect"], o.get("current_mode") or {}
        out.append(dict(name=o.get("name"), scale=float(o.get("scale") or 1.0), rect=(rc["x"], rc["y"], rc["width"], rc["height"]),
                        mode=(md.get("width") or rc["width"], md.get("height") or rc["height"])))
    return out


def layout_bbox(outputs):
    """(x, y, w, h) of the box around every output's layout rectangle, or None without outputs: the area a virtual pointer's absolute motion spans"""
    if not outputs:
        return None
    x0, y0 = min(o["rect"][0] for o in outputs), min(o["rect"][1] for o in outputs)
    x1, y1 = max(o["rect"][0] + o["rect"][2] for o in outputs), max(o["rect"][1] + o["rect"][3] for o in outputs)
    return x0, y0, x1 - x0, y1 - y0


def output_at(outputs, rect):
    """the output whose layout rectangle holds the centre of the window rectangle (x, y, w, h), or None"""
    if not rect:
        return None
    cx, cy = rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0
    for o in outputs:
        x, y, w, h = o["rect"]
        if x <= cx < x + w and y <= cy < y + h:
            return o
    return None


def xrdb_with_dpi(text, dpi):
    """the X resources `text` with Xft.dpi set to `dpi` (every older Xft.dpi line dropped); None for dpi removes it"""
    keep = [l for l in (text or "").splitlines() if not re.match(r"\s*Xft[.*]dpi\s*:", l)]
    if dpi is not None:
        keep.append("Xft.dpi:\t%d" % dpi)
    return "".join(l + "\n" for l in keep)


# ---- T37 notifications
THRESHOLD_S, LONG_S, SHORT_S = 3, 12, 2     # the profile's long-running threshold; the command over it; the command under it
NOTIFY_TABLE = "notifications.preferences"  # app/src/terminal/session_settings.rs (toml_path of `notifications`)
NOTIFY_HOP_RE = re.compile(r"hop=desktop_notification title_chars=(\d+) body_chars=(\d+)")       # crates/warpui_core/src/input_order.rs
CONTROL_APP = "appci-control"
MAC_NOTIFY_ENV = "APPCI_MAC_NOTIFY_OK"
MAC_NOTIFY_NEEDS = ("NEEDS: on the Mac runner the owner answers the macOS prompt 'Phosphor would like to send you notifications' once for the test "
                    "bundle, then sets %s=1 for the job. With notifications enabled the app asks macOS for permission; an unanswered prompt belongs to "
                    "UserNotificationCenter, which the harness treats as a system dialog in front of the app and stops every later case" % MAC_NOTIFY_ENV)


def notify_hops(text):
    """[(title chars, body chars)] of every hop=desktop_notification line"""
    return [(int(a), int(b)) for a, b in NOTIFY_HOP_RE.findall(text or "")]


def notification_settings(text, threshold_s):
    """-> (settings text with desktop notifications enabled and the long-running threshold at `threshold_s` seconds, None), or (None, why) when the
    file writes the table in a form toml_text cannot edit. The file format of the Duration is a plain integer of seconds (settings/init_tests.rs)."""
    if T.get_key(text, "notifications", "preferences") is not None or re.search(r"^\s*notifications\.preferences\s*=", text, re.M) \
            or re.search(r"^\s*notifications\s*=", text, re.M):
        return None, ("the settings file writes notifications.preferences as an inline table or a dotted key, which toml_text cannot edit without "
                      "rewriting the owner's file")
    for key, lit in (("mode", '"enabled"'), ("is_long_running_enabled", "true"), ("long_running_threshold", "%d" % threshold_s),
                     ("is_password_prompt_enabled", "true"), ("is_agent_task_completed_enabled", "true"), ("is_needs_attention_enabled", "true"),
                     ("play_notification_sound", "false")):
        text = T.set_key(text, NOTIFY_TABLE, key, lit)
    return text, None


def recorder_pre_script(recorder, log, ready, err):
    """the shell lines app.py's launcher sources before the app: the recorder owns the name before the app can ask for it"""
    q = shlex.quote
    return ("rm -f %s\n" % q(ready) +
            "python3 %s serve --log %s --ready %s >> %s 2>&1 &\n" % (q(recorder), q(log), q(ready), q(err)) +
            "_i=0; while [ ! -s %s ] && [ $_i -lt 50 ]; do sleep 0.1; _i=$((_i+1)); done\n" % q(ready))


def own_records(records):
    """the notifications the APP sent: everything but the harness's own control"""
    return [x for x in records if x.get("app") != CONTROL_APP]


TITLE_START_CHARS = 8          # the title is "'<the command's first characters>...' finished after Ns" (terminal/view.rs create_notification_content)


def judge_notifications(records, hops, want, needle=None, recorded=True, starts=None):
    """problems. records = what the notification service received (judged when `recorded`), hops = the app's hop lines, want = how many are right.
    With want == 1: a `needle` must be in the one notification's text (a token the command PRINTED: the body is "Latest output: <the end of the
    output>"), and with `starts` (the command line) the title must begin with the command's first characters in a quote. Round 14 (#2109): the
    needle was a token at character 16 of the command line; the title holds the command's first 16 characters by design, so it was never there."""
    probs = []
    if recorded:
        shown = [("%s | %s" % (x.get("summary", ""), x.get("body", "")))[:120] for x in records]
        if len(records) != want:
            probs.append("the notification service received %d notification(s), expected %d: %r" % (len(records), want, shown[:4]))
        elif want == 1 and needle and needle not in (records[0].get("summary", "") + " " + records[0].get("body", "")):
            probs.append("the notification is not this command's: the token it printed (%r) is not in %r" % (needle, shown[0]))
        elif want == 1 and starts and not records[0].get("summary", "").startswith("'" + starts[:TITLE_START_CHARS]):
            probs.append("the notification's title does not start with the command (%r): %r" % ("'" + starts[:TITLE_START_CHARS], shown[0]))
    if len(hops) != want:
        probs.append("the app logged %d hop=desktop_notification line(s), expected %d%s" % (
            len(hops), want, " (the service and the app's own trace disagree)" if recorded and len(records) == want else ""))
    return probs


def long_cmd(shell, secs, append, say=None):
    """a command that runs `secs` seconds, then runs `append` (Ctx.append_cmd of the pane's shell); with `say` it then prints that word as the
    last line of its output (what a notification's body quotes)"""
    if shell == "pwsh":
        return "Start-Sleep -Seconds %d; %s%s" % (secs, append, "; Write-Output %s" % say if say else "")
    if shell == "cmd":
        return "ping -n %d 127.0.0.1 >nul & %s%s" % (secs + 1, append, " & echo %s" % say if say else "")
    return "sleep %d; %s%s" % (secs, append, "; echo %s" % say if say else "")


# ---- T38 drag and drop
DRAG_NEEDS = ("NEEDS: a drag source on the Linux test machine: the packages python3-gi and gir1.2-gtk-3.0 (dragsrc.py, both display systems), or "
              "the package dragon-drop (binary `dragon-drop` or `dragon`). Owner: machine change; nothing is installed by the harness")
WIN_DRAG_NEEDS = ("NEEDS: a harness capability that is not built: an OLE drag source for Windows (IDropSource / DoDragDrop in a helper process) and "
                  "Windows pointer synthesis (mouse SendInput, r14-test-gaps T31)")
MAC_DRAG_NEEDS = ("NEEDS: a harness capability that is not built: a pasteboard drag source for macOS (NSDraggingSource in a helper app) and a macOS "
                  "pointer tool (r14-test-gaps T32)")
WAYLAND_DROP_ISSUE = "#2085"            # the native Wayland drop target (vendored winit, wayland/dnd); before it a Wayland drop could not work
WAYLAND_DROP_KILL_SWITCH = "PHOSPHOR_WAYLAND_DND"
WL_DND_HOP_RE = re.compile(r"wayland_dnd hop=(\w+)((?: \w+=[\w.]+)*)")                              # lib/winit/src/platform_impl/linux/wayland/dnd/mod.rs


def wl_dnd_hops(text):
    """the app's own account of a Wayland drag, in order (the vendored winit's drop target logs counts, never a path):
    ['enter accepted=true reading=true', 'data bytes=41 usable=true', 'drop waiting_for_data=false', 'finish accepted=true finished=true']"""
    return [m.group(1) + m.group(2) for m in WL_DND_HOP_RE.finditer(text or "")]


def gdk_backend(app_display):
    """GDK_BACKEND for the drag source: the display system the APP uses, so the source can never open another display (the user's own session)"""
    return "x11" if app_display == "x11" else "wayland"


def drag_source_argv(have_gi, dragon, ready, at, paths):
    """-> (argv, kind) of the drag source to start, or (None, None): dragsrc.py when GTK loads, else dragon-drop"""
    if have_gi:
        return ["python3", os.path.join(HERE, "dragsrc.py"), "--ready", ready, "--at", "%d,%d" % at] + list(paths), "dragsrc.py (GTK 3)"
    if dragon:
        return [dragon] + list(paths), os.path.basename(dragon)
    return None, None


def drag_events(text):
    """(first line as dict or None, [event names]) of dragsrc.py's --ready file"""
    first, events = None, []
    for line in (text or "").splitlines():
        try:
            d = json.loads(line)
        except ValueError:
            continue
        if "event" in d:
            events.append(d["event"])
        elif first is None:
            first = d
    return first, events


def drop_line_verdict(cmds, path, out):
    """None when the shell received exactly one line that is `ls -d <path as ONE argument> > out`, else what is wrong"""
    if len(cmds) != 1:
        return "the shell received %d line(s), expected one: %r" % (len(cmds), cmds[:3])
    try:
        words = shlex.split(cmds[0])
    except ValueError as e:
        return "the received line %r does not parse as shell words (%s): the dropped path is not quoted" % (cmds[0], e)
    if words != ["ls", "-d", path, ">", out]:
        if "ls -d" in cmds[0] and path not in " ".join(words) and len(words) <= 4:
            return "nothing was inserted by the drop: the shell received %r" % cmds[0]
        return "the shell received %r = words %r, expected `ls -d`, the path %r as ONE argument, `> %s`" % (cmds[0], words, path, out)
    return None


# ---- T39 IME
PINYIN = (("nihao", "你好"), ("zhongwen", "中文"))        # typed letters -> the first candidate, committed with Space
LINUX_IME_NEEDS = ("NEEDS: an input method on the Linux test machine: the packages fcitx5 and fcitx5-chinese-addons (Wayland and X11), or ibus and "
                   "ibus-libpinyin (X11 only: ibus has no input-method-v2 front end for sway). Owner: machine change; nothing is installed by the harness")
WIN_IME_NEEDS = ("NEEDS: the language Chinese (Simplified, China) with the Microsoft Pinyin keyboard added for the runner user on the Windows VM "
                 "(Settings > Time & Language > Language > Add a language). Owner: machine change; the harness selects it for the Phosphor window only")
MAC_IME_NEEDS = ("NEEDS: the owner enables the Pinyin - Simplified input source for the runner user (System Settings > Keyboard > Text Input > Input "
                 "Sources). The macOS arm of this test is intl_ime_pinyin / intl_ime_japanese (cases_caseintl.py), which type the same compositions "
                 "and skip with NEEDS_OWNER until then")
FCITX_PINYIN_FILES = ("/usr/share/fcitx5/inputmethod/pinyin.conf",)
IBUS_ENGINES = (("libpinyin", "/usr/share/ibus/component/libpinyin.xml"), ("pinyin", "/usr/share/ibus/component/pinyin.xml"))
WIN_PINYIN_LANGS = ("zh-hans", "zh-cn")


def linux_ime_plan(app_display, which, exists):
    """-> dict(framework, engine, plain, xmodifiers) for the input method to use, or a `NEEDS:` string. which(name) -> path or None; exists(path)."""
    if which("fcitx5") and which("fcitx5-remote") and any(exists(f) for f in FCITX_PINYIN_FILES):
        return dict(framework="fcitx5", engine="pinyin", plain="keyboard-us", xmodifiers="@im=fcitx")
    if app_display == "x11" and which("ibus-daemon") and which("ibus"):
        for engine, f in IBUS_ENGINES:
            if exists(f):
                return dict(framework="ibus", engine=engine, plain="xkb:us::eng", xmodifiers="@im=ibus")
    return LINUX_IME_NEEDS


def fcitx_profile(engine):
    """~/.config/fcitx5/profile: one group, the US keyboard first (inactive state) and the engine under test"""
    return ("[Groups/0]\nName=Default\nDefault Layout=us\nDefaultIM=%s\n\n[Groups/0/Items/0]\nName=keyboard-us\nLayout=\n\n"
            "[Groups/0/Items/1]\nName=%s\nLayout=\n\n[GroupOrder]\n0=Default\n" % (engine, engine))


def ime_pre_script(plan, addr_file, log):
    """the shell lines sourced before the app: the input method's environment for the app, and its daemon inside the app's session (not daemonized:
    it must stay in the launcher's process group, which is all the harness ever signals)"""
    q = shlex.quote
    head = "export XMODIFIERS=%s GTK_IM_MODULE=%s QT_IM_MODULE=%s\nprintf '%%s\\n' \"$DBUS_SESSION_BUS_ADDRESS\" > %s\n" % (
        q(plan["xmodifiers"]), plan["framework"].rstrip("5"), plan["framework"].rstrip("5"), q(addr_file))
    if plan["framework"] == "fcitx5":
        return head + "fcitx5 >> %s 2>&1 &\n_i=0; while ! fcitx5-remote >/dev/null 2>&1 && [ $_i -lt 50 ]; do sleep 0.1; _i=$((_i+1)); done\n" % q(log)
    return head + "ibus-daemon -rx >> %s 2>&1 &\n_i=0; while ! ibus address >/dev/null 2>&1 && [ $_i -lt 50 ]; do sleep 0.1; _i=$((_i+1)); done\n" % q(log)


def ime_switch_argv(plan, target):
    """(argv that selects `target`, argv that prints the current input method) for the plan's framework"""
    if plan["framework"] == "fcitx5":
        return ["fcitx5-remote", "-s", target], ["fcitx5-remote", "-n"]
    return ["ibus", "engine", target], ["ibus", "engine"]


def win_pinyin_installed(langs):
    """True when the user's language list (Get-WinUserLanguageList as [{tag, tips}]) has Chinese (Simplified) with at least one input method"""
    for l in langs or []:
        tag = str((l or {}).get("tag", "")).lower()
        if tag.startswith(WIN_PINYIN_LANGS) and (l.get("tips") or []):
            return True
    return False


# ================================================================= shared runtime helpers
def linux_app(ctx):
    """True for the Linux App launcher (app.py App: the private dbus session and the session-pre hook exist only there)"""
    return ctx.app is not None and hasattr(ctx.app, "display") and not ctx.win and not U.mac(ctx)


def desk_dir(ctx):
    d = os.path.join(ctx.run, "desk")
    os.makedirs(d, exist_ok=True)
    return d


def arm_session_pre(ctx, name, text):
    """write the lines the next launch sources inside the app's session and point the launcher at them"""
    p = os.path.join(desk_dir(ctx), "%s_pre.sh" % name)
    with open(p, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
    ctx.app.extra_env[SESSION_PRE_ENV] = p


def disarm_session_pre(ctx):
    try:
        ctx.app.extra_env.pop(SESSION_PRE_ENV, None)
    except Exception:
        pass


def relaunch(ctx, r, label):
    """stop and start the app on the same profile (cases_robust.start_app); True when a shell came up"""
    _n, ok = R.start_app(ctx, r, label)
    if ok:
        S.after_relaunch(ctx)
        time.sleep(0.5)
    return ok


class Away:
    """the keyboard focus given to the reference terminal and taken back (cases.ref_terminal, as focus_cycle). Linux / macOS keep both windows and
    switch; Windows keeps one target, so leaving opens the reference and coming back closes it."""

    def __init__(self, ctx):
        self.ctx, self.ref = ctx, None

    def open(self):
        """-> None when the reference is up (it has the focus), else why there is none"""
        self.ref = C.ref_terminal(self.ctx)
        if not self.ref:
            return getattr(self.ctx, "ref_reason", None) or "no reference terminal on this machine"
        return None

    @property
    def can_switch(self):
        return self.ref is not None and bool(self.ref.switch)

    def leave(self):
        """-> None when the reference has the focus, else why not"""
        if self.ref is None:
            return self.open()
        if not self.ref.switch:
            return "this backend cannot return to a reference terminal it left"
        try:
            self.ref.switch("reference")
        except RuntimeError as e:
            return str(e)
        return None

    def back(self):
        """Phosphor has the focus again -> None, else why not"""
        if self.ref is None:
            return None
        try:
            if self.ref.switch:
                self.ref.switch("phosphor")
            else:
                self.ref.close()
                self.ref = None
                time.sleep(1.2)
                self.ctx.be.refocus()
        except RuntimeError as e:
            return str(e)
        return None

    def close(self):
        if self.ref is not None:
            try:
                C.close_ref(self.ref)
            except Exception:
                pass
            self.ref = None
        self.ctx.prof.ref_active = False
        time.sleep(0.8)
        try:
            self.ctx.be.refocus()
        except Exception:
            pass


# ================================================================= T35 scale_matrix
def _run(argv, **kw):
    return subprocess.run(argv, capture_output=True, timeout=kw.pop("timeout", 15), **kw)


class LayoutMouse(U.Mouse):
    """U.Mouse whose Wayland extent is the whole output layout as it is NOW (cases_ui caches the first output's size, which a scale change or a
    second output makes wrong: the virtual pointer's absolute motion spans the layout box)"""

    def _wl_extent(self):
        b = layout_bbox(sway_outputs(_run(["swaymsg", "-t", "get_outputs", "-r"]).stdout))
        return (int(b[2]), int(b[3])) if b else (1600, 1000)


class SwayScale:
    """the scale knob of headless sway: `output <name> scale <s>` while the app runs; the window floats at a set logical size"""
    how = "sway `output <name> scale`"
    second_output = True

    def __init__(self, ctx, r):
        self.ctx, self.pid, self.extra = ctx, ctx.app.pid(), None
        outs = self.outputs()
        if not outs:
            r.skip("swaymsg -t get_outputs gave no active output: the scale cannot be set")
        self.out = outs[0]
        self.base_scale, self.mode = self.out["scale"], self.out["mode"]
        node = WU.Window(ctx)._sway_node()
        self.was_floating = bool(node) and node.get("type") == "floating_con"

    def outputs(self):
        return sway_outputs(_run(["swaymsg", "-t", "get_outputs", "-r"]).stdout)

    def msg(self, cmd):
        """one sway command -> None, or why it failed"""
        p = _run(["swaymsg", cmd])
        out = p.stdout.decode(errors="replace")
        ok = p.returncode == 0 and '"success": false' not in out and '"success":false' not in out      # swaymsg can exit 0 on a refused command
        return None if ok else "swaymsg %r: %s" % (cmd, (p.stderr.decode(errors="replace") or out).strip()[:160])

    def win(self, cmd):
        return self.msg("[pid=%d] %s" % (self.pid, cmd))

    def rect(self):
        return U.sway_window_rect(self.pid)

    def set(self, scale, lw, lh):
        err = self.msg("output %s scale %s" % (self.out["name"], fmt_scale(scale)))
        err = err or self.win("floating enable, resize set width %d px height %d px, move absolute position 0 0" % (lw, lh))
        time.sleep(0.8)
        return err

    def origin_scale(self):
        """((x, y) of the window in layout pixels, layout pixels per device pixel) for the app's find hook, or None"""
        rect = self.rect()
        o = output_at(self.outputs(), rect)
        return ((rect[0], rect[1]), 1.0 / o["scale"]) if rect and o else None

    def add_output(self, scale):
        """a second headless output right of the first -> (name, None) or (None, why)"""
        before = self.outputs()
        err = self.msg("create_output")
        if err:
            return None, err
        time.sleep(0.6)
        new = [o for o in self.outputs() if o["name"] not in {b["name"] for b in before}]
        if not new:
            return None, "`swaymsg create_output` succeeded but get_outputs lists no new output"
        self.extra = new[0]["name"]
        x = max(b["rect"][0] + b["rect"][2] for b in before)
        err = self.msg("output %s resolution %dx%d position %d 0 scale %s" % (self.extra, self.mode[0], self.mode[1], x, fmt_scale(scale)))
        time.sleep(0.6)
        return (self.extra, None) if not err else (None, err)

    def move_to(self, name):
        err = self.win("move container to output %s" % name)
        o = next((x for x in self.outputs() if x["name"] == name), None)
        if not err and o:
            err = self.win("move absolute position %d %d" % (o["rect"][0], o["rect"][1]))
        time.sleep(0.8)
        return err

    def restore(self):
        if self.extra:
            self.win("move container to output %s" % self.out["name"])
            if self.msg("output %s unplug" % self.extra):
                self.msg("output %s disable" % self.extra)
            self.extra = None
        self.msg("output %s scale %s" % (self.out["name"], fmt_scale(self.base_scale)))
        if not self.was_floating:
            self.win("floating disable")
        time.sleep(1.0)


class XrdbScale:
    """the scale knob of X11: the Xft.dpi X resource, which the app's windowing layer re-reads when RESOURCE_MANAGER changes; the window is then
    sized in device pixels for the wanted logical size (the job's Xvfb has no window manager)"""
    how = "X resources (`xrdb`, Xft.dpi)"
    second_output = False

    def __init__(self, ctx, r):
        if not shutil.which("xrdb"):
            r.skip("NEEDS: `xrdb` on the Linux test machine (package x11-xserver-utils): the X11 display scale is the Xft.dpi X resource. Owner: machine change")
        self.ctx, self.pid = ctx, ctx.app.pid()
        g = U.get_geometry(ctx)
        if not g:
            r.skip("no Phosphor window found by xdotool: the scale arm cannot size it")
        self.wid, self.orig = g[0], (g[1], g[2])
        self.saved = _run(["xrdb", "-query"]).stdout.decode(errors="replace")
        dg = _run(["xdotool", "getdisplaygeometry"]).stdout.decode().split()
        self.mode = (int(dg[0]), int(dg[1])) if len(dg) == 2 else self.orig

    def _load(self, text):
        if text.strip():
            p = subprocess.run(["xrdb", "-load"], input=text.encode(), capture_output=True, timeout=15)
        else:
            p = _run(["xrdb", "-remove"])
        return None if p.returncode == 0 else "xrdb: %s" % p.stderr.decode(errors="replace").strip()[:160]

    def set(self, scale, lw, lh):
        err = self._load(xrdb_with_dpi(self.saved, xft_dpi(scale)))
        time.sleep(1.0)
        _run(["xdotool", "windowsize", self.wid, str(int(round(lw * scale))), str(int(round(lh * scale)))])
        _run(["xdotool", "windowmove", self.wid, "0", "0"])
        time.sleep(0.8)
        return err

    def origin_scale(self):
        o = U.x11_window_origin(self.pid)
        return (o, 1.0) if o else None

    def restore(self):
        self._load(self.saved)
        time.sleep(1.0)
        _run(["xdotool", "windowsize", self.wid, str(self.orig[0]), str(self.orig[1])])
        _run(["xdotool", "windowmove", self.wid, "0", "0"])
        time.sleep(1.0)


def stable_size(ctx, tag):
    """`stty size` once the grid has settled: two reads 0.6 s apart that agree (a third when they do not) -> (rows, cols) or None"""
    time.sleep(1.0)
    a = S.pane_size(ctx, tag + "a")
    time.sleep(0.6)
    b = S.pane_size(ctx, tag + "b")
    if a != b:
        time.sleep(1.0)
        b = S.pane_size(ctx, tag + "c")
    return b


def click_word_arm(ctx, r, m, knob, label, tag):
    """six known words on one line; a double click on the fifth, placed from the app's own draw position through the knob's pixel mapping, then the
    copy key: the clipboard (a sentinel before) holds exactly that word. A pointer mapped with the wrong scale lands on another word or on none."""
    words = P.distinct_words(ctx.rng, 6)
    seen = P.print_file(ctx, r, "sc_words_%s.txt" % tag, " ".join(words) + "\n", tokens=words)
    if seen is None:
        return
    w = words[4]
    os_ = knob.origin_scale()
    if not os_:
        r.check(False, "[%s] the app's window was not found on an output: the pointer cannot be placed" % label)
        return
    origin, hscale = os_
    ctx.until(lambda: U.hook_find(ctx, w, origin=origin, scale=hscale, since=seen) is not None, 5, step=0.25)
    row = U.hook_find(ctx, w, origin=origin, scale=hscale, since=seen)
    if not row:
        msg = "[%s] the app logged no `appci_find` position for the printed word %r: the pointer cannot be placed" % (label, w)
        if os.environ.get("APPCI_NO_FIND_HOOK") == "1":
            r.unverifiable(msg + " (APPCI_NO_FIND_HOOK=1: the hook is switched off for this run)")
        else:
            r.check(False, msg + " (the text was not drawn, or not where a line is laid out)")
        return
    r.metric("click_%s_at" % tag, [row[1], row[2]])
    got = P.copy_after(ctx, lambda: m.click(row[1], row[2], 1, 2), tag="sc_click_%s" % tag)
    err = None if got.strip() == w else "clipboard %r" % got[:60]
    P.ptr_check(ctx, r, got, err, "[%s] double click on %r at (%d, %d) + copy: %s, expected exactly the word (line: %r)" % (label, w, row[1], row[2], err, " ".join(words)))
    WV.back_to_input(ctx, r)


def judge_grid(ctx, r, label, control, got, hop, scale):
    probs, weak = grid_scale_verdict(control, got, hop, scale)
    for p in probs:
        r.check(False, "[%s] %s" % (label, p))
    if weak and not probs:
        r.unverifiable("[%s] the grid has too few %s (%r) for a wrong scale to move it by more than the tolerance: that dimension is not judged"
                       % (label, " and ".join(weak), control))
    if hop is None:
        r.note("[%s] no hop=grid_resize line since the case began: `stty size` is compared with the control only" % label)


@case("scale_matrix", order=76, gui=True)
def c_scale_matrix(ctx, r):
    """T35 (G9, H5; #2078): display scale other than 1. For 1.25, 1.5 and 2: the window gets one logical size, first at scale 1 (the control), then at
    the scale; `stty size` at the scale equals the app's own grid and the control's (+- a cell or two; a grid in device pixels or one that ignored
    the scale differs by the factor); a double click on a printed word copies exactly it; a typed burst is exact. Wayland: the window then moves to a
    second headless output of another scale and both hold there. The scale, the window and the outputs are put back."""
    if ctx.win:
        r.skip("Windows: " + WIN_SCALE_NEEDS)
    if U.mac(ctx):
        r.skip("macOS: NEEDS: a display mode with another backing scale on the Mac runner (owner: a HiDPI / non-HiDPI mode switch of the runner's "
               "display) and a pointer tool for the word click (r14-test-gaps T32)")
    if not linux_app(ctx):
        r.skip("needs the Linux App launcher (not the sim backend)")
    if ctx.be.name == "wayland" and ctx.app.display == "x11":
        r.skip("xwayland pass: an XWayland client is scaled by the compositor and never told the scale; judged in the wayland and the x11 pass")
    m0 = U.mouse_or_skip(ctx, r)
    m = LayoutMouse(m0.be)
    knob = SwayScale(ctx, r) if ctx.be.name == "wayland" else XrdbScale(ctx, r)
    r.metric("scale_knob", knob.how)
    base = stable_size(ctx, "sc_base")
    if not r.check(base is not None, "the shell did not answer `stty size` before any scale change"):
        return
    off0 = ctx.log_size()
    controls, lsize = {}, None
    try:
        for s in SCALES:
            tag, label = scale_tag(s), "scale %s" % fmt_scale(s)
            lw, lh = logical_size(knob.mode[0], knob.mode[1], s)
            err = knob.set(1.0, lw, lh)
            if not r.check(err is None, "[%s] the control (scale 1, window %d x %d) could not be set: %s" % (label, lw, lh, err)):
                break
            control = stable_size(ctx, "sc%s_c" % tag)
            err = knob.set(s, lw, lh)
            if not r.check(err is None, "[%s] the scale could not be set: %s" % (label, err)):
                break
            got = stable_size(ctx, "sc%s_s" % tag)
            hop = last_grid(ctx.log_since(off0))
            ctx.shot("sc_%s" % tag)
            r.metric("grid_%s" % tag, dict(control=control, scaled=got, hop=hop, logical=[lw, lh]))
            judge_grid(ctx, r, label, control, got, hop, s)
            click_word_arm(ctx, r, m, knob, label, tag)
            U.exact_burst(ctx, r, "sc%s" % tag, 3)
            controls[s], lsize = control, (lw, lh)
            if not ctx.app.alive():
                break
        if not knob.second_output:
            r.note("second output: not played on X11 (the job's Xvfb has one screen); the Wayland pass plays it")
        elif SCALES[-1] in controls and ctx.app.alive():
            s2, label = SCALES[-1], "second output, scale %s" % fmt_scale(SCALES[-1])
            err = knob.set(1.0, lsize[0], lsize[1])
            name, why = knob.add_output(s2) if not err else (None, err)
            if not name:
                r.unverifiable("[second output] sway did not give a second headless output (%s): 'moving the window to the other output keeps "
                               "both' is NOT judged. NEEDS: sway with `create_output` on the headless backend" % why)
            else:
                err = knob.move_to(name)
                o = output_at(knob.outputs(), knob.rect())
                if r.check(err is None and o is not None and o["name"] == name,
                           "[%s] the window is on output %r after `move container to output %s` (%s)" % (label, o and o["name"], name, err)):
                    try:
                        ctx.be.refocus()
                    except Exception:
                        pass
                    got = stable_size(ctx, "sc_out2")
                    hop = last_grid(ctx.log_since(off0))
                    ctx.shot("sc_out2")
                    r.metric("grid_out2", dict(control=controls[s2], scaled=got, hop=hop, output=name))
                    judge_grid(ctx, r, label, controls[s2], got, hop, s2)
                    click_word_arm(ctx, r, m, knob, label, "out2")
                    U.exact_burst(ctx, r, "sc_out2", 3)
        r.check(ctx.app.alive(), "the app died in the scale case")
    finally:
        knob.restore()
        try:
            ctx.be.refocus()
        except Exception:
            pass
    if ctx.app.alive():
        end = stable_size(ctx, "sc_end")
        r.check(end == base, "after the scale, the window and the outputs were put back `stty size` is %r, it was %r before the case" % (end, base))
        U.exact_burst(ctx, r, "sc_after", 3)


# ================================================================= T37 desktop_notification
class NotifySetup:
    """notifications enabled in the profile's settings (threshold THRESHOLD_S) and the app relaunched on them; on Linux with the recording service
    inside the app's session, proven by a control notification of the harness's own. restore() puts the settings back and relaunches."""

    def __init__(self, ctx, r, label):
        self.ctx, self.r, self.label = ctx, r, label
        self.path, self.text0, self.changed, self.recorded = R.settings_file(ctx), None, False, False
        d = desk_dir(ctx)
        self.log, self.ready = os.path.join(d, "%s_notify.jsonl" % label), os.path.join(d, "%s_notify.ready" % label)
        self.err = os.path.join(d, "%s_notify.err" % label)

    def start(self):
        ctx, r = self.ctx, self.r
        with open(self.path, encoding="utf-8", errors="replace") as f:
            self.text0 = f.read()
        new, why = notification_settings(self.text0, THRESHOLD_S)
        if new is None:
            r.skip(why)
        linux = linux_app(ctx)
        if linux:
            for p in (self.log, self.ready):
                if os.path.exists(p):
                    os.remove(p)
            arm_session_pre(ctx, self.label, recorder_pre_script(os.path.join(HERE, "notify_recorder.py"), self.log, self.ready, self.err))
        self.changed = True
        with open(self.path, "w", encoding="utf-8") as f:
            f.write(new)
        if not relaunch(ctx, r, self.label + "_start"):
            return False
        if not linux:
            r.note("no recording notification service on this platform: the verdicts are the app's own hop=desktop_notification lines")
            return True
        return self._prove_recorder()

    def _prove_recorder(self):
        ctx, r = self.ctx, self.r
        ctx.until(lambda: os.path.exists(self.ready) and os.path.getsize(self.ready) > 0, 8, step=0.2)
        try:
            info = json.loads(open(self.ready, encoding="utf-8").read().splitlines()[0])
        except Exception:
            info = None
        if not info or not info.get("ok"):
            tail = ""
            try:
                tail = open(self.err, errors="replace").read()[-200:]
            except OSError:
                pass
            r.unverifiable("the recording notification service did not take org.freedesktop.Notifications on the app's private bus (%s%s): no "
                           "notification can be observed, nothing is judged" % ((info or {}).get("why", "no ready line within 8 s"), "; stderr: " + tail if tail else ""))
            return False
        tok = "control " + ctx.tok(6)
        try:
            NR.send(info["address"], tok, "sent by the harness", app=CONTROL_APP)
        except Exception as e:
            r.unverifiable("the harness's control notification could not be sent on the app's bus (%s: %s): the recording path is not proven" % (type(e).__name__, e))
            return False
        ctx.until(lambda: any(x.get("summary") == tok for x in NR.read_log(self.log)), 5, step=0.2)
        if not any(x.get("summary") == tok for x in NR.read_log(self.log)):
            r.unverifiable("the control notification was sent but the service did not record it: the recording path is not proven")
            return False
        self.recorded = True
        return True

    def mark(self):
        return len(NR.read_log(self.log)) if self.recorded else 0, self.ctx.log_size()

    def since(self, mark):
        """(the app's notifications the service recorded, the app's hop lines) since mark()"""
        recs = own_records(NR.read_log(self.log)[mark[0]:]) if self.recorded else []
        return recs, notify_hops(self.ctx.log_since(mark[1]))

    def wait(self, mark, n, timeout):
        """until at least n notifications are seen (the service's on Linux, else the hop's)"""
        def seen():
            recs, hops = self.since(mark)
            return (len(recs) if self.recorded else len(hops)) >= n
        return bool(self.ctx.until(seen, timeout, step=0.2))

    def judge(self, mark, want, what, needle=None, starts=None):
        recs, hops = self.since(mark)
        self.r.metric("notify_%s" % what.split(":")[0].replace(" ", "_"), dict(recorded=len(recs) if self.recorded else None, hops=len(hops)))
        probs = judge_notifications(recs, hops, want, needle, self.recorded, starts)
        for p in probs:
            self.r.check(False, "%s: %s" % (what, p))
        return not probs

    def restore(self):
        if not self.changed:
            return
        ctx = self.ctx
        try:
            disarm_session_pre(ctx)
            with open(self.path, "w", encoding="utf-8") as f:
                f.write(self.text0)
            relaunch(ctx, self.r, self.label + "_restore")
        except Exception as e:          # never mask the case's own verdict
            self.r.check(False, "%s: the profile's settings could not be put back and the app relaunched: %r" % (self.label, e))


def notify_or_skip(ctx, r, name):
    R.need_relaunch_app(ctx, "%s relaunches the app on a settings file with desktop notifications enabled" % name)
    if U.mac(ctx) and os.environ.get(MAC_NOTIFY_ENV) != "1":
        r.skip("macOS: " + MAC_NOTIFY_NEEDS)
    if not ctx.log_path:
        r.skip("no app log: hop=desktop_notification cannot be read")


def timed_command(ctx, sh, secs, tag, say=None):
    """type a `secs`-second command with an effect file at the owner rate + Enter -> (token, effect file, time of the Enter). With `say` the
    command prints that word last (long_cmd)."""
    tok, f = ctx.tok(8).lower(), "dn_%s_%s.txt" % (tag, ctx.tok(4).lower())
    ctx.rm(f)
    ctx.be.type(long_cmd(sh, secs, ctx.append_cmd(sh, tok, f), say), ctx.rng.randint(10, 25), enter=True)
    return tok, f, time.time()


@case("desktop_notification", order=77, gui=True)
def c_desktop_notification(ctx, r):
    """T37 (G-row 'notifications', H7; #2075): a long command that ends while the window has the focus sends no desktop notification; the same
    command ending while another window has the focus sends exactly one, naming the command; a command under the threshold ending unfocused sends
    none. Linux: a recording notification service on the app's private bus, proven by a control notification first. Elsewhere: the app's hop."""
    notify_or_skip(ctx, r, "desktop_notification")
    sh = ctx.pane_shell()
    ns, away = NotifySetup(ctx, r, "dn"), Away(ctx)
    try:
        if not ns.start():
            return
        if not ctx.win:          # Linux / macOS: the reference is opened once, up front; later switches are immediate
            why = away.open()
            if why:
                r.skip("no reference terminal to give the keyboard focus to (%s): 'ends while unfocused' cannot be played" % why)
            time.sleep(2.5)
            why = away.back()
            if why:
                r.check(False, "the focus did not return to Phosphor from the reference terminal: %s" % why)
                return
        # ---- control: the long command ends while Phosphor has the focus
        mark = ns.mark()
        tok, f, _t = timed_command(ctx, sh, LONG_S, "focused")
        done = ctx.until(lambda: ctx.lines(f) == [tok], LONG_S + 20, step=0.25)
        if not r.check(bool(done), "focused: the %d s command did not finish as typed (%s has %r, expected [%r])" % (LONG_S, f, ctx.lines(f), tok)):
            return
        time.sleep(3.0)
        ns.judge(mark, 0, "focused: a %d s command ended while Phosphor had the focus" % LONG_S)
        # ---- the same command ends while the reference terminal has the focus
        mark = ns.mark()
        said = "dnsaid" + ctx.tok(8).lower().replace("_", "")      # printed by the command: the notification's body quotes the end of the output
        tok, f, t_enter = timed_command(ctx, sh, LONG_S, "away", say=said)
        why = away.leave()
        left_after = time.time() - t_enter
        r.metric("focus_left_after_enter_s", round(left_after, 2))
        if why:
            ctx.until(lambda: ctx.lines(f) == [tok], LONG_S + 20, step=0.25)
            r.unverifiable("the reference terminal did not take the focus (%s): 'ends while unfocused' is NOT judged" % why)
            return
        if left_after > LONG_S - 2:
            ctx.until(lambda: ctx.lines(f) == [tok], LONG_S + 20, step=0.25)
            away.back()
            r.unverifiable("the focus left Phosphor %.1f s after the Enter of a %d s command: the command may have ended first; not judged" % (left_after, LONG_S))
            return
        done = ctx.until(lambda: ctx.lines(f) == [tok], LONG_S + 20, step=0.25)
        r.check(bool(done), "unfocused: the %d s command did not finish as typed (%s has %r)" % (LONG_S, f, ctx.lines(f)))
        ns.wait(mark, 1, 8)
        time.sleep(2.0)                  # a second notification for the same block would arrive here
        ctx.shot("dn_unfocused")
        ns.judge(mark, 1, "unfocused: a %d s command ended while the reference terminal had the focus" % LONG_S, needle=said,
                 starts=long_cmd(sh, LONG_S, ""))
        why = away.back()
        if why:
            r.check(False, "the focus did not return to Phosphor: %s" % why)
            return
        # ---- under the threshold, unfocused: none (an app that notifies for every command fails here)
        if away.can_switch:
            mark = ns.mark()
            tok, f, t_enter = timed_command(ctx, sh, SHORT_S, "short")
            why = away.leave()
            late = time.time() - t_enter > SHORT_S - 0.5
            ctx.until(lambda: ctx.lines(f) == [tok], SHORT_S + 15, step=0.25)
            time.sleep(3.0)
            if why or late:
                r.unverifiable("the focus did not leave Phosphor before the %d s command ended (%s): 'under the threshold: none' is NOT judged"
                               % (SHORT_S, why or "the switch took too long"))
            else:
                ns.judge(mark, 0, "short: a %d s command (threshold %d s) ended while the reference terminal had the focus" % (SHORT_S, THRESHOLD_S))
            away.back()
        else:
            r.note("'under the threshold: none' NOT played on this platform: the harness keeps one target window, and opening the reference "
                   "terminal takes longer than a command under the threshold runs")
        r.check(ctx.app.alive(), "the app died in the notification case")
    finally:
        away.close()
        ns.restore()
    U.exact_burst(ctx, r, "dn_after", 3)


@case("desktop_notification_agent_question", modes=("agent",), order=78, gui=True)
def c_desktop_notification_agent_question(ctx, r):
    """T37, agent half (scripted fake model): a question card that arrives while another window has the focus sends exactly one desktop notification
    (none before the card, with the focus already away); the card is then answered with the focus back and the turn ends, which sends no second."""
    AR.need_fake(ctx, r)
    notify_or_skip(ctx, r, "desktop_notification_agent_question")
    ns, away, f = NotifySetup(ctx, r, "dnq"), Away(ctx), AR.fake()
    try:
        if not ns.start():
            return
        if ctx.log_view_mode() != "agent":
            ctx.be.type("/agent", 12, enter=True)
            if not ctx.until(lambda: ctx.log_view_mode() == "agent", 10):
                r.skip("after the relaunch the pane did not open the agent view (hop=view_mode says %s)" % ctx.log_view_mode())
            time.sleep(0.8)
        f.clear_faults()
        if not ctx.win:
            why = away.open()
            if why:
                r.skip("no reference terminal to give the keyboard focus to (%s): 'a question in the background' cannot be played" % why)
            time.sleep(2.5)
            why = away.back()
            if why:
                r.check(False, "the focus did not return to Phosphor from the reference terminal: %s" % why)
                return
        tok = "N" + ctx.tok(6).upper().replace("_", "")
        f.inject("ask_slow", times=1, match=tok, secs=20 if ctx.win else 12)
        off_q, tools0, mark = ctx.log_size(), len(f.tool_results), ns.mark()
        AG.type_fast(ctx, "Please ask me a question %s" % tok)
        if not AG.wait_marker(ctx, off_q, "[byop] opening stream", timeout=60):
            r.check(False, "no model request within 60 s of the prompt (the question was never asked)")
            return
        why = away.leave()
        if why:
            r.unverifiable("the reference terminal did not take the focus (%s): 'a question in the background' is NOT judged" % why)
            return
        if AQ.ask_blocked_ids(ctx.log_since(off_q)):
            r.unverifiable("the question card was already up when the focus left Phosphor (the script's delay was too short on this runner): not judged")
            return
        before = ns.since(mark)
        r.check(not before[0] and not before[1], "a desktop notification was sent before the question arrived (service %d, hops %d)" % (len(before[0]), len(before[1])))
        if not ctx.until(lambda: bool(AQ.ask_blocked_ids(ctx.log_since(off_q))), 60, step=0.25):
            r.check(False, "the question never became answerable within 60 s (ask actions %r, last turn state %r)"
                    % (AQ.ask_actions(ctx.log_since(off_q)), AR.last_turn_state(ctx.log_since(off_q))))
            return
        ns.wait(mark, 1, 8)
        time.sleep(2.0)
        ctx.shot("dnq_unfocused")
        recs, _h = ns.since(mark)
        if recs:
            r.metric("question_notification_chars", [len(recs[0].get("summary", "")), len(recs[0].get("body", ""))])
        ns.judge(mark, 1, "question: the card arrived while the reference terminal had the focus")
        why = away.back()
        if why:
            r.check(False, "the focus did not return to Phosphor: %s" % why)
            return
        time.sleep(0.6)
        if r.check(AQ.answer_card(ctx, r, off_q, "dnq"), "with the focus back, Enter did not answer the card (no ask_user_question action reached done)"):
            state = AR.wait_turn(ctx, off_q, ("success", "error", "cancelled"), 60)
            r.check(state == "success", "after the answer the turn ended %r, expected `success`" % state)
            r.check(len(f.tool_results) > tools0, "the model's next request carried no answer to the question")
        time.sleep(2.5)
        ns.judge(mark, 1, "answered: the turn ended with the focus on Phosphor (still one in total)")
        for p in AR.judge_errors(ctx.log_since(off_q), limit=2):
            r.check(False, p)
        r.check(ctx.app.alive(), "the app died in the notification question case")
    finally:
        try:
            f.clear_faults()
            AG.cleanup_tmp(ctx)
        finally:
            away.close()
            ns.restore()


# ================================================================= T38 drag_drop_file
def slow_drag(ctx, m, x0, y0, x1, y1):
    """press on the source, move to the target in steps, dwell there (the target answers the position before a drop counts), release"""
    x11 = ctx.be.name == "x11"
    m.move(x0, y0)
    time.sleep(0.2)
    if x11:
        _run(["xdotool", "mousedown", "1"])
    else:
        m._btn(1, True)
    time.sleep(0.25)
    steps = 12
    for i in range(1, steps + 1):
        x, y = x0 + (x1 - x0) * i / steps, y0 + (y1 - y0) * i / steps
        if x11:
            _run(["xdotool", "mousemove", str(int(x)), str(int(y))])
        else:
            m._move(x, y)
        time.sleep(0.05)
    for dx in (3, -3, 0):                 # a little motion on the target: a position update after the enter
        if x11:
            _run(["xdotool", "mousemove", str(int(x1 + dx)), str(int(y1))])
        else:
            m._move(x1 + dx, y1)
        time.sleep(0.12)
    time.sleep(0.4)
    if x11:
        _run(["xdotool", "mouseup", "1"])
    else:
        m._btn(1, False)
    time.sleep(0.3)


def app_window_rect(ctx):
    """(x, y, w, h) of the app's window in pointer coordinates, or None"""
    pid = ctx.app.pid()
    if ctx.be.name == "wayland":
        return U.sway_window_rect(pid)
    g, o = U.get_geometry(ctx), U.x11_window_origin(pid)
    return (o[0], o[1], g[1], g[2]) if g and o else None


def source_rect(ctx, proc, kind, at, ready):
    """(x, y, w, h) of the drag source's window once it is up, or None"""
    if kind.startswith("dragsrc"):
        ctx.until(lambda: drag_events(open(ready).read() if os.path.exists(ready) else "")[0] is not None, 8, step=0.2)
        first, _ = drag_events(open(ready).read() if os.path.exists(ready) else "")
        if not first or first.get("error"):
            return None
        if ctx.be.name == "wayland":
            return U.sway_window_rect(proc.pid)
        return (at[0], at[1], first.get("w", dragsrc.W), first.get("h", dragsrc.H))
    time.sleep(1.5)                       # dragon-drop
    if ctx.be.name == "wayland":
        return U.sway_window_rect(proc.pid)
    ids = _run(["xdotool", "search", "--onlyvisible", "--pid", str(proc.pid)]).stdout.decode().split()
    if not ids:
        return None
    kv = dict(l.split("=") for l in _run(["xdotool", "getwindowgeometry", "--shell", ids[-1]]).stdout.decode().split() if "=" in l)
    return int(kv["X"]), int(kv["Y"]), int(kv["WIDTH"]), int(kv["HEIGHT"])


@case("drag_drop_file", order=79, gui=True)
def c_drag_drop_file(ctx, r):
    """T38 (F2 made real, H8; #2075): `ls -d ` is typed, then a file whose name has a space is dragged from a drag source client onto the terminal
    and ` > file` + Enter is typed: the shell receives one line whose words are `ls -d`, the path as ONE argument, `> file`, and the file holds the
    path. The source's own events tell a drag that never began (harness) from one the app did not accept. Wayland (#2085, the drop target in the
    vendored winit) is judged like X11, and there the source must also not be told that the drop failed (the offer is finished, not cancelled)."""
    if ctx.win:
        r.skip("Windows: " + WIN_DRAG_NEEDS)
    if U.mac(ctx):
        r.skip("macOS: " + MAC_DRAG_NEEDS)
    if not linux_app(ctx):
        r.skip("needs the Linux App launcher (not the sim backend)")
    wl = ctx.be.name == "wayland"
    if wl and ctx.app.display == "x11":
        r.skip("xwayland pass: a drag between a Wayland source and an XWayland target goes through the compositor's bridge; judged in the wayland and the x11 pass")
    m = U.mouse_or_skip(ctx, r)
    if wl and not U.wl_pointer_real(ctx):
        r.skip("NEEDS: the session's virtual pointer (wlptr.py; APPCI_WL_PTR_HOLD=0, or the holder did not start): a drag cannot be played with sway's seat commands")
    env = dict(os.environ, GDK_BACKEND=gdk_backend(ctx.app.display), NO_AT_BRIDGE="1")
    have_gi = subprocess.run(["python3", os.path.join(HERE, "dragsrc.py"), "--probe"], capture_output=True, env=env, timeout=30).returncode == 0
    dragon = shutil.which("dragon-drop") or shutil.which("dragon")
    rect = app_window_rect(ctx)
    if not rect:
        r.skip("the app's window rectangle could not be read (sway tree / xdotool): no drop target")
    tag = ctx.tok(4).lower()
    path, out = os.path.join(ctx.W, "dnd file %s.txt" % tag), "dnd_out_%s.txt" % tag
    ready = os.path.join(desk_dir(ctx), "dragsrc_%s.ready" % tag)
    at = (rect[0] + max(0, rect[2] - dragsrc.W - 30), rect[1] + 30)
    argv, kind = drag_source_argv(have_gi, dragon, ready, at, [path])
    if not argv:
        r.skip(DRAG_NEEDS)
    r.metric("drag_source", kind)
    with open(path, "w") as fh:
        fh.write("x\n")
    ctx.rm(out)
    tx, ty = rect[0] + rect[2] // 2, rect[1] + rect[3] // 2
    src, mark, events = None, ctx.cmd_mark(), []
    log_off = ctx.log_size()
    try:
        ctx.be.type("ls -d ", ctx.rng.randint(10, 25))
        m.move(tx, ty)                        # the app uses its last pointer position for the drop: no motion reaches it while the source holds the grab
        time.sleep(0.3)
        if wl:
            for rule in ('no_focus [app_id="%s"]' % dragsrc.APP_ID,
                         'for_window [app_id="%s"] floating enable, move absolute position %d %d' % (dragsrc.APP_ID, at[0], at[1])):
                _run(["swaymsg", rule])
        src = subprocess.Popen(argv, env=env, start_new_session=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        with open(os.path.join(ctx.run, "pids"), "a") as pf:
            pf.write("dragsrc %d\n" % src.pid)
        srect = source_rect(ctx, src, kind, at, ready)
        if not srect:
            ctx.reset()                       # the typed `ls -d ` leaves the input
            r.unverifiable("the drag source (%s) showed no window in the job's display: the drag cannot be played, nothing is judged" % kind)
            return
        ctx.shot("dnd_source")
        slow_drag(ctx, m, srect[0] + srect[2] // 2, srect[1] + srect[3] // 2, tx, ty)
        if kind.startswith("dragsrc"):
            ctx.until(lambda: drag_events(open(ready).read())[1][-1:] in (["end"], ["failed"]), 5, step=0.2)
            events = drag_events(open(ready).read())[1]
            r.metric("drag_source_events", events)
        time.sleep(0.8)                       # the app collects the dropped files for a moment before it inserts them
        ctx.shot("dnd_dropped")
    finally:
        if src is not None:
            try:
                os.killpg(src.pid, 15)
            except Exception:
                pass
    try:
        ctx.be.refocus()
    except Exception:
        pass
    if kind.startswith("dragsrc") and "begin" not in events:
        ctx.reset()
        r.unverifiable("the drag source reported no drag at all (events %r): the pointer press and motion did not start a drag on it. The app was "
                       "never offered the file; nothing is judged" % events)
        return
    ctx.be.type(" > %s" % out, ctx.rng.randint(10, 25), enter=True)
    ctx.until(lambda: len(ctx.cmds_since(mark)) >= 1, 8)
    ctx.until(lambda: ctx.lines(out), 5)
    got = ctx.cmds_since(mark)
    known = ""
    if wl:
        try:
            hops = wl_dnd_hops(ctx.log_since(log_off))
        except Exception:
            hops = []
        r.metric("wayland_dnd_hops", hops)
        known = " (Wayland: the drop target of %s in the vendored winit; %s=0 turns it off; the app's wayland_dnd hops for this drag: %r)" % (
            WAYLAND_DROP_ISSUE, WAYLAND_DROP_KILL_SWITCH, hops)
    if kind.startswith("dragsrc"):
        r.check("data" in events, "the app did not take the dropped file: the drag began on the source and ended without the target asking for the "
                                  "data (source events %r)%s" % (events, known))
        if wl:
            r.check("failed" not in events, "the drag source was told the drop FAILED (source events %r): the app took the data and then did not finish "
                                            "the offer (wl_data_offer.finish needs the copy action the compositor selected)%s" % (events, known))
    err = drop_line_verdict(got, path, out)
    r.check(err is None, "after `ls -d `, the drop and ` > %s`: %s%s" % (out, err, known))
    r.check(ctx.lines(out) == [path], "`ls -d` of the dropped path printed %r, expected [%r]%s" % (ctx.lines(out), path, known))
    ctx.reset()
    r.check(ctx.app.alive(), "the app died in the drag and drop case")
    U.exact_burst(ctx, r, "dnd_after", 3)


# ================================================================= T39 ime_compose
class LinuxIme:
    """an input method daemon (fcitx5 or ibus) inside the app's private session; abc() / ime() select the plain layout / the engine under test and
    prove the selection by asking the daemon. The interface cases_caseintl.compose_raw / compose_editor take as `sw`."""

    def __init__(self, ctx, r, plan):
        self.ctx, self.r, self.plan, self.armed = ctx, r, plan, False
        d = desk_dir(ctx)
        self.addr_file, self.log = os.path.join(d, "ime_bus_address"), os.path.join(d, "ime_daemon.log")
        self.unproven = None

    def start(self):
        ctx, plan = self.ctx, self.plan
        if plan["framework"] == "fcitx5":
            p = os.path.join(ctx.prof.home, ".config", "fcitx5")
            os.makedirs(p, exist_ok=True)
            with open(os.path.join(p, "profile"), "w", encoding="utf-8") as f:
                f.write(fcitx_profile(plan["engine"]))
        if os.path.exists(self.addr_file):
            os.remove(self.addr_file)
        arm_session_pre(ctx, "ime", ime_pre_script(plan, self.addr_file, self.log))
        self.armed = True
        return relaunch(ctx, self.r, "ime_start")

    def _env(self):
        e = dict(os.environ, HOME=self.ctx.prof.home, XDG_CONFIG_HOME=os.path.join(self.ctx.prof.home, ".config"))
        try:
            e["DBUS_SESSION_BUS_ADDRESS"] = open(self.addr_file).read().strip()
        except OSError:
            pass
        return e

    def _select(self, target):
        sel, cur = ime_switch_argv(self.plan, target)
        subprocess.run(sel, env=self._env(), capture_output=True, timeout=15)
        time.sleep(0.5)
        now = subprocess.run(cur, env=self._env(), capture_output=True, timeout=15).stdout.decode(errors="replace").strip()
        if now != target:
            self.unproven = "%s reports the input method %r after selecting %r" % (self.plan["framework"], now, target)
        return now == target

    def abc(self):
        self._select(self.plan["plain"])

    def ime(self):
        self._select(self.plan["engine"])

    def restore(self):
        if not self.armed:
            return
        try:
            disarm_session_pre(self.ctx)
            relaunch(self.ctx, self.r, "ime_restore")
        except Exception as e:
            self.r.check(False, "the app could not be relaunched without the input method: %r" % e)


class WinIme:
    """Microsoft Pinyin selected for the Phosphor window's thread only (WM_INPUTLANGCHANGEREQUEST, as cases_caseintl.German); abc() puts the
    original layout back. Keys are SendInput virtual keys (WindowsBackend.keys), which the input method composes."""

    KLID, LANG = "00000804", 0x0804

    def __init__(self, ctx, r):
        import ctypes
        import winapi
        self.ctx, self.r, self.u = ctx, r, winapi.user32
        self.hwnd = winapi.main_window(ctx.app.pid())
        self.u.LoadKeyboardLayoutW.argtypes = [ctypes.c_wchar_p, ctypes.c_uint]
        self.u.LoadKeyboardLayoutW.restype = ctypes.c_void_p
        self.u.GetKeyboardLayout.argtypes = [ctypes.c_uint]
        self.u.GetKeyboardLayout.restype = ctypes.c_void_p
        self.tid = self.u.GetWindowThreadProcessId(self.hwnd, None)
        INTL.kbd_snapshot(ctx)
        self.orig = self.u.GetKeyboardLayout(self.tid)
        self.hkl, self.unproven = None, None

    def _to(self, hkl, lang):
        self.u.PostMessageW(self.hwnd, 0x50, 0, hkl)             # WM_INPUTLANGCHANGEREQUEST
        t = time.time()
        while time.time() - t < 5:
            if (self.u.GetKeyboardLayout(self.tid) or 0) & 0xFFFF == lang:
                time.sleep(0.4)
                return True
            time.sleep(0.2)
        self.unproven = "the Phosphor window's input language is 0x%04x after a request for 0x%04x" % ((self.u.GetKeyboardLayout(self.tid) or 0) & 0xFFFF, lang)
        return False

    def ime(self):
        if not self.hkl:
            self.hkl = self.u.LoadKeyboardLayoutW(self.KLID, 0)
        if not self.hkl:
            self.unproven = "LoadKeyboardLayout(%s) failed" % self.KLID
            return
        self._to(self.hkl, self.LANG)

    def abc(self):
        if self.orig:
            self._to(self.orig, (self.orig or 0) & 0xFFFF)

    def restore(self):
        try:
            if self.ctx.app.alive():
                self.abc()
        except Exception:
            pass


def win_language_list():
    """[{tag, tips}] of the runner user's language list (read-only), or [] when it cannot be read"""
    try:
        ps = subprocess.run(["powershell.exe", "-NoProfile", "-Command",
                             "Get-WinUserLanguageList | ForEach-Object { @{ tag = $_.LanguageTag; tips = @($_.InputMethodTips) } } | ConvertTo-Json -Depth 4 -Compress"],
                            capture_output=True, text=True, timeout=60)
        j = json.loads(ps.stdout)
        return j if isinstance(j, list) else [j]
    except Exception:
        return []


@case("ime_compose", order=66, gui=True)
def c_ime_compose(ctx, r):
    """T39 (F6, H9; #2075, and the test #1892 never had): Pinyin through a real input method. The letters of a word are typed with the input method
    selected: nothing reaches a raw-tty reader before the commit, Space commits and the reader gets exactly the word's UTF-8 bytes; in the input
    editor `echo ` + the composition + ` >> file` runs nothing while composing and the shell then receives exactly that line."""
    if U.mac(ctx):
        r.skip("macOS: " + MAC_IME_NEEDS)
    if ctx.app is None:
        r.skip("no app process (sim backend)")
    if ctx.win:
        if not win_pinyin_installed(win_language_list()):
            r.skip("Windows: " + WIN_IME_NEEDS)
        sw, raw = WinIme(ctx, r), False
    else:
        if not linux_app(ctx):
            r.skip("needs the Linux App launcher")
        if ctx.be.name == "wayland" and ctx.app.display == "x11":
            r.skip("xwayland pass: the input method path of an XWayland client is XIM through the compositor; judged in the wayland and the x11 pass")
        plan = linux_ime_plan(ctx.app.display, shutil.which, os.path.exists)
        if isinstance(plan, str):
            r.skip(plan)
        r.metric("input_method", "%s / %s" % (plan["framework"], plan["engine"]))
        sw, raw = LinuxIme(ctx, r, plan), bool(ctx.which("python3"))
    keys = lambda word: (lambda: ctx.be.keys(list(word), delay_ms=20))      # noqa: E731
    commit = lambda: ctx.be.key("space")                                    # noqa: E731
    try:
        if not ctx.win and not sw.start():
            return
        sw.ime()
        ctx.shot("ime_selected")
        sw.abc()
        if sw.unproven:
            r.unverifiable("the input method could not be selected for the app (%s): the app may not have registered a text input with it, or the "
                           "daemon did not start (run/desk/ime_daemon.log on Linux). Nothing was typed through it; nothing is judged" % sw.unproven)
            return
        if raw:
            INTL.compose_raw(ctx, r, "ime_%s_raw" % PINYIN[0][0], keys(PINYIN[0][0]), commit, PINYIN[0][1].encode("utf-8"), sw=sw)
        else:
            r.note("raw reader half NOT run here: bytes_reader.py needs termios (Windows) or python3 on the pane's PATH")
        for word, text in PINYIN:
            INTL.compose_editor(ctx, r, "ime_" + word, keys(word), commit, text, sw=sw)
        if sw.unproven:
            r.unverifiable("a later input-method switch was not proven (%s): the rows above may have been typed on the wrong layout" % sw.unproven)
        r.check(ctx.app.alive(), "the app died in the input method case")
    finally:
        sw.restore()
    ctx.reset()
    U.exact_burst(ctx, r, "ime_after", 3)
