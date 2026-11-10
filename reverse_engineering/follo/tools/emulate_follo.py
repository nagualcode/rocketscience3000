#!/usr/bin/env python3
"""Run the ORIGINAL Follo DSP (i386 slice of Follo.vst 3.6.2) under Unicorn.

Follo's process class (`aihjf…`, obfuscated) uses virtual inheritance: the
Mission base sits at `this + 0x48`; `InitBuffers`, `Process` and
`ReceiveMidi` take the derived pointer. Parameters are the binary's own
objects; only the `std::map` lookup is answered from Python.

    emulate_follo.py BINARY scenario.json out.f32 [--trace]
"""
import json
import os
import struct
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', 'tools'))
from mission_emu import Mission  # noqa: E402

CTOR = 0x2342E
SIZE = 200
BASE = 0x48
INIT_BUFFERS = 0x22020
PROCESS = 0x21426
RECEIVE_MIDI = 0x21346
GET_PARAMETER = 0x22E7A
GET_PARAM_FROM_MAP = 0xE7BC
GET_SAMPLE_RATE = 0xE6DE
IS_BYPASSED = 0xE5AE
OPTION_KEY_DOWN = 0x67B0
INIT_BLACKOUT = 0x22AC8
BLACKOUT = 0x22954

PARAM_IDS = {'bandwidth': 0, 'release': 1, 'low': 2, 'high': 3, 'mean': 4, 'analysis': 5}


class Follo(Mission):
    def __init__(self, binary, sample_rate, variant, buffer, values):
        super().__init__(binary)
        self.sr = sample_rate
        self.hook(GET_PARAM_FROM_MAP, lambda: self.ret(self.params.get(self.arg(1), 0)))
        self.hook(GET_SAMPLE_RATE, lambda: self.ret_double(self.sr))
        for a in (IS_BYPASSED, OPTION_KEY_DOWN, INIT_BLACKOUT, BLACKOUT):
            self.hook(a, lambda: self.ret(0))

        desc = self.alloc(0x100)
        self.params = {i: self.call(GET_PARAMETER, [desc, i]) for i in range(6)}
        self.settings = self.alloc(0x100)
        notify = self.make_vtable_object({})
        self.w32(self.settings + 0x18, notify)
        get_settings = self.code(b'\xB8' + struct.pack('<I', self.settings) + b'\xC3')
        host = self.make_vtable_object({0x84: get_settings})

        self.this = self.alloc(SIZE)
        self.call(CTOR, [self.this])
        b = self.this + BASE
        n_in, n_out = {0: (1, 1), 1: (1, 2), 2: (2, 2)}[variant]
        self.n_in, self.n_out = n_in, n_out
        self.w32(b + 0x18, variant)
        self.w64(b + 0x1C, sample_rate)
        self.w32(b + 0x24, n_in)
        self.w32(b + 0x28, n_out)
        self.w32(b + 0x2C, self.settings)
        self.w32(b + 0x30, buffer)
        self.w32(b + 0x38, host)
        self.set_values(values)
        self.call(INIT_BUFFERS, [self.this])

    def set_values(self, values):
        for name, v in values.items():
            self.w32(self.params[PARAM_IDS[name]] + 0x78, int(v))

    def value(self, name):
        return self.i32(self.params[PARAM_IDS[name]] + 0x78)

    def midi(self, status, d1, d2):
        self.call(RECEIVE_MIDI, [self.this, status, d1, d2])

    def process(self, inputs):
        n = len(inputs[0])
        ins = [self.alloc(4 * n) for _ in range(self.n_in)]
        outs = [self.alloc(4 * n) for _ in range(self.n_out)]
        for p, x in zip(ins, inputs):
            self.uc.mem_write(p, struct.pack(f'<{n}f', *x))
        pin, pout = self.alloc(16), self.alloc(16)
        for i, p in enumerate(ins):
            self.w32(pin + 4 * i, p)
        for i, p in enumerate(outs):
            self.w32(pout + 4 * i, p)
        self.call(PROCESS, [self.this, pin, pout, n])
        res = [struct.unpack(f'<{n}f', self.uc.mem_read(p, 4 * n)) for p in outs]
        self.brk = ins[0]
        return res

    def trace(self):
        vals = ' '.join(f'{k}={self.value(k)}' for k in PARAM_IDS)
        return f'{vals} env={self.f32(self.this + 0x44):.7f} agc={self.f32(self.this + BASE + 0x7c):.7f}'


DEFAULTS = {'bandwidth': 50, 'release': 50, 'low': 250, 'high': 800, 'analysis': 0}


def main():
    binary, scen, out_path = sys.argv[1:4]
    trace = '--trace' in sys.argv
    sc = json.load(open(scen))
    variant = {'mono': 0, 'mono_to_stereo': 1, 'stereo': 2}[sc.get('layout', 'stereo')]
    values = dict(DEFAULTS)
    values.update(sc.get('params', {}))
    n = sc['block']
    fo = Follo(binary, sc['sample_rate'], variant, sc.get('buffer', n), values)
    raw = open(sc['input'], 'rb').read()
    sig = struct.unpack(f'<{len(raw) // 4}f', raw)
    left, right = sig[0::2], sig[1::2]
    changes = {int(k): v for k, v in sc.get('changes', {}).items()}
    midi = {int(k): v for k, v in sc.get('midi', {}).items()}
    out = bytearray()
    for b in range(sc['blocks']):
        if b in changes:
            fo.set_values(changes[b])
        for ev in midi.get(b, []):
            fo.midi(*ev)
        o = (b * n) % (len(left) - n)
        ins = [left[o:o + n]] if fo.n_in == 1 else [left[o:o + n], right[o:o + n]]
        res = fo.process(ins)
        if trace:
            print(f'{b:4d} {fo.trace()}')
        for frame in zip(*res):
            out += struct.pack(f'<{len(frame)}f', *frame)
    open(out_path, 'wb').write(out)
    print(f'{sc["blocks"]} blocks: {fo.trace()}')


if __name__ == '__main__':
    main()
