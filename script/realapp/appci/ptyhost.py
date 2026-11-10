# appci/ptyhost.py: what the Windows fake sshd (fakesshd_win.py) needs to give `exec` + pty a terminal that changes no byte (#2007).
#
# Why: a managed ssh session is `ssh -t fake '<bootstrap command>'`. The remote command prints two hooks as ONE escape sequence each
# (ESC ] 9278 ; d ; <hex> BEL, about 470 and 280 bytes). On kernel32's pseudo console of Windows 10 an msys program cannot do that: the msys
# console layer hands a long sequence over in 256-byte pieces and that console host passes on only the last piece of an unknown OSC. The app
# then never sees the hooks (round 9: four Git Bash cases, "no remote shell prompt"). A POSIX sshd gives the remote a pty that passes bytes
# through; so does Git for Windows' own sshd.exe (an msys pty). This file holds
#   - the config of that sshd as a loopback "pty host" (started by sshd.py, never the user's or the system's sshd),
#   - BackendPty: an exec + pty on it, with the interface of fakesshd_win.ConPty, so the front server's pumps are the same code,
#   - probe(): the measurement sshd.py runs at start through the FRONT server for every arm (a long OSC out, a counted raw read in),
#   - choose_arm() / summary(): which arm serves exec + pty, and the line that goes into the job log and into every ssh_diag.
# Nothing here imports paramiko at module level: run.py --self-test checks the pure parts on a machine without it.
import hashlib
import os
import re
import time

ARMS = ("msys", "app", "inbox")      # in order of preference: a real msys pty; the app's own console host (conpty.dll); kernel32's pseudo console
ARM_TEXT = {"msys": "a real msys pty (Git for Windows' sshd.exe as a loopback pty host)",
            "app": "a pseudo console of the app's own conpty.dll",
            "inbox": "the pseudo console that ships with Windows (kernel32 CreatePseudoConsole)"}
ARM_ENV = "FAKEPTY_ARM"              # an env request with this name pins the arm for ONE exec (the probe); the server refuses instead of falling back


def arm_order(available, requested, chosen):
    """-> (arms to try in order, pinned) for one exec + pty request. `requested` (the client's FAKEPTY_ARM env request: the start-up probe) pins
    ONE arm: it is served or refused, never replaced by another, or the probe would measure the wrong terminal. Otherwise `chosen` (the arm
    sshd.py measured) comes first and the other available ones follow, so an arm that stops working mid-run degrades to the next instead of
    ending every ssh case; with nothing measured the order is the order of preference."""
    if requested:
        return ([requested] if requested in available else []), True
    if chosen in available:
        return [chosen] + [a for a in available if a != chosen], False
    return list(available), False


# #2051: the console host behind the `app` arm (the app's conpty.dll / OpenConsole) opens every pseudo console by asking ITS terminal for
# the primary device attributes and waits for the answer before it goes on. Round 12: `conpty first output after 0.01 s: 4 bytes` before any
# program output on that arm only, the app answered `ESC [ ? 62 c` 47 ms later, and the exec still needed 3 s more than on `inbox` (the
# start-up probe, which answers nothing: 5 s against 1 s). A POSIX sshd's pty asks its client nothing, and this server stands in for one,
# so it answers the request itself and does not pass it on (the app's late answer would otherwise arrive as typed input).
DA1_QUERIES = (b"\x1b[c", b"\x1b[0c")
DA1_REPLY = b"\x1b[?62c"             # what the app answers (round 12 app log, class=Reply)


def strip_startup_da1(first):
    """-> (the first output chunk without a leading device-attributes request, the request or b"")"""
    for q in DA1_QUERIES:
        if first.startswith(q):
            return first[len(q):], q
    return first, b""


class InputStats:
    """what the client sent on one session, for the `client input ended` line (#2051: round 12's managed sessions received MORE bytes than the
    app wrote, about one byte per packet at 1.3 KB/s, and the log could not say what the bytes were)"""

    def __init__(self, now=None):
        self.t0 = self.t1 = None
        self.n = self.esc = self.cr = self.lf = self.ctl = self.high = 0
        self.head = b""
        self.now = now or time.time

    def add(self, d):
        t = self.now()
        if self.t0 is None:
            self.t0 = t
        self.t1 = t
        self.n += len(d)
        if len(self.head) < 48:
            self.head += d[:48 - len(self.head)]
        for b in d:
            if b == 0x1b:
                self.esc += 1
            elif b == 0x0d:
                self.cr += 1
            elif b == 0x0a:
                self.lf += 1
            elif b < 0x20 or b == 0x7f:
                self.ctl += 1
            elif b >= 0x80:
                self.high += 1

    def text(self):
        if not self.n:
            return "nothing received"
        dur = self.t1 - self.t0
        rate = (" %.0f bytes/s," % (self.n / dur)) if dur >= 0.5 else ""
        return "%.1f s first to last byte,%s ESC %d, CR %d, LF %d, other control %d, non-ASCII %d; starts %r" % (
            dur, rate, self.esc, self.cr, self.lf, self.ctl, self.high, self.head)


def msys_path(p):
    """`C:\\a\\b` or `C:/a/b` -> `/c/a/b` (Git for Windows mounts drives at /<letter>); anything else only gets forward slashes"""
    p = p.replace("\\", "/")
    if len(p) >= 2 and p[1] == ":" and p[0].isalpha():
        return "/" + p[0].lower() + p[2:]
    return p


def sh_quote(s):
    return "'" + s.replace("'", "'\\''") + "'"


def sshd_config(port, hostkey, authorized_keys, pidfile, force):
    """sshd_config of the pty host: loopback only, the scratch client key only, every session forced into force.sh (run by bash, so it needs no
    execute bit). All paths msys form. No UsePAM line (the msys build has none); StrictModes off (NTFS permissions are not POSIX modes)."""
    return "\n".join([
        "Port %d" % port, "ListenAddress 127.0.0.1", "HostKey %s" % hostkey, "PidFile %s" % pidfile,
        "AuthorizedKeysFile %s" % authorized_keys, "PasswordAuthentication no", "KbdInteractiveAuthentication no",
        "StrictModes no", "PubkeyAuthentication yes", "ForceCommand /usr/bin/bash %s" % sh_quote(force), "PermitTTY yes",
        "X11Forwarding no", "AllowTcpForwarding no", "LogLevel DEBUG1", ""])


def force_script(rhome):
    """what the pty host runs for every session: the remote side fakesshd_win gives a ConPTY session (HOME = the scratch remote home, bash as
    the login shell, started in that home), then the command as sshd does, `$SHELL -c`; without one, the interactive shell with the scratch rc."""
    return ("#!/bin/bash\n"
            "# appci pty host (ptyhost.py): the remote side of an exec + pty of the Windows fake sshd\n"
            "export HOME=%s SHELL=/usr/bin/bash CHERE_INVOKING=1\n"
            "export USER=\"${USER:-${USERNAME:-tester}}\" LOGNAME=\"${LOGNAME:-${USERNAME:-tester}}\"\n"
            "cd \"$HOME\" || exit 96\n"
            "if [ -n \"${SSH_ORIGINAL_COMMAND:-}\" ]; then exec \"$SHELL\" -c \"$SSH_ORIGINAL_COMMAND\"; fi\n"
            "exec \"$SHELL\" --rcfile \"$HOME/.bashrc\" -i\n") % sh_quote(rhome)


def pubkey_line(key):
    """authorized_keys line of a paramiko key"""
    return "%s %s appci-scratch\n" % (key.get_name(), key.get_base64())


def load_key(path):
    import paramiko
    for cls in (paramiko.Ed25519Key, paramiko.RSAKey, paramiko.ECDSAKey):
        try:
            return cls(filename=path)
        except Exception:
            pass
    raise ValueError("cannot load key " + path)


class BackendPty:
    """`exec` with a pty on the pty host, through a paramiko client. Same interface as fakesshd_win.ConPty (pid, read, write, resize, exited,
    exit_code, close): read() blocks and returns b"" when the remote side is done."""

    def __init__(self, port, user, key, term, cols, rows, cmd, env=None, timeout=10.0):
        import paramiko
        self.pid = "ptyhost:%d" % port
        self.closed = False
        self._code = None
        self.t = paramiko.Transport(("127.0.0.1", port))
        try:
            self.t.start_client(timeout=timeout)
            self.t.auth_publickey(user, key)
            self.ch = self.t.open_session(timeout=timeout)
            for k, v in (env or {}).items():
                try:                      # sshd accepts none without AcceptEnv; the remote side does not depend on them
                    self.ch.set_environment_variable(k, v)
                except Exception:
                    pass
            self.ch.get_pty(term or "dumb", cols or 80, rows or 24)
            self.ch.exec_command(cmd or "true")
        except Exception:
            try:
                self.t.close()
            except Exception:
                pass
            raise

    def read(self):
        try:
            return self.ch.recv(65536)
        except Exception:
            return b""

    def write(self, data):
        try:
            self.ch.sendall(data)
        except Exception:
            pass

    def resize(self, cols, rows):
        try:
            self.ch.resize_pty(cols, rows)
        except Exception:
            pass

    def exited(self):
        if self._code is None and self.ch.exit_status_ready():
            self._code = self.ch.exit_status
        return self._code is not None or self.ch.closed

    def exit_code(self):
        self.exited()
        return self._code if self._code is not None and self._code >= 0 else 0

    def close(self):
        if self.closed:
            return
        self.exited()
        self.closed = True
        for c in (self.ch, self.t):
            try:
                c.close()
            except Exception:
                pass


# ------------------------------------------------------------------ the measurement
PROBE_OSC_HEX = ("7068782d70726f62652d" * 30)                    # 600 hex characters: longer than two of the console layer's 256-byte pieces
PROBE_OSC = b"\x1b]9278;d;" + PROBE_OSC_HEX.encode() + b"\x07"    # the form of the remote's SSH / InitShell hooks (not a hook: only the probe's own client reads it)


def probe_payload():
    """what the counted raw read gets: 4 KB shaped like the bootstrap body (text lines ending in LF, one of them with non-ASCII bytes)"""
    lines = ["# phx probe line %03d %s" % (i, "abcdefghijklmnopqrstuvwxyz0123456789 $(){}[]<>|&;'\"\\"[: 20 + i % 30]) for i in range(70)]
    lines[7] = "# non-ascii: \u00e9\u2014\u00fc"
    return ("\n".join(lines) + "\n").encode("utf-8")


def probe_command(n):
    """the remote command of the probe. Prints the long OSC, then reads exactly `n` bytes from its tty in raw mode and prints their md5."""
    return ("printf 'PRB1'; printf '\\033]9278;d;%%s\\007' '%s'; printf 'PRB2\\n'; "
            "o=$(command -p stty -g 2>/dev/null); command -p stty raw -echo 2>/dev/null; printf 'RDY\\n'; "
            "d=$(LC_ALL=C command -p head -c %d | command -p md5sum); "
            "test -n \"$o\" && command -p stty \"$o\" 2>/dev/null; printf 'MD5=%%s=\\n' \"${d%%%% *}\"") % (PROBE_OSC_HEX, n)


def judge_output(data):
    """(ok, text) for the output half: the OSC must be in `data` whole"""
    if PROBE_OSC in data:
        return True, "a %d-byte OSC arrived whole" % len(PROBE_OSC)
    hexes = re.findall(rb"(?:7068782d70726f62652d)+", data)
    got = max((len(h) for h in hexes), default=0)
    if b"PRB1" not in data and b"PRB2" not in data and not got:
        return False, "the remote command printed nothing the client saw (%d bytes: %r)" % (len(data), data[:80])
    return False, "a %d-byte OSC did NOT arrive whole: the longest piece of its %d payload characters is %d (%d bytes seen)" % (
        len(PROBE_OSC), len(PROBE_OSC_HEX), got, len(data))


def judge_input(data, payload):
    """(ok, text) for the input half: the remote's md5 of the bytes it read must be the payload's"""
    want = hashlib.md5(payload).hexdigest().encode()
    m = re.search(rb"MD5=([0-9a-f]{32})=", data)
    if m is None:
        return False, "a counted raw read of %d bytes did not finish (bytes lost on the way in, or the tty is not raw)" % len(payload)
    if m.group(1) != want:
        return False, "a counted raw read of %d bytes got other bytes than were sent (md5 differs)" % len(payload)
    return True, "a counted raw read of %d bytes got exactly what was sent" % len(payload)


def probe(port, user, keypath, arm, timeout=15.0):
    """Measure one exec + pty arm through the FRONT server on 127.0.0.1:port (the path the app's ssh takes, minus the ssh client).
    -> dict(arm, out_ok, in_ok, out, inp). Never raises."""
    res = dict(arm=arm, out_ok=False, in_ok=False, out="not measured", inp="not measured")
    t = None
    try:
        import paramiko
        payload = probe_payload()
        t = paramiko.Transport(("127.0.0.1", port))
        t.start_client(timeout=timeout)
        t.auth_publickey(user, load_key(keypath))
        ch = t.open_session(timeout=timeout)
        ch.set_environment_variable(ARM_ENV, arm)
        ch.get_pty("xterm-256color", 120, 30)
        ch.exec_command(probe_command(len(payload)))
        ch.settimeout(0.25)
        data, sent, end = b"", False, time.time() + timeout
        while time.time() < end:
            try:
                d = ch.recv(65536)
                if not d:
                    break
                data += d
            except Exception as e:
                if type(e).__name__ != "timeout" and not isinstance(e, TimeoutError):
                    break
            if not sent and b"RDY" in data:
                ch.sendall(payload)
                sent = True
            if re.search(rb"MD5=[0-9a-f]{32}=", data):
                break
        res["out_ok"], res["out"] = judge_output(data)
        if sent:
            res["in_ok"], res["inp"] = judge_input(data, payload)
        else:
            res["inp"] = "the remote command never got to its read (no RDY within %.0f s)" % timeout
    except Exception as e:
        res["out"] = res["inp"] = "the probe could not run: %r" % (e,)
    finally:
        try:
            if t is not None:
                t.close()
        except Exception:
            pass
    return res


def choose_arm(results):
    """the arm that serves exec + pty: the first (in ARMS order) that passed both halves; else the first whose OUTPUT is whole (the remote's hooks
    reach the app; the bootstrap going in is then what fails, visibly); else None (the server keeps its own order of what is available)."""
    by = {r["arm"]: r for r in results}
    for want in (lambda r: r["out_ok"] and r["in_ok"], lambda r: r["out_ok"]):
        for a in ARMS:
            if a in by and want(by[a]):
                return a
    return None


def summary(results, chosen, notes=()):
    """one line for the job log and for ssh_diag"""
    if not results:
        return "exec+pty of the test sshd was not measured (%s)" % ("; ".join(notes) or "no probe ran")
    parts = ["%s: out %s, in %s (%s; %s)" % (r["arm"], "ok" if r["out_ok"] else "BROKEN", "ok" if r["in_ok"] else "BROKEN", r["out"], r["inp"]) for r in results]
    by = {r["arm"]: r for r in results}
    if chosen and by[chosen]["out_ok"] and by[chosen]["in_ok"]:
        head = "exec+pty of the test sshd is served by %s, measured byte-exact both ways" % ARM_TEXT[chosen]
    elif chosen:
        head = ("TEST HOST FAULT: no exec+pty arm of the test sshd is byte-exact both ways; using %s (output whole, input not): a managed ssh session "
                "gets its hooks out but its bootstrap may not get in" % ARM_TEXT[chosen])
    else:
        head = ("TEST HOST FAULT: no exec+pty arm of the test sshd passes a long escape sequence whole, so a managed ssh session cannot start on this "
                "test host whatever the app does (#2007)")
    return "%s. Measured: %s%s" % (head, " | ".join(parts), ("; " + "; ".join(notes)) if notes else "")
