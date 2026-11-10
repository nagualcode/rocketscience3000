//! The editor, after the 2001 original (interface by Oskar Luyer):
//!
//! ┌ FOLLO 3K ─────────────────────────────────────────────────────┐
//! │ analysis · bandwidth · release │  the band now (response)      │
//! ├────────────────────────────────────────────────────────────────┤
//! │ FILTER PEAK AT LOW AMPLITUDE   (yellow slider)                 │
//! │ 20Hz ─ 40 ─ 80 ─ … ─ 10.4kHz        ● the ball                 │
//! │ FILTER PEAK AT HIGH AMPLITUDE  (blue slider)                   │
//! └────────────────────────────────────────────────────────────────┘

use crate::engine::{coefficients, hz_slider, response, slider_hz, BW_SEMITONES};
use crate::params::{hz_label, FolloParams, P};
use egui::{pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke};
use mission::breakpoint::breakpoint;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use truce::prelude::*;
use truce_core::editor::PluginContext;
use truce_egui::widgets::{level_meter, param_knob};

pub const WINDOW_W: u32 = 900;
pub const WINDOW_H: u32 = 520;
pub const MIN_W: u32 = 760;
pub const MIN_H: u32 = 460;
pub const MAX_W: u32 = 1800;
pub const MAX_H: u32 = 1100;

type Ctx = PluginContext<FolloParams>;

#[derive(Clone, Copy)]
struct Palette {
    bg: Color32,
    panel: Color32,
    screen: Color32,
    grid: Color32,
    text: Color32,
    dim: Color32,
    low: Color32,
    high: Color32,
    ball: Color32,
    curve: Color32,
    header: Color32,
}

const MODERN: Palette = Palette {
    bg: Color32::from_rgb(26, 28, 33),
    panel: Color32::from_rgb(40, 44, 52),
    screen: Color32::from_rgb(12, 15, 20),
    grid: Color32::from_rgb(50, 56, 66),
    text: Color32::from_rgb(216, 220, 226),
    dim: Color32::from_rgb(130, 138, 150),
    low: Color32::from_rgb(250, 210, 60),
    high: Color32::from_rgb(70, 150, 255),
    ball: Color32::from_rgb(255, 120, 60),
    curve: Color32::from_rgb(120, 200, 255),
    header: Color32::from_rgb(52, 56, 64),
};

/// The original: an orange case, a deep-blue panel with silver lettering.
const CLASSIC: Palette = Palette {
    bg: Color32::from_rgb(236, 150, 30),
    panel: Color32::from_rgb(28, 78, 170),
    screen: Color32::from_rgb(10, 30, 80),
    grid: Color32::from_rgb(60, 100, 180),
    text: Color32::from_rgb(232, 238, 248),
    dim: Color32::from_rgb(176, 196, 230),
    low: Color32::from_rgb(250, 220, 40),
    high: Color32::from_rgb(110, 200, 255),
    ball: Color32::from_rgb(255, 90, 40),
    curve: Color32::from_rgb(255, 220, 120),
    header: Color32::from_rgb(160, 80, 10),
};

fn visuals(p: &Palette) -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.panel_fill = p.bg;
    v.window_fill = p.panel;
    v.extreme_bg_color = p.screen;
    v.faint_bg_color = p.panel;
    v.override_text_color = Some(p.text);
    v.selection.bg_fill = p.ball;
    for (w, fill) in [
        (&mut v.widgets.noninteractive, p.header),
        (&mut v.widgets.inactive, p.header),
        (&mut v.widgets.hovered, p.grid),
        (&mut v.widgets.active, p.ball),
        (&mut v.widgets.open, p.header),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::NONE;
        w.corner_radius = egui::CornerRadius::same(3);
    }
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.dim);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, p.text);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, p.text);
    v.widgets.active.fg_stroke = Stroke::new(1.5_f32, p.text);
    v
}

fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

fn normalized(ctx: &Ctx, id: P, plain: f64) -> f64 {
    let id: u32 = id.into();
    ctx.params().param_infos().iter().find(|i| i.id == id).map(|i| i.range.normalize(plain)).unwrap_or(0.0)
}

fn label(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.label(egui::RichText::new(text).size(11.5).color(p.dim).strong().italics());
}

fn panel(ui: &mut egui::Ui, fill: Color32, size: egui::Vec2, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE.fill(fill).corner_radius(6.0).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
        let inner = size - vec2(20.0, 20.0);
        ui.set_min_size(inner);
        ui.set_max_size(inner);
        ui.vertical(|ui| add(ui));
    });
}

pub fn follo_ui(ui: &mut egui::Ui, ctx: &Ctx) {
    let params: &Arc<FolloParams> = ctx.params();
    let classic = params.theme.read().map(|t| *t == 1).unwrap_or(false);
    let p = if classic { CLASSIC } else { MODERN };
    let v = visuals(&p);
    ui.ctx().set_visuals(v.clone());
    ui.style_mut().visuals = v;

    egui::Panel::top("follo-top")
        .exact_size(38.0)
        .frame(egui::Frame::NONE.fill(p.header).inner_margin(egui::Margin::symmetric(12, 8)))
        .show_inside(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(egui::RichText::new("FOLLO 3K").size(17.0).strong().italics().color(p.text));
                ui.label(egui::RichText::new("energy driven band booster · rocketscience3000").size(10.5).color(p.dim));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let b = egui::Button::new(egui::RichText::new(if classic { "Classic" } else { "Modern" }).size(11.5));
                    if ui.add(b).on_hover_text("Theme").clicked() {
                        if let Ok(mut t) = params.theme.write() {
                            *t = if classic { 0 } else { 1 };
                        }
                    }
                });
            });
        });

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(p.bg).inner_margin(egui::Margin::same(12)))
        .show_inside(ui, |ui| {
            let h = ui.available_height();
            let gap = ui.spacing().item_spacing;
            let top = (h * 0.52).floor();
            ui.horizontal_top(|ui| {
                let left = 380.0;
                panel(ui, p.panel, vec2(left, top), |ui| knobs(ui, ctx, &p));
                let w = ui.available_width();
                panel(ui, p.panel, vec2(w, top), |ui| band(ui, ctx, &p));
            });
            let rest = h - top - gap.y;
            panel(ui, p.panel, vec2(ui.available_width(), rest), |ui| strip(ui, ctx, &p));
        });
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
}

fn knobs(ui: &mut egui::Ui, ctx: &Ctx, p: &Palette) {
    let params = ctx.params();
    ui.horizontal(|ui| {
        param_knob(ui, ctx, P::Analysis, "analysis");
        param_knob(ui, ctx, P::Bandwidth, "bandwidth");
        param_knob(ui, ctx, P::Release, "release");
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        param_knob(ui, ctx, P::Output, "output");
        level_meter(ui, ctx, &[P::MeterL, P::MeterR], 70.0);
        ui.vertical(|ui| {
            let g = f32::from_bits(params.status.limiter.load(Ordering::Relaxed));
            ui.label(egui::RichText::new(format!("limiter {:+.1} dB", 20.0 * g.max(1e-6).log10())).size(10.5).color(p.dim))
                .on_hover_text("The original look-ahead peak limiter (−6 dBFS, 300 samples)");
            ui.label(egui::RichText::new("Analysis sets how hard the level drives the ball; it does not change the output gain.").size(10.0).color(p.dim));
        });
    });
}

/// The band as it is now: the filter's response around the ball.
fn band(ui: &mut egui::Ui, ctx: &Ctx, p: &Palette) {
    let params = ctx.params();
    let st = &params.status;
    let sr = st.sample_rate.load(Ordering::Relaxed) as f64;
    let f = f32::from_bits(st.frequency.load(Ordering::Relaxed)) as f64;
    let bw = st.bandwidth.load(Ordering::Relaxed) as i32;
    label(ui, p, "THE BAND");
    let w = ui.available_width();
    let h = ui.available_height() - 16.0;
    let (rect, _) = ui.allocate_exact_size(vec2(w, h.max(40.0)), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, p.screen);
    let (fa, fb) = (20.0f64, (sr * 0.5).min(16000.0));
    let fx = |hz: f64| rect.left() + ((hz / fa).ln() / (fb / fa).ln()) as f32 * rect.width();
    for hz in [50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0] {
        let x = fx(hz);
        painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.0_f32, alpha(p.grid, 0.6)));
        let t = if hz >= 1000.0 { format!("{}k", hz / 1000.0) } else { format!("{hz}") };
        painter.text(pos2(x + 2.0, rect.bottom() - 2.0), Align2::LEFT_BOTTOM, t, FontId::proportional(9.0), alpha(p.dim, 0.7));
    }
    let lo = slider_hz(st.low.load(Ordering::Relaxed) as i32);
    let hi = slider_hz(st.high.load(Ordering::Relaxed) as i32);
    painter.rect_filled(Rect::from_x_y_ranges(fx(lo.min(hi))..=fx(hi.max(lo)), rect.y_range()), 0.0, alpha(p.grid, 0.25));
    if f > 1.0 && sr > 0.0 {
        let co = coefficients(f, breakpoint(bw as i16, &BW_SEMITONES), sr);
        let steps = rect.width() as usize;
        let pts: Vec<Pos2> = (0..=steps)
            .map(|i| {
                let hz = fa * (fb / fa).powf(i as f64 / steps as f64);
                let db = 20.0 * response(&co, std::f64::consts::TAU * hz / sr).max(1e-6).log10();
                // +30 dB at the top, −30 at the bottom.
                let y = rect.top() + rect.height() * ((30.0 - db as f32) / 60.0).clamp(0.0, 1.0);
                pos2(fx(hz), y)
            })
            .collect();
        painter.add(egui::Shape::line(pts, Stroke::new(1.8_f32, p.curve)));
        painter.circle_filled(pos2(fx(f.clamp(fa, fb)), rect.top() + 10.0), 5.0, p.ball);
    }
    ui.label(egui::RichText::new(format!("peak at {}", hz_label(f))).size(10.5).color(p.dim));
}

/// One slider of the strip: drag sets the parameter.
fn slider(ui: &mut egui::Ui, ctx: &Ctx, p: &Palette, id: P, value: i32, col: Color32, caption: &str) {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 26.0), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let track = rect.shrink2(vec2(14.0, 10.0));
    painter.rect_filled(track, 3.0, Color32::from_rgb(6, 8, 12));
    let x = track.left() + value as f32 / 1080.0 * track.width();
    painter.rect_filled(Rect::from_center_size(pos2(x, track.center().y), vec2(12.0, 20.0)), 3.0, col);
    painter.text(rect.right_center() - vec2(18.0, 0.0), Align2::RIGHT_CENTER, caption, FontId::proportional(10.0), alpha(p.dim, 0.9));
    if resp.drag_started() {
        ctx.begin_edit(id);
    }
    if let Some(pt) = resp.interact_pointer_pos() {
        if resp.dragged() || resp.clicked() {
            let v = (((pt.x - track.left()) / track.width()) * 1080.0).round().clamp(0.0, 1080.0) as f64;
            if resp.clicked() { ctx.automate(id, normalized(ctx, id, v)) } else { ctx.set_param(id, normalized(ctx, id, v)) }
        }
    }
    if resp.drag_stopped() {
        ctx.end_edit(id);
    }
    resp.on_hover_text(ctx.format_param(id));
}

fn strip(ui: &mut egui::Ui, ctx: &Ctx, p: &Palette) {
    let params = ctx.params();
    let st = &params.status;
    let lo = st.low.load(Ordering::Relaxed) as i32;
    let hi = st.high.load(Ordering::Relaxed) as i32;
    label(ui, p, "FILTER PEAK AT LOW AMPLITUDE");
    slider(ui, ctx, p, P::Low, lo, p.low, &hz_label(slider_hz(lo)));
    // The scale and the ball.
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 30.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let track = rect.shrink2(vec2(14.0, 0.0));
    for k in 0..10 {
        let pos = k as f32 * 120.0;
        let x = track.left() + pos / 1080.0 * track.width();
        let hz = slider_hz(pos as i32);
        let t = if hz >= 1000.0 { format!("{:.1}k", hz / 1000.0) } else { format!("{hz:.0}") };
        painter.text(pos2(x, rect.center().y), Align2::CENTER_CENTER, t, FontId::monospace(10.0), alpha(p.text, 0.8));
    }
    let f = f32::from_bits(st.frequency.load(Ordering::Relaxed)) as f64;
    if f > 1.0 {
        let x = track.left() + (hz_slider(f) as f32 / 1080.0).clamp(0.0, 1.05) * track.width();
        let env = f32::from_bits(st.envelope.load(Ordering::Relaxed));
        painter.circle_filled(pos2(x, rect.center().y), 7.0 + 3.0 * env.clamp(0.0, 1.0), alpha(p.ball, 0.9));
    }
    slider(ui, ctx, p, P::High, hi, p.high, &hz_label(slider_hz(hi)));
    label(ui, p, "FILTER PEAK AT HIGH AMPLITUDE");
    ui.label(
        egui::RichText::new("Loud input sends the ball toward the blue slider; it falls back to the yellow one at the release speed.")
            .size(10.0)
            .color(p.dim),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cargo test --release -p follo --lib readme_screenshots -- --ignored`
    #[test]
    #[ignore]
    fn readme_screenshots() {
        let params = Arc::new(FolloParams::new());
        let st = &params.status;
        st.frequency.store((slider_hz(560) as f32).to_bits(), Ordering::Relaxed);
        st.envelope.store(0.6f32.to_bits(), Ordering::Relaxed);
        st.bandwidth.store(30, Ordering::Relaxed);
        let docs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
        std::fs::create_dir_all(&docs).unwrap();
        for (theme, file) in [(0u8, docs.join("follo.png")), (1, docs.join("follo-classic.png"))] {
            *params.theme.write().unwrap() = theme;
            let mut editor = crate::FolloPlugin::editor(params.clone());
            if let Some((px, w, h)) = editor.screenshot(params.clone()) {
                truce_test::save_png(&file, &px, w, h);
            }
        }
    }
}
