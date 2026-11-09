# Reverse engineering Roger

How the vowel filter of **Roger** (Audio Ease, Rocket Science bundle,
2001–2013; interface by Oskar Luyer) was recovered from the original machine
code, and how `plugins/roger/src/engine.rs` was proven **bit-identical** to
it. Method and tools are those of
[nautilus3000](https://github.com/nagualcode/nautilus3000) (Riverrun, Deep
Phase Nine, PeriScope); the shared tools live in [`../tools`](../tools).

| Path | What it is |
|---|---|
| `original_binaries/Roger-3.6.2-VST-i386` | i386 slice of `Roger.vst` 3.6.2 — the reference |
| `original_binaries/Roger-3.6.2-VST-ppc` | its PowerPC slice |
| `decompiled/roger_i386.c`, `symbols.txt` | Ghidra output and symbol table |
| `tools/emulate_roger.py` | runs the original `InitBuffers` / `Process` / `ReceiveMidi` under Unicorn |
| `tools/differential_test.py` | 55 scenarios, original vs Rust, bit for bit |
| `ui/` | the original panel (PICT 2080), knob strip and about box |

## 1. Map

Same "Mission" framework as the Nautilus Bundle, but in Rocket Science the
plug-in's own classes kept their names — only the framework is obfuscated:

| Symbol | Role |
|---|---|
| `RogerProcess` | ctor @ 0x21d0e (0x1c4 bytes), `InitBuffers` @ 0x22992, `Process` @ 0x22d10, `ReceiveMidi` @ 0x2255c, `GetLatency` @ 0x22988 (300) |
| `RogerInfo` | `GetParameter` @ 0x215d4 (9 parameters, no private ones), 3 variants (mono, mono→stereo, stereo) |
| `RogerVowelParm`, `RogerFamilyMemberParm`, `RogerAutoSpeedParm`, `RogerKnobModeParm` | value strings |
| `_form` @ 0x3cba0 | the vowel table |
| `wkbtgem…` | the Mission AGC / look-ahead limiter (`yruwg…` in Nautilus) |
| `adodg…` | the host-time context: `GetTempo(struct*)`, `GetSamplesToNextBeat`, `GetTransportChanged` |

## 2. Parameters

| Id | Name | Range | Default | MIDI |
|---|---|---|---|---|
| 0 | Bandwidth | 1…100 | 40 | CC 1 or velocity, per Width Mode |
| 1 | Portamento | 1…100 | 40 | CC 1 or velocity, per Port Mode |
| 2 | Auto Speed | 1…12 = 1/16, 1/8, 1/4, 1/2, 1, 2, 3, 4, 6, 8, 12, 16 vowels per beat | 4 (1/2) | |
| 3 | Width Mode | knob / modulation wheel / velocity | knob | |
| 4 | Port Mode | knob / modulation wheel / velocity | knob | |
| 5 | Speed Mode (= Auto) | off / on | **on** | |
| 6 | Vowel | OO AW UH AH U IR A E I EE | 5 (IR) | notes |
| 7 | Family Member | 0 Cindy, 1 Roger, 2 Patty | 0 (Cindy) | notes |
| 8 | Skip Percentage | 0…100 | 10 | |

## 3. The vowel table

`_form` holds 30 rows of `[F1, A1, F2, A2, F3, A3]` doubles — and they are
**Peterson & Barney's 1952 vowel formants**, the classic measurements of
American English vowels: averages for men (Roger), women (Patty) and children
(Cindy), with the formant amplitudes converted from dB to linear. Roger's
/i/ ("EE", bEEt) is 270 / 2290 / 3010 Hz, his /ɑ/ ("AH") 730 / 1090 / 2440 Hz,
exactly as in the 1952 paper. The ten vowels are P&B's ten: /u ɔ ʊ ɑ ʌ ɝ æ ɛ ɪ i/.
A fresh instance starts from Roger's "U" (hard-coded in `Process`).

## 4. Process

Per channel, **four two-pole resonators in parallel**, summed:

| | centre | damping | input gain | amplitude |
|---|---|---|---|---|
| 1 | F1 | 0.005 | ×0.72 | A1 |
| 2 | F2 | 0.004 | ×1 | A2 |
| 3 | F3 | 0.003 | ×1.2 | A3 |
| 4 | **2·F3** | 0.008 | ×0.8 | A3 |

with `r = exp(−F·damping·w / sr)`, `y = b·x + c·x[n−1] + 2r·cos(2πF/sr)·y[n−1] − r²·y[n−2]`,
`b = (1−r)·g·A·k`, `c = r(1−r)·g·k` — an extra zero from the previous input
(without the amplitude factor), and a fourth formant an octave above the
third, made up for brightness. The coefficients are double, the state is
rounded to float every sample.

**Control rate.** Every 128 samples, and always at the first sample of a
buffer (the counter is a local of `Process`): bandwidth glide, portamento
step, coefficients.

**Bandwidth** *w*: up to 56, `w = 5b·1.01^(5b) / 16.217` and a gain
compensation `g` from a breakpoint table (50 at 1 … 0.3 at 56). Above 56 the
filter stops narrowing; instead the dry signal fades in (`dry = (b−56)/45`,
`wet = √(1 − (b−56)/45)`) — "turning the knob all the way to the right will
make the filters so wide that the output will in fact not be filtered". The
wet/dry pair is computed at the control ticks but applied once, after the
loop, with the last tick's value. A new bandwidth glides there in 64 ticks.

**Portamento**: a glide of `sr · 5 · 1.07^(p − 100)` samples (5 s at 100,
86 ms at 40, 6 ms at 1) cut into `/128` control ticks; the six increments are
recomputed when vowel, family or glide time change, and kept between buffers
in **single precision** (`this + 0x154…`) while used in double within one.

**2π** is typed **6.2831853056** — the same typo as PeriScope's Hann window
(and Riverrun's π 3.1415926528): one programmer's constant across both
bundles.

**Auto**: a countdown in samples (double). At zero, a linear congruential
generator (×1664525 + 1013904223, the *Numerical Recipes* one) is stepped; if
its value ≥ skip % and Auto is on, the vowel becomes `seed % 10`, re-rolled
until it differs from the current one, and is written into the Vowel
parameter itself. Then the countdown gets one period: the Auto Speed's
fraction of `samples per quarter`. It resynchronises to the host's next beat
when the transport changes, the speed changes, or Auto is switched on.

**Output**: mono / stereo as above; mono→stereo puts the filtered signal left
and **the residue** right (`x − filtered`, plus the dry part) — the vowel and
its complement on either side. Then the Mission limiter (−6 dBFS, 300
samples look-ahead, identical to Deep Phase Nine's), hence `GetLatency` = 300.

**MIDI** (`ReceiveMidi`): note-on → vowel from the pitch class (C and C♯ OO,
D AW, D♯ UH, E AH, F U, F♯ IR, G and G♯ A, A E, A♯ I, B EE) and family from
`(note / 12) mod 3` (Roger 48–59, Patty 60–71, Cindy 72–83, repeating);
CC 1 → bandwidth / portamento linearly, or the velocity through a curve
(0–8 → 100, 30 → 55, 100 → 10, 127 → 0) — hard hits, sharp quick vowels.

## 5. Quirks kept

* The generator's seed (`this + 0x190`) and the last Auto Speed (`+ 0x194`)
  are never initialised: operator-new memory. On a fresh heap they are 0,
  which is what the emulation and the port use.
* **No host tempo, no beat**: the host-time context's constructor calls
  `SetTempo(120)` *before* it stores the sample rate, so samples per quarter
  stays 0 until the host sends a tempo — and Auto then rolls a new vowel on
  every sample.
* Wet/dry from the last control tick of a buffer, applied to all of it; the
  control clock restarts with every buffer, so (like Riverrun's grain clock)
  the exact output depends on the host buffer size.

## 6. Verification

`tools/emulate_roger.py` runs the binary's own constructor, `InitBuffers`,
`Process` and `ReceiveMidi` under Unicorn, with the binary's parameter
objects and host-time context:

```
55 scenarios — defaults, no transport, 3 family members × 4 vowels, bandwidth
1/20/56/57/80/100 and glides, portamento 1/50/100, all 12 Auto speeds, skips
0/90/100, Auto toggling, 97 and 174 bpm, MIDI notes / velocity / mod wheel,
mono, mono→stereo, stereo, blocks of 64/333/512/1024, a million-sample run,
22.05/44.1/48/96 kHz:  55 identical, 0 different
```

## 7. Mac OS 9 (v2.04)

`reference/MacOS9/Roger™` (PEF): the vowel table is **bit-identical** (all
180 doubles), and so are the constants — 6.2831853056, 16.217, the damping
factors 0.005 / 0.004 / 0.003 / 0.008, the gains 0.72 / 1.2 / 0.8, 1.07, 1.01,
45, 56. Only the 1/64 of the bandwidth glide is absent, so v2.04 moved the
bandwidth differently. Executing it (PowerPC/CFM) is an open item.

## 8. What the port changes

| | Original 3.6.2 | Port |
|---|---|---|
| Engine output | — | bit-identical (§6) |
| Auto / MIDI vowel changes | written into the parameter objects | held by the engine until the host moves the parameter |
| Output trim | — | Output (dB), default 0 |
| Editor | QuickDraw, portraits | egui: vowel chart (F1/F2), live filter response, drawn portraits |
