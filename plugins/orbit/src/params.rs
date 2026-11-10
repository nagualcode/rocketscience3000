//! Parameters (original units and ranges, `OrbitInfo::GetParameter` @
//! 0x2398a) and the lock-free status the audio thread publishes.

use crate::engine::Controls;
use std::sync::atomic::AtomicU32;
use std::sync::RwLock;
use truce::prelude::*;

/// `OrbitModeParm` 1…5: how the source moves.
#[derive(ParamEnum, Default, Debug)]
pub enum Motion {
    #[name = "Static"]
    Static,
    #[name = "Follow clicks"]
    Follow,
    #[default]
    #[name = "Ellipse"]
    Ellipse,
    #[name = "Ellipse backwards"]
    Backwards,
    #[name = "Buck Rogers on LSD"]
    Random,
}

#[derive(Params)]
pub struct OrbitParams {
    #[param(name = "Room Size", short_name = "Room", range = "linear(8, 80)", default = 25.0, format = "fmt_metres")]
    pub room: FloatParam,

    #[param(name = "Reflection Order", short_name = "Order", range = "discrete(0, 5)", default = 2)]
    pub order: IntParam,

    /// "Inner Room X / Y Pos": where the listener stands, −1…1.
    #[param(name = "Listener X", range = "linear(-1, 1)", default = 0.0)]
    pub inner_x: FloatParam,
    #[param(name = "Listener Y", range = "linear(-1, 1)", default = -0.6)]
    pub inner_y: FloatParam,

    #[param(name = "High Damp", short_name = "Damp", range = "discrete(0, 100)", default = 0, format = "fmt_percent")]
    pub damp: IntParam,

    #[param(name = "Speed", range = "linear(0, 99)", default = 3.0, format = "fmt_speed")]
    pub speed: FloatParam,

    #[param(name = "Mode", default = 2)]
    pub mode: EnumParam<Motion>,

    #[param(name = "Ellipse radius (horizontal)", short_name = "Radius X", range = "linear(0, 1)", default = 0.8)]
    pub rad_x: FloatParam,
    #[param(name = "Ellipse radius (vertical)", short_name = "Radius Y", range = "linear(0, 1)", default = 0.8)]
    pub rad_y: FloatParam,
    #[param(name = "Ellipse position (horizontal)", short_name = "Ellipse X", range = "linear(-1, 1)", default = 0.0)]
    pub pos_x: FloatParam,
    #[param(name = "Ellipse position (vertical)", short_name = "Ellipse Y", range = "linear(-1, 1)", default = 0.5)]
    pub pos_y: FloatParam,

    #[param(name = "Speaker span", short_name = "Span", range = "linear(0.2, 6)", default = 3.0, format = "fmt_metres")]
    pub span: FloatParam,

    /// "X / Y Position": where the source goes in Static and Follow modes.
    #[param(name = "X Position", short_name = "X", range = "linear(-1, 1)", default = 0.0)]
    pub x: FloatParam,
    #[param(name = "Y Position", short_name = "Y", range = "linear(-1, 1)", default = 0.0)]
    pub y: FloatParam,

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
    /// The source now, −1…1 ("Source X / Y").
    pub source_x: AtomicU32,
    pub source_y: AtomicU32,
    /// Geometry in force (MIDI can move it): room, listener, span.
    pub room: AtomicU32,
    pub inner_x: AtomicU32,
    pub inner_y: AtomicU32,
    pub span: AtomicU32,
    pub order: AtomicU32,
}

impl Default for Status {
    fn default() -> Self {
        let f = |v: f32| AtomicU32::new(v.to_bits());
        Self {
            sample_rate: AtomicU32::new(44100),
            source_x: f(0.0),
            source_y: f(0.0),
            room: f(25.0),
            inner_x: f(0.0),
            inner_y: f(-0.6),
            span: f(3.0),
            order: AtomicU32::new(2),
        }
    }
}

impl OrbitParams {
    pub fn controls(&self) -> Controls {
        Controls {
            room: self.room.value(),
            order: self.order.value() as i32,
            inner_y: self.inner_y.value(),
            inner_x: self.inner_x.value(),
            damp: self.damp.value() as i32,
            speed: self.speed.value(),
            mode: self.mode.index() as i32 + 1,
            rad_x: self.rad_x.value(),
            rad_y: self.rad_y.value(),
            pos_x: self.pos_x.value(),
            pos_y: self.pos_y.value(),
            span: self.span.value(),
            x: self.x.value(),
            y: self.y.value(),
        }
    }

    fn fmt_percent(&self, v: f64) -> String {
        format!("{v:.0} %")
    }

    fn fmt_metres(&self, v: f64) -> String {
        format!("{v:.1} m")
    }

    fn fmt_speed(&self, v: f64) -> String {
        format!("{v:.1} m/s")
    }
}

pub use OrbitParamsParamId as P;
