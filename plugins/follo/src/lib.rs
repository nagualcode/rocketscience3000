//! Follo 3K — the "energy driven band booster" of Audio Ease's Rocket Science
//! bundle (2001), rebuilt from the original binary.
//!
//! The engine (`engine.rs`) is a translation of the i386 slice of
//! `Follo.vst` 3.6.2 and renders bit-identical output to that machine code
//! (see `reverse_engineering/follo/`).

pub mod engine;
pub mod params;
pub mod ui;

use engine::{receive_midi, Controls, Follo, Layout};
use mission::agc::LOOKAHEAD;
use mission::denormal::DenormalFlush;
use params::{FolloParams, P};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use truce::prelude::*;
use truce_egui::EguiEditor;

/// The limiter's look-ahead (`GetLatency` @ 0x213e2: 300).
pub const LATENCY: u32 = LOOKAHEAD as u32;

pub struct FolloDsp {
    engine: Box<Follo>,
    sample_rate: f64,
    /// Controls in force: MIDI CCs write into them (as into the original's
    /// parameter objects); a parameter the host moves overrides.
    controls: Controls,
    seen: Controls,
    l: Vec<f32>,
    r: Vec<f32>,
}

impl FolloDsp {
    fn new(sample_rate: f64, layout: Layout, params: &FolloParams, block: usize) -> Self {
        let c = params.controls();
        Self { engine: Box::new(Follo::new(sample_rate, layout)), sample_rate, controls: c, seen: c, l: vec![0.0; block], r: vec![0.0; block] }
    }

    fn follow(&mut self, p: Controls) {
        let (c, s) = (&mut self.controls, &self.seen);
        macro_rules! take {
            ($($f:ident),*) => { $( if p.$f != s.$f { c.$f = p.$f; } )* };
        }
        take!(bandwidth, release, low, high, analysis);
        self.seen = p;
    }
}

impl Default for FolloDsp {
    fn default() -> Self {
        FolloDsp::new(44100.0, Layout::Stereo, &FolloParams::new(), 4096)
    }
}

pub struct FolloPlugin;

impl PluginLogic for FolloPlugin {
    type Params = FolloParams;
    type DspState = FolloDsp;

    fn bus_layouts() -> Vec<BusLayout> {
        vec![
            BusLayout::stereo(),
            BusLayout::new().with_input("Main", ChannelConfig::Mono).with_output("Main", ChannelConfig::Stereo),
            BusLayout::mono(),
        ]
    }

    fn init(params: &Self::Params, _cx: &InitContext) -> Self::DspState {
        FolloDsp::new(44100.0, Layout::Stereo, params, 4096)
    }

    fn reset(state: &mut Self::DspState, params: &Self::Params, config: &AudioConfig) {
        *state = FolloDsp::new(config.sample_rate, state.engine.layout, params, config.max_block_size.max(64));
        params.status.sample_rate.store(config.sample_rate as u32, Ordering::Relaxed);
    }

    fn latency(_state: &Self::DspState) -> u32 {
        LATENCY
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
        let layout = match (nin, nout) {
            (_, 1) => Layout::Mono,
            (1, _) => Layout::MonoToStereo,
            _ => Layout::Stereo,
        };
        if layout != state.engine.layout {
            let sr = state.sample_rate;
            *state = FolloDsp::new(sr, layout, params, state.l.len().max(n));
        }
        state.follow(params.controls());
        for ev in events.iter() {
            if let EventBody::ControlChange { cc, value, .. } = ev.body {
                receive_midi(&mut state.controls, 0xb0, cc, value);
            }
        }
        if state.l.len() < n {
            state.l.resize(n, 0.0);
            state.r.resize(n, 0.0);
        }
        let (l, r) = (&mut state.l[..n], &mut state.r[..n]);
        let a = if nin > 0 { buffer.input(0) } else { &[][..] };
        for i in 0..n {
            l[i] = a.get(i).copied().unwrap_or(0.0);
        }
        if nin > 1 {
            r.copy_from_slice(buffer.input(1));
        } else {
            r.copy_from_slice(l);
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
        let e = &state.engine;
        st.frequency.store((e.frequency as f32).to_bits(), Ordering::Relaxed);
        st.envelope.store(e.envelope.to_bits(), Ordering::Relaxed);
        st.limiter.store(e.limiter_gain().to_bits(), Ordering::Relaxed);
        st.low.store(state.controls.low as u32, Ordering::Relaxed);
        st.high.store(state.controls.high as u32, Ordering::Relaxed);
        st.bandwidth.store(state.controls.bandwidth as u32, Ordering::Relaxed);
        context.set_meter(P::MeterL, buffer.output_peak(0));
        context.set_meter(P::MeterR, buffer.output_peak(if nout > 1 { 1 } else { 0 }));
        ProcessStatus::Normal
    }

    fn editor(params: Arc<Self::Params>) -> Box<dyn Editor> {
        EguiEditor::new(params.clone(), (ui::WINDOW_W, ui::WINDOW_H), ui::follo_ui)
            .with_visuals(truce_egui::theme::dark())
            .resizable(true)
            .min_size((ui::MIN_W, ui::MIN_H))
            .max_size((ui::MAX_W, ui::MAX_H))
            .into_editor()
    }
}

truce::plugin! {
    logic: FolloPlugin,
    params: FolloParams,
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
    fn filters_a_signal() {
        let result = driver!(Plugin)
            .sample_rate(48_000.0)
            .channels(2)
            .block_size(333)
            .duration(Duration::from_millis(1500))
            .input(InputSource::Generator(Box::new(|i, sr| {
                let ph = (i as f64 * 110.0 / sr).fract();
                (ph * 2.0 - 1.0) as f32 * 0.4
            })))
            .run();
        assertions::assert_no_nans(&result);
        assertions::assert_nonzero(&result);
    }
}
