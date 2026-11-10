//! The Follo engine, translated from the i386 slice of `Follo.vst` 3.6.2
//! (`aihjf…::Process` @ 0x21426; see `reverse_engineering/follo/`).
//!
//! A peak follower (instant attack, release from the Release knob) moves the
//! centre of one resonant band-pass between the Lo (yellow) and Hi (blue)
//! frequencies; coefficients every 50 samples; the Mission look-ahead
//! limiter on the output.

use mission::agc::Agc;
use mission::breakpoint::breakpoint;

/// The controls (`sjgac…::GetParameter` @ 0x22e7a), original ranges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Controls {
    /// 0 "Bandwidth", 1…100.
    pub bandwidth: i32,
    /// 1 "Release", 1…100.
    pub release: i32,
    /// 2 "Lo Freq Cutoff", 3 "Hi Freq Cutoff": 0…1080, tenths of a
    /// semitone above 20 Hz (the yellow and the blue slider).
    pub low: i32,
    pub high: i32,
    /// 5 "Analysis": input sensitivity, −12…12 dB.
    pub analysis: i32,
}

impl Default for Controls {
    fn default() -> Self {
        Self { bandwidth: 50, release: 50, low: 250, high: 800, analysis: 0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Mono,
    MonoToStereo,
    Stereo,
}

/// Bandwidth knob → band width in semitones (`local_54`).
pub const BW_SEMITONES: [f32; 10] = [1.0, 0.07, 25.0, 0.2, 50.0, 2.5, 90.0, 12.0, 100.0, 400000.0];
/// The sliders' semitone, typed as 1.059463 (the bandwidth uses the exact 2^(1/12)).
const SLIDER_SEMITONE: f64 = 1.059463;
const SEMITONE: f64 = f64::from_bits(0x3ff0f38f92d97963);
/// π and 2π as typed in the source (…6528 and …3056).
const PI: f64 = 3.1415926528;
const TWO_PI: f64 = 6.2831853056;

/// Slider position → Hz.
pub fn slider_hz(pos: i32) -> f64 {
    SLIDER_SEMITONE.powf(pos as f64 / 10.0) * 20.0
}

/// Release knob → per-sample decay of the follower (−20 dB in
/// `0.04 + 2.96 · r·1.05^r / 1.05^100 / 100` seconds).
pub fn release_coef(release: i32, sample_rate: f64) -> f64 {
    let r = release as f64;
    let t = r * 1.05f64.powf(r);
    let full = 1.05f64.powf(100.0);
    0.1f64.powf(1.0 / ((((t / full) / 100.0) * 2.96 + 0.04) * sample_rate))
}

/// One band-pass (the coefficients of `Process`).
#[derive(Clone, Copy, Default, Debug)]
pub struct Coefs {
    pub b0: f64,
    pub b1: f64,
    pub a1: f64,
    pub a2: f64,
}

/// Coefficients for a centre frequency and a band width in semitones.
pub fn coefficients(f: f64, semitones: f32, sample_rate: f64) -> Coefs {
    let q = SEMITONE.powf(semitones as f64);
    let bw = f - f / q;
    let r = (((bw + bw) * -PI) / sample_rate).exp();
    let w = f * (TWO_PI / sample_rate);
    let (s, c) = (w.sin(), w.cos());
    let a2 = r * -r;
    Coefs { b0: s * 1.1, b1: s * a2, a1: (r + r) * c, a2 }
}

#[derive(Clone, Copy, Default)]
struct Voice {
    x1: f32,
    y1: f32,
    y2: f32,
}

impl Voice {
    #[inline]
    fn run(&mut self, co: &Coefs, x: f32) -> f32 {
        let y = (x as f64 * co.b0 + self.x1 as f64 * co.b1 + self.y1 as f64 * co.a1 + self.y2 as f64 * co.a2) as f32;
        self.y2 = self.y1;
        self.y1 = y;
        self.x1 = x;
        y
    }
}

pub struct Follo {
    pub layout: Layout,
    sample_rate: f64,
    voices: [Voice; 2],
    /// The follower (`this + 0x44`, single precision between buffers).
    pub envelope: f32,
    /// Centre frequency of the last coefficient update ("Mean display").
    pub frequency: f64,
    agc: Agc,
}

impl Follo {
    /// `InitBuffers` @ 0x22020.
    pub fn new(sample_rate: f64, layout: Layout) -> Self {
        let ch = if layout == Layout::Mono { 1 } else { 2 };
        Self {
            layout,
            sample_rate,
            voices: [Voice::default(); 2],
            envelope: 0.0,
            frequency: 0.0,
            agc: Agc::limiter(ch, sample_rate),
        }
    }

    pub fn limiter_gain(&self) -> f32 {
        self.agc.gain()
    }

    /// One `Process` call. Mono: `l` in and out. Mono→stereo: input in `l`,
    /// band in `l`, residue in `r`. Stereo: both.
    pub fn process(&mut self, c: &Controls, l: &mut [f32], r: &mut [f32]) {
        let n = l.len();
        let lo = slider_hz(c.low);
        let hi = slider_hz(c.high);
        let semis = breakpoint(c.bandwidth as i16, &BW_SEMITONES);
        let gain = 10f64.powf(c.analysis.clamp(-12, 12) as f64 / 20.0);
        let rel = release_coef(c.release, self.sample_rate);
        let span = hi - lo;
        let sr = self.sample_rate;
        let mut env = self.envelope as f64;
        let mut co = Coefs::default();
        let mut f = self.frequency;
        let m2s = self.layout == Layout::MonoToStereo;
        let stereo = self.layout == Layout::Stereo;
        for i in 0..n {
            let x = l[i];
            let mut a = x.abs() as f64;
            if stereo && a < r[i].abs() as f64 {
                a = r[i].abs() as f64;
            }
            let e = if m2s { a * gain } else { gain * a };
            if env <= e {
                env = e;
            }
            env *= rel;
            if i % 50 == 0 {
                f = hi;
                if env <= 1.0 {
                    // Mono→stereo follows the level; the others its square.
                    f = if m2s { lo + span * env } else { lo + span * env * env };
                }
                co = coefficients(f, semis, sr);
            }
            let y = self.voices[0].run(&co, x);
            l[i] = y;
            if m2s {
                r[i] = x - y;
            } else if stereo {
                r[i] = self.voices[1].run(&co, r[i]);
            }
        }
        self.envelope = env as f32;
        self.frequency = f;
        if self.layout == Layout::Mono {
            self.agc.process(&mut [&mut l[..]]);
        } else {
            self.agc.process(&mut [&mut l[..], &mut r[..n]]);
        }
    }
}

/// Normalised value → parameter (`ayjxq…::SetValueNormalized` @ 0xd5c8):
/// a power curve that puts 0.5 on the parameter's centre, then truncated.
pub fn from_normalized(n: f32, min: i32, max: i32, center: i32) -> i32 {
    let (lo, hi, c) = (min as f32, max as f32, center as f32);
    let p = 1.0 / ((0.5f64.ln() / (((c - lo) / (hi - lo)) as f64).ln()) as f32) as f64;
    let y = (n as f64).powf(p) as f32;
    (lo + y * (hi - lo)) as i32
}

/// `ReceiveMidi` @ 0x21346: CC 11 analysis, 12 bandwidth, 13 release,
/// 14 low slider, 15 high slider (value / 128, through the centre curve).
pub fn receive_midi(c: &mut Controls, status: u8, d1: u8, d2: u8) {
    if status & 0xf0 != 0xb0 {
        return;
    }
    let n = (d2 as f64 * 0.0078125) as f32;
    match d1 {
        11 => c.analysis = from_normalized(n, -12, 12, 0),
        12 => c.bandwidth = from_normalized(n, 1, 100, 50),
        13 => c.release = from_normalized(n, 1, 100, 50),
        14 => c.low = from_normalized(n, 0, 1080, 540),
        15 => c.high = from_normalized(n, 0, 1080, 540),
        _ => {}
    }
}

/// |H(e^jω)| of the band-pass (b0 + b1·z⁻¹) / (1 − a1·z⁻¹ − a2·z⁻²), for the editor.
pub fn response(co: &Coefs, omega: f64) -> f64 {
    let (c1, s1) = (omega.cos(), -omega.sin());
    let (c2, s2) = ((2.0 * omega).cos(), -(2.0 * omega).sin());
    let (nr, ni) = (co.b0 + co.b1 * c1, co.b1 * s1);
    let (dr, di) = (1.0 - co.a1 * c1 - co.a2 * c2, -co.a1 * s1 - co.a2 * s2);
    ((nr * nr + ni * ni) / (dr * dr + di * di)).sqrt()
}

/// Slider position of a frequency (inverse of [`slider_hz`]).
pub fn hz_slider(hz: f64) -> f64 {
    (hz / 20.0).ln() / SLIDER_SEMITONE.ln() * 10.0
}
