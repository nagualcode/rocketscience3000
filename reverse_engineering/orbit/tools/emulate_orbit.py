#!/usr/bin/env python3
"""Run the ORIGINAL Orbit DSP (i386 slice of Orbit.vst 3.6.2) under Unicorn.

`OrbitProcess` uses virtual inheritance (the Mission base is found through
the vtable's −0xc offset); `InitBuffers`, `Process` and `ReceiveMidi` take
the derived pointer. Parameters are the binary's own objects.

    emulate_orbit.py BINARY scenario.json out.f32 [--trace]

Output: interleaved stereo float32.
"""
import json
import os
import struct
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', 'tools'))
from mission_emu import Mission  # noqa: E402

CTOR = 0x251A8                 # OrbitProcess::OrbitProcess() (complete object)
SIZE = 0xD0
INIT_BUFFERS = 0x25926
PROCESS = 0x263DA
RECEIVE_MIDI = 0x25620
GET_PARAMETER = 0x2398A
GET_PRIVATE_PARAMETER = 0x2373A
GET_PARAM_FROM_MAP = 0xE62C
GET_SAMPLE_RATE = 0xE54E
IS_BYPASSED = 0xE41E
OPTION_KEY_DOWN = 0x6620
INIT_BLACKOUT = 0x26FCC
BLACKOUT = 0x26D06

PARAM_IDS = {
    'source_x': 0, 'source_y': 1, 'room': 2, 'order': 3, 'inner_y': 4, 'inner_x': 5, 'damp': 6,
    'speed': 7, 'mode': 8, 'rad_x': 9, 'rad_y': 10, 'pos_x': 11, 'pos_y': 12, 'span': 13,
    'preset': 14, 'x': 15, 'y': 16,
}
LONG_PARAMS = {3, 6, 8, 14}


class Orbit(Mission):
    # The Doppler resampler reads up to two samples past its input block; in
    # the original that is whatever follows in the heap (here: the next
    # block's pointer table, ~1e-29 as a float). Allocations get a zeroed
    # guard band so those reads are 0, as in the port.
    def _new(self):
        self.ret(self.alloc(self.arg(0) + 64))
    imp__Znwm = imp__Znam = imp_malloc = imp_NewPtrClear = imp_NewPtr = _new

    def imp__Z31kfyjsttqvsrhiwsqisskgxpbrvgylwdPfS_l(self):
        """kfyj…(float* a, float* b, long n): b[i] = b[i] + a[i]."""
        a, b, n = self.arg(0), self.arg(1), self.arg(2)
        x = struct.unpack(f'<{n}f', self.uc.mem_read(a, 4 * n))
        y = struct.unpack(f'<{n}f', self.uc.mem_read(b, 4 * n))
        out = [struct.unpack('<f', struct.pack('<f', q + p))[0] for p, q in zip(x, y)]
        self.uc.mem_write(b, struct.pack(f'<{n}f', *out))
        self.ret(0)

    def __init__(self, binary, sample_rate, variant, buffer, values):
        super().__init__(binary)
        self.sr = sample_rate
        self.hook(GET_PARAM_FROM_MAP, lambda: self.ret(self.params.get(self.arg(1), 0)))
        self.hook(GET_SAMPLE_RATE, lambda: self.ret_double(self.sr))
        for a in (IS_BYPASSED, OPTION_KEY_DOWN, INIT_BLACKOUT, BLACKOUT):
            self.hook(a, lambda: self.ret(0))

        desc = self.alloc(0x100)
        self.params = {i: self.call(GET_PARAMETER, [desc, i]) for i in range(17)}
        for k in range(2):
            self.params[0x11 + k] = self.call(GET_PRIVATE_PARAMETER, [desc, k])
        self.settings = self.alloc(0x100)
        notify = self.make_vtable_object({})
        self.w32(self.settings + 0x18, notify)
        get_settings = self.code(b'\xB8' + struct.pack('<I', self.settings) + b'\xC3')
        get_variant = self.code(b'\xB8' + struct.pack('<I', variant) + b'\xC3')
        host = self.make_vtable_object({0x84: get_settings, 0x78: get_variant})

        self.this = self.alloc(SIZE)
        self.call(CTOR, [self.this])
        vt = self.r32(self.this)
        self.base = self.this + self.i32(vt - 0xC)
        b = self.base
        self.n_in = 1 if variant == 0 else 2
        self.w32(b + 0x18, variant)
        self.w64(b + 0x1C, sample_rate)
        self.w32(b + 0x24, self.n_in)
        self.w32(b + 0x28, 2)
        self.w32(b + 0x2C, self.settings)
        self.w32(b + 0x30, buffer)
        self.w32(b + 0x38, host)
        self.set_values(values)
        self.call(INIT_BUFFERS, [self.this])

    def set_values(self, values):
        for name, v in values.items():
            pid = PARAM_IDS[name]
            if pid in LONG_PARAMS:
                self.w32(self.params[pid] + 0x78, int(v))
            else:
                self.wf32(self.params[pid] + 0x78, float(v))

    def value(self, name):
        pid = PARAM_IDS[name]
        return self.i32(self.params[pid] + 0x78) if pid in LONG_PARAMS else self.f32(self.params[pid] + 0x78)

    def midi(self, status, d1, d2):
        self.call(RECEIVE_MIDI, [self.this, status, d1, d2])

    def process(self, inputs):
        n = len(inputs[0])
        ins = [self.alloc(4 * n) for _ in range(2)]
        outs = [self.alloc(4 * n) for _ in range(2)]
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
        return (f'src=({self.value("source_x"):.6f},{self.value("source_y"):.6f}) '
                f'angle={self.f32(self.this + 0x2c):.6f} seed={self.r32(self.this + 0x3c):#x}')


DEFAULTS = {'source_x': 0.0, 'source_y': 0.0, 'room': 25.0, 'order': 2, 'inner_y': -0.6, 'inner_x': 0.0,
            'damp': 0, 'speed': 3.0, 'mode': 3, 'rad_x': 0.8, 'rad_y': 0.8, 'pos_x': 0.0, 'pos_y': 0.5,
            'span': 3.0, 'x': 0.0, 'y': 0.0}


def main():
    binary, scen, out_path = sys.argv[1:4]
    trace = '--trace' in sys.argv
    sc = json.load(open(scen))
    variant = {'mono_to_stereo': 0, 'stereo': 1}[sc.get('layout', 'mono_to_stereo')]
    values = dict(DEFAULTS)
    values.update(sc.get('params', {}))
    n = sc['block']
    ob = Orbit(binary, sc['sample_rate'], variant, sc.get('buffer', n), values)
    raw = open(sc['input'], 'rb').read()
    sig = struct.unpack(f'<{len(raw) // 4}f', raw)
    left, right = sig[0::2], sig[1::2]
    changes = {int(k): v for k, v in sc.get('changes', {}).items()}
    midi = {int(k): v for k, v in sc.get('midi', {}).items()}
    out = bytearray()
    for b in range(sc['blocks']):
        if b in changes:
            ob.set_values(changes[b])
        for ev in midi.get(b, []):
            ob.midi(*ev)
        o = (b * n) % (len(left) - n)
        res = ob.process([left[o:o + n], right[o:o + n]])
        if trace:
            print(f'{b:4d} {ob.trace()}')
        for frame in zip(*res):
            out += struct.pack('<2f', *frame)
    open(out_path, 'wb').write(out)
    print(f'{sc["blocks"]} blocks: {ob.trace()}')


if __name__ == '__main__':
    main()
