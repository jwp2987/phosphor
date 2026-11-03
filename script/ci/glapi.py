#!/usr/bin/env python3
# script/ci/glapi.py: the one way CI scripts call the GitLab API (`glab api -i`), honouring GitLab.com's rate limits.
#
# GitLab.com applies a per-user limit to this account (observed 2026-11-03: `Ratelimit-Name: burst_traffic_per_user_free_plan`,
# `Ratelimit-Limit: 100` per minute). Every response carries Ratelimit-Limit/-Remaining/-Reset (epoch s); a throttled one is
# HTTP 429 with Retry-After (s). See https://docs.gitlab.com/user/gitlab_com/rate_limits/#when-you-exceed-a-limit
#   * 429: sleep Retry-After (else until Ratelimit-Reset, else exponential), plus jitter; exponential on repeats. A 429 is never
#     "no such job" or "no results": the caller only gets an answer GitLab actually gave.
#   * 5xx / dropped connection: exponential backoff (5 s doubling, capped at 5 min).
#   * Pacing: when Ratelimit-Remaining falls under 10% of the limit, wait for Ratelimit-Reset BEFORE the next call instead of
#     spending the rest of the window and being throttled.
# Gives up only after `deadline_s` (default 30 min) of failed attempts, raising GitLabUnavailable.
import random, re, subprocess, sys, time

_PACE_UNTIL = 0.0          # module-wide: no call before this time (set when the window is nearly spent)
TRANSIENT = ("502", "503", "504", "timeout", "Timeout", "error connecting", "connection", "Connection", "dial tcp", "EOF",
             "no such host")


class GitLabUnavailable(RuntimeError):
    pass


def _log(msg):
    sys.stderr.write("glapi: %s\n" % msg); sys.stderr.flush()


def _split(raw):
    """`glab api -i` output -> (status, {header-lower: value}, body bytes). Redirects can print several header blocks; the last wins."""
    status, headers, body = None, {}, raw
    while body.startswith(b"HTTP/"):
        m = re.search(rb"\r?\n\r?\n", body)
        if not m:
            break
        block, body = body[:m.start()].decode("latin-1"), body[m.end():]
        lines = block.splitlines()
        parts = lines[0].split()
        status = int(parts[1]) if len(parts) > 1 and parts[1].isdigit() else None
        headers = {}
        for l in lines[1:]:
            k, _, v = l.partition(":")
            headers[k.strip().lower()] = v.strip()
    return status, headers, body


def _num(h, k):
    try:
        return float(h[k])
    except (KeyError, ValueError):
        return None


def call(path, method="GET", body=None, headers=(), deadline_s=1800):
    """-> (status, body bytes). status is GitLab's HTTP status (200, 404, ...); raises GitLabUnavailable only after deadline_s of
    throttling / outage. 4xx other than 429 are returned, not retried."""
    global _PACE_UNTIL
    a = ["glab", "api", "-i", path] + (["-X", method] if method != "GET" else [])
    for h in headers:
        a += ["--header", h]
    if body is not None:
        a += ["--input", "-"]
    start, attempt = time.time(), 0
    while True:
        wait = _PACE_UNTIL - time.time()
        if wait > 0:
            time.sleep(wait)
        r = subprocess.run(a, input=body.encode() if isinstance(body, str) else body, capture_output=True)
        status, h, out = _split(r.stdout or b"")
        lim, rem, reset = _num(h, "ratelimit-limit"), _num(h, "ratelimit-remaining"), _num(h, "ratelimit-reset")
        if lim and rem is not None and reset and rem < max(1, lim * 0.10):
            _PACE_UNTIL = max(_PACE_UNTIL, reset + 1)
            _log("%s: %d of %d requests left in this window; pausing until reset (%ds)" % (h.get("ratelimit-name", "rate limit"), rem, lim,
                                                                                         max(0, reset + 1 - time.time())))
        err = (r.stderr or b"").decode(errors="replace")
        if status == 429:
            ra, now = _num(h, "retry-after"), time.time()
            # never sooner than GitLab asked; when throttled again, back off exponentially on top (capped at 10 min)
            told = ra if ra is not None else (reset - now if reset and reset > now else 0)
            delay = max(1.0, told, min(600, 5 * 2 ** attempt) if attempt else 0) + random.uniform(0, 3)
            _log("429 on %s (%s); retrying in %.0fs" % (path, h.get("ratelimit-name", "?"), delay))
        elif status is not None and status < 500:
            return status, out
        elif status is None and not any(x in err for x in TRANSIENT) and r.returncode:
            raise GitLabUnavailable("glab api %s failed without an HTTP status: %s" % (path, err.strip()[:300]))
        else:
            delay = min(300, 5 * 2 ** attempt) + random.uniform(0, 3)
            _log("%s on %s; retrying in %.0fs" % (status or err.strip()[:80], path, delay))
        if time.time() - start + delay > deadline_s:
            raise GitLabUnavailable("glab api %s: no answer after %ds (last: %s)" % (path, deadline_s, status or err.strip()[:200]))
        time.sleep(delay)
        attempt += 1


if __name__ == "__main__" and sys.argv[1:] == ["--self-test"]:
    s, h, b = _split(b"HTTP/2.0 429 Too Many Requests\r\nRetry-After: 42\r\nRatelimit-Limit: 100\r\nRatelimit-Remaining: 0\r\n\r\n{\"m\":1}")
    assert (s, h["retry-after"], h["ratelimit-remaining"], b) == (429, "42", "0", b'{"m":1}'), (s, h, b)
    s, h, b = _split(b"HTTP/1.1 302 Found\r\nLocation: x\r\n\r\nHTTP/2.0 200 OK\r\nContent-Type: a\r\n\r\n\x00\x01\r\n\r\nbin")
    assert (s, b) == (200, b"\x00\x01\r\n\r\nbin"), (s, b)
    s, h, b = _split(b"plain body")
    assert (s, h, b) == (None, {}, b"plain body")
    print("glapi self-test: ok")
