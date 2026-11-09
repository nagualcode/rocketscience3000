//! The editor, laid out like the 2001 original (interface by Oskar Luyer):
//!
//! ┌ ROGER 3K ─────────────────────────────────────────────────────┐
//! │ family portraits      │  the mouth: vowel chart  │ bandwidth   │
//! │ AUTO on/off, tempo,   │  + filter response       │ portamento  │
//! │ beat skips            │                          │ output      │
//! ├───────────────────────────────────────────────────────────────┤
//! │ OO  AW  UH  AH  U  IR  A  E  I  EE        (the vowel bar)      │
//! └───────────────────────────────────────────────────────────────┘

use crate::engine::{coefficients_for, response};
use crate::params::{AutoSpeed, RogerParams, P};
use crate::tables::{FAMILY, FORM, VOWELS};
use egui::{pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use truce::prelude::*;
use truce_core::editor::PluginContext;
use truce_egui::widgets::{level_meter, param_knob};

pub const WINDOW_W: u32 = 940;
pub const WINDOW_H: u32 = 600;
pub const MIN_W: u32 = 820;
pub const MIN_H: u32 = 540;
pub const MAX_W: u32 = 1800;
pub const MAX_H: u32 = 1200;

type Ctx = PluginContext<RogerParams>;

#[derive(Clone, Copy)]
struct Palette {
    bg: Color32,
    panel: Color32,
    screen: Color32,
    grid: Color32,
    text: Color32,
    dim: Color32,
    accent: Color32,
    accent_text: Color32,
    key: Color32,
    key_text: Color32,
    curve: Color32,
    header: Color32,
    /// Portrait backgrounds: Cindy blue, Roger green, Patty orange.
    family: [Color32; 3],
}

const MODERN: Palette = Palette {
    bg: Color32::from_rgb(28, 29, 33),
    panel: Color32::from_rgb(42, 44, 50),
    screen: Color32::from_rgb(14, 16, 20),
    grid: Color32::from_rgb(52, 56, 64),
    text: Color32::from_rgb(216, 218, 222),
    dim: Color32::from_rgb(130, 136, 146),
    accent: Color32::from_rgb(255, 120, 70),
    accent_text: Color32::from_rgb(28, 12, 6),
    key: Color32::from_rgb(58, 62, 70),
    key_text: Color32::from_rgb(220, 224, 230),
    curve: Color32::from_rgb(255, 150, 90),
    header: Color32::from_rgb(54, 57, 64),
    family: [Color32::from_rgb(110, 160, 200), Color32::from_rgb(120, 170, 70), Color32::from_rgb(225, 120, 50)],
};

/// The original: a yellow-gold case, red-orange panels with rivets,
/// teal-grey vowel keys.
const CLASSIC: Palette = Palette {
    bg: Color32::from_rgb(232, 176, 40),
    panel: Color32::from_rgb(214, 62, 28),
    screen: Color32::from_rgb(40, 22, 10),
    grid: Color32::from_rgb(120, 60, 30),
    text: Color32::from_rgb(255, 246, 228),
    dim: Color32::from_rgb(255, 214, 170),
    accent: Color32::from_rgb(255, 200, 40),
    accent_text: Color32::from_rgb(30, 20, 0),
    key: Color32::from_rgb(140, 176, 176),
    key_text: Color32::from_rgb(20, 30, 34),
    curve: Color32::from_rgb(255, 214, 80),
    header: Color32::from_rgb(150, 40, 18),
    family: [Color32::from_rgb(120, 170, 200), Color32::from_rgb(130, 170, 60), Color32::from_rgb(230, 120, 40)],
};

fn visuals(p: &Palette) -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.panel_fill = p.bg;
    v.window_fill = p.panel;
    v.extreme_bg_color = p.screen;
    v.faint_bg_color = p.panel;
    v.override_text_color = Some(p.text);
    v.selection.bg_fill = p.accent;
    v.selection.stroke = Stroke::new(1.0_f32, p.accent_text);
    for (w, fill) in [
        (&mut v.widgets.noninteractive, p.header),
        (&mut v.widgets.inactive, p.header),
        (&mut v.widgets.hovered, p.grid),
        (&mut v.widgets.active, p.accent),
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

fn pill(ui: &mut egui::Ui, p: &Palette, text: &str, on: bool) -> egui::Response {
    let (fill, fg) = if on { (p.accent, p.accent_text) } else { (p.header, p.text) };
    ui.add(egui::Button::new(egui::RichText::new(text).size(11.5).color(fg)).fill(fill).min_size(vec2(0.0, 22.0)))
}

fn panel(ui: &mut egui::Ui, fill: Color32, size: egui::Vec2, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::NONE.fill(fill).corner_radius(6.0).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
        let inner = size - vec2(20.0, 20.0);
        ui.set_min_size(inner);
        ui.set_max_size(inner);
        ui.vertical(|ui| add(ui));
    });
}

/// Radio row for a knob mode (knob / mod wheel / velocity).
fn mode_row(ui: &mut egui::Ui, ctx: &Ctx, p: &Palette, id: P, current: usize) {
    ui.horizontal(|ui| {
        for (i, name) in ["knob", "mod wheel", "velocity"].iter().enumerate() {
            if pill(ui, p, name, i == current).clicked() && i != current {
                ctx.automate(id, normalized(ctx, id, i as f64));
            }
        }
    });
}

// --- Editor ------------------------------------------------------------------

pub fn roger_ui(ui: &mut egui::Ui, ctx: &Ctx) {
    let params: &Arc<RogerParams> = ctx.params();
    let classic = params.theme.read().map(|t| *t == 1).unwrap_or(false);
    let p = if classic { CLASSIC } else { MODERN };
    let v = visuals(&p);
    ui.ctx().set_visuals(v.clone());
    ui.style_mut().visuals = v;

    egui::Panel::top("roger-top")
        .exact_size(38.0)
        .frame(egui::Frame::NONE.fill(p.header).inner_margin(egui::Margin::symmetric(12, 8)))
        .show_inside(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(egui::RichText::new("ROGER 3K").size(17.0).strong().italics().color(p.text));
                ui.label(egui::RichText::new("multiple gender vowel bank · rocketscience3000").size(10.5).color(p.dim));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if pill(ui, &p, if classic { "Classic" } else { "Modern" }, false).on_hover_text("Theme").clicked() {
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
            let bar = 64.0;
            let top = h - bar - gap.y;
            ui.horizontal_top(|ui| {
                let left = 250.0;
                let right = 230.0;
                let mid = ui.available_width() - left - right - 2.0 * gap.x;
                panel(ui, p.panel, vec2(left, top), |ui| family_and_auto(ui, ctx, &p));
                panel(ui, p.panel, vec2(mid, top), |ui| mouth(ui, ctx, &p));
                panel(ui, p.panel, vec2(right, top), |ui| knobs(ui, ctx, &p));
            });
            panel(ui, p.panel, vec2(ui.available_width(), bar), |ui| vowel_bar(ui, ctx, &p));
        });
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
}

// --- Family and Auto -----------------------------------------------------------

/// A stylised head in profile on the member's colour.
fn portrait(painter: &egui::Painter, c: Pos2, r: f32, bg: Color32, member: usize, lit: bool, p: &Palette) {
    painter.circle_filled(c, r, if lit { bg } else { alpha(bg, 0.35) });
    painter.circle_stroke(c, r, Stroke::new(if lit { 3.0_f32 } else { 1.5_f32 }, if lit { p.accent } else { alpha(p.dim, 0.5) }));
    let skin = Color32::from_rgb(236, 196, 160);
    let hair = [Color32::from_rgb(40, 28, 20), Color32::from_rgb(70, 50, 30), Color32::from_rgb(30, 20, 18)][member];
    let head = (0.36 + 0.06 * (member != 0) as i32 as f32) * r;
    let hc = c + vec2(0.0, -0.08 * r);
    painter.circle_filled(c + vec2(0.0, 0.62 * r), 0.42 * r, alpha(Color32::from_rgb(60, 60, 80), if lit { 1.0 } else { 0.5 }));
    match member {
        0 => painter.circle_filled(hc + vec2(-0.12 * r, -0.05 * r), head * 1.08, hair), // Cindy: bob
        2 => painter.circle_filled(hc + vec2(-0.16 * r, 0.06 * r), head * 1.2, hair),   // Patty: long hair
        _ => painter.circle_filled(hc + vec2(-0.06 * r, -0.1 * r), head * 1.0, hair),   // Roger: short
    };
    painter.circle_filled(hc + vec2(0.06 * r, 0.04 * r), head, if lit { skin } else { alpha(skin, 0.5) });
    painter.circle_filled(hc + vec2(0.18 * r, -0.02 * r), 0.035 * r, Color32::from_rgb(30, 30, 30));
    if member == 0 {
        painter.circle_filled(hc + vec2(-0.2 * r, -0.36 * r), 0.09 * r, Color32::from_rgb(240, 150, 40)); // bow
    }
}

fn family_and_auto(ui: &mut egui::Ui, ctx: &Ctx, p: &Palette) {
    let params = ctx.params();
    let st = &params.status;
    let fam = st.family.load(Ordering::Relaxed).clamp(0, 2) as usize;
    label(ui, p, "FAMILY");
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 90.0), Sense::click());
    let painter = ui.painter_at(rect);
    // Ordered as on the keyboard: Roger (C2), Patty (C3), Cindy (C4).
    let order = [1usize, 2, 0];
    let cw = rect.width() / 3.0;
    for (slot, &m) in order.iter().enumerate() {
        let c = pos2(rect.left() + cw * (slot as f32 + 0.5), rect.top() + 36.0);
        portrait(&painter, c, 32.0, p.family[m], m, m == fam, p);
        painter.text(pos2(c.x, rect.bottom() - 2.0), Align2::CENTER_BOTTOM, FAMILY[m], FontId::proportional(11.5), if m == fam { p.text } else { p.dim });
    }
    if let Some(pt) = resp.interact_pointer_pos() {
        if resp.clicked() {
            let slot = (((pt.x - rect.left()) / cw) as usize).min(2);
            ctx.automate(P::Family, normalized(ctx, P::Family, order[slot] as f64));
        }
    }
    resp.on_hover_text("Roger, Patty and Cindy: a man's, a woman's and a child's vowels (Peterson & Barney, 1952)");

    ui.add_space(10.0);
    ui.horizontal(|ui| {
        label(ui, p, "AUTO");
        let on = params.auto.value();
        if pill(ui, p, "on", on).clicked() {
            ctx.automate(P::Auto, 1.0);
        }
        if pill(ui, p, "off", !on).clicked() {
            ctx.automate(P::Auto, 0.0);
        }
        // A light that flashes on every Auto vowel.
        let beats = st.beats.load(Ordering::Relaxed);
        let id = egui::Id::new("roger-beat");
        let (last, t) = ui.data(|d| d.get_temp::<(u32, f64)>(id)).unwrap_or((0, 0.0));
        let now = ui.input(|i| i.time);
        let t = if beats != last { now } else { t };
        ui.data_mut(|d| d.insert_temp(id, (beats, t)));
        let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
        let a = (1.0 - (now - t) as f32 / 0.25).clamp(0.0, 1.0);
        ui.painter().circle_filled(r.center(), 6.0, alpha(p.accent, 0.2 + 0.8 * a));
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        label(ui, p, "tempo");
        let names = AutoSpeed::variant_names();
        let cur = params.auto_speed.index() as usize;
        egui::ComboBox::from_id_salt("roger-speed")
            .width(70.0)
            .selected_text(names[cur])
            .show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    if ui.selectable_label(i == cur, *n).clicked() && i != cur {
                        ctx.automate(P::AutoSpeed, normalized(ctx, P::AutoSpeed, i as f64));
                    }
                }
            });
        ui.label(egui::RichText::new("× bpm").size(11.0).color(p.dim));
    })
    .response
    .on_hover_text("Vowels per beat of the host tempo");
    ui.add_space(4.0);
    param_knob(ui, ctx, P::Skip, "beat skips");
    ui.label(egui::RichText::new("Auto babbles random vowels in time; skips leave some beats out.").size(10.0).color(p.dim));
}

// --- The mouth ---------------------------------------------------------------------

fn mouth(ui: &mut egui::Ui, ctx: &Ctx, p: &Palette) {
    let params = ctx.params();
    let st = &params.status;
    let sr = st.sample_rate.load(Ordering::Relaxed) as f64;
    let fam = st.family.load(Ordering::Relaxed).clamp(0, 2) as usize;
    let vowel = st.vowel.load(Ordering::Relaxed).clamp(0, 9) as usize;
    let f: [f64; 6] = std::array::from_fn(|k| f32::from_bits(st.formants[k].load(Ordering::Relaxed)) as f64);
    let bw = f32::from_bits(st.bandwidth.load(Ordering::Relaxed));
    label(ui, p, "THE MOUTH");
    let w = ui.available_width();
    let h = ui.available_height();

    // Vowel chart: F2 right to left (front vowels left), F1 top to bottom,
    // as phoneticians draw it.
    let chart_h = (h * 0.55).floor();
    let (rect, _) = ui.allocate_exact_size(vec2(w, chart_h), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, p.screen);
    let inner = rect.shrink(14.0);
    let (f2a, f2b) = (3600.0f64, 700.0f64);
    let (f1a, f1b) = (200.0f64, 1150.0f64);
    let at = |f1: f64, f2: f64| {
        let x = (f2a / f2.clamp(f2b, f2a)).ln() / (f2a / f2b).ln();
        let y = (f1.clamp(f1a, f1b) / f1a).ln() / (f1b / f1a).ln();
        pos2(inner.left() + x as f32 * inner.width(), inner.top() + y as f32 * inner.height())
    };
    for (m, col) in p.family.iter().enumerate() {
        let pts: Vec<Pos2> = [9usize, 8, 7, 6, 3, 1, 0].iter().map(|&v| at(FORM[v + m * 10][0], FORM[v + m * 10][2])).collect();
        painter.add(egui::Shape::line(pts, Stroke::new(1.0_f32, alpha(*col, if m == fam { 0.55 } else { 0.15 }))));
    }
    for v in 0..10 {
        let e = &FORM[v + fam * 10];
        let q = at(e[0], e[2]);
        let lit = v == vowel;
        painter.circle_filled(q, if lit { 4.0 } else { 2.5 }, alpha(p.family[fam], if lit { 1.0 } else { 0.7 }));
        painter.text(q + vec2(5.0, -2.0), Align2::LEFT_BOTTOM, VOWELS[v].0, FontId::proportional(11.0), if lit { p.text } else { p.dim });
    }
    if f[0] > 1.0 {
        let q = at(f[0], f[2]);
        painter.circle_filled(q, 7.0, alpha(p.accent, 0.35));
        painter.circle_filled(q, 4.0, p.accent);
    }
    painter.text(rect.right_bottom() - vec2(6.0, 4.0), Align2::RIGHT_BOTTOM, "F2: front … back", FontId::proportional(9.5), alpha(p.dim, 0.7));
    painter.text(rect.left_top() + vec2(6.0, 4.0), Align2::LEFT_TOP, "F1: close … open", FontId::proportional(9.5), alpha(p.dim, 0.7));

    // Response of the four resonators (+ dry part above bandwidth 56).
    ui.add_space(6.0);
    let rh = ui.available_height() - 18.0;
    let (rect, _) = ui.allocate_exact_size(vec2(w, rh.max(40.0)), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, p.screen);
    let (fa, fb) = (80.0f64, (sr * 0.5).min(12000.0));
    let fx = |hz: f64| rect.left() + ((hz / fa).ln() / (fb / fa).ln()) as f32 * rect.width();
    for hz in [100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0] {
        if hz < fb {
            let x = fx(hz);
            painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.0_f32, alpha(p.grid, 0.6)));
            let t = if hz >= 1000.0 { format!("{}k", hz / 1000.0) } else { format!("{hz}") };
            painter.text(pos2(x + 2.0, rect.bottom() - 2.0), Align2::LEFT_BOTTOM, t, FontId::proportional(9.0), alpha(p.dim, 0.7));
        }
    }
    if f[0] > 1.0 && sr > 0.0 {
        let (co, wet, dry) = coefficients_for(&f, bw, sr);
        let steps = rect.width() as usize;
        let mut pts = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let hz = fa * (fb / fa).powf(i as f64 / steps as f64);
            let g = response(&co, std::f64::consts::TAU * hz / sr) * wet + dry;
            let db = 20.0 * g.max(1e-6).log10();
            // +24 dB at the top, −42 at the bottom.
            let y = rect.top() + rect.height() * ((24.0 - db as f32) / 66.0).clamp(0.0, 1.0);
            pts.push(pos2(fx(hz), y));
        }
        painter.add(egui::Shape::line(pts, Stroke::new(1.8_f32, p.curve)));
        for k in [0usize, 2, 4] {
            let x = fx(f[k].clamp(fa, fb));
            painter.text(pos2(x, rect.top() + 2.0), Align2::CENTER_TOP, format!("F{}", k / 2 + 1), FontId::proportional(9.5), alpha(p.text, 0.8));
        }
    }
    ui.label(
        egui::RichText::new(format!("{} — {} · F1 {:.0} · F2 {:.0} · F3 {:.0} Hz", FAMILY[fam], VOWELS[vowel].1, f[0], f[2], f[4]))
            .size(10.5)
            .color(p.dim),
    );
}

// --- Bandwidth / portamento ------------------------------------------------------

fn knobs(ui: &mut egui::Ui, ctx: &Ctx, p: &Palette) {
    let params = ctx.params();
    label(ui, p, "BANDWIDTH");
    ui.horizontal(|ui| {
        param_knob(ui, ctx, P::Bandwidth, "width");
        let bw = f32::from_bits(params.status.bandwidth.load(Ordering::Relaxed));
        ui.label(egui::RichText::new(if bw > 56.0 { "opening to dry" } else if bw < 8.0 { "whistling" } else { "" }).size(10.0).color(p.dim));
    });
    mode_row(ui, ctx, p, P::WidthMode, params.width_mode.index() as usize);
    ui.add_space(10.0);
    label(ui, p, "PORTAMENTO");
    param_knob(ui, ctx, P::Portamento, "glide");
    mode_row(ui, ctx, p, P::PortMode, params.port_mode.index() as usize);
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        param_knob(ui, ctx, P::Output, "output");
        level_meter(ui, ctx, &[P::MeterL, P::MeterR], 70.0);
    });
    let g = f32::from_bits(params.status.limiter.load(Ordering::Relaxed));
    ui.label(egui::RichText::new(format!("limiter {:+.1} dB", 20.0 * g.max(1e-6).log10())).size(10.5).color(p.dim))
        .on_hover_text("The original look-ahead peak limiter (−6 dBFS, 300 samples) that keeps whistling resonances in check");
}

// --- The vowel bar --------------------------------------------------------------

fn vowel_bar(ui: &mut egui::Ui, ctx: &Ctx, p: &Palette) {
    let params = ctx.params();
    let vowel = params.status.vowel.load(Ordering::Relaxed).clamp(0, 9) as usize;
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 40.0), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let kw = rect.width() / 10.0;
    for v in 0..10 {
        let r = Rect::from_min_size(pos2(rect.left() + v as f32 * kw, rect.top()), vec2(kw, rect.height())).shrink2(vec2(4.0, 2.0));
        let lit = v == vowel;
        painter.rect_filled(r, 4.0, if lit { p.accent } else { p.key });
        painter.text(r.center() - vec2(0.0, 5.0), Align2::CENTER_CENTER, VOWELS[v].0, FontId::proportional(16.0), if lit { p.accent_text } else { p.key_text });
        painter.text(r.center() + vec2(0.0, 11.0), Align2::CENTER_CENTER, VOWELS[v].1, FontId::proportional(9.5), alpha(if lit { p.accent_text } else { p.key_text }, 0.7));
    }
    // Click, or drag across the keys to trigger vowels quickly.
    if let Some(pt) = resp.interact_pointer_pos() {
        if resp.clicked() || resp.dragged() {
            let v = (((pt.x - rect.left()) / kw) as usize).min(9);
            if v != params.vowel.index() as usize {
                ctx.automate(P::Vowel, normalized(ctx, P::Vowel, v as f64));
            }
        }
    }
    resp.on_hover_text("Click a vowel, or drag across the bar to trigger them quickly");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cargo test --release -p roger --lib readme_screenshots -- --ignored`
    #[test]
    #[ignore]
    fn readme_screenshots() {
        let params = Arc::new(RogerParams::new());
        let st = &params.status;
        let mid: [f64; 6] = std::array::from_fn(|k| 0.6 * FORM[13][k] + 0.4 * FORM[16][k]);
        for k in 0..6 {
            st.formants[k].store((mid[k] as f32).to_bits(), Ordering::Relaxed);
        }
        st.vowel.store(6, Ordering::Relaxed);
        st.family.store(1, Ordering::Relaxed);
        st.bandwidth.store(30.0f32.to_bits(), Ordering::Relaxed);
        st.sample_rate.store(44100, Ordering::Relaxed);
        let docs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
        std::fs::create_dir_all(&docs).unwrap();
        for (theme, file) in [(0u8, docs.join("roger.png")), (1, docs.join("roger-classic.png"))] {
            *params.theme.write().unwrap() = theme;
            let mut editor = crate::RogerPlugin::editor(params.clone());
            if let Some((px, w, h)) = editor.screenshot(params.clone()) {
                truce_test::save_png(&file, &px, w, h);
            }
        }
    }
}
