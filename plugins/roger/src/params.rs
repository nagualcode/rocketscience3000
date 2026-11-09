//! Parameters (original units and ranges, `RogerInfo::GetParameter` @
//! 0x215d4) and the lock-free status the audio thread publishes for the
//! editor.

use crate::engine::Controls;
use std::sync::atomic::{AtomicI32, AtomicU32};
use std::sync::RwLock;
use truce::prelude::*;

/// `RogerVowelParm`: the vowel bar, left to right.
#[derive(ParamEnum, Default, Debug)]
pub enum Vowel {
    #[name = "OO (boot)"]
    Oo,
    #[name = "AW (law)"]
    Aw,
    #[name = "UH (duh)"]
    Uh,
    #[name = "AH (utah)"]
    Ah,
    #[name = "U (but)"]
    U,
    #[default]
    #[name = "IR (bird)"]
    Ir,
    #[name = "A (bat)"]
    A,
    #[name = "E (bet)"]
    E,
    #[name = "I (bit)"]
    I,
    #[name = "EE (beet)"]
    Ee,
}

/// `RogerFamilyMemberParm`.
#[derive(ParamEnum, Default, Debug)]
pub enum Family {
    #[default]
    #[name = "Cindy"]
    Cindy,
    #[name = "Roger"]
    Roger,
    #[name = "Patty"]
    Patty,
}

/// `RogerAutoSpeedParm`: vowels per beat (values 1…12 in the original).
#[derive(ParamEnum, Default, Debug)]
pub enum AutoSpeed {
    #[name = "1/16"]
    S1,
    #[name = "1/8"]
    S2,
    #[name = "1/4"]
    S3,
    #[default]
    #[name = "1/2"]
    S4,
    #[name = "1"]
    S5,
    #[name = "2"]
    S6,
    #[name = "3"]
    S7,
    #[name = "4"]
    S8,
    #[name = "6"]
    S9,
    #[name = "8"]
    S10,
    #[name = "12"]
    S11,
    #[name = "16"]
    S12,
}

/// `RogerKnobModeParm`: what drives bandwidth / portamento.
#[derive(ParamEnum, Default, Debug)]
pub enum KnobMode {
    #[default]
    #[name = "Knob"]
    Knob,
    #[name = "Modulation wheel"]
    Wheel,
    #[name = "Velocity"]
    Velocity,
}

#[derive(Params)]
pub struct RogerParams {
    #[param(name = "Bandwidth", range = "discrete(1, 100)", default = 40)]
    pub bandwidth: IntParam,

    #[param(name = "Portamento", range = "discrete(1, 100)", default = 40, format = "fmt_portamento")]
    pub portamento: IntParam,

    #[param(name = "Auto Speed", short_name = "Tempo", default = 3)]
    pub auto_speed: EnumParam<AutoSpeed>,

    #[param(name = "Width Mode")]
    pub width_mode: EnumParam<KnobMode>,

    #[param(name = "Port Mode")]
    pub port_mode: EnumParam<KnobMode>,

    /// "Speed Mode" in the original: Auto on/off — on by default.
    #[param(name = "Auto", default = 1)]
    pub auto: BoolParam,

    #[param(name = "Vowel", default = 5)]
    pub vowel: EnumParam<Vowel>,

    #[param(name = "Family Member", short_name = "Family")]
    pub family: EnumParam<Family>,

    #[param(name = "Skip Percentage", short_name = "Skips", range = "discrete(0, 100)", default = 10, format = "fmt_percent")]
    pub skip: IntParam,

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

/// Published by the audio thread (f32 bits in the u32s).
pub struct Status {
    pub sample_rate: AtomicU32,
    /// Current formants F1, A1, F2, A2, F3, A3 (gliding).
    pub formants: [AtomicU32; 6],
    /// Bandwidth in force (it glides too).
    pub bandwidth: AtomicU32,
    /// Vowel and family member in force (Auto and MIDI change them).
    pub vowel: AtomicI32,
    pub family: AtomicI32,
    pub limiter: AtomicU32,
    /// Bumped on every Auto vowel, for the editor's flash.
    pub beats: AtomicU32,
}

impl Default for Status {
    fn default() -> Self {
        Self {
            sample_rate: AtomicU32::new(44100),
            formants: std::array::from_fn(|_| AtomicU32::new(0)),
            bandwidth: AtomicU32::new(40.0f32.to_bits()),
            vowel: AtomicI32::new(5),
            family: AtomicI32::new(0),
            limiter: AtomicU32::new(1.0f32.to_bits()),
            beats: AtomicU32::new(0),
        }
    }
}

/// Portamento → glide time: 5 s · 1.07^(p − 100).
pub fn glide_seconds(p: f64) -> f64 {
    5.0 * 1.07f64.powf(p - 100.0)
}

impl RogerParams {
    pub fn controls(&self) -> Controls {
        Controls {
            bandwidth: self.bandwidth.value() as i32,
            portamento: self.portamento.value() as i32,
            auto_speed: self.auto_speed.index() as i32 + 1,
            width_mode: self.width_mode.index() as i32,
            port_mode: self.port_mode.index() as i32,
            auto: self.auto.value(),
            vowel: self.vowel.index() as i32,
            family: self.family.index() as i32,
            skip: self.skip.value() as i32,
        }
    }

    fn fmt_percent(&self, v: f64) -> String {
        format!("{v:.0} %")
    }

    fn fmt_portamento(&self, v: f64) -> String {
        let s = glide_seconds(v.round());
        if s >= 1.0 { format!("{s:.2} s") } else { format!("{:.0} ms", s * 1000.0) }
    }
}

pub use RogerParamsParamId as P;
