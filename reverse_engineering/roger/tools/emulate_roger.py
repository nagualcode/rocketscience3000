#!/usr/bin/env python3
"""Run the ORIGINAL Roger DSP (i386 slice of Roger.vst 3.6.2) under Unicorn.

The parameter objects are the binary's own (`RogerInfo::GetParameter`);
only the `std::map` lookup is answered from Python. The host-time context is
the binary's own object too; the harness writes tempo and beat position into
it like the VST wrapper does. MIDI goes through the binary's
`RogerProcess::ReceiveMidi`.

    emulate_roger.py BINARY scenario.json out.f32 [--trace]

Output: interleaved float32, one or two channels.
"""
import json
import math
import os
import struct
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', 'tools'))
from mission_emu import Mission  # noqa: E402

CTOR = 0x21D0E                 # RogerProcess::RogerProcess
SIZE = 0x1C4                   # RogerInfo::MakeProcess: operator new(0x1c4)
INIT_BUFFERS = 0x22992
PROCESS = 0x22D10
RECEIVE_MIDI = 0x2255C
GET_PARAMETER = 0x215D4        # RogerInfo::GetParameter
GET_PARAM_FROM_MAP = 0xE98C
GET_SAMPLE_RATE = 0xE8AE
IS_BYPASSED = 0xE77E
OPTION_KEY_DOWN = 0x6980
INIT_BLACKOUT = 0x24E9E
BLACKOUT = 0x2477A

PARAM_IDS = {
    'bandwidth': 0, 'portamento': 1, 'auto_speed': 2, 'width_mode': 3, 'port_mode': 4,
    'auto': 5, 'vowel': 6, 'family': 7, 'skip': 8,
}


class Roger(Mission):
    def __init__(self, binary, sample_rate, variant, buffer, values):
        super().__init__(binary)
        self.sr = sample_rate
        self.hook(GET_PARAM_FROM_MAP, lambda: self.ret(self.params.get(self.arg(1), 0)))
        self.hook(GET_SAMPLE_RATE, lambda: self.ret_double(self.sr))
        for a in (IS_BYPASSED, OPTION_KEY_DOWN, INIT_BLACKOUT, BLACKOUT):
            self.hook(a, lambda: self.ret(0))

        desc = self.alloc(0x100)
        self.params = {i: self.call(GET_PARAMETER, [desc, i]) for i in range(9)}
        self.settings = self.alloc(0x100)
        notify = self.make_vtable_object({})
        self.w32(self.settings + 0x18, notify)
        get_settings = self.code(b'\xB8' + struct.pack('<I', self.settings) + b'\xC3')
        get_variant = self.code(b'\xB8' + struct.pack('<I', variant) + b'\xC3')
        host = self.make_vtable_object({0x84: get_settings, 0x78: get_variant})

        self.this = self.alloc(SIZE)
        self.call(CTOR, [self.this])
        n_in, n_out = {0: (1, 1), 1: (1, 2), 2: (2, 2)}[variant]
        self.n_in, self.n_out = n_in, n_out
        self.w32(self.this + 0x18, variant)
        self.w64(self.this + 0x1C, sample_rate)
        self.w32(self.this + 0x24, n_in)
        self.w32(self.this + 0x28, n_out)
        self.w32(self.this + 0x2C, self.settings)
        self.w32(self.this + 0x30, buffer)
        self.w32(self.this + 0x38, host)
        self.set_values(values)
        self.call(INIT_BUFFERS, [self.this])

    def set_values(self, values):
        for name, v in values.items():
            obj = self.params[PARAM_IDS[name]]
            self.w32(obj + 0x78, int(v))

    def value(self, name):
        return self.i32(self.params[PARAM_IDS[name]] + 0x78)

    def set_transport(self, playing, changed, bpm, ppq):
        ctx = self.r32(self.this + 0x34)
        spq = self.r64(ctx + 0x44) / (bpm / 60.0)
        self.w64(ctx + 0x28, bpm)
        self.w64(ctx + 0x20, spq)
        self.w64(ctx + 0x18, spq / 24.0)
        frac = ppq - math.trunc(ppq)
        self.w32(ctx + 0x38, int((1.0 - frac) * spq))
        self.w32(ctx + 0x4C, 1 if playing else 0)
        self.w32(ctx + 0x50, 1 if changed else 0)
        self.w64(ctx + 0x54, frac)

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
        f = self.r32(self.this + 0x150)
        fm = struct.unpack('<6d', self.uc.mem_read(f, 48))
        return (f'vowel={self.value("vowel")} fam={self.value("family")} bw={self.f32(self.this + 0x16c):.4f} '
                f'beat={self.r64(self.this + 0x184):.3f} seed={self.r32(self.this + 0x190):#x} '
                f'steps={self.i32(self.this + 0x178)} F=({fm[0]:.3f},{fm[2]:.3f},{fm[4]:.3f}) '
                f'agc={self.f32(self.this + 0x7c):.7f}')


DEFAULTS = {'bandwidth': 40, 'portamento': 40, 'auto_speed': 4, 'width_mode': 0, 'port_mode': 0,
            'auto': 1, 'vowel': 5, 'family': 0, 'skip': 10}


def main():
    binary, scen, out_path = sys.argv[1:4]
    trace = '--trace' in sys.argv
    sc = json.load(open(scen))
    variant = {'mono': 0, 'mono_to_stereo': 1, 'stereo': 2}[sc.get('layout', 'stereo')]
    values = dict(DEFAULTS)
    values.update(sc.get('params', {}))
    n = sc['block']
    rg = Roger(binary, sc['sample_rate'], variant, sc.get('buffer', n), values)
    raw = open(sc['input'], 'rb').read()
    sig = struct.unpack(f'<{len(raw) // 4}f', raw)
    left, right = sig[0::2], sig[1::2]
    changes = {int(k): v for k, v in sc.get('changes', {}).items()}
    midi = {int(k): v for k, v in sc.get('midi', {}).items()}
    out = bytearray()
    for b in range(sc['blocks']):
        if b in changes:
            rg.set_values(changes[b])
        for ev in midi.get(b, []):
            rg.midi(*ev)
        tr = sc.get('transport')
        if tr:
            ppq = tr.get('start_ppq', 0.0) + b * n / (sc['sample_rate'] * 60.0 / tr['bpm'])
            rg.set_transport(tr['playing'], b == 0, tr['bpm'], ppq)
        o = (b * n) % (len(left) - n)
        ins = [left[o:o + n]] if rg.n_in == 1 else [left[o:o + n], right[o:o + n]]
        res = rg.process(ins)
        if trace:
            print(f'{b:4d} {rg.trace()}')
        for frame in zip(*res):
            out += struct.pack(f'<{len(frame)}f', *frame)
    open(out_path, 'wb').write(out)
    print(f'{sc["blocks"]} blocks: {rg.trace()}')


if __name__ == '__main__':
    main()
