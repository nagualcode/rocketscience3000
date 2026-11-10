//! Orbit 3K — the "psycho-acoustic flight path simulator" of Audio Ease's
//! Rocket Science bundle (2001), rebuilt from the original binary.
//!
//! The engine (`engine.rs`) is a translation of the i386 slice of
//! `Orbit.vst` 3.6.2 and renders bit-identical output to that machine code
//! (see `reverse_engineering/orbit/`).

pub mod engine;
pub mod params;
pub mod ui;

use engine::{receive_midi, Controls, Layout, Orbit};
use mission::denormal::DenormalFlush;
use params::{OrbitParams, P};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use truce::prelude::*;
use truce_egui::EguiEditor;

pub struct OrbitDsp {
    engine: Box<Orbit>,
    sample_rate: f64,
    buffer: usize,
    /// Controls in force: MIDI writes into them; host moves override.
    controls: Controls,
    seen: Controls,
    l: Vec<f32>,
    r: Vec<f32>,
}

impl OrbitDsp {
    fn new(sample_rate: f64, layout: Layout, params: &OrbitParams, buffer: usize) -> Self {
        let c = params.controls();
        Self {
            engine: Box::new(Orbit::new(sample_rate, layout, buffer, &c, (0.0, 0.0))),
            sample_rate,
            buffer,
            controls: c,
            seen: c,
            l: vec![0.0; buffer],
            r: vec![0.0; buffer],
        }
    }

    fn follow(&mut self, p: Controls) {
        let (c, s) = (&mut self.controls, &self.seen);
        macro_rules! take {
            ($($f:ident),*) => { $( if p.$f != s.$f { c.$f = p.$f; } )* };
        }
        take!(room, order, inner_y, inner_x, damp, speed, mode, rad_x, rad_y, pos_x, pos_y, span, x, y);
        self.seen = p;
    }
}

impl Default for OrbitDsp {
    fn default() -> Self {
        OrbitDsp::new(44100.0, Layout::MonoToStereo, &OrbitParams::new(), 512)
    }
}

/// MIDI as raw bytes for `OrbitProcess::ReceiveMidi`.
fn raw_midi(ev: &EventBody) -> Option<(u8, u8, u8)> {
    match *ev {
        EventBody::NoteOn { note, velocity, .. } => Some((0x90, note, velocity)),
        EventBody::ControlChange { cc, value, .. } => Some((0xb0, cc, value)),
        EventBody::PitchBend { value, .. } => Some((0xe0, (value & 0x7f) as u8, (value >> 7 & 0x7f) as u8)),
        EventBody::ProgramChange { program, .. } => Some((0xc0, program, 0)),
        _ => None,
    }
}

pub struct OrbitPlugin;

impl PluginLogic for OrbitPlugin {
    type Params = OrbitParams;
    type DspState = OrbitDsp;

    fn bus_layouts() -> Vec<BusLayout> {
        vec![
            BusLayout::new().with_input("Main", ChannelConfig::Mono).with_output("Main", ChannelConfig::Stereo),
            BusLayout::stereo(),
        ]
    }

    fn init(params: &Self::Params, _cx: &InitContext) -> Self::DspState {
        OrbitDsp::new(44100.0, Layout::MonoToStereo, params, 512)
    }

    fn reset(state: &mut Self::DspState, params: &Self::Params, config: &AudioConfig) {
        *state = OrbitDsp::new(config.sample_rate, state.engine.layout, params, config.max_block_size.max(32));
        params.status.sample_rate.store(config.sample_rate as u32, Ordering::Relaxed);
    }

    fn process(
        state: &mut Self::DspState,
        params: &Self::Params,
        buffer: &mut AudioBuffer,
        events: &EventList,
        context: &mut ProcessContext,
    ) -> ProcessStatus {
        let n = buffer.num_samples();
        if n == 0 {
            return ProcessStatus::Normal;
        }
        let _ftz = DenormalFlush::on();
        let nin = buffer.num_input_channels();
        let nout = buffer.num_output_channels();
        let layout = if nin > 1 { Layout::Stereo } else { Layout::MonoToStereo };
        if layout != state.engine.layout || n > state.buffer {
            let (sr, b) = (state.sample_rate, state.buffer.max(n));
            *state = OrbitDsp::new(sr, layout, params, b);
        }
        state.follow(params.controls());
        for ev in events.iter() {
            if let Some((s, d1, d2)) = raw_midi(&ev.body) {
                receive_midi(&mut state.controls, s, d1, d2);
            }
        }
        let (l, r) = (&mut state.l[..n], &mut state.r[..n]);
        let a = if nin > 0 { buffer.input(0) } else { &[][..] };
        for i in 0..n {
            l[i] = a.get(i).copied().unwrap_or(0.0);
        }
        if nin > 1 {
            r.copy_from_slice(buffer.input(1));
        } else {
            r.iter_mut().for_each(|v| *v = 0.0);
        }
        state.engine.process(&state.controls, l, r);

        let g = 10f32.powf(params.output.read() / 20.0);
        for i in 0..n {
            buffer.output(0)[i] = l[i] * g;
            if nout > 1 {
                buffer.output(1)[i] = r[i] * g;
            }
        }
        let st = &params.status;
        let c = &state.controls;
        let src = state.engine.source;
        st.source_x.store(src.0.to_bits(), Ordering::Relaxed);
        st.source_y.store(src.1.to_bits(), Ordering::Relaxed);
        st.room.store(c.room.to_bits(), Ordering::Relaxed);
        st.inner_x.store(c.inner_x.to_bits(), Ordering::Relaxed);
        st.inner_y.store(c.inner_y.to_bits(), Ordering::Relaxed);
        st.span.store(c.span.to_bits(), Ordering::Relaxed);
        st.order.store(c.order as u32, Ordering::Relaxed);
        context.set_meter(P::MeterL, buffer.output_peak(0));
        context.set_meter(P::MeterR, buffer.output_peak(if nout > 1 { 1 } else { 0 }));
        ProcessStatus::Normal
    }

    fn editor(params: Arc<Self::Params>) -> Box<dyn Editor> {
        EguiEditor::new(params.clone(), (ui::WINDOW_W, ui::WINDOW_H), ui::orbit_ui)
            .with_visuals(truce_egui::theme::dark())
            .resizable(true)
            .min_size((ui::MIN_W, ui::MIN_H))
            .max_size((ui::MAX_W, ui::MAX_H))
            .into_editor()
    }
}

truce::plugin! {
    logic: OrbitPlugin,
    params: OrbitParams,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use truce_test::{assertions, driver, InputSource};

    #[test]
    fn silence_in_silence_out() {
        let result = driver!(Plugin)
            .sample_rate(44_100.0)
            .channels(2)
            .block_size(256)
            .duration(Duration::from_millis(500))
            .input(InputSource::Silence)
            .run();
        assertions::assert_no_nans(&result);
        assertions::assert_silence(&result);
    }

    #[test]
    fn places_a_signal() {
        let result = driver!(Plugin)
            .sample_rate(48_000.0)
            .channels(2)
            .block_size(333)
            .duration(Duration::from_millis(1500))
            .input(InputSource::Generator(Box::new(|i, sr| (i as f64 * 440.0 * std::f64::consts::TAU / sr).sin() as f32 * 0.5)))
            .run();
        assertions::assert_no_nans(&result);
        assertions::assert_nonzero(&result);
    }
}
