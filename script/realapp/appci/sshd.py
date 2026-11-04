# appci/sshd.py: a local ssh server on 127.0.0.1 so the ssh cases need no external host.
# Preferred: the system OpenSSH sshd run as the CI user (no root: pubkey only, ForceCommand starts bash with
# HOME = the scratch "remote" home so the real home is never touched). Fallback: script/realapp/linux/bin/fakesshd.py
# when python3-paramiko is importable. Neither available -> start() returns None and the ssh cases SKIP.
import os, shutil, socket, subprocess, sys, time

PW_PASSPHRASE = "ppCI-7431-test"       # protects ONLY the throw-away key below; it exists to give the ssh cases a real tty prompt (cases_caseremote.py)
# #1535: a REAL password prompt ("pwuser@127.0.0.1's password:") without an account or PAM: the paramiko fake sshd accepts exactly this one user + password
# (env FAKE_PW_USER / FAKE_PASSWORD) and nothing else. Throw-away values, valid only for the harness's own loopback server.
PW_USER = "pwuser"
PW_PASSWORD = "pwCI-9482-test"
HERE = os.path.dirname(os.path.abspath(__file__))


def _free_port():
    s = socket.socket(); s.bind(("127.0.0.1", 0)); p = s.getsockname()[1]; s.close(); return p


def _start_windows(prof, run, d, port):
    """Windows: fakesshd_win.py (paramiko + a ConPTY running Git Bash) on 127.0.0.1; the Windows OpenSSH Server service is never used.
    The scratch keys come from the built-in ssh-keygen.exe (else a paramiko RSA key). None -> the ssh cases SKIP with the reason in Windows_why."""
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
    p = subprocess.Popen([sys.executable, os.path.join(HERE, "fakesshd_win.py"), str(port), d, prof.rhome, bash, log],
                         stdout=subprocess.DEVNULL, stderr=open(os.path.join(d, "fakesshd.stderr"), "w"), stdin=subprocess.DEVNULL,
                         env=dict(os.environ, FAKE_PASSWORD=PW_PASSWORD, FAKE_PW_USER=PW_USER))
    for _ in range(50):
        try:
            socket.create_connection(("127.0.0.1", port), 0.2).close()
            return {"port": port, "ident": os.path.join(d, "clientkey"), "kind": "fakesshd-win", "popen": p,
                    "pwhost": {"port": port, "user": PW_USER, "password": PW_PASSWORD}}      # the same server also takes the password login (#1535)
        except OSError:
            if p.poll() is not None:
                break
            time.sleep(0.1)
    p.kill()
    WHY = "fakesshd_win.py did not listen on 127.0.0.1:%d (see ssh/fakesshd.stderr)" % port
    return None


WHY = None      # why start() returned None on Windows (run.py reports it as the skip reason)


def start(prof, run):
    """returns dict(port, ident, kind, popen) or None"""
    d = os.path.join(run, "ssh"); os.makedirs(d, exist_ok=True)
    port = _free_port()
    if os.name == "nt":
        return _start_windows(prof, run, d, port)
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
    if info and info.get("popen") and info["popen"].poll() is None:
        if os.name == "nt":      # taskkill /T also ends the ConPTY bash children of the fake server; exact PID only
            subprocess.run(["taskkill", "/PID", str(info["popen"].pid), "/T", "/F"], capture_output=True, timeout=30)
        info["popen"].terminate()
        try:
            info["popen"].wait(5)
        except Exception:
            info["popen"].kill()
