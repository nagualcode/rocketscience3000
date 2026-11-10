//! Parameters (original units and ranges, `sjgac…::GetParameter` @ 0x22e7a)
//! and the lock-free status the audio thread publishes for the editor.

use crate::engine::{slider_hz, Controls};
use std::sync::atomic::AtomicU32;
use std::sync::RwLock;
use truce::prelude::*;

#[derive(Params)]
pub struct FolloParams {
    #[param(name = "Analysis", range = "discrete(-12, 12)", default = 0, unit = "dB")]
    pub analysis: IntParam,

    #[param(name = "Bandwidth", range = "discrete(1, 100)", default = 50)]
    pub bandwidth: IntParam,

    #[param(name = "Release", range = "discrete(1, 100)", default = 50, format = "fmt_release")]
    pub release: IntParam,

    /// "Lo Freq Cutoff": where the peak rests at low amplitude (yellow).
    #[param(name = "Low amplitude peak", short_name = "Low", range = "discrete(0, 1080)", default = 250, format = "fmt_hz")]
    pub low: IntParam,

    /// "Hi Freq Cutoff": where the peak goes at high amplitude (blue).
    #[param(name = "High amplitude peak", short_name = "High", range = "discrete(0, 1080)", default = 800, format = "fmt_hz")]
    pub high: IntParam,

    // ---- not in the original ----
    #[param(name = "Output", range = "linear(-24, 12)", default = 0.0, unit = "dB", smooth = "exp(10)")]
    pub output: FloatParam,

    #[meter]
    pub meter_l: MeterSlot,
    #[meter]
    pub meter_r: MeterSlot,

    /// 0 = Modern, 1 = Classic.
    #[persist = "theme"]
    pub theme: RwLock<u8>,

    #[skip]
    pub status: Status,
}

/// Published by the audio thread (f32 bits).
pub struct Status {
    pub sample_rate: AtomicU32,
    /// Centre of the band now (the ball), Hz.
    pub frequency: AtomicU32,
    /// The follower's level.
    pub envelope: AtomicU32,
    pub limiter: AtomicU32,
    /// Controls in force (MIDI CCs can move them), as i32 bits.
    pub low: AtomicU32,
    pub high: AtomicU32,
    pub bandwidth: AtomicU32,
}

impl Default for Status {
    fn default() -> Self {
        Self {
            sample_rate: AtomicU32::new(44100),
            frequency: AtomicU32::new((slider_hz(250) as f32).to_bits()),
            envelope: AtomicU32::new(0),
            limiter: AtomicU32::new(1.0f32.to_bits()),
            low: AtomicU32::new(250),
            high: AtomicU32::new(800),
            bandwidth: AtomicU32::new(50),
        }
    }
}

/// Release knob → seconds for the follower to fall 20 dB.
pub fn release_seconds(r: f64) -> f64 {
    ((r * 1.05f64.powf(r)) / 1.05f64.powf(100.0) / 100.0) * 2.96 + 0.04
}

pub fn hz_label(hz: f64) -> String {
    if hz >= 1000.0 { format!("{:.2} kHz", hz / 1000.0) } else { format!("{hz:.0} Hz") }
}

impl FolloParams {
    pub fn controls(&self) -> Controls {
        Controls {
            bandwidth: self.bandwidth.value() as i32,
            release: self.release.value() as i32,
            low: self.low.value() as i32,
            high: self.high.value() as i32,
            analysis: self.analysis.value() as i32,
        }
    }

    fn fmt_hz(&self, v: f64) -> String {
        hz_label(slider_hz(v.round() as i32))
    }

    fn fmt_release(&self, v: f64) -> String {
        let s = release_seconds(v.round());
        if s >= 1.0 { format!("{s:.2} s") } else { format!("{:.0} ms", s * 1000.0) }
    }
}

pub use FolloParamsParamId as P;
