#!/usr/bin/env python3
# realapp/linux/torture: seeded expert-typist fuzzer and XTest key sequencer (kseq.c, rec.c) with exact-received-command verification, run against Phosphor and a reference terminal.
# Platform: Linux X11. Prereqs: cc -O2 kseq.c rec.c -lX11 -lXtst; PHOSPHOR_DEB_DIR extracted deb; config.example.sh sourced.
# Run: run.sh / all.sh (scenarios in scen.py); lib.sh builds the scratch profile (owner-config copy, agent mode, LAN provider).
# Restores: scratch profile only; the owner's config is only read (copied) and never printed.
"""Seeded expert-typist fuzzer.  fuzz.py --seed N --count C|--duration S --mode phos|xterm [--log LOG] [--out DIR]
Every command line has an intended final text; the editor model (with ghost-text accept) predicts what the shell must
receive.  Received text is read from the PROMPT_COMMAND `history 1` records in $REC_LOG.  Replays exactly with same seed.
Env: KSEQ, REC_LOG, DISPLAY, PID (main app pid for CPU sampling), PAINT (x,y,w,h region for keystroke->paint)."""
import argparse, os, random, subprocess, sys, time, json, re, statistics

ap = argparse.ArgumentParser()
ap.add_argument('--seed', type=int, default=1)
ap.add_argument('--count', type=int, default=0)
ap.add_argument('--duration', type=float, default=0)
ap.add_argument('--out', default='.')
ap.add_argument('--label', default='run')
ap.add_argument('--hist', default=os.path.expanduser('~/.bash_history'), help='seed history file for the ghost model')
ap.add_argument('--speed', type=float, default=1.0, help='scale key delays (1.0 = 12-30ms bursts)')
ap.add_argument('--roll', type=float, default=0.5, help='probability a command is typed in rollover mode')
ap.add_argument('--noghost', action='store_true', help='model has no ghost text (xterm/bash reference)')
ap.add_argument('--wordmode', default='alnum', choices=['alnum','ws'])
ap.add_argument('--wait', type=float, default=3.0)
a = ap.parse_args()
KSEQ = os.environ.get('KSEQ', os.path.join(os.path.dirname(os.path.abspath(__file__)), 'kseq'))
LOG = os.environ['REC_LOG']
PAINT = os.environ.get('PAINT', '')
APPPID = os.environ.get('PID', '')
rng = random.Random(a.seed)

ROWS = ['qwertyuiop', 'asdfghjkl', 'zxcvbnm']
ADJ = {}
for r, row in enumerate(ROWS):
    for i, c in enumerate(row):
        n = []
        if i > 0: n.append(row[i-1])
        if i < len(row)-1: n.append(row[i+1])
        ADJ[c] = n
WORDS = ['alpha', 'alphabet', 'alpine', 'beta', 'git', 'status', 'stash', 'echo', 'hello', 'world', 'big/f0', 'big/f04999.dat',
         't/alpha', 't/small.txt', '-la', '--color=auto', 'src/main.rs', '/usr/local/bin', 'foo_bar', 'x1', 'deadbeef', 'a-b', 'q_r',
         '$HOME', '$PATH', 'one', 'two', 'three', 'checkout', 'main', 'long' + 'x'*40]
PIPES = [' | cat', ' | wc -l', ' | sort | uniq -c | head -3', ' | tr a-z A-Z']

def load_hist():
    try:
        return [l.rstrip('\n') for l in open(a.hist, errors='replace') if l.strip()]
    except OSError:
        return []
hist = load_hist()

def ghost(buf):
    if a.noghost or not buf: return None
    for h in reversed(hist):
        if len(h) > len(buf) and h.startswith(buf): return h
    return None

def intended():
    r = rng.random()
    if r < 0.45 and hist:      # a history-derived line, possibly with an altered tail
        base = rng.choice(hist[-60:] if rng.random() < .5 else hist)
        if not base.isascii(): base = 'rec ' + rng.choice(WORDS)
        if not base.startswith('rec '): base = 'rec ' + base.split(' ', 1)[-1]
        if '|' in base or '"' in base or "'" in base or '{' in base or ';' in base or '&' in base or '>' in base or '<' in base or '`' in base or '(' in base: base = 'rec ' + rng.choice(WORDS)
        if rng.random() < .4: base += ' ' + rng.choice(WORDS)
        return base[:150]
    n = rng.randint(1, 6)
    line = 'rec ' + ' '.join(rng.choice(WORDS) for _ in range(n))
    if rng.random() < .15: line += rng.choice(PIPES)
    return line[:150]

# ---- editor model ----
def word_back(buf):
    i = len(buf)
    while i > 0 and buf[i-1] == ' ': i -= 1
    while i > 0 and ((buf[i-1].isalnum() or buf[i-1] == '_') if a.wordmode == 'alnum' else buf[i-1] != ' '): i -= 1
    return i
def step(states, op, arg=None, amb=False):
    out = set()
    for (b, c) in states:
        if op == 'ch': out.add((b[:c]+arg+b[c:], c+1))
        elif op == 'bs': out.add((b[:c-1]+b[c:], c-1) if c > 0 else (b, c))
        elif op == 'del': out.add((b[:c]+b[c+1:], c))
        elif op == 'left': out.add((b, max(0, c-1)))
        elif op in ('right', 'end'):
            if op == 'end' or c >= len(b):
                g = ghost(b) if c == len(b) else None
                tgt = (b, len(b))
                if g and amb: out.add(tgt); out.add((g, len(g)))
                elif g: out.add((g, len(g)))
                else: out.add(tgt)
            else: out.add((b, c+1))
        elif op == 'home': out.add((b, 0))
        elif op == 'cw':
            i = word_back(b[:c]); out.add((b[:i]+b[c:], i))
        elif op == 'cu': out.add(('', 0))
    return out

class Typist:
    def __init__(s):
        s.toks = []; s.trace = []; s.states = {('', 0)}; s.t = 0.0; s.lastedit = -1e9
        s.roll = rng.random() < a.roll
    def delay(s):
        r = rng.random()
        if r < .04: d = rng.uniform(80, 200)
        elif r < .08: d = rng.uniform(1, 5)       # near-simultaneous
        else: d = rng.uniform(12, 30)
        return max(1, d * a.speed) if d > 5 else d
    def emit(s, tok, op, arg=None, pre=None):
        pre = s.delay() if pre is None else pre
        s.t += pre
        amb = (op in ('right', 'end')) and (s.t - s.lastedit) < 70
        s.states = step(s.states, op, arg, amb=amb)
        if op in ('ch', 'bs', 'del', 'cw', 'cu'): s.lastedit = s.t
        s.events.append((round(pre), tok))
    def begin(s): s.events = []
    def believed(s):
        # planning state: the "accepted" branch (largest buffer from ghost accept) if ambiguous
        return max(s.states, key=lambda x: (len(x[0]) if True else 0, x[1])) if len(s.states) > 1 else next(iter(s.states))

def kchar(c):
    return "'" + c

def plan(F):
    """Return (events, states) for typing intended F. events = [(pre_delay_ms, kseq_token)]"""
    T = Typist(); T.begin()
    i = 0  # index into F of next char to type given believed buffer == F[:i] when clean
    guard = 0
    def buf(): return T.believed()[0]
    def curs(): return T.believed()[1]
    while guard < 400:
        guard += 1
        b, c = T.believed()
        if b == F and c == len(b): break
        # buffer correct but cursor not at end -> End key
        if b == F: T.emit('<End>', 'end'); continue
        if c != len(b):
            # editing mid-line: finish by moving to end then re-evaluate
            T.emit('<End>', 'end'); continue
        # common prefix of buffer and F
        k = 0
        while k < len(b) and k < len(F) and b[k] == F[k]: k += 1
        if k < len(b):                    # buffer has wrong tail -> delete it
            wrong = len(b) - k
            r = rng.random()
            if wrong >= 4 and r < .2: T.emit('<C-u>', 'cu'); continue
            if b[-1].isalnum() and word_back(b) >= k and r < .45: T.emit('<C-w>', 'cw'); continue
            T.emit('<BackSpace>', 'bs'); continue
        # buffer is a strict prefix of F: choose next action
        rest = F[len(b):]
        g = ghost(b)
        if g and len(g) > len(b)+1 and rng.random() < .55:
            common = 0
            while common < len(g) and common < len(F) and g[common] == F[common]: common += 1
            if common >= len(b)+2 and (common == len(g) or rng.random() < .5):
                fast = rng.random() < .6
                useend = rng.random() < .2
                T.emit('<End>' if useend else '<Right>', 'end' if useend else 'right', pre=(rng.uniform(8, 25) if fast else rng.uniform(40, 150)))
                continue
        ch = rest[0]
        r = rng.random()
        if r < .035 and ch.lower() in ADJ:                 # adjacent-key substitution, fixed at once or late
            w = rng.choice(ADJ[ch.lower()]); T.emit(kchar(w), 'ch', w)
            continue
        if r < .06 and len(rest) >= 2:                      # transposition
            T.emit(kchar(rest[1]), 'ch', rest[1]); T.emit(kchar(rest[0]), 'ch', rest[0]); continue
        if r < .085:                                        # doubled
            T.emit(kchar(ch), 'ch', ch); T.emit(kchar(ch), 'ch', ch); continue
        if r < .10 and len(b) >= 4:                         # late catch by arrow: wrong char mid-line, left-fix
            w = rng.choice(ADJ.get(ch.lower(), ['x'])) if ch.lower() in ADJ else 'x'
            T.emit(kchar(w), 'ch', w)
            n = min(rng.randint(1, 4), len(rest) - 1)
            for j in range(n): T.emit(kchar(rest[1+j]), 'ch', rest[1+j])
            for j in range(n+1): T.emit('<Left>', 'left')
            T.emit('<Delete>', 'del'); T.emit(kchar(ch), 'ch', ch)
            for j in range(n): T.emit('<Right>', 'right')   # (mid-line -> plain cursor move, no ghost)
            continue
        T.emit(kchar(ch), 'ch', ch)
    return T

def pat(ch):
    return ch

def run_cmd(idx, F, mode):
    T = plan(F)
    final = T.states
    n_before = count_h()
    # last-key -> Enter gap
    gap = rng.choice([4, 8, 12, 15, 20, 25, 40, 120, 300])
    toks = []
    if T.roll: toks.append('roll')
    for pre, tok in T.events:
        toks += ['d%d' % pre, tok] if False else []
    # kseq sleeps AFTER each key by dl: emit "dX key" where X is the delay *following* the key. Shift delays by one.
    ev = T.events
    seq = []
    for j, (pre, tok) in enumerate(ev):
        nxt = ev[j+1][0] if j+1 < len(ev) else gap
        seq += ['d%d' % nxt, tok]
    seq += ['d30', '<Return>']
    if T.roll: seq = ['roll'] + seq + ['roll']
    kargs = [KSEQ] + (['-P', PAINT] if PAINT else []) + seq
    cpu0 = cpu()
    p = subprocess.run(kargs, capture_output=True, text=True)
    keys = [(l.split()[1] if len(l.split()) > 2 else ' ', int(l.split()[-1])) for l in p.stdout.splitlines() if l.startswith('K ')]
    paints = [int(l.split()[1]) for l in p.stdout.splitlines() if l.startswith('P ')]
    t_enter = keys[-1][1] if keys else 0
    # wait for the shell to report
    got = None; t0 = time.time()
    while time.time() - t0 < a.wait:
        if count_h() > n_before: break
        time.sleep(0.01)
    time.sleep(0.05)
    recs = parse_log()
    received = None; rstart = None
    if len(recs['H']) > n_before:
        received = recs['H'][n_before][1]
    # command start time: first R after t_enter
    for ts, args in recs['R']:
        if ts >= t_enter - 5_000_000: rstart = ts; break
    # key->paint
    kp = []
    for k, ts in keys[:-1]:
        nxt = next((pt for pt in paints if pt > ts), None)
        if nxt and nxt - ts < 400_000_000: kp.append((nxt - ts) / 1e6)
    ok = received is not None and any(b == received for (b, c) in final)
    return dict(idx=idx, intended=F, received=received, ok=ok, expected=sorted(b for b, c in final)[:4], nexp=len(final),
                enter_to_start_ms=((rstart - t_enter) / 1e6 if (rstart and received is not None) else None),
                key_paint_ms=kp, keys=[k for k, _ in keys], events=[(p_, t_) for p_, t_ in ev], gap=gap, roll=T.roll,
                cpu_ticks=cpu() - cpu0, lost=received is None)

def count_h():
    try: return sum(1 for l in open(LOG, errors='replace') if l.startswith('H '))
    except OSError: return 0
def parse_log():
    H = []; R = []
    lines = open(LOG, errors='replace').read().split('\n')
    i = 0
    while i < len(lines):
        l = lines[i]
        if l.startswith('H '):
            txt = lines[i+1] if i+1 < len(lines) else ''
            m = re.match(r'^\s*\d+\s{2}(.*)$', txt)
            H.append((l.split()[1], m.group(1) if m else txt.strip())); i += 2; continue
        if l.startswith('R '):
            parts = l.split('\t'); R.append((int(parts[0].split()[1]), parts[1:]))
        i += 1
    return dict(H=H, R=R)
def cpu():
    if not APPPID: return 0
    tot = 0
    for pid in [APPPID] + [x for x in os.popen('pgrep -P %s' % APPPID).read().split()]:
        try:
            f = open('/proc/%s/stat' % pid).read().rsplit(')', 1)[1].split(); tot += int(f[11]) + int(f[12])
        except Exception: pass
    return tot

def clear_line():
    subprocess.run([KSEQ, 'd20', '<C-c>', '<C-u>', 'w120'], capture_output=True)

def main():
    os.makedirs(a.out, exist_ok=True)
    resf = open(os.path.join(a.out, a.label + '.jsonl'), 'w')
    t0 = time.time(); n = 0; fails = []; e2s = []; kp = []; lost = 0
    while True:
        if a.count and n >= a.count: break
        if a.duration and time.time() - t0 >= a.duration: break
        if not a.count and not a.duration and n >= 50: break
        global rng
        rng = random.Random(a.seed * 100003 + n)
        F = intended()
        r = run_cmd(n, F, None)
        resf.write(json.dumps(r) + '\n'); resf.flush()
        if r['lost'] or not r['ok']:
            fails.append(r)
            subprocess.run(['ffmpeg','-loglevel','error','-y','-f','x11grab','-video_size','1280x800','-i',os.environ.get('DISPLAY',':97'),'-frames:v','1',os.path.join(a.out,'fail-%s-%d.png'%(a.label,n))])
            print('FAIL seed=%d idx=%d intended=%r received=%r expected=%r lost=%s' % (a.seed, n, F, r['received'], r['expected'], r['lost']), flush=True)
            clear_line()
        else:
            if r['enter_to_start_ms'] is not None: e2s.append(r['enter_to_start_ms'])
        if r['received'] is not None: hist.append(r['received'])
        kp += r['key_paint_ms']
        n += 1
        time.sleep(rng.choice([0.12, 0.2, 0.3, 0.5]))
    dur = time.time() - t0
    def q(v, p): v = sorted(v); return v[min(len(v)-1, int(len(v)*p))] if v else float('nan')
    print('SUMMARY label=%s seed=%d cmds=%d fails=%d (lost=%d) dur=%.0fs' % (a.label, a.seed, n, len(fails), sum(1 for f in fails if f['lost']), dur))
    for nm, v in (('enter_to_start_ms', e2s), ('key_to_paint_ms', kp)):
        if v: print('  %s n=%d median=%.1f p95=%.1f p99=%.1f max=%.1f' % (nm, len(v), statistics.median(v), q(v, .95), q(v, .99), max(v)))
main()
