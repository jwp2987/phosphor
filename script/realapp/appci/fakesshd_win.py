#!/usr/bin/env python3
# appci/fakesshd_win.py: the Windows twin of script/realapp/linux/bin/fakesshd.py (that one needs pty/fcntl/termios and /bin/bash).
# A paramiko ssh server on 127.0.0.1:PORT for the ssh cases; NO OpenSSH Server service is involved. Public-key auth for the one scratch
# client key; passwords are rejected EXCEPT when FAKE_PASSWORD is set: then user FAKE_PW_USER (default pwuser) with exactly that password logs in
# (#1535: a real password prompt for the Windows ssh clients, no account needed). A shell request gets a REAL pseudo console (ConPTY, CreatePseudoConsole through ctypes: the same
# mechanism Windows' own sshd uses) running Git Bash with HOME = the scratch remote home and the scratch rc (--rcfile <rhome>/.bashrc), so the
# remote side logs prompt.log / rec.log exactly like the POSIX sshd.py does. An exec request runs `bash -c <cmd>` on pipes.
# usage: fakesshd_win.py PORT KEYDIR REMOTE_HOME BASH_EXE LOGFILE      (KEYDIR holds hostkey + clientkey)
import ctypes
import msvcrt
import os
import socket
import subprocess
import sys
import threading
import time
from ctypes import wintypes as wt

import paramiko

PORT, KEYDIR, RHOME, BASH, LOGF = int(sys.argv[1]), sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5]
PW = os.environ.get("FAKE_PASSWORD", "")
PWU = os.environ.get("FAKE_PW_USER", "pwuser")
LAT = float(os.environ.get("FAKE_LAT", "0"))        # artificial connect delay before auth (s)
LOG = open(LOGF, "a", buffering=1)


def log(*a):
    print(time.strftime("%H:%M:%S"), *a, file=LOG)


def load_key(path):
    for cls in (paramiko.Ed25519Key, paramiko.RSAKey, paramiko.ECDSAKey):
        try:
            return cls(filename=path)
        except Exception:
            pass
    raise SystemExit("cannot load key " + path)


HK = load_key(os.path.join(KEYDIR, "hostkey"))
CK = load_key(os.path.join(KEYDIR, "clientkey"))

# ------------------------------------------------------------------ ConPTY
k32 = ctypes.WinDLL("kernel32", use_last_error=True)
# r18ct: sshd.py starts this process with stdin/stdout = NUL and stderr = fakesshd.stderr, and the ConPTY Git Bash children wrote their prompt and echo into that
# stderr FILE (it holds "bash: cannot set terminal process group ... ~$ exit") instead of the pseudo console, so the ssh client never saw a byte. Python keeps its own
# fds, so clearing the process's Win32 standard handles loses nothing here and leaves the children nothing to inherit.
k32.SetStdHandle.argtypes = [wt.DWORD, ctypes.c_void_p]
for _h in (-10, -11, -12):          # STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE
    k32.SetStdHandle(_h & 0xFFFFFFFF, None)


class COORD(ctypes.Structure):
    _fields_ = [("X", ctypes.c_short), ("Y", ctypes.c_short)]


class STARTUPINFO(ctypes.Structure):
    _fields_ = [("cb", wt.DWORD), ("lpReserved", wt.LPWSTR), ("lpDesktop", wt.LPWSTR), ("lpTitle", wt.LPWSTR), ("dwX", wt.DWORD), ("dwY", wt.DWORD),
                ("dwXSize", wt.DWORD), ("dwYSize", wt.DWORD), ("dwXCountChars", wt.DWORD), ("dwYCountChars", wt.DWORD), ("dwFillAttribute", wt.DWORD),
                ("dwFlags", wt.DWORD), ("wShowWindow", wt.WORD), ("cbReserved2", wt.WORD), ("lpReserved2", ctypes.c_void_p),
                ("hStdInput", ctypes.c_void_p), ("hStdOutput", ctypes.c_void_p), ("hStdError", ctypes.c_void_p)]


class STARTUPINFOEX(ctypes.Structure):
    _fields_ = [("StartupInfo", STARTUPINFO), ("lpAttributeList", ctypes.c_void_p)]


class PROCESS_INFORMATION(ctypes.Structure):
    _fields_ = [("hProcess", ctypes.c_void_p), ("hThread", ctypes.c_void_p), ("dwProcessId", wt.DWORD), ("dwThreadId", wt.DWORD)]


k32.CreatePipe.argtypes = [ctypes.POINTER(ctypes.c_void_p), ctypes.POINTER(ctypes.c_void_p), ctypes.c_void_p, wt.DWORD]
k32.CreatePseudoConsole.argtypes = [COORD, ctypes.c_void_p, ctypes.c_void_p, wt.DWORD, ctypes.POINTER(ctypes.c_void_p)]
k32.CreatePseudoConsole.restype = ctypes.c_long
k32.ResizePseudoConsole.argtypes = [ctypes.c_void_p, COORD]
k32.ClosePseudoConsole.argtypes = [ctypes.c_void_p]
k32.InitializeProcThreadAttributeList.argtypes = [ctypes.c_void_p, wt.DWORD, wt.DWORD, ctypes.POINTER(ctypes.c_size_t)]
k32.UpdateProcThreadAttribute.argtypes = [ctypes.c_void_p, wt.DWORD, ctypes.c_size_t, ctypes.c_void_p, ctypes.c_size_t, ctypes.c_void_p, ctypes.c_void_p]
k32.CreateProcessW.argtypes = [wt.LPCWSTR, wt.LPWSTR, ctypes.c_void_p, ctypes.c_void_p, wt.BOOL, wt.DWORD, ctypes.c_void_p, wt.LPCWSTR,
                               ctypes.POINTER(STARTUPINFOEX), ctypes.POINTER(PROCESS_INFORMATION)]
k32.WaitForSingleObject.argtypes = [ctypes.c_void_p, wt.DWORD]
k32.TerminateProcess.argtypes = [ctypes.c_void_p, wt.UINT]
k32.CloseHandle.argtypes = [ctypes.c_void_p]
k32.GetExitCodeProcess.argtypes = [ctypes.c_void_p, ctypes.POINTER(wt.DWORD)]


class ConPty:
    def __init__(self, cmdline, env, cwd, cols, rows):
        in_r, in_w, out_r, out_w = (ctypes.c_void_p() for _ in range(4))
        if not k32.CreatePipe(ctypes.byref(in_r), ctypes.byref(in_w), None, 0) or not k32.CreatePipe(ctypes.byref(out_r), ctypes.byref(out_w), None, 0):
            raise OSError("CreatePipe failed: %d" % ctypes.get_last_error())
        self.hpc = ctypes.c_void_p()
        hr = k32.CreatePseudoConsole(COORD(cols, rows), in_r, out_w, 0, ctypes.byref(self.hpc))
        if hr != 0:
            raise OSError("CreatePseudoConsole failed: HRESULT 0x%08x" % (hr & 0xFFFFFFFF))
        k32.CloseHandle(in_r)
        k32.CloseHandle(out_w)
        size = ctypes.c_size_t(0)
        k32.InitializeProcThreadAttributeList(None, 1, 0, ctypes.byref(size))
        self.attrs = ctypes.create_string_buffer(size.value)
        if not k32.InitializeProcThreadAttributeList(self.attrs, 1, 0, ctypes.byref(size)):
            raise OSError("InitializeProcThreadAttributeList failed: %d" % ctypes.get_last_error())
        if not k32.UpdateProcThreadAttribute(self.attrs, 0, 0x00020016, self.hpc, ctypes.sizeof(ctypes.c_void_p), None, None):     # PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE
            raise OSError("UpdateProcThreadAttribute failed: %d" % ctypes.get_last_error())
        si = STARTUPINFOEX()
        si.StartupInfo.cb = ctypes.sizeof(STARTUPINFOEX)
        si.lpAttributeList = ctypes.cast(self.attrs, ctypes.c_void_p)
        pi = PROCESS_INFORMATION()
        envblock = ctypes.create_unicode_buffer("\0".join("%s=%s" % kv for kv in env.items()) + "\0")
        cmd = ctypes.create_unicode_buffer(cmdline)
        if not k32.CreateProcessW(None, cmd, None, None, False, 0x00080000 | 0x00000400, envblock, cwd, ctypes.byref(si), ctypes.byref(pi)):   # EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT
            raise OSError("CreateProcessW failed: %d" % ctypes.get_last_error())
        k32.CloseHandle(pi.hThread)
        self.hproc, self.pid = pi.hProcess, pi.dwProcessId
        self.rfd = msvcrt.open_osfhandle(out_r.value, os.O_RDONLY)
        self.wfd = msvcrt.open_osfhandle(in_w.value, 0)
        self.closed = False

    def read(self):
        try:
            return os.read(self.rfd, 65536)
        except OSError:
            return b""

    def write(self, data):
        try:
            os.write(self.wfd, data)
        except OSError:
            pass

    def resize(self, cols, rows):
        k32.ResizePseudoConsole(self.hpc, COORD(cols, rows))

    def exited(self):
        return k32.WaitForSingleObject(self.hproc, 0) == 0

    def exit_code(self):
        c = wt.DWORD(0)
        k32.GetExitCodeProcess(self.hproc, ctypes.byref(c))
        return c.value

    def close(self):
        if self.closed:
            return
        self.closed = True
        if not self.exited():
            k32.TerminateProcess(self.hproc, 1)
        k32.ClosePseudoConsole(self.hpc)
        k32.CloseHandle(self.hproc)


# ------------------------------------------------------------------ paramiko server
class Srv(paramiko.ServerInterface):
    def __init__(s):
        s.ptys, s.envs, s.evs, s.reqs, s.pty = {}, {}, {}, {}, {}

    def ev(s, ch):
        return s.evs.setdefault(ch.chanid, threading.Event())

    def check_channel_request(s, kind, cid):
        return paramiko.OPEN_SUCCEEDED if kind == "session" else paramiko.OPEN_FAILED_ADMINISTRATIVELY_PROHIBITED

    def get_allowed_auths(s, u):
        return "publickey,password"

    def check_auth_publickey(s, u, key):
        ok = key == CK
        log("auth publickey user=%s key=%s %s -> %s" % (u, key.get_name(), key.get_fingerprint().hex(), "ok" if ok else "REJECTED (not the scratch client key)"))
        return paramiko.AUTH_SUCCESSFUL if ok else paramiko.AUTH_FAILED

    def check_auth_password(s, u, p):
        ok = bool(PW) and u == PWU and p == PW
        log("password attempt user=%s len %d -> %s" % (u, len(p), "ok" if ok else "rejected"))
        return paramiko.AUTH_SUCCESSFUL if ok else paramiko.AUTH_FAILED

    def check_channel_pty_request(s, ch, term, w, h, pw, ph, modes):
        s.ptys[ch.chanid] = (term.decode() if isinstance(term, bytes) else term, w, h)
        return True

    def check_channel_shell_request(s, ch):
        s.reqs[ch.chanid] = ("shell", None)
        s.ev(ch).set()
        return True

    def check_channel_exec_request(s, ch, cmd):
        s.reqs[ch.chanid] = ("exec", cmd.decode())
        s.ev(ch).set()
        return True

    def check_channel_env_request(s, ch, n, v):
        s.envs.setdefault(ch.chanid, {})[n.decode() if isinstance(n, bytes) else n] = v.decode() if isinstance(v, bytes) else v
        return True

    def check_channel_window_change_request(s, ch, w, h, pw, ph):
        p = s.pty.get(ch.chanid)
        if p:
            p.resize(w, h)
        return True


def serve(sock):
    # every connection is logged (r18cj: a connection that fails authentication used to leave NO line, so "ssh never connected" and "ssh was refused" looked the same)
    try:
        peer = sock.getpeername()
    except Exception:
        peer = "?"
    log("connect from", peer)
    chans = 0
    try:
        t = paramiko.Transport(sock)
        t.add_server_key(HK)
        if LAT:
            time.sleep(LAT)
        srv = Srv()
        t.start_server(server=srv)
        while True:
            ch = t.accept(30)
            if ch is None:
                if not t.is_active():
                    break
                continue
            chans += 1
            threading.Thread(target=handle, args=(ch, srv), daemon=True).start()
        log("connection closed: authenticated=%s channels=%d" % (t.is_authenticated(), chans))
    except Exception as e:
        log("serve err", repr(e))


def remote_env(term, extra):
    env = dict(os.environ)
    env.update(HOME=RHOME, USER=os.environ.get("USERNAME", "tester"), LOGNAME=os.environ.get("USERNAME", "tester"), SHELL="/usr/bin/bash",
               TERM=term or "dumb", CHERE_INVOKING="1")
    env.update(extra)
    return env


def handle(ch, srv):
    srv.ev(ch).wait(10)
    kind, cmd = srv.reqs.get(ch.chanid, (None, None))
    spty = srv.ptys.get(ch.chanid)
    env = remote_env(spty[0] if spty else "dumb", srv.envs.get(ch.chanid, {}))
    log("session", kind, cmd, "pty" if spty else "nopty")
    try:
        if spty and kind == "shell":
            cp = ConPty('"%s" --rcfile "%s" -i' % (BASH, os.path.join(RHOME, ".bashrc")), env, RHOME, spty[1], spty[2])
            srv.pty[ch.chanid] = cp
            log("conpty shell pid", cp.pid)

            def pump_out():
                t_open, sent = time.time(), 0
                while True:
                    d = cp.read()
                    if not d:
                        break
                    if not sent:
                        log("conpty first output after %.2f s: %d bytes" % (time.time() - t_open, len(d)))   # r18ct: 0 bytes ever = the shell's output is not on the pseudo console
                    sent += len(d)
                    try:
                        ch.sendall(d)
                    except Exception:
                        break
                log("conpty output ended: %d bytes sent to the client in total" % sent)
                # r18cz: EOF alone leaves the channel open (the main loop below blocks in recv until the CLIENT closes), so ssh.exe stayed in front for ~40 s
                # after the remote `exit` and the next trial's `ssh fake` was typed into the dead client. A real sshd sends the exit status and closes.
                try:
                    ch.shutdown_write()
                except Exception:
                    pass
                try:
                    for _ in range(25):          # the exit code is available a moment after the pseudo console closes
                        if cp.exited():
                            break
                        time.sleep(0.1)
                    ch.send_exit_status(cp.exit_code() if cp.exited() else 0)
                except Exception:
                    pass
                try:
                    ch.close()
                except Exception:
                    pass

            def watch():
                while not cp.exited() and not ch.closed:
                    time.sleep(0.2)
                time.sleep(0.4)
                cp.close()          # closing the pseudo console ends pump_out's read
            threading.Thread(target=pump_out, daemon=True).start()
            threading.Thread(target=watch, daemon=True).start()
            while not ch.closed:
                try:
                    d = ch.recv(65536)
                except Exception:
                    break
                if not d:
                    break
                cp.write(d)
            code = cp.exit_code() if cp.exited() else 0
            try:
                cp.close()
                ch.send_exit_status(code)
            except Exception:
                pass
        else:
            argv = [BASH, "-i"] if kind == "shell" else [BASH, "-c", cmd or "true"]
            p = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env, cwd=RHOME)

            def pump(src, sendf):
                for chunk in iter(lambda: src.read1(65536) if hasattr(src, "read1") else src.read(65536), b""):
                    sendf(chunk)
            t1 = threading.Thread(target=pump, args=(p.stdout, ch.sendall))
            t2 = threading.Thread(target=pump, args=(p.stderr, ch.sendall_stderr))
            t1.start()
            t2.start()

            def feed():
                try:
                    while True:
                        d = ch.recv(65536)
                        if not d:
                            break
                        p.stdin.write(d)
                        p.stdin.flush()
                except Exception:
                    pass
                try:
                    p.stdin.close()
                except Exception:
                    pass
            threading.Thread(target=feed, daemon=True).start()
            p.wait()
            t1.join()
            t2.join()
            ch.send_exit_status(p.returncode)
    except Exception as e:
        log("session error", repr(e))
    try:
        ch.close()
    except Exception:
        pass


s = socket.socket()
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("127.0.0.1", PORT))
s.listen(50)
log("listening", PORT)
while True:
    c, _ = s.accept()
    threading.Thread(target=serve, args=(c,), daemon=True).start()
