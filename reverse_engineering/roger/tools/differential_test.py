#!/usr/bin/env python3
"""Differential test: the original Roger (emulated) against the Rust engine,
bit for bit.

    differential_test.py ORIGINAL_I386_BINARY path/to/roger-render WORKDIR [name-filter]
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


PLAY = {'bpm': 120.0, 'playing': True, 'start_ppq': 0.3}


def scenarios():
    S = []

    def add(name, params=None, changes=None, sr=44100, blocks=200, block=512, layout='stereo',
            transport=PLAY, midi=None, buffer=None):
        S.append(dict(name=name, sample_rate=sr, block=block, blocks=blocks, layout=layout,
                      params=params or {}, changes=changes or {}, transport=transport,
                      midi=midi or {}, buffer=buffer or block))

    add('defaults')
    add('defaults-no-transport', transport=None)
    for fam in range(3):
        for vowel in (0, 3, 5, 9):
            add(f'static-f{fam}-v{vowel}', {'auto': 0, 'family': fam, 'vowel': vowel}, blocks=80)
    for bw in (1, 20, 56, 57, 80, 100):
        add(f'bandwidth-{bw}', {'auto': 0, 'bandwidth': bw, 'vowel': 7}, blocks=80)
    add('bandwidth-glide', {'auto': 0}, changes={'10': {'bandwidth': 100}, '60': {'bandwidth': 3},
                                                 '120': {'bandwidth': 57}})
    for port in (1, 50, 100):
        add(f'portamento-{port}', {'auto': 0, 'portamento': port},
            changes={'5': {'vowel': 0}, '40': {'vowel': 9, 'family': 2}, '90': {'vowel': 3}})
    for sp in range(1, 13):
        add(f'auto-speed-{sp}', {'auto_speed': sp, 'portamento': 20}, blocks=120)
    add('auto-skip-0', {'auto_speed': 9, 'skip': 0})
    add('auto-skip-90', {'auto_speed': 10, 'skip': 90})
    add('auto-skip-100', {'auto_speed': 10, 'skip': 100})
    add('auto-toggle', {'auto': 0}, changes={'20': {'auto': 1}, '80': {'auto': 0}, '120': {'auto': 1, 'auto_speed': 7}})
    add('tempo-97', {'auto_speed': 8}, transport={'bpm': 97.0, 'playing': True, 'start_ppq': 3.71})
    add('tempo-174', {'auto_speed': 11, 'portamento': 5}, transport={'bpm': 174.0, 'playing': True, 'start_ppq': 0.0})
    notes = {str(b): [[0x90, 36 + (b * 7) % 48, 30 + (b * 13) % 97]] for b in range(0, 200, 6)}
    notes.update({str(b): [[0x80, 40, 0]] for b in range(3, 200, 12)})
    add('midi-notes', {'auto': 0}, midi=notes)
    add('midi-velocity', {'auto': 0, 'width_mode': 2, 'port_mode': 2}, midi=notes)
    wheel = {str(b): [[0xB0, 1, (b * 5) % 128]] for b in range(0, 200, 4)}
    add('midi-wheel', {'auto': 0, 'width_mode': 1, 'port_mode': 1}, midi=wheel)
    add('layout-mono', {'vowel': 2}, layout='mono')
    add('layout-mono-to-stereo', {'bandwidth': 70}, layout='mono_to_stereo')
    add('layout-mono-to-stereo-narrow', {'bandwidth': 10, 'auto': 0}, layout='mono_to_stereo')
    add('block-64', {'auto_speed': 9}, block=64, blocks=1500)
    add('block-333', {'auto_speed': 6}, block=333, blocks=300)
    add('block-1024', {'auto_speed': 10}, block=1024, blocks=100)
    add('long-run', {'auto_speed': 9, 'skip': 30, 'portamento': 30}, blocks=2000)
    add('sr-48000', {'auto_speed': 8}, sr=48000)
    add('sr-96000', {'auto_speed': 9, 'bandwidth': 15}, sr=96000, blocks=300)
    add('sr-22050', {'auto_speed': 7, 'family': 2}, sr=22050)
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
    emulator = os.path.join(HERE, 'emulate_roger.py')
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
