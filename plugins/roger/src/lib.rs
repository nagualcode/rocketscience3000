//! Roger 3K — the "multiple gender vowel bank" of Audio Ease's Rocket Science
//! bundle (2001), rebuilt from the original binary.
//!
//! The engine (`engine.rs`) is a translation of the i386 slice of
//! `Roger.vst` 3.6.2 and renders bit-identical output to that machine code
//! (see `reverse_engineering/roger/`).

pub mod engine;
pub mod params;
pub mod tables;
pub mod ui;

use engine::{receive_midi, Controls, HostTime, Layout, Roger};
use mission::agc::LOOKAHEAD;
use mission::denormal::DenormalFlush;
use params::{RogerParams, P};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use truce::prelude::*;
use truce_egui::EguiEditor;

/// The limiter's look-ahead (`RogerProcess::GetLatency` @ 0x22988: 300).
pub const LATENCY: u32 = LOOKAHEAD as u32;

pub struct RogerDsp {
    engine: Box<Roger>,
    sample_rate: f64,
    /// The controls in force. Like the original's parameter objects, Auto
    /// and MIDI write into them; a parameter the host moves overrides.
    controls: Controls,
    /// The parameters as last seen, to notice host moves.
    seen: Controls,
    last_playing: bool,
    l: Vec<f32>,
    r: Vec<f32>,
}

impl RogerDsp {
    fn new(sample_rate: f64, layout: Layout, params: &RogerParams, block: usize) -> Self {
        let c = params.controls();
        Self {
            engine: Box::new(Roger::new(sample_rate, layout, c.bandwidth)),
            sample_rate,
            controls: c,
            seen: c,
            last_playing: false,
            l: vec![0.0; block],
            r: vec![0.0; block],
        }
    }

    /// Host moves of the parameters win over Auto / MIDI.
    fn follow(&mut self, p: Controls) {
        let (c, s) = (&mut self.controls, &self.seen);
        macro_rules! take {
            ($($f:ident),*) => { $( if p.$f != s.$f { c.$f = p.$f; } )* };
        }
        take!(bandwidth, portamento, auto_speed, width_mode, port_mode, auto, vowel, family, skip);
        self.seen = p;
    }
}

impl Default for RogerDsp {
    fn default() -> Self {
        RogerDsp::new(44100.0, Layout::Stereo, &RogerParams::new(), 4096)
    }
}

/// MIDI as raw status / data bytes for `RogerProcess::ReceiveMidi`.
fn raw_midi(ev: &EventBody) -> Option<(u8, u8, u8)> {
    match *ev {
        EventBody::NoteOn { note, velocity, .. } => Some((0x90, note, velocity)),
        EventBody::NoteOff { note, .. } => Some((0x80, note, 0)),
        EventBody::ControlChange { cc, value, .. } => Some((0xb0, cc, value)),
        _ => None,
    }
}

pub struct RogerPlugin;

impl PluginLogic for RogerPlugin {
    type Params = RogerParams;
    type DspState = RogerDsp;

    fn bus_layouts() -> Vec<BusLayout> {
        vec![
            BusLayout::stereo(),
            BusLayout::new().with_input("Main", ChannelConfig::Mono).with_output("Main", ChannelConfig::Stereo),
            BusLayout::mono(),
        ]
    }

    fn init(params: &Self::Params, _cx: &InitContext) -> Self::DspState {
        RogerDsp::new(44100.0, Layout::Stereo, params, 4096)
    }

    fn reset(state: &mut Self::DspState, params: &Self::Params, config: &AudioConfig) {
        *state = RogerDsp::new(config.sample_rate, state.engine.layout, params, config.max_block_size.max(64));
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
            // Only on a layout change (the original had one process per variant).
            let sr = state.sample_rate;
            *state = RogerDsp::new(sr, layout, params, state.l.len().max(n));
        }
        state.follow(params.controls());
        for ev in events.iter() {
            if let Some((s, d1, d2)) = raw_midi(&ev.body) {
                receive_midi(&mut state.controls, s, d1, d2);
            }
        }

        let tr = context.transport;
        let changed = tr.playing != state.last_playing;
        state.last_playing = tr.playing;
        let host = if tr.tempo > 0.0 {
            HostTime::new(state.sample_rate, tr.tempo, tr.position_beats, changed)
        } else {
            HostTime { samples_per_quarter: 0.0, samples_to_next_beat: 0, changed }
        };

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
        let vowel = state.controls.vowel;
        state.engine.process(&mut state.controls, &host, l, r);

        let g = 10f32.powf(params.output.read() / 20.0);
        for i in 0..n {
            buffer.output(0)[i] = l[i] * g;
            if nout > 1 {
                buffer.output(1)[i] = r[i] * g;
            }
        }

        let st = &params.status;
        let e = &state.engine;
        for k in 0..6 {
            st.formants[k].store((e.formants[k] as f32).to_bits(), Ordering::Relaxed);
        }
        st.bandwidth.store(e.bandwidth.to_bits(), Ordering::Relaxed);
        st.vowel.store(state.controls.vowel, Ordering::Relaxed);
        st.family.store(state.controls.family, Ordering::Relaxed);
        st.limiter.store(e.limiter_gain().to_bits(), Ordering::Relaxed);
        if state.controls.vowel != vowel {
            st.beats.fetch_add(1, Ordering::Relaxed);
        }
        context.set_meter(P::MeterL, buffer.output_peak(0));
        context.set_meter(P::MeterR, buffer.output_peak(if nout > 1 { 1 } else { 0 }));
        ProcessStatus::Normal
    }

    fn editor(params: Arc<Self::Params>) -> Box<dyn Editor> {
        EguiEditor::new(params.clone(), (ui::WINDOW_W, ui::WINDOW_H), ui::roger_ui)
            .with_visuals(truce_egui::theme::dark())
            .resizable(true)
            .min_size((ui::MIN_W, ui::MIN_H))
            .max_size((ui::MAX_W, ui::MAX_H))
            .into_editor()
    }
}

truce::plugin! {
    logic: RogerPlugin,
    params: RogerParams,
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
