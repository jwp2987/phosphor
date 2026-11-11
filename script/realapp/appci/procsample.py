# appci/procsample.py: memory and CPU of the app's processes on every platform (r14-test-gaps T49, round 15 wave 5c). The soak (cases_robust.c_soak)
# sampled /proc, so it ran on Linux only: macOS read ONE process through `ps` (the main one, without its terminal-server helper) and Windows SKIPPED
# by winmap ("samples /proc").
#   sample(ctx) -> {"pids": [...], "rss_kib": int, "cpu_s": float, "how": str, "private_kib": int | None}   or None when no process could be read
#     Linux    /proc/<pid>/stat + statm of every process running the app's executable (unchanged figures: resident pages, utime + stime);
#     macOS    `ps -axo pid=,rss=,time=,command=` filtered to this job's private bundle (MacApp._instances' rule): rss in KiB, cputime;
#     Windows  the app's Win32 job (WindowsApp.job: the app starts inside it) gives the exact pids; those whose image is the app's exe are read with
#              GetProcessTimes / GetProcessMemoryInfo, the calls Get-Process wraps. Without a job: one `Get-Process` by image path (GET_PROCESS_PS).
#              rss_kib is the PRIVATE bytes there (PrivateUsage / PrivateMemorySize64): the working set is trimmed by the OS and shared with other
#              processes (#1340), so a leak shows in private bytes; the working set is reported as "ws_kib".
# cpu_s is CPU seconds summed over the processes, so a caller needs no clock-tick constant (os.sysconf does not exist on Windows).
# The parsers are pure (selftest_r15_w5c.py); sample() does the reading.
import csv, io, os, subprocess

GET_PROCESS_PS = ("Get-Process | Where-Object { $_.Path -eq '%s' } | Select-Object Id,@{n='Cpu';e={$_.TotalProcessorTime.TotalSeconds}},WorkingSet64,PrivateMemorySize64 "
                  "| ConvertTo-Csv -NoTypeInformation")


def ps_time_seconds(s):
    """pure: a ps cputime as seconds. Shapes: macOS `M:SS.cc` (minutes may exceed 59: `123:45.67`), procps `HH:MM:SS` and `D-HH:MM:SS`. None when it
    is not a time."""
    s = (s or "").strip()
    days = 0
    if "-" in s:
        d, s = s.split("-", 1)
        if not d.isdigit():
            return None
        days = int(d)
    parts = s.split(":")
    if not 2 <= len(parts) <= 3:
        return None
    try:
        nums = [float(x) for x in parts]
    except ValueError:
        return None
    secs = 0.0
    for x in nums:
        secs = secs * 60 + x
    return days * 86400 + secs


def parse_ps(text):
    """pure: rows of `ps -axo pid=,rss=,time=,command=` -> [(pid, rss_kib, cpu_s, command)]; a line that does not parse is dropped"""
    out = []
    for l in (text or "").splitlines():
        f = l.strip().split(None, 3)
        if len(f) < 4 or not f[0].isdigit() or not f[1].isdigit():
            continue
        t = ps_time_seconds(f[2])
        if t is None:
            continue
        out.append((int(f[0]), int(f[1]), t, f[3]))
    return out


def total(rows):
    """pure: [(pid, rss_kib, cpu_s, ...)] -> (sorted pids, rss_kib, cpu_s)"""
    return sorted(r[0] for r in rows), sum(r[1] for r in rows), sum(r[2] for r in rows)


def mac_rows(text, bundle):
    """pure: the ps rows whose executable lives inside `bundle` (the job's private .app): the main process and its helpers, never another install"""
    pre = bundle.rstrip("/") + "/Contents/MacOS/"
    return [r for r in parse_ps(text) if r[3].startswith(pre)]


def parse_get_process(text):
    """pure: the CSV of GET_PROCESS_PS -> [(pid, private_kib, cpu_s, ws_kib)]. A process that exited between the listing and the read has empty
    fields and is dropped; a decimal comma (a German Windows) is read as a point."""
    out = []
    rows = list(csv.reader(io.StringIO((text or "").replace("\r\n", "\n"))))
    if not rows:
        return out
    head = [h.strip().strip('"') for h in rows[0]]
    for row in rows[1:]:
        d = dict(zip(head, row))
        try:
            out.append((int(d["Id"]), int(d["PrivateMemorySize64"]) // 1024, float((d["Cpu"] or "").replace(",", ".")), int(d["WorkingSet64"]) // 1024))
        except (KeyError, ValueError):
            continue
    return out


def _linux(ctx):
    pid = ctx.app.pid()
    try:
        exe = os.readlink("/proc/%d/exe" % pid)
    except (OSError, TypeError):
        return None
    tck, page = os.sysconf("SC_CLK_TCK"), os.sysconf("SC_PAGE_SIZE")
    rows = []
    for p in os.listdir("/proc"):
        if not p.isdigit():
            continue
        try:
            if os.readlink("/proc/%s/exe" % p) != exe:
                continue
            f = open("/proc/%s/stat" % p).read().rsplit(")", 1)[1].split()
            rows.append((int(p), int(open("/proc/%s/statm" % p).read().split()[1]) * page // 1024, (int(f[11]) + int(f[12])) / tck))
        except (OSError, IndexError, ValueError):
            pass
    if not rows:
        return None
    pids, rss, cpu = total(rows)
    return dict(pids=pids, rss_kib=rss, cpu_s=cpu, how="/proc", private_kib=None)


def _mac(ctx):
    out = subprocess.run(["ps", "-axo", "pid=,rss=,time=,command="], capture_output=True, text=True, timeout=30).stdout
    rows = mac_rows(out, ctx.app.bundle)
    if not rows:
        return None
    pids, rss, cpu = total(rows)
    return dict(pids=pids, rss_kib=rss, cpu_s=cpu, how="ps", private_kib=None)


def _windows(ctx):
    import winapi
    app = ctx.app
    exe = os.path.basename(app.bin).lower()
    rows, how = [], "job+GetProcessTimes"
    job = getattr(app, "job", None)
    if job:
        for pid in winapi.job_pids(job):
            if winapi.proc_name(pid) != exe:
                continue                       # the job also holds the app's shells and conhosts: not the app's own memory
            try:
                cpu, ws, private = winapi.process_times_private(pid)
            except OSError:
                continue                       # exited between the listing and the read
            rows.append((pid, private, cpu / 1e7, ws))
    if not rows:
        how = "Get-Process"
        p = subprocess.run(["powershell", "-NoProfile", "-NonInteractive", "-Command", GET_PROCESS_PS % app.bin.replace("'", "''")],
                           capture_output=True, text=True, timeout=60)
        rows = parse_get_process(p.stdout)
    if not rows:
        return None
    pids, private, cpu = total(rows)
    return dict(pids=pids, rss_kib=private, cpu_s=cpu, how=how, private_kib=private, ws_kib=sum(r[3] for r in rows))


def sample(ctx):
    """one reading of the app's processes (see the header); None when nothing could be read (the app is gone, or the platform tool failed)"""
    try:
        if os.name == "nt" or getattr(ctx, "win", False):
            return _windows(ctx)
        if ctx.be.name == "mac":
            return _mac(ctx)
        return _linux(ctx)
    except Exception as e:                      # a sampler must never end a 60-minute case; the caller counts the misses
        print("[appci] procsample: %r" % (e,), flush=True)
        return None


def cpu_pct(a, b, wall_s):
    """pure: CPU percent of one core between two readings `wall_s` apart; None when either reading is missing or the clock did not move"""
    if a is None or b is None or wall_s <= 0:
        return None
    return (b["cpu_s"] - a["cpu_s"]) / wall_s * 100.0
