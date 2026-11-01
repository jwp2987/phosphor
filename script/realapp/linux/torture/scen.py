#!/usr/bin/env python3
# realapp/linux/torture: seeded expert-typist fuzzer and XTest key sequencer (kseq.c, rec.c) with exact-received-command verification, run against Phosphor and a reference terminal.
# Platform: Linux X11. Prereqs: cc -O2 kseq.c rec.c -lX11 -lXtst; PHOSPHOR_DEB_DIR extracted deb; config.example.sh sourced.
# Run: run.sh / all.sh (scenarios in scen.py); lib.sh builds the scratch profile (owner-config copy, agent mode, LAN provider).
# Restores: scratch profile only; the owner's config is only read (copied) and never printed.
"""Targeted scenarios (differential: the same key sequence typed SLOWLY is the oracle for the same sequence typed FAST).
scen.py <group> [reps]    groups: ghost tab interleave hist enter
Env: KSEQ REC_LOG PAINT PID DISPLAY OUT.  Everything starts with `rec` so it is harmless and timestamped."""
import os, re, subprocess, sys, time, json, random, statistics, collections
KSEQ = os.environ['KSEQ']; LOG = os.environ['REC_LOG']; OUT = os.environ.get('OUT', '.'); os.makedirs(OUT, exist_ok=True)
HIST = [l.rstrip('\n') for l in open(os.path.join(os.environ['HOME'], '.bash_history'))] if os.path.exists(os.path.join(os.environ.get('HOME',''), '.bash_history')) else []
REPS = int(sys.argv[2]) if len(sys.argv) > 2 else 12
rng = random.Random(7)

def parse():
    H = []; R = []
    lines = open(LOG, errors='replace').read().split('\n'); i = 0
    while i < len(lines):
        l = lines[i]
        if l.startswith('H '):
            m = re.match(r'^\s*\d+\s{2}(.*)$', lines[i+1] if i+1 < len(lines) else ''); H.append((float(l.split()[1]), m.group(1) if m else None)); i += 2; continue
        if l.startswith('R '):
            p = l.split('\t'); R.append(int(p[0].split()[1]))
        i += 1
    return H, R
def nH(): return len(parse()[0])
def shot(name):
    p = os.path.join(OUT, name + '.png')
    subprocess.run(['ffmpeg', '-loglevel', 'error', '-y', '-f', 'x11grab', '-video_size', '1280x800', '-i', os.environ.get('DISPLAY', ':97'), '-frames:v', '1', p])
    return p
def ocr(p):
    return subprocess.run(['tesseract', p, '-', '--psm', '6'], capture_output=True, text=True).stdout

def build(steps):
    """steps: list of (token, delay_after_ms)."""
    a = []
    for tok, d in steps:
        a += ['d%d' % d, tok]
    return a
def typ(text, d): return [("'" + c, d) for c in text]
def key(k, d): return [(k, d)]
def send(steps, final_enter=True, roll=False, paint=False):
    args = [KSEQ] + (['-P', os.environ['PAINT']] if paint and os.environ.get('PAINT') else []) + (['roll'] if roll else []) + build(steps) + (['roll'] if roll else [])
    n0 = nH()
    p = subprocess.run(args, capture_output=True, text=True)
    keys = [(l.split()[1], int(l.split()[2])) for l in p.stdout.splitlines() if l.startswith('K ')]
    t0 = time.time()
    while time.time() - t0 < 4 and nH() <= n0: time.sleep(0.01)
    time.sleep(0.08)
    H, R = parse()
    got = H[n0][1] if len(H) > n0 else None
    ret = [ts for k, ts in keys if k == 'Return']
    tstart = None
    if ret:
        tstart = next((r for r in R if r >= ret[-1] - 5_000_000), None)
    return got, ((tstart - ret[-1]) / 1e6 if tstart and ret and got is not None else None), (keys, p.stdout)
def recover():
    subprocess.run([KSEQ, 'd20', '<C-c>', '<C-u>', 'w150'], capture_output=True)
def settle(): time.sleep(0.25)

results = collections.OrderedDict()
def record(name, kind, detail):
    results.setdefault(name, []).append((kind, detail))

def differential(name, make, reps, baseline_reps=2, speeds=None, note=''):
    """make(speed) -> steps; speed dict {kd: per-key ms, acc: delay before accept key, ent: gap before Enter, roll: bool}"""
    slow = dict(kd=90, acc=450, ent=300, roll=False)
    base = None
    for i in range(baseline_reps):
        base, _, _ = send(make(slow)); settle()
        if base is None: recover()
    if base is None: record(name, 'BASELINE-LOST', ''); return
    outcomes = collections.Counter(); bad = []
    for i in range(reps):
        sp = dict(kd=rng.choice([10, 15, 20, 25]), acc=rng.choice([0, 5, 10, 15, 25, 40]), ent=rng.choice([0, 5, 10, 15, 25]), roll=rng.random() < .5)
        if speeds: sp.update(speeds)
        got, e2s, (keys, raw) = send(make(sp), roll=sp['roll']); settle()
        if got is None:
            outcomes['LOST(no command ran)'] += 1; bad.append((sp, None)); recover(); continue
        if got == base: outcomes['same-as-slow'] += 1
        else: outcomes['DIFF'] += 1; bad.append((sp, got))
    print('== %s  baseline=%r %s' % (name, base, note))
    print('   %s' % dict(outcomes))
    for sp, got in bad[:4]: print('   FAIL speed=%s got=%r' % (sp, got))
    record(name, 'summary', dict(base=base, outcomes=dict(outcomes), fails=[(s, g) for s, g in bad]))

def feature_shot(name, steps, expect):
    send_steps = [(t, d) for t, d in steps]
    subprocess.run([KSEQ] + build(send_steps), capture_output=True)
    time.sleep(0.6)
    p = shot(name); txt = ocr(p)
    hit = [e for e in expect if e.lower() in txt.lower()]
    recover(); time.sleep(0.3)
    print('   [feature] %s expect=%s seen=%s -> %s' % (name, expect, hit, os.path.basename(p)))
    return len(hit) == len(expect)

def g_ghost():
    for kname, k in (('Right', '<Right>'), ('End', '<End>')):
        differential('ghost %s then Enter' % kname, lambda s, k=k: typ('rec alp', s['kd']) + [(k, s['ent'])] if False else typ('rec alp', s['acc']) + key(k, s['ent']) + key('<Return>', 30), REPS)
        differential('ghost %s then type X then Enter' % kname, lambda s, k=k: typ('rec alp', s['acc']) + key(k, s['kd']) + typ('X', s['ent']) + key('<Return>', 30), REPS)
        differential('ghost %s then Backspace x5 @25ms then Enter' % kname, lambda s, k=k: typ('rec alp', s['acc']) + key(k, s['kd']) + key('<BackSpace>', 25) * 0 + [('<BackSpace>', 25)] * 5 + key('<Return>', 30) if False else typ('rec alp', s['acc']) + key(k, s['kd']) + [('<BackSpace>', 25)] * 5 + [('<Return>', 30)], REPS)
    differential('ghost Right, BS x12 into history text, retype, Enter', lambda s: typ('rec alphabet', s['acc']) + key('<Right>', s['kd']) + [('<BackSpace>', 25)] * 6 + typ('1', s['kd']) + key('<Return>', 30), REPS)
    differential('ghost Right twice quickly then Enter', lambda s: typ('rec git s', s['acc']) + key('<Right>', 8) + key('<Right>', s['ent']) + key('<Return>', 30), REPS)
    differential('ghost accept, Left x3, type, Enter', lambda s: typ('rec echo h', s['acc']) + key('<Right>', s['kd']) + [('<Left>', 15)] * 3 + typ('Q', s['kd']) + key('<Return>', 30), REPS)
    differential('type, 10x Backspace, retype over stale ghost, Enter', lambda s: typ('rec alpha 1', s['kd']) + [('<BackSpace>', 12)] * 10 + typ('rec git st', s['kd']) + key('<Right>', s['ent']) + key('<Return>', 30), REPS)
    differential('Ctrl-F with ghost', lambda s: typ('rec alp', s['acc']) + key('<C-f>', s['ent']) + key('<Return>', 30), REPS, note='(Ctrl-F semantics probe)')
    differential('Ctrl-Right with ghost', lambda s: typ('rec alp', s['acc']) + key('<C-Right>', s['ent']) + key('<Return>', 30), REPS, note='(word-accept probe)')
    feature_shot('feature-ghost-up', typ('rec alp', 60), ['alpha-one'])

def g_tab():
    cases = [
     ('unique file in 5k dir', "rec big/f0500", 'big/f05000.dat'),
     ('dir with space', 'rec t/with', None), ('dir with quote', "rec t/quo", None), ('unicode dir', 'rec t/uni', None),
     ('dollar-literal dir', 'rec t/\\$HO', None), ('ambiguous alp*', 'rec t/alp', None), ('ambiguous 5k f04', 'rec big/f0499', None), ('cd dirs', 'cd t/', None),
     ('flag ls --co', 'ls --co', None), ('git checkout -', 'git checkout ', None), ('command name', 'rec_', None), ('home tilde', 'ls ~/t/with', None)]
    for nm, pre, _ in cases:
        # Tab -> accept (Enter) -> BS x10 @25 -> retype -> Enter
        def mk(s, pre=pre):
            return (typ(pre, s['kd']) + key('<Tab>', s['acc']) + key('<Return>' if False else '<Tab>', s['acc']) + key('<Escape>', s['kd']) + [('<BackSpace>', 25)] * 10 + typ('rec z', s['kd']) + key('<Return>', 30)) 
        differential('tab(%s): Tab,Tab,Esc,BS x10,retype,Enter' % nm, mk, max(10, REPS // 2))
        def mk2(s, pre=pre):
            return typ(pre, s['kd']) + key('<Tab>', s['acc']) + key('<Down>', s['kd']) + key('<Return>', s['ent']) + [('<BackSpace>', 25)] * 4 + typ('rec k', s['kd']) + key('<Return>', 30)
        differential('tab(%s): Tab,Down,Enter(accept),BS x4,retype,Enter' % nm, mk2, max(10, REPS // 2))
    # feature evidence
    feature_shot('feature-menu-alp', typ('rec t/alp', 40) + key('<Tab>', 50), ['alphabet', 'alpine'])
    feature_shot('feature-menu-big', typ('rec big/f0499', 40) + key('<Tab>', 50), ['f04990'])

def g_interleave():
    differential('big dir: Tab then type before result lands', lambda s: typ('rec big/f0', s['kd']) + key('<Tab>', 3) + typ('4999', s['kd']) + key('<Tab>', s['acc']) + key('<Return>', s['ent']) + [('<BackSpace>', 25)] * 3 + typ('rec y', s['kd']) + key('<Return>', 30), REPS)
    differential('Tab twice quick in big dir then Esc then type', lambda s: typ('rec big/f', s['kd']) + key('<Tab>', 2) + key('<Tab>', s['kd']) + key('<Escape>', s['kd']) + typ('rec q', s['kd']) + key('<Return>', 30), REPS)
    differential('Ctrl-C mid-menu then Enter on empty then new cmd', lambda s: typ('rec big/f0', s['kd']) + key('<Tab>', s['acc']) + key('<C-c>', s['kd']) + key('<Return>', 20) + typ('rec after', s['kd']) + key('<Return>', 30), REPS)
    differential('Esc mid-menu then Enter runs typed text', lambda s: typ('rec t/alp', s['kd']) + key('<Tab>', s['acc']) + key('<Escape>', s['kd']) + key('<Return>', s['ent']), REPS)
    differential('Right before ghost updates after fast typing', lambda s: typ('rec alphab', 6) + key('<Right>', s['ent']) + key('<Return>', 30), REPS)

def g_hist():
    # collisions
    for i in range(3):
        for c in ['rec alphabet soup', 'rec alpha beta', 'rec alpha', 'rec alphabet']:
            send(typ(c, 15) + key('<Return>', 30)); settle()
    differential('prefix typed fast then Right', lambda s: typ('rec alph', s['kd']) + key('<Right>', s['acc']) + key('<Return>', 30), REPS)
    differential('Right then BS to shorter prefix then Right again', lambda s: typ('rec alp', s['acc']) + key('<Right>', s['kd']) + [('<BackSpace>', 20)] * 8 + key('<Right>', s['acc']) + key('<Return>', 30), REPS)
    differential('delete back past prefix to empty then new prefix', lambda s: typ('rec alpha', s['kd']) + [('<BackSpace>', 12)] * 9 + typ('rec git', s['kd']) + key('<Right>', s['acc']) + key('<Return>', 30), REPS)

def g_enter():
    def lat(name, mk, n=30, pre=None):
        L = []; bad = 0
        for i in range(n):
            if pre: pre()
            got, e2s, _ = send(mk()); settle()
            if got is None: bad += 1; recover()
            elif e2s is not None: L.append(e2s)
        L.sort()
        if L:
            q = lambda p: L[min(len(L)-1, int(len(L)*p))]
            print('ENTER %-45s n=%d lost=%d median=%.1f p95=%.1f max=%.1f ms' % (name, len(L), bad, statistics.median(L), q(.95), L[-1]))
        else: print('ENTER %-45s no samples lost=%d' % (name, bad))
        record(name, 'enter', dict(n=len(L), lost=bad, samples=L))
    lat('plain short (type 25ms, wait 300, Enter)', lambda: typ('rec x', 25) + [('w300', 0)] + key('<Return>', 30))
    lat('plain short, Enter 15ms after last key', lambda: typ('rec x', 25) + key('<Return>', 30)) if False else None
    lat('short, Enter right after last key (15ms)', lambda: typ('rec y', 25)[:-1] + [("'y", 15), ('<Return>', 30)])
    lat('after Right accept (ghost), Enter 15ms', lambda: typ('rec alpha-o', 20) + [('w250', 0)] + key('<Right>', 15) + key('<Return>', 30))
    lat('after Tab accept (unique), Enter 15ms', lambda: typ('rec big/f0500', 20) + [('w250', 0)] + key('<Tab>', 15) + [('w100', 0)] + key('<Return>', 30))
    def bigout():
        subprocess.run([KSEQ, 'd20', "'rec seq 1 1 && seq 1 400000 | tail -1", '<Return>', 'w2500'], capture_output=True)
    lat('right after big-output command finished', lambda: typ('rec z', 20) + key('<Return>', 30), n=15, pre=bigout)

GROUPS = dict(ghost=g_ghost, tab=g_tab, interleave=g_interleave, hist=g_hist, enter=g_enter)
g = sys.argv[1]
GROUPS[g]()
json.dump(results, open(os.path.join(OUT, 'scen-%s.json' % g), 'w'), indent=1, default=str)
