# appci/cases_agent_netgate.py (#1830): the network gate under the agent, in the real app (docs/testing.md "Agent network gate cases"). Registered by one import line at the end of cases.py.
# HOW A GATE-CHECKED REQUEST IS MADE (round 10, docs/design/r10-wl-gate.md): through the app's MARKDOWN VIEWER. show_image() writes a small .md whose only content is a
#   heading and `![img](<url>)` on its own line and hands it to the running app the way the OS does (cases.open_file_in_app -> uri::open_file -> a viewer tab). The viewer
#   paints the image block, the asset cache fetches the URL, and `asset_cache::fetch_file_to_memory` asks the gate (purpose remote-asset, allowlist only). The viewer tab is
#   closed again with the close-pane key (close_viewer), only after the app log showed the tab opening.
#   Rounds 6-9 asked the fake model for a markdown image instead. That never made a request: an http(s) image in AGENT OUTPUT is not loaded at all
#   (ai/blocklist/block/view_impl/common.rs is_supported_blocklist_image_source: "HTTP / HTTPS sources are intentionally rejected", the alt text is shown), so the gate was
#   never asked and every case failed at its first step. agent_gate_deny_allow now asserts that fact too (step 0: the model's image reaches no listener and no gate line).
#   agent_gate_deny_allow         a viewer image names `localhost` + a port where a LOCAL LISTENER waits: a loopback NAME is refused unless configured, so the gate must refuse,
#                                 the denial must reach the
#                                 app log as `network gate: denied "remote-asset" (host hash <fnv>)` (never the host, port or URL), and the listener must have got NOTHING. Then the Settings > Network
#                                 page is driven BY KEYBOARD (#1830: Tab enters its focus ring, Enter/Space/Delete act): Allow on the log row, Remove on the origin, typing an origin + Enter; each
#                                 asserted by the app's `hop=network_gate_ui action=... origin_hash=<fnv>` line, settings.toml and the listener (reached / silent). Then the same edits as live
#                                 settings.toml edits (reload without a restart), and a POINTER pass (OCR + click: the toast's Allow, the page's Remove / Allow / editor + Add) with the same oracles.
#   agent_gate_off_live           the gate switch (network.gate_enabled) flipped in settings.toml with the app running: OFF logs `network gate: turned OFF` once and the request reaches the listener; ON logs
#                                 `turned ON` and the same request is refused again with the listener silent. Then the page's switch is CLICKED (OCR for its row) and `hop=network_gate_ui action=toggle_gate` judged.
#   agent_gate_key_never_leaves   a fake provider key (an X-Appci-Key extra header on the fake-model provider: the key store is not scriptable, a provider header travels the same way) is configured. The fake model must
#                                 receive it (precondition; without it the case is noref). Then images are requested from a denied origin, an allowed origin, an origin that REDIRECTS to a denied one,
#                                 and with the gate off: no capturing listener may ever see the sentinel (header, path or body) or any Authorization header, the redirect target gets nothing, and the app log
#                                 never contains the sentinel, a host, a port or an image URL on a gate line. Also asserts the local-only log property: gate lines carry host hashes only.
# Network oracle (all three cases): the app runs under `strace -f -e trace=connect,sendto,sendmsg`; every destination must be loopback or a unix socket (NetOracle,
#   non_local_destinations). No strace -> that step is noref (unverifiable), never a pass.
# Linux only (settings.toml edits of the Linux profile + pointer + OCR); winmap lists the reason. Mac/Windows: not applicable until the same relaunch/pointer paths exist there.
import hashlib, json, os, re, socket, sys, threading, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cases  # noqa: E402  (cases.py imports this module as its last line)
from cases import case  # noqa: E402
import cases_robust as R  # noqa: E402

PNG = bytes.fromhex("89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000d49444154789c6360000002000001e221bc330000000049454e44ae426082")
SENTINEL_DEFAULT = "appci-gate-key-sentinel-7c41"


class Capture:
    """a loopback HTTP listener (127.0.0.1, and ::1 on the same port when it can bind) that records every request: method, path, lower-cased headers, body. `redirect_to` makes every
    request answer 302 to that URL; otherwise a 1x1 PNG. Used as the 'origin' the gate must keep the app away from."""
    def __init__(self, redirect_to=None):
        self.hits, self.redirect_to, self.servers = [], redirect_to, []
        owner = self

        class H(BaseHTTPRequestHandler):
            def log_message(self, *a):
                pass

            def _do(self):
                n = int(self.headers.get("Content-Length") or 0)
                body = self.rfile.read(n) if n else b""
                owner.hits.append(dict(method=self.command, path=self.path, headers={k.lower(): v for k, v in self.headers.items()}, body=body))
                if owner.redirect_to:
                    self.send_response(302)
                    self.send_header("Location", owner.redirect_to + self.path)
                    self.send_header("Content-Length", "0")
                    self.end_headers()
                    return
                self.send_response(200)
                self.send_header("Content-Type", "image/png")
                self.send_header("Content-Length", str(len(PNG)))
                self.end_headers()
                self.wfile.write(PNG)
            do_GET = do_POST = do_HEAD = _do

        s4 = ThreadingHTTPServer(("127.0.0.1", 0), H)
        self.port = s4.server_address[1]
        self.servers.append(s4)
        try:
            class H6(ThreadingHTTPServer):
                address_family = socket.AF_INET6
            self.servers.append(H6(("::1", self.port), H))
        except OSError:
            pass                                             # no IPv6 loopback: `localhost` then resolves to 127.0.0.1 alone
        for s in self.servers:
            threading.Thread(target=s.serve_forever, daemon=True).start()

    def stop(self):
        for s in self.servers:
            s.shutdown()
            s.server_close()

    def paths(self):
        return [h["path"] for h in self.hits]

    def saw(self, needle):
        return any(needle in h["path"] for h in self.hits)


def leaks(hits, sentinel):
    """pure: the reasons a list of captured requests carries the provider key: the sentinel anywhere (header name/value, path, body) or an Authorization / x-api-key header"""
    out = []
    for h in hits:
        blob = json.dumps(h["headers"]) + h["path"] + h["body"].decode("latin-1")
        if sentinel in blob:
            out.append("%s %s carries the sentinel" % (h["method"], h["path"]))
        for k in ("authorization", "x-api-key", "proxy-authorization"):
            if k in h["headers"]:
                out.append("%s %s carries a %s header" % (h["method"], h["path"], k))
    return out


def fnv(host):
    h = 0xcbf29ce484222325
    for b in host.lower().encode():
        h = ((h ^ b) * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return "%016x" % h


def denied_line(host="localhost"):
    return 'network gate: denied "remote-asset" (host hash %s)' % fnv(host)


def gate_lines(log):
    return [l for l in log.splitlines() if "network gate" in l]


HEX16 = re.compile(r"\b[0-9a-f]{16}\b")


def host_leaks(log, needles):
    """pure: the gate lines of an app log that name any of `needles` (host, port, token, sentinel): a gate line carries a purpose and a host hash only. The 16-digit hex
    hashes themselves (host hash, asset key fingerprint) are taken out before the search: a 5-digit port can occur inside one by chance, and a hash names nothing."""
    return [l for l in gate_lines(log) if any(n and n in HEX16.sub("", l) for n in needles)]


def edit_setting(ctx, table, key, literal):
    """read-modify-write of the profile's settings.toml WHILE THE APP RUNS (settings reload live, ui_settings_live)"""
    p = R.settings_file(ctx)
    t = open(p, encoding="utf-8", errors="replace").read()
    open(p, "w", encoding="utf-8").write(R.T.set_key(t, table, key, literal))


def setting(ctx, table, key):
    """the literal after `key =` in settings.toml, or None"""
    span = R.T.get_key(open(R.settings_file(ctx), encoding="utf-8", errors="replace").read(), table, key)
    return span.split("=", 1)[1].strip() if span and "=" in span else None


VIEWER_OPENED = 'dispatching action for "root_view:add_file_pane"'      # warpui_core App::dispatch_action (Info), from uri::open_file for a file the OS handed over
IMAGE_DOC_WORDS = ("allow", "remove", "add", "network", "gate", "example", "clear")   # words the pointer steps look for by OCR: the document and its name must not show them


def image_doc(url, name_tok):
    """pure: (file name, markdown text) of the document that makes the viewer request `url`. The image is on its OWN line (the markdown parser only makes an image block
    from such a line). The file name carries `name_tok` only, never the URL's token, host or port: the app may log the path of a file it opens, and the cases assert that
    the log never shows the host, port or URL token of a gate-checked request."""
    return "gimg_%s.md" % name_tok, "# GIMG %s\n\n![img](%s)\n" % (name_tok.upper(), url)


def viewer_opened(log):
    """pure: whether an app-log excerpt shows the app taking a handed-over file into a viewer tab"""
    return VIEWER_OPENED in log


def show_image(ctx, r, tag, url_for_tok):
    """make the app request an image URL through the network gate: open a one-image markdown file in the app's markdown viewer (see the module header). Returns
    (log offset before, URL token, opened). Every image gets a fresh token so the asset cache never answers. `opened` is whether the app log showed the viewer tab opening;
    the caller passes it to close_viewer() once it has read its oracle."""
    off = ctx.log_size()
    tok = ctx.tok(6).lower()
    name, text = image_doc(url_for_tok(tok, tag), ctx.tok(6).lower())
    path = os.path.join(ctx.W, name)
    with open(path, "w") as f:
        f.write(text)
    err = cases.open_file_in_app(ctx, path)
    opened = err is None and bool(ctx.until(lambda: viewer_opened(ctx.log_since(off)), 15, step=0.3))
    r.check(opened, "image %s: the markdown file was not opened in the app's viewer (%s), so no gate-checked request was made"
            % (tag, err or "no '%s' line in the app log within 15 s" % VIEWER_OPENED))
    return off, tok, opened


def close_viewer(ctx, opened):
    """close the viewer tab show_image() opened (the close-pane key acts on the tab in front, which is the tab just added). Nothing is pressed when the open was not seen in
    the app log: the key would then close whatever else is in front."""
    if opened and R.CLOSE_PANE:
        ctx.be.key(R.CLOSE_PANE)
        time.sleep(0.8)


def ask_model_image(ctx, url):
    """the agent turn of rounds 6-9: the fake model answers with a markdown image of the URL (fake_model.py IMG_RE). Agent output does not load http(s) images at all."""
    import cases_caseagent as AG
    AG.type_fast(ctx, "Please show this image: " + url)


def settle(ctx, off, line, secs=12, want=True):
    """wait up to `secs` for `line` in the log since `off` (want) or, when not wanting it, for that long as a quiet period; returns whether it is there"""
    if want:
        ctx.until(lambda: line in ctx.log_since(off), secs, step=0.4)
    else:
        time.sleep(secs)
    return line in ctx.log_since(off)


# ---------------------------------------------------------------- whole-case network oracle (strace)
FAMILY = re.compile(r"sa_family=(AF_\w+)")
V4 = re.compile(r'inet_addr\("([^"]+)"\)')
V6 = re.compile(r'inet_pton\(AF_INET6, "([^"]+)"')
LOOPBACK = re.compile(r"^(127\.|0\.0\.0\.0$|::1$|::$|::ffff:127\.)")


def non_local_destinations(text):
    """pure: [(syscall, family, address)] for every connect/sendto/sendmsg line of an `strace -f` text whose destination is neither loopback (127.*, ::1, ::ffff:127.*, unspecified) nor a unix
    socket / netlink / AF_UNSPEC. An inet line whose address cannot be read is reported as '?' (never silently passed). Lines without a destination (send on a connected socket) have none to judge."""
    out = []
    for line in text.splitlines():
        m = re.search(r"\b(connect|sendto|sendmsg)\(", line)
        if not m:
            continue
        fam = FAMILY.search(line)
        if not fam or fam.group(1) in ("AF_UNIX", "AF_LOCAL", "AF_NETLINK", "AF_UNSPEC"):
            continue
        a = V4.search(line) or V6.search(line)
        addr = a.group(1) if a else "?"
        if fam.group(1) in ("AF_INET", "AF_INET6") and a and LOOPBACK.match(addr):
            continue
        out.append((m.group(1), fam.group(1), addr))
    return out


class NetOracle:
    """runs the app under strace for the span begin()..end() and judges every destination at end(). begin() always (re)starts the app so the caller has a running app either way."""
    def __init__(self, label):
        self.label, self.on, self.trace = label, False, None

    def begin(self, ctx, r):
        import cases_netless as NL
        q = R.stop_for_restart(ctx, r, 20, self.label)
        r.check(q["exited"], "%s: the app did not exit within 20 s" % self.label)
        if NL.strace_works():
            self.trace = os.path.join(getattr(ctx, "run", None) or ctx.W, "netgate_%s.strace" % self.label)
            try:
                os.remove(self.trace)
            except OSError:
                pass
            ctx.app.wrap = "strace -f -qq -e trace=connect,sendto,sendmsg -s 48 -o '%s' " % self.trace
            self.on = True
        else:
            r.unverifiable("noref: strace is missing or ptrace is not permitted here, so 'nothing leaves except to loopback / allowed origins' (whole-case network oracle) is NOT judged")
        _, up = R.start_app(ctx, r, self.label + "_net")
        return up

    def end(self, ctx, r):
        if not self.on:
            return
        self.on = False
        ctx.app.wrap = ""
        ctx.app.stop()
        text = open(self.trace, errors="replace").read() if self.trace and os.path.exists(self.trace) else ""
        r.check(bool(text), "%s: the network trace is empty: strace did not follow the app" % self.label)
        bad = non_local_destinations(text)
        r.metric("%s_syscalls_traced" % self.label, len(text.splitlines()))
        r.check(not bad, "%s: the app addressed a non-loopback destination during the case: %s" % (self.label, "; ".join("%s %s %s" % b for b in bad[:8])))
        R.start_app(ctx, r, self.label + "_restore")


# ---------------------------------------------------------------- driving Settings > Network
def hop_line(action, origin_host):
    return "hop=network_gate_ui action=%s origin_hash=%s" % (action, fnv(origin_host))


def open_network_kb(ctx, r):
    """settings from its shortcut, then the command palette entry 'Settings: Network: focus allowed origins' puts the page's focus ring on the origin editor"""
    import cases_coverage_b as CB
    ctx.reset()
    time.sleep(0.3)
    ctx.be.key(CB.SETTINGS_KEY["mac" if ctx.be.name == "mac" else "linux"])      # round 15: network_gate_switch also runs on macOS (cmd+,)
    time.sleep(1.5)
    ctx.be.key(os.environ.get("APPCI_KEY_PALETTE", "ctrl+shift+p"))
    time.sleep(0.8)
    ctx.be.type("focus allowed origins", 12)
    time.sleep(0.8)
    ctx.be.key("Return")
    time.sleep(1.2)
    ctx.shot("agnet_kb_open")


def open_network_pointer(ctx, r, m):
    """settings from its shortcut, then a click on 'Network' in the sidebar (OCR: the leftmost word)"""
    import cases_coverage_b as CB
    import cases_ui as UI
    ctx.reset()
    time.sleep(0.3)
    ctx.be.key(CB.SETTINGS_KEY["linux"])
    time.sleep(1.5)
    ws, _ = UI.words(ctx, "agnet_ptr_side")
    side = sorted([w for w in ws if w[0].strip().lower() == "network"], key=lambda w: w[3])
    if not side:
        return False
    m.click(side[0][1], side[0][2])
    time.sleep(1.2)
    return True


def close_settings(ctx):
    ctx.be.key("Escape")
    time.sleep(0.5)
    ctx.be.key("Escape")
    time.sleep(0.6)
    ctx.reset()
    time.sleep(0.3)


TOAST_LOOK_S = 3.0          # how long the denial toast is looked for after the denial is in the log; the toast is up for about 4 s


def find_toast_word(ctx, tag, sub, secs=TOAST_LOOK_S, find=None):
    """the denial toast's word (e.g. 'allow'), looked for by OCR until it is on screen or `secs` have passed -> (word or None, screenshot path).
    Round 10 (Linux Wayland; #2027): the toast is drawn a frame or two after the gate logs the denial and is gone about 4 s later. agent_gate_deny_allow
    looked ONCE, right at the log line (shots/agdn_find_allow: no toast yet), and network_gate_notice once after a 4 s sleep (ngn_denied shows the toast,
    ngn_find_allow a moment later does not): both ended UNVERIFIABLE and the pointer Allow was never judged. `find` is cases_ui.find_word (a seam for the
    self-test)."""
    if find is None:
        import cases_ui as UI
        find = UI.find_word
    t_end = time.time() + secs
    w, p = find(ctx, tag, sub)
    while not w and time.time() < t_end:
        time.sleep(0.2)
        w, p = find(ctx, tag, sub)
    return w, p


def click_exact(ctx, m, tag, text, pick="top"):
    """click the word whose whole text is `text` (case-insensitive); 'top' = highest on screen, 'bottom' = lowest. Returns whether one was found."""
    import cases_ui as UI
    ws, _ = UI.words(ctx, tag)
    hits = sorted([w for w in ws if w[0].strip(".,:;").lower() == text.lower()], key=lambda w: w[2])
    if not hits:
        return False
    w = hits[0] if pick == "top" else hits[-1]
    m.click(w[1], w[2])
    return True


def expect_reach(ctx, r, cap, url, tag, why):
    off, tok, opened = show_image(ctx, r, tag, url)
    r.check(ctx.until(lambda: cap.saw(tok), 30, step=0.5), "%s: the request did not reach the listener (paths %s)" % (why, cap.paths()[-3:]))
    r.check(denied_line() not in ctx.log_since(off), "%s: the gate refused it" % why)
    close_viewer(ctx, opened)


def expect_denied(ctx, r, cap, url, tag, why):
    n = len(cap.hits)
    off, tok, opened = show_image(ctx, r, tag, url)
    r.check(settle(ctx, off, denied_line(), 30), "%s: the gate did not refuse it (no '%s')" % (why, denied_line()))
    time.sleep(3.0)
    r.check(len(cap.hits) == n, "%s: the listener received a request (%s)" % (why, cap.paths()[n:]))
    close_viewer(ctx, opened)


def origin_count(ctx, origin):
    """how often the settings file the RUNNING app writes holds `origin` (Linux: the profile's file; macOS / Windows: the app's own copy)"""
    return open(R.live_settings_file(ctx), encoding="utf-8", errors="replace").read().count(origin)


def expect_hop(ctx, r, off, action, why, host="localhost"):
    ok = ctx.until(lambda: hop_line(action, host) in ctx.log_since(off), 10, step=0.3)
    r.check(ok, "%s: the app log has no '%s' (the page action did not run)" % (why, hop_line(action, host)))
    return ok


SWITCH_DX = (720, 700, 740, 680, 760)        # from the row label's word "gate" to the switch at the row's right edge (1600 px wide window: label at x~705, switch at x~1425)
ADD_DX = (280, 260, 300)                     # from the origin editor's placeholder to the Add button on the same row (placeholder at x~1146, button at x~1425)


def switch_spots(ws):
    """pure: the (x, y) points to click for the Settings > Network gate switch, best first, from the OCR words of the page. The page shows 'Network gate' TWICE: the section
    heading, and below it the switch row's label; the switch is at the right end of the LABEL row (rounds 8/9 clicked right of the heading, the top-most 'gate', and
    toggled nothing). 'Gate log' further down is not a candidate (no 'network' before it on its line). The right edge is where the 'Add' button is, when OCR read it."""
    low = [(w[0].strip(".,:;").lower(),) + tuple(w[1:]) for w in ws]
    rows = sorted([w for w in low if w[0] == "gate" and any(o[0] == "network" and abs(o[2] - w[2]) <= 10 and o[1] < w[1] for o in low)], key=lambda w: w[2])
    if not rows:
        return []
    order = ([rows[1]] if len(rows) > 1 else []) + [rows[0]]
    add = sorted([w for w in low if w[0] == "add"], key=lambda w: w[2])
    out = []
    for g in order:
        xs = ([add[0][1]] if add and add[0][1] > g[1] else []) + [g[1] + dx for dx in SWITCH_DX]
        out += [(x, g[2]) for x in xs if (x, g[2]) not in out]
    return out


def pointer_gate_switch(ctx, r, cap, url):
    """click the Settings > Network gate switch (located from its row label by OCR; the switch is to the right of the label, so a few x offsets are tried and each is judged by the
    app's own `hop=network_gate_ui action=toggle_gate` line) -> gate off: the request reaches the listener; click again -> on: refused."""
    import cases_ui as UI
    m = UI.mouse_or_skip(ctx, r)
    UI.need_tesseract(r)
    off = ctx.log_size()
    if not open_network_pointer(ctx, r, m):
        r.unverifiable("gate switch by pointer: the Network page was not found by OCR (shots/agnet_ptr_side.png)")
        return
    ws, _ = UI.words(ctx, "agoff_ptr_switch")
    spots = switch_spots(ws)
    if not spots:
        r.unverifiable("gate switch by pointer: the 'Network gate' row label was not found by OCR (shots/agoff_ptr_switch.png)")
        close_settings(ctx)
        return
    spot = None
    for x, y in spots:
        m.click(x, y)
        if ctx.until(lambda: "hop=network_gate_ui action=toggle_gate" in ctx.log_since(off), 1.5, step=0.2):
            spot = (x, y)
            break
    if spot is None:
        r.unverifiable("gate switch by pointer: no click on the 'Network gate' row toggled it (no toggle_gate hop; tried %s; shots/agoff_ptr_switch.png)" % spots)
        close_settings(ctx)
        return
    r.check("hop=network_gate_ui action=toggle_gate on=false" in ctx.log_since(off), "the switch click did not turn the gate OFF (hop on=false missing)")
    r.check(settle(ctx, off, "network gate: turned OFF", 10), "the switch click did not log 'network gate: turned OFF'")
    r.check(setting(ctx, "network", "gate_enabled") == "false", "the switch click did not write network.gate_enabled = false")
    close_settings(ctx)
    expect_reach(ctx, r, cap, url, "p", "with the gate switched off by pointer")
    on = ctx.log_size()
    if open_network_pointer(ctx, r, m):
        m.click(spot[0], spot[1])
        time.sleep(0.8)
        r.check("hop=network_gate_ui action=toggle_gate on=true" in ctx.log_since(on), "the second switch click did not turn the gate back ON (hop on=true missing)")
    close_settings(ctx)
    expect_denied(ctx, r, cap, url, "q", "with the gate switched back on by pointer")


NOT_PORTED_WHY = ("agent_gate: Linux only. NEEDS (harness, not built): on macOS a pointer tool (r14-test-gaps T32) for the toast and the page switch, a close key "
                  "for the viewer tab, and the polled trace (nettrace.py) in place of the strace oracle; on Windows the pointer (T31) and the same two")


def prelude(ctx, r):
    import cases_replay as CRP
    R.need_linux_app(ctx, NOT_PORTED_WHY)
    CRP.need_model(ctx, r)


def restore_settings(ctx, saved):
    try:
        p = R.settings_file(ctx)
        t = open(p, encoding="utf-8", errors="replace").read()
        t = R.T.set_key(t, "network", "gate_enabled", "true")
        t = R.T.set_key(t, "network", "allowed_origins", '""')
        for (tbl, key), lit in saved.items():
            if lit is not None:
                t = R.T.set_key(t, tbl, key, lit)
        open(p, "w", encoding="utf-8").write(t)
    except Exception:
        pass


@case("agent_gate_deny_allow", modes=("agent",), order=95, gui=True)
def c_agent_gate_deny_allow(ctx, r):
    """see the module header: a denied local origin gets nothing; the Network page's own controls (keyboard, then pointer) and live origin edits change that without a restart"""
    import cases_ui as UI
    prelude(ctx, r)
    m = UI.mouse_or_skip(ctx, r)
    UI.need_tesseract(r)
    cap = Capture()
    origin = "localhost:%d" % cap.port
    url = lambda tok, tag: "http://%s/%s_%s.png" % (origin, tag, tok)
    saved = {}
    oracle = NetOracle("agdn")
    try:
        r.check((setting(ctx, "network", "allowed_origins") or '""') in ('""', "''"), "precondition: network.allowed_origins is not empty before the case")
        oracle.begin(ctx, r)
        # 0. AGENT OUTPUT never loads a remote image (by design, see the module header): the fake model's markdown image makes no request and asks the gate nothing
        import fake_model as FM
        off0, mtok = ctx.log_size(), ctx.tok(6).lower()
        murl = url(mtok, "m")
        ask_model_image(ctx, murl)
        fake = FM.ACTIVE
        answered = bool(fake is not None and ctx.until(lambda: any(murl in t for t in fake.fresh), 30, step=0.5))
        time.sleep(4.0)                                                # the reply is a few bytes; a fetch, if one were made, would start as soon as it is drawn
        if answered:
            r.check(not cap.hits, "an http image in the AGENT'S ANSWER was requested (%s): agent output must not load remote images" % cap.paths()[:3])
            r.check('"remote-asset"' not in ctx.log_since(off0), "an http image in the agent's answer reached the network gate (agent output must not load remote images at all)")
        else:
            r.unverifiable("the scripted model did not receive the 'show this image' turn within 30 s: 'agent output loads no remote image' is not judged")
        # 1. denied: the log has the hashed line, the listener nothing
        off, tok, opened = show_image(ctx, r, "a", url)
        got = settle(ctx, off, denied_line(), 30)
        r.check(got, "the image from the unlisted loopback NAME %s was not refused (no '%s' line)" % ("localhost", denied_line()))
        time.sleep(3.0)                                                # a refused request must stay refused: give a late connection time to show up
        r.check(not cap.hits, "the listener on the denied origin received %d request(s) (%s): the request left the app" % (len(cap.hits), cap.paths()[:3]))
        log = ctx.log_since(off)
        r.check(not host_leaks(log, ["localhost", str(cap.port), tok]), "a gate line in the app log names the host, port or URL: %s" % host_leaks(log, ["localhost", str(cap.port), tok])[:2])
        ctx.shot("agdn_denied")
        close_viewer(ctx, opened)
        # 2. KEYBOARD: Allow on the page's log row (Tab from the editor lands on it: no origins are listed)
        off = ctx.log_size()
        open_network_kb(ctx, r)
        ctx.be.key("Tab")
        time.sleep(0.5)
        ctx.shot("agdn_kb_allow_focus")
        ctx.be.key("Return")
        time.sleep(0.8)
        expect_hop(ctx, r, off, "allow_from_log", "keyboard Allow on the log row")
        r.check(origin_count(ctx, origin) == 1, "keyboard Allow: settings.toml holds %s %d times (expected once)" % (origin, origin_count(ctx, origin)))
        close_settings(ctx)
        expect_reach(ctx, r, cap, url, "b", "after the keyboard Allow")
        # 3. KEYBOARD: Remove (Tab from the editor lands on the origin's Remove; Delete removes it)
        off = ctx.log_size()
        open_network_kb(ctx, r)
        ctx.be.key("Tab")
        time.sleep(0.5)
        ctx.shot("agdn_kb_remove_focus")
        ctx.be.key("Delete")
        time.sleep(0.8)
        expect_hop(ctx, r, off, "remove_origin", "keyboard Remove")
        r.check(origin_count(ctx, origin) == 0, "keyboard Remove: settings.toml still holds %s" % origin)
        close_settings(ctx)
        expect_denied(ctx, r, cap, url, "c", "after the keyboard Remove")
        # 4. KEYBOARD: type an origin into the editor + Enter
        off = ctx.log_size()
        open_network_kb(ctx, r)
        ctx.be.type(origin, 12)
        time.sleep(0.3)
        ctx.be.key("Return")
        time.sleep(0.8)
        expect_hop(ctx, r, off, "add_origin", "keyboard add (typed origin + Enter)")
        r.check(origin_count(ctx, origin) == 1, "keyboard add: settings.toml holds %s %d times (expected once)" % (origin, origin_count(ctx, origin)))
        close_settings(ctx)
        expect_reach(ctx, r, cap, url, "d", "after the keyboard add")
        # 5. live file edits (settings reload without a restart): removed -> refused, added back -> reaches
        edit_setting(ctx, "network", "allowed_origins", '""')
        time.sleep(2.0)
        expect_denied(ctx, r, cap, url, "e", "removing the origin from settings.toml with the app running")
        edit_setting(ctx, "network", "allowed_origins", '"%s"' % origin)
        time.sleep(2.0)
        expect_reach(ctx, r, cap, url, "f", "adding the origin to settings.toml with the app running")
        # 6. POINTER pass. 6a: the toast's Allow (the origin is removed live first so a fresh denial raises the toast)
        edit_setting(ctx, "network", "allowed_origins", '""')
        time.sleep(2.0)
        off, tok, opened = show_image(ctx, r, "g", url)
        r.check(settle(ctx, off, denied_line(), 30), "pointer pass: the origin removed live was not refused")
        w, _ = find_toast_word(ctx, "agdn_find_allow", "allow")
        if w:
            m.click(w[1], w[2])
            ctx.until(lambda: origin_count(ctx, origin) >= 1, 10, step=0.5)
            r.check(origin_count(ctx, origin) == 1, "after the toast's Allow, settings.toml holds %s %d times (expected once)" % (origin, origin_count(ctx, origin)))
            close_viewer(ctx, opened)
            expect_reach(ctx, r, cap, url, "h", "after the toast's Allow click")
        else:
            r.unverifiable("the toast's 'Allow' was not found by OCR (shots/agdn_find_allow.png): the click is not judged")
            close_viewer(ctx, opened)
            edit_setting(ctx, "network", "allowed_origins", '"%s"' % origin)
        # 6b. pointer Remove on the page
        off = ctx.log_size()
        if open_network_pointer(ctx, r, m) and click_exact(ctx, m, "agdn_ptr_remove", "remove"):
            time.sleep(0.8)
            expect_hop(ctx, r, off, "remove_origin", "pointer Remove")
            r.check(origin_count(ctx, origin) == 0, "pointer Remove: settings.toml still holds %s" % origin)
            close_settings(ctx)
            expect_denied(ctx, r, cap, url, "i", "after the pointer Remove")
            # 6c. pointer Allow on a denied log row
            off = ctx.log_size()
            if open_network_pointer(ctx, r, m) and click_exact(ctx, m, "agdn_ptr_allow", "allow", pick="bottom"):
                time.sleep(0.8)
                expect_hop(ctx, r, off, "allow_from_log", "pointer Allow on the log row")
                r.check(origin_count(ctx, origin) == 1, "pointer Allow: settings.toml holds %s %d times (expected once)" % (origin, origin_count(ctx, origin)))
                close_settings(ctx)
                expect_reach(ctx, r, cap, url, "j", "after the pointer Allow")
            else:
                r.unverifiable("pointer Allow: the page or its Allow button was not found by OCR (shots/agdn_ptr_allow.png)")
                close_settings(ctx)
        else:
            r.unverifiable("pointer Remove: the Network page or its Remove button was not found by OCR (shots/agdn_ptr_remove.png, agnet_ptr_side.png)")
            close_settings(ctx)
        # 6d. pointer add: remove first (file edit), then click the editor, type, click Add
        edit_setting(ctx, "network", "allowed_origins", '""')
        time.sleep(2.0)
        off = ctx.log_size()
        if open_network_pointer(ctx, r, m):
            ws, _ = UI.words(ctx, "agdn_ptr_editor")
            ed = [w for w in ws if "example" in w[0].lower()]
            if ed:
                m.click(ed[0][1], ed[0][2])
                time.sleep(0.3)
                ctx.be.type(origin, 12)
                time.sleep(0.3)
                clicked = click_exact(ctx, m, "agdn_ptr_add", "add", pick="top")
                if not clicked:
                    # rounds 8/9: tesseract did not read the button's white-on-blue label. The button is on the editor's row, right of it: click there, and
                    # count the click only when the app's own add_origin hop shows it landed on the button
                    for dx in ADD_DX:
                        m.click(ed[0][1] + dx, ed[0][2])
                        if ctx.until(lambda: hop_line("add_origin", "localhost") in ctx.log_since(off), 1.5, step=0.2):
                            clicked = True
                            break
                if clicked:
                    time.sleep(0.8)
                    expect_hop(ctx, r, off, "add_origin", "pointer add (editor + Add)")
                    r.check(origin_count(ctx, origin) == 1, "pointer add: settings.toml holds %s %d times (expected once)" % (origin, origin_count(ctx, origin)))
                    close_settings(ctx)
                    expect_reach(ctx, r, cap, url, "k", "after the pointer add")
                else:
                    r.unverifiable("pointer add: the Add button was not found by OCR and no click right of the editor added the origin (shots/agdn_ptr_add.png)")
            else:
                r.unverifiable("pointer add: the origin editor's placeholder was not found by OCR (shots/agdn_ptr_editor.png)")
        close_settings(ctx)
        r.check(ctx.app.alive(), "the app exited during the case")
    finally:
        restore_settings(ctx, saved)
        oracle.end(ctx, r)
        cap.stop()


@case("agent_gate_off_live", modes=("agent",), order=96, gui=True)
def c_agent_gate_off_live(ctx, r):
    """network.gate_enabled flipped live: OFF passes the request (logged once), ON refuses it again"""
    prelude(ctx, r)
    cap = Capture()
    origin = "localhost:%d" % cap.port
    url = lambda tok, tag: "http://%s/%s_%s.png" % (origin, tag, tok)
    oracle = NetOracle("agoff")
    try:
        oracle.begin(ctx, r)
        off, tok, opened = show_image(ctx, r, "a", url)
        r.check(settle(ctx, off, denied_line(), 30), "precondition: the gate (on) did not refuse the unlisted origin")
        time.sleep(2.0)
        r.check(not cap.hits, "the listener received a request while the gate was on and the origin unlisted")
        close_viewer(ctx, opened)
        off_log = ctx.log_size()
        edit_setting(ctx, "network", "gate_enabled", "false")
        r.check(settle(ctx, off_log, "network gate: turned OFF", 20), "gate_enabled=false written with the app running did not log 'network gate: turned OFF' (not live?)")
        r.check(ctx.log_since(off_log).count("network gate: turned OFF") == 1, "the OFF line was logged more than once")
        off, tok, opened = show_image(ctx, r, "b", url)
        r.check(ctx.until(lambda: cap.saw(tok), 30, step=0.5), "with the gate off the request did not reach the listener")
        r.check(denied_line() not in ctx.log_since(off), "the gate is off but the app log shows a denial")
        close_viewer(ctx, opened)
        on_log = ctx.log_size()
        edit_setting(ctx, "network", "gate_enabled", "true")
        r.check(settle(ctx, on_log, "network gate: turned ON", 20), "gate_enabled=true written with the app running did not log 'network gate: turned ON'")
        n = len(cap.hits)
        off, tok, opened = show_image(ctx, r, "c", url)
        r.check(settle(ctx, off, denied_line(), 30), "with the gate back on the request was not refused again")
        time.sleep(3.0)
        r.check(len(cap.hits) == n, "the listener received a request after the gate was turned back on")
        close_viewer(ctx, opened)
        log = ctx.log_since(0)
        r.check(not host_leaks(log, ["localhost", str(cap.port)]), "a gate line in the app log names the host or port")
        # the page's own switch, by pointer: OFF -> request reaches, ON -> refused (the switch sits in the row labelled 'Network gate')
        pointer_gate_switch(ctx, r, cap, url)
        r.note("never uploaded: the whole-case strace oracle (NetOracle) judges every connect/sendto/sendmsg destination of this case")
    finally:
        restore_settings(ctx, {})
        oracle.end(ctx, r)
        cap.stop()


@case("agent_gate_key_never_leaves", modes=("agent",), order=97, gui=True)
def c_agent_gate_key_never_leaves(ctx, r):
    """the provider key (sentinel extra header on the fake-model provider) reaches the provider and no other origin: denied, allowed, redirecting and gate-off listeners never see it"""
    import fake_model as FM
    prelude(ctx, r)
    fake = FM.ACTIVE
    if fake is None:
        r.skip("the fake model object is not in this process (APPCI_FAKE_MODEL=1 starts it in run.py)")
    sentinel = os.environ.get("APPCI_GATE_SENTINEL", SENTINEL_DEFAULT) + "-" + ctx.tok(4).lower()
    prov_key = ("agents.warp_agent", "providers")
    prov_old = setting(ctx, *prov_key)
    if prov_old is None or not prov_old.startswith("["):
        r.unverifiable("settings.toml has no inline agents.warp_agent.providers list to carry the key header (array-of-tables form?): the case cannot configure the fake key")
        return
    saved = {prov_key: prov_old}
    denied, allowed, final = Capture(), Capture(), Capture()
    redir = Capture(redirect_to="http://localhost:%d" % final.port)          # allowed first hop (127.0.0.1 literal) -> redirect to a NAME that is not allowed
    mk = lambda c, host="localhost": (lambda tok, tag: "http://%s:%d/%s_%s.png" % (host, c.port, tag, tok))
    caps = [denied, allowed, final, redir]
    oracle = NetOracle("agkey")
    try:
        provider = ('[ { id = "appci-ollama", name = "CI Ollama", api_type = "ollama", base_url = "%s", models = [ { name = "fake-model", id = "fake-model" } ], '
                    'extra_headers = [ [ "X-Appci-Key", "%s" ] ] } ]' % (fake.url, sentinel))
        edit_setting(ctx, "agents.warp_agent", "providers", provider)
        up = oracle.begin(ctx, r)                                       # graceful stop, then the relaunch (traced when strace works)
        if not up:
            return
        # precondition: the provider receives the key (otherwise 'never elsewhere' proves nothing)
        before = len(fake.headers)
        import cases_caseagent as AG
        AG.type_fast(ctx, "Reply with the word GK%s" % ctx.tok(3))
        ctx.until(lambda: any(sentinel in json.dumps(h) for h in fake.headers[before:]), 40, step=0.5)
        if not any(sentinel in json.dumps(h) for h in fake.headers[before:]):
            ctx.skip_reason = "the provider's key header never reached the fake model"
            r.unverifiable("noref: the fake model received no request carrying the configured key header, so 'the key goes nowhere else' is not judged (extra_headers not sent for this provider type?)")
            return
        r.metric("provider_requests_with_key", sum(1 for h in fake.headers if sentinel in json.dumps(h)))
        # a. denied origin
        off, tok, opened = show_image(ctx, r, "a", mk(denied))
        r.check(settle(ctx, off, denied_line(), 30), "the denied origin was not refused (no '%s')" % denied_line())
        close_viewer(ctx, opened)
        # b. allowed origin (live edit) -> reaches, with no key
        edit_setting(ctx, "network", "allowed_origins", '"localhost:%d"' % allowed.port)
        time.sleep(2.0)
        off, tok, opened = show_image(ctx, r, "b", mk(allowed))
        r.check(ctx.until(lambda: allowed.saw(tok), 30, step=0.5), "the allowed origin did not receive its request (no leak judgement without it)")
        close_viewer(ctx, opened)
        # c. allowed 127.0.0.1 origin that redirects to a name that is NOT allowed
        off, tok, opened = show_image(ctx, r, "c", mk(redir, "127.0.0.1"))
        r.check(ctx.until(lambda: redir.saw(tok), 30, step=0.5), "the redirecting loopback origin did not receive its request")
        time.sleep(4.0)
        r.check(not final.hits, "the redirect target (a denied origin) received %d request(s): a redirect hop skipped the gate (%s)" % (len(final.hits), final.paths()[:3]))
        close_viewer(ctx, opened)
        # d. gate off
        edit_setting(ctx, "network", "gate_enabled", "false")
        off_ok = settle(ctx, 0, "network gate: turned OFF", 20)
        off, tok, opened = show_image(ctx, r, "d", mk(denied))
        r.check(off_ok and ctx.until(lambda: denied.saw(tok), 30, step=0.5), "with the gate off the request did not reach the (previously denied) listener")
        close_viewer(ctx, opened)
        # the safety property, over everything every listener was sent
        for name, c in (("denied", denied), ("allowed", allowed), ("redirect-first-hop", redir), ("redirect-target", final)):
            bad = leaks(c.hits, sentinel)
            r.check(not bad, "the %s origin received the provider key: %s" % (name, bad[:3]))
        r.metric("captured_requests", sum(len(c.hits) for c in caps))
        log = ctx.log_since(0)
        r.check(sentinel not in log, "the app log contains the provider key")
        needles = ["localhost", str(denied.port), str(allowed.port), str(redir.port), str(final.port), sentinel]
        r.check(not host_leaks(log, needles), "a gate line in the app log names a host, port or key: %s" % host_leaks(log, needles)[:2])
        ctx.shot("agkey_done")
    finally:
        restore_settings(ctx, saved)
        oracle.end(ctx, r)
        for c in caps:
            c.stop()
