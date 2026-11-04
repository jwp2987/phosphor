# appci/profile.py: builds the scratch HOME (owner-like config, NO credentials), the shell logging rc files
# and the test tree. Never reads the owner's real config: CI has none. The owner-like settings are the ones in
# docs/testing.md 3.1 (agent mode default, autodetect + natural-language detection on, vertical tabs are not
# set because they do not affect input), plus a provider ONLY when the endpoint is configured (ci_env.py: env or ~/.config/phosphor-ci/ci.env).
import os, random, shutil, subprocess, sys


def _wr(path, text):
    """write text with LF line endings on every OS (a Windows text-mode write would make CRLF shell scripts and rc files). Creates the parent
    directory (#1550's .refzdot/ was never created: every pass crashed in prof.build, caught by the #1414 diagnostic run)."""
    d = os.path.dirname(path)
    if d:
        os.makedirs(d, exist_ok=True)
    with open(path, "w", newline="\n") as f:
        f.write(text)


def _fwd(path):
    """C:/dir/file form: accepted by msys bash, OpenSSH and Python alike, and free of backslash escapes inside rc files and ssh config"""
    return path.replace("\\", "/")

BASHRC = r'''# appci scratch rc: logs every prompt and the exact command line the shell received.
export PS1='\w\$ '
export HISTSIZE=200000 HISTFILESIZE=200000 HISTCONTROL=
export PA_CTL="%(ctl)s"
export PATH="$HOME/bin:$PATH"
__pa_n=0
__pa_first=1
__pa_rec() {
  local h c n
  h=$(HISTTIMEFORMAT= history 1)
  if [[ $h =~ ^[[:space:]]*([0-9]+)[[:space:]]+(.*)$ ]]; then
    n=${BASH_REMATCH[1]}; c=${BASH_REMATCH[2]}; c=${c//$'\n'/\\n}
    # the FIRST prompt of a shell only adopts the loaded history's last number: that line was not received by this shell
    # (a restored/relaunched shell would otherwise log it as a command and look like a session-restore re-run)
    if [ -n "$__pa_first" ]; then __pa_n=$n
    elif [ "$n" != "$__pa_n" ]; then __pa_n=$n; printf 'C\t%%s\t%%s\n' "$EPOCHREALTIME" "$c" >> "$PA_CTL/rec.log"; fi
    if [ "$n" != "$__pa_n" ]; then __pa_n=$n; printf 'C\\t%%s\\t%%s\\n' "$EPOCHREALTIME" "$c" >> "$PA_CTL/rec.log"; fi
  fi
  __pa_first=
  printf 'P\t%%s\n' "$EPOCHREALTIME" >> "$PA_CTL/prompt.log"
}
PROMPT_COMMAND="__pa_rec${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
if [ -n "$APPCI_W" ]; then cd "$APPCI_W"; elif [ "$PWD" = "$HOME" ]; then cd "$HOME/w" 2>/dev/null; fi
true
'''
BOOTSTRAP_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "app", "assets", "bundled", "bootstrap")


def compose_bash_bootstrap(conpty, session_id=1, bootdir=None):
    """#1583 G2: Phosphor's bash bootstrap as ONE sourceable script: bash.sh with its `#include bundled/bootstrap/bash_body.sh` line replaced by the
    body (what the app's `load_and_escape_script`/include step does), @@WARP_SESSION_ID@@ and @@USING_CON_PTY_BOOLEAN@@ filled in like
    terminal/bootstrap.rs does (`conpty` is true for a Windows ConPTY pane). None when the assets are not in this checkout (a harness copied out of the repo)."""
    d = bootdir or BOOTSTRAP_DIR
    try:
        head = open(os.path.join(d, "bash.sh"), encoding="utf-8").read()
        body = open(os.path.join(d, "bash_body.sh"), encoding="utf-8").read()
    except OSError:
        return None
    inc = "#include bundled/bootstrap/bash_body.sh"
    if inc not in head:
        return None
    text = head.replace(inc, body.rstrip("\n"))
    return text.replace("@@WARP_SESSION_ID@@", str(session_id)).replace("@@USING_CON_PTY_BOOLEAN@@", "true" if conpty else "false")


def ref_boot_rc_text(ref_rc, boot_sh, conpty):
    """the reference bash's rc for G2: the normal reference rc (prompt/command logging), then the same variables the InitShell script sets in the app,
    then the bootstrap. The reference terminal's shell then carries the hooks Phosphor's pane has (bash-preexec DEBUG trap, precmd/preexec messages, title)."""
    return '. "%s"\nWARP_SESSION_ID=1; WARP_USING_WINDOWS_CON_PTY=%s\n. "%s"\n' % (ref_rc, "true" if conpty else "false", boot_sh)


BASH_PROFILE = '[ -f ~/.bashrc ] && . ~/.bashrc\n'
ZSHRC = r'''# appci scratch rc (zsh): same log format as the bash rc
zmodload zsh/datetime
export PA_CTL="%(ctl)s"
export PATH="$HOME/bin:$PATH"
PS1='%%~%%# '
HISTFILE=$HOME/.zsh_history; HISTSIZE=200000; SAVEHIST=200000
__pa_n=0
__pa_rec() {
  local c; c=${history[$((HISTCMD-1))]};   # raw history text: `fc -l` prints a ZWJ / unprintable as the TEXT \\u200d (r18e4)
  [[ -n $c ]] || c=$(fc -ln -1 2>/dev/null)
  c=${c#"${c%%%%[![:space:]]*}"}; c=${c//$'\n'/\\n}
  if [[ $HISTCMD != $__pa_n ]]; then __pa_n=$HISTCMD; print -r -- "C"$'\t'"$EPOCHREALTIME"$'\t'"$c" >> "$PA_CTL/rec.log"; fi
  print -r -- "P"$'\t'"$EPOCHREALTIME" >> "$PA_CTL/prompt.log"
}
precmd_functions+=(__pa_rec)
# #1606: when the line editor let go of the line (zle-line-finish) and when this rc's preexec hook ran, both in the shell's own clock.
# In Phosphor the bootstrap puts warp_preexec (the Preexec message) first, so __pa_pxh runs after it, but the bootstrap's title hook
# (warp_set_title_active_on_preexec) is appended later and runs AFTER __pa_pxh. The latency_enter stamp line prints both times next to
# its stamp, so Enter -> command start splits into ZLE ingest/accept, warp_preexec, and the title hook + the command, the same way in
# Phosphor and in the reference terminal. Variables only, no I/O.
__pa_px=; __pa_lf=
__pa_pxh() { __pa_px=$EPOCHREALTIME }
preexec_functions+=(__pa_pxh)
__pa_lfw() { __pa_lf=$EPOCHREALTIME }
zle -N zle-line-finish __pa_lfw 2>/dev/null
if [[ -n $APPCI_W ]]; then cd $APPCI_W; elif [[ $PWD == $HOME ]]; then cd $HOME/w; fi
'''
FISH = '''# appci scratch config (fish)
set -gx PA_CTL "%(ctl)s"
function fish_prompt; echo -n "fish> "; end
# fish_postexec marker: the exact line fish executed, in its own log (fish_rec.log) so the bash rec.log counts stay untouched
function __pa_postexec --on-event fish_postexec; printf 'C\\t%%s\\t%%s\\n' (date +%%s) "$argv[1]" >> $PA_CTL/fish_rec.log; end
'''


def history_lines(n=2200, seed=1):
    r = random.Random(seed)
    verbs = ["ls", "cd", "git", "cat", "grep", "make", "cargo", "docker", "ssh", "echo", "find", "tar", "sudo", "vim", "less"]
    stems = ["alpha", "alphabet", "alpine", "src", "build", "status", "stash", "commit", "checkout", "log", "diff", "test", "target", "release"]
    out = []
    for i in range(n):
        v = r.choice(verbs)
        out.append("%s %s %s%d" % (v, r.choice(stems), r.choice(stems)[:3], r.randint(0, 99)))
    # colliding prefixes the ghost text can pick from
    for i in range(30):
        out.append("echo alpha %d" % i)
    out += ["echo alphabet soup", "echo alpha-one two", "git status", "git stash", "git checkout -"]
    return out


class Profile:
    def __init__(self, run, shell_home_name="home", pane=None):
        self.win = os.name == "nt"
        self.variant = None     # #1534: "pwsh7" | "cmd" for a startup_shell_override pass (run.py sets it together with pane = its family), else None
        self.pane = pane or ("powershell" if self.win else None)   # Windows pane shell: powershell | gitbash (docs/testing.md 2.8); None = POSIX bash/zsh
        self.run = run
        self.home = os.path.join(run, shell_home_name)
        self.ctl = os.path.join(run, "ctl")
        self.rctl = os.path.join(run, "rctl")          # the ssh "remote" side logs here
        self.refctl = os.path.join(run, "refctl")      # the Linux reference terminal logs here, never into ctl (#1500)
        self.ref_rc = os.path.join(self.home, ".refrc")
        self.ref_boot_rc = os.path.join(self.home, ".refrc_boot")      # #1583 G2: ref_rc, then Phosphor's own bash bootstrap sourced (the shell hooks the pane has)
        self.ref_active = False                        # True while the reference terminal is the target: commands()/read() then read refctl
        self.rhome = os.path.join(run, "rhome")
        self.shots = os.path.join(run, "shots")

    @property
    def local_prompts(self):
        """False when the pane's shell cannot run the rc hook that writes prompt.log / rec.log: the app starts PowerShell with -NoProfile, so a
        PowerShell pane has neither (cases fall back to effect files and fixed waits and say so)"""
        return not (self.win and self.pane in ("powershell", "cmd"))        # cmd.exe and pwsh 7 (family "powershell") have no rc hook either

    def _c(self, p):
        """a path as it goes INTO a shell rc file: C:/forward/slashes on Windows, unchanged elsewhere"""
        return _fwd(p) if self.win else p

    def build(self, mode, ollama_url=None, ollama_model="gpt-oss:20b", ssh_port=None, ssh_ident=None, ssh_pw=None):
        for d in (self.home, self.ctl, self.rctl, self.refctl, self.rhome, self.shots):
            shutil.rmtree(d, ignore_errors=True) if d in (self.home, self.ctl, self.rctl, self.refctl, self.rhome) else None
            os.makedirs(d, exist_ok=True)
        h = self.home
        os.makedirs(h + "/w", exist_ok=True)
        os.makedirs(h + "/bin", exist_ok=True)
        for f in ("rec.log", "prompt.log"):
            open(os.path.join(self.ctl, f), "w").close()
            open(os.path.join(self.rctl, f), "w").close()
            open(os.path.join(self.refctl, f), "w").close()
        if self.win:
            # no python3 on the VM's PATH: sh shims (Git Bash pane only: PowerShell would offer to "open" an extensionless file) exec the private
            # interpreter that runs this harness
            if self.pane == "gitbash":
                py = _fwd(sys.executable)
                _wr(h + "/bin/now", "#!/bin/sh\nexec \"%s\" -c 'import time;print(\"%%.9f\" %% time.time())'\n" % py)
                _wr(h + "/bin/python3", "#!/bin/sh\nexec \"%s\" \"$@\"\n" % py)
                _wr(h + "/bin/python", "#!/bin/sh\nexec \"%s\" \"$@\"\n" % py)
        else:
            _wr(h + "/bin/now", "#!/bin/sh\nexec python3 -c 'import time;print(\"%.9f\" % time.time())'\n")
        if os.path.exists(h + "/bin/now"):
            os.chmod(h + "/bin/now", 0o755)
        _wr(h + "/.bashrc", BASHRC % {"ctl": self._c(self.ctl)})
        _wr(h + "/.bash_profile", BASH_PROFILE)
        _wr(self.ref_rc, BASHRC % {"ctl": self._c(self.refctl)})
        boot = compose_bash_bootstrap(self.win)
        if boot:
            _wr(os.path.join(h, ".refboot.sh"), boot)
            _wr(self.ref_boot_rc, ref_boot_rc_text(self._c(self.ref_rc), self._c(os.path.join(h, ".refboot.sh")), self.win))
        elif os.path.exists(self.ref_boot_rc):
            os.remove(self.ref_boot_rc)
        _wr(h + "/.zshrc", ZSHRC % {"ctl": self._c(self.ctl)})
        self.ref_zdot = os.path.join(h, ".refzdot")       # macOS reference zsh: its own ZDOTDIR so its hook writes refctl (#1550)
        _wr(os.path.join(self.ref_zdot, ".zshrc"), ZSHRC % {"ctl": self._c(self.refctl)})
        os.makedirs(h + "/.config/fish", exist_ok=True)
        _wr(h + "/.config/fish/config.fish", FISH % {"ctl": self._c(self.ctl)})
        os.makedirs(h + "/.config/nushell", exist_ok=True)       # empty configs: nu's first-run "create config files?" question never appears
        for nf in ("config.nu", "env.nu"):
            open(h + "/.config/nushell/" + nf, "a").close()
        hist = history_lines()
        _wr(h + "/.bash_history", "\n".join(hist) + "\n")
        _wr(h + "/.zsh_history", "\n".join(hist) + "\n")
        # remote side
        # Windows (r18ct): the ConPTY Git Bash behind fakesshd_win wrote its prompt, line-editor echo and "cannot set terminal process group" to fakesshd's inherited
        # STDERR FILE (fakesshd.stderr), not to the pseudo console, so ssh.exe printed nothing and the app saw zero bytes from the session. Rebinding fd 2 to the
        # pty's stdout first thing in the remote rc puts the prompt and echo back where the client can see them (no effect where fd 2 already is the pty).
        _wr(self.rhome + "/.bashrc", ("exec 2>&1\n" if self.win else "") + BASHRC % {"ctl": self._c(self.rctl)})
        _wr(self.rhome + "/.bash_profile", BASH_PROFILE)
        _wr(self.rhome + "/.bash_history", "\n".join(history_lines(2200, 2)) + "\n")
        # test tree for Tab completion
        t = h + "/w/tc"
        os.makedirs(t, exist_ok=True)
        for d in ("with space", "alpha", "alphabet", "alpine"):
            os.makedirs(os.path.join(h, "w", d), exist_ok=True)
        open(t + "/zzcompl_alpha_unique.txt", "w").close()
        self.write_settings(mode, ollama_url, ollama_model)
        if ssh_port and self.win:
            self.write_ssh_windows(ssh_port, ssh_ident, ssh_pw)
        elif ssh_port:
            os.makedirs(h + "/.ssh", mode=0o700, exist_ok=True)
            _wr(h + "/.ssh/config", 
                "Host fake\n  HostName 127.0.0.1\n  Port %d\n  User %s\n  IdentityFile %s\n  IdentitiesOnly yes\n"
                "  StrictHostKeyChecking no\n  UserKnownHostsFile %s/.ssh/known_hosts\n  BatchMode yes\n  LogLevel ERROR\n"
                % (ssh_port, os.environ.get("USER", "runner"), ssh_ident, h))
            pw = os.path.join(os.path.dirname(ssh_ident), "pwkey")      # sshd.py: passphrase-protected key (openssh kind only)
            if os.path.exists(pw):
                open(h + "/.ssh/config", "a").write(
                    "\nHost fakepw\n  HostName 127.0.0.1\n  Port %d\n  User %s\n  IdentityFile %s\n  IdentitiesOnly yes\n"
                    "  StrictHostKeyChecking no\n  UserKnownHostsFile %s/.ssh/known_hosts\n  BatchMode no\n  NumberOfPasswordPrompts 1\n  LogLevel ERROR\n"
                    % (ssh_port, os.environ.get("USER", "runner"), pw, h))
            if ssh_pw:
                open(h + "/.ssh/config", "a").write("\n" + self.password_host_block(ssh_pw, h))
            self.write_ssh_wrappers()
            # the remote side (rhome, same machine) needs the same host entries and wrappers, or a NESTED `ssh fake` typed in the
            # remote shell cannot resolve (ssh_nested_typeahead, r18ck 2026-11-02)
            os.makedirs(self.rhome + "/.ssh", mode=0o700, exist_ok=True)
            shutil.copyfile(h + "/.ssh/config", self.rhome + "/.ssh/config")
            os.chmod(self.rhome + "/.ssh/config", 0o600)
            self.write_ssh_wrappers(self.rhome)
        return h

    @staticmethod
    def password_host_block(pw, home):
        """ssh config for `ssh fakepass` (#1535): the harness fake sshd's one password user, password auth only, one attempt: ssh stops at its own
        `pwuser@127.0.0.1's password:` prompt on the tty"""
        return ("Host fakepass\n  HostName 127.0.0.1\n  Port %d\n  User %s\n  PreferredAuthentications password\n  PubkeyAuthentication no\n"
                "  StrictHostKeyChecking no\n  UserKnownHostsFile %s/.ssh/known_hosts\n  BatchMode no\n  NumberOfPasswordPrompts 1\n  LogLevel ERROR\n"
                % (pw["port"], pw["user"], _fwd(home)))

    TOOLS = ("ssh", "scp", "sftp")
    WIN_OPENSSH = os.path.join(os.environ.get("SystemRoot", r"C:\Windows"), "System32", "OpenSSH")

    def write_ssh_windows(self, port, ident, pwhost=None):
        """Windows: the PowerShell pane's client is the built-in Win32-OpenSSH (System32\\OpenSSH\\ssh.exe), which reads the user config from the PROFILE
        directory, not %HOME%; the Git Bash pane's is Git for Windows' own ssh (win_client). Either way `ssh fake` is resolved through wrappers in
        <home>/bin (first on the app's PATH) that pass `-F <home>/.ssh/config`:
        `ssh` (sh, written for the Git Bash pane) or `ssh.cmd` (PowerShell pane; found before System32\\OpenSSH\\ssh.exe).
        Win32-OpenSSH refuses key files other users can read, so the scratch key and config get an owner-only ACL (icacls, our own files only)."""
        h = self.home
        os.makedirs(h + "/.ssh", exist_ok=True)
        user = os.environ.get("USERNAME", "runner")
        cfg = h + "/.ssh/config"
        blocks = ["Host fake\n  HostName 127.0.0.1\n  Port %d\n  User %s\n  IdentityFile %s\n  IdentitiesOnly yes\n"
                  "  StrictHostKeyChecking no\n  UserKnownHostsFile %s/.ssh/known_hosts\n  BatchMode yes\n  LogLevel ERROR\n" % (port, user, _fwd(ident), _fwd(h))]
        if pwhost:
            blocks.append(self.password_host_block(pwhost, h))
        _wr(cfg, "\n".join(blocks))
        for f in (cfg, ident):
            subprocess.run(["icacls", f, "/inheritance:r", "/grant:r", "%s:F" % user], capture_output=True)
        for t in self.TOOLS:
            if self.pane == "gitbash":
                real = self.win_client(t, True)
                if real:
                    _wr(os.path.join(h, "bin", t), '#!/bin/sh\n# appci: the ssh a Git Bash user gets (Git for Windows\' own), config from the scratch home (#1481)\nexec "%s" -F "%s" "$@"\n' % (_fwd(real), _fwd(cfg)))
            else:       # PowerShell resolves `ssh` to ssh.cmd (never an extensionless file: it would be "opened" through a file association)
                real = self.win_client(t, False)
                if real:
                    with open(os.path.join(h, "bin", t + ".cmd"), "w", newline="\r\n") as f:
                        f.write('@echo off\n"%s" -F "%s" %%*\n' % (real, cfg.replace("/", "\\")))
        # The REMOTE shell is always Git Bash (fakesshd_win.py), so a nested `ssh fake` typed there needs the same host entry and an sh wrapper in
        # the remote home (ssh_nested_typeahead; the Mac fix of 907e1171e, same cause): config copy + <rhome>/bin/ssh -F <rhome>/.ssh/config.
        rh = self.rhome
        os.makedirs(rh + "/.ssh", exist_ok=True)
        os.makedirs(rh + "/bin", exist_ok=True)
        shutil.copyfile(cfg, rh + "/.ssh/config")
        for f in (rh + "/.ssh/config",):
            subprocess.run(["icacls", f, "/inheritance:r", "/grant:r", "%s:F" % user], capture_output=True)
        for t in self.TOOLS:
            real = self.win_client(t, True)
            if real:
                _wr(os.path.join(rh, "bin", t), '#!/bin/sh\n# appci: nested ssh from the remote shell (the remote home is a different HOME)\nexec "%s" -F "%s" "$@"\n' % (_fwd(real), _fwd(rh + "/.ssh/config")))

    def win_client(self, tool, bash):
        """the ssh-family client a pane's shell would really run: Git for Windows' own (usr/bin) for Git Bash (it comes first on a Git Bash user's PATH, and
        unlike Win32-OpenSSH it supports the ControlMaster options Phosphor's ssh wrapper adds: r18cj showed `getsockname failed: Not a socket` from
        System32\\OpenSSH\\ssh.exe in the Git Bash pane), else System32\\OpenSSH; the PowerShell pane (no wrapper) uses the Win32-OpenSSH it has."""
        if bash:
            import winapi
            for d in reversed(winapi.git_dirs()):
                p = os.path.join(d, tool + ".exe")
                if os.path.isfile(p):
                    return p
        p = os.path.join(self.WIN_OPENSSH, tool + ".exe")
        return p if os.path.isfile(p) else None

    def write_ssh_wrappers(self, home=None):
        """OpenSSH reads ~/.ssh/config from the PASSWD home, not $HOME (#1481): on a runner whose passwd home differs from the scratch HOME,
        `ssh fake` does not resolve. <home>/bin is first on PATH in the shell rc files, so ssh/scp/sftp wrappers there add `-F $HOME/.ssh/config`
        and exec the REAL binary (absolute path resolved now, from a PATH without the wrapper dir, so the wrapper can never exec itself)."""
        bindir = os.path.join(home or self.home, "bin")
        os.makedirs(bindir, exist_ok=True)
        clean = os.pathsep.join(d for d in os.environ.get("PATH", os.defpath).split(os.pathsep) if os.path.abspath(d or ".") != os.path.abspath(bindir))
        for t in self.TOOLS:
            real = shutil.which(t, path=clean)
            if not real:
                continue
            w = os.path.join(bindir, t)
            _wr(w, '#!/bin/sh\n# appci: OpenSSH ignores $HOME for ~/.ssh/config (#1481)\nexec "%s" -F "$HOME/.ssh/config" "$@"\n' % real.replace('"', '\\"'))
            os.chmod(w, 0o755)

    def ssh_probe(self, timeout=15):
        """setup-time check, in the environment the app's shell gets (HOME = scratch home, <home>/bin first on PATH): `ssh fake true`
        (BatchMode is in the config) must succeed. Returns None when it does, else a skip reason carrying ssh's stderr."""
        self.ssh_hint = None
        env = dict(os.environ, HOME=self.home, PATH=os.path.join(self.home, "bin") + os.pathsep + os.environ.get("PATH", os.defpath))
        argv = ["ssh", "fake", "true"]
        if self.win:        # run the pane's real client with the config, as the wrapper does (the .cmd/sh wrappers are for the shells)
            client = self.win_client("ssh", self.pane == "gitbash")
            if not client:
                return "ssh test host unusable: no ssh client found (Git for Windows usr\\bin\\ssh.exe or System32\\OpenSSH\\ssh.exe)"
            argv = [client, "-F", self.home + "/.ssh/config", "fake", "true"]
        try:
            p = subprocess.run(argv, env=env, capture_output=True, text=True, timeout=timeout, stdin=subprocess.DEVNULL)
        except subprocess.TimeoutExpired:
            return "ssh test host unusable: `ssh fake true` did not finish within %d s" % timeout
        except OSError as e:
            return "ssh test host unusable: cannot run ssh (%s)" % e
        if p.returncode != 0:
            return "ssh test host unusable: `ssh fake true` exited %d: %s" % (p.returncode, (p.stderr or "").strip()[:300])
        if self.win and self.pane == "gitbash":
            # Phosphor's bash ssh wrapper starts a ControlMaster (-o ControlMaster=yes -o ControlPath=...) on every ssh. A plain probe passed in r18cj while the pane's
            # ssh died in ~1 s with `getsockname failed: Not a socket` (Win32-OpenSSH has no multiplexing), so the ssh cases blamed the app for a client that cannot work.
            # Probe the same option set; a client that fails it SKIPS the ssh cases with its own error as the reason.
            import tempfile
            d = tempfile.mkdtemp(prefix="pcm")
            try:
                a2 = [client, "-F", self.home + "/.ssh/config", "-o", "ControlMaster=yes", "-o", "ControlPath=" + _fwd(os.path.join(d, "cm")), "-o", "ControlPersist=3", "fake", "true"]
                p = subprocess.run(a2, env=env, capture_output=True, text=True, timeout=timeout, stdin=subprocess.DEVNULL)
                if p.returncode != 0:
                    # NOT a skip (r18co skipped all three ssh cases here): the app's bash wrapper runs exactly this on every ssh from a Git Bash pane, so a real Git Bash
                    # user gets the same failure. The cases run and report what the user gets; this text is added to their failure messages as the lead.
                    self.ssh_hint = ("Git Bash pane: the ControlMaster probe with the wrapper's options FAILED before the case (%s cannot start a ControlMaster, exit %d: %s); "
                                     "the app's bash ssh wrapper starts `-o ControlMaster=yes ... -N -f` on every ssh, so a real Git Bash user's ssh is expected to fail the same way "
                                     "(candidate APP finding: no fallback to plain ssh when the master cannot start)" % (client, p.returncode, " ".join((p.stderr or "").split())[:200]))
            except (subprocess.TimeoutExpired, OSError) as e:
                self.ssh_hint = "Git Bash pane: the ControlMaster probe with the wrapper's options did not finish (%s); candidate APP finding, see above" % e
            finally:
                shutil.rmtree(d, ignore_errors=True)
        return None

    ssh_hint = None      # Windows Git Bash: why Phosphor's ssh wrapper is expected to fail with Git for Windows' ssh.exe (set by ssh_probe); appended to ssh case failures

    def ps_ssh_function(self):
        """PowerShell source of a function `ssh` for the PowerShell pane: the pane has no rc, so nothing puts <home>/bin first on PATH and System32\\OpenSSH\\ssh.exe
        would read the REAL user's config (`ssh fake` -> 'Could not resolve hostname fake', r18co). None when there is no such client. Typed into the pane once."""
        if not self.win:
            return None
        real = self.win_client("ssh", False)
        if not real:
            return None
        return "function ssh { & '%s' -F '%s' @args }" % (_fwd(real), self.home.replace("\\", "/") + "/.ssh/config")

    OWNER_SEED = os.path.join(os.path.expanduser("~"), ".config", "phosphor-ci", "owner-settings.toml")
    owner_note = None       # "sha256 <12>" or "not found (bare profile)": reported in results.json meta / summary.md

    def owner_seed(self):
        return os.environ.get("APPCI_OWNER_SETTINGS") or self.OWNER_SEED

    def gitbash_toml(self, new_session=False):
        """TOML value of the shell a Windows pane is started with through session.startup_shell_override, else None: Git for Windows' bash.exe for the Git
        Bash pane, and for a #1534 startup_shell_override pass PowerShell 7's pwsh.exe (variant "pwsh7") or cmd.exe (variant "cmd"). startup_shell_override
        is a plain string (StartupShell: a custom path); new_session_shell_override is an enum, so its TOML is the inline table `{ executable = "<path>" }`
        (r18co: a bare string made the app show 'Invalid value for new_session_shell_override' and use the default for new tabs)."""
        if not self.win:
            return None
        import winapi
        if self.variant == "pwsh7":
            b = winapi.pwsh7()
            if not b:
                raise RuntimeError("PowerShell 7 (pwsh.exe) not found: the pwsh7 pane cannot start")
        elif self.variant == "cmd":
            b = winapi.cmd_exe()
            if not b:
                raise RuntimeError("cmd.exe not found: the cmd pane cannot start")
        elif self.pane == "gitbash":
            b = winapi.git_bash()
            if not b:
                raise RuntimeError("Git for Windows (bash.exe) not found: the Git Bash pane cannot start")
        else:
            return None
        return '{ executable = "%s" }' % _fwd(b) if new_session else '"%s"' % _fwd(b)

    def write_settings(self, mode, ollama_url=None, ollama_model="gpt-oss:20b"):
        """Start from the owner's real settings.toml (machine-local seed, never modified, never printed or uploaded) and apply ONLY
        the per-pass overrides textually (toml_text.py). No seed: the minimal bare profile below, and owner_note says so."""
        import hashlib
        import toml_text as T
        cfg = self.home + "/.config/phosphor"
        os.makedirs(cfg, exist_ok=True)
        seed = self.owner_seed()
        m = "agent" if mode == "agent" else "terminal"
        if os.path.isfile(seed):
            raw = open(seed, "rb").read()
            Profile.owner_note = "sha256 " + hashlib.sha256(raw).hexdigest()[:12]
            t = raw.decode("utf-8", "replace").replace("\r\n", "\n")
            t = T.set_key(t, "general", "default_session_mode", '"%s"' % m)
            t = T.set_key(t, "general", "restore_session", "false")                # no saved tabs/sessions
            t = T.set_key(t, "warpify.ssh", "ssh_extension_install_mode", '"never_install"')
            for k in ("startup_shell_override", "new_session_shell_override"):     # the scratch rc logging needs the login shell
                t = T.del_key(t, "session", k)
            gb = self.gitbash_toml()
            if gb:              # Windows Git Bash pane: the app's startup + new-tab shell is Git for Windows' bash.exe (HOME = scratch home, so ~/.bashrc logs)
                t = T.set_key(t, "session", "startup_shell_override", gb)
                t = T.set_key(t, "session", "new_session_shell_override", self.gitbash_toml(True))
            if mode == "agent":
                t = T.set_key(t, "agents.warp_agent.input", "ai_auto_detection_enabled", "true")
                t = T.set_key(t, "agents.warp_agent.input", "nld_in_terminal_enabled", "true")
            if ollama_url:      # the owner's list also names a laptop-only proxy: the CI provider list is the Ollama endpoint alone, and it is the ACTIVE model
                t = T.set_key(t, "agents.warp_agent", "providers", '[ { id = "appci-ollama", name = "CI Ollama", api_type = "ollama", base_url = "%s", models = [ { name = "%s", id = "%s" } ] } ]' % (ollama_url, ollama_model, ollama_model))
                t = T.set_key(t, "agents.byop", "last_used_model_id", '"byop:appci-ollama:%s"' % ollama_model)
            _wr(cfg + "/settings.toml", t)
            os.chmod(cfg + "/settings.toml", 0o600)
            return
        Profile.owner_note = "not found (bare profile)"
        lines = ['[warpify.ssh]', 'ssh_extension_install_mode = "never_install"', '',
                 '[general]', 'default_session_mode = "%s"' % m, 'restore_session = false', '']
        gb = self.gitbash_toml()
        if gb:
            lines += ['[session]', 'startup_shell_override = ' + gb, 'new_session_shell_override = ' + self.gitbash_toml(True), '']
        if mode == "agent":
            lines += ['[agents.warp_agent.input]', 'ai_auto_detection_enabled = true', 'nld_in_terminal_enabled = true', '']
        if ollama_url:
            lines += ['[[agents.warp_agent.providers]]', 'id = "appci-ollama"', 'name = "CI Ollama"', 'api_type = "ollama"',
                      'base_url = "%s"' % ollama_url, 'models = [{ name = "%s", id = "%s" }]' % (ollama_model, ollama_model), '']
            lines += ['[agents.byop]', 'last_used_model_id = "byop:appci-ollama:%s"' % ollama_model, '']
        _wr(cfg + "/settings.toml", "\n".join(lines))

    def read(self, which, remote=False):
        p = os.path.join(self.rctl if remote else (self.refctl if self.ref_active else self.ctl), which)
        try:
            return open(p, errors="replace").read().splitlines()
        except FileNotFoundError:
            return []

    def commands(self, remote=False):
        """the exact command lines the shell received, in order: [(epoch, cmd)]"""
        out = []
        for l in self.read("rec.log", remote):
            p = l.split("\t", 2)
            if len(p) == 3 and p[0] == "C":
                try:
                    ts = float(p[1])
                except ValueError:
                    # macOS /bin/bash 3.2 (the ssh test sshd's remote shell, force.sh) has no $EPOCHREALTIME: the rc logs an empty timestamp
                    # (r18ct: "could not convert string to float: ''" killed ssh_connect_typing). Callers use order and text, never the epoch.
                    ts = 0.0
                out.append((ts, p[2]))
        return out

    def prompts(self, remote=False):
        return len([l for l in self.read("prompt.log", remote) if l.startswith("P")])
