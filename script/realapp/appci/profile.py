# appci/profile.py: builds the scratch HOME (owner-like config, NO credentials), the shell logging rc files
# and the test tree. Never reads the owner's real config: CI has none. The owner-like settings are the ones in
# docs/testing.md 3.1 (agent mode default, autodetect + natural-language detection on, vertical tabs are not
# set because they do not affect input), plus a provider ONLY when the endpoint is configured (ci_env.py: env or ~/.config/phosphor-ci/ci.env).
import os, random, shutil, subprocess, sys


def _wr(path, text):
    """write text with LF line endings on every OS (a Windows text-mode write would make CRLF shell scripts and rc files)"""
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
BASH_PROFILE = '[ -f ~/.bashrc ] && . ~/.bashrc\n'
ZSHRC = r'''# appci scratch rc (zsh): same log format as the bash rc
zmodload zsh/datetime
export PA_CTL="%(ctl)s"
export PATH="$HOME/bin:$PATH"
PS1='%%~%%# '
HISTFILE=$HOME/.zsh_history; HISTSIZE=200000; SAVEHIST=200000
__pa_n=0
__pa_rec() {
  local c; c=$(fc -ln -1 2>/dev/null); c=${c#"${c%%%%[![:space:]]*}"}; c=${c//$'\n'/\\n}
  if [[ $HISTCMD != $__pa_n ]]; then __pa_n=$HISTCMD; print -r -- "C"$'\t'"$EPOCHREALTIME"$'\t'"$c" >> "$PA_CTL/rec.log"; fi
  print -r -- "P"$'\t'"$EPOCHREALTIME" >> "$PA_CTL/prompt.log"
}
precmd_functions+=(__pa_rec)
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
        self.pane = pane or ("powershell" if self.win else None)   # Windows pane shell: powershell | gitbash (docs/testing.md 2.8); None = POSIX bash/zsh
        self.run = run
        self.home = os.path.join(run, shell_home_name)
        self.ctl = os.path.join(run, "ctl")
        self.rctl = os.path.join(run, "rctl")          # the ssh "remote" side logs here
        self.rhome = os.path.join(run, "rhome")
        self.shots = os.path.join(run, "shots")

    @property
    def local_prompts(self):
        """False when the pane's shell cannot run the rc hook that writes prompt.log / rec.log: the app starts PowerShell with -NoProfile, so a
        PowerShell pane has neither (cases fall back to effect files and fixed waits and say so)"""
        return not (self.win and self.pane == "powershell")

    def _c(self, p):
        """a path as it goes INTO a shell rc file: C:/forward/slashes on Windows, unchanged elsewhere"""
        return _fwd(p) if self.win else p

    def build(self, mode, ollama_url=None, ollama_model="gpt-oss:20b", ssh_port=None, ssh_ident=None):
        for d in (self.home, self.ctl, self.rctl, self.rhome, self.shots):
            shutil.rmtree(d, ignore_errors=True) if d in (self.home, self.ctl, self.rctl, self.rhome) else None
            os.makedirs(d, exist_ok=True)
        h = self.home
        os.makedirs(h + "/w", exist_ok=True)
        os.makedirs(h + "/bin", exist_ok=True)
        for f in ("rec.log", "prompt.log"):
            open(os.path.join(self.ctl, f), "w").close()
            open(os.path.join(self.rctl, f), "w").close()
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
        _wr(h + "/.zshrc", ZSHRC % {"ctl": self._c(self.ctl)})
        os.makedirs(h + "/.config/fish", exist_ok=True)
        _wr(h + "/.config/fish/config.fish", FISH % {"ctl": self._c(self.ctl)})
        os.makedirs(h + "/.config/nushell", exist_ok=True)       # empty configs: nu's first-run "create config files?" question never appears
        for nf in ("config.nu", "env.nu"):
            open(h + "/.config/nushell/" + nf, "a").close()
        hist = history_lines()
        _wr(h + "/.bash_history", "\n".join(hist) + "\n")
        _wr(h + "/.zsh_history", "\n".join(hist) + "\n")
        # remote side
        _wr(self.rhome + "/.bashrc", BASHRC % {"ctl": self._c(self.rctl)})
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
            self.write_ssh_windows(ssh_port, ssh_ident)
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
            self.write_ssh_wrappers()
        return h

    TOOLS = ("ssh", "scp", "sftp")
    WIN_OPENSSH = os.path.join(os.environ.get("SystemRoot", r"C:\Windows"), "System32", "OpenSSH")

    def write_ssh_windows(self, port, ident):
        """Windows: the client is the built-in Win32-OpenSSH (System32\\OpenSSH\\ssh.exe), which reads the user config from the PROFILE
        directory, not %HOME%: so `ssh fake` is resolved through wrappers in <home>/bin (first on the app's PATH) that pass `-F <home>/.ssh/config`:
        `ssh` (sh, written for the Git Bash pane) or `ssh.cmd` (PowerShell pane; found before System32\\OpenSSH\\ssh.exe).
        Win32-OpenSSH refuses key files other users can read, so the scratch key and config get an owner-only ACL (icacls, our own files only)."""
        h = self.home
        os.makedirs(h + "/.ssh", exist_ok=True)
        user = os.environ.get("USERNAME", "runner")
        cfg = h + "/.ssh/config"
        blocks = ["Host fake\n  HostName 127.0.0.1\n  Port %d\n  User %s\n  IdentityFile %s\n  IdentitiesOnly yes\n"
                  "  StrictHostKeyChecking no\n  UserKnownHostsFile %s/.ssh/known_hosts\n  BatchMode yes\n  LogLevel ERROR\n" % (port, user, _fwd(ident), _fwd(h))]
        _wr(cfg, "\n".join(blocks))
        for f in (cfg, ident):
            subprocess.run(["icacls", f, "/inheritance:r", "/grant:r", "%s:F" % user], capture_output=True)
        for t in self.TOOLS:
            real = os.path.join(self.WIN_OPENSSH, t + ".exe")
            if not os.path.isfile(real):
                continue
            if self.pane == "gitbash":
                _wr(os.path.join(h, "bin", t), '#!/bin/sh\n# appci: Win32-OpenSSH reads the profile dir, not HOME (#1481)\nexec "%s" -F "%s" "$@"\n' % (_fwd(real), _fwd(cfg)))
            else:       # PowerShell resolves `ssh` to ssh.cmd (never an extensionless file: it would be "opened" through a file association)
                with open(os.path.join(h, "bin", t + ".cmd"), "w", newline="\r\n") as f:
                    f.write('@echo off\n"%s" -F "%s" %%*\n' % (real, cfg.replace("/", "\\")))

    def write_ssh_wrappers(self):
        """OpenSSH reads ~/.ssh/config from the PASSWD home, not $HOME (#1481): on a runner whose passwd home differs from the scratch HOME,
        `ssh fake` does not resolve. <home>/bin is first on PATH in the shell rc files, so ssh/scp/sftp wrappers there add `-F $HOME/.ssh/config`
        and exec the REAL binary (absolute path resolved now, from a PATH without the wrapper dir, so the wrapper can never exec itself)."""
        bindir = os.path.join(self.home, "bin")
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
        env = dict(os.environ, HOME=self.home, PATH=os.path.join(self.home, "bin") + os.pathsep + os.environ.get("PATH", os.defpath))
        argv = ["ssh", "fake", "true"]
        if self.win:        # run the real client with the config, as the wrapper does (the .cmd is for the shells)
            argv = [os.path.join(self.WIN_OPENSSH, "ssh.exe"), "-F", self.home + "/.ssh/config", "fake", "true"]
        try:
            p = subprocess.run(argv, env=env, capture_output=True, text=True, timeout=timeout, stdin=subprocess.DEVNULL)
        except subprocess.TimeoutExpired:
            return "ssh test host unusable: `ssh fake true` did not finish within %d s" % timeout
        except OSError as e:
            return "ssh test host unusable: cannot run ssh (%s)" % e
        if p.returncode != 0:
            return "ssh test host unusable: `ssh fake true` exited %d: %s" % (p.returncode, (p.stderr or "").strip()[:300])
        return None

    OWNER_SEED = os.path.join(os.path.expanduser("~"), ".config", "phosphor-ci", "owner-settings.toml")
    owner_note = None       # "sha256 <12>" or "not found (bare profile)": reported in results.json meta / summary.md

    def owner_seed(self):
        return os.environ.get("APPCI_OWNER_SETTINGS") or self.OWNER_SEED

    def gitbash_toml(self):
        """TOML string literal of Git for Windows' bash.exe for a Windows Git Bash pane, else None"""
        if not (self.win and self.pane == "gitbash"):
            return None
        import winapi
        b = winapi.git_bash()
        if not b:
            raise RuntimeError("Git for Windows (bash.exe) not found: the Git Bash pane cannot start")
        return '"%s"' % _fwd(b)

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
                t = T.set_key(t, "session", "new_session_shell_override", gb)
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
            lines += ['[session]', 'startup_shell_override = ' + gb, 'new_session_shell_override = ' + gb, '']
        if mode == "agent":
            lines += ['[agents.warp_agent.input]', 'ai_auto_detection_enabled = true', 'nld_in_terminal_enabled = true', '']
        if ollama_url:
            lines += ['[[agents.warp_agent.providers]]', 'id = "appci-ollama"', 'name = "CI Ollama"', 'api_type = "ollama"',
                      'base_url = "%s"' % ollama_url, 'models = [{ name = "%s", id = "%s" }]' % (ollama_model, ollama_model), '']
            lines += ['[agents.byop]', 'last_used_model_id = "byop:appci-ollama:%s"' % ollama_model, '']
        _wr(cfg + "/settings.toml", "\n".join(lines))

    def read(self, which, remote=False):
        p = os.path.join(self.rctl if remote else self.ctl, which)
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
                out.append((float(p[1]), p[2]))
        return out

    def prompts(self, remote=False):
        return len([l for l in self.read("prompt.log", remote) if l.startswith("P")])
