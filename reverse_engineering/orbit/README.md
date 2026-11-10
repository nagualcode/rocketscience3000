# Reverse engineering Orbit

How **Orbit**, the "psycho-acoustic flight path simulator" of Audio Ease's
Rocket Science bundle (2001–2013; interface by Oskar Luyer), was recovered
from the original machine code, and how `plugins/orbit/src/engine.rs` was
proven **bit-identical** to it. Method and tools as for
[Roger](../roger/README.md) and [Follo](../follo/README.md).

| Path | What it is |
|---|---|
| `original_binaries/Orbit-3.6.2-VST-i386` | i386 slice of `Orbit.vst` 3.6.2 — the reference |
| `original_binaries/Orbit-3.6.2-VST-ppc` | its PowerPC slice |
| `decompiled/orbit_i386.c`, `symbols.txt` | Ghidra output and symbols |
| `tools/emulate_orbit.py` | runs the original `InitBuffers` / `Process` / `ReceiveMidi` under Unicorn |
| `tools/differential_test.py` | 39 scenarios, original vs Rust, bit for bit |
| `ui/` | the original panel (PICT 4000), knob strip, about box |

## 1. Map

Orbit's classes kept their names — and some of them are Dutch:

| Symbol | Role |
|---|---|
| `OrbitProcess` | ctor @ 0x251a8 (0xd0 bytes, virtual inheritance), `InitBuffers` @ 0x25926, `Process` @ 0x263da, `MoveSource` @ 0x25d2a, `ReceiveMidi` @ 0x25620 |
| `OrbitInfo` | `GetParameter` @ 0x2398a (17 parameters), `GetPrivateParameter` (Source X/Y "Attempt"), 2 variants: mono→stereo, stereo (summed) → stereo |
| `SetMooreParms`, `MakePhantomSourceList`, `CalcPhantomSourceCoordinates`, `CalcTaps`, `KillAbsorbedPS` | the room model |
| `OrbitCBRightLooking` | the multi-tapped delay line: `PutMultiTappedStereo` @ 0x21f5a (Doppler resampling), `GetMoore` |
| `ManMetHond` ("man with dog") | the path follower: `DaarIsHond` ("there's a dog" — a new target), `WaarIsMan` ("where's the man"), `Aai` ("pat" — stand still), `ClearHansworst` ("clear the clown"), `SetEtappeDuur` (leg duration) |

## 2. Parameters

| Id | Name | Range | Default | MIDI |
|---|---|---|---|---|
| 0 / 1 | Source X / Y (not automatable; written by the motion) | −1…1 | 0 | |
| 2 | Room Size | 8…80 m | 25 | CC 11 |
| 3 | Reflection Order | 0…5 | 2 | |
| 4 / 5 | Inner Room Y / X Pos (the listener) | −1…1 | −0.6 / 0 | CC 12 / 13 |
| 6 | High Damp | 0…100 | 0 | CC 14 |
| 7 | Speed | 0…99 m/s | 3 | CC 15 |
| 8 | Mode: 1 static, 2 follow, 3 ellipse, 4 ellipse backwards, 5 random | 1…5 | 3 | program change (`p mod 5 + 1`) |
| 9 / 10 | ellipse radius (horizontal / vertical) | 0…1 | 0.8 | CC 16 / 17 |
| 11 / 12 | ellipse position (horizontal / vertical) | −1…1 | 0 / 0.5 | CC 18 / 19 |
| 13 | Speaker span | 0.2…6 m | 3 | CC 20 |
| 15 / 16 | X / Y Position (where to go) | −1…1 | 0 | notes / velocity, pitch bend (X), CC 1 (Y) |

CCs: `min + value/128·range`. Notes: X from `(note − 36) mod 60` (C2 = left
wall, five octaves across) and Y from the velocity. A value outside a
parameter's range is **ignored**, not clamped (notes below 36 do nothing).

## 3. The room model (F. R. Moore, 1983)

`SetMooreParms` is named after F. Richard Moore's "A general model for
spatial processing of sounds" (*Computer Music Journal* 7/3, 1983): image
sources in a room, each heard with its own delay, attenuation and Doppler
shift.

* The **outer room** is a square of side *room size*; positions −1…1 map to
  0…*room* metres. The **listener** has two ears (the "speakers") a *speaker
  span* apart at `(x ∓ span/2, y + span/2)`.
* **Image sources**: the source mirrored in the walls, orders 1…5 (always
  computed to 5; `CalcPhantomSourceCoordinates` builds the 4k images of order
  k in four diamond-shaped sweeps, the mirror flip alternating). 61 sources.
* **Taps** (`CalcTaps`): for each image and ear, `d = |image − ear| + 1` (the
  distance in single precision), delay `d/340 s`, gain `d^−1.2`, times
  `e^(−0.1)` = 0.905 per reflection (`exp(2/−20)` — a dB-to-gain conversion
  with `exp` in place of `pow(10, …)`), times 5.
* **Head shadow** (`KillAbsorbedPS`): an image behind the ears (lower y) and
  on the far side of an ear is halved for that ear.
* The **reflection order** parameter only sets how many of the 61 taps are
  used: 1, 5, 13, 25, 41, 61.
* **High Damp**: the reflections (not the direct sound) go through a
  one-pole low-pass `y = a·x − (a−1)·y[n−1]`, `a = ((100 − damp)/100)²`
  computed in double.

## 4. Doppler: the multi-tapped delay line

Each `Process`, unless the mode is static and nothing moved, new tap lists
are computed (two pairs of lists used alternately). Then for every tap the
input block of n samples is **resampled to `n + Δdelay` samples** while its
gain glides from the old to the new value, and added into the left / right
ring at the old delay. A source coming closer is squeezed (pitch up), one
moving away stretched (pitch down) — Doppler, and the reflections each get
their own. The resampler computes one read position and one pair of weights
per **two** output samples, the weights in double and rounded to float.

**Quirk — reading past the block.** The read index can run up to two samples
beyond the n input samples; the original then reads whatever follows the
buffer in the heap (in the emulation: the next block's pointer table, about
1e-29 as a float). The port reads zeros, and the emulator gives its
allocations a zeroed guard band so the comparison is meaningful.

The rings hold `3n + n·(int)(80·6·sr/340/n + 5)` samples: enough for 480 m.
An 80 m room at order 5 has images further away than that, and their writes
wrap around the ring — kept as is (and identical).

There is no limiter and no latency.

## 5. MoveSource: the five motions

* **Static**: the source sits at X/Y Position.
* **Follow**: each new X/Y target is queued (20 at most) for the man with
  his dog; he walks a cosine-eased, corner-cutting path through them, one leg
  per `(room/2)/speed` seconds.
* **Ellipse / backwards**: an angle advances by `speed/(max radius · room)`
  radians per second (at most π per buffer), wrapped with the mistyped
  2π = 6.2831853056; position `centre + radius·(cos, sin)` in double.
* **Random** ("like Buck Rogers on LSD", the manual): every buffer the
  target queue is cleared and a new random point inside the ellipse is
  given; the man is always half-way to somewhere else. Two steps of the
  *Numerical Recipes* generator per buffer (seed never initialised: 0).
* The source parameters ignore values outside −1…1, so an ellipse that
  crosses a wall stops the source at the wall.

Ghidra showed most of this arithmetic as single precision; the asm does it
in double (speed `·10·0.1`, angle wrap, positions, the random point).

## 6. Verification

```
39 scenarios — mono→stereo and stereo, reflection orders 0…5, rooms 8–80 m,
damping, speaker spans, the listener moved, all five motions (fast, backwards,
outside the room, zero radius, random), mode switches, automation, MIDI (CCs,
notes, pitch bend, program changes), blocks 64…1024 and a buffer larger than
the block, 768 000-sample runs, 8 / 22.05 / 44.1 / 48 / 96 kHz:
39 identical, 0 different
```

## 7. Mac OS 9 (v2.04)

`reference/MacOS9/Orbit™` shares 340 m/s, both mistyped π constants, the
distance exponent 1.2, and the 4294967295 of the random flight; it has no
`exp(2/−20)` constant, so its reflection attenuation was computed differently.

## 8. What the port changes

| | Original 3.6.2 | Port |
|---|---|---|
| Engine output | — | bit-identical (§6) |
| Reads past the block | heap contents | zeros |
| MIDI | written into the parameter objects | held by the engine until the host moves the parameter |
| Output trim | — | Output (dB), default 0 |
| Editor | QuickDraw room | the room from above with image sources, trail, ellipse; drag the source target and the listener |
