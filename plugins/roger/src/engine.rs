//! The Roger engine, translated from the i386 slice of `Roger.vst` 3.6.2
//! (`RogerProcess::Process` @ 0x22d10; see `reverse_engineering/roger/`).
//!
//! Four two-pole resonators per channel in parallel — F1, F2, F3 and an
//! octave above F3 — tuned from the vowel table every 128 samples, gliding
//! between vowels (portamento), widened by the bandwidth knob; an "Auto"
//! clock that picks random vowels on the host's beat grid; the Mission
//! look-ahead limiter on the output.

use crate::tables::FORM;
use mission::agc::Agc;
use mission::breakpoint::breakpoint;

/// The controls (`RogerInfo::GetParameter` @ 0x215d4), original ranges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Controls {
    /// 0 "Bandwidth", 1…100.
    pub bandwidth: i32,
    /// 1 "Portamento", 1…100.
    pub portamento: i32,
    /// 2 "Auto Speed", 1…12 (vowels per beat: 1/16 … 16).
    pub auto_speed: i32,
    /// 3 "Width Mode", 4 "Port Mode": 0 knob, 1 modulation wheel, 2 velocity.
    pub width_mode: i32,
    pub port_mode: i32,
    /// 5 "Speed Mode": Auto on/off.
    pub auto: bool,
    /// 6 "Vowel", 0…9 (OO AW UH AH U IR A E I EE).
    pub vowel: i32,
    /// 7 "Family Member": 0 Cindy, 1 Roger, 2 Patty.
    pub family: i32,
    /// 8 "Skip Percentage", 0…100.
    pub skip: i32,
}

impl Default for Controls {
    fn default() -> Self {
        Self { bandwidth: 40, portamento: 40, auto_speed: 4, width_mode: 0, port_mode: 0, auto: true, vowel: 5, family: 0, skip: 10 }
    }
}

/// What the host-time context (`adodg…`) tells `Process`.
#[derive(Clone, Copy, Debug, Default)]
pub struct HostTime {
    /// `GetTempo` → the struct at `this + 0x1a8`; +8 = samples per quarter.
    pub samples_per_quarter: f64,
    /// `GetSamplesToNextBeat`.
    pub samples_to_next_beat: i32,
    /// `GetTransportChanged`.
    pub changed: bool,
}

impl HostTime {
    /// From tempo and position in quarters, like the VST wrapper fills it.
    pub fn new(sample_rate: f64, bpm: f64, ppq: f64, changed: bool) -> Self {
        let spq = sample_rate / (bpm / 60.0);
        let frac = ppq - ppq.trunc();
        Self { samples_per_quarter: spq, samples_to_next_beat: ((1.0 - frac) * spq) as i32, changed }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Mono,
    MonoToStereo,
    Stereo,
}

/// Bandwidth → gain compensation (`local_5c`, 6 breakpoints).
pub const BW_GAIN: [f32; 12] = [1.0, 50.0, 3.0, 10.0, 12.0, 4.0, 23.0, 2.0, 37.0, 0.6, 56.0, 0.3];
/// Velocity → bandwidth / portamento (`ReceiveMidi` @ 0x2255c).
const VELOCITY: [f32; 10] = [0.0, 100.0, 8.0, 100.0, 30.0, 55.0, 100.0, 10.0, 127.0, 0.0];
/// 2π as typed in the source (…3056 for …307179586), as in PeriScope.
const TWO_PI: f64 = 6.2831853056;
/// Formants a fresh instance starts from: Roger's "U" (`Process`, first call).
const START: [f64; 6] = [
    f64::from_bits(0x4084000000000000),
    f64::from_bits(0x3fec8520b0fa8949),
    f64::from_bits(0x4092980000000000),
    f64::from_bits(0x3fd43d13668f2b2c),
    f64::from_bits(0x40a2ac0000000000),
    f64::from_bits(0x3fa6dec56a245871),
];

/// One channel's resonator memory (`this + 0x80 …` / `+ 0xd0 …`).
#[derive(Clone, Copy, Default)]
struct Voice {
    x1: f64,
    y: [[f64; 2]; 4],
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Coefs {
    /// Per resonator: input gain, previous-input gain, feedback 1 and 2.
    pub b: [f64; 4],
    pub c: [f64; 4],
    pub a1: [f64; 4],
    pub a2: [f64; 4],
}

pub struct Roger {
    pub layout: Layout,
    sample_rate: f64,
    voices: [Voice; 2],
    /// Current formants `[F1, A1, F2, A2, F3, A3]` (`*(this + 0x150)`).
    pub formants: [f64; 6],
    /// Portamento increments, kept in single precision between buffers
    /// (`this + 0x154 …`), and the control ticks still to go (`+ 0x178`).
    inc: [f32; 6],
    steps: i32,
    /// Vowel, family and glide time the current glide was planned for.
    last: (i32, i32, i32),
    /// Bandwidth glide: current (`+ 0x16c`), step (`+ 0x120`), target (`+ 0x128`).
    pub bandwidth: f32,
    bw_step: f64,
    bw_target: f64,
    /// Auto: samples to the next vowel (`+ 0x184`), the generator (`+ 0x190`),
    /// last Auto Speed (`+ 0x194`), Auto last buffer (`+ 0x18c`), and the
    /// transport-changed latch (`+ 0x1c0`).
    pub beat: f64,
    seed: u32,
    last_speed: i32,
    was_auto: bool,
    resync: bool,
    agc: Agc,
    /// Resonator outputs for one buffer (`this + 0x130 …`).
    scratch: [[Vec<f32>; 4]; 2],
}

impl Roger {
    /// `InitBuffers` @ 0x22992. `bandwidth` is the parameter's value then
    /// (the glide starts from it).
    pub fn new(sample_rate: f64, layout: Layout, bandwidth: i32) -> Self {
        let ch = if layout == Layout::Mono { 1 } else { 2 };
        Self {
            layout,
            sample_rate,
            voices: [Voice::default(); 2],
            formants: [0.0; 6],
            inc: [0.0; 6],
            steps: 10,
            last: (-1, -1, -1),
            bandwidth: bandwidth as f32,
            bw_step: 0.0,
            bw_target: 0.0,
            beat: f64::from_bits(0x430c6bf526340000), // 1e15: no Auto before the first resync
            // `this + 0x190` and `+ 0x194` are never initialised: operator new
            // memory, zero on a fresh heap (as in the emulation).
            seed: 0,
            last_speed: 0,
            was_auto: false,
            resync: false,
            agc: Agc::limiter(ch, sample_rate),
            scratch: Default::default(),
        }
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    /// Gain reduction of the limiter (1 = none), for the editor.
    pub fn limiter_gain(&self) -> f32 {
        self.agc.gain()
    }

    /// One `Process` call. Mono: `l` in and out. Mono→stereo: input in `l`,
    /// outputs in `l` and `r`. Stereo: both. Auto may change `c.vowel`, as
    /// the original sets its own Vowel parameter.
    pub fn process(&mut self, c: &mut Controls, host: &HostTime, l: &mut [f32], r: &mut [f32]) {
        let n = l.len();
        for ch in &mut self.scratch {
            for s in ch.iter_mut() {
                if s.len() < n {
                    s.resize(n, 0.0);
                }
            }
        }
        if host.changed {
            self.resync = true;
        }
        if self.resync || self.last_speed != c.auto_speed || (c.auto && !self.was_auto) {
            self.last_speed = c.auto_speed;
            self.resync = false;
            self.beat = host.samples_to_next_beat as f64;
        }
        self.was_auto = c.auto;
        let mut beat = self.beat;
        let skip = c.skip as f32 / 100.0;
        let target = c.bandwidth as f64;
        if target != self.bw_target {
            self.bw_step = (target - self.bandwidth as f64) * 0.015625;
        }
        self.bw_target = target;
        if self.formants[0] < 1.0 {
            self.formants = START;
        }
        let mut inc: [f64; 6] = std::array::from_fn(|k| self.inc[k] as f64);
        let spq = host.samples_per_quarter;
        let sr = self.sample_rate as f32 as f64;
        let stereo = self.layout == Layout::Stereo;

        let (mut wet, mut dry) = (1.0f64, 0.0f64);
        let mut co = Coefs::default();
        let mut tick = 0i32;
        for i in 0..n {
            // Auto: a new random vowel on every beat division.
            beat -= 1.0;
            if !(0.0 < beat) {
                self.seed = lcg(self.seed);
                let rnd = ((self.seed >> 16) as f32 * 65536.0 + (self.seed & 0xffff) as f32) * 2.328_306_4e-10;
                if skip <= rnd && c.auto {
                    while (self.seed % 10) as i32 == c.vowel {
                        self.seed = lcg(self.seed);
                    }
                    c.vowel = (self.seed % 10) as i32;
                }
                let period = match c.auto_speed {
                    1 => spq * 16.0,
                    2 => 8.0 * spq,
                    3 => spq * 4.0,
                    4 => spq + spq,
                    5 => spq,
                    6 => 0.5 * spq,
                    7 => spq / 3.0,
                    8 => 0.25 * spq,
                    9 => spq / 6.0,
                    10 => 0.125 * spq,
                    11 => spq / 12.0,
                    12 => 0.0625 * spq,
                    _ => -0.0,
                };
                if (1..=12).contains(&c.auto_speed) {
                    beat = period + beat;
                }
            }

            // Control tick, the first sample of every buffer and every 128th.
            tick -= 1;
            if tick == -1 {
                if (self.bandwidth as f64 - target).abs() > 1.0 {
                    self.bandwidth = (self.bandwidth as f64 + self.bw_step) as f32;
                }
                let (ec, extra) = if self.bandwidth > 56.0 { (56.0f64, self.bandwidth - 56.0) } else { (self.bandwidth as f64, 0.0f32) };
                dry = extra as f64 / 45.0;
                wet = (extra as f64 / -45.0 + 1.0).sqrt();
                let g = breakpoint(ec as i32 as i16, &BW_GAIN) as f64;
                let e5 = ec * 5.0;
                let width = (e5 * 1.01f64.powf(e5)) / 16.217;

                let glide = (sr * 5.0 * (1.07f64.powf(c.portamento as f32 as f64) / 1.07f64.powf(100.0))) as i32;
                let now = (c.vowel, c.family, glide);
                if now != self.last {
                    let to = &FORM[(c.vowel + c.family * 10) as usize];
                    let steps = glide / 128;
                    let s = steps as f64;
                    inc = std::array::from_fn(|k| (to[k] - self.formants[k]) / s);
                    self.steps = steps;
                    self.last = now;
                }
                if self.steps != 0 {
                    let p = &mut self.formants;
                    p[0] += inc[0];
                    p[2] += inc[2];
                    p[4] += inc[4];
                    p[1] += inc[1];
                    p[3] += inc[3];
                    p[5] += inc[5];
                    self.steps -= 1;
                }
                co = coefficients(&self.formants, width, g, sr);
                tick = 0x7f;
            }

            let x = l[i] as f64;
            run(&mut self.voices[0], &co, x, &mut self.scratch[0], i);
            if stereo {
                run(&mut self.voices[1], &co, r[i] as f64, &mut self.scratch[1], i);
            }
        }

        let s = &self.scratch;
        let sum = |ch: usize, i: usize| s[ch][0][i] + s[ch][1][i] + s[ch][2][i] + s[ch][3][i];
        for i in 0..n {
            let x = l[i];
            let out = (sum(0, i) as f64 * wet + x as f64 * dry) as f32;
            l[i] = out;
            match self.layout {
                Layout::Stereo => r[i] = (sum(1, i) as f64 * wet + r[i] as f64 * dry) as f32,
                Layout::MonoToStereo => r[i] = ((x - out) as f64 + x as f64 * dry) as f32,
                Layout::Mono => {}
            }
        }
        if self.layout == Layout::Mono {
            self.agc.process(&mut [&mut l[..]]);
        } else {
            self.agc.process(&mut [&mut l[..], &mut r[..n]]);
        }

        self.inc = std::array::from_fn(|k| inc[k] as f32);
        self.beat = beat;
    }
}

fn lcg(s: u32) -> u32 {
    s.wrapping_mul(0x19660d).wrapping_add(0x3c6ef35f)
}

/// The four resonators' coefficients from the current formants.
pub fn coefficients(p: &[f64; 6], width: f64, g: f64, sr: f64) -> Coefs {
    let (f1, f2, f3) = (p[0], p[2], p[4]);
    let r1 = (-(f1 * 0.005 * width) / sr).exp();
    let a1_1 = (r1 + r1) * ((f1 * TWO_PI) / sr).cos();
    let r2 = (-(f2 * 0.004 * width) / sr).exp();
    let a1_2 = (r2 + r2) * ((f2 * TWO_PI) / sr).cos();
    let r3 = (-(f3 * 0.003 * width) / sr).exp();
    let w3 = f3 * TWO_PI;
    let a1_3 = (r3 + r3) * (w3 / sr).cos();
    let r4 = (-(width * f3 * 0.008) / sr).exp();
    let a1_4 = (r4 + r4) * ((w3 + w3) / sr).cos();
    Coefs {
        b: [
            (1.0 - r1) * g * p[1] * 0.72,
            (1.0 - r2) * g * p[3],
            (1.0 - r3) * g * p[5] * 1.2,
            (1.0 - r4) * g * p[5] * 0.8,
        ],
        c: [g * 0.72 * r1 * (1.0 - r1), r2 * (1.0 - r2) * g, g * 1.2 * r3 * (1.0 - r3), g * 0.8 * r4 * (1.0 - r4)],
        a1: [a1_1, a1_2, a1_3, a1_4],
        a2: [-r1 * r1, -r2 * r2, -r3 * r3, -r4 * r4],
    }
}

/// One sample through a channel's four resonators.
#[inline]
fn run(v: &mut Voice, co: &Coefs, x: f64, out: &mut [Vec<f32>; 4], i: usize) {
    for k in 0..4 {
        let y = (co.b[k] * x + v.x1 * co.c[k] + v.y[k][0] * co.a1[k] + v.y[k][1] * co.a2[k]) as f32;
        out[k][i] = y;
        v.y[k][1] = v.y[k][0];
        v.y[k][0] = y as f64;
    }
    v.x1 = x;
}

/// `RogerProcess::ReceiveMidi` @ 0x2255c: notes pick vowel and family member,
/// the modulation wheel or the velocity drive bandwidth / portamento when
/// their mode says so.
pub fn receive_midi(c: &mut Controls, status: u8, d1: u8, d2: u8) {
    let kind = status & 0xf0;
    let note_on = kind == 0x90;
    let on = d2 != 0;
    if note_on && on {
        c.vowel = match d1 % 12 {
            0 | 1 => 0,
            2 => 1,
            3 => 2,
            4 => 3,
            5 => 4,
            6 => 5,
            7 | 8 => 6,
            9 => 7,
            10 => 8,
            _ => 9,
        };
        c.family = ((d1 / 12) % 3) as i32;
    }
    let wheel = |v: u8| ((99.0f32 / 127.0) * v as f32 + 1.0) as i32;
    let velocity = |v: u8| breakpoint(v as i16, &VELOCITY) as i32;
    if c.width_mode == 1 {
        if d1 == 1 && kind == 0xb0 {
            c.bandwidth = wheel(d2).clamp(1, 100);
        }
    } else if c.width_mode == 2 && note_on && on {
        c.bandwidth = velocity(d2).clamp(1, 100);
    }
    if c.port_mode == 1 {
        if d1 == 1 && kind == 0xb0 {
            c.portamento = wheel(d2).clamp(1, 100);
        }
    } else if c.port_mode == 2 && note_on && on {
        c.portamento = velocity(d2).clamp(1, 100);
    }
}

/// The filter's gain at `omega` (radians per sample), for the editor:
/// |Σ (b + c·z⁻¹) / (1 − a1·z⁻¹ − a2·z⁻²)|.
pub fn response(co: &Coefs, omega: f64) -> f64 {
    let (c1, s1) = (omega.cos(), -omega.sin());
    let (c2, s2) = ((2.0 * omega).cos(), -(2.0 * omega).sin());
    let (mut re, mut im) = (0.0, 0.0);
    for k in 0..4 {
        let (nr, ni) = (co.b[k] + co.c[k] * c1, co.c[k] * s1);
        let (dr, di) = (1.0 - co.a1[k] * c1 - co.a2[k] * c2, -co.a1[k] * s1 - co.a2[k] * s2);
        let d = dr * dr + di * di;
        re += (nr * dr + ni * di) / d;
        im += (ni * dr - nr * di) / d;
    }
    (re * re + im * im).sqrt()
}

/// Coefficients for a bandwidth setting, as a control tick computes them.
pub fn coefficients_for(formants: &[f64; 6], bandwidth: f32, sample_rate: f64) -> (Coefs, f64, f64) {
    let (ec, extra) = if bandwidth > 56.0 { (56.0f64, bandwidth - 56.0) } else { (bandwidth as f64, 0.0f32) };
    let g = breakpoint(ec as i32 as i16, &BW_GAIN) as f64;
    let e5 = ec * 5.0;
    let width = (e5 * 1.01f64.powf(e5)) / 16.217;
    let wet = (extra as f64 / -45.0 + 1.0).sqrt();
    (coefficients(formants, width, g, sample_rate as f32 as f64), wet, extra as f64 / 45.0)
}
