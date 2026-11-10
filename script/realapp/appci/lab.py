# appci/lab.py: client of the fake ssh lab (script/realapp/sshlab, docs/testing.md 2.15) plus its pure pieces (matrix expansion, TOTP, the RTT gate,
# the paired-latency verdict, sshd-log channel counts, the remote-read adapters). No keys, hostnames or IPs live in the repo: the box address comes
# from APPCI_LAB_BOX / SSHLAB_BOX (remote mode), the forced-command key from APPCI_LAB_KEY, and every lab key is generated per run. These settings (and APPCI_LAB, APPCI_LAB_KNOWN_HOSTS) may also come from the machine-local ~/.config/phosphor-ci/ci.env (ci_env.get; environment wins).
#   APPCI_LAB=off|local|fixed|remote   default: local when podman exists (the box); else fixed when SSHLAB_BOX and SSHLAB_TOKEN are set (the Mac/Windows runners: a persistent
#       lab container on a fixed port set per platform, leased over HTTP with the token, no ssh to the box); else off (the lab cases SKIP with the reason).
#       remote = LEGACY forced-command ssh mode, kept working but no longer the documented path.
#   APPCI_LAB_SLOT=<name> overrides the slot (default: mac on macOS, windows on Windows); APPCI_LAB_SLOTS overrides the slot table (labctl serve --slots syntax).
#   SSHLAB_TOKEN is read through ci_env.get and is NEVER printed, logged or written to an artifact.
#   APPCI_LAB_MATRIX=quick|full|<spec>   which arms of a lab case run (see matrix_selection)
import base64, secrets, fnmatch, hashlib, hmac, io, itertools, json, os, re, shutil, socket, statistics, struct, subprocess, sys, tarfile, tempfile, time, urllib.parse, urllib.request, urllib.error

# #2043: `lab.py stop <run dir>` runs as its own process (windows/ci/after_job.ps1), and the Windows machine's embeddable Python does not put the
# script's folder on sys.path: without this line the sibling import raised ModuleNotFoundError there (as fakesshd_win.py's did).
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ci_env  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
SSHLAB_DIR = os.path.normpath(os.path.join(HERE, "..", "sshlab"))
USERS_PY = os.path.join(SSHLAB_DIR, "rootfs", "usr", "local", "lib", "sshlab", "usersdb.py")
ALL_SHELLS = ("bash", "zsh", "fish", "nu", "pwsh", "dash", "ash", "rbash")
LATS = {"lan": 0, "150": 150, "300": 300}          # RTT label -> ms
HELPERS = ("nohelper", "helper", "upload", "upgrade", "never")   # nohelper is the default and always the first arm. helper: pre-installed. upload: nothing installed, the app uploads
# it in-band (the lab blocks outbound, so only an upload can succeed). upgrade: older versions pre-seeded, the app replaces them. never: per-host policy never, nothing may be uploaded
RTT_TOL_MS = 15.0
NET_FAULTS = ("rst", "blackhole", "stall10")


# ------------------------------------------------------------------ matrix
def lat_label(ms):
    return "lan" if not ms else str(int(ms))


def matrix_selection(value=None):
    """APPCI_LAB_MATRIX -> dict(shells, lats, helpers, faults, scenarios). quick / full / 'shells=a,b;lat=lan,150;helper=nohelper;faults=1;scenarios=0'.
    shells/lats/helpers None = every value the case declares. Unknown keys or values raise ValueError (a typo must not select nothing)."""
    v = (value if value is not None else os.environ.get("APPCI_LAB_MATRIX", "quick")).strip() or "quick"
    if v == "quick":
        return dict(shells={"bash", "zsh", "fish", "nu"}, lats={"lan", "150"}, helpers={"nohelper", "helper"}, faults=False, scenarios=False)
    if v == "full":
        return dict(shells=None, lats=None, helpers=None, faults=True, scenarios=True)
    sel = dict(shells=None, lats=None, helpers={"nohelper"}, faults=False, scenarios=False)
    for part in v.split(";"):
        if not part.strip():
            continue
        if "=" not in part:
            raise ValueError("bad APPCI_LAB_MATRIX part %r" % part)
        k, val = [x.strip() for x in part.split("=", 1)]
        items = {x.strip() for x in val.split(",") if x.strip()}
        if k == "shells":
            bad = items - set(ALL_SHELLS)
            sel["shells"] = items
        elif k == "lat":
            bad = items - set(LATS)
            sel["lats"] = items
        elif k == "helper":
            bad = items - set(HELPERS)
            sel["helpers"] = items
        elif k in ("faults", "scenarios"):
            bad = set()
            sel[k] = val in ("1", "true", "yes")
        else:
            raise ValueError("unknown APPCI_LAB_MATRIX key %r" % k)
        if bad:
            raise ValueError("unknown value(s) %s for %s" % (sorted(bad), k))
    return sel


def expand_arms(case, shells=("bash",), lats=("lan",), helpers=("nohelper",), faults=(), scenarios=()):
    """every arm a lab case DECLARES (the full matrix): list of dicts {name, case, shell, user, lat_ms, lat, helper, fault, scenario}.
    Names: lab_<case>.<shell|scenario user>.<lat>.<helper>[.<fault>]. Arms are ordered helper-major (so the app relaunches only when the helper mode
    changes), nohelper first; a fault arm runs at LAN and at 150 only."""
    out = []
    hs = sorted(helpers, key=lambda h: HELPERS.index(h))
    for h in hs:
        for who, is_scen in [(s, False) for s in shells] + [(s, True) for s in scenarios]:
            for lat in lats:
                for fault in (None,) + tuple(faults):
                    if fault and (is_scen or lat not in ("lan", "150") or who not in ("bash", "zsh")):
                        continue
                    user = who if is_scen else "u_" + who
                    out.append(dict(name="lab_%s.%s.%s.%s%s" % (case, who, lat, h, "." + fault if fault else ""), case=case, shell="bash" if is_scen else who,
                                    user=user, lat_ms=LATS[lat], lat=lat, helper=h, fault=fault, scenario=is_scen))
    return out


def arm_selected(arm, sel):
    """does the matrix selection `sel` (matrix_selection) want this arm?"""
    if arm["fault"] and not sel["faults"]:
        return False
    if arm["scenario"] and not sel["scenarios"]:
        return False
    if sel["shells"] is not None and not arm["scenario"] and arm["shell"] not in sel["shells"]:
        return False
    if sel["lats"] is not None and arm["lat"] not in sel["lats"]:
        return False
    if sel["helpers"] is not None and arm["helper"] not in sel["helpers"]:
        return False
    return True


def name_matches(patterns, name):
    """--cases accepts exact names and fnmatch globs, comma separated"""
    return any(fnmatch.fnmatchcase(name, p) for p in patterns.split(",") if p)


# ------------------------------------------------------------------ verdict maths
def totp(secret_b32, t=None, step=30, digits=6):
    """RFC 6238 TOTP (HMAC-SHA1), the code pam_google_authenticator expects for the secret labd leased"""
    s = secret_b32.upper()
    key = base64.b32decode(s + "=" * (-len(s) % 8))
    counter = int((time.time() if t is None else t) // step)
    h = hmac.new(key, struct.pack(">Q", counter), hashlib.sha1).digest()
    o = h[-1] & 0x0F
    return str((struct.unpack(">I", h[o:o + 4])[0] & 0x7FFFFFFF) % (10 ** digits)).zfill(digits)


def rtt_gate_verdict(samples_ms, target_ms, tol=RTT_TOL_MS, min_n=5):
    """the per-arm gate: the median measured echo RTT must be target +- tol (LAN: <= tol). Anything else is 'mismatch' = the arm is UNVERIFIABLE (never a pass);
    too few samples is 'no-data'."""
    xs = [x for x in samples_ms if x is not None]
    if len(xs) < min_n:
        return dict(verdict="no-data", median=None, target=target_ms, n=len(xs))
    med = statistics.median(xs)
    ok = med <= tol if not target_ms else abs(med - target_ms) <= tol
    return dict(verdict="ok" if ok else "mismatch", median=round(med, 1), target=target_ms, n=len(xs), tol=tol)


def paired_verdict(app, plain, floor_ms=15.0, pct=0.10, min_n=10):
    """owner decision 2026-11-08: median(app) <= median(plain ssh) + max(15 ms, 10 %) at 150/300 ms (the +-1 ms budget is for LAN key->echo only).
    Samples are interleaved by the caller. -> dict(verdict pass|fail|no-data, app, plain, allowed, over_ms)"""
    a, p = [x for x in app if x is not None], [x for x in plain if x is not None]
    if len(a) < min_n or len(p) < min_n:
        return dict(verdict="no-data", n_app=len(a), n_plain=len(p))
    ma, mp = statistics.median(a), statistics.median(p)
    allowed = mp + max(floor_ms, pct * mp)
    return dict(verdict="pass" if ma <= allowed else "fail", app=round(ma, 1), plain=round(mp, 1), allowed=round(allowed, 1), over_ms=round(ma - allowed, 1))


# ---- lab_enter_start: where the Enter starts and where the command starts (round 10, #2027)
CORE_ENTER_WINDOW_RE = re.compile(r"input_order hop=core_enter stage=handle_window_event\b.* t=(\d+(?:\.\d+)?)\s*$")
ENTER_BEGIN_T_RE = re.compile(r"input_order hop=enter_begin\b.* t=(\d+(?:\.\d+)?)\s*$")
ENTER_KEY_TO_BEGIN_MAX_MS = 250.0       # an Enter's window event and its enter_begin are a fraction of a ms apart; further apart, they are not a pair


def app_enter_epoch(lines, tok):
    """pure: the wall time (epoch SECONDS, the app's clock = this machine's) at which the app RECEIVED the Enter that submitted the line holding `tok`, from
    the app's own log: the `hop=core_enter stage=handle_window_event` line just before the `hop=enter_begin ... buffer="...tok..."` line (the enter_begin time
    itself when no window event precedes it closely). None when the log has no enter_begin for the token (input-order logging off, or the line's text is not
    logged). Round 10 lab, Wayland: the case stamped the Enter BEFORE the injector call, and every Wayland injector call starts with a priming Shift key and a
    20 ms wait (backend.WaylandBackend._wtype, APPCI_WL_PRIME_MS): the app log showed the Shift at ...514.086 and the Enter at ...538.847, so about 35 ms of
    injector time per Enter were charged to the app against a 15 ms budget, while the plain ssh control stamps right before its CR write. The time the app
    itself needs from the key to the write (1.3 ms in that run) stays inside the measure."""
    window_ms = None
    for l in lines:
        m = CORE_ENTER_WINDOW_RE.search(l)
        if m:
            window_ms = float(m.group(1))
            continue
        if "hop=enter_begin" in l and tok in l:
            m = ENTER_BEGIN_T_RE.search(l)
            if not m:
                return None
            begin_ms = float(m.group(1))
            if window_ms is not None and 0 <= begin_ms - window_ms <= ENTER_KEY_TO_BEGIN_MAX_MS:
                return window_ms / 1000.0
            return begin_ms / 1000.0
    return None


def start_stamp_line(tok, fname):
    """pure: the line lab_enter_start types into a bash or zsh: the command writes ITS OWN start time (the shell's clock, expanded when the command runs,
    after every preexec hook) next to its token. rec.log cannot say when a command started: the bash and zsh hooks write the C line at the NEXT prompt, after
    the command and after the session's prompt hooks, which is a different question (kept as the metric enter_to_next_prompt_*)."""
    return "echo %s $EPOCHREALTIME >> %s" % (tok, fname)


def start_stamps(lines):
    """pure: {token: epoch seconds on the REMOTE clock} from the lines start_stamp_line() commands wrote (`tok 1794305734.123456`; a locale's decimal comma
    is read too). A line without a parseable stamp (a shell with no $EPOCHREALTIME) is left out."""
    out = {}
    for l in lines:
        parts = l.split()
        if len(parts) >= 2:
            try:
                out.setdefault(parts[0], float(parts[1].replace(",", ".")))
            except ValueError:
                pass
    return out


def enter_start_samples(toks, enters, starts, offset):
    """pure: per token, ms from its Enter (local epoch seconds, None = unknown) to its start stamp (remote epoch seconds; `offset` = remote clock minus local
    clock), None where either is missing"""
    out = []
    for tok, t in zip(toks, enters):
        s = starts.get(tok)
        out.append(None if t is None or s is None else (s - offset - t) * 1000.0)
    return out


# ---- lab_ctrl_c_no_esc_i: the long command a round interrupts, per login shell (round 11, #2045)
def long_sleep_line(shell):
    """pure: the line that runs for 30 s in `shell`. nushell's own `sleep` takes a duration (`sleep 30` is a type error there, the command ends at once and the
    Ctrl-C would meet an idle prompt, proving nothing); every other lab shell runs the external `sleep 30`."""
    return "sleep 30sec" if shell == "nu" else "sleep 30"


# ---- lab_ctrl_c_no_esc_i: how many prompts a Ctrl-C burst may draw (round 10, #2027)
def ctrl_c_prompt_bounds(log_text, pressed):
    """pure: (fewest, most) prompt.log lines a round of lab_ctrl_c_no_esc_i may add in bash: one for the interrupted `sleep`, one for the echo, plus at most one
    for every FURTHER Ctrl-C the app wrote to the pty (`hop=ctrl_c_pty_write`): a Ctrl-C that reaches a bash sitting at its prompt draws a fresh prompt, under
    plain ssh too. The app writes a Ctrl-C while it has not seen the prompt come back; on a 150 ms link a second key 33 ms after the first is in that window
    (round 10: two `ctrl_c_route route=pty long_running=true` 33 ms apart, then two CommandFinished / Precmd pairs), with a 200 ms gap it is not (`route=none`,
    count 2). Without the hop in the log (input-order logging off) every pressed Ctrl-C may have been written."""
    wrote = log_text.count("hop=ctrl_c_pty_write")
    if "hop=ctrl_c_" not in log_text:
        wrote = pressed
    return 2, 2 + max(0, min(wrote, pressed) - 1)


def enter_wait_verdict(delayed_ms, lan_ms, rtt_ms, budget_ms=2.0, min_n=8):
    """#1970: did an Enter wait on the network before its line was written? Inputs are the APP's own Enter -> write times in ms (`hop=enter_begin` -> the
    line's `pty_write`, cases.enter_segment_samples): `delayed_ms` typed with the link at `rtt_ms`, `lan_ms` typed in the same ssh session with the delay
    removed (None or too few: no control). Two rules:
      1. no delayed sample may reach HALF the RTT. A wait on any network round trip costs at least one whole RTT, so a sample under half of it cannot
         contain one; the bound follows from the delay itself and is not a tuned number.
      2. with a control, median(delayed) <= median(lan) + budget_ms (the owner's Enter budget, 2 ms): the app's share of Enter must not grow with the link.
    -> dict(verdict pass|fail|no-data, n, worst, limit, median, lan_median, delta, why)"""
    d = [x for x in (delayed_ms or []) if x is not None]
    l = [x for x in (lan_ms or []) if x is not None]
    if len(d) < min_n or not rtt_ms:
        return dict(verdict="no-data", n=len(d), why="%d Enter -> write sample(s) at the delayed link, %d needed" % (len(d), min_n) if rtt_ms else "no link delay: nothing to judge")
    worst, limit, med = max(d), rtt_ms / 2.0, statistics.median(d)
    out = dict(n=len(d), worst=round(worst, 2), limit=limit, median=round(med, 2), lan_median=None, delta=None)
    if worst >= limit:
        return dict(out, verdict="fail", why="an Enter's line was written %.1f ms after the Enter with a %d ms link: that is half a round trip or more (limit %.0f ms), "
                    "so the Enter waited on the network" % (worst, rtt_ms, limit))
    if len(l) < min_n:
        return dict(out, verdict="pass", why="every Enter -> write under half the RTT (worst %.2f ms, limit %.0f ms); no undelayed control (%d sample(s))" % (worst, limit, len(l)))
    lm = statistics.median(l)
    out.update(lan_median=round(lm, 2), delta=round(med - lm, 2))
    if med - lm > budget_ms:
        return dict(out, verdict="fail", why="Enter -> write median %.2f ms with a %d ms link against %.2f ms without it in the same session: %.2f ms more, the budget is %.1f ms "
                    "(the app's share of Enter must not grow with the link delay)" % (med, rtt_ms, lm, med - lm, budget_ms))
    return dict(out, verdict="pass", why="worst Enter -> write %.2f ms (limit %.0f ms); median %.2f ms delayed against %.2f ms undelayed" % (worst, limit, med, lm))


# ---- lab_raw_key_arrival: a key written to the pty -> the byte read by a raw-input program on the server (#985)
RAW_KEY_LOGGER_NAME = "rawkeys.sh"
RAW_KEY_GIVE_UP_S = 120             # give-up net of the logger's read: a key that never comes ends the logger, the case fails on the missing bytes
# bash >= 5 ($EPOCHREALTIME; the lab image's bash). The tty is put in non-canonical, no-echo mode ONCE, so a byte is readable the moment it arrives and
# nothing is echoed between two reads; the stamp is taken right after the read returns. `<out>.ready` says the logger owns the tty, `<out>.done` that it ended.
RAW_KEY_LOGGER = """#!/bin/bash
out=$1; n=$2
old=$(stty -g)
trap 'stty "$old"; : > "$out.done"' EXIT
stty -icanon -echo min 1 time 0
: > "$out"
: > "$out.ready"
i=0
while [ "$i" -lt "$n" ] && IFS= read -r -s -N 1 -t %d c; do
  printf '%%s %%s\\n' "$EPOCHREALTIME" "$c" >> "$out"
  i=$((i + 1))
done
""" % RAW_KEY_GIVE_UP_S
RAW_KEY_ALPHABET = "abcdefghijklmnopqrstuvwxyz0123456789"       # one byte each, no shell or terminal meaning, the same key on every layout the harness maps


def raw_key_logger_line(script_path, out_path, n):
    """pure: the line that starts the logger from any lab login shell (an external command with three plain words)"""
    return "bash %s %s %d" % (script_path, out_path, n)


def raw_key_arrivals(lines):
    """pure: [(epoch seconds on the REMOTE clock, character)] from the logger's file (`1794305734.123456 a`; a locale's decimal comma is read too).
    A line that is not `<number> <one character>` is left out, so a caller comparing the characters with the typed ones sees it as a missing key."""
    out = []
    for l in lines:
        p = l.split(" ")
        if len(p) == 2 and len(p[1]) == 1:
            try:
                out.append((float(p[0].replace(",", ".")), p[1]))
            except ValueError:
                pass
    return out


def raw_key_samples(keys, send_times, arrivals, offset):
    """pure: per typed key, ms from the moment it was WRITTEN towards ssh (local clock) to the moment the server's logger read it (remote clock; `offset` =
    remote minus local). `send_times`: the epoch seconds of every byte written in the window, in write order; `arrivals`: raw_key_arrivals(). The pairing
    is by position, so it is made only when it cannot be wrong: the arrivals ARE the typed keys, in order, and exactly one byte was written per key.
    Anything else gives no sample at all ([]): a time between two different keys is not a delivery time."""
    if [c for _, c in arrivals] != list(keys) or len(send_times) != len(keys):
        return []
    return [(arr_t - offset - t) * 1000.0 for (arr_t, _), t in zip(arrivals, send_times)]


# DSCP classes as the IP TOS byte `ss --tos` prints (DSCP << 2). CS1 is the lower-effort class OpenSSH <= 10.0 uses for a connection whose newest
# session has no tty (`IPQoS af21 cs1`): Wi-Fi sends it in the background queue, which is the 40-100 ms of #985.
TOS_CS1, TOS_AF21, TOS_EF = 0x20, 0x48, 0xB8
SS_USER_RE = re.compile(r'users:\(\("([^"]*)",pid=(\d+)')
SS_TOS_RE = re.compile(r"\btos:(0x[0-9a-fA-F]+|\d+)")


def ss_tos_by_pid(text):
    """pure: {pid: (program, TOS byte)} from `ss -Htnp --tos` output, one entry per socket line that names its owner and a tos (a line without either,
    the header included, is left out; a process with several sockets keeps its last line)"""
    out = {}
    for l in text.splitlines():
        u, t = SS_USER_RE.search(l), SS_TOS_RE.search(l)
        if u and t:
            out[int(u.group(2))] = (u.group(1), int(t.group(1), 0))
    return out


def ssh_conn_tos(port, timeout=10):
    """{pid: (program, TOS byte)} of this machine's TCP sockets connected to remote port `port` (Linux `ss`); None where `ss` is missing or fails (macOS,
    Windows: the marking is then not observed, which is said, never assumed)"""
    if not shutil.which("ss"):
        return None
    try:
        p = subprocess.run(["ss", "-H", "-t", "-n", "-p", "--tos", "dport", "=", ":%d" % int(port)], capture_output=True, text=True, timeout=timeout, stdin=subprocess.DEVNULL)
    except (OSError, subprocess.SubprocessError):
        return None
    return ss_tos_by_pid(p.stdout) if p.returncode == 0 else None


def count_starts(sshd_log_text, kinds=("command", "shell", "subsystem")):
    """sshd (LogLevel VERBOSE) 'Starting session: command for u from ip port N id K' lines -> {kind: count}; the exec channels the app opened"""
    out = {k: 0 for k in kinds}
    for m in re.finditer(r"Starting session: (\w+)", sshd_log_text):
        if m.group(1) in out:
            out[m.group(1)] += 1
    return out


def clock_offset(samples):
    """samples: [(t_send, remote_now, t_recv)] from /health -> remote_clock - local_clock, using the smallest round trip (NTP-style midpoint)"""
    best = min(samples, key=lambda s: s[2] - s[0])
    return best[1] - (best[0] + best[2]) / 2.0


def known_hosts_line(public, alias="sshlab"):
    f = public.split()
    return "%s %s %s\n" % (alias, f[0], f[1])


def ssh_config_text(host, port22, port2222, users, key, known_hosts, password_users=(), alias="sshlab"):
    """`Host lab-<user>` blocks for the harness's ssh_config (forward slashes: also valid for the Windows clients), plus lab-changedkey (port 2222, host key B)"""
    key, known_hosts = key.replace("\\", "/"), known_hosts.replace("\\", "/")
    common = ("  HostName %s\n  IdentityFile %s\n  IdentitiesOnly yes\n  UserKnownHostsFile %s\n  HostKeyAlias %s\n  StrictHostKeyChecking yes\n  LogLevel ERROR\n"
              % (host, key, known_hosts, alias))
    out = []
    for u in users:
        interactive = u in password_users or u == "p_mfa"
        out.append("Host lab-%s\n  Port %d\n  User %s\n%s  BatchMode %s\n%s\n" % (u, port22, u, common, "no" if interactive else "yes",
                                                                                   "  NumberOfPasswordPrompts 1\n" if interactive else ""))
    out.append("Host lab-changedkey\n  Port %d\n  User u_bash\n%s  BatchMode yes\n" % (port2222, common))
    return "".join(out)


# ------------------------------------------------------------------ remote-read adapters (ctx.remote)
class LocalRemote:
    """today's behaviour: the 'remote' is this machine, its files are prof.rhome / prof.rctl"""

    def __init__(self, prof):
        self.prof = prof

    def lines(self, name):
        for attempt in range(5):
            try:
                return open(os.path.join(self.prof.rhome, name), errors="replace").read().splitlines()
            except FileNotFoundError:
                return []
            except PermissionError:       # Windows: the shell still has the file open for writing
                time.sleep(0.05 * (attempt + 1))
        return []

    def rm(self, *names):
        for n in names:
            try:
                os.remove(os.path.join(self.prof.rhome, n))
            except FileNotFoundError:
                pass


class LabRemote:
    """the lab user's side over HTTP: effect files live in /work/<user>; a relative name that is not there is looked up in the home (`echo x >> ~/f`);
    the shell logs are the ctl dir's rec.log / prompt.log (fish: fish_rec.log)"""

    def __init__(self, lab, user, shell=None):
        self.lab, self.user = lab, user
        self.shell = shell or ("fish" if user == "u_fish" else "bash")

    def _read(self, p):
        try:
            return self.lab.file(p).decode("utf-8", "replace").splitlines()
        except LabError:
            return None

    def lines(self, name):
        if name.startswith("/"):
            return self._read(name) or []
        w = self._read("/work/%s/%s" % (self.user, name))
        return w if w is not None else (self._read("/home/%s/%s" % (self.user, name)) or [])

    def exists(self, name):
        """the file is on the host, EMPTY or not (lines() cannot tell an empty file from a missing one): a `touch` marker a running command left"""
        return self._read(name if name.startswith("/") else "/work/%s/%s" % (self.user, name)) is not None

    def rm(self, *names):
        for n in names:
            for base in ("/work/%s/" % self.user, "/home/%s/" % self.user):
                self.lab.rm_file(n if n.startswith("/") else base + n)

    def read(self, which):
        """rec.log / prompt.log / fish_rec.log of the user (Profile.read(remote=True) goes through here)"""
        return self.lab.ctl(self.user, which)["data"].splitlines()

    def commands(self):
        """[(epoch on the REMOTE clock, line)] the shell received, in order (fish: whole-second epoch)"""
        out = []
        for l in self.read("fish_rec.log" if self.shell == "fish" else "rec.log"):
            p = l.split("\t", 2)
            if len(p) == 3 and p[0] == "C":
                try:
                    out.append((float(p[1]), p[2]))
                except ValueError:
                    out.append((0.0, p[2]))
        return out

    def prompts(self):
        return len([l for l in self.read("prompt.log") if l.startswith("P")])


# ------------------------------------------------------------------ the client
class LabError(Exception):
    pass


def safe_text(text, secret):
    """text that may be printed: the secret (if any) replaced"""
    return text.replace(secret, "<token>") if secret else text


def _host_of(box):
    return box.rsplit("@", 1)[-1]


def _labctl():
    """labctl.py imported by path (its pure slot helpers; import-safe)"""
    import importlib.util
    spec = importlib.util.spec_from_file_location("labctl_mod", os.path.join(SSHLAB_DIR, "labctl.py"))
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


def default_slot(platform=None):
    p = platform or sys.platform
    return "mac" if p == "darwin" else "windows" if p.startswith(("win", "cygwin", "msys")) else "linux"


def slot_name():
    return (ci_env.get("APPCI_LAB_SLOT") or default_slot()).strip()


def slot_ports(name=None, spec=None):
    """-> {22: ssh, 2222: ssh-B, 7: echo, 7000: control} for the slot, from APPCI_LAB_SLOTS or the labctl default table. Raises LabError for an unknown slot."""
    name = name or slot_name()
    table = _labctl().parse_slots(spec or ci_env.get("APPCI_LAB_SLOTS") or _labctl().DEFAULT_SLOTS)
    if name not in table:
        raise LabError("no lab slot %r (known: %s; set APPCI_LAB_SLOT)" % (name, ", ".join(sorted(table))))
    return table[name]


def fixed_ready():
    """-> '' when the fixed mode has what it needs, else the reason (never contains the token)"""
    if not box_addr():
        return "SSHLAB_BOX (the lab box address) is not set"
    if not ci_env.get("SSHLAB_TOKEN"):
        return "SSHLAB_TOKEN (the protected project variable, or the machine-local ci.env) is not set: this job is not on a protected ref, or the variable is missing"
    return ""


def lab_mode():
    """-> (mode, reason): APPCI_LAB=off|local|fixed|remote; unset = local when podman is on PATH, else fixed when SSHLAB_BOX + SSHLAB_TOKEN are set, else off"""
    v = ci_env.get("APPCI_LAB", "").strip().lower()
    if v in ("off", "0", "no"):
        return "off", "APPCI_LAB=off"
    if v == "fixed":
        why = fixed_ready()
        return ("off", "APPCI_LAB=fixed but " + why) if why else ("fixed", "")
    if v == "remote":
        if not box_addr():
            return "off", "APPCI_LAB=remote but neither APPCI_LAB_BOX nor SSHLAB_BOX is set"
        if not ci_env.get("APPCI_LAB_KEY"):
            return "off", "APPCI_LAB=remote but APPCI_LAB_KEY (the forced-command key) is not set"
        kh = ci_env.get("APPCI_LAB_KNOWN_HOSTS", "")
        if not kh or not os.path.isfile(kh):
            return "off", "APPCI_LAB=remote but APPCI_LAB_KNOWN_HOSTS (a machine-local known_hosts file pinning the box's host key) is not set or not a file: the lab fails closed rather than trusting a key on first use"
        return "remote", ""
    if v in ("", "local"):
        if shutil.which("podman") and os.name != "nt":
            return "local", ""
        if v == "" and not fixed_ready():
            return "fixed", ""
        if v == "local":
            return "off", "APPCI_LAB=local but there is no podman on this machine"
        return "off", "no podman on this machine and the fixed lab is not configured (%s); set APPCI_LAB=fixed with SSHLAB_BOX and SSHLAB_TOKEN" % fixed_ready()
    return "off", "unknown APPCI_LAB=%r" % v


def box_addr():
    return ci_env.get("APPCI_LAB_BOX") or ci_env.get("SSHLAB_BOX") or ""


def sshlab_tar():
    """the client's sshlab tree as a tar (the remote verb hashes it to pick the image tag)"""
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w") as tf:
        tf.add(SSHLAB_DIR, arcname="sshlab", filter=lambda ti: None if (ti.name.endswith(".pyc") or "__pycache__" in ti.name) else ti)
    return buf.getvalue()


class Lab:
    def __init__(self, info, host, mode, key_for_box=None):
        self.info, self.host, self.mode, self.box_key = info, host, mode, key_for_box
        self.cid, self.token = info["cid"], info["token"]
        self.ports = {int(k): v for k, v in info["ports"].items()}
        self.key = None             # the lab user key (private path), set by lease()
        self.known_hosts = None
        self.leased = None
        self.home_cfg = None
        self.stopped = False
        self.offset = None
        self.owner = secrets.token_hex(12)        # this client's lease identity on a fixed slot (never printed)
        self.owns_lease = False

    @classmethod
    def from_info(cls, info, host, key=None, local=True):
        lab = cls(info, host, "local" if local else "remote")
        lab.key = key
        return lab

    # -- lifecycle
    @classmethod
    def start_fixed(cls, run):
        """-> (Lab, None) or (None, reason): attach to this platform's persistent slot on the box. Only HTTP to <box>:<control port>; nothing is started and no ssh runs."""
        why = fixed_ready()
        if why:
            return None, "the fixed lab is not configured: " + why
        try:
            ports = slot_ports()
        except (LabError, ValueError) as e:
            return None, str(e)
        info = dict(cid=None, ports=ports, token=ci_env.get("SSHLAB_TOKEN"), slot=slot_name(), image="slot:" + slot_name(), run=os.path.basename(os.path.normpath(run)), flavor="full", expires=0)
        lab = cls(info, _host_of(box_addr()), "fixed")
        lab.run = run
        try:
            h = lab.health()
        except LabError as e:
            return None, "the %s lab slot is not reachable (%s)" % (slot_name(), safe_text(str(e), lab.token))
        if h.get("leased") and h.get("exclusive"):
            return None, "the %s lab slot is busy (leased by another client, %ss left; 409): the lab cases SKIP rather than share a container" % (slot_name(), h.get("lease_left"))
        try:
            json.dump({"mode": "fixed", "slot": slot_name(), "run": info["run"]}, open(os.path.join(run, "lab.json"), "w"))       # no token, no address
        except OSError:
            pass
        return lab, None

    @classmethod
    def start(cls, run, flavor="full", ttl=3 * 3600):
        """-> (Lab, None) or (None, reason). local: runs labctl.py here; fixed: attach to the box's persistent slot; remote (legacy): ssh to the box with the forced-command key, the sshlab tree as a tar on stdin"""
        mode, why = lab_mode()
        if mode == "off":
            return None, why
        if mode == "fixed":
            return cls.start_fixed(run)
        runid = re.sub(r"[^A-Za-z0-9._-]", "-", os.path.basename(os.path.normpath(run)))[:48] or "run"
        try:
            if mode == "local":
                ctl = os.environ.get("APPCI_LAB_CTL") or os.path.join(SSHLAB_DIR, "labctl.py")
                p = subprocess.run([sys.executable, ctl, "start", "--run", runid, "--flavor", flavor, "--ttl", str(ttl)], capture_output=True, text=True, timeout=300)
                host = "127.0.0.1"
            else:
                box = box_addr()
                p = subprocess.run(["ssh", "-i", ci_env.get("APPCI_LAB_KEY"), "-o", "BatchMode=yes", "-o", "IdentitiesOnly=yes", "-o", "StrictHostKeyChecking=yes",
                                    "-o", "UserKnownHostsFile=" + ci_env.get("APPCI_LAB_KNOWN_HOSTS"), "-o", "ConnectTimeout=20", box,
                                    "start --run %s --flavor %s --ttl %d" % (runid, flavor, ttl)], input=sshlab_tar(), capture_output=True, timeout=300)
                p.stdout, p.stderr = p.stdout.decode(errors="replace"), p.stderr.decode(errors="replace")
                host = _host_of(box)
            if p.returncode:
                return None, "labctl start failed (%d): %s" % (p.returncode, (p.stderr or p.stdout).strip()[-400:])
            info = json.loads(p.stdout.strip().splitlines()[-1])
        except (OSError, ValueError, subprocess.TimeoutExpired) as e:
            return None, "the lab could not be started: %s" % e
        lab = cls(info, host, mode)
        lab.run = run
        try:
            json.dump({k: info[k] for k in ("cid", "run", "flavor", "image", "expires")} | {"mode": mode}, open(os.path.join(run, "lab.json"), "w"))     # no token, no address
        except OSError:
            pass
        return lab, None

    def stop(self):
        if self.stopped:
            return
        self.stopped = True
        try:
            if self.mode == "fixed":
                if self.owns_lease:          # never release a lease this client does not hold (a 409 on lease means somebody else's run is in there)
                    self._req("POST", "/release", dict(owner=self.owner), timeout=60)
                    self.owns_lease = False
                return
            stop_container(self.cid, self.mode, self.box_key)
        except Exception as e:  # noqa: BLE001
            print("lab: stop failed: %s (a fixed slot's lease also expires on its own)" % safe_text(repr(e), self.token), flush=True)

    # -- HTTP
    def _req(self, method, path, body=None, timeout=30, raw=False):
        data = json.dumps(body).encode() if body is not None else None
        r = urllib.request.Request("http://%s:%d%s" % (self.host, self.ports[7000], path), data=data, method=method,
                                   headers={"Authorization": "Bearer " + self.token, "Content-Type": "application/json"})
        try:
            resp = urllib.request.urlopen(r, timeout=timeout).read()
        except urllib.error.HTTPError as e:
            raise LabError("%s %s -> %d %s" % (method, path, e.code, safe_text(e.read()[:200].decode(errors="replace"), self.token)))
        except (urllib.error.URLError, OSError) as e:
            raise LabError("%s %s failed: %s" % (method, path, safe_text(str(e), self.token)))
        return resp if raw else json.loads(resp or b"{}")

    def health(self):
        return self._req("GET", "/health")

    def lease(self, pubkeys=None, hooks=None):
        """generate the run's lab user key (unless given), push keys + the shell hook templates, remember the secrets"""
        if pubkeys is None:
            self.make_key()
            pubkeys = [open(self.key + ".pub").read().strip()]
        if hooks is None:
            import profile
            hooks = profile.lab_hooks()
        self.leased = self._req("POST", "/lease", dict(run=os.path.basename(getattr(self, "run", "")), pubkeys=pubkeys, hooks=hooks, owner=self.owner), timeout=120)
        self.owns_lease = True
        if self.mode == "fixed":
            try:        # the owner id is not a secret without the token; teardown uses it to release a lease whose run crashed
                j = json.load(open(os.path.join(self.run, "lab.json")))
                j["owner"] = self.owner
                json.dump(j, open(os.path.join(self.run, "lab.json"), "w"))
            except (OSError, ValueError, AttributeError):
                pass
        return self.leased

    def make_key(self):
        d = os.path.join(getattr(self, "run", tempfile.gettempdir()), "labssh")
        os.makedirs(d, exist_ok=True)
        self.key = os.path.join(d, "labkey")
        if not os.path.exists(self.key):
            subprocess.run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", self.key], check=True, stdin=subprocess.DEVNULL)
        if os.name == "nt":
            user = os.environ.get("USERNAME", "runner")
            for f in (self.key,):
                subprocess.run(["icacls", f, "/inheritance:r", "/grant:r", "%s:F" % user], capture_output=True)
        return self.key

    def fault(self, rtt_ms=0, jitter_ms=0, loss_pct=0, rate_kbit=0):
        return self._req("POST", "/fault", dict(rtt_ms=rtt_ms, jitter_ms=jitter_ms, loss_pct=loss_pct, rate_kbit=rate_kbit))

    def clear_fault(self):
        return self._req("DELETE", "/fault")

    def drop(self, user, kind, seconds=10):
        return self._req("POST", "/drop", dict(user=user, kind=kind, seconds=seconds))

    def file(self, path):
        return self._req("GET", "/file?path=" + urllib.parse.quote(path), raw=True)

    def rm_file(self, path):
        try:
            self._req("DELETE", "/file?path=" + urllib.parse.quote(path))
        except LabError:
            pass

    def ctl(self, user, name, since=0):
        return self._req("GET", "/ctl?user=%s&name=%s&since=%d" % (user, name, since))

    def tree(self, user, files, dir="", git=False, branches=(), branch="main"):
        return self._req("PUT", "/tree", dict(user=user, dir=dir, branch=branch, files={k: base64.b64encode(v if isinstance(v, bytes) else v.encode()).decode() for k, v in files.items()},
                                              git=git, branches=list(branches)))

    def helper(self, user, files=None):
        if files is None:
            return self._req("DELETE", "/helper", dict(user=user))
        return self._req("POST", "/helper", dict(user=user, files={k: base64.b64encode(v).decode() for k, v in files.items()}))

    def complete(self, user, line):
        return self._req("POST", "/complete", dict(user=user, line=line))

    def sshd_since(self, since=0, port=22):
        return self._req("GET", "/sshdlog?since=%d&port=%d" % (since, port))

    def clock_offset(self, n=5):
        samples = []
        for _ in range(n):
            t0 = time.time()
            h = self.health()
            samples.append((t0, h.get("now", t0), time.time()))
        self.offset = clock_offset(samples)
        return self.offset

    # -- ssh
    def write_ssh(self, home):
        """write <home>/.ssh/lab_known_hosts and append the Host lab-* blocks to <home>/.ssh/config (after prof.build wrote its own). Returns the config text."""
        if not self.leased:
            raise LabError("lease first")
        d = os.path.join(home, ".ssh")
        os.makedirs(d, exist_ok=True)
        kh = os.path.join(d, "lab_known_hosts")
        open(kh, "w").write(known_hosts_line(self.leased["hostkeys"]["A"]["public"]))
        text = ssh_config_text(self.host, self.ports[22], self.ports[2222], [u["user"] for u in self.leased["users"]], self.key, kh, self.leased.get("password_users", ()))
        with open(os.path.join(d, "config"), "a", newline="\n") as f:
            f.write("\n" + text)
        self.home_cfg = os.path.join(d, "config")
        if os.name == "nt":
            subprocess.run(["icacls", self.home_cfg, "/inheritance:r", "/grant:r", "%s:F" % os.environ.get("USERNAME", "runner")], capture_output=True)
        return text

    def ssh(self, user, cmd, key=None, timeout=30):
        """a one-shot ssh as the lab user (BatchMode, no tty); used by selftest and helpers"""
        kh = os.path.join(tempfile.gettempdir(), "sshlab_kh_%d" % self.ports[22])
        open(kh, "w").write(known_hosts_line(self.leased["hostkeys"]["A"]["public"]) if self.leased else "")
        argv = ["ssh", "-i", key or self.key, "-p", str(self.ports[22]), "-o", "IdentitiesOnly=yes", "-o", "BatchMode=yes", "-o", "UserKnownHostsFile=" + kh,
                "-o", "HostKeyAlias=sshlab", "-o", "StrictHostKeyChecking=yes", "-o", "LogLevel=ERROR", "-o", "ConnectTimeout=15", "%s@%s" % (user, self.host), cmd]
        return subprocess.run(argv, capture_output=True, text=True, timeout=timeout, stdin=subprocess.DEVNULL)

    def measure_rtt(self, n=20, port=7):
        """echo-probe RTTs (ms) to the container's port 7 through the published port: the per-arm gate's evidence. The first sample (connect) is dropped."""
        out = []
        try:
            s = socket.create_connection((self.host, self.ports[port]), 5)
            s.setsockopt(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)
            s.settimeout(5)
            for i in range(n + 1):
                t0 = time.perf_counter()
                s.sendall(b"x")
                s.recv(1)
                if i:
                    out.append((time.perf_counter() - t0) * 1000.0)
                time.sleep(0.02)
            s.close()
        except OSError:
            pass
        return out

    def diag(self):
        """tails for ssh_diag(): /health and the last sshd log lines"""
        out = []
        try:
            h = self.health()
            out.append("lab /health: sshd=%s fault=%s tc=%s ttl_left=%s" % (h.get("sshd"), h.get("fault"), h.get("tc"), h.get("ttl_left")))
        except LabError as e:
            out.append("lab /health failed: %s" % e)
        try:
            d = self.sshd_since(0)
            out.append("lab sshd.log tail: " + " | ".join([l for l in d["data"].splitlines() if l.strip()][-8:]))
        except LabError as e:
            out.append("lab /sshdlog failed: %s" % e)
        return " ;; ".join(out)


def stop_container(cid, mode, box_key=None):
    if mode == "fixed":
        return          # a fixed slot is never stopped by a client; release() needs the lease owner (the Lab object) and the slot's lease expires by itself
    if mode == "local":
        ctl = os.environ.get("APPCI_LAB_CTL") or os.path.join(SSHLAB_DIR, "labctl.py")
        subprocess.run([sys.executable, ctl, "stop", "--id", cid], capture_output=True, timeout=60)
    else:
        subprocess.run(["ssh", "-i", ci_env.get("APPCI_LAB_KEY"), "-o", "BatchMode=yes", "-o", "IdentitiesOnly=yes", "-o", "StrictHostKeyChecking=yes",
                        "-o", "UserKnownHostsFile=" + os.environ.get("APPCI_LAB_KNOWN_HOSTS", os.devnull), box_addr(), "stop --id " + cid],
                       capture_output=True, timeout=60, stdin=subprocess.DEVNULL)


def stop_from_json(run):
    """teardown.sh / the run.py finally: stop the container named by <run>/lab.json (exact id; no token needed). True when something was asked to stop."""
    try:
        j = json.load(open(os.path.join(run, "lab.json")))
    except (OSError, ValueError):
        return False
    if j.get("mode") == "fixed":
        if not j.get("owner") or fixed_ready():
            return False
        lab = Lab(dict(cid=None, ports=slot_ports(j.get("slot")), token=ci_env.get("SSHLAB_TOKEN")), _host_of(box_addr()), "fixed")
        lab.owner, lab.owns_lease = j["owner"], True
        lab.stop()
        return True
    stop_container(j["cid"], j.get("mode", "local"))
    return True


# ------------------------------------------------------------------ selftest helpers (labctl selftest)
def mfa_login_ok(lab, key):
    """publickey + keyboard-interactive TOTP login as p_mfa (SSH_ASKPASS supplies the code)"""
    d = tempfile.mkdtemp(prefix="sshlab-askpass-")
    try:
        ap = os.path.join(d, "ask.sh")
        open(ap, "w").write("#!/bin/sh\necho %s\n" % totp(lab.leased["mfa_secret"]))
        os.chmod(ap, 0o700)
        kh = os.path.join(d, "kh")
        open(kh, "w").write(known_hosts_line(lab.leased["hostkeys"]["A"]["public"]))
        env = dict(os.environ, SSH_ASKPASS=ap, SSH_ASKPASS_REQUIRE="force", DISPLAY=os.environ.get("DISPLAY", "none"))
        p = subprocess.run(["ssh", "-i", key, "-p", str(lab.ports[22]), "-o", "IdentitiesOnly=yes", "-o", "UserKnownHostsFile=" + kh, "-o", "HostKeyAlias=sshlab",
                            "-o", "NumberOfPasswordPrompts=1", "-o", "LogLevel=ERROR", "p_mfa@" + lab.host, "echo MFA_OK"], env=env, capture_output=True, text=True,
                           timeout=60, stdin=subprocess.DEVNULL)
        return "MFA_OK" in p.stdout
    finally:
        shutil.rmtree(d, ignore_errors=True)


def onesess_refuses(lab, key, tmp):
    """p_onesess has MaxSessions 1: through one ControlMaster connection the second session must be refused"""
    cp = os.path.join(tmp, "cm_onesess")
    kh = os.path.join(tmp, "kh_onesess")
    open(kh, "w").write(known_hosts_line(lab.leased["hostkeys"]["A"]["public"]))
    base = ["ssh", "-i", key, "-p", str(lab.ports[22]), "-o", "IdentitiesOnly=yes", "-o", "UserKnownHostsFile=" + kh, "-o", "HostKeyAlias=sshlab", "-o", "BatchMode=yes",
            "-o", "LogLevel=ERROR", "-o", "ControlPath=" + cp]
    m = subprocess.Popen(base + ["-o", "ControlMaster=yes", "p_onesess@" + lab.host, "sleep 20"], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(50):
            if os.path.exists(cp):
                break
            time.sleep(0.2)
        p = subprocess.run(base + ["-o", "ControlMaster=no", "p_onesess@" + lab.host, "echo SECOND"], capture_output=True, text=True, timeout=30, stdin=subprocess.DEVNULL)
        # ssh answers a refused multiplexed session with "Session open refused by peer" and then RETRIES on a fresh connection (which MaxSessions does not limit), so the
        # fallback may still print SECOND: the refusal itself, on stderr, is the evidence
        return "refused by peer" in p.stderr or "SECOND" not in p.stdout
    finally:
        m.kill()


# ------------------------------------------------------------------ plain ssh control (POSIX): the paired baseline
class PlainSsh:
    """`ssh -tt -o ControlMaster=no -F <cfg> lab-<user>` in a pty, driven like a person: per-key echo times and Enter -> record times. Nothing of Phosphor
    is involved, so it measures the path + the remote shell only. POSIX only (None from open() elsewhere: the paired verdict is then UNVERIFIABLE)."""

    def __init__(self, cfg, user, env=None):
        self.cfg, self.user, self.fd, self.pid, self.buf = cfg, user, None, None, bytearray()
        self.env = env or {}

    def open(self, timeout=30):
        if os.name == "nt" or not shutil.which("ssh"):
            return False
        import pty, fcntl, termios
        pid, fd = pty.fork()
        if pid == 0:
            try:
                os.execvpe("ssh", ["ssh", "-tt", "-o", "ControlMaster=no", "-o", "ControlPath=none", "-F", self.cfg, "lab-" + self.user], dict(os.environ, TERM="xterm-256color", **self.env))
            finally:
                os._exit(127)
        self.pid, self.fd = pid, fd
        fcntl.fcntl(fd, fcntl.F_SETFL, fcntl.fcntl(fd, fcntl.F_GETFL) | os.O_NONBLOCK)
        end = time.perf_counter() + timeout
        while time.perf_counter() < end:          # the first prompt is "whatever the shell prints, then quiet for 0.7 s" (bash $, zsh %, fish >, nu, pwsh)
            n = len(self.buf)
            self._pump(0.7)
            if len(self.buf) == n and n:
                return True
        return False

    def _pump(self, dur):
        import select
        end = time.perf_counter() + dur
        while True:
            left = end - time.perf_counter()
            if left <= 0:
                return
            r, _, _ = select.select([self.fd], [], [], min(left, 0.01))
            if r:
                try:
                    d = os.read(self.fd, 65536)
                except (BlockingIOError, OSError):
                    d = b""
                if d:
                    self.buf += d

    def wait_for(self, needle, timeout, start=0):
        """seconds (perf_counter timestamp) when `needle` first appears in the output after offset `start`, else None"""
        end = time.perf_counter() + timeout
        while time.perf_counter() < end:
            if needle in self.buf[start:]:
                return time.perf_counter()
            self._pump(0.005)
        return None

    def key_echo(self, ch=b"q", timeout=5):
        """send one key, return the ms until it is echoed back (the shell's own echo through the path)"""
        start = len(self.buf)
        t0 = time.perf_counter()
        os.write(self.fd, ch)
        t1 = self.wait_for(ch, timeout, start)
        os.write(self.fd, b"\x15")      # Ctrl-U: clear the line
        self._pump(0.05)
        return None if t1 is None else (t1 - t0) * 1000.0

    def run_line(self, text, gap_ms=(10, 25), rng=None):
        """type `text` at 10-25 ms/key and Enter at once; returns the wall time (time.time()) of the Enter write: the caller reads the record time from the remote rec.log"""
        import random
        rng = rng or random.Random(1)
        for ch in text.encode():
            os.write(self.fd, bytes([ch]))
            self._pump(rng.uniform(*gap_ms) / 1000.0)
        t = time.time()
        os.write(self.fd, b"\r")
        return t

    def send_key(self, ch):
        """write ONE key to the session, returns the wall time (time.time()) right before the write: the plain twin of the app's pty write (#985)"""
        t = time.time()
        os.write(self.fd, ch if isinstance(ch, bytes) else ch.encode())
        return t

    def close(self):
        import signal
        try:
            os.kill(self.pid, signal.SIGKILL)
            os.waitpid(self.pid, 0)
        except OSError:
            pass
        try:
            os.close(self.fd)
        except OSError:
            pass


if __name__ == "__main__":
    # lab.py stop <run dir>: teardown.sh / after_job.ps1 stop the container named by <run dir>/lab.json (exact id)
    if len(sys.argv) == 3 and sys.argv[1] == "stop":
        print("lab stop: %s" % ("asked to stop the container in lab.json" if stop_from_json(sys.argv[2]) else "no lab.json in %s (nothing to stop)" % sys.argv[2]))
    else:
        sys.exit("usage: lab.py stop <run dir>")
