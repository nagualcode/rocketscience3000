#!/usr/bin/env python3
"""Differential test: the original Follo (emulated) against the Rust engine,
bit for bit.

    differential_test.py ORIGINAL_I386_BINARY path/to/follo-render WORKDIR [name-filter]
"""
import json
import math
import os
import random
import struct
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))


def make_input(path, sr, seconds=6.0, seed=3):
    """Stereo test signal: a buzzy sawtooth "voice" with vibrato plus noise,
    and near-silent passages so the resonators ring out into the denormal
    range."""
    n = int(sr * seconds)
    rnd = random.Random(seed)
    v = []
    ph_l = ph_r = 0.0
    for i in range(n):
        t = i / sr
        ph_l = (ph_l + (110 + 3 * math.sin(2 * math.pi * 5 * t)) / sr) % 1.0
        ph_r = (ph_r + 165 / sr) % 1.0
        l = 0.3 * (2 * ph_l - 1) + 0.05 * (rnd.random() - 0.5)
        r = 0.3 * (2 * ph_r - 1) + 0.1 * (rnd.random() - 0.5)
        if int(t * 2) % 4 == 3:
            l *= 0.0001
            r *= 0.0001
        v += [l, r]
    with open(path, 'wb') as f:
        f.write(struct.pack(f'<{len(v)}f', *v))


def scenarios():
    S = []

    def add(name, params=None, changes=None, sr=44100, blocks=200, block=512, layout='stereo', midi=None, buffer=None):
        S.append(dict(name=name, sample_rate=sr, block=block, blocks=blocks, layout=layout,
                      params=params or {}, changes=changes or {}, midi=midi or {}, buffer=buffer or block))

    for lay in ('mono', 'mono_to_stereo', 'stereo'):
        add(f'defaults-{lay}', layout=lay)
    for bw in (1, 10, 25, 50, 75, 90, 100):
        add(f'bandwidth-{bw}', {'bandwidth': bw}, blocks=80)
        add(f'bandwidth-{bw}-m2s', {'bandwidth': bw}, layout='mono_to_stereo', blocks=80)
    for rel in (1, 20, 50, 80, 100):
        add(f'release-{rel}', {'release': rel}, blocks=100)
    for an in (-12, -6, 6, 12):
        add(f'analysis-{an}', {'analysis': an}, blocks=100)
    add('sliders-inverted', {'low': 900, 'high': 100})
    add('sliders-equal', {'low': 500, 'high': 500})
    add('sliders-extreme', {'low': 0, 'high': 1080, 'analysis': 12})
    add('sliders-extreme-m2s', {'low': 0, 'high': 1080, 'bandwidth': 8}, layout='mono_to_stereo')
    add('bass-thump', {'low': 0, 'high': 150, 'bandwidth': 5, 'analysis': 12, 'release': 30}, layout='mono_to_stereo')
    add('automation', changes={'20': {'bandwidth': 10}, '50': {'low': 100, 'high': 1000},
                               '90': {'release': 5, 'analysis': 9}, '140': {'bandwidth': 99, 'release': 99}})
    cc = {str(b): [[0xB0, 11 + (b // 4) % 5, (b * 23) % 128]] for b in range(0, 200, 4)}
    add('midi-cc', midi=cc)
    add('midi-cc-m2s', midi=cc, layout='mono_to_stereo')
    add('block-64', {'bandwidth': 30}, block=64, blocks=1500)
    add('block-333', {'release': 70}, block=333, blocks=300)
    add('block-1024', {'analysis': 6}, block=1024, blocks=100)
    add('long-run', {'bandwidth': 40, 'release': 60}, blocks=2000, layout='mono_to_stereo')
    add('sr-48000', {'bandwidth': 20}, sr=48000)
    add('sr-96000', {'bandwidth': 60, 'high': 1000}, sr=96000, blocks=300)
    add('sr-22050', {'low': 50, 'high': 700}, sr=22050)
    return S


def run(cmd):
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f'{" ".join(cmd)}\n{r.stdout}\n{r.stderr}')


def compare(a, b):
    x, y = open(a, 'rb').read(), open(b, 'rb').read()
    if x == y:
        return True, ''
    n = min(len(x), len(y)) // 4
    xa, ya = struct.unpack(f'<{n}f', x[:4 * n]), struct.unpack(f'<{n}f', y[:4 * n])
    d = [i for i in range(n) if xa[i] != ya[i]]
    if not d:
        return False, f'length {len(x)} vs {len(y)}'
    peak = max(map(abs, xa)) or 1.0
    md = max(abs(xa[i] - ya[i]) for i in d)
    return False, f'{len(d)} values differ, first at value {d[0]}, max |diff| {md:.3e} ({md / peak:.1e} of peak)'


def main():
    binary, runner, work = sys.argv[1:4]
    filt = sys.argv[4] if len(sys.argv) > 4 else ''
    os.makedirs(work, exist_ok=True)
    emulator = os.path.join(HERE, 'emulate_follo.py')
    ok = fail = 0
    for sc in scenarios():
        if filt not in sc['name']:
            continue
        inp = os.path.join(work, f'input-{sc["sample_rate"]}.f32')
        if not os.path.exists(inp):
            make_input(inp, sc['sample_rate'])
        sc['input'] = inp
        js = os.path.join(work, sc['name'] + '.json')
        json.dump(sc, open(js, 'w'))
        eo, ro = js[:-5] + '.orig.f32', js[:-5] + '.rust.f32'
        t0 = time.time()
        run([sys.executable, emulator, binary, js, eo])
        t1 = time.time()
        run([runner, '--scenario', js, ro])
        same, why = compare(eo, ro)
        frames = sc['blocks'] * sc['block']
        print(f'{sc["name"]:<30} {frames:>8} frames  {"IDENTICAL" if same else "DIFFERENT"}  '
              f'(emulated in {t1 - t0:.1f}s) {why}', flush=True)
        ok += same
        fail += not same
    print(f'\n{ok} identical, {fail} different')
    sys.exit(1 if fail else 0)


if __name__ == '__main__':
    main()
