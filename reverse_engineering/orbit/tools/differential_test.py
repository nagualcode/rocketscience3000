#!/usr/bin/env python3
"""Differential test: the original Orbit (emulated) against the Rust engine,
bit for bit.

    differential_test.py ORIGINAL_I386_BINARY path/to/orbit-render WORKDIR [name-filter]
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

    def add(name, params=None, changes=None, sr=44100, blocks=200, block=512, layout='mono_to_stereo', midi=None, buffer=None):
        S.append(dict(name=name, sample_rate=sr, block=block, blocks=blocks, layout=layout,
                      params=params or {}, changes=changes or {}, midi=midi or {}, buffer=buffer or block))

    add('defaults')
    add('defaults-stereo', layout='stereo')
    for o in range(6):
        add(f'order-{o}', {'order': o, 'speed': 6.0}, blocks=120)
    for room in (8.0, 40.0, 80.0):
        add(f'room-{room:.0f}', {'room': room, 'order': 3}, blocks=150)
    for d in (30, 70, 100):
        add(f'damp-{d}', {'damp': d, 'order': 4}, blocks=120)
    for sp in (0.2, 6.0):
        add(f'span-{sp}', {'span': sp, 'speed': 10.0}, blocks=120)
    add('listener-moved', {'inner_x': 0.7, 'inner_y': 0.3, 'order': 2})
    add('static', {'mode': 1, 'x': -0.4, 'y': 0.7, 'order': 3})
    add('static-moved', {'mode': 1, 'order': 2}, changes={'40': {'x': 0.6}, '80': {'y': -0.5}, '120': {'inner_x': 0.5}})
    add('follow', {'mode': 2, 'x': 0.9, 'y': -0.8, 'speed': 12.0}, changes={'60': {'x': -0.9, 'y': 0.9}, '61': {'x': 0.2}})
    add('ellipse-fast', {'mode': 3, 'speed': 60.0, 'order': 2})
    add('ellipse-back', {'mode': 4, 'speed': 25.0, 'rad_x': 0.3, 'rad_y': 0.9})
    add('ellipse-outside', {'mode': 3, 'pos_x': 0.8, 'rad_x': 0.8, 'speed': 30.0})
    add('ellipse-zero', {'mode': 3, 'rad_x': 0.0, 'rad_y': 0.0})
    add('lsd', {'mode': 5, 'speed': 20.0, 'order': 3})
    add('lsd-fast', {'mode': 5, 'speed': 99.0, 'rad_x': 1.0, 'rad_y': 1.0})
    add('mode-switches', changes={'30': {'mode': 5}, '70': {'mode': 2, 'x': 0.5, 'y': 0.5}, '110': {'mode': 4}, '150': {'mode': 1}})
    add('automation', changes={'25': {'room': 60.0}, '50': {'order': 5}, '75': {'damp': 50}, '100': {'span': 1.0, 'speed': 15.0}})
    midi = {str(b): [[0xB0, 11 + (b // 3) % 10, (b * 29) % 128]] for b in range(0, 200, 3)}
    midi.update({str(b): [[0x90, 30 + (b * 7) % 70, 20 + b % 100], [0xE0, b % 128, (b * 3) % 128]] for b in range(1, 200, 9)})
    midi.update({str(b): [[0xC0, b % 7, 0]] for b in range(2, 200, 40)})
    add('midi', midi=midi)
    add('block-64', {'speed': 8.0}, block=64, blocks=1200)
    add('block-333', {'mode': 5, 'speed': 15.0}, block=333, blocks=250)
    add('block-1024', {'order': 4}, block=1024, blocks=100)
    add('block-small-buffer-big', {'speed': 10.0}, block=256, buffer=1024, blocks=300)
    add('long-run', {'mode': 5, 'speed': 12.0, 'order': 3, 'damp': 20}, blocks=1500)
    add('sr-48000', {'order': 3, 'speed': 12.0}, sr=48000)
    add('sr-96000', {'order': 2, 'speed': 12.0}, sr=96000, blocks=300)
    add('sr-22050', {'order': 4}, sr=22050)
    add('sr-8000', {'room': 80.0, 'order': 5, 'mode': 5, 'speed': 40.0}, sr=8000)
    add('huge-room-order-5', {'room': 80.0, 'order': 5, 'mode': 5, 'speed': 40.0, 'rad_x': 1.0, 'rad_y': 1.0})
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
    emulator = os.path.join(HERE, 'emulate_orbit.py')
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
