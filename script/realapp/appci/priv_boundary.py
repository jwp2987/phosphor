# appci/priv_boundary.py: what the real privilege-boundary cases need from the test machine (r14-test-gaps T47, round 15 wave 5c):
# sudo_su_real_shell (cases_coverage_a.py), subshell_ready_relay_shape and subshell_ready_docker_t_without_i (cases_subshell_ready.py).
# Until round 15 the three SKIPPED on every machine. They asked for passwordless ROOT sudo of the CI user, which no test machine has and which is
# more than the cases need: the boundary under test is "another uid behind sudo", not root. This module finds what the machine offers and, when it
# offers nothing, says the exact machine change (the harness never edits sudoers, adds a user, pulls an image or starts a daemon).
#
#   sudo_target()   who the pane may become without a password:
#                     APPCI_SUDO_USER=<user> (environment, or the machine-local ~/.config/phosphor-ci/ci.env) and `sudo -n -u <user> true` succeeds
#                       -> that user (an unprivileged test user: the smallest machine change, the CI user never becomes root);
#                     else `sudo -n true` succeeds -> root (the machine already lets the CI user be root);
#                     else not ok, with the reason.
#                   A target with the CI user's own uid is refused: there would be no boundary and `id -u` could not tell the shells apart.
#   sudo_needs()    the NEEDS: text for a machine with neither.
#   shared_dir()    a directory BOTH users can write: the inner shell's effect files. Root reaches the pane's work dir; another user needs every
#                   parent directory to let others through (others_can_traverse), else a 0777 directory under /var/tmp, removed afterwards.
#   docker_ready()  docker usable by this user AND the image present, each failure with its own reason and machine change.
# Everything takes its probes as arguments, so selftest_r15_w5c.py judges every branch without sudo, docker or a second user.
import os, shutil, stat, subprocess, tempfile

SUDO_USER_ENV = "APPCI_SUDO_USER"
SU_USER_ENV = "APPCI_SU_USER"
TEST_USER = "appci-priv"          # the name the NEEDS text proposes; any existing user works through APPCI_SUDO_USER
DOCKER_IMAGE = "alpine"


def _run(argv, timeout=20):
    try:
        p = subprocess.run(argv, capture_output=True, text=True, timeout=timeout)
        return p.returncode, (p.stderr or p.stdout or "").strip().splitlines()[:1]
    except (OSError, subprocess.TimeoutExpired) as e:
        return 127, [repr(e)]


def _get(key):
    import ci_env
    return ci_env.get(key)


def _pw(name):
    import pwd
    e = pwd.getpwnam(name)
    return e.pw_uid, e.pw_dir, e.pw_shell


def sudo_target(get=None, which=shutil.which, run=_run, pw=None, euid=None):
    """-> dict(ok, user, uid, home, root, flags, explicit, why). flags is what goes between `sudo` and the command ("-n -u appci-priv", or "-n" for
    root); explicit says the target came from APPCI_SUDO_USER. Not ok: `why` names the probe that failed and with what."""
    get, pw = get or _get, pw or _pw
    euid = os.geteuid() if euid is None else euid
    bad = dict(ok=False, user=None, uid=None, home=None, root=False, flags="", explicit=False)
    if not which("sudo"):
        return dict(bad, why="sudo is not installed")
    user = (get(SUDO_USER_ENV) or "").strip()
    if user:
        try:
            uid, home, _shell = pw(user)
        except KeyError:
            return dict(bad, explicit=True, why="%s names %r and this machine has no such user" % (SUDO_USER_ENV, user))
        if uid == euid:
            return dict(bad, explicit=True, why="%s names %r, which is the CI user itself (uid %d): no boundary to cross" % (SUDO_USER_ENV, user, uid))
        rc, err = run(["sudo", "-n", "-u", user, "true"])
        if rc != 0:
            return dict(bad, explicit=True, why="`sudo -n -u %s true` failed (rc %d%s)" % (user, rc, ": " + err[0] if err else ""))
        return dict(ok=True, user=user, uid=uid, home=home, root=uid == 0, flags="-n -u %s" % user, explicit=True, why="")
    rc, err = run(["sudo", "-n", "true"])
    if rc != 0:
        return dict(bad, why="`sudo -n true` failed (rc %d%s) and %s is not set" % (rc, ": " + err[0] if err else "", SUDO_USER_ENV))
    if euid == 0:
        return dict(bad, why="the runner is root already (euid 0): `sudo` to root crosses no boundary; set %s to an unprivileged user" % SUDO_USER_ENV)
    try:
        home = pw("root")[1]
    except KeyError:
        home = "/root"
    return dict(ok=True, user="root", uid=0, home=home, root=True, flags="-n", explicit=False, why="")


def sudo_needs(ci_user=None, platform=None):
    """the exact machine change, for the skip text. Two ways, the smaller first."""
    import getpass, sys
    ci_user = ci_user or getpass.getuser()
    mac = (platform or sys.platform) == "darwin"
    add = ("`sudo sysadminctl -addUser %s -shell /bin/bash`" % TEST_USER) if mac else ("`sudo useradd -m -s /bin/bash %s`" % TEST_USER)
    return ("NEEDS: a sudoers entry on this test machine (owner: machine change; the harness never edits sudoers or adds a user). EITHER an unprivileged test user: "
            "%s, a file /etc/sudoers.d/%s (mode 0440, checked with `visudo -cf`) holding the two lines `%s ALL=(%s) NOPASSWD: ALL` and `Defaults:%s use_pty`, "
            "and the line `%s=%s` in ~/.config/phosphor-ci/ci.env of the CI user (the CI user can then become only that user, never root). "
            "OR, where the CI user may be root: `%s ALL=(ALL) NOPASSWD: ALL`" % (add, TEST_USER, ci_user, TEST_USER, ci_user, SUDO_USER_ENV, TEST_USER, ci_user))


def others_can_traverse(path, st=os.stat):
    """pure given st: can a user who is neither the owner nor in the group reach `path`: every directory from / down to it has the others-execute
    bit. ACLs are not read (a machine that grants the test user by ACL uses the /var/tmp directory, which also works)."""
    p = os.path.abspath(path)
    while True:
        try:
            if not st(p).st_mode & stat.S_IXOTH:
                return False
        except OSError:
            return False
        parent = os.path.dirname(p)
        if parent == p:
            return True
        p = parent


def shared_dir(work, uid, label, st=os.stat, mkdtemp=tempfile.mkdtemp, tmp_root="/var/tmp"):
    """-> (directory both the CI user and uid `uid` can write, cleanup()). Root: the work dir itself (cleanup does nothing). Another user: a 0777
    directory inside the work dir when others can reach it, else one under /var/tmp (never /tmp: tmpfs on some machines; the directory is ours, so
    we may remove the other user's files in it)."""
    if uid == 0:
        return work, (lambda: None)
    if others_can_traverse(work, st):
        d = os.path.join(work, "pb_%s" % label)
        shutil.rmtree(d, ignore_errors=True)
        os.makedirs(d)
    else:
        d = mkdtemp(prefix="appci_pb_%s_" % label, dir=tmp_root)
    os.chmod(d, 0o777)
    return d, (lambda: shutil.rmtree(d, ignore_errors=True))


def read_lines(path):
    try:
        return open(path, errors="replace").read().splitlines()
    except OSError:
        return []


def docker_ready(which=shutil.which, run=_run, image=DOCKER_IMAGE):
    """-> (ok, why). Three separate failures, each with its machine change: no docker client; a daemon this user cannot reach; the image absent.
    The harness never pulls an image and never starts a daemon."""
    if not which("docker"):
        return False, "docker is not installed. NEEDS: the docker engine on this test machine (Debian/Ubuntu package docker.io; macOS: Docker Desktop or colima)"
    rc, err = run(["docker", "info", "--format", "{{.ServerVersion}}"])
    if rc != 0:
        return False, ("the docker daemon is not usable by the CI user (`docker info` failed, rc %d%s). NEEDS: the daemon running and the CI user in the "
                       "`docker` group (`sudo usermod -aG docker <ci-user>`, then restart the runner service)" % (rc, ": " + err[0] if err else ""))
    rc, err = run(["docker", "image", "inspect", image])
    if rc != 0:
        return False, "docker works but the `%s` image is not on this machine (`docker image inspect %s` failed). NEEDS: `docker pull %s` once by hand" % (image, image, image)
    return True, ""
