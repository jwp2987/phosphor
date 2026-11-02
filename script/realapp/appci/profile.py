# appci/profile.py: builds the scratch HOME (owner-like config, NO credentials), the shell logging rc files
# and the test tree. Never reads the owner's real config: CI has none. The owner-like settings are the ones in
# docs/testing.md 3.1 (agent mode default, autodetect + natural-language detection on, vertical tabs are not
# set because they do not affect input), plus a provider ONLY when the endpoint is configured (ci_env.py: env or ~/.config/phosphor-ci/ci.env).
import os, random, shutil

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
    def __init__(self, run, shell_home_name="home"):
        self.run = run
        self.home = os.path.join(run, shell_home_name)
        self.ctl = os.path.join(run, "ctl")
        self.rctl = os.path.join(run, "rctl")          # the ssh "remote" side logs here
        self.rhome = os.path.join(run, "rhome")
        self.shots = os.path.join(run, "shots")

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
        open(h + "/bin/now", "w").write("#!/bin/sh\nexec python3 -c 'import time;print(\"%.9f\" % time.time())'\n")
        os.chmod(h + "/bin/now", 0o755)
        open(h + "/.bashrc", "w").write(BASHRC % {"ctl": self.ctl})
        open(h + "/.bash_profile", "w").write(BASH_PROFILE)
        open(h + "/.zshrc", "w").write(ZSHRC % {"ctl": self.ctl})
        os.makedirs(h + "/.config/fish", exist_ok=True)
        open(h + "/.config/fish/config.fish", "w").write(FISH % {"ctl": self.ctl})
        os.makedirs(h + "/.config/nushell", exist_ok=True)       # empty configs: nu's first-run "create config files?" question never appears
        for nf in ("config.nu", "env.nu"):
            open(h + "/.config/nushell/" + nf, "a").close()
        hist = history_lines()
        open(h + "/.bash_history", "w").write("\n".join(hist) + "\n")
        open(h + "/.zsh_history", "w").write("\n".join(hist) + "\n")
        # remote side
        open(self.rhome + "/.bashrc", "w").write(BASHRC % {"ctl": self.rctl})
        open(self.rhome + "/.bash_profile", "w").write(BASH_PROFILE)
        open(self.rhome + "/.bash_history", "w").write("\n".join(history_lines(2200, 2)) + "\n")
        # test tree for Tab completion
        t = h + "/w/tc"
        os.makedirs(t, exist_ok=True)
        for d in ("with space", "alpha", "alphabet", "alpine"):
            os.makedirs(os.path.join(h, "w", d), exist_ok=True)
        open(t + "/zzcompl_alpha_unique.txt", "w").close()
        self.write_settings(mode, ollama_url, ollama_model)
        if ssh_port:
            os.makedirs(h + "/.ssh", mode=0o700, exist_ok=True)
            open(h + "/.ssh/config", "w").write(
                "Host fake\n  HostName 127.0.0.1\n  Port %d\n  User %s\n  IdentityFile %s\n  IdentitiesOnly yes\n"
                "  StrictHostKeyChecking no\n  UserKnownHostsFile %s/.ssh/known_hosts\n  BatchMode yes\n  LogLevel ERROR\n"
                % (ssh_port, os.environ.get("USER", "runner"), ssh_ident, h))
            pw = os.path.join(os.path.dirname(ssh_ident), "pwkey")      # sshd.py: passphrase-protected key (openssh kind only)
            if os.path.exists(pw):
                open(h + "/.ssh/config", "a").write(
                    "\nHost fakepw\n  HostName 127.0.0.1\n  Port %d\n  User %s\n  IdentityFile %s\n  IdentitiesOnly yes\n"
                    "  StrictHostKeyChecking no\n  UserKnownHostsFile %s/.ssh/known_hosts\n  BatchMode no\n  NumberOfPasswordPrompts 1\n  LogLevel ERROR\n"
                    % (ssh_port, os.environ.get("USER", "runner"), pw, h))
        return h

    OWNER_SEED = os.path.join(os.path.expanduser("~"), ".config", "phosphor-ci", "owner-settings.toml")
    owner_note = None       # "sha256 <12>" or "not found (bare profile)": reported in results.json meta / summary.md

    def owner_seed(self):
        return os.environ.get("APPCI_OWNER_SETTINGS") or self.OWNER_SEED

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
            if mode == "agent":
                t = T.set_key(t, "agents.warp_agent.input", "ai_auto_detection_enabled", "true")
                t = T.set_key(t, "agents.warp_agent.input", "nld_in_terminal_enabled", "true")
            if ollama_url:      # the owner's list also names a laptop-only proxy: the CI provider list is the Ollama endpoint alone, and it is the ACTIVE model
                t = T.set_key(t, "agents.warp_agent", "providers", '[ { id = "appci-ollama", name = "CI Ollama", api_type = "ollama", base_url = "%s", models = [ { name = "%s", id = "%s" } ] } ]' % (ollama_url, ollama_model, ollama_model))
                t = T.set_key(t, "agents.byop", "last_used_model_id", '"byop:appci-ollama:%s"' % ollama_model)
            open(cfg + "/settings.toml", "w").write(t)
            os.chmod(cfg + "/settings.toml", 0o600)
            return
        Profile.owner_note = "not found (bare profile)"
        lines = ['[warpify.ssh]', 'ssh_extension_install_mode = "never_install"', '',
                 '[general]', 'default_session_mode = "%s"' % m, 'restore_session = false', '']
        if mode == "agent":
            lines += ['[agents.warp_agent.input]', 'ai_auto_detection_enabled = true', 'nld_in_terminal_enabled = true', '']
        if ollama_url:
            lines += ['[[agents.warp_agent.providers]]', 'id = "appci-ollama"', 'name = "CI Ollama"', 'api_type = "ollama"',
                      'base_url = "%s"' % ollama_url, 'models = [{ name = "%s", id = "%s" }]' % (ollama_model, ollama_model), '']
            lines += ['[agents.byop]', 'last_used_model_id = "byop:appci-ollama:%s"' % ollama_model, '']
        open(cfg + "/settings.toml", "w").write("\n".join(lines))

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
