# Reverse engineering Follo

How **Follo**, the "energy driven band booster" of Audio Ease's Rocket
Science bundle (2001–2013; interface by Oskar Luyer), was recovered from the
original machine code, and how `plugins/follo/src/engine.rs` was proven
**bit-identical** to it. Method and tools as for [Roger](../roger/README.md).

| Path | What it is |
|---|---|
| `original_binaries/Follo-3.6.2-VST-i386` | i386 slice of `Follo.vst` 3.6.2 — the reference |
| `original_binaries/Follo-3.6.2-VST-ppc` | its PowerPC slice |
| `decompiled/follo_i386.c`, `symbols.txt` | Ghidra output and symbols |
| `tools/emulate_follo.py` | runs the original `InitBuffers` / `Process` / `ReceiveMidi` under Unicorn |
| `tools/differential_test.py` | 41 scenarios, original vs Rust, bit for bit |
| `ui/` | the original panel (PICT 3004), knob strip, about box |

## 1. Map

Unlike Roger, Follo's own classes are obfuscated:

| Symbol | Role |
|---|---|
| `aihjf…` | the process (200 bytes, **virtual inheritance**: the Mission base at `this + 0x48`, reached through the vtable's −0xc offset): ctor @ 0x2342e, `InitBuffers` @ 0x22020, `Process` @ 0x21426, `ReceiveMidi` @ 0x21346, `GetLatency` @ 0x213e2 (300) |
| `sjgac…` | description: `GetParameter` @ 0x22e7a (6 parameters), `MakeProcess` @ 0x23312 |
| `dB2Amp` @ 0x2119a | `pow(10, dB/20)` |
| `wkbtgem…` | the Mission AGC / look-ahead limiter (as in Roger) |

## 2. Parameters

| Id | Name | Range | Default | Centre | MIDI |
|---|---|---|---|---|---|
| 0 | Bandwidth | 1…100 | 50 | 50 | CC 12 |
| 1 | Release | 1…100 | 50 | 50 | CC 13 |
| 2 | Lo Freq Cutoff (yellow: peak at low amplitude) | 0…1080 | 250 | 540 | CC 14 |
| 3 | Hi Freq Cutoff (blue: peak at high amplitude) | 0…1080 | 800 | 540 | CC 15 |
| 4 | Mean display | 0…10800 | 20 | | written by `Process` (the ball, Hz) |
| 5 | Analysis | −12…12 dB | 0 | 0 | CC 11 |

The sliders count **tenths of a semitone above 20 Hz**: `20 · 1.059463^(pos/10)`,
0…1080 = 20 Hz…10.2 kHz in nine octaves — with the semitone typed as
**1.059463**, while the bandwidth uses the exact `2^(1/12)`.

MIDI CCs go through the framework's `SetValueNormalized` (@ 0xd5c8): value/128,
then a **power curve that puts 0.5 on the parameter's centre**
(`n^p`, `p = ln((centre−min)/(max−min)) / ln ½`), truncated. For the
sliders and Analysis the centre is the midpoint, so it is linear; for
Bandwidth and Release (centre 50 on 1…100) slightly bent. Checked against all
5 × 128 values of the original.

## 3. Process

**The follower**: `env = max(env, |x|·10^(analysis/20))`, then
`env *= 0.1^(1/(T·sr))` every sample — an instant attack and a release that
falls 20 dB in `T = 0.04 + 2.96 · (r·1.05^r / 1.05^100) / 100` seconds
(40 ms…3 s; 169 ms at 50). In stereo the louder channel drives it. The
follower is double inside a buffer and stored as float between buffers.

**Every 50 samples** (counted from the start of each buffer) the centre is
set from the follower: the blue frequency if `env > 1`, otherwise
`lo + (hi − lo)·env` — but only in the **mono→stereo** variant; mono and
stereo use **env²**. Then one band-pass:

```
semitones = breakpoint(bandwidth: 1→0.07, 25→0.2, 50→2.5, 90→12, 100→400000)
width     = f − f / 2^(semitones/12)
r         = exp(−2·width·π / sr)          π typed 3.1415926528
w         = f · 6.2831853056 / sr         2π typed 6.2831853056
y = 1.1·sin w · x + (−r²)·sin w · x[n−1] + 2r·cos w · y[n−1] − r² · y[n−2]
```

(at Bandwidth 100, 400 000 semitones: `pow` overflows to ∞ and the band is
as wide as the frequency itself.) Mono→stereo outputs the band left and the
**residue** `x − y` right — the stereo trick the manual recommends for synth
basses. Then the Mission limiter (−6 dBFS, 300 samples), so `GetLatency` = 300.

The mistyped constants are the same as in Riverrun (π) and in PeriScope and
Roger (2π): Follo has both.

## 4. Verification

```
41 scenarios — the three layouts, bandwidth 1…100 (also mono→stereo), release
1…100, analysis −12…12, inverted / equal / extreme sliders, a "bass thump"
setting, automation, MIDI CCs, blocks of 64/333/512/1024, a million samples,
22.05/44.1/48/96 kHz:  41 identical, 0 different
```

The harness needed C semantics for `pow` overflow (Python raises; C returns ∞).

## 5. Mac OS 9 (v2.04)

`reference/MacOS9/Follo™` has the same constants — 3.1415926528,
6.2831853056, 1.059463, 2^(1/12), the bandwidth table (0.07 … 400 000), 1.1,
1.05, 0.04 — but no 2.96: its release curve differs.

## 6. What the port changes

| | Original 3.6.2 | Port |
|---|---|---|
| Engine output | — | bit-identical (§4) |
| MIDI CCs | written into the parameter objects | held by the engine until the host moves the parameter |
| Output trim | — | Output (dB), default 0 |
| Editor | the strip with two sliders and the ball | the same, plus the band's live response |
