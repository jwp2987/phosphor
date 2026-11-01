//! The consolidated command-vs-agent routing corpus (#1234): every line a routing issue or a
//! routing test has pinned so far, plus the round-18 torture corpus, each with the route it must
//! take and the issue that set it. `routing_corpus_tests.rs` runs the table against the decision
//! procedure; this file holds only data.
//!
//! A case names the facts routing had when the line was typed, because the same text routes
//! differently with different facts: the shell it is typed into, whether an agent conversation is
//! the context, what the session knows about the first word (installed / known absent / cannot
//! say), whether the exact line ran before, and whether the text is typeahead for a running
//! command. `Expect::Model` is a legitimate expectation: the rules must leave the line to the
//! classifier, and no rule may force it either way.

use crate::request_corpus::{PROSE_WITH_COMMAND_HEADS, REAL_COMMANDS_WITH_ENGLISH_ARGS};

/// The shell the line is typed into. bash and zsh share a syntax here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shell {
    Posix,
    Fish,
    PowerShell,
    Nu,
}

/// Where the line is typed: the terminal with no conversation open, or the input of an active
/// agent conversation (the follow-up box after an agent turn).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Surface {
    Plain,
    Conversation,
}

/// What the session knows about the line's first word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Head {
    /// An executable on the session's `$PATH`, or an alias, function, builtin or keyword of its
    /// shell: the completer describes the token and the session answers "known".
    Installed,
    /// The session has a complete command index and the word is not in it.
    Absent,
    /// The session cannot say (no bootstrap data, a `$PATH` scan in flight, stale data).
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Expect {
    Shell,
    Agent,
    /// No rule decides; the classifier does.
    Model,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Typeahead {
    None,
    /// Text the shell handed back that was typed ahead for a running command (#886).
    Running,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Case {
    pub line: &'static str,
    pub shell: Shell,
    pub surface: Surface,
    pub head: Head,
    /// The exact line ran in this session's shell before and exited 0 (#1203).
    pub ran_before: bool,
    pub typeahead: Typeahead,
    pub expect: Expect,
    /// The issue (or owner decision) that set the expectation.
    pub issue: &'static str,
    /// Stage 1: the pre-#1234 procedure is known to miss this case, by this issue. Strict xfail:
    /// the case must fail on that procedure, so a fix elsewhere shows up as an unexpected pass.
    pub v1_xfail: Option<&'static str>,
}

impl Case {
    /// A command typed in the plain terminal whose first word is installed.
    pub const fn cmd(line: &'static str, issue: &'static str) -> Self {
        Self {
            line,
            shell: Shell::Posix,
            surface: Surface::Plain,
            head: Head::Installed,
            ran_before: false,
            typeahead: Typeahead::None,
            expect: Expect::Shell,
            issue,
            v1_xfail: None,
        }
    }

    /// Prose typed in the plain terminal whose first word is no command here.
    pub const fn prose(line: &'static str, issue: &'static str) -> Self {
        Self {
            head: Head::Absent,
            expect: Expect::Agent,
            ..Self::cmd(line, issue)
        }
    }

    /// A line the rules must leave to the classifier.
    pub const fn model(line: &'static str, issue: &'static str) -> Self {
        Self {
            head: Head::Unknown,
            expect: Expect::Model,
            ..Self::cmd(line, issue)
        }
    }

    pub const fn in_conversation(self) -> Self {
        Self {
            surface: Surface::Conversation,
            ..self
        }
    }

    pub const fn head(self, head: Head) -> Self {
        Self { head, ..self }
    }

    pub const fn shell(self, shell: Shell) -> Self {
        Self { shell, ..self }
    }

    pub const fn expect(self, expect: Expect) -> Self {
        Self { expect, ..self }
    }

    pub const fn ran_before(self) -> Self {
        Self {
            ran_before: true,
            ..self
        }
    }

    pub const fn running_typeahead(self) -> Self {
        Self {
            typeahead: Typeahead::Running,
            ..self
        }
    }

    pub const fn v1_xfail(self, issue: &'static str) -> Self {
        Self {
            v1_xfail: Some(issue),
            ..self
        }
    }
}

/// The 212 command lines of the round-18 torture corpus (`round18/torture-agentmode/corpus.py`,
/// `CMDS`), typed at 10-25 ms per key into a bash terminal pane on the Linux box with the owner's
/// configuration. Each is a command with an installed head on that box unless
/// [`TORTURE_OVERRIDES`] says otherwise.
pub(crate) const TORTURE_COMMANDS: &[&str] = &[
    "ls -la",
    "ls",
    "ls -l",
    "ls -a",
    "ls -lh",
    "ls the",
    "ls it",
    "ls this",
    "ls that",
    "ls a",
    "ls -la /tmp",
    "pwd",
    "whoami",
    "date",
    "uname -a",
    "uname -r",
    "hostname",
    "id",
    "df -h",
    "du -sh .",
    "free -m",
    "uptime",
    "env",
    "printenv HOME",
    "echo hello",
    "echo the quick brown fox",
    "echo a b c",
    "echo it works",
    "echo this is that",
    "echo \"do it\"",
    "echo what",
    "echo where",
    "echo say",
    "echo write",
    "printf '%s\\n' this that",
    "cd ..",
    "cd ~/t",
    "cd /tmp",
    "cd -",
    "cd",
    "git status",
    "git log --oneline",
    "git diff",
    "git branch -a",
    "git checkout -",
    "git stash list",
    "git remote -v",
    "git add .",
    "git commit -m \"fix the bug\"",
    "git push",
    "git pull --rebase",
    "git show HEAD",
    "git log --oneline -5",
    "git fetch --all",
    "docker ps",
    "docker ps -a",
    "docker images",
    "docker compose up -d",
    "docker logs web",
    "kubectl get pods",
    "kubectl get svc -A",
    "kubectl describe pod web",
    "kubectl logs -f web",
    "systemctl status nginx",
    "systemctl --user status",
    "systemctl list-units",
    "journalctl -u nginx -n 5",
    "make",
    "make test",
    "make -j4",
    "make clean",
    "python3 x.py",
    "python3 -c \"print(1)\"",
    "python3 --version",
    "python x.py",
    "pip list",
    "pip install requests",
    "npm run build",
    "npm test",
    "npm install",
    "node app.js",
    "node -v",
    "cargo build",
    "cargo test",
    "cargo run",
    "go build",
    "go test ./...",
    "rustc --version",
    "gcc -o a a.c",
    "g++ main.cpp",
    "javac Main.java",
    "java -version",
    "ruby -v",
    "perl -e 'print 1'",
    "php -v",
    "cat /etc/hostname",
    "cat the",
    "cat it",
    "head -n 3 /etc/passwd",
    "tail -n 2 /etc/passwd",
    "wc -l /etc/passwd",
    "sort /etc/hostname",
    "uniq -c /etc/hostname",
    "grep root /etc/passwd",
    "grep -r the .",
    "grep -rn \"a\" big/f00001.dat",
    "find . -name \"*.txt\"",
    "find . -type f -name a",
    "find /tmp -maxdepth 1",
    "xargs echo",
    "awk '{print $1}' /etc/hostname",
    "sed -n 1p /etc/hostname",
    "cut -d: -f1 /etc/passwd",
    "tr a-z A-Z",
    "tee /dev/null",
    "touch a",
    "touch the",
    "mkdir -p x/y",
    "rmdir x/y",
    "cp a b",
    "mv b c",
    "rm -f c",
    "ln -s a l",
    "chmod 644 a",
    "chown $USER a",
    "stat a",
    "file a",
    "diff a b",
    "cmp a b",
    "tar tf x.tgz",
    "gzip -h",
    "zip -h",
    "unzip -h",
    "curl -sI localhost:1",
    "wget -q localhost:1",
    "ping -c1 127.0.0.1",
    "ping -c1 localhost",
    "dig localhost",
    "nslookup localhost",
    "ip a",
    "ip route",
    "ifconfig",
    "ss -tln",
    "netstat -tln",
    "lsof -i :1",
    "ps aux",
    "ps -ef",
    "top -bn1",
    "htop -h",
    "kill -l",
    "jobs",
    "history",
    "alias",
    "type ls",
    "which ls",
    "which it",
    "whereis ls",
    "command -v ls",
    "man -w ls",
    "apropos ls",
    "true",
    "false",
    "test -f a",
    "sleep 0.1",
    "time true",
    "exit 0",
    "clear",
    "reset",
    "bash -c 'echo hi'",
    "sh -c true",
    "source ~/.bashrc",
    ". ~/.profile",
    "export FOO=the",
    "unset FOO",
    "set -o",
    "shopt",
    "ulimit -a",
    "umask",
    "bg",
    "fg",
    "wait",
    "exec true",
    "read -t 0.1 x",
    "eval echo hi",
    "nohup true",
    "nice true",
    "ionice true",
    "watch -h",
    "sudo -n true",
    "ssh -V",
    "scp -h",
    "rsync --version",
    "ssh-add -l",
    "gpg --version",
    "openssl version",
    "lsblk",
    "lscpu",
    "lsusb",
    "lspci",
    "dmesg",
    "free",
    "vmstat 1 1",
    "iostat",
    "sar -h",
    "mount",
    "findmnt",
    "df",
    "du",
    "lsmod",
    "modinfo ext4",
    "uname",
    "arch",
    "nproc",
    "locale",
    "tzselect -h",
    "cal",
    "bc -h",
    "dc -V",
    "expr 1 + 1",
    "seq 1 3",
    "yes the | head -1",
    "factor 12",
    "shuf -e a b c",
    "rev /etc/hostname",
    "nl /etc/hostname",
    "fold /etc/hostname",
    "fmt /etc/hostname",
    "column -t /etc/hostname",
    "paste /etc/hostname",
    "join /etc/hostname /etc/hostname",
    "comm /etc/hostname /etc/hostname",
    "split --help",
    "csplit --help",
    "od -c /etc/hostname",
    "xxd /etc/hostname",
    "hexdump -C /etc/hostname",
    "strings /etc/hostname",
    "base64 /etc/hostname",
    "md5sum /etc/hostname",
    "sha256sum /etc/hostname",
    "cksum /etc/hostname",
    "ls | grep a",
    "ls | head -3",
    "echo a | cat",
    "git log | head -3",
    "ps aux | grep a",
    "a=1; echo $a",
    "echo $HOME",
    "echo $PATH",
    "echo ${HOME}",
    "echo \"$(date)\"",
    "echo `date`",
    "ls && echo ok",
    "false || echo no",
    "ls; ls",
    "ls > /dev/null",
    "ls 2>&1",
    "echo a >> /dev/null",
    "cat < /dev/null",
    "say hello",
    "say the",
    "what",
    "where",
    "write",
    "do",
    "fish",
    "zsh",
    "csh",
    "dash",
    "ksh",
    "tcsh",
    "pwsh",
    "nu",
    "elvish",
    "xonsh",
    "foo",
    "bar",
    "asdf",
    "kubectl",
    "docker",
    "terraform plan",
    "ansible --version",
    "helm list",
    "aws s3 ls",
    "gcloud --version",
    "az --version",
    "heroku --version",
    "vagrant status",
    "brew list",
    "apt list",
    "dnf list",
    "pacman -Q",
    "snap list",
    "flatpak list",
    "rpm -qa",
    "dpkg -l",
    "yum list",
    "zypper se",
    "emerge -s",
];

/// Torture command lines whose facts or expectation differ from "installed head, shell". Every
/// variant of the line is listed; the plain expansion is skipped for these lines.
pub(crate) const TORTURE_OVERRIDES: &[Case] = &[
    // #965 (owner decision 2026-10-26): a greeting after `say` goes to the agent, installed or not.
    Case::cmd("say hello", "#965 owner decision").expect(Expect::Agent),
    Case::prose("say hello", "#965 owner decision"),
    // `say the` is neither a greeting nor a sentence: the macOS command when installed; on the
    // Linux box `say` is absent and English, so the agent (#1185: check the word against `$PATH`).
    Case::cmd("say the", "#965"),
    Case::prose("say the", "#1185"),
    // #1185: `where`, `what`, `write` were counted as commands by the torture run, but only an
    // installed program is one (#965 owner decision). Bare `what` is a one-off natural-language
    // word everywhere.
    Case::prose("what", "#1185 / #965 (one-off natural-language word)"),
    Case::cmd("what", "#965 collision head")
        .expect(Expect::Agent)
        .v1_xfail("#935: a lone installed `what` passes the classifier prelude as a command in the terminal"),
    Case::prose("where", "#1185 (not on `$PATH` in bash)"),
    Case::cmd("where", "#1185 (zsh builtin)"),
    Case::cmd("write", "#1185 (installed)"),
    Case::prose("write", "#1185"),
    // `do` is a reserved word that is never a command by itself (#965).
    Case::cmd("do", "#965 (`do` is never a command on its own)")
        .expect(Expect::Agent)
        .v1_xfail("#965: a described keyword passes the classifier prelude as a command"),
    // #964 / #965 / #1018: a shell name is a command only when the program is installed.
    Case::cmd("fish", "#964"),
    Case::prose("fish", "#965"),
    Case::cmd("zsh", "#964"),
    Case::prose("zsh", "#1018")
        .v1_xfail("#1018: fast Enter has no prose evidence, inherited Shell"),
    Case::cmd("csh", "#964"),
    Case::prose("csh", "#1018"),
    Case::cmd("dash", "#964"),
    Case::model(
        "dash",
        "#1018 (`dash` is neither English nor a listed shell name)",
    )
    .head(Head::Absent),
    Case::cmd("ksh", "#964"),
    Case::prose("ksh", "#1018"),
    Case::cmd("tcsh", "#964"),
    Case::prose("tcsh", "#1018")
        .v1_xfail("#1018: fast Enter has no prose evidence, inherited Shell"),
    Case::cmd("pwsh", "#964"),
    Case::prose("pwsh", "#1018")
        .v1_xfail("#1018: fast Enter has no prose evidence, inherited Shell"),
    Case::cmd("nu", "#964"),
    Case::prose("nu", "#1018").v1_xfail("#1018: fast Enter has no prose evidence, inherited Shell"),
    Case::cmd("elvish", "#964"),
    Case::prose("elvish", "#1018")
        .v1_xfail("#1018: fast Enter has no prose evidence, inherited Shell"),
    Case::cmd("xonsh", "#964"),
    Case::prose("xonsh", "#1018")
        .v1_xfail("#1018: fast Enter has no prose evidence, inherited Shell"),
    // Unknown words: an English one with no command evidence is prose (#696); a non-English one
    // is the classifier's.
    Case::prose("bar", "#696 (English word, no command evidence)"),
    Case::model("foo", "#1185 (not English, not installed)").head(Head::Absent),
    Case::model("asdf", "#1185 (not English, not installed)").head(Head::Absent),
];

/// The 55 prose lines of the round-18 torture corpus (`PROSE0`), typed in the same run. Their
/// first words are no commands on the box, except where [`TORTURE_PROSE_OVERRIDES`] says so.
pub(crate) const TORTURE_PROSE: &[&str] = &[
    "how do I list files here",
    "what is the biggest file in this folder",
    "please explain this error to me",
    "write a python script that prints hello",
    "where is my config file stored",
    "can you help me fix the failing test",
    "what does this command do",
    "show me all running docker containers",
    "why is my build so slow",
    "do you know how to undo a git commit",
    "explain what a segfault is",
    "tell me a joke about programmers",
    "how can I find the largest directories on disk",
    "summarize what changed in the last commit",
    "what is the difference between a thread and a process",
    "write a haiku about the terminal",
    "where can I read about bash arrays",
    "how do I create a new git branch",
    "what should I name this function",
    "help me write a regex for email addresses",
    "is there a way to speed up my rust builds",
    "what is the meaning of exit code 137",
    "could you review my code",
    "why does my script say permission denied",
    "how do I check which process uses port 8080",
    "write a shell one-liner to count lines in all rust files",
    "what is the weather like in a terminal emulator",
    "please refactor this function to be shorter",
    "describe how autosuggestions work",
    "how do I change the default shell",
    "what is a dangling pointer",
    "give me an example of a systemd unit file",
    "how do I rebase onto main",
    "tell me what time it is in tokyo",
    "explain the difference between tcp and udp",
    "what happened to my last command",
    "can you list the files in my home directory and tell me which are large",
    "I need to find where the word foo is defined in this repo",
    "how many files are in this directory",
    "what is my ip address and how do I find it",
    "please write a commit message for my staged changes",
    "why is the sky blue",
    "translate hello world into french",
    "how do I exit vim",
    "what does chmod 755 mean",
    "show me how to use awk to sum a column",
    "do the tests pass right now",
    "explain this stack trace please",
    "what is the best way to learn rust",
    "write a function that reverses a string",
    "how do I make my prompt show the git branch",
    "what is the capital of france",
    "help",
    "what is going on with my disk space",
    "can you check if nginx is running",
    "I want to delete all the log files older than a week",
    "make me a backup script",
    "where did I put that file yesterday",
    "find the process that is eating my memory",
    "say something nice",
    "what do you think about tabs versus spaces",
];

/// Torture prose lines whose first word is a command on the box (`write`, `do`, `help`, `make`,
/// `find`), or that the current procedure is known to miss.
pub(crate) const TORTURE_PROSE_OVERRIDES: &[Case] = &[
    Case::cmd(
        "write a python script that prints hello",
        "#965 collision head",
    )
    .expect(Expect::Agent),
    Case::cmd("write a haiku about the terminal", "#965 collision head").expect(Expect::Agent),
    Case::cmd(
        "write a shell one-liner to count lines in all rust files",
        "#965 collision head",
    )
    .expect(Expect::Agent),
    Case::cmd(
        "write a function that reverses a string",
        "#965 collision head",
    )
    .expect(Expect::Agent),
    Case::cmd(
        "do you know how to undo a git commit",
        "#965 collision head",
    )
    .expect(Expect::Agent),
    Case::cmd("do the tests pass right now", "#965 collision head").expect(Expect::Agent),
    Case::cmd("help me write a regex for email addresses", "#834").expect(Expect::Agent),
    // #1185: bare `help` is a bash builtin and an everyday request; the one prose misroute of the
    // torture run.
    Case::cmd("help", "#1185").expect(Expect::Agent).v1_xfail(
        "#1185: a described builtin passes the classifier prelude as a command in the terminal",
    ),
    Case::cmd("find the process that is eating my memory", "#834").expect(Expect::Agent),
    // `make` is installed; `me` and the article `a` make a sentence out of it.
    Case::cmd("make me a backup script", "#1234 (sentence shape)")
        .expect(Expect::Agent)
        .v1_xfail("#1234: one request word after `make` is not a request to the #834 rule"),
    // A `why` question head: the old rules place these too (stage 1 confirmed no miss).
    Case::prose("why is my build so slow", "#1234 (question head)"),
    Case::prose(
        "why does my script say permission denied",
        "#1234 (question head)",
    ),
];

/// Every pinned case that is not a torture expansion, grouped by the issue that set it.
pub(crate) const CASES: &[Case] = &[
    // -- #808: unambiguous shell syntax is never a question to the model.
    Case::cmd("yes | head -c 50000000 > /dev/null", "#808"),
    Case::cmd("trap 'echo t' DEBUG", "#808"),
    Case::cmd("cat foo.txt | grep -i error", "#808"),
    Case::cmd("echo b30 >> ~/blk.txt", "#886"),
    Case::cmd("echo b196 >> ~/blk.txtecho b199 >> ~/blk.txt", "#886"),
    Case::cmd("echo b129 b131 >> ~/blk.txt", "#886"),
    // -- #886: headless typeahead for a running command is the shell's when it has no prose.
    Case::cmd("ho b199 >> ~/blk.txt", "#886")
        .head(Head::Unknown)
        .running_typeahead(),
    Case::cmd("b22 b23", "#886")
        .head(Head::Unknown)
        .running_typeahead(),
    Case::cmd("xtecho b5 >> ~/blk.txt", "#886")
        .head(Head::Unknown)
        .running_typeahead(),
    Case::model("b22 b23", "#886 (no head the rules recognise)"),
    Case::model("exit vim", "#886 (typeahead prose is the pipeline's)").running_typeahead(),
    Case::model(
        "rm the temp files",
        "#886 (typeahead prose is the pipeline's)",
    )
    .running_typeahead(),
    Case::model("b22 b23 what does this do", "#886").running_typeahead(),
    // -- #696: an English first word with no command evidence is prose, whatever a model says.
    Case::prose("Run exactly this: sleep 8; echo hi > ~/lrc.txt", "#696"),
    Case::prose("run exactly this: sleep 8; echo hi > ~/lrc.txt", "#696"),
    Case::prose("Then run: echo hi > ~/followup.txt", "#696"),
    Case::prose("Then delete the temp directory", "#696"),
    Case::prose("run this; echo hi > x", "#696"),
    Case::prose("FOO=1 run exactly this: sleep 8; echo hi > ~/x", "#696"),
    Case::cmd("cargo --version", "#696 (the pre-filter knows `cargo`)").head(Head::Unknown),
    Case::model(
        "mynewtool arg1 arg2",
        "#696 (index not loaded: do not guess)",
    ),
    Case::cmd("./script.sh a b", "#696 (a path is intent)").head(Head::Unknown),
    Case::cmd("~/bin/x a b", "#696").head(Head::Unknown),
    Case::cmd("FOO=1 mycmd", "#696").head(Head::Unknown),
    Case::cmd("deploy --prod", "#696 (a user function named `deploy`)"),
    Case::cmd("please clean up", "#696 (a user alias named `please`)"),
    Case::cmd("docker run -p 80:80", "#696"),
    Case::cmd("scp host:path .", "#696"),
    Case::cmd("echo a: b", "#696"),
    // -- #831: prose shapes the pre-filter must not claim; what then happens depends on the head.
    Case::cmd(
        "make a PR",
        "#831 / #964 (installed `make`, ordinary arguments)",
    ),
    Case::prose("make a PR", "#831 / #696"),
    Case::cmd(
        "rm -rf node_modules, reinstall",
        "#831 / #964 (installed `rm`, a flag argument)",
    ),
    Case::prose("kill Chrome", "#831 / #696"),
    Case::cmd("kill Chrome please", "#1234 (sentence shape)")
        .expect(Expect::Agent)
        .v1_xfail("#1234: `please` alone is not a request to the #834 rule"),
    // -- #832 / #964: a sentence headed by an installed command with no request shape runs
    // (owner decision #964: an installed command with ordinary arguments runs in the shell).
    Case::cmd("find the bug in main.rs", "#964 / #906"),
    Case::prose("find the bug in main.rs", "#832 / #696"),
    Case::cmd("find large files", "#964 / #906"),
    // -- #833: speed must not decide. `yes please` is a request however fast it was typed.
    Case::cmd("yes please", "#833 F8")
        .expect(Expect::Agent)
        .v1_xfail(
            "#833: two tokens with a described head pass the classifier prelude as a command",
        ),
    Case::prose("summarize the open issues and tell me what to fix", "#833"),
    Case::prose("tell Josh what happened please", "#833"),
    Case::prose("frobnicate everything for me please", "#833"),
    Case::model(
        "newtool deploy the app to staging",
        "#833 (head unknown: do not guess)",
    ),
    Case::model("poetry add requests numpy", "#833"),
    Case::model("mytool --force run migrations for prod", "#833"),
    Case::cmd("tell Josh when my build finished", "#1234 (sentence shape)")
        .expect(Expect::Agent)
        .v1_xfail("#1234: a session that knows `tell` kept the inherited Shell"),
    Case::prose("tell Josh when my build finished", "#833"),
    Case::prose("frobnicate which tests can fail", "#833")
        .v1_xfail("#833: only Enter has the unknown-head prose rule; the keystroke path has none"),
    Case::prose("explain where my config lives", "#833"),
    Case::prose(
        "tell Josh about outages",
        "#1234 (request opener, not installed)",
    ),
    // -- #834: prose headed by an installed command reads as a request.
    Case::cmd("help me", "#834").expect(Expect::Agent),
    Case::cmd("watch the logs for errors", "#834").expect(Expect::Agent),
    Case::cmd("make sure the build passes", "#834").expect(Expect::Agent),
    Case::prose("can you fix this", "#834"),
    Case::prose("why is the build slow", "#834"),
    Case::prose("docker vs podman?", "#834"),
    Case::cmd("docker vs podman?", "#834").expect(Expect::Agent),
    Case::prose("how do I pipe ls into grep", "#808"),
    Case::prose("what does > mean?", "#808"),
    Case::prose("explain the difference between > and >>", "#808"),
    Case::prose(
        "find all files > 1MB",
        "#808 (a comparison, not a redirect; `find` absent)",
    ),
    Case::cmd("ls > listing.txt", "#808"),
    Case::cmd(
        "export data > backup.csv",
        "#808 / #964 (the pre-filter leaves it; installed `export` runs it)",
    ),
    // -- #897 / #906: real commands with English arguments stay commands.
    Case::cmd("grep -rn this src", "#897"),
    Case::cmd("git commit -m the fix", "#897"),
    Case::cmd("grep this that", "#906"),
    Case::cmd("make the build", "#906"),
    Case::cmd("which the thing", "#906"),
    Case::cmd("nohup grep this the file", "#906"),
    Case::cmd("ssh host grep the log", "#906"),
    Case::cmd("ollama run llama3 why is the sky blue", "#906"),
    Case::model("grep this that", "#906 (no session data: the classifier's)"),
    Case::model("make the build", "#906"),
    Case::model("nohup grep this the file", "#906"),
    Case::cmd(
        "find the bug\nin main.rs",
        "#906 (multi-line is never vetoed to the shell)",
    )
    .expect(Expect::Model),
    // -- #933 / #1184: slash spellings are never paths to run.
    Case::cmd("/agent go", "#933")
        .head(Head::Absent)
        .expect(Expect::Agent)
        .v1_xfail("#1184: outside a conversation a slash spelling is left to the classifier"),
    Case::cmd("/mod", "#1184")
        .head(Head::Absent)
        .in_conversation()
        .expect(Expect::Agent),
    Case::cmd("/model gpt", "#1184")
        .head(Head::Absent)
        .in_conversation()
        .expect(Expect::Agent),
    Case::cmd("/", "#1184")
        .head(Head::Absent)
        .in_conversation()
        .expect(Expect::Agent),
    Case::cmd("/mod", "#1184")
        .head(Head::Absent)
        .expect(Expect::Agent)
        .v1_xfail("#1184: outside a conversation a slash spelling is left to the classifier"),
    Case::cmd("/usr/bin/ls", "#1184 (a path)").in_conversation(),
    Case::cmd("/usr/bin/ls -la", "#1184").in_conversation(),
    // -- #935: a lone word the session knows nothing by is a message to the agent.
    Case::prose("frobxone", "#935")
        .in_conversation()
        .v1_xfail("#935: fast Enter has no prose evidence and keeps an inherited Shell"),
    Case::prose("zzqx", "#935")
        .in_conversation()
        .v1_xfail("#935: fast Enter has no prose evidence and keeps an inherited Shell"),
    Case::model(
        "frobxone",
        "#935 (outside a conversation the classifier decides)",
    )
    .head(Head::Absent),
    Case::cmd("ls", "#935").in_conversation(),
    Case::cmd("git", "#935").in_conversation(),
    Case::cmd("deploy", "#935 (a user function)")
        .in_conversation()
        .v1_xfail("#1185: at a fast Enter no parse describes `deploy`, so it lacks evidence"),
    // -- #964: pronoun arguments to installed commands.
    Case::cmd("ls this that it", "#964"),
    Case::cmd("ls that is it", "#964"),
    Case::cmd("cat this that", "#964"),
    Case::cmd("cp it that", "#964"),
    Case::cmd("rm this that it", "#964"),
    Case::cmd("grep it this", "#964"),
    Case::cmd("git add this that it", "#964"),
    Case::cmd("ls -la this that it", "#964"),
    Case::cmd("fish --version", "#964"),
    Case::cmd("fish is great", "#965 owner decision (installed)"),
    Case::prose("fish is great", "#965 owner decision (absent)"),
    Case::cmd("ruby is great", "#965 owner decision (installed)"),
    Case::prose("ruby is great", "#965 owner decision (absent)")
        .v1_xfail("#965: `ruby` is not a dictionary word, so only the model routes it"),
    Case::prose("swift vs kotlin?", "#965"),
    // -- #965: macOS / zsh collision words.
    Case::cmd("say hello world", "#965 owner decision").expect(Expect::Agent),
    Case::cmd("say how are you", "#965").expect(Expect::Agent),
    Case::cmd("say \"build done\"", "#965"),
    Case::cmd("say -v Alex hello world", "#965"),
    Case::cmd("say build done", "#965"),
    Case::cmd("what does ls -la do", "#965").expect(Expect::Agent),
    Case::cmd("what is ls", "#965"),
    Case::cmd("what /usr/bin/ls", "#965"),
    Case::cmd("where is the config", "#965").expect(Expect::Agent),
    Case::cmd("where is git", "#965"),
    Case::cmd("where git", "#965"),
    Case::prose("where is git", "#965 (bash: `where` is no command)"),
    Case::cmd("write a script that counts lines", "#965").expect(Expect::Agent),
    Case::cmd("write user", "#965"),
    Case::cmd("do it", "#965").expect(Expect::Agent),
    Case::cmd("do ls", "#965"),
    Case::prose("say something nice", "#1185 (Linux: `say` absent)"),
    // -- #1018 / #1019: installed decides, and installing mid-session counts.
    Case::prose("ksh is great", "#1018"),
    Case::prose("tcsh is great", "#1018"),
    Case::cmd("ksh is great", "#1018 (installed)"),
    Case::prose("zorktool the file", "#1019 (before the install)")
        .v1_xfail("#1019: `zorktool` is not a dictionary word, so only the model routes it"),
    Case::cmd("zorktool the file", "#1019 (after `export PATH=...`)"),
    Case::model("zorktool the file", "#1019 (session cannot say)"),
    Case::prose("zorkleN is great", "#1019")
        .v1_xfail("#1019: `zorkleN` is not a dictionary word, so only the model routes it"),
    Case::cmd("zorkleN is great", "#1019 (installed)"),
    // -- #1183: in a conversation a line needs positive command evidence.
    Case::cmd("make commit port", "#1183")
        .in_conversation()
        .expect(Expect::Agent),
    Case::cmd("last about build script", "#1183")
        .in_conversation()
        .expect(Expect::Agent),
    Case::cmd("git can memory my to", "#1183")
        .in_conversation()
        .expect(Expect::Agent),
    Case::cmd("for tell where\u{2026}", "#1183")
        .in_conversation()
        .expect(Expect::Agent),
    Case::cmd("make more sense", "#1183")
        .in_conversation()
        .expect(Expect::Agent),
    Case::cmd("git is great", "#1183")
        .in_conversation()
        .expect(Expect::Agent),
    Case::cmd("help", "#1185")
        .in_conversation()
        .expect(Expect::Agent),
    Case::cmd(
        "make foo baz",
        "#1183 (not routine targets: no evidence in a conversation)",
    )
    .in_conversation()
    .expect(Expect::Agent),
    Case::cmd(
        "frobnicate everything now",
        "#1183 (unknown head is no evidence)",
    )
    .head(Head::Absent)
    .in_conversation()
    .expect(Expect::Agent)
    .v1_xfail("#1183: a non-English unknown head counted as evidence"),
    Case::cmd("make commit port", "#1183 (plain terminal: out of scope)"),
    Case::cmd("ls this that it", "#1183").in_conversation(),
    Case::cmd("ls the", "#1183").in_conversation(),
    Case::cmd("cat this that", "#1183").in_conversation(),
    Case::cmd("touch the", "#1183").in_conversation(),
    Case::cmd("git add this that it", "#1183").in_conversation(),
    Case::cmd("git commit -m \"fix the thing\"", "#1183").in_conversation(),
    Case::cmd("git commit -m fix", "#1183").in_conversation(),
    Case::cmd("git status", "#1183").in_conversation(),
    Case::cmd("systemctl status nginx", "#1183").in_conversation(),
    Case::cmd("echo the quick brown fox", "#1183").in_conversation(),
    Case::cmd("grep -rn this src", "#1183").in_conversation(),
    Case::cmd("docker ps", "#1183").in_conversation(),
    Case::cmd("cargo build", "#1183").in_conversation(),
    Case::cmd("npm run build", "#1183").in_conversation(),
    Case::cmd("kubectl get pods", "#1183").in_conversation(),
    Case::cmd("make", "#1183").in_conversation(),
    Case::cmd("make test", "#1183").in_conversation(),
    Case::cmd("make -j4", "#1183").in_conversation(),
    Case::cmd("make clean install", "#1183").in_conversation(),
    Case::cmd("write", "#1185 (installed)")
        .in_conversation()
        .v1_xfail("#1185: at a fast Enter no parse describes `write`, so it lacks evidence"),
    Case::cmd("write alice", "#1185").in_conversation(),
    Case::cmd("write alice pts/1", "#1185").in_conversation(),
    Case::cmd("ls -la /tmp", "#1183").in_conversation(),
    Case::cmd("python3 x.py", "#1183").in_conversation(),
    Case::cmd("sudo systemctl restart nginx", "#1183").in_conversation(),
    // -- #1291: in a conversation `sudo` is judged on what it wraps, like env/time/timeout.
    Case::cmd("sudo apt update", "#1291").in_conversation(),
    Case::cmd("sudo -s", "#1291").in_conversation(),
    Case::cmd("sudo", "#1291").in_conversation(),
    Case::cmd("sudo rm the old logs for me", "#1291")
        .in_conversation()
        .expect(Expect::Agent),
    Case::cmd("sudo please fix my wifi", "#1291")
        .in_conversation()
        .expect(Expect::Agent),
    // The plain terminal is unchanged by #1291: request shape (#834/#965: two distinct request
    // words, one of them addressed to someone) sends `rm the old logs for me` to the agent with or
    // without the `sudo` in front, as it does `look at the logs for me`. The owner's
    // installed-head rule (#964) covers English-looking arguments, not a sentence.
    Case::cmd(
        "sudo rm the old logs for me",
        "#1291 (plain terminal unchanged)",
    )
    .expect(Expect::Agent),
    Case::cmd("for i in a b; do echo $i; done", "#1183").in_conversation(),
    Case::cmd("ls | grep a", "#1183").in_conversation(),
    Case::cmd("./run.sh --fast", "#1183").in_conversation(),
    Case::cmd(
        "mytool --json",
        "#1183 (data-shaped arguments are evidence)",
    )
    .head(Head::Absent)
    .in_conversation()
    .expect(Expect::Model),
    // -- #1185: a trailing comment is not part of the command.
    Case::cmd("rec    # for its own body of text", "#1185"),
    Case::cmd("myscript # tell me why the build failed", "#1185")
        .v1_xfail("#1185: the Enter-time request test reads the comment's words"),
    Case::cmd("rec    # for its own body of text", "#1185").in_conversation(),
    Case::cmd("write", "#1185 (installed `write` is a command)"),
    Case::cmd("write alice", "#1185"),
    // -- #1203: a known CLI's own subcommand is not prose; an exact successful run is a command.
    Case::cmd("git status", "#1203"),
    Case::cmd("git status --short", "#1203"),
    Case::cmd("systemctl status nginx", "#1203"),
    Case::cmd("systemctl restart nginx.service", "#1203"),
    Case::cmd("docker ps", "#1203"),
    Case::cmd("git commit -m 'fix the login bug'", "#1203"),
    Case::cmd("git log --since yesterday", "#1203").ran_before(),
    Case::cmd(
        "git log --since yesterday",
        "#1203 (installed `git`, no request shape)",
    ),
    Case::model(
        "git log --since yesterday",
        "#1203 (no session data, never ran)",
    ),
    Case::cmd("npm install everything", "#1203 (installed `npm`)"),
    // A bare trailing `?` is a glob character and stays shell (`reads_as_command_headed_prose`).
    Case::cmd("git status please?", "#1203 (trailing `?` is a glob)"),
    // -- #1036 / #1174: an oversized buffer never changes the input type (added in
    //    `all_cases`, built at run time).
    // -- #1234: PowerShell 5.1 / pwsh 7.
    Case::cmd("Get-ChildItem -Recurse", "#1234 PowerShell").shell(Shell::PowerShell),
    Case::cmd(
        "Get-Process | Sort-Object CPU -Descending | Select-Object -First 5",
        "#1234 PowerShell",
    )
    .shell(Shell::PowerShell),
    Case::cmd("Get-Date", "#1234 PowerShell").shell(Shell::PowerShell),
    Case::cmd("Write-Host hello world", "#1234 PowerShell").shell(Shell::PowerShell),
    Case::cmd("$x = Get-Date", "#1234 PowerShell")
        .shell(Shell::PowerShell)
        .head(Head::Absent)
        .v1_xfail("#1234: a `$variable` head is nothing the old rules know; inherited AI stands"),
    Case::cmd("$env:PATH -split ';'", "#1234 PowerShell")
        .shell(Shell::PowerShell)
        .head(Head::Absent)
        .v1_xfail("#1234: a `$variable` head is nothing the old rules know; inherited AI stands"),
    Case::cmd(
        "ls | Where-Object { $_.Length -gt 1kb }",
        "#1234 PowerShell",
    )
    .shell(Shell::PowerShell),
    Case::cmd("Set-Location C:\\Users\\josh", "#1234 PowerShell").shell(Shell::PowerShell),
    Case::cmd("Get-Service nginx | Restart-Service", "#1234 PowerShell").shell(Shell::PowerShell),
    Case::cmd("gci -Recurse *.rs | sls \"TODO\"", "#1234 PowerShell").shell(Shell::PowerShell),
    Case::cmd("dir", "#1234 PowerShell").shell(Shell::PowerShell),
    Case::cmd("echo $env:PATH", "#1234 PowerShell").shell(Shell::PowerShell),
    Case::cmd(
        "Remove-Item -Recurse -Force node_modules",
        "#1234 PowerShell",
    )
    .shell(Shell::PowerShell),
    Case::cmd("winget install Microsoft.PowerToys", "#1234 PowerShell").shell(Shell::PowerShell),
    Case::cmd(
        "Invoke-WebRequest https://example.com -OutFile x.zip",
        "#1234 PowerShell",
    )
    .shell(Shell::PowerShell),
    Case::cmd(
        "Get-Content log.txt | Select-String error",
        "#1234 PowerShell",
    )
    .shell(Shell::PowerShell),
    Case::cmd("Get-ChildItem -Recurse", "#1234 PowerShell")
        .shell(Shell::PowerShell)
        .in_conversation(),
    Case::cmd("Write-Host hello world", "#1234 PowerShell")
        .shell(Shell::PowerShell)
        .in_conversation(),
    Case::cmd("$x = Get-Date", "#1234 PowerShell")
        .shell(Shell::PowerShell)
        .head(Head::Absent)
        .in_conversation()
        .v1_xfail("#1234: a `$variable` head is nothing the old rules know; inherited AI stands"),
    Case::prose(
        "what is the difference between Get-ChildItem and dir",
        "#1234 PowerShell",
    )
    .shell(Shell::PowerShell),
    Case::prose(
        "how do I list hidden files in PowerShell",
        "#1234 PowerShell",
    )
    .shell(Shell::PowerShell),
    Case::cmd(
        "where is my profile script",
        "#1234 PowerShell (`where` is an alias)",
    )
    .shell(Shell::PowerShell)
    .expect(Expect::Agent),
    Case::cmd(
        "write a function that returns the date",
        "#1234 PowerShell (`write` is an alias)",
    )
    .shell(Shell::PowerShell)
    .expect(Expect::Agent),
    // -- #1234: nushell.
    Case::cmd("ls | where size > 1kb", "#1234 nushell").shell(Shell::Nu),
    Case::cmd("open Cargo.toml | get package.name", "#1234 nushell").shell(Shell::Nu),
    Case::cmd("let x = 5", "#1234 nushell").shell(Shell::Nu),
    Case::cmd("let name = \"world\"", "#1234 nushell").shell(Shell::Nu),
    Case::cmd("def greet [] { echo hi }", "#1234 nushell").shell(Shell::Nu),
    Case::cmd("$env.PATH | split row (char esep)", "#1234 nushell")
        .shell(Shell::Nu)
        .head(Head::Absent),
    Case::cmd("http get https://api.github.com", "#1234 nushell").shell(Shell::Nu),
    Case::cmd("ls | sort-by size | first 3", "#1234 nushell").shell(Shell::Nu),
    Case::cmd("print \"hello\"", "#1234 nushell").shell(Shell::Nu),
    Case::cmd("version", "#1234 nushell").shell(Shell::Nu),
    Case::cmd("sys host", "#1234 nushell").shell(Shell::Nu),
    Case::cmd("help commands", "#1234 nushell").shell(Shell::Nu),
    Case::cmd("ls | where size > 1kb", "#1234 nushell")
        .shell(Shell::Nu)
        .in_conversation(),
    Case::cmd("let x = 5", "#1234 nushell")
        .shell(Shell::Nu)
        .in_conversation(),
    Case::prose(
        "how do I filter rows where size is bigger than 1kb",
        "#1234 nushell",
    )
    .shell(Shell::Nu),
    Case::cmd(
        "let me know when the build finishes",
        "#1234 nushell (`let` is a keyword)",
    )
    .shell(Shell::Nu)
    .expect(Expect::Agent),
    // -- #1234: fish.
    Case::cmd("set -x EDITOR nvim", "#1234 fish").shell(Shell::Fish),
    Case::cmd("for f in *.txt; echo $f; end", "#1234 fish").shell(Shell::Fish),
    Case::cmd("function greet; echo hi; end", "#1234 fish").shell(Shell::Fish),
    Case::cmd("string replace foo bar $x", "#1234 fish").shell(Shell::Fish),
    Case::cmd("abbr -a gs git status", "#1234 fish").shell(Shell::Fish),
    Case::cmd("fish_add_path ~/bin", "#1234 fish").shell(Shell::Fish),
    Case::cmd("not test -f x", "#1234 fish").shell(Shell::Fish),
    Case::cmd("math 2 + 2", "#1234 fish").shell(Shell::Fish),
    Case::cmd("count *.rs", "#1234 fish").shell(Shell::Fish),
    Case::cmd("alias ll 'ls -la'", "#1234 fish").shell(Shell::Fish),
    Case::cmd("set -q FOO; and echo yes", "#1234 fish").shell(Shell::Fish),
    Case::cmd("echo (date)", "#1234 fish").shell(Shell::Fish),
    Case::cmd("set -x EDITOR nvim", "#1234 fish")
        .shell(Shell::Fish)
        .in_conversation(),
    Case::cmd("string replace foo bar $x", "#1234 fish")
        .shell(Shell::Fish)
        .in_conversation(),
    Case::prose("how do I set an environment variable in fish", "#1234 fish").shell(Shell::Fish),
    Case::cmd(
        "set the editor to nvim please",
        "#1234 fish (`set` is a builtin)",
    )
    .shell(Shell::Fish)
    .expect(Expect::Agent),
    // -- #1234: remote shells whose installed commands differ from the local machine's.
    Case::cmd(
        "apt list --installed",
        "#1234 remote (absent locally, known CLI)",
    )
    .head(Head::Absent),
    Case::cmd(
        "brew install ripgrep",
        "#1234 remote (absent on Linux, known CLI)",
    )
    .head(Head::Absent),
    Case::cmd("kubectl get pods -A", "#1234 remote").head(Head::Absent),
    Case::cmd(
        "systemctl status nginx",
        "#1234 remote (installed on the remote)",
    ),
    Case::model(
        "systemctl please restart nginx",
        "#1203 (the pre-filter leaves it)",
    ),
];

/// Every case: the pinned ones, the torture corpus expanded with its overrides, the #897 request
/// corpus, and the oversized-paste case built at run time (#1036 / #1174).
pub(crate) fn all_cases() -> Vec<Case> {
    let mut cases: Vec<Case> = CASES.to_vec();
    for line in TORTURE_COMMANDS {
        let overrides: Vec<Case> = TORTURE_OVERRIDES
            .iter()
            .filter(|case| case.line == *line)
            .copied()
            .collect();
        if overrides.is_empty() {
            cases.push(Case::cmd(line, "#1185 torture corpus (command)"));
        } else {
            cases.extend(overrides);
        }
    }
    for line in TORTURE_PROSE {
        let overrides: Vec<Case> = TORTURE_PROSE_OVERRIDES
            .iter()
            .filter(|case| case.line == *line)
            .copied()
            .collect();
        if overrides.is_empty() {
            cases.push(Case::prose(line, "#1185 torture corpus (prose)"));
        } else {
            cases.extend(overrides);
        }
    }
    for line in REAL_COMMANDS_WITH_ENGLISH_ARGS {
        cases.push(Case::cmd(line, "#897 request corpus (real command)"));
    }
    for line in PROSE_WITH_COMMAND_HEADS {
        cases.push(Case::cmd(line, "#897 / #965 request corpus (prose)").expect(Expect::Agent));
    }
    // #1036 / #1174: a paste over every size cap is never classified, so no rule may move it.
    let oversized: &'static str = Box::leak(format!("ls {}", "-a ".repeat(3000)).into_boxed_str());
    cases.push(Case::model(
        oversized,
        "#1036 / #1174 (oversized buffer keeps its type)",
    ));
    cases
}
