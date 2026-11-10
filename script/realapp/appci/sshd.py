# appci/sshd.py: a local ssh server on 127.0.0.1 so the ssh cases need no external host.
# Preferred: the system OpenSSH sshd run as the CI user (no root: pubkey only, ForceCommand starts bash with
# HOME = the scratch "remote" home so the real home is never touched). Fallback: script/realapp/linux/bin/fakesshd.py
# when python3-paramiko is importable. Neither available -> start() returns None and the ssh cases SKIP in a local run; where the host is
# required (required(): CI, or APPCI_REQUIRE_SSH=1) run.py emits a FAIL row `ssh_test_host` and the ssh cases are NOREF (#2043).
import os, shutil, socket, subprocess, sys, time

PW_PASSPHRASE = "ppCI-7431-test"       # protects ONLY the throw-away key below; it exists to give the ssh cases a real tty prompt (cases_caseremote.py)
# #1535: a REAL password prompt ("pwuser@127.0.0.1's password:") without an account or PAM: the paramiko fake sshd accepts exactly this one user + password
# (env FAKE_PW_USER / FAKE_PASSWORD) and nothing else. Throw-away values, valid only for the harness's own loopback server.
PW_USER = "pwuser"
PW_PASSWORD = "pwCI-9482-test"
HERE = os.path.dirname(os.path.abspath(__file__))


def _free_port():
    s = socket.socket(); s.bind(("127.0.0.1", 0)); p = s.getsockname()[1]; s.close(); return p


def _start_pty_host(d, rhome, bash, clientkey, hostkey):
    """#2007: Git for Windows' own sshd.exe on a second loopback port, as the pty host of the fake sshd's exec + pty (ptyhost.py): it gives the
    remote command a real msys pty, which passes every byte, where a pseudo console does not. Runs as the CI user: the scratch client key only,
    loopback only, every session forced into <ssh>/ptyhost/force.sh (HOME = the scratch remote home). Never the system's or the user's sshd, and
    nothing outside the run directory is written. -> (dict(port, popen, log), None) or (None, why); never raises."""
    try:
        import ptyhost
        root = os.path.dirname(os.path.dirname(bash))            # ...\Git\bin\bash.exe -> ...\Git
        usr = os.path.join(root, "usr", "bin")
        sshd_exe, ubash = os.path.join(usr, "sshd.exe"), os.path.join(usr, "bash.exe")
        if not (os.path.isfile(sshd_exe) and os.path.isfile(ubash)):
            return None, "Git for Windows has no usr\\bin\\sshd.exe next to its bash (%s)" % usr
        h = os.path.join(d, "ptyhost")
        os.makedirs(h, exist_ok=True)
        mp = ptyhost.msys_path
        with open(os.path.join(h, "authorized_keys"), "w", newline="\n") as f:
            f.write(ptyhost.pubkey_line(ptyhost.load_key(clientkey)))
        with open(os.path.join(h, "force.sh"), "w", newline="\n") as f:
            f.write(ptyhost.force_script(mp(rhome)))
        # its own copy of the scratch host key, readable by this user only: where msys maps NTFS ACLs to modes, sshd refuses a key others can read
        hk = os.path.join(h, "hostkey")
        shutil.copyfile(hostkey, hk)
        subprocess.run(["icacls", hk, "/inheritance:r", "/grant:r", "%s:F" % os.environ.get("USERNAME", "runner")], capture_output=True)
        port = _free_port()
        conf = os.path.join(h, "sshd_config")
        with open(conf, "w", newline="\n") as f:
            f.write(ptyhost.sshd_config(port, mp(hk), mp(os.path.join(h, "authorized_keys")), mp(os.path.join(h, "sshd.pid")), mp(os.path.join(h, "force.sh"))))
        log = os.path.join(h, "sshd.log")
        # through bash so that sshd's argv[0] is the absolute msys path it insists on ("sshd re-exec requires execution with an absolute path");
        # usr\bin first on PATH so the remote command finds stty / dd / od / tr / md5sum the way a Git Bash user's shell does
        p = subprocess.Popen([ubash, "-c", 'exec /usr/bin/sshd -D -e -f "$0"', mp(conf)], stdout=subprocess.DEVNULL, stderr=open(log, "w"),
                             stdin=subprocess.DEVNULL, env=dict(os.environ, PATH=usr + os.pathsep + os.environ.get("PATH", "")))
        for _ in range(80):
            try:
                socket.create_connection(("127.0.0.1", port), 0.2).close()
                return {"port": port, "popen": p, "log": log}, None
            except OSError:
                if p.poll() is not None:
                    break
                time.sleep(0.1)
        try:
            tail = " | ".join([l.strip() for l in open(log, errors="replace").read().splitlines() if l.strip()][-3:])
        except OSError:
            tail = ""
        _kill_tree(p)
        return None, "Git for Windows' sshd.exe did not listen on 127.0.0.1:%d (%s)" % (port, tail or "no output")
    except Exception as e:
        return None, "the pty host could not be set up: %r" % (e,)


def stderr_tail(path, lines=3, cap=500):
    """the last non-empty lines of a server's stderr file as one line (the exception line of a traceback is the last one); "" when unreadable"""
    try:
        ls = [l.strip() for l in open(path, errors="replace").read().splitlines() if l.strip()]
    except OSError:
        return ""
    return " | ".join(ls[-lines:])[-cap:]


def required(env=None):
    """#2043: must this run have a usable ssh test host? Yes in CI (GitLab sets CI) and with APPCI_REQUIRE_SSH=1: every CI test machine is set up
    to have one, so a host that does not start there is a harness or machine fault that took a whole test area with it, and run.py reports it as
    a FAIL row (`ssh_test_host`) with the ssh cases NOREF. A local run on a machine without sshd keeps the plain skip."""
    env = os.environ if env is None else env
    return env.get("APPCI_REQUIRE_SSH") == "1" or bool(env.get("CI"))


def host_missing_row(label, why, stderr_path=None):
    """#2043: the FAIL row of a pass that has no usable ssh test host where one is required. A TEST / machine verdict, never an app one."""
    tail = stderr_tail(stderr_path) if stderr_path else ""
    detail = ["TEST HOST MISSING (harness or test machine, not an app verdict): the %s pass has no usable ssh test host, so none of its ssh cases "
              "was judged (each is NOREF): %s" % (label, why or NO_HOST)]
    if tail and tail not in detail[0]:
        detail.append("ssh/fakesshd.stderr: " + tail)
    return dict(name="ssh_test_host", mode=label, status="fail", seconds=0, metrics={}, shots=[], detail=detail)


NO_HOST = "no local ssh server (needs /usr/sbin/sshd + ssh-keygen, i.e. apt package openssh-server; or python3-paramiko for fakesshd.py)"


def _kill_tree(p):
    """Windows: end exactly this process and what it started (the msys bash stub, sshd and its sessions; the fake server's console children)"""
    if p is None or p.poll() is not None:
        return
    try:
        subprocess.run(["taskkill", "/PID", str(p.pid), "/T", "/F"], capture_output=True, timeout=30)
    except Exception:
        pass
    try:
        p.wait(5)
    except Exception:
        try:
            p.kill()
        except Exception:
            pass


def _measure_exec_pty(d, port, ident, arms, notes):
    """#2007: measure every available exec + pty arm of the fake sshd through the server itself (ptyhost.probe: a long OSC out, a counted raw read
    in), write the arm that serves from now on to <ssh>/exec_pty_arm, and return the one-line result (job log, ssh_diag). Never raises: a probe
    that cannot run leaves the server on its own order of what is available."""
    try:
        import ptyhost
        user = os.environ.get("USERNAME", "tester")
        results = [ptyhost.probe(port, user, ident, a) for a in arms]
        chosen = ptyhost.choose_arm(results)
        if chosen:
            with open(os.path.join(d, "exec_pty_arm"), "w") as f:
                f.write(chosen + "\n")
        return ptyhost.summary(results, chosen, notes)
    except Exception as e:
        return "exec+pty of the test sshd was not measured: %r%s" % (e, ("; " + "; ".join(notes)) if notes else "")


def _start_windows(prof, run, d, port, app_bin=None):
    """Windows: fakesshd_win.py (paramiko + a ConPTY running Git Bash) on 127.0.0.1; the Windows OpenSSH Server service is never used.
    The scratch keys come from the built-in ssh-keygen.exe (else a paramiko RSA key). None -> the ssh cases SKIP with the reason in Windows_why.
    #2007: exec + pty (a managed ssh session) is served by the first arm that measures byte-exact: an msys pty on the pty host, the app's own
    conpty.dll (next to `app_bin`), or kernel32's pseudo console; the measurement is in the returned dict as "exec_pty"."""
    global WHY
    WHY = None
    import winapi
    try:
        import paramiko  # noqa: F401
    except ImportError:
        WHY = "paramiko is not importable by this Python (%s): fakesshd_win.py cannot run" % sys.executable
        return None
    bash = winapi.git_bash()
    if not bash:
        WHY = "Git for Windows (bash.exe) not found: the fake ssh server's remote shell is Git Bash"
        return None
    kg = shutil.which("ssh-keygen", path=os.path.join(os.environ.get("SystemRoot", r"C:\Windows"), "System32", "OpenSSH")) or shutil.which("ssh-keygen")
    for k in ("hostkey", "clientkey"):
        kp = os.path.join(d, k)
        if os.path.exists(kp):
            continue
        if kg:
            subprocess.run([kg, "-q", "-t", "ed25519", "-N", "", "-f", kp], check=True, stdin=subprocess.DEVNULL)
        else:
            paramiko.RSAKey.generate(2048).write_private_key_file(kp)
    log = os.path.join(d, "sshd.log")
    ident = os.path.join(d, "clientkey")
    arm_file = os.path.join(d, "exec_pty_arm")
    try:
        os.remove(arm_file)             # a pass never inherits the arm an earlier pass measured
    except OSError:
        pass
    notes, arms = [], ["inbox"]
    host, why = _start_pty_host(d, prof.rhome, bash, ident, os.path.join(d, "hostkey"))
    if host:
        arms.insert(0, "msys")
    else:
        notes.append("no msys pty host: %s" % why)
    dll = os.path.join(os.path.dirname(os.path.abspath(app_bin)), "conpty.dll") if app_bin else ""
    if dll and os.path.isfile(dll):
        arms.insert(len(arms) - 1, "app")
    else:
        notes.append("no app conpty.dll (%s)" % (dll or "the app binary's folder is not known here"))
        dll = ""
    env = dict(os.environ, FAKE_PASSWORD=PW_PASSWORD, FAKE_PW_USER=PW_USER, FAKE_ARM_FILE=arm_file, FAKE_CONPTY_DLL=dll,
               FAKE_PTY_PORT=str(host["port"]) if host else "0")
    p = subprocess.Popen([sys.executable, os.path.join(HERE, "fakesshd_win.py"), str(port), d, prof.rhome, bash, log],
                         stdout=subprocess.DEVNULL, stderr=open(os.path.join(d, "fakesshd.stderr"), "w"), stdin=subprocess.DEVNULL, env=env)
    for _ in range(50):
        try:
            socket.create_connection(("127.0.0.1", port), 0.2).close()
            exec_pty = _measure_exec_pty(d, port, ident, arms, notes)
            print("ssh test host: %s" % exec_pty, flush=True)
            return {"port": port, "ident": ident, "kind": "fakesshd-win", "popen": p, "ptyhost": host, "exec_pty": exec_pty,
                    "pwhost": {"port": port, "user": PW_USER, "password": PW_PASSWORD}}      # the same server also takes the password login (#1535)
        except OSError:
            if p.poll() is not None:
                break
            time.sleep(0.1)
    p.kill()
    if host:
        _kill_tree(host["popen"])
    # #2043: the reason itself goes into the row (rounds 10 and 11 said only "see ssh/fakesshd.stderr", and the cause was one line in that file)
    WHY = "fakesshd_win.py did not listen on 127.0.0.1:%d (ssh/fakesshd.stderr: %s)" % (port, stderr_tail(os.path.join(d, "fakesshd.stderr")) or "empty")
    return None


WHY = None      # why start() returned None on Windows (run.py reports it as the skip reason)


def start(prof, run, app_bin=None):
    """returns dict(port, ident, kind, popen) or None. `app_bin` (Windows): the app binary, whose folder holds the app's own conpty.dll (#2007)."""
    d = os.path.join(run, "ssh"); os.makedirs(d, exist_ok=True)
    port = _free_port()
    if os.name == "nt":
        return _start_windows(prof, run, d, port, app_bin)
    sshd = shutil.which("sshd") or ("/usr/sbin/sshd" if os.path.exists("/usr/sbin/sshd") else None)
    kg = shutil.which("ssh-keygen")
    if sshd and kg:
        for k in ("hostkey", "clientkey"):
            if not os.path.exists(os.path.join(d, k)):
                subprocess.run([kg, "-q", "-t", "ed25519", "-N", "", "-f", os.path.join(d, k)], check=True)
        # second client key, protected by a passphrase: `ssh fakepw` stops at "Enter passphrase for key" on the tty, the only password-style prompt an unprivileged sshd can give
        if not os.path.exists(os.path.join(d, "pwkey")):
            subprocess.run([kg, "-q", "-t", "ed25519", "-N", PW_PASSPHRASE, "-f", os.path.join(d, "pwkey")], check=True)
        with open(os.path.join(d, "authorized_keys"), "w") as ak:
            ak.write(open(os.path.join(d, "clientkey.pub")).read())
            ak.write(open(os.path.join(d, "pwkey.pub")).read())
        os.chmod(os.path.join(d, "authorized_keys"), 0o600)
        force = os.path.join(d, "force.sh")
        # Behave like sshd for a user whose login shell is bash (the remote home is set up for bash): sshd sets SHELL to the login
        # shell and runs a command with `$SHELL -c`. Without SHELL=/bin/bash the Mac's gitrunner SHELL (/bin/zsh) leaked through, so
        # Phosphor's ssh wrapper sent its zsh bootstrap, which this script ran under bash ("local: can only be used in a function").
        open(force, "w").write('#!/bin/bash\nexport HOME="%s" SHELL=/bin/bash\ncd "$HOME"\nif [ -n "$SSH_ORIGINAL_COMMAND" ]; then exec "$SHELL" -c "$SSH_ORIGINAL_COMMAND"; else exec "$SHELL" -l -i; fi\n' % prof.rhome)
        os.chmod(force, 0o755)
        conf = os.path.join(d, "sshd_config")
        open(conf, "w").write("\n".join([
            "Port %d" % port, "ListenAddress 127.0.0.1", "HostKey %s/hostkey" % d, "PidFile %s/sshd.pid" % d,
            "AuthorizedKeysFile %s/authorized_keys" % d, "PasswordAuthentication no", "KbdInteractiveAuthentication no",
            "UsePAM no", "StrictModes no", "PubkeyAuthentication yes", "ForceCommand " + force, "LogLevel DEBUG1",
            "PermitTTY yes", "X11Forwarding no", ""]))
        log = open(os.path.join(d, "sshd.log"), "w")
        # r18d8: LogLevel DEBUG1 (was ERROR) so an ssh case that gets no remote shell leaves the connection/auth/session lines in sshd.log
        # (an empty sshd.log could not tell 'ssh never reached sshd' from 'connected, then no shell')
        p = subprocess.Popen([sshd, "-D", "-e", "-f", conf], stdout=log, stderr=log, start_new_session=True)
        for _ in range(50):
            try:
                socket.create_connection(("127.0.0.1", port), 0.2).close(); break
            except OSError:
                if p.poll() is not None:
                    break
                time.sleep(0.1)
        else:
            p.kill()
        if p.poll() is None:
            return {"port": port, "ident": os.path.join(d, "clientkey"), "kind": "openssh", "popen": p,
                    "pwident": os.path.join(d, "pwkey"), "passphrase": PW_PASSPHRASE, "pwhost": _pw_host(prof, run, kg, p)}
    p, ident = _fakesshd_linux(prof, run, "fk", port, kg, PW_PASSWORD)       # this server takes the password login itself: no second one
    if not p:
        return None
    return {"port": port, "ident": ident, "kind": "fakesshd", "popen": p, "pwhost": {"port": port, "user": PW_USER, "password": PW_PASSWORD}}


def _fakesshd_linux(prof, run, sub, port, kg, password=None):
    """start script/realapp/linux/bin/fakesshd.py (paramiko) on 127.0.0.1:port with its scratch keys under <run>/<sub>; -> (Popen, client key path) or (None, None).
    `password`: the one password the server accepts for PW_USER (None = passwords always rejected)."""
    try:
        import paramiko  # noqa: F401
    except ImportError:
        return None, None
    if not kg:
        return None, None
    fk = os.path.join(run, sub); os.makedirs(fk + "/fakessh", exist_ok=True); os.makedirs(fk + "/raw", exist_ok=True)
    for k in ("hostkey", "clientkey"):
        if not os.path.exists(fk + "/fakessh/" + k):
            subprocess.run([kg, "-q", "-t", "ed25519", "-N", "", "-f", fk + "/fakessh/" + k], check=True)
    if os.path.islink(fk + "/remote_home") or os.path.exists(fk + "/remote_home"):
        pass
    else:
        os.symlink(prof.rhome, fk + "/remote_home")
    env = dict(os.environ, PHOSPHOR_LINUX_RUN=fk)
    if password:
        env.update(FAKE_PASSWORD=password, FAKE_PW_USER=PW_USER)
    p = subprocess.Popen([sys.executable, os.path.join(HERE, "..", "linux", "bin", "fakesshd.py"), str(port)], env=env,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
    time.sleep(1.5)
    if p.poll() is not None:
        return None, None
    return p, fk + "/fakessh/clientkey"


def _pw_host(prof, run, kg, main_popen):
    """#1535: the password-prompt host next to the system OpenSSH sshd (pubkey only: no PAM and no account password as an unprivileged user): a second
    paramiko fake sshd on its own port that accepts exactly PW_USER / PW_PASSWORD. None when paramiko is not importable (the case then covers only the
    passphrase prompt). (The fallback fake sshd and the Windows one serve the password themselves.)"""
    port = _free_port()
    p, _ident = _fakesshd_linux(prof, run, "fkpw", port, kg, PW_PASSWORD)
    if not p:
        return None
    return {"port": port, "user": PW_USER, "password": PW_PASSWORD, "popen": p}


def stop(info):
    pw = (info or {}).get("pwhost") or {}
    if pw.get("popen") and pw["popen"].poll() is None:      # the second (password) fake sshd of the system-sshd setup; exact PID, own session
        pw["popen"].terminate()
        try:
            pw["popen"].wait(5)
        except Exception:
            pw["popen"].kill()
    host = (info or {}).get("ptyhost") or {}
    if os.name == "nt" and host.get("popen") is not None:      # #2007: the msys sshd pty host of the Windows fake sshd; exact PID and its children
        _kill_tree(host["popen"])
    if info and info.get("popen") and info["popen"].poll() is None:
        if os.name == "nt":      # taskkill /T also ends the ConPTY bash children of the fake server; exact PID only
            subprocess.run(["taskkill", "/PID", str(info["popen"].pid), "/T", "/F"], capture_output=True, timeout=30)
        info["popen"].terminate()
        try:
            info["popen"].wait(5)
        except Exception:
            info["popen"].kill()
