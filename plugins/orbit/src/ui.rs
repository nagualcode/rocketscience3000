//! The editor, after the 2001 original (interface by Oskar Luyer): the room
//! seen from above — the outer room (tiled), the listener's inner room (light
//! blue, its two "ears" a speaker span apart), the red ball of the source
//! with its trail — and the copper control panel.
//!
//! Click or drag in the room to send the source there (Static and Follow
//! modes); drag the inner room to move the listener. The faint dots are the
//! image sources of the reflections, mirrored in the walls up to the
//! reflection order.

use crate::engine::image_sources;
use crate::params::{Motion, OrbitParams, P};
use egui::{pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke};
use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use truce::prelude::*;
use truce_core::editor::PluginContext;
use truce_egui::widgets::{level_meter, param_knob};

pub const WINDOW_W: u32 = 960;
pub const WINDOW_H: u32 = 640;
pub const MIN_W: u32 = 820;
pub const MIN_H: u32 = 560;
pub const MAX_W: u32 = 1900;
pub const MAX_H: u32 = 1300;

type Ctx = PluginContext<OrbitParams>;

#[derive(Clone, Copy)]
struct Palette {
    bg: Color32,
    panel: Color32,
    screen: Color32,
    tile: Color32,
    inner: Color32,
    text: Color32,
    dim: Color32,
    ball: Color32,
    accent: Color32,
    accent_text: Color32,
    header: Color32,
}

const MODERN: Palette = Palette {
    bg: Color32::from_rgb(24, 27, 32),
    panel: Color32::from_rgb(40, 44, 52),
    screen: Color32::from_rgb(14, 18, 24),
    tile: Color32::from_rgb(36, 44, 54),
    inner: Color32::from_rgb(90, 170, 230),
    text: Color32::from_rgb(216, 220, 226),
    dim: Color32::from_rgb(130, 138, 150),
    ball: Color32::from_rgb(240, 70, 60),
    accent: Color32::from_rgb(240, 160, 70),
    accent_text: Color32::from_rgb(30, 18, 6),
    header: Color32::from_rgb(52, 56, 64),
};

/// The original: a teal sky, a copper panel, a pale-blue room.
const CLASSIC: Palette = Palette {
    bg: Color32::from_rgb(28, 110, 140),
    panel: Color32::from_rgb(176, 110, 64),
    screen: Color32::from_rgb(196, 226, 244),
    tile: Color32::from_rgb(160, 196, 220),
    inner: Color32::from_rgb(60, 140, 210),
    text: Color32::from_rgb(250, 240, 226),
    dim: Color32::from_rgb(244, 220, 190),
    ball: Color32::from_rgb(220, 30, 30),
    accent: Color32::from_rgb(40, 220, 240),
    accent_text: Color32::from_rgb(10, 30, 40),
    header: Color32::from_rgb(120, 70, 36),
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
        (&mut v.widgets.hovered, p.tile),
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
    ui.label(egui::RichText::new(text).size(11.5).color(p.dim).strong());
}

fn pill(ui: &mut egui::Ui, p: &Palette, text: &str, on: bool) -> egui::Response {
    let (fill, fg) = if on { (p.accent, p.accent_text) } else { (p.header, p.text) };
    ui.add(egui::Button::new(egui::RichText::new(text).size(11.5).color(fg)).fill(fill).min_size(vec2(24.0, 22.0)))
}

struct State {
    trail: VecDeque<(f32, f32)>,
    drag: Option<Grab>,
}

#[derive(Clone, Copy)]
enum Grab {
    Source,
    Listener,
}

type Shared = Arc<Mutex<State>>;

pub fn orbit_ui(ui: &mut egui::Ui, ctx: &Ctx) {
    let params: &Arc<OrbitParams> = ctx.params();
    let classic = params.theme.read().map(|t| *t == 1).unwrap_or(false);
    let p = if classic { CLASSIC } else { MODERN };
    let v = visuals(&p);
    ui.ctx().set_visuals(v.clone());
    ui.style_mut().visuals = v;

    egui::Panel::top("orbit-top")
        .exact_size(38.0)
        .frame(egui::Frame::NONE.fill(p.header).inner_margin(egui::Margin::symmetric(12, 8)))
        .show_inside(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(egui::RichText::new("ORBIT 3K").size(17.0).strong().italics().color(p.text));
                ui.label(egui::RichText::new("psycho-acoustic flight path simulator · rocketscience3000").size(10.5).color(p.dim));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if pill(ui, &p, if classic { "Classic" } else { "Modern" }, false).on_hover_text("Theme").clicked() {
                        if let Ok(mut t) = params.theme.write() {
                            *t = if classic { 0 } else { 1 };
                        }
                    }
                });
            });
        });

    let id = egui::Id::new("orbit-state");
    let shared: Shared = ui.data_mut(|d| d.get_temp_mut_or_insert_with(id, || Arc::new(Mutex::new(State { trail: VecDeque::new(), drag: None }))).clone());
    let mut state = shared.lock().unwrap();

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(p.bg).inner_margin(egui::Margin::same(12)))
        .show_inside(ui, |ui| {
            let h = ui.available_height();
            ui.horizontal_top(|ui| {
                let side = 300.0;
                let w = ui.available_width() - side - ui.spacing().item_spacing.x;
                let s = w.min(h);
                room(ui, ctx, &p, &mut state, s);
                egui::Frame::NONE.fill(p.panel).corner_radius(8.0).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                    ui.set_min_size(vec2(side - 24.0, h - 24.0));
                    ui.set_max_size(vec2(side - 24.0, h - 24.0));
                    ui.vertical(|ui| controls(ui, ctx, &p));
                });
            });
        });
    drop(state);
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
}

fn room(ui: &mut egui::Ui, ctx: &Ctx, p: &Palette, state: &mut State, size: f32) {
    let params = ctx.params();
    let st = &params.status;
    let ld = |a: &std::sync::atomic::AtomicU32| f32::from_bits(a.load(Ordering::Relaxed));
    let (room_m, ix, iy, span) = (ld(&st.room), ld(&st.inner_x), ld(&st.inner_y), ld(&st.span));
    let order = st.order.load(Ordering::Relaxed) as i32;
    let src = (ld(&st.source_x), ld(&st.source_y));
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6.0, p.screen);
    // The outer room in the middle third; around it the mirrored rooms of
    // the first reflections, so images are visible.
    let r = Rect::from_center_size(rect.center(), vec2(size / 3.0, size / 3.0));
    // −1…1 → screen (y up, as in the original: "forward" is up).
    let to = |x: f32, y: f32| pos2(r.center().x + x * r.width() * 0.5, r.center().y - y * r.height() * 0.5);
    let from = |q: Pos2| ((q.x - r.center().x) / (r.width() * 0.5), -(q.y - r.center().y) / (r.height() * 0.5));
    for i in -1..=1 {
        for j in -1..=1 {
            let cell = r.translate(vec2(i as f32 * r.width(), j as f32 * r.height()));
            painter.rect_stroke(cell, 0.0, Stroke::new(1.0_f32, alpha(p.tile, if i == 0 && j == 0 { 1.0 } else { 0.4 })), egui::StrokeKind::Inside);
        }
    }
    // Tiles of the outer room.
    let tiles = 8;
    for k in 1..tiles {
        let t = k as f32 / tiles as f32;
        painter.line_segment([pos2(r.left() + t * r.width(), r.top()), pos2(r.left() + t * r.width(), r.bottom())], Stroke::new(1.0_f32, alpha(p.tile, 0.5)));
        painter.line_segment([pos2(r.left(), r.top() + t * r.height()), pos2(r.right(), r.top() + t * r.height())], Stroke::new(1.0_f32, alpha(p.tile, 0.5)));
    }
    painter.rect_stroke(r, 0.0, Stroke::new(2.0_f32, p.tile), egui::StrokeKind::Outside);

    // Image sources (metres, origin at a corner; the original's y grows the
    // same way as its −1…1 parameters).
    let m = |v: f32| (v + 1.0) * room_m * 0.5;
    for (x, y, o) in image_sources(m(src.0) as f64, m(src.1) as f64, room_m as f64, order) {
        let q = to((x as f32 / room_m) * 2.0 - 1.0, (y as f32 / room_m) * 2.0 - 1.0);
        if rect.contains(q) {
            painter.circle_filled(q, 3.0, alpha(p.ball, 0.55 / o as f32));
        }
    }

    // Ellipse of the orbit modes.
    let mode = params.mode.index() as usize;
    if mode >= 2 {
        let (cx, cy, rx, ry) = (params.pos_x.value(), params.pos_y.value(), params.rad_x.value(), params.rad_y.value());
        let pts: Vec<Pos2> = (0..=72).map(|k| {
            let a = k as f32 / 72.0 * std::f32::consts::TAU;
            to(cx + rx * a.cos(), cy + ry * a.sin())
        }).collect();
        painter.add(egui::Shape::line(pts, Stroke::new(1.0_f32, alpha(Color32::WHITE, if mode == 4 { 0.25 } else { 0.6 }))));
    }
    // Target of Static / Follow.
    if mode <= 1 {
        let q = to(params.x.value(), params.y.value());
        painter.circle_stroke(q, 9.0, Stroke::new(1.5_f32, alpha(p.ball, 0.7)));
    }

    // The listener's inner room and ears.
    let half = (span / room_m).min(1.0);
    let inner = Rect::from_center_size(to(ix, iy), vec2(half * r.width(), half * r.height()));
    painter.rect_filled(inner, 2.0, alpha(p.inner, 0.45));
    painter.rect_stroke(inner, 2.0, Stroke::new(1.5_f32, p.inner), egui::StrokeKind::Outside);
    let ear_y = iy + half;
    for ex in [ix - half, ix + half] {
        painter.circle_filled(to(ex, ear_y), 4.0, p.inner);
    }

    // Trail and ball.
    state.trail.push_back(src);
    while state.trail.len() > 90 {
        state.trail.pop_front();
    }
    let n = state.trail.len();
    for (k, w) in state.trail.iter().collect::<Vec<_>>().windows(2).enumerate() {
        let a = k as f32 / n as f32;
        painter.line_segment([to(w[0].0, w[0].1), to(w[1].0, w[1].1)], Stroke::new(2.0_f32, alpha(p.ball, 0.6 * a)));
    }
    let ball = to(src.0, src.1);
    painter.circle_filled(ball, 8.0, p.ball);
    painter.circle_stroke(ball, 8.0, Stroke::new(1.0_f32, Color32::BLACK));
    let mx = |v: f32| (v + 1.0) * room_m * 0.5;
    painter.text(rect.left_bottom() + vec2(8.0, -6.0), Align2::LEFT_BOTTOM,
        format!("source x {:.1} m · y {:.1} m · room {:.0} m", mx(src.0), mx(src.1), room_m), FontId::monospace(10.5), alpha(if p.screen.r() > 128 { Color32::BLACK } else { Color32::WHITE }, 0.7));

    // Interaction.
    if let Some(pt) = resp.interact_pointer_pos() {
        if state.drag.is_none() && (resp.drag_started() || resp.clicked() || resp.is_pointer_button_down_on()) {
            let grab = if inner.expand(6.0).contains(pt) { Grab::Listener } else { Grab::Source };
            match grab {
                Grab::Source => {
                    ctx.begin_edit(P::X);
                    ctx.begin_edit(P::Y);
                }
                Grab::Listener => {
                    ctx.begin_edit(P::InnerX);
                    ctx.begin_edit(P::InnerY);
                }
            }
            state.drag = Some(grab);
        }
        let (x, y) = from(pt);
        let (x, y) = (x.clamp(-1.0, 1.0) as f64, y.clamp(-1.0, 1.0) as f64);
        match state.drag {
            Some(Grab::Source) => {
                ctx.set_param(P::X, normalized(ctx, P::X, x));
                ctx.set_param(P::Y, normalized(ctx, P::Y, y));
            }
            Some(Grab::Listener) => {
                ctx.set_param(P::InnerX, normalized(ctx, P::InnerX, x));
                ctx.set_param(P::InnerY, normalized(ctx, P::InnerY, y));
            }
            None => {}
        }
    }
    if let Some(g) = state.drag {
        if resp.drag_stopped() || !resp.is_pointer_button_down_on() {
            let ids = match g {
                Grab::Source => [P::X, P::Y],
                Grab::Listener => [P::InnerX, P::InnerY],
            };
            for id in ids {
                ctx.end_edit(id);
            }
            state.drag = None;
        }
    }
    resp.on_hover_text("Click or drag: where the source goes (Static and Follow modes). Drag the blue inner room: the listener.");
}

fn controls(ui: &mut egui::Ui, ctx: &Ctx, p: &Palette) {
    let params = ctx.params();
    label(ui, p, "MOTION");
    let names = Motion::variant_names();
    let cur = params.mode.index() as usize;
    egui::ComboBox::from_id_salt("orbit-mode").width(200.0).selected_text(names[cur]).show_ui(ui, |ui| {
        for (i, n) in names.iter().enumerate() {
            if ui.selectable_label(i == cur, *n).clicked() && i != cur {
                ctx.automate(P::Mode, normalized(ctx, P::Mode, i as f64));
            }
        }
    });
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        param_knob(ui, ctx, P::Speed, "speed");
        param_knob(ui, ctx, P::Damp, "high damp");
    });
    ui.horizontal(|ui| {
        param_knob(ui, ctx, P::Room, "room size");
        param_knob(ui, ctx, P::Span, "speaker span");
    });
    ui.add_space(4.0);
    label(ui, p, "REFLECTIONS");
    ui.horizontal(|ui| {
        let cur = params.order.value();
        for o in 0..=5 {
            if pill(ui, p, &o.to_string(), o == cur).clicked() {
                ctx.automate(P::Order, normalized(ctx, P::Order, o as f64));
            }
        }
    });
    if cur >= 2 {
        ui.add_space(4.0);
        label(ui, p, "ELLIPSE");
        ui.horizontal(|ui| {
            param_knob(ui, ctx, P::RadX, "radius x");
            param_knob(ui, ctx, P::RadY, "radius y");
        });
        ui.horizontal(|ui| {
            param_knob(ui, ctx, P::PosX, "centre x");
            param_knob(ui, ctx, P::PosY, "centre y");
        });
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        param_knob(ui, ctx, P::Output, "output");
        level_meter(ui, ctx, &[P::MeterL, P::MeterR], 70.0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cargo test --release -p orbit --lib readme_screenshots -- --ignored`
    #[test]
    #[ignore]
    fn readme_screenshots() {
        let params = Arc::new(OrbitParams::new());
        params.order.set_value(2);
        let st = &params.status;
        st.source_x.store(0.55f32.to_bits(), Ordering::Relaxed);
        st.source_y.store(0.95f32.to_bits(), Ordering::Relaxed);
        st.order.store(2, Ordering::Relaxed);
        let docs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs");
        std::fs::create_dir_all(&docs).unwrap();
        for (theme, file) in [(0u8, docs.join("orbit.png")), (1, docs.join("orbit-classic.png"))] {
            *params.theme.write().unwrap() = theme;
            let mut editor = crate::OrbitPlugin::editor(params.clone());
            if let Some((px, w, h)) = editor.screenshot(params.clone()) {
                truce_test::save_png(&file, &px, w, h);
            }
        }
    }
}
