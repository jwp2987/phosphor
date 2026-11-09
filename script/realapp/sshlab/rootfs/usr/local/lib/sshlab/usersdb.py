# usersdb.py: the lab's user table (users.tsv) as data. Pure Python, stdlib only; used by build-users.sh (via `usersdb.py useradd`),
# labd.py (inside the container) and the harness selftest (appci/selftest.py loads it by path). No I/O except reading the file given.
import os, sys

HERE = os.path.dirname(os.path.abspath(__file__))
SHELLS = {"bash": "/bin/bash", "zsh": "/usr/bin/zsh", "fish": "/usr/bin/fish", "nu": "/opt/nu/nu", "pwsh": "/opt/pwsh/pwsh",
          "dash": "/bin/dash", "ash": "/usr/local/bin/ash", "rbash": "/bin/rbash"}
# profile -> how labd builds the home. home: "normal" | "ro" (root-owned, mode 555) | "none" (the account's home does not exist).
# rc: the bash scenario fragment scenarios/<profile>.sh is sourced after the hook (a profile without a file adds nothing).
PROFILES = {
    "default": dict(home="normal"), "banner": dict(home="normal"), "rcerr": dict(home="normal"), "rcprompt": dict(home="normal"),
    "slowrc": dict(home="normal"), "colon": dict(home="normal"), "question": dict(home="normal"), "sete": dict(home="normal"),
    "exec": dict(home="normal"), "rohome": dict(home="ro"), "nohome": dict(home="none"), "tinypath": dict(home="normal"),
    "noinstall": dict(home="normal", ro_phosphor=True), "sudo": dict(home="normal"),
}
# users that authenticate with the unix password / keyboard-interactive (labd sets the password, pam.d/sshd keys on these names)
PASSWORD_USERS = ("p_password", "p_kbdint")
MFA_USERS = ("p_mfa",)
SLOWAUTH_USERS = ("p_slowauth",)


class UserRow:
    def __init__(self, user, shell, profile, sshd):
        self.user, self.shell, self.profile, self.sshd = user, shell, profile, sshd

    def directives(self):
        return [d.strip() for d in self.sshd.split(";") if d.strip()] if self.sshd != "-" else []

    def __repr__(self):
        return "UserRow(%r, %r, %r, %r)" % (self.user, self.shell, self.profile, self.sshd)


def parse_users(text):
    """users.tsv text -> [UserRow]. Comment (#) and blank lines are skipped. A malformed row, an unknown shell or profile, or a duplicate user raises ValueError."""
    rows, seen = [], set()
    for n, line in enumerate(text.splitlines(), 1):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        f = line.rstrip("\n").split("\t")
        if len(f) != 4:
            raise ValueError("users.tsv line %d: expected 4 tab-separated fields, got %d" % (n, len(f)))
        user, shell, profile, sshd = [x.strip() for x in f]
        if not user.replace("_", "").isalnum():
            raise ValueError("users.tsv line %d: bad user name %r" % (n, user))
        if shell not in SHELLS:
            raise ValueError("users.tsv line %d: unknown shell %r" % (n, shell))
        if profile not in PROFILES:
            raise ValueError("users.tsv line %d: unknown profile %r" % (n, profile))
        if user in seen:
            raise ValueError("users.tsv line %d: duplicate user %r" % (n, user))
        seen.add(user)
        rows.append(UserRow(user, shell, profile, sshd))
    return rows


def load(path=None):
    return parse_users(open(path or os.path.join(HERE, "users.tsv")).read())


def match_blocks(rows):
    """the sshd_config `Match User` blocks for the rows that carry directives (first match wins per keyword in sshd, so one block per user)"""
    out = []
    for r in rows:
        d = r.directives()
        if d:
            out.append("Match User %s\n%s" % (r.user, "".join("    %s\n" % x for x in d)))
    return "".join(out)


def etc_shells(rows):
    return sorted({SHELLS[r.shell] for r in rows} | {"/bin/sh"})


def useradd_commands(rows):
    """argv lists for build-users.sh: accounts WITHOUT a home (labd creates homes at lease time on a tmpfs /home)"""
    return [["useradd", "-M", "-d", "/home/" + r.user, "-s", SHELLS[r.shell], "-U", r.user] for r in rows]


if __name__ == "__main__":
    rows = load(sys.argv[2] if len(sys.argv) > 2 else None)
    if sys.argv[1:2] == ["useradd"]:
        for c in useradd_commands(rows):
            print(" ".join(c))
    elif sys.argv[1:2] == ["shells"]:
        print("\n".join(etc_shells(rows)))
    else:
        sys.exit("usage: usersdb.py useradd|shells [users.tsv]")
