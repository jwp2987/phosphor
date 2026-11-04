# appci/cases_caseintl.py: group 8 of the new real-app cases (#1466): international input.
#   all backends : intl_wide_type / intl_wide_paste / intl_wide_edit / intl_wide_program  (CJK, accents, emoji, ZWJ sequences typed and pasted,
#                  cursor + BackSpace over wide characters, a running program receiving them; exact UTF-8 bytes checked)
#   Linux        : intl_compose_linux   (xkb dead keys / Multi_key via wtype or xdotool key names; nothing may reach the pty before the commit)
#   macOS        : intl_ime_japanese, intl_ime_pinyin, intl_deadkey_option   (REAL key codes through the input method, source switched with
#                  mac/tools/tis.swift = TISSelectInputSource; the original source is restored in a finally block)
# Windows (#1488, the old windows/ci/cases_caseintl.ps1 is gone): the wide-character cases run in both panes (PowerShell writes the file itself
#   with UTF8Encoding($false): `echo >>` is UTF-16 there), plus intl_altgr_ps / intl_altgr_bash: AltGr on a German layout (KLID 00000407) loaded for the
#   Phosphor window only (WM_INPUTLANGCHANGEREQUEST; nothing system-wide changes; the user's language list + Preload are snapshotted to
#   APPCI_WIN_BAK/kbd_pre.json first and restore.ps1 step "keyboard-layout" re-verifies them).
# Every check is against bytes: effect files written by the typed commands (opened in binary) and the shell's own rec.log; for the
# marked-text rule a raw-tty reader (bytes_reader.py) running in the pane logs every byte that reaches the pty with a timestamp.
# Non-ASCII literals are written as \u escapes so a mangled locale of the editor/CI cannot change what the case expects.
import os, subprocess, time

import cases
from cases import case

HERE = os.path.dirname(os.path.abspath(__file__))
READER = os.path.join(HERE, "bytes_reader.py")

WIDE = [
    ("cjk", "日本語のテスト"),
    ("accents", "éüñçßø"),
    ("emoji", "\U0001F600\U0001F680"),
    ("zwj", "\U0001F468‍\U0001F469‍\U0001F467"),
    ("mixed", "a日b\U0001F600c"),
]


def wide_parts(ctx, fname):
    """(prefix, suffix) of a command that appends  <text> + LF  to fname as UTF-8; bash: `echo text >> f`. A Windows PowerShell pane writes the
    bytes itself ([IO.File]::AppendAllText, UTF-8 without BOM): `>>` / Add-Content would re-encode them."""
    if ctx.pane == "powershell":
        return ("[IO.File]::AppendAllText('%s', '" % os.path.join(ctx.W, fname),
                "' + \"`n\", (New-Object Text.UTF8Encoding $false))")
    return "echo ", " >> " + fname


def wide_parts_edit(ctx, fname):
    """intl_wide_edit's prefix/suffix (r18u8 H11). The editor auto-pairs quotes, and wide_parts() opens a quote in the PREFIX and closes it in the suffix, which is
    fine when the cursor never moves but not when Left/BackSpace run in between: the pair state is lost and a second quote pair lands in the command
    (`file bytes None` in every PowerShell edit). Here the edited text is a bareword (`Write-Output <text>`) with no quote in the edited region, and every quote/brace
    of the file-writing pipeline is typed in one go after End, balanced."""
    if ctx.pane == "powershell":
        return ("Write-Output ", " | ForEach-Object { [IO.File]::AppendAllText('%s', $_ + \"`n\", (New-Object Text.UTF8Encoding $false)) }" % os.path.join(ctx.W, fname))
    return "echo ", " >> " + fname


def fbytes(ctx, name):
    try:
        return open(os.path.join(ctx.W, name), "rb").read()
    except FileNotFoundError:
        return None


def hexs(b):
    return "None" if b is None else b.hex()


def dbg(s):
    """repr() ESCAPES non-printing characters (a real ZWJ prints as the two-character text \\u200d), which made a correct ZWJ look like a typed backslash sequence in r18e0.
    Always show the UTF-8 hex next to the repr so the characters actually received are unambiguous."""
    return "%r [utf8 %s]" % (s, s.encode("utf-8", "surrogatepass").hex())


def check_file(ctx, r, name, text, label, timeout=12):
    """the effect file holds exactly text + newline in UTF-8"""
    want = (text + "\n").encode("utf-8")
    ctx.until(lambda: (fbytes(ctx, name) or b"") != b"" and len(fbytes(ctx, name)) >= len(want), timeout)
    time.sleep(0.3)
    got = fbytes(ctx, name)
    return r.check(got == want, "%s: file bytes %s, expected %s (text %s)" % (label, hexs(got), want.hex(), dbg(text)))


def check_cmd(ctx, r, mark, want_cmd, label):
    """the shell's own record of the command line it received (rec.log); rec.log empty => note only"""
    have = ctx.cmds_since(mark)
    if not have:
        r.note("%s: rec.log empty (hook not run), effect file checked only" % label)
        return
    # exact string, compared as UTF-8 bytes (a ZWJ or variation selector must not be dropped, escaped or normalised on the way)
    r.check(want_cmd in have, "%s: shell received %s, expected %s" % (label, " | ".join(dbg(h) for h in have[-3:]), dbg(want_cmd)))


# ---------------------------------------------------------------- wide characters, typed
@case("intl_wide_type", order=60)
def c_wide_type(ctx, r):
    """CJK, accented Latin, emoji and a ZWJ family sequence typed at 10-25 ms/key with instant Enter: the file and the shell's command
    line are byte-exact UTF-8. Screenshot of the last line is the rendering evidence (wide cells, no tofu is checked by eye / OCR-less)."""
    for tag, text in WIDE:
        f = "iw_%s.txt" % tag
        ctx.rm(f)
        mark = ctx.cmd_mark()
        pre, post = wide_parts(ctx, f)
        cmd = pre + text + post
        try:
            ctx.be.type(cmd, ctx.rng.randint(10, 25), enter=True)
        except Exception as e:      # e.g. an input tool that cannot send this character
            r.check(False, "%s: input tool failed to type %r: %r" % (tag, text, e))
            continue
        check_file(ctx, r, f, text, "typed " + tag)
        check_cmd(ctx, r, mark, cmd, "typed " + tag)
    ctx.be.type(("Write-Output" if ctx.pane == "powershell" else "echo") + " INTL_SHOWN 日本語 \U0001F600 é", 12, enter=True)
    time.sleep(0.8)
    ctx.shot("wide_rendered")


# ---------------------------------------------------------------- wide characters, pasted
@case("intl_wide_paste", order=61)
def c_wide_paste(ctx, r):
    """the same strings pasted (inside a typed command, and a whole pasted command + instant Enter); the clipboard round-trip is checked
    first so a clipboard tool that mangles UTF-8 is blamed on the harness, not the app"""
    for tag, text in WIDE:
        f = "ip_%s.txt" % tag
        f2 = "ipe_%s.txt" % tag
        ctx.rm(f, f2)
        ctx.be.clip_set(text)
        back = ctx.be.clip_get()
        if back.rstrip(b"\n") != text.encode("utf-8"):
            r.check(False, "%s: harness clipboard round-trip altered the text (got %s, expected %s): cannot judge the app" % (tag, back.hex(), text.encode("utf-8").hex()))
            continue
        mark = ctx.cmd_mark()
        pre, post = wide_parts(ctx, f)
        ctx.be.type(pre, 12)
        ctx.be.paste()
        ctx.be.type(post, 12, enter=True)
        check_file(ctx, r, f, text, "paste inside typed " + tag)
        check_cmd(ctx, r, mark, pre + text + post, "paste inside typed " + tag)
        pre2, post2 = wide_parts(ctx, f2)
        ctx.be.clip_set(pre2 + text + post2)
        ctx.be.paste(enter=True)
        check_file(ctx, r, f2, text, "paste + instant Enter " + tag)
    ctx.shot("wide_pasted")


# ---------------------------------------------------------------- editing across wide characters
@case("intl_wide_edit", order=62)
def c_wide_edit(ctx, r):
    """cursor keys and BackSpace over double-width / astral characters move by whole characters (the editor's cell math), checked on the
    command line the shell receives"""
    cjk = "日本語"
    plan = [
        # (typed head, keys, typed tail, expected text in the file)
        ("echo A" + cjk, ["Left"], "Z", "A日本Z語"),
        ("echo A" + cjk, ["BackSpace"], "", "A日本"),
        ("echo B\U0001F600", ["BackSpace"], "q", "Bq"),
        ("echo C" + cjk, ["Left", "Left"], "Y", "C日Y本語"),
        ("echo Dé", ["Left"], "x", "Dxé"),
    ]
    for i, (head, keys, tail, want) in enumerate(plan):
        f = "ie_%d.txt" % i
        ctx.rm(f)
        mark = ctx.cmd_mark()
        pre, post = wide_parts_edit(ctx, f)
        # r18u8 (H12, Git Bash): End at the end of the line ACCEPTS the autosuggestion, and the previous case's command (`echo A日本Z語 >> ie_0.txt`) is a prefix match for
        # `echo A日本` -> the shell got `echo A日本Z語 >> ie_0.txt >> ie_1.txt`. A per-edit unique lead word makes no history line a prefix of the line being edited, so
        # no suggestion exists to be accepted; it is part of the expected text on both sides.
        lead = "k%d%s" % (i, ctx.tok(4).lower())
        head, want = "echo " + lead + head[len("echo "):], lead + want
        ctx.be.type(pre + head[len("echo "):], 12)
        ctx.be.keys(keys, delay_ms=20)
        if tail:
            ctx.be.type(tail, 12)
        ctx.be.key("End")
        ctx.be.type(post, 12, enter=True)
        check_file(ctx, r, f, want, "edit %d (%s)" % (i, "+".join(keys)))
        check_cmd(ctx, r, mark, pre + want + post, "edit %d" % i)
    ctx.shot("wide_edit")


# ---------------------------------------------------------------- a running program receives them
@case("intl_wide_program", order=63)
def c_wide_program(ctx, r):
    """`head -n 1 > file` is the running program (no Ctrl-D needed: it exits after one line): typed and pasted wide text reaches it as exact UTF-8"""
    for tag, text in WIDE[:4]:
        for how in ("typed", "pasted"):
            if how == "pasted" and ctx.be.name == "sim":
                r.note("sim: a paste into a raw program carries the bracketed-paste markers (no terminal emulator in between), pasted variant not judged")
                continue
            f = "pg_%s_%s.txt" % (how, tag)
            ctx.rm(f)
            ctx.be.type("head -n 1 > " + f, 12, enter=True)
            time.sleep(1.0)
            if how == "typed":
                ctx.be.type(text, ctx.rng.randint(10, 25), enter=True)
            else:
                ctx.be.clip_set(text)
                ctx.be.paste()
                time.sleep(0.2)
                ctx.be.key("Return")
            check_file(ctx, r, f, text, "program got %s %s" % (how, tag))
            time.sleep(0.4)
    ctx.shot("wide_program")


# ---------------------------------------------------------------- marked text / compose must not leak to the pty
def read_chunks(ctx, name):
    out = []
    try:
        for l in open(os.path.join(ctx.W, name)):
            p = l.split()
            if len(p) == 2:
                out.append(bytes.fromhex(p[1]))
    except FileNotFoundError:
        pass
    return out


def compose_raw(ctx, r, label, pre, commit, expect, hold=0.8, sw=None):
    """a raw-tty reader runs in the pane. pre() starts a composition (dead key / IME letters): NOTHING may reach the pty. commit() ends it:
    the bytes that arrive are exactly `expect`. Evidence: screenshot while the marked text is shown."""
    f = "cr_%s.txt" % label
    ctx.rm(f, f + ".ready")
    if sw:
        sw.abc()          # ASCII (the reader command) is typed on the plain layout: an IME would compose it
    ctx.be.type("python3 %s %s 40" % (READER, f), 12, enter=True)
    if not ctx.until(lambda: os.path.exists(os.path.join(ctx.W, f + ".ready")), 20):
        r.check(False, "%s: the raw reader never started" % label)
        return
    time.sleep(0.4)
    n0 = len(read_chunks(ctx, f))
    if sw:
        sw.ime()
    pre()
    time.sleep(hold)
    ctx.shot("%s_marked" % label)
    leaked = read_chunks(ctx, f)[n0:]
    r.check(not leaked, "%s: marked text / dead key leaked %s to the pty before the commit" % (label, [c.hex() for c in leaked]))
    n1 = len(read_chunks(ctx, f))
    commit()
    ctx.until(lambda: len(read_chunks(ctx, f)) > n1, 6)
    time.sleep(0.5)
    got = b"".join(read_chunks(ctx, f)[n1:])
    ok = r.check(got == expect, "%s: pty received %s, expected %s" % (label, got.hex(), expect.hex()))
    if sw:
        sw.abc()
    ctx.be.key("ctrl+c")
    time.sleep(0.5)
    return ok


def compose_editor(ctx, r, label, pre, commit, expect_text, pre_check=True, sw=None):
    """input editor: `echo ` + composition + ` >> file`. Nothing may run before Enter (rec.log unchanged while composing); the shell then
    receives exactly `echo <expect_text> >> file` and the file holds expect_text in UTF-8."""
    f = "ce_%s.txt" % label
    ctx.rm(f)
    mark = ctx.cmd_mark()
    if sw:
        sw.abc()
    ctx.be.type("echo ", 12)
    if sw:
        sw.ime()
    pre()
    time.sleep(0.8)
    ctx.shot("%s_editor_marked" % label)
    if pre_check:
        r.check(not ctx.cmds_since(mark), "%s: a command ran while only composing: %s" % (label, ctx.cmds_since(mark)))
    commit()
    time.sleep(0.4)
    if sw:
        sw.abc()
    ctx.be.type(" >> " + f, 12, enter=True)
    check_file(ctx, r, f, expect_text, "editor " + label)
    check_cmd(ctx, r, mark, "echo %s >> %s" % (expect_text, f), "editor " + label)


# ---------------------------------------------------------------- Linux compose / dead keys
@case("intl_compose_linux", order=64, gui=True)
def c_compose_linux(ctx, r):
    if ctx.be.name not in ("wayland", "x11"):
        r.skip("Linux compose/dead keys need the wayland (wtype) or x11 (xdotool) backend, this is %s" % ctx.be.name)
    loc = os.environ.get("LC_ALL") or os.environ.get("LC_CTYPE") or os.environ.get("LANG") or ""
    if "utf" not in loc.lower():
        r.skip("the job locale is %r, not UTF-8: xkb compose is disabled for the app, so dead keys cannot compose (set LANG=C.UTF-8 or en_US.UTF-8 for the job)" % loc)
    seqs = [
        ("acute_e", ["dead_acute", "e"], "é"),
        ("diaer_u", ["dead_diaeresis", "u"], "ü"),
        ("tilde_n", ["dead_tilde", "n"], "ñ"),
        ("multi_e", ["Multi_key", "apostrophe", "e"], "é"),
        ("multi_u", ["Multi_key", "quotedbl", "u"], "ü"),
    ]
    for tag, keys, out in seqs:
        compose_raw(ctx, r, "lx_" + tag, lambda k=keys: ctx.be.keys(k[:-1], delay_ms=20), lambda k=keys: ctx.be.keys(k[-1:], delay_ms=20), out.encode("utf-8"))
    # the editor: dead key, letter, then the rest typed normally
    tag, keys, out = seqs[0]
    compose_editor(ctx, r, "lx_" + tag, lambda: ctx.be.keys(keys[:-1], delay_ms=20), lambda: ctx.be.keys(keys[-1:], delay_ms=20), out)
    ctx.shot("compose_linux")


# ---------------------------------------------------------------- macOS: input sources, IME, Option dead keys
def tis(ctx, *args):
    p = subprocess.run([os.path.join(ctx.be.TOOLS, "tis")] + list(args), capture_output=True, timeout=30)
    return p.returncode, p.stdout.decode(errors="replace").strip()


def need_owner(ctx, what, how):
    msg = ("NEEDS_OWNER: %s. In the GUI session of user gitrunner (Screen Sharing / console): %s. The harness does not enable input sources itself "
           "(that changes the account's settings). Then re-run the job." % (what, how))
    for d in {ctx.run, os.path.dirname(os.path.abspath(ctx.run))}:
        try:
            with open(os.path.join(d, "NEEDS_OWNER_ime.txt"), "a") as fh:
                fh.write(msg + "\n")
        except OSError:
            pass
    raise cases.Skip(msg)


class Switcher:
    """Input-source switching for one case. `abc()` selects the plain layout (ASCII is typed there: the injection tool posts unicode
    events that an active IME would compose), `ime()` the input method under test (+ the Japanese kana key), `restore()` the source that
    was current when the case began. Used as a context manager so the original is restored even when a check throws."""
    def __init__(self, ctx, r, ime_want, how):
        self.ctx, self.r, self.cur = ctx, r, None
        rc, self.orig = tis(ctx, "current")
        if rc != 0:
            raise cases.Skip("tis tool unusable (exit %d: %s): cannot read the input source" % (rc, self.orig))
        rc, lst = tis(ctx, "list")
        rows = [l.split("\t") for l in lst.splitlines()]
        self.enabled = ", ".join(x[0] for x in rows)

        def pick(want):
            c = [x[0] for w in want for x in rows if len(x) >= 2 and w(x[0]) and x[1] == "true"]
            return c[0] if c else None
        self.abc_id, self.ime_id = pick(ABC), pick(ime_want) if ime_want else pick(ABC)
        if not self.abc_id:
            need_owner(ctx, "neither the ABC nor the U.S. keyboard layout is enabled for this user (enabled: %s)" % self.enabled,
                       "System Settings > Keyboard > Text Input > Input Sources > Edit... > + > English > ABC")
        if not self.ime_id:
            need_owner(ctx, "the input method under test is not enabled for this user (enabled: %s)" % self.enabled, how)
        self.kana = bool(ime_want) and ime_want is JA

    def _sel(self, i):
        if self.cur == i:
            return
        rc, out = tis(self.ctx, "select", i)
        if rc != 0:
            self.restore()
            raise cases.Skip("TISSelectInputSource %s failed (exit %d: %s): the gitrunner session cannot switch input sources" % (i, rc, out))
        self.cur = i
        time.sleep(0.8)

    def abc(self):
        self._sel(self.abc_id)

    def ime(self):
        self._sel(self.ime_id)
        if self.kana:
            self.ctx.be._ht(["k:104:0:40"])      # key code 104 = the Japanese 'kana' key: Hiragana mode whatever the last mode was
            time.sleep(0.4)

    def restore(self):
        if self.orig and self.cur is not None and self.cur != self.orig:
            rc, out = tis(self.ctx, "select", self.orig)
            if rc != 0:
                self.r.check(False, "could not restore the original input source %s (exit %d: %s)" % (self.orig, rc, out))
            else:
                self.r.note("input source restored to %s" % self.orig)
        self.cur = self.orig

    def __enter__(self):
        return self

    def __exit__(self, *a):
        self.restore()
        return False


def mac_only(ctx):
    if ctx.be.name != "mac":
        raise cases.Skip("macOS input methods: needs the mac backend (this is %s)" % ctx.be.name)


ABC = [lambda i: i == "com.apple.keylayout.ABC", lambda i: i == "com.apple.keylayout.US"]
JA = [lambda i: "Kotoeri" in i and "Romaji" in i and "Japanese" in i, lambda i: "Kotoeri" in i and "Japanese" in i and "Kana" not in i]
ZH = [lambda i: i == "com.apple.inputmethod.SCIM.ITABC", lambda i: "SCIM" in i and "Pinyin" in i]


@case("intl_ime_japanese", order=65, gui=True)
def c_ime_ja(ctx, r):
    mac_only(ctx)
    how = "System Settings > Keyboard > Text Input > Input Sources > Edit... > + > Japanese > Romaji (Hiragana), Add"
    with Switcher(ctx, r, JA, how) as sw:
        sw.ime()
        ctx.shot("ja_selected")
        sw.abc()
        # marked text never reaches a running program; the commit does, exactly
        compose_raw(ctx, r, "ja_nihon_raw", lambda: ctx.be.keys(list("nihon"), delay_ms=20), lambda: ctx.be.key("Return"),
                    "\u306b\u307b\u3093".encode("utf-8"), sw=sw)
        # the input editor: words committed with Return (hiragana as typed)
        for tag, romaji, kana in (("nihon", "nihon", "\u306b\u307b\u3093"), ("kyou", "kyou", "\u304d\u3087\u3046"),
                                  ("konnichiha", "konnnichiha", "\u3053\u3093\u306b\u3061\u306f")):
            compose_editor(ctx, r, "ja_" + tag, lambda ro=romaji: ctx.be.keys(list(ro), delay_ms=20), lambda: ctx.be.key("Return"), kana, sw=sw)
        # kanji conversion with Space: the candidate is the IME's choice, so the check is structural (only non-ASCII, nothing of the
        # romaji leaked); the value is a metric
        f = "ja_conv.txt"
        ctx.rm(f)
        sw.abc()
        ctx.be.type("echo ", 12)
        sw.ime()
        ctx.be.keys(list("nihon") + ["space"], delay_ms=20)
        time.sleep(0.6)
        ctx.shot("ja_candidates")
        ctx.be.key("Return")
        time.sleep(0.3)
        sw.abc()
        ctx.be.type(" >> " + f, 12, enter=True)
        ctx.until(lambda: fbytes(ctx, f), 10)
        got = (fbytes(ctx, f) or b"").decode("utf-8", "replace").rstrip("\n")
        r.metric("ja_converted", got)
        r.check(bool(got) and all(ord(c) > 127 for c in got) and "\ufffd" not in got, "kanji conversion of 'nihon' produced %r (expected only non-ASCII text, no romaji leak)" % got)
        if got != "\u65e5\u672c":
            r.note("conversion candidate was %r, not the usual \u65e5\u672c (IME ranking is the IME's)" % got)


@case("intl_ime_pinyin", order=66, gui=True)
def c_ime_zh(ctx, r):
    mac_only(ctx)
    how = "System Settings > Keyboard > Text Input > Input Sources > Edit... > + > Chinese, Simplified > Pinyin - Simplified, Add"
    with Switcher(ctx, r, ZH, how) as sw:
        sw.ime()
        ctx.shot("zh_selected")
        sw.abc()
        # Space selects the first candidate and commits it
        compose_raw(ctx, r, "zh_nihao_raw", lambda: ctx.be.keys(list("nihao"), delay_ms=20), lambda: ctx.be.key("space"),
                    "\u4f60\u597d".encode("utf-8"), sw=sw)
        compose_editor(ctx, r, "zh_nihao", lambda: ctx.be.keys(list("nihao"), delay_ms=20), lambda: ctx.be.key("space"), "\u4f60\u597d", sw=sw)
        compose_editor(ctx, r, "zh_zhongwen", lambda: ctx.be.keys(list("zhongwen"), delay_ms=20), lambda: ctx.be.key("space"), "\u4e2d\u6587", sw=sw)


DEAD = [
    # (tag, pre keys, commit keys, expected text)
    ("e_acute", ["alt+e"], ["e"], "\u00e9"),
    ("u_diaer", ["alt+u"], ["u"], "\u00fc"),
    ("n_tilde", ["alt+n"], ["n"], "\u00f1"),
    ("e_acute_up", ["alt+e"], ["shift+e"], "\u00c9"),
    ("i_circ", ["alt+i"], ["i"], "\u00ee"),
    ("acute_x", ["alt+e"], ["x"], "\u00b4x"),       # a dead key followed by a letter that does not combine: the accent, then the letter
    ("acute_sp", ["alt+e"], ["space"], "\u00b4"),   # dead key + Space: the bare accent
]


@case("intl_deadkey_option", order=67, gui=True)
def c_deadkey(ctx, r):
    mac_only(ctx)
    how = "the ABC (or U.S.) keyboard layout must be enabled: System Settings > Keyboard > Text Input > Input Sources"
    with Switcher(ctx, r, None, how) as sw:
        sw.abc()
        for tag, pre, com, out in DEAD[:4]:
            compose_raw(ctx, r, "dk_" + tag, lambda p=pre: ctx.be.keys(p, delay_ms=20), lambda c=com: ctx.be.keys(c, delay_ms=20), out.encode("utf-8"))
        for tag, pre, com, out in DEAD:
            compose_editor(ctx, r, "dk_" + tag, lambda p=pre: ctx.be.keys(p, delay_ms=20), lambda c=com: ctx.be.keys(c, delay_ms=20), out)
        # Option letters that are not dead keys: direct characters
        for tag, key, out in (("alt_s", "alt+s", "\u00df"), ("alt_o", "alt+o", "\u00f8"), ("alt_c", "alt+c", "\u00e7")):
            compose_editor(ctx, r, "dk_" + tag, lambda k=key: ctx.be.key(k), lambda: None, out, pre_check=False)


# ---------------------------------------------------------------- Windows: AltGr on a German layout (#1466, ported from cases_caseintl.ps1 in #1488)
# (virtual key on the German layout, the character AltGr+key must give): Q @, E EUR, 7 {, 8 [, 9 ], 0 }, sz \, < |, + ~, 2 sq, 3 cu, M micro; the plain Y key gives z
ALTGR = [(0x51, 0x40), (0x45, 0x20AC), (0x37, 0x7B), (0x38, 0x5B), (0x39, 0x5D), (0x30, 0x7D), (0xDB, 0x5C), (0xE2, 0x7C), (0xBB, 0x7E), (0x32, 0xB2), (0x33, 0xB3), (0x4D, 0xB5)]
_kbd_snapped = []


def altgr_expected():
    return "".join(chr(c) for _, c in ALTGR) + "z"


def kbd_snapshot(ctx):
    """save the user's keyboard configuration BEFORE any layout is loaded: HKCU\\Keyboard Layout\\Preload and the language list (read-only), to
    $APPCI_WIN_BAK\\kbd_pre.json, the file restore.ps1's "keyboard-layout" step verifies and repairs from"""
    if _kbd_snapped:
        return
    import json
    import winreg
    pre = {"preload": {}, "langs": []}
    try:
        k = winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Keyboard Layout\Preload")
        i = 0
        while True:
            try:
                n, v, _ = winreg.EnumValue(k, i)
            except OSError:
                break
            pre["preload"][n] = str(v)
            i += 1
    except OSError:
        pass
    try:
        ps = subprocess.run(["powershell.exe", "-NoProfile", "-Command",
                             "Get-WinUserLanguageList | ForEach-Object { @{ tag = $_.LanguageTag; tips = @($_.InputMethodTips) } } | ConvertTo-Json -Depth 4 -Compress"],
                            capture_output=True, text=True, timeout=60)
        j = json.loads(ps.stdout)
        pre["langs"] = j if isinstance(j, list) else [j]
    except Exception:
        pass
    d = os.environ.get("APPCI_WIN_BAK") or ctx.run
    os.makedirs(d, exist_ok=True)
    with open(os.path.join(d, "kbd_pre.json"), "w") as f:
        json.dump(pre, f)
    _kbd_snapped.append(1)


class German:
    """the Phosphor window's keyboard layout switched to German (KLID 00000407) for that window's thread only, and back in off()"""
    def __init__(self, ctx):
        import ctypes
        import winapi
        self.w, self.ct, self.ctx = winapi, ctypes, ctx
        self.hwnd = winapi.main_window(ctx.app.pid())
        self.u = winapi.user32
        self.u.LoadKeyboardLayoutW.argtypes = [ctypes.c_wchar_p, ctypes.c_uint]
        self.u.LoadKeyboardLayoutW.restype = ctypes.c_void_p
        self.u.GetKeyboardLayout.argtypes = [ctypes.c_uint]
        self.u.GetKeyboardLayout.restype = ctypes.c_void_p
        self.u.UnloadKeyboardLayout.argtypes = [ctypes.c_void_p]
        self.tid = self.u.GetWindowThreadProcessId(self.hwnd, None)
        self.orig = self.hkl = None

    def on(self):
        """(ok, reason)"""
        kbd_snapshot(self.ctx)
        self.orig = self.u.GetKeyboardLayout(self.tid)
        self.hkl = self.u.LoadKeyboardLayoutW("00000407", 0)
        if not self.hkl:
            return False, "LoadKeyboardLayout(00000407, German) failed: the layout is not available on this VM"
        self.u.PostMessageW(self.hwnd, 0x50, 0, self.hkl)          # WM_INPUTLANGCHANGEREQUEST
        cur = self.orig
        t = time.time()
        while time.time() - t < 5:
            cur = self.u.GetKeyboardLayout(self.tid)
            if (cur or 0) & 0xFFFF == 0x0407:
                break
            time.sleep(0.2)
        if (cur or 0) & 0xFFFF != 0x0407:
            return False, "the Phosphor window's layout did not change to German (is 0x%x, was 0x%x): the VM did not apply WM_INPUTLANGCHANGEREQUEST" % (cur or 0, self.orig or 0)
        time.sleep(0.4)
        return True, ""

    def off(self):
        try:
            if self.orig and self.ctx.app.alive():
                self.u.PostMessageW(self.hwnd, 0x50, 0, self.orig)
                time.sleep(0.6)
            if self.hkl and self.hkl != self.orig:
                self.u.UnloadKeyboardLayout(self.hkl)
        except Exception:
            pass


def altgr_string(ctx):
    for vk, _ in ALTGR:
        ctx.be.altgr(vk)
        time.sleep(ctx.rng.randint(10, 25) / 1000)
    ctx.be.plain_vk(0x59)             # the Y key on a German layout types z
    time.sleep(0.02)


class unicode_keys:
    """printable characters as KEYEVENTF_UNICODE packets for the block (they do not depend on the target's layout)"""
    def __init__(self, ctx):
        self.be = ctx.be

    def __enter__(self):
        self.old = self.be.key_method
        self.be.key_method = "unicode"

    def __exit__(self, *a):
        self.be.key_method = self.old


def altgr_check(ctx, r, label, f, got_wait=12):
    want = altgr_expected().encode("utf-8")
    ctx.until(lambda: len(fbytes(ctx, f) or b"") >= len(want), got_wait)
    time.sleep(0.3)
    got = fbytes(ctx, f)
    return r.check(got == want, "%s: file bytes %s, expected %s" % (label, hexs(got), want.hex()))


def altgr_need_windows(ctx, r):
    if not ctx.win:
        r.skip("Windows only: a per-window German keyboard layout + AltGr (Linux has intl_compose_linux, macOS intl_deadkey_option)")


@case("intl_altgr_ps", order=68, gui=True)
def c_altgr_ps(ctx, r):
    altgr_need_windows(ctx, r)
    if ctx.pane != "powershell":
        r.skip("PowerShell variant: runs in the PowerShell pane pass (intl_altgr_bash covers the Git Bash pane)")
    g = German(ctx)
    ok, why = g.on()
    if not ok:
        g.off()
        r.skip("German layout unavailable: " + why)
    try:
        f = "ag_ps.txt"
        ctx.rm(f)
        with unicode_keys(ctx):
            ctx.be.type("[IO.File]::WriteAllText('%s', '" % os.path.join(ctx.W, f), 12)
            altgr_string(ctx)
            ctx.be.type("', (New-Object Text.UTF8Encoding $false))", 12, enter=True)
        altgr_check(ctx, r, "AltGr string in PowerShell", f)
        r.metric("layout", "German 00000407, per-window")
    finally:
        g.off()


@case("intl_altgr_bash", order=69, gui=True)
def c_altgr_bash(ctx, r):
    altgr_need_windows(ctx, r)
    import winapi
    bash = winapi.git_bash()
    if not bash:
        r.skip("Git for Windows (Git Bash) is not installed on this VM: no bash.exe under Program Files\\Git\\bin")
    g = German(ctx)
    ok, why = g.on()
    if not ok:
        g.off()
        r.skip("German layout unavailable: " + why)
    try:
        f = "ag_bash.txt"
        ctx.rm(f)
        with unicode_keys(ctx):
            if ctx.pane == "powershell":      # Git Bash from the pane, as a user would start it
                ctx.be.type("$env:CHERE_INVOKING = '1'; $env:LC_ALL = 'C.UTF-8'; & '%s' --login -i" % bash.replace("\\", "/"), 12, enter=True)
                time.sleep(4)
            else:
                ctx.be.type("export LC_ALL=C.UTF-8", 12, enter=True)
                time.sleep(0.5)
            ctx.be.type("printf '%s' '", 12)
            altgr_string(ctx)
            ctx.be.type("' > " + f, 12, enter=True)
            time.sleep(0.8)
        altgr_check(ctx, r, "AltGr string in Git Bash (LC_ALL=C.UTF-8)", f)
        with unicode_keys(ctx):
            if ctx.pane == "powershell":
                ctx.be.type("exit", 12, enter=True)
                time.sleep(1)
                ctx.be.type("Remove-Item Env:LC_ALL, Env:CHERE_INVOKING -ErrorAction SilentlyContinue", 12, enter=True)
            else:
                ctx.be.type("unset LC_ALL", 12, enter=True)
        r.metric("layout", "German 00000407, per-window")
    finally:
        g.off()
