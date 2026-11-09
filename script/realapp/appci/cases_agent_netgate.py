# appci/cases_agent_netgate.py (#1830): the network gate under the agent, in the real app (docs/testing.md "Agent network gate cases"). Registered by one import line at the end of cases.py.
#   agent_gate_deny_allow         agent mode: the fake model answers "Please show this image: <url>" with a markdown image; the app fetches it through the gate (purpose remote-asset, allowlist
#                                 only). The URL names `localhost` + a port where a LOCAL LISTENER waits: a loopback NAME is refused unless configured, so the gate must refuse, the denial must reach the
#                                 app log as `network gate: denied "remote-asset" (host hash <fnv>)` (never the host, port or URL), and the listener must have got NOTHING. Then: press the toast's Allow
#                                 (OCR + pointer) -> settings.toml holds the origin once and a retry reaches the listener; remove the origin by editing settings.toml with the app RUNNING -> denied again,
#                                 listener silent; add it back live -> reaches (no restart anywhere). The Settings > Network allowed-origins editor itself has no verified keyboard path: reported UNVERIFIABLE
#                                 (the live file edit drives the same setting).
#   agent_gate_off_live           the gate switch (network.gate_enabled) flipped in settings.toml with the app running: OFF logs `network gate: turned OFF` once and the request reaches the listener; ON logs
#                                 `turned ON` and the same request is refused again with the listener silent.
#   agent_gate_key_never_leaves   a fake provider key (an X-Appci-Key extra header on the fake-model provider: the key store is not scriptable, a provider header travels the same way) is configured. The fake model must
#                                 receive it (precondition; without it the case is noref). Then images are requested from a denied origin, an allowed origin, an origin that REDIRECTS to a denied one,
#                                 and with the gate off: no capturing listener may ever see the sentinel (header, path or body) or any Authorization header, the redirect target gets nothing, and the app log
#                                 never contains the sentinel, a host, a port or an image URL on a gate line. Also asserts the local-only log property: gate lines carry host hashes only.
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


def host_leaks(log, needles):
    """pure: the gate lines of an app log that name any of `needles` (host, port, token, sentinel): a gate line carries a purpose and a host hash only"""
    return [l for l in gate_lines(log) if any(n and n in l for n in needles)]


def edit_setting(ctx, table, key, literal):
    """read-modify-write of the profile's settings.toml WHILE THE APP RUNS (settings reload live, ui_settings_live)"""
    p = R.settings_file(ctx)
    t = open(p, encoding="utf-8", errors="replace").read()
    open(p, "w", encoding="utf-8").write(R.T.set_key(t, table, key, literal))


def setting(ctx, table, key):
    """the literal after `key =` in settings.toml, or None"""
    span = R.T.get_key(open(R.settings_file(ctx), encoding="utf-8", errors="replace").read(), table, key)
    return span.split("=", 1)[1].strip() if span and "=" in span else None


def ask_image(ctx, tag, url_for_tok):
    """the agent turn: the fake model answers with a markdown image of the URL. Returns (log offset before, token). Every image gets a fresh token so the asset cache never answers."""
    import cases_caseagent as AG
    off = ctx.log_size()
    tok = ctx.tok(6).lower()
    AG.type_fast(ctx, "Please show this image: " + url_for_tok(tok, tag))
    return off, tok


def settle(ctx, off, line, secs=12, want=True):
    """wait up to `secs` for `line` in the log since `off` (want) or, when not wanting it, for that long as a quiet period; returns whether it is there"""
    if want:
        ctx.until(lambda: line in ctx.log_since(off), secs, step=0.4)
    else:
        time.sleep(secs)
    return line in ctx.log_since(off)


def prelude(ctx, r):
    import cases_replay as CRP
    R.need_linux_app(ctx, "agent_gate")
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
    """see the module header: a denied local origin gets nothing; Allow (toast) and live origin edits change that without a restart"""
    import cases_ui as UI
    prelude(ctx, r)
    m = UI.mouse_or_skip(ctx, r)
    UI.need_tesseract(r)
    cap = Capture()
    origin = "localhost:%d" % cap.port
    url = lambda tok, tag: "http://%s/%s_%s.png" % (origin, tag, tok)
    saved = {}
    try:
        r.check((setting(ctx, "network", "allowed_origins") or '""') in ('""', "''"), "precondition: network.allowed_origins is not empty before the case")
        # 1. denied: the log has the hashed line, the listener nothing
        off, tok = ask_image(ctx, "a", url)
        got = settle(ctx, off, denied_line(), 30)
        r.check(got, "the image from the unlisted loopback NAME %s was not refused (no '%s' line)" % ("localhost", denied_line()))
        time.sleep(3.0)                                                # a refused request must stay refused: give a late connection time to show up
        r.check(not cap.hits, "the listener on the denied origin received %d request(s) (%s): the request left the app" % (len(cap.hits), cap.paths()[:3]))
        log = ctx.log_since(off)
        r.check(not host_leaks(log, ["localhost", str(cap.port), tok]), "a gate line in the app log names the host, port or URL: %s" % host_leaks(log, ["localhost", str(cap.port), tok])[:2])
        ctx.shot("agdn_denied")
        # 2. Allow on the toast
        w, _ = UI.find_word(ctx, "agdn_find_allow", "allow")
        if w:
            m.click(w[1], w[2])
            text = ""
            for _ in range(20):
                time.sleep(0.5)
                text = open(R.settings_file(ctx), encoding="utf-8", errors="replace").read()
                if origin in text:
                    break
            r.check(text.count(origin) == 1, "after Allow, settings.toml holds %s %d times (expected once)" % (origin, text.count(origin)))
        else:
            r.unverifiable("the toast's 'Allow' was not found by OCR (shots/agdn_find_allow.png): the click is not judged; the origin is written to settings.toml directly so the rest still runs")
            edit_setting(ctx, "network", "allowed_origins", '"%s"' % origin)
        off, tok = ask_image(ctx, "b", url)
        ok = ctx.until(lambda: cap.saw(tok), 30, step=0.5)
        r.check(ok, "after Allow the retry did not reach the listener (paths %s)" % cap.paths()[-3:])
        r.check(denied_line() not in ctx.log_since(off), "the allowed origin was still refused after Allow")
        # 3. live edit away: no restart, refused again
        n = len(cap.hits)
        edit_setting(ctx, "network", "allowed_origins", '""')
        time.sleep(2.0)
        off, tok = ask_image(ctx, "c", url)
        r.check(settle(ctx, off, denied_line(), 30), "removing the origin from settings.toml with the app running did not make the gate refuse it again (settings not live?)")
        time.sleep(3.0)
        r.check(len(cap.hits) == n, "the listener received a request after the origin was removed live (%s)" % cap.paths()[n:])
        # 4. live edit back: allowed again
        edit_setting(ctx, "network", "allowed_origins", '"%s"' % origin)
        time.sleep(2.0)
        off, tok = ask_image(ctx, "d", url)
        r.check(ctx.until(lambda: cap.saw(tok), 30, step=0.5), "adding the origin to settings.toml with the app running did not let the request through")
        r.check(ctx.app.alive(), "the app exited during the case")
        r.unverifiable("the Settings > Network allowed-origins editor and the log row's own Allow button: no verified keyboard path to the Network page widgets (the live file edit drives the same "
                       "setting; the page is covered by app/src/settings_view/network_page_tests.rs)")
    finally:
        restore_settings(ctx, saved)
        cap.stop()


@case("agent_gate_off_live", modes=("agent",), order=96, gui=True)
def c_agent_gate_off_live(ctx, r):
    """network.gate_enabled flipped live: OFF passes the request (logged once), ON refuses it again"""
    prelude(ctx, r)
    cap = Capture()
    origin = "localhost:%d" % cap.port
    url = lambda tok, tag: "http://%s/%s_%s.png" % (origin, tag, tok)
    try:
        off, tok = ask_image(ctx, "a", url)
        r.check(settle(ctx, off, denied_line(), 30), "precondition: the gate (on) did not refuse the unlisted origin")
        time.sleep(2.0)
        r.check(not cap.hits, "the listener received a request while the gate was on and the origin unlisted")
        off_log = ctx.log_size()
        edit_setting(ctx, "network", "gate_enabled", "false")
        r.check(settle(ctx, off_log, "network gate: turned OFF", 20), "gate_enabled=false written with the app running did not log 'network gate: turned OFF' (not live?)")
        r.check(ctx.log_since(off_log).count("network gate: turned OFF") == 1, "the OFF line was logged more than once")
        off, tok = ask_image(ctx, "b", url)
        r.check(ctx.until(lambda: cap.saw(tok), 30, step=0.5), "with the gate off the request did not reach the listener")
        r.check(denied_line() not in ctx.log_since(off), "the gate is off but the app log shows a denial")
        on_log = ctx.log_size()
        edit_setting(ctx, "network", "gate_enabled", "true")
        r.check(settle(ctx, on_log, "network gate: turned ON", 20), "gate_enabled=true written with the app running did not log 'network gate: turned ON'")
        n = len(cap.hits)
        off, tok = ask_image(ctx, "c", url)
        r.check(settle(ctx, off, denied_line(), 30), "with the gate back on the request was not refused again")
        time.sleep(3.0)
        r.check(len(cap.hits) == n, "the listener received a request after the gate was turned back on")
        log = ctx.log_since(0)
        r.check(not host_leaks(log, ["localhost", str(cap.port)]), "a gate line in the app log names the host or port")
        r.note("never uploaded: not provable by a listener; zero_network_startup's strace covers the idle app, this case proves only that gate lines hold hashes")
    finally:
        restore_settings(ctx, {})
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
    try:
        provider = ('[ { id = "appci-ollama", name = "CI Ollama", api_type = "ollama", base_url = "%s", models = [ { name = "fake-model", id = "fake-model" } ], '
                    'extra_headers = [ [ "X-Appci-Key", "%s" ] ] } ]' % (fake.url, sentinel))
        edit_setting(ctx, "agents.warp_agent", "providers", provider)
        q = R.stop_for_restart(ctx, r, 20, "agkey")
        r.check(q["exited"], "agkey: the app did not exit within 20 s")
        _, up = R.start_app(ctx, r, "agkey")
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
        off, tok = ask_image(ctx, "a", mk(denied))
        r.check(settle(ctx, off, denied_line(), 30), "the denied origin was not refused (no '%s')" % denied_line())
        # b. allowed origin (live edit) -> reaches, with no key
        edit_setting(ctx, "network", "allowed_origins", '"localhost:%d"' % allowed.port)
        time.sleep(2.0)
        off, tok = ask_image(ctx, "b", mk(allowed))
        r.check(ctx.until(lambda: allowed.saw(tok), 30, step=0.5), "the allowed origin did not receive its request (no leak judgement without it)")
        # c. allowed 127.0.0.1 origin that redirects to a name that is NOT allowed
        off, tok = ask_image(ctx, "c", mk(redir, "127.0.0.1"))
        r.check(ctx.until(lambda: redir.saw(tok), 30, step=0.5), "the redirecting loopback origin did not receive its request")
        time.sleep(4.0)
        r.check(not final.hits, "the redirect target (a denied origin) received %d request(s): a redirect hop skipped the gate (%s)" % (len(final.hits), final.paths()[:3]))
        # d. gate off
        edit_setting(ctx, "network", "gate_enabled", "false")
        off_ok = settle(ctx, 0, "network gate: turned OFF", 20)
        off, tok = ask_image(ctx, "d", mk(denied))
        r.check(off_ok and ctx.until(lambda: denied.saw(tok), 30, step=0.5), "with the gate off the request did not reach the (previously denied) listener")
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
        for c in caps:
            c.stop()
