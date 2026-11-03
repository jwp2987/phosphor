"""appci/deadline.py: the run's own time budget, so a long run ENDS CLEANLY (report written, artifacts uploaded) instead of being killed by the
GitLab job timeout (r18e0 macOS: killed inside the agent pass, artifacts 404, ~47 cases never ran; r18d8 Wayland: same).

Budget = APPCI_DEADLINE_S (seconds from job start), else CI_JOB_TIMEOUT (GitLab exports the job's timeout in seconds). Job start =
CI_JOB_STARTED_AT (ISO 8601 UTC) when plausible, else this process's start. No budget known (a local run): never stops anything.
Before each case: start it only when time_left >= max(MIN_MARGIN, that case's own timeout) + RESERVE (report, teardown, upload)."""
import calendar, os, time

MIN_MARGIN_S = 600           # at least 10 min must remain for any case
RESERVE_S = 300              # report + teardown + artifact upload
_PROC_START = time.time()


def budget_seconds(env=None):
    env = os.environ if env is None else env
    for k in ("APPCI_DEADLINE_S", "CI_JOB_TIMEOUT"):
        try:
            v = float(env.get(k, "") or 0)
        except ValueError:
            v = 0
        if v > 0:
            return v, k
    return None, None


def job_start(budget, now=None, env=None):
    """epoch of the job start: CI_JOB_STARTED_AT if it parses and is not in the future / older than the whole budget, else process start"""
    env = os.environ if env is None else env
    now = time.time() if now is None else now
    s = env.get("CI_JOB_STARTED_AT", "")
    try:
        t = calendar.timegm(time.strptime(s.replace("+00:00", "Z")[:19] + "Z", "%Y-%m-%dT%H:%M:%SZ"))
        if t <= now + 5 and (budget is None or now - t < budget):
            return float(t), "CI_JOB_STARTED_AT"
    except ValueError:
        pass
    return _PROC_START, "process start"


def margin_s(next_case_timeout_s):
    return max(MIN_MARGIN_S, float(next_case_timeout_s or 0)) + RESERVE_S


class Clock:
    def __init__(self, env=None, now=None):
        self.budget, self.budget_src = budget_seconds(env)
        self.start, self.start_src = job_start(self.budget, now, env)
        self.logged_stop = False

    def left(self, now=None):
        if self.budget is None:
            return None
        return self.start + self.budget - (time.time() if now is None else now)

    def describe(self):
        if self.budget is None:
            return "no job time budget (APPCI_DEADLINE_S / CI_JOB_TIMEOUT unset): the run never stops itself"
        return "job time budget %d min from %s (%s); %d min left now" % (self.budget // 60, self.start_src, self.budget_src, self.left() // 60)

    def allows(self, next_case_timeout_s, now=None):
        """(ok, why). why is the row detail text when not ok."""
        left = self.left(now)
        if left is None or left >= margin_s(next_case_timeout_s):
            return True, ""
        return False, "not run: job time budget (%d min left, %d min needed for this case plus report/teardown/upload)" % (max(left, 0) // 60, margin_s(next_case_timeout_s) // 60)
