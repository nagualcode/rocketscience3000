//! The Orbit engine, translated from the i386 slice of `Orbit.vst` 3.6.2
//! (`OrbitProcess::Process` @ 0x263da; see `reverse_engineering/orbit/`).
//!
//! F. R. Moore's image-source model of a sound moving in a room: the source
//! is mirrored in the walls of a square "outer room" up to the fifth order;
//! every image reaches each ear of a listener (the "inner room", two points a
//! speaker span apart) with the delay of its distance at 340 m/s and a gain
//! falling as distance^−1.2, 0.905 per reflection, halved behind the head.
//! Each host buffer, every tap glides from its last delay and gain to the new
//! one by resampling the block — the Doppler shift. Reflections can be
//! low-passed (High Damp). The source moves on its own: still, following
//! clicks, around an ellipse either way, or at random.

use mission::ring::Ring;

/// The controls (`OrbitInfo::GetParameter` @ 0x2398a), original ranges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Controls {
    /// 2 "Room Size", 8…80 m.
    pub room: f32,
    /// 3 "Reflection Order", 0…5.
    pub order: i32,
    /// 4 / 5 "Inner Room Y / X Pos", −1…1 (the listener).
    pub inner_y: f32,
    pub inner_x: f32,
    /// 6 "High Damp", 0…100.
    pub damp: i32,
    /// 7 "Speed", 0…99 m/s.
    pub speed: f32,
    /// 8 "Mode": 1 static, 2 follow clicks, 3 ellipse, 4 ellipse backwards, 5 random.
    pub mode: i32,
    /// 9 / 10 ellipse radius, 11 / 12 ellipse position.
    pub rad_x: f32,
    pub rad_y: f32,
    pub pos_x: f32,
    pub pos_y: f32,
    /// 13 "Speaker span", 0.2…6 m.
    pub span: f32,
    /// 15 / 16 "X / Y Position": where to go (modes 1 and 2).
    pub x: f32,
    pub y: f32,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            room: 25.0,
            order: 2,
            inner_y: -0.6,
            inner_x: 0.0,
            damp: 0,
            speed: 3.0,
            mode: 3,
            rad_x: 0.8,
            rad_y: 0.8,
            pos_x: 0.0,
            pos_y: 0.5,
            span: 3.0,
            x: 0.0,
            y: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// One input to two outputs.
    MonoToStereo,
    /// Two inputs, summed, to two outputs.
    Stereo,
}

/// π and 2π of the source (…6528, …3056) and their float twins.
const PI_TYPO: f64 = 3.1415926528;
const TWO_PI_TYPO: f64 = 6.2831853056;
/// Taps used for reflection orders 0…8 (`1 + 2n(n+1)`).
const TAPS: [usize; 9] = [1, 5, 13, 25, 41, 61, 85, 113, 145];
const MAX_TAPS: usize = 0x91;

/// One phantom (image) source (`ps`, 0x58 bytes).
#[derive(Clone, Copy, Default, Debug)]
struct Phantom {
    x: f64,
    y: f64,
    t_right: f64,
    t_left: f64,
    g_left: f64,
    g_right: f64,
    order: i16,
}

/// `CalcPhantomSourceCoordinates` @ 0x21336: the 4k images of order k.
fn phantoms_of_order(x: f64, y: f64, w: f64, h: f64, k: i16, ps: &mut Vec<Phantom>) {
    let kk = k as i32;
    let mut flip = true;
    let push = |ps: &mut Vec<Phantom>, px: f64, py: f64| ps.push(Phantom { x: px, y: py, order: k, ..Default::default() });
    let even = k & 1 == 0;
    // Each quadrant: the x index runs one way, y the complement.
    let emit = |ps: &mut Vec<Phantom>, i: i32, j: i32, flip: bool| {
        let (px, py) = if flip {
            let py = if even { j as f64 * h + (h - y) } else { j as f64 * h + y };
            (i as f64 * w + (w - x), py)
        } else {
            let py = if even { j as f64 * h + y } else { j as f64 * h + (h - y) };
            (i as f64 * w + x, py)
        };
        push(ps, px, py);
    };
    for s in 0..kk {
        flip = !flip;
        emit(ps, s, kk - s, flip);
    }
    for s in (1..=kk).rev() {
        flip = !flip;
        emit(ps, s, s - kk, flip);
    }
    let mut s = 0;
    while -kk < s {
        flip = !flip;
        emit(ps, s, -kk - s, flip);
        s -= 1;
    }
    for s in -kk..0 {
        flip = !flip;
        emit(ps, s, s + kk, flip);
    }
}

/// `MakePhantomSourceList` @ 0x21b48 with `CalcTaps` and `KillAbsorbedPS`.
#[allow(clippy::too_many_arguments)]
fn phantom_list(sx: f64, sy: f64, lx: f64, ly: f64, rx: f64, ry: f64, w: f64, h: f64) -> Vec<Phantom> {
    let att = (2.0f64 / -20.0).exp();
    let mut ps = Vec::with_capacity(64);
    ps.push(Phantom { x: sx, y: sy, ..Default::default() });
    for k in 1..=5i16 {
        phantoms_of_order(sx, sy, w, h, k, &mut ps);
    }
    for p in ps.iter_mut() {
        let (a, b) = (p.x - lx, ly - p.y);
        let (c, d) = (p.x - rx, ry - p.y);
        let dl = ((a * a + b * b).sqrt() as f32) as f64 + 1.0;
        let dr = ((c * c + d * d).sqrt() as f32) as f64 + 1.0;
        p.t_left = dl / 340.0;
        p.t_right = dr / 340.0;
        p.g_left = dl.powf(-1.2);
        p.g_right = dr.powf(-1.2);
        for _ in 0..p.order {
            p.g_left *= att;
            p.g_right *= att;
        }
    }
    for p in ps.iter_mut() {
        if lx < p.x && p.y < ly {
            p.g_left *= 0.5;
        }
        if p.x < rx && p.y < ry {
            p.g_right *= 0.5;
        }
    }
    ps
}

#[derive(Clone, Copy, Default, Debug)]
struct Tap {
    delay: i32,
    gain: f32,
    order: i32,
}

/// `OrbitCBRightLooking`: the multi-tapped delay line.
struct Taps {
    /// Two pairs of tap lists (left, right), written alternately.
    lists: [[[Tap; MAX_TAPS]; 2]; 2],
    new: usize,
    fresh: bool,
    interpolate: usize,
    left: Ring,
    right: Ring,
    raw: Vec<f32>,
    damped: Vec<f32>,
    out_l: Vec<f32>,
    out_r: Vec<f32>,
    /// One-pole low-pass for the reflections: `y = a·x − b·y[n−1]`.
    a: f32,
    b: f32,
    y1: f32,
}

impl Taps {
    /// `Init(len, buffer)` @ 0x2280e.
    fn new(len: usize, buffer: usize) -> Self {
        Self {
            lists: [[[Tap::default(); MAX_TAPS]; 2]; 2],
            new: 0,
            fresh: false,
            interpolate: 0,
            left: Ring::new(buffer * 3 + len),
            right: Ring::new(buffer * 3 + len),
            raw: vec![0.0; buffer],
            damped: vec![0.0; buffer],
            out_l: vec![0.0; len],
            out_r: vec![0.0; len],
            a: 1.0,
            b: 0.0,
            y1: 0.0,
        }
    }

    /// `SetMooreParms` @ 0x21cf6: new tap lists from the phantom sources.
    #[allow(clippy::too_many_arguments)]
    fn set(&mut self, sx: f64, sy: f64, lx: f64, ly: f64, rx: f64, ry: f64, w: f64, h: f64, sr: f64) {
        let ps = phantom_list(sx, sy, lx, ly, rx, ry, w, h);
        self.new ^= 1;
        let [l, r] = &mut self.lists[self.new];
        for (i, p) in ps.iter().enumerate() {
            l[i] = Tap { delay: (sr * p.t_left) as i32, gain: (p.g_left * 5.0) as f32, order: p.order as i32 };
            r[i] = Tap { delay: (sr * p.t_right) as i32, gain: (p.g_right * 5.0) as f32, order: p.order as i32 };
        }
        for i in ps.len()..MAX_TAPS {
            l[i].delay = 0;
            l[i].gain = 0.0;
            r[i].delay = 0;
            r[i].gain = 0.0;
        }
        self.fresh = true;
    }

    /// `PutMultiTappedStereo` @ 0x21f5a and `GetMoore` @ 0x21f12.
    fn run(&mut self, input: &[f32], count: usize, out_l: &mut [f32], out_r: &mut [f32]) {
        let n = input.len();
        if self.raw.len() < n {
            self.raw.resize(n, 0.0);
            self.damped.resize(n, 0.0);
        }
        self.raw[..n].copy_from_slice(input);
        if self.a == 1.0 {
            self.damped[..n].copy_from_slice(input);
            self.y1 = input[n - 1];
        } else {
            let mut y1 = self.y1;
            for i in 0..n {
                let y = self.a * self.raw[i] - y1 * self.b;
                self.damped[i] = y;
                y1 = y;
            }
            self.y1 = y1;
        }
        let new = self.new;
        let old = new ^ 1;
        let mut done = 0;
        if self.fresh && self.interpolate >= 1 {
            for t in 0..count {
                let src: &[f32] = if self.lists[old][0][t].order == 0 { &self.raw } else { &self.damped };
                for side in 0..2 {
                    let (from, to) = (self.lists[old][side][t], self.lists[new][side][t]);
                    let (ring, buf) = if side == 0 { (&mut self.left, &mut self.out_l) } else { (&mut self.right, &mut self.out_r) };
                    let m = n as i32 + to.delay - from.delay;
                    if m <= 0 {
                        continue;
                    }
                    let mm = m as usize;
                    if buf.len() < mm {
                        buf.resize(mm, 0.0);
                    }
                    resample(src, from, to, mm, buf);
                    ring.offset_put(from.delay);
                    ring.add_fast(&buf[..mm]);
                    ring.offset_put(-(from.delay + m));
                }
            }
            self.fresh = false;
            done = self.interpolate;
        }
        if done < n {
            for t in 0..count {
                let src: &[f32] = if self.lists[old][0][t].order == 0 { &self.raw } else { &self.damped };
                for side in 0..2 {
                    let to = self.lists[new][side][t];
                    let (ring, buf) = if side == 0 { (&mut self.left, &mut self.out_l) } else { (&mut self.right, &mut self.out_r) };
                    if buf.len() < n {
                        buf.resize(n, 0.0);
                    }
                    for i in 0..n {
                        buf[i] = src[i] * to.gain;
                    }
                    ring.offset_put(to.delay);
                    ring.add_fast(&buf[..n]);
                    ring.offset_put(-(n as i32 + to.delay));
                }
            }
        }
        self.left.offset_put(n as i32);
        self.right.offset_put(n as i32);
        self.left.get_and_clear(out_l);
        self.right.get_and_clear(out_r);
    }
}

/// The Doppler resampler: the block stretched to `m` samples while the
/// gain glides; one read position (and weight pair) per two outputs.
fn resample(src: &[f32], from: Tap, to: Tap, m: usize, out: &mut [f32]) {
    let inv = 1.0 / m as i32 as f64;
    let step = (1.0 - (to.delay as f64 - from.delay as f64) * inv) * 2.0;
    let gstep = (to.gain as f64 - from.gain as f64) * inv * 2.0;
    let mut g = from.gain as f64;
    let mut pos = 0.0f64;
    let mut frac = 0.0f64;
    let mut idx = 0usize;
    let at = |i: usize| src.get(i).copied().unwrap_or(0.0);
    let mut o = 0;
    for _ in 0..m / 2 {
        let (x0, x1, x2) = (at(idx), at(idx + 1), at(idx + 2));
        let a = (frac * g) as f32;
        let b = (g - a as f64) as f32;
        g += gstep;
        out[o] = b * x0 + a * x1;
        out[o + 1] = x1 * b + a * x2;
        o += 2;
        pos += step;
        idx = pos as i32 as usize;
        frac = pos - idx as i32 as f64;
    }
    if m & 1 != 0 {
        let a = (frac * g) as f32;
        out[o] = (g - a as f64) as f32 * at(idx) + a * at(idx + 1);
    }
}

/// `ManMetHond` ("man with dog"): the path follower. The man walks a
/// cosine-eased, corner-cutting path through the targets (the "dogs") he is
/// given, one leg per `leg` buffers.
#[derive(Clone)]
pub struct Walker {
    p0: f32,
    p1: f32,
    leg: f32,
    pts: [(f32, f32); 3],
    dogs: std::collections::VecDeque<(f32, f32)>,
}

impl Walker {
    fn new() -> Self {
        Self { p0: 1.0, p1: 1.0, leg: 60.0, pts: [(0.0, 0.0); 3], dogs: Default::default() }
    }

    /// `DaarIsHond`: a new target (20 at most).
    fn dog(&mut self, x: f32, y: f32) {
        if self.dogs.len() < 20 {
            self.dogs.push_back((x, y));
        }
    }

    /// `WaarIsMan`: where he is, then one step on.
    fn step(&mut self) -> (f32, f32) {
        let leg = self.leg;
        if 0.5 < self.p1 {
            if let Some(d) = self.dogs.pop_front() {
                self.p0 = self.p1;
                self.p1 = 0.0;
                self.pts = [self.pts[1], self.pts[2], d];
            }
        }
        let t0 = ((self.p0 as f64 * PI_TYPO).cos() * -0.5 + 0.5) as f32;
        let t1 = ((self.p1 as f64 * PI_TYPO).cos() * -0.5 + 0.5) as f32;
        let [a, b, c] = self.pts;
        let x = a.0 + ((b.0 + (c.0 - b.0) * t1) - a.0) * t0;
        let y = a.1 + t0 * ((b.1 + t1 * (c.1 - b.1)) - a.1);
        for p in [&mut self.p0, &mut self.p1] {
            if *p < 1.0 {
                *p += 1.0 / leg;
            }
            if 1.0 < *p {
                *p = 1.0;
            }
        }
        (x, y)
    }

    /// `Aai` ("pat"): stand here, forget the targets.
    fn stay(&mut self, x: f32, y: f32) {
        self.dogs.clear();
        self.pts = [(x, y); 3];
        self.p0 = 1.0;
        self.p1 = 1.0;
    }
}

pub struct Orbit {
    pub layout: Layout,
    sample_rate: f64,
    taps: Taps,
    walker: Walker,
    /// The source, −1…1 (parameters 0 and 1, written by the motion).
    pub source: (f32, f32),
    angle: f32,
    seed: u32,
    last_target: (f32, f32),
    /// Last geometry (`this + 0xc …`), to notice changes in static mode.
    last: (f32, f32, f32, i32, f32, f32, f32),
    changed: bool,
    tap_count: usize,
    mono: Vec<f32>,
}

impl Orbit {
    /// `InitBuffers` @ 0x25926 (`buffer` = the host buffer size).
    pub fn new(sample_rate: f64, layout: Layout, buffer: usize, c: &Controls, source: (f32, f32)) -> Self {
        let n = buffer as i32;
        let room_max = 80.0f32;
        let blocks = ((((room_max * 6.0) as f64 * sample_rate) / 340.0) as f32 / n as f32 + 5.0) as i32;
        let mut taps = Taps::new((n * blocks) as usize, buffer);
        let room = c.room;
        let sx = room * (source.0 + 1.0) * 0.5;
        let sy = room * (source.1 + 1.0) * 0.5;
        let iy = room * (c.inner_y + 1.0) * 0.5;
        let span = c.span;
        let ear_y = (iy + span * 0.5) as f64;
        taps.set(sx as f64, sy as f64, (room * 0.5 + span * -0.5) as f64, ear_y, (span * 0.5 + room * 0.5) as f64, ear_y, room as f64, room as f64, sample_rate);
        Self {
            layout,
            sample_rate,
            taps,
            walker: Walker::new(),
            source,
            angle: 0.0,
            seed: 0,
            last_target: (0.0, 0.0),
            // `this + 0xc` is set to −1 after SetMooreParms: the first Process
            // always sees a change.
            last: (-1.0, sy, room, 0, c.damp as f32, iy, span),
            changed: false,
            tap_count: 1,
            mono: vec![0.0; buffer],
        }
    }

    /// `MoveSource` @ 0x25d2a (precision as in the asm: Ghidra shows floats
    /// where the code works in double).
    fn move_source(&mut self, c: &Controls, n: usize) {
        let mode = c.mode as f32 as i32;
        let speed = (((c.speed * 10.0) as f64) * 0.1) as f32;
        let room = c.room;
        let sr = self.sample_rate;
        let leg = || ((((room as f64 * 0.5) / speed as f64) * sr) / n as u32 as f64) as f32;
        match mode {
            3 | 4 => {
                let w = if c.rad_x == 0.0 && c.rad_y == 0.0 {
                    0.0
                } else {
                    let r = if c.rad_x > c.rad_y { c.rad_x } else { c.rad_y };
                    let v = if mode == 3 { speed } else { -speed };
                    v / (r * room)
                };
                let nf = (n as u32 >> 16) as f32 * 65536.0 + (n as u32 & 0xffff) as f32;
                let mut d = ((nf * w) as f64 / sr) as f32;
                if d as f64 > PI_TYPO {
                    d = 3.141_592_7;
                }
                if (d as f64) < -PI_TYPO {
                    d = -3.141_592_7;
                }
                self.angle += d;
                if self.angle as f64 >= TWO_PI_TYPO {
                    self.angle = (self.angle as f64 - TWO_PI_TYPO) as f32;
                }
                let mut ad = self.angle as f64;
                if ad < -TWO_PI_TYPO {
                    self.angle = (ad + TWO_PI_TYPO) as f32;
                    ad = self.angle as f64;
                }
                let x = (c.pos_x as f64 + c.rad_x as f64 * ad.cos()) as f32;
                let y = (c.pos_y as f64 + c.rad_y as f64 * ad.sin()) as f32;
                self.set_source(x, y);
                self.walker.stay(x, y);
            }
            5 => {
                self.walker.leg = leg();
                self.walker.dogs.clear();
                let r1 = self.seed.wrapping_mul(0x19660d).wrapping_add(0x3c6ef35f);
                let r2 = r1.wrapping_mul(0x19660d).wrapping_add(0x3c6ef35f);
                self.seed = r2;
                let unit = |r: u32| (r.wrapping_sub(0x8000_0000) as i32) as f64 + 2147483648.0;
                let u = (unit(r2) / 4294967295.0) as f32;
                let ang = ((unit(r1) * TWO_PI_TYPO / 4294967295.0) as f32) as f64;
                let y = (c.pos_y as f64 + (c.rad_y * u) as f64 * ang.sin()) as f32;
                let x = ((u * c.rad_x) as f64 * ang.cos() + c.pos_x as f64) as f32;
                self.walker.dog(x, y);
                let (x, y) = self.walker.step();
                self.set_source(x, y);
            }
            1 => {
                self.set_source(c.x, c.y);
                self.walker.stay(c.x, c.y);
            }
            2 => {
                self.walker.leg = leg();
                if c.y != self.last_target.1 || c.x != self.last_target.0 {
                    self.walker.dog(c.x, c.y);
                    self.last_target = (c.x, c.y);
                }
                let (x, y) = self.walker.step();
                self.set_source(x, y);
            }
            _ => {}
        }
    }

    /// `SetParamFloat(0 / 1)`: the parameter object ignores values outside
    /// −1…1 (it does not clamp), so the source stops at the room's edge.
    fn set_source(&mut self, x: f32, y: f32) {
        if (-1.0..=1.0).contains(&x) {
            self.source.0 = x;
        }
        if (-1.0..=1.0).contains(&y) {
            self.source.1 = y;
        }
    }

    /// One `Process` call: `l` (and `r` in stereo) in, both out.
    pub fn process(&mut self, c: &Controls, l: &mut [f32], r: &mut [f32]) {
        let n = l.len();
        self.move_source(c, n);
        if self.mono.len() < n {
            self.mono.resize(n, 0.0);
        }
        self.mono[..n].copy_from_slice(l);
        if self.layout == Layout::Stereo {
            for i in 0..n {
                self.mono[i] += r[i];
            }
        }
        let room = c.room;
        let rd = room as f64;
        let sx = (((self.source.0 as f64) + 1.0) * rd * 0.5) as f32;
        let sy = (((self.source.1 as f64) + 1.0) * rd * 0.5) as f32;
        let iy = (((c.inner_y as f64) + 1.0) * rd * 0.5) as f32;
        let ix = (((c.inner_x as f64) + 1.0) * rd * 0.5) as f32;
        let damp = c.damp as f32;
        let span = c.span;
        let now = (sx, sy, room, c.order, damp, iy, span);
        if now != self.last {
            self.last = now;
            self.changed = true;
        }
        if c.mode != 1 || self.changed {
            let ear_y = (iy + span * 0.5) as f64;
            self.taps.set(sx as f64, sy as f64, (ix + span * -0.5) as f64, ear_y, (span * 0.5 + ix) as f64, ear_y, rd, rd, self.sample_rate);
            let a = (100.0 - damp as f64) / 100.0;
            let a = (a * a) as f32;
            self.taps.a = a;
            self.taps.b = a - 1.0;
            self.tap_count = TAPS.get(c.order.max(0) as usize).copied().unwrap_or(1);
            if !(1..=8).contains(&c.order) {
                self.tap_count = 1;
            }
            self.changed = false;
        }
        self.taps.interpolate = n;
        let input = std::mem::take(&mut self.mono);
        self.taps.run(&input[..n], self.tap_count, l, r);
        self.mono = input;
    }
}

/// Sets a −1…1 parameter as the original's parameter objects do: values
/// outside the range are ignored.
fn set_in_range(p: &mut f32, v: f32, lo: f32, hi: f32) {
    if lo <= v && v <= hi {
        *p = v;
    }
}

/// `OrbitProcess::ReceiveMidi` @ 0x25620. CCs: 1 Y target, 11 room size,
/// 12 / 13 listener Y / X, 14 high damp, 15 speed, 16 / 17 ellipse radius,
/// 18 / 19 ellipse position, 20 speaker span — `min + value/128·range`;
/// notes: X target from `(note − 36) mod 60` and Y from the velocity; pitch
/// bend: X; program change: motion mode `program mod 5 + 1`.
pub fn receive_midi(c: &mut Controls, status: u8, d1: u8, d2: u8) {
    let lin = |v: u8, lo: f32, hi: f32| lo + (v as f32 / 128.0) * (hi - lo);
    match status & 0xf0 {
        0x90 if d2 != 0 => {
            let k = (d1 as i32 - 36) % 60;
            set_in_range(&mut c.x, -1.0 + (k as f32 * 2.0) / 60.0, -1.0, 1.0);
            set_in_range(&mut c.y, -1.0 + (d2 as f32 / 128.0) * 2.0, -1.0, 1.0);
        }
        0xe0 => {
            let b = (d2 as u32) << 7 | d1 as u32;
            set_in_range(&mut c.x, -1.0 + (b as f32 / 16384.0) * 2.0, -1.0, 1.0);
        }
        0xc0 => c.mode = (d1 % 5) as i32 + 1,
        0xb0 => match d1 {
            1 => c.y = lin(d2, -1.0, 1.0),
            11 => c.room = lin(d2, 8.0, 80.0),
            12 => c.inner_y = lin(d2, -1.0, 1.0),
            13 => c.inner_x = lin(d2, -1.0, 1.0),
            14 => c.damp = lin(d2, 0.0, 100.0) as i32,
            15 => c.speed = lin(d2, 0.0, 99.0),
            16 => c.rad_x = lin(d2, 0.0, 1.0),
            17 => c.rad_y = lin(d2, 0.0, 1.0),
            18 => c.pos_x = lin(d2, -1.0, 1.0),
            19 => c.pos_y = lin(d2, -1.0, 1.0),
            20 => c.span = lin(d2, 0.2, 6.0),
            _ => {}
        },
        _ => {}
    }
}

/// Image sources up to `order` for a source at (x, y) metres in a square
/// room of side `room`, with their reflection order (for the editor).
pub fn image_sources(x: f64, y: f64, room: f64, order: i32) -> Vec<(f64, f64, i16)> {
    let mut ps = Vec::new();
    for k in 1..=order.clamp(0, 5) as i16 {
        phantoms_of_order(x, y, room, room, k, &mut ps);
    }
    ps.iter().map(|p| (p.x, p.y, p.order)).collect()
}
