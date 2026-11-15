//! Rendering: neon line-art in 2.5D. A calm deep-space background shader, the arena as a floating
//! platform of thin grid lines, wireframe solids (Heart, enemies, portals), telegraphs with height,
//! music visualised as spectrum ridgelines around the arena edge / a crown around the Heart and a
//! "heartline" ECG trace on the rim, bloom + composite, and the vector-font HUD.
//!
//! The gameplay plane (z = 0) is drawn exactly top-down; height only ever goes through `pj`, which
//! maps z = 0 onto itself. In drift mode (`idle` -> 1) the same line elements unfold into a
//! synthwave vista (grid -> perspective floor, edge ridges -> mountains, rim -> heartline,
//! Heart -> wireframe in front of the sun). Everything is procedural, no art assets.

use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Equation, PipelineParams, UniformType};
use macroquad::models::Vertex;
use macroquad::prelude::*;
use std::f32::consts::{PI, TAU};

use crate::analysis::{SectionKind, N_SPEC};
use crate::game::*;
use crate::level::*;
use crate::rally::{self, Character, FxKind, OrbState, Power, Rally, AVATAR_R, GOAL_HALF, ORB_R, SWING_ACTIVE, SWING_HALF, SWING_R, ZONE_W};

// ---------------------------------------------------------------------------------------------
// Music pulse layer: instant-attack / exponential-decay envelopes per instrument, kept separate
// from the smoothed spectrum. Every amplitude below is multiplied by `Renderer::fx` (menu
// VISUALS: FULL 1.0 / SUBTLE 0.45 / OFF 0). "Global" ones (camera, exposure, bloom, aberration)
// are also damped when the screen is full of bullets.
// ---------------------------------------------------------------------------------------------

// kick: the whole frame punches
const KICK_TAU: f32 = 0.18; // s, punch decay time constant
const KICK_ZOOM: f32 = 0.022; // camera zoom-in per kick (fraction)
const KICK_EXPO: f32 = 0.07; // composite exposure bump (photosensitivity: keep <= 0.15)
const KICK_BLOOM: f32 = 0.22; // bloom gain bump
const KICK_BG: f32 = 0.30; // background shader brightness bump
const KICK_GRID: f32 = 0.06; // floor grid alpha flash
const RIPPLE_R0: f32 = 170.0; // px: ripple starts at the crown so it is not hidden by the Heart
const RIPPLE_SPEED: f32 = 1700.0; // px/s: grid ripple travelling outward from the Heart
const RIPPLE_W: f32 = 95.0; // px, ripple band half-width
const RIPPLE_MAX: f32 = 0.7; // per-vertex alpha cap (overlapping ripples)
const RIPPLE_A: f32 = 1.1; // ripple alpha added to floor grid lines
const RIPPLE_LIFE: f32 = 0.75; // s
const RIPPLE_BG: f32 = 2.2; // deep (background) grid brightening under the ripple
const LUMA_GAP: f32 = 0.34; // s: full-frame luminance bumps scale down if kicks come faster (<= ~3/s)
// snare: a thin ring off the Heart + a rim / chroma crack
const SNARE_TAU: f32 = 0.11;
const SNARE_RINGS: usize = 4;
const SNARE_R0: f32 = 110.0; // px, ring start radius
const SNARE_SPEED: f32 = 1250.0; // px/s
const SNARE_LIFE: f32 = 0.5; // s
const SNARE_A: f32 = 0.55; // ring alpha
const SNARE_W: f32 = 2.0; // px, ring width (thin, cool: never reads as a hot shockwave)
const SNARE_ABERR: f32 = 0.30; // chromatic aberration spike
const SNARE_RIM: f32 = 0.45; // rim heartline brightness flash
// hats: small fast shimmer
const HAT_TAU: f32 = 0.07;
const HAT_SPARK_A: f32 = 0.6; // grid-intersection sparkle alpha
const HAT_SPARK_L: f32 = 7.0; // px, sparkle arm length
const HAT_SPARK_FRAC: f32 = 0.3; // share of intersections lit per hat
const HAT_RIDGE: f32 = 0.5; // live ridgeline alpha shimmer
const HAT_JIT: f32 = 5.0; // px, live ridgeline jitter
// sustained bass: slow breathing
const BASS_RISE: f32 = 2.5; // 1/s
const BASS_FALL: f32 = 1.1; // 1/s
const BASS_ZOOM: f32 = 0.010; // camera breathing
const BASS_PLAT: f32 = 0.6; // platform underside depth breathing
// crowd control
const BUSY_BULLETS: f32 = 140.0; // bullet count at which global effects are damped the most
const BUSY_DAMP: f32 = 0.6; // max damping of global effects
// oscilloscope ring around the Heart (the actual waveform at the audible position)
pub const SCOPE_N: usize = 512; // points kept per frame
const SCOPE_DECIM: usize = 2; // source frames per point (512 x 2 = ~23 ms at 44.1 kHz)
const SCOPE_PTS: usize = 256; // drawn vertices
const SCOPE_TRIG: usize = 384; // source frames searched for the zero-crossing trigger (~9 ms)
const SCOPE_AGC_UP: f32 = 40.0; // 1/s, auto-gain attack
const SCOPE_AGC_DN: f32 = 3.5; // 1/s, auto-gain release
const SCOPE_R: f32 = 208.0; // px, base radius (inside the crown's history rings)
const SCOPE_AMP: f32 = 34.0; // px at full auto-gained level
const SCOPE_KICK: f32 = 30.0; // px, kick pushes the ring outward
const SCOPE_A: f32 = 0.6;
const SCOPE_W: f32 = 2.2;
const SCOPE_Z: f32 = 14.0; // floats slightly above the floor
const SCOPE_TENSE: f32 = 0.28; // ring contracts by this fraction during a build-up
const BEAT_STEP: f32 = TAU / 16.0; // phase-locked ring rotation per beat
// anticipation (file mode): tension over the bars before a drop, released on its downbeat
const BUILD_BARS: f32 = 4.0;
const BUILD_MIN: f32 = 3.0; // s
const BUILD_MAX: f32 = 10.0; // s
const TENSION_VIG: f32 = 0.7; // vignette tightening
const TENSION_DESAT: f32 = 0.65; // background desaturation
const TENSION_ZOOM: f32 = 0.03; // slow camera creep-in
const TENSION_RIDGE: f32 = 0.9; // ridgeline stretch
const TENSION_SCROLL: f32 = 7.0; // deep grid rushes (x base speed)
const TENSION_GRID: f32 = 0.05; // deep grid brightening
const DROP_TAU: f32 = 0.55; // s, release decay
const DROP_FLASH: f32 = 0.10; // one colour flash (not white)
const DROP_ZOOM: f32 = 0.045; // camera snaps out
const DROP_BLOOM: f32 = 0.4;
const DROP_SAT: f32 = 0.6; // background saturation boost
const DROP_ABERR: f32 = 0.6;

// ---------------------------------------------------------------------------------------------
// Palettes
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg0: Color,
    pub bg1: Color,
    pub grid: Color,
    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub danger: Color,
}

fn hex(h: u32) -> Color {
    Color::new(
        ((h >> 16) & 0xff) as f32 / 255.0,
        ((h >> 8) & 0xff) as f32 / 255.0,
        (h & 0xff) as f32 / 255.0,
        1.0,
    )
}

pub fn palette(i: u8) -> Palette {
    match i % N_PALETTES {
        // Sunset
        0 => Palette { bg0: hex(0x0b0220), bg1: hex(0x2b0a3d), grid: hex(0xd94fd8), primary: hex(0x19f0ff), secondary: hex(0xff71ce), accent: hex(0xfff36b), danger: hex(0xff2e4c) },
        // Miami
        1 => Palette { bg0: hex(0x040a26), bg1: hex(0x0b1f4a), grid: hex(0x01b8fe), primary: hex(0x05ffa1), secondary: hex(0xb967ff), accent: hex(0xfffb96), danger: hex(0xff3d6e) },
        // Dusk
        2 => Palette { bg0: hex(0x130622), bg1: hex(0x3a0f55), grid: hex(0xa45cff), primary: hex(0x7df9ff), secondary: hex(0xff901f), accent: hex(0xffd319), danger: hex(0xff2975) },
        // Aqua
        3 => Palette { bg0: hex(0x02121c), bg1: hex(0x04364a), grid: hex(0x1fb5c9), primary: hex(0x5cf6ff), secondary: hex(0x4f8dff), accent: hex(0xff6ac1), danger: hex(0xff3050) },
        // Rose
        _ => Palette { bg0: hex(0x0e0626), bg1: hex(0x2a1060), grid: hex(0xb07cf0), primary: hex(0x7df9ff), secondary: hex(0xff9de1), accent: hex(0xc4ffb3), danger: hex(0xff3a50) },
    }
}

fn mixc(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::new(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, a.a + (b.a - a.a) * t)
}

fn alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a.clamp(0.0, 1.0))
}

fn lerp_pal(a: &Palette, b: &Palette, t: f32) -> Palette {
    Palette {
        bg0: mixc(a.bg0, b.bg0, t),
        bg1: mixc(a.bg1, b.bg1, t),
        grid: mixc(a.grid, b.grid, t),
        primary: mixc(a.primary, b.primary, t),
        secondary: mixc(a.secondary, b.secondary, t),
        accent: mixc(a.accent, b.accent, t),
        danger: mixc(a.danger, b.danger, t),
    }
}

fn v3(c: Color) -> Vec3 {
    vec3(c.r, c.g, c.b)
}

// ---------------------------------------------------------------------------------------------
// Low-level primitives (no heap allocation: everything goes straight into the batcher)
// ---------------------------------------------------------------------------------------------

thread_local! {
    /// Added to every vertex: lets the Heart's layers be drawn around the origin and placed
    /// wherever the Heart is (`set_origin`).
    static ORIGIN: std::cell::Cell<Vec2> = const { std::cell::Cell::new(Vec2::ZERO) };
}

fn set_origin(o: Vec2) {
    ORIGIN.with(|c| c.set(o));
}

#[inline]
fn vtx(p: Vec2, c: Color) -> Vertex {
    let o = ORIGIN.with(|c| c.get());
    Vertex::new(p.x + o.x, p.y + o.y, 0.0, 0.0, 0.0, c)
}

#[inline]
fn geo(v: &[Vertex], i: &[u16]) {
    let gl = unsafe { get_internal_gl() }.quad_gl;
    gl.texture(None);
    gl.draw_mode(DrawMode::Triangles);
    gl.geometry(v, i);
}

/// Line segment with a colour per end (gradients), slightly extended caps.
fn seg(a: Vec2, b: Vec2, w: f32, ca: Color, cb: Color) {
    if ca.a <= 0.003 && cb.a <= 0.003 {
        return;
    }
    let d = b - a;
    let l = d.length();
    if l < 1e-3 {
        return;
    }
    let t = d / l;
    let n = vec2(-t.y, t.x) * (w * 0.5);
    let (a, b) = (a - t * (w * 0.3), b + t * (w * 0.3));
    geo(&[vtx(a + n, ca), vtx(a - n, ca), vtx(b - n, cb), vtx(b + n, cb)], &[0, 1, 2, 0, 2, 3]);
}

#[inline]
fn line(a: Vec2, b: Vec2, w: f32, c: Color) {
    seg(a, b, w, c, c);
}

fn quad4(p: [Vec2; 4], c: [Color; 4]) {
    if c[0].a + c[1].a + c[2].a + c[3].a <= 0.004 {
        return;
    }
    geo(&[vtx(p[0], c[0]), vtx(p[1], c[1]), vtx(p[2], c[2]), vtx(p[3], c[3])], &[0, 1, 2, 0, 2, 3]);
}

/// Filled disc (triangle fan); `rim` is the edge colour (pass a transparent one for a soft glow).
fn disc2(c: Vec2, r: f32, n: usize, col: Color, rim: Color) {
    if col.a <= 0.003 && rim.a <= 0.003 {
        return;
    }
    let n = n.clamp(3, 24);
    let mut v = [vtx(c, col); 25];
    let mut idx = [0u16; 72];
    for k in 0..n {
        let a = k as f32 * TAU / n as f32;
        v[k + 1] = vtx(c + vec2(a.cos(), a.sin()) * r, rim);
        idx[k * 3 + 1] = (k + 1) as u16;
        idx[k * 3 + 2] = if k + 1 == n { 1 } else { (k + 2) as u16 };
    }
    geo(&v[..n + 1], &idx[..n * 3]);
}

#[inline]
fn disc(c: Vec2, r: f32, n: usize, col: Color) {
    disc2(c, r, n, col, col);
}

fn rot2(v: Vec2, a: f32) -> Vec2 {
    let (s, c) = a.sin_cos();
    vec2(v.x * c - v.y * s, v.x * s + v.y * c)
}

fn dir(a: f32) -> Vec2 {
    vec2(a.cos(), a.sin())
}

fn rquad(center: Vec2, half: Vec2, rot: f32, color: Color) {
    let p = [
        center + rot2(vec2(-half.x, -half.y), rot),
        center + rot2(vec2(half.x, -half.y), rot),
        center + rot2(vec2(half.x, half.y), rot),
        center + rot2(vec2(-half.x, half.y), rot),
    ];
    quad4(p, [color; 4]);
}

fn poly_lines(center: Vec2, n: usize, r: f32, rot: f32, thick: f32, color: Color) {
    let mut prev = center + dir(rot) * r;
    for i in 1..=n {
        let p = center + dir(rot + i as f32 * TAU / n as f32) * r;
        line(prev, p, thick, color);
        prev = p;
    }
}

/// Thick arc from angle a0 to a1 (radians) on the z = 0 plane, drawn as a triangle strip.
fn arc(center: Vec2, r: f32, a0: f32, a1: f32, thick: f32, color: Color) {
    let span = a1 - a0;
    if span.abs() < 1e-4 || r <= 0.0 || color.a <= 0.003 {
        return;
    }
    let segs = ((span.abs() * r / 14.0).ceil() as usize).clamp(3, 128);
    let (ri, ro) = ((r - thick * 0.5).max(0.0), r + thick * 0.5);
    let mut pi = center + dir(a0) * ri;
    let mut po = center + dir(a0) * ro;
    for s in 1..=segs {
        let d = dir(a0 + span * s as f32 / segs as f32);
        let (ci, co) = (center + d * ri, center + d * ro);
        quad4([pi, po, co, ci], [color; 4]);
        pi = ci;
        po = co;
    }
}

fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    ease((x - a) / (b - a))
}

/// Wrap an angle into (-PI, PI].
fn wrap(a: f32) -> f32 {
    let mut a = (a + PI).rem_euclid(TAU) - PI;
    if a <= -PI {
        a += TAU;
    }
    a
}

/// Diamond outline (rotated square) in screen or world space.
fn diamond(c: Vec2, r: f32, w: f32, col: Color) {
    let p = [c + vec2(0.0, -r), c + vec2(r, 0.0), c + vec2(0.0, r), c + vec2(-r, 0.0)];
    for i in 0..4 {
        line(p[i], p[(i + 1) % 4], w, col);
    }
}

fn diamond_fill(c: Vec2, r: f32, col: Color) {
    quad4([c + vec2(0.0, -r), c + vec2(r, 0.0), c + vec2(0.0, r), c + vec2(-r, 0.0)], [col; 4]);
}

/// Corner brackets around a rectangle.
fn brackets(x: f32, y: f32, w: f32, h: f32, len: f32, th: f32, col: Color) {
    for (cx, cy, sx, sy) in [(x, y, 1.0, 1.0), (x + w, y, -1.0, 1.0), (x + w, y + h, -1.0, -1.0), (x, y + h, 1.0, -1.0)] {
        line(vec2(cx, cy), vec2(cx + sx * len, cy), th, col);
        line(vec2(cx, cy), vec2(cx, cy + sy * len), th, col);
    }
}

fn rect_lines(x: f32, y: f32, w: f32, h: f32, th: f32, col: Color) {
    let p = [vec2(x, y), vec2(x + w, y), vec2(x + w, y + h), vec2(x, y + h)];
    for i in 0..4 {
        line(p[i], p[(i + 1) % 4], th, col);
    }
}

/// Smoothly interpolated (cosine) value of a spectrum at fraction f in 0..1.
/// `band` with a little blur across the spectrum, so a shape built from it has no corners.
fn soft_band(spec: &[f32; N_SPEC], f: f32) -> f32 {
    0.25 * band(spec, f - 0.06) + 0.5 * band(spec, f) + 0.25 * band(spec, f + 0.06)
}

fn band(spec: &[f32; N_SPEC], f: f32) -> f32 {
    let x = f.clamp(0.0, 1.0) * (N_SPEC - 1) as f32;
    let i = (x as usize).min(N_SPEC - 2);
    let t = x - i as f32;
    let t = (1.0 - (t * PI).cos()) * 0.5;
    spec[i] + (spec[i + 1] - spec[i]) * t
}

// ---------------------------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------------------------

pub fn spaced(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push(ch);
    }
    out
}

thread_local! {
    /// The smooth interface font; `None` falls back to macroquad's built-in pixel font.
    static UI_FONT: std::cell::RefCell<Option<Font>> = const { std::cell::RefCell::new(None) };
}

/// Sizes all over the game were chosen for the old pixel font, whose letters are smaller for
/// the same nominal size. This brings the smooth font to about the same width.
const FONT_SCALE: f32 = 0.86;

/// Oxanium SemiBold (SIL Open Font License 1.1, see `assets/fonts/oxanium/OFL.txt`), built into
/// the game. It is the fixed-weight file: the text rasteriser cannot pick a weight out of a
/// variable font and would draw its thinnest one.
const FONT_OXANIUM: &[u8] = include_bytes!("../assets/fonts/oxanium/Oxanium-SemiBold.ttf");

/// Load the interface font. Order: `heartline-font.ttf` next to the game (any TTF the player
/// drops there), then the built-in Oxanium, then Bahnschrift from the Windows fonts folder.
/// If none of them loads, text stays in macroquad's pixel font.
pub fn init_font() {
    let read = |p: std::path::PathBuf| std::fs::read(p).ok();
    let own = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join("heartline-font.ttf"))).and_then(read);
    let system = std::env::var_os("WINDIR").map(|w| std::path::Path::new(&w).join("Fonts").join("bahnschrift.ttf"));
    let sources: [Box<dyn Fn() -> Option<Vec<u8>>>; 3] = [Box::new(move || own.clone()), Box::new(|| Some(FONT_OXANIUM.to_vec())), Box::new(move || system.clone().and_then(read))];
    for src in sources {
        let Some(bytes) = src() else { continue };
        if let Ok(mut f) = load_ttf_font_from_bytes(&bytes) {
            f.set_filter(FilterMode::Linear);
            UI_FONT.with(|c| *c.borrow_mut() = Some(f));
            return;
        }
    }
}

/// Pixel size to rasterise at, and the scale that brings it to exactly `size`.
fn font_px(size: f32) -> (u16, f32) {
    let has = UI_FONT.with(|c| c.borrow().is_some());
    if !has {
        return (size.max(1.0) as u16, 1.0);
    }
    let want = (size * FONT_SCALE).max(1.0);
    let px = want.ceil().clamp(1.0, 300.0);
    (px as u16, want / px)
}

pub fn text_w(s: &str, size: f32) -> f32 {
    let (px, sc) = font_px(size);
    UI_FONT.with(|c| measure_text(s, c.borrow().as_ref(), px, sc).width)
}

pub fn text_left(s: &str, x: f32, y: f32, size: f32, color: Color) {
    let (px, sc) = font_px(size);
    UI_FONT.with(|c| {
        draw_text_ex(s, x, y, TextParams { font: c.borrow().as_ref(), font_size: px, font_scale: sc, color, ..Default::default() });
    });
}

pub fn text_center(s: &str, x: f32, y: f32, size: f32, color: Color) {
    let w = text_w(s, size);
    text_left(s, x - w * 0.5, y, size, color);
}

#[allow(dead_code)]
pub fn text_right(s: &str, x: f32, y: f32, size: f32, color: Color) {
    let w = text_w(s, size);
    text_left(s, x - w, y, size, color);
}

/// Text with a soft dark backing so it stays readable over the scene.
pub fn text_shadow(s: &str, x: f32, y: f32, size: f32, color: Color, align: i8) {
    let w = text_w(s, size);
    let x0 = match align {
        0 => x - w * 0.5,
        1 => x - w,
        _ => x,
    };
    let o = (size * 0.07).max(1.0);
    text_left(s, x0 + o, y + o, size, Color::new(0.0, 0.0, 0.05, 0.75 * color.a));
    text_left(s, x0, y, size, color);
}

// ---- vector stroke font ----
// Glyphs live on a 4 x 6 grid (y down, 6 = baseline). Each glyph is a list of strokes separated by
// spaces; a stroke is a run of "xy" digit pairs.

/// Italic slant of the vector font (grid units of x per unit of height).
const VFONT_SLANT: f32 = 0.16;
/// Advance of a normal glyph / a narrow punctuation glyph, in grid units.
const VFONT_ADV: f32 = 5.6;
const VFONT_ADV_NARROW: f32 = 3.0;

fn glyph(c: char) -> Option<&'static str> {
    Some(match c {
        'A' => "060110304146 0343",
        'B' => "06003041423303 3344453606",
        'C' => "4130100105163645",
        'D' => "00304145360600",
        'E' => "40000646 0333",
        'F' => "400006 0333",
        'G' => "41301001051636454323",
        'H' => "0006 4046 0343",
        'I' => "1030 2026 1636",
        'J' => "4045361605",
        'K' => "0006 400346",
        'L' => "000646",
        'M' => "0600234046",
        'N' => "06004640",
        'O' => "103041453616050110",
        'P' => "06003041423303",
        'Q' => "103041453616050110 2446",
        'R' => "06003041423303 2346",
        'S' => "413010010213334445361605",
        'T' => "0040 2026",
        'U' => "000516364540",
        'V' => "0004264440",
        'W' => "0006234640",
        'X' => "0046 4006",
        'Y' => "002340 2326",
        'Z' => "00400646",
        '0' => "103041453616050110 4105",
        '1' => "112026 1636",
        '2' => "01103041420646",
        '3' => "01103041423313 334445361605",
        '4' => "36300444",
        '5' => "400003334445361605",
        '6' => "30100105163645443303",
        '7' => "00404126",
        '8' => "103041423313020110 1304051636454433",
        '9' => "43130201103041453616",
        '.' => "2526",
        ',' => "252617",
        ':' => "2122 2526",
        '!' => "2023 2526",
        '?' => "011030414223 2526",
        '-' => "1333",
        '+' => "1333 2224",
        '/' => "4006",
        '\'' => "2021",
        '"' => "1011 3031",
        '(' => "30111536",
        ')' => "10313516",
        '<' => "300336",
        '>' => "104316",
        '=' => "0242 0444",
        '_' => "0646",
        '[' => "30101636",
        ']' => "10303616",
        '|' => "2026",
        '%' => "4006 0010110100 3545463635",
        '#' => "1016 3036 0242 0444",
        '*' => "1135 3115 0343",
        _ => return None,
    })
}

fn narrow(c: char) -> bool {
    matches!(c, '.' | ',' | ':' | '!' | '\'' | '|')
}

/// Draw vector text with its baseline at y, left edge at x. Colours fade top -> bottom.
fn vtext_draw(s: &str, x: f32, y: f32, size: f32, ctop: Color, cbot: Color, glow: bool) {
    let u = size * 0.12;
    let th = (size * 0.072).max(1.2);
    let passes: &[(f32, f32)] = if glow { &[(3.4, 0.13), (1.8, 0.22), (1.0, 1.0)] } else { &[(1.0, 1.0)] };
    for &(wm, am) in passes {
        let mut cx = x;
        for ch in s.chars() {
            let c = ch.to_ascii_uppercase();
            if c == ' ' {
                cx += 4.0 * u;
                continue;
            }
            if let Some(g) = glyph(c) {
                let dx = if narrow(c) { -1.1 } else { 0.0 };
                for stroke in g.split(' ') {
                    let b = stroke.as_bytes();
                    let pt = |k: usize| -> (Vec2, f32) {
                        let gx = (b[k * 2] - b'0') as f32;
                        let gy = (b[k * 2 + 1] - b'0') as f32;
                        (vec2(cx + (gx + dx + (6.0 - gy) * VFONT_SLANT) * u, y + (gy - 6.0) * u), gy / 6.0)
                    };
                    let n = b.len() / 2;
                    for k in 1..n {
                        let (p0, f0) = pt(k - 1);
                        let (p1, f1) = pt(k);
                        seg(p0, p1, th * wm, alpha(mixc(ctop, cbot, f0), am), alpha(mixc(ctop, cbot, f1), am));
                    }
                }
                cx += if narrow(c) { VFONT_ADV_NARROW } else { VFONT_ADV } * u;
            } else if am >= 1.0 {
                let mut buf = [0u8; 4];
                let cs = ch.encode_utf8(&mut buf);
                text_left(cs, cx, y, size * 0.95, ctop);
                cx += text_w(cs, size * 0.95) + 1.2 * u;
            } else {
                let mut buf = [0u8; 4];
                cx += text_w(ch.encode_utf8(&mut buf), size * 0.95) + 1.2 * u;
            }
        }
    }
}

/// Neon heading text (screen space, vector stroke font). `align`: -1 left, 0 centre, 1 right.
/// `y` is the baseline, like `text_left`.
pub fn vtext(s: &str, x: f32, y: f32, size: f32, color: Color, align: i8) {
    let w = vtext_w(s, size);
    let x0 = match align {
        0 => x - w * 0.5,
        1 => x - w,
        _ => x,
    };
    vtext_draw(s, x0, y, size, color, mixc(color, Color::new(color.r * 0.7, color.g * 0.7, color.b * 0.85, color.a), 0.6), true);
}

pub fn vtext_w(s: &str, size: f32) -> f32 {
    let u = size * 0.12;
    let mut w = 0.0;
    let mut last_gap = 0.0;
    for ch in s.chars() {
        let c = ch.to_ascii_uppercase();
        if c == ' ' {
            w += 4.0 * u;
            last_gap = 0.0;
        } else if glyph(c).is_some() {
            w += if narrow(c) { VFONT_ADV_NARROW } else { VFONT_ADV } * u;
            last_gap = if narrow(c) { 0.4 } else { 1.6 } * u;
        } else {
            let mut buf = [0u8; 4];
            w += text_w(ch.encode_utf8(&mut buf), size * 0.95) + 1.2 * u;
            last_gap = 1.2 * u;
        }
    }
    (w - last_gap).max(0.0)
}

// ---------------------------------------------------------------------------------------------
// View + 2.5D projection
// ---------------------------------------------------------------------------------------------

pub const VIEW_W: f32 = 1920.0;
pub const VIEW_H: f32 = 1080.0;

#[derive(Clone, Copy)]
pub struct View {
    pub sw: f32,
    pub sh: f32,
    pub scale: f32,
    pub center: Vec2,
}

impl View {
    pub fn to_world(&self, p: Vec2) -> Vec2 {
        (p - vec2(self.sw, self.sh) * 0.5) / self.scale + self.center
    }
    #[allow(dead_code)]
    pub fn to_screen(&self, w: Vec2) -> Vec2 {
        (w - self.center) * self.scale + vec2(self.sw, self.sh) * 0.5
    }
}

/// Fraction of the player's position the camera follows (gives the parallax something to do).
const CAM_FOLLOW: f32 = 0.16;
/// Camera smoothing rate (1/s).
const CAM_RATE: f32 = 7.0;
/// Perspective focal distance in world units: height z scales things by F / (F - z) about the camera.
const PERSP_F: f32 = 2200.0;
/// Oblique tilt: one unit of height moves a point this many units up-screen.
const TILT: f32 = 0.5;

// ---- level geometry (world units) ----
/// Heartline rim: distance outside the arena edge and ECG spike amplitude.
const RIM_OFF: f32 = 9.0;
const RIM_ECG: f32 = 16.0;
/// Depth of the platform's underside rim.
const PLAT_D: f32 = 70.0;
/// Floor grid alpha: resting, extra while tiles are armed, extra on bar swells.
const FLOOR_A: f32 = 0.085;
const FLOOR_ARMED_A: f32 = 0.07;
const FLOOR_SWELL_A: f32 = 0.035;
/// Edge spectrum ridgelines outside the arena.
const RIDGES: usize = 10;
const RIDGE_BASE: f32 = 12.0;
const RIDGE_GAP: f32 = 9.0;
const RIDGE_AMP: f32 = 46.0;
const RIDGE_A: f32 = 0.42;
/// Spectrum crown around the Heart.
const CROWN_LAYERS: usize = 5;
const CROWN_R: f32 = 168.0;
const CROWN_GAP: f32 = 15.0;
const CROWN_AMP: f32 = 30.0;
const CROWN_A: f32 = 0.38;
/// Heart construct.
const HEART_Z: f32 = 44.0;
const HEART_SHELL_R: f32 = 60.0;
const MORPH_RATE: f32 = 1.3;
/// Seconds a sky takes to sink and the next one to rise.
const SKY_TRANSITION: f32 = 1.8;
/// Tile strike hammer slab start height and slam light column height.
const SLAB_H: f32 = 150.0;
const TILE_COL_H: f32 = 260.0;
/// Laser wall / shockwave wall heights.
const LASER_H: f32 = 95.0;
const WAVE_H: f32 = 46.0;
/// Pending pulse: approach ring start radius (beyond the Heart) and height, danger-zone reach.
const PULSE_R0: f32 = 330.0;
const PULSE_Z: f32 = 150.0;
const ZONE_REACH: f32 = 520.0;
/// Bullets closer than this (radians) count as one danger wedge; wedge padding each side.
const ZONE_MERGE: f32 = 0.42;
const ZONE_PAD: f32 = 0.07;

// ---- drift vista (screen fractions) ----
const HZ: f32 = 0.58;
const SUN_Y: f32 = 0.40;
const SUN_R: f32 = 0.22;
const HL_Y: f32 = 0.80;
const HL_AMP: f32 = 0.075;
const MOUNT_H: f32 = 0.16;
const VISTA_FAN: f32 = 1.0;
const VISTA_GRID_A: f32 = 0.55;
/// Vista grid rows scrolled per second (kicks surge it).
const VISTA_SCROLL: f32 = 0.55;
/// Background deep grid scroll (world units / s) and its warp boost.
const DEEP_SCROLL: f32 = 14.0;
const WARP_SCROLL: f32 = 900.0;

// ---- music state ----
const HIST: usize = 14;
const HIST_DT: f32 = 0.085;
/// Heartline samples: one per perimeter point (P segments -> P + 1 points).
const P: usize = 240;
const ECG_N: usize = P + 1;
const ECG_RATE: f32 = 110.0;
/// Resting heartbeat interval in silence (s).
const REST_BEAT: f32 = 1.05;
/// Ambient spectrum level when nothing is playing.
const AMBIENT: f32 = 0.10;

// Perimeter layout (segments per side) so the corners land exactly on sample indices. Index 0 is
// the middle of the top edge, going clockwise.
const P_TOP_HALF: usize = 38;
const P_SIDE: usize = 44;
const P_BOTTOM: usize = 76;
const P_C1: usize = P_TOP_HALF;
const P_C2: usize = P_C1 + P_SIDE;
const P_C3: usize = P_C2 + P_BOTTOM;
const P_C4: usize = P_C3 + P_SIDE;

/// Point on the arena edge and its outward normal for perimeter index i (0..=P).
fn perim(i: usize) -> (Vec2, Vec2) {
    let (w, h) = (HALF_W, HALF_H);
    let f = |a: usize, n: usize| (i - a) as f32 / n as f32;
    let s = std::f32::consts::FRAC_1_SQRT_2;
    match i {
        _ if i == P_C1 => (vec2(w, -h), vec2(s, -s)),
        _ if i == P_C2 => (vec2(w, h), vec2(s, s)),
        _ if i == P_C3 => (vec2(-w, h), vec2(-s, s)),
        _ if i == P_C4 => (vec2(-w, -h), vec2(-s, -s)),
        _ if i < P_C1 => (vec2(w * f(0, P_TOP_HALF), -h), vec2(0.0, -1.0)),
        _ if i < P_C2 => (vec2(w, -h + 2.0 * h * f(P_C1, P_SIDE)), vec2(1.0, 0.0)),
        _ if i < P_C3 => (vec2(w - 2.0 * w * f(P_C2, P_BOTTOM), h), vec2(0.0, 1.0)),
        _ if i < P_C4 => (vec2(-w, h - 2.0 * h * f(P_C3, P_SIDE)), vec2(-1.0, 0.0)),
        _ => (vec2(-w + w * f(P_C4, P_TOP_HALF), -h), vec2(0.0, -1.0)),
    }
}

/// Spectrum fraction for perimeter index i: bass at the corners, treble at the edge midpoints.
fn perim_band(i: usize) -> f32 {
    let corner = [P_C1, P_C2, P_C3, P_C4].iter().map(|&c| (i as i32 - c as i32).unsigned_abs()).min().unwrap_or(0);
    (corner as f32 / P_TOP_HALF as f32).min(1.0)
}

/// ECG-style template of one heartbeat, `x` seconds after the onset.
fn ecg_shape(x: f32) -> f32 {
    if !(-0.02..0.5).contains(&x) {
        return 0.0;
    }
    let b = |c: f32, w: f32| (-((x - c) / w).powi(2)).exp();
    -0.18 * b(0.010, 0.012) + 1.0 * b(0.038, 0.016) - 0.38 * b(0.066, 0.016) + 0.22 * b(0.21, 0.05)
}

// ---------------------------------------------------------------------------------------------
// Wireframe solids
// ---------------------------------------------------------------------------------------------

/// Every Heart solid is padded to this many edges so any two can morph edge-for-edge.
const ME: usize = 36;
type Edges = [[Vec3; 2]; ME];

const SH_TETRA: usize = 0;
const SH_CUBE: usize = 1;
const SH_OCTA: usize = 2;
const SH_ICOSA: usize = 3;
const SH_DODECA: usize = 4;
const SH_STELLA: usize = 5;
const SH_CUBOCT: usize = 6;
const SH_STAR: usize = 7;

fn min_edges(v: &[Vec3]) -> Vec<[Vec3; 2]> {
    let mut md = f32::MAX;
    for i in 0..v.len() {
        for j in i + 1..v.len() {
            md = md.min(v[i].distance(v[j]));
        }
    }
    let mut e = Vec::new();
    for i in 0..v.len() {
        for j in i + 1..v.len() {
            if v[i].distance(v[j]) < md * 1.03 {
                e.push([v[i], v[j]]);
            }
        }
    }
    e
}

fn signs3() -> impl Iterator<Item = Vec3> {
    (0..8).map(|k| vec3(if k & 1 == 0 { 1.0 } else { -1.0 }, if k & 2 == 0 { 1.0 } else { -1.0 }, if k & 4 == 0 { 1.0 } else { -1.0 }))
}

fn cyc(a: f32, b: f32, c: f32) -> [Vec3; 3] {
    [vec3(a, b, c), vec3(c, a, b), vec3(b, c, a)]
}

fn unit(mut v: Vec<Vec3>) -> Vec<Vec3> {
    for p in v.iter_mut() {
        *p = p.normalize();
    }
    v
}

fn tetra_verts(s: f32) -> Vec<Vec3> {
    unit(vec![vec3(1.0, 1.0, 1.0) * s, vec3(1.0, -1.0, -1.0) * s, vec3(-1.0, 1.0, -1.0) * s, vec3(-1.0, -1.0, 1.0) * s])
}

fn octa_verts() -> Vec<Vec3> {
    vec![Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z]
}

fn pad_edges(mut e: Vec<[Vec3; 2]>) -> Edges {
    let key = |x: &[Vec3; 2]| {
        let m = (x[0] + x[1]) * 0.5;
        m.y.atan2(m.x) + m.z * 0.3
    };
    e.sort_by(|a, b| key(a).partial_cmp(&key(b)).unwrap_or(std::cmp::Ordering::Equal));
    // orient every edge the same way round so morphs don't cross over
    for x in e.iter_mut() {
        if x[0].y.atan2(x[0].x) > x[1].y.atan2(x[1].x) {
            x.swap(0, 1);
        }
    }
    let mut out = [[Vec3::ZERO; 2]; ME];
    let n = e.len().max(1);
    for (i, o) in out.iter_mut().enumerate() {
        *o = e[(i * n / ME).min(n - 1)];
    }
    out
}

fn build_solids() -> Vec<Edges> {
    let phi = (1.0 + 5f32.sqrt()) * 0.5;
    let tetra = min_edges(&tetra_verts(1.0));
    let cube = min_edges(&unit(signs3().collect()));
    let octa = min_edges(&octa_verts());
    let mut ico = Vec::new();
    for s in signs3().filter(|s| s.x > 0.0) {
        ico.extend(cyc(0.0, s.y, s.z * phi));
    }
    let icosa = min_edges(&unit(ico));
    let mut dod: Vec<Vec3> = signs3().collect();
    for s in signs3().filter(|s| s.x > 0.0) {
        dod.extend(cyc(0.0, s.y / phi, s.z * phi));
    }
    let dodeca = min_edges(&unit(dod));
    let mut stella = min_edges(&tetra_verts(1.0));
    stella.extend(min_edges(&tetra_verts(-1.0)));
    let mut co = Vec::new();
    for s in signs3().filter(|s| s.z > 0.0) {
        co.extend(cyc(s.x, s.y, 0.0));
    }
    let cuboct = min_edges(&unit(co));
    let mut star = min_edges(&octa_verts());
    for s in signs3() {
        let tip = s.normalize() * 1.45;
        star.push([vec3(s.x, 0.0, 0.0), tip]);
        star.push([vec3(0.0, s.y, 0.0), tip]);
        star.push([vec3(0.0, 0.0, s.z), tip]);
    }
    // scale the star down so its spikes sit at about the same radius as the other solids
    for e in star.iter_mut() {
        e[0] *= 0.78;
        e[1] *= 0.78;
    }
    vec![pad_edges(tetra), pad_edges(cube), pad_edges(octa), pad_edges(icosa), pad_edges(dodeca), pad_edges(stella), pad_edges(cuboct), pad_edges(star)]
}

/// Enemy shapes, local +x = heading. Index: chaser, darter, orbiter, splitter, elite.
fn build_kinds() -> Vec<Vec<[Vec3; 2]>> {
    // chaser: triangular pyramid pointing forward (arrowhead from above)
    let tip = vec3(1.35, 0.0, 0.0);
    let b: Vec<Vec3> = (0..3).map(|k| {
        let a = PI * 0.5 + k as f32 * TAU / 3.0;
        vec3(-0.7, a.cos() * 0.85, a.sin() * 0.85)
    }).collect();
    let mut chaser = Vec::new();
    for k in 0..3 {
        chaser.push([tip, b[k]]);
        chaser.push([b[k], b[(k + 1) % 3]]);
    }
    // darter: long bipyramid (kite)
    let (f, r) = (vec3(1.8, 0.0, 0.0), vec3(-1.25, 0.0, 0.0));
    let m = [vec3(0.0, 0.62, 0.0), vec3(0.0, 0.0, 0.5), vec3(0.0, -0.62, 0.0), vec3(0.0, 0.0, -0.5)];
    let mut darter = Vec::new();
    for k in 0..4 {
        darter.push([f, m[k]]);
        darter.push([r, m[k]]);
        darter.push([m[k], m[(k + 1) % 4]]);
    }
    // orbiter: cube
    let orbiter: Vec<[Vec3; 2]> = min_edges(&signs3().map(|s| s * 0.72).collect::<Vec<_>>());
    // splitter: hexagonal prism with a tetra inside
    let mut splitter = Vec::new();
    for k in 0..6 {
        let a0 = dir(k as f32 * TAU / 6.0);
        let a1 = dir((k + 1) as f32 * TAU / 6.0);
        for z in [-0.42f32, 0.42] {
            splitter.push([vec3(a0.x, a0.y, z), vec3(a1.x, a1.y, z)]);
        }
        splitter.push([vec3(a0.x, a0.y, -0.42), vec3(a0.x, a0.y, 0.42)]);
    }
    for e in min_edges(&tetra_verts(1.0)) {
        splitter.push([e[0] * 0.42, e[1] * 0.42]);
    }
    // elite: stellated octahedron (spiky star)
    let elite: Vec<[Vec3; 2]> = build_solids()[SH_STAR].to_vec();
    vec![chaser, darter, orbiter, splitter, elite]
}

fn kind_index(k: EnemyKind) -> usize {
    match k {
        EnemyKind::Chaser | EnemyKind::Mini => 0,
        EnemyKind::Darter | EnemyKind::Wisp => 1,
        EnemyKind::Orbiter | EnemyKind::Bomber => 2,
        EnemyKind::Splitter | EnemyKind::Turret => 3,
        EnemyKind::Elite | EnemyKind::Sprite => 4,
    }
}

/// Bullet directions of a Heart pulse: calls `f(angle, speed_factor)` once per bullet. Mirrors
/// `World::emit_pulse`; `aim` is the angle from the Heart to the player. Unknown patterns fall
/// back to a ring.
fn pulse_dirs(pattern: Pattern, aim: f32, rot: f32, count: usize, mut f: impl FnMut(f32, f32)) {
    if count == 0 {
        return;
    }
    match pattern {
        Pattern::Fan => {
            let n = count.max(1) | 1;
            for k in 0..n {
                f(aim + (k as f32 - (n as f32 - 1.0) * 0.5) * FAN_STEP, 1.0);
            }
        }
        Pattern::Spiral => {
            let arms = 2usize;
            let per = (count / arms).max(1);
            for a in 0..arms {
                for j in 0..per {
                    f(rot + a as f32 * TAU / arms as f32 + j as f32 * 0.12, (1.0 - 0.09 * j as f32).max(0.4));
                }
            }
        }
        Pattern::Cross => {
            for a in 0..4 {
                f(aim + a as f32 * TAU / 4.0, 1.0);
            }
        }
        Pattern::Wall => {
            let step = 2.0 * WALL_HALF / (count.max(2) - 1) as f32;
            for k in 0..count {
                let off = -WALL_HALF + k as f32 * step;
                if (off - rot).abs() >= WALL_GAP_HALF {
                    f(aim + off, 1.0);
                }
            }
        }
        Pattern::Burst => f(aim, 1.0),
        Pattern::Pincer => {
            let per = (count / 2).max(1);
            for side in [-1.0f32, 1.0] {
                for k in 0..per {
                    let fr = if per > 1 { k as f32 / (per - 1) as f32 - 0.5 } else { 0.0 };
                    f(aim + side * PINCER_OFFSET + fr * PINCER_SPREAD, 1.0);
                }
            }
        }
        Pattern::Comb => {
            for tooth in 0..5 {
                f(aim + (tooth as f32 - 2.0) * COMB_STEP, 1.0);
            }
        }
        Pattern::Sweep => {
            let side = if rot >= 0.0 { 1.0 } else { -1.0 };
            for k in 0..count {
                let fr = if count > 1 { k as f32 / (count - 1) as f32 } else { 0.5 };
                f(aim + side * (fr * 2.0 - 1.0) * SWEEP_HALF, 1.0 - 0.38 * fr);
            }
        }
        Pattern::Flower => {
            for k in 0..count {
                let a = k as f32 * TAU / count as f32;
                f(rot + a, 0.78 + 0.22 * (5.0 * a).cos());
            }
        }
        Pattern::Arc => {
            let span = TAU * 0.74;
            for k in 0..count {
                let fr = if count > 1 { k as f32 / (count - 1) as f32 } else { 0.5 };
                f(rot + TAU * 0.13 + fr * span, 1.0);
            }
        }
        _ => {
            for k in 0..count {
                f(rot + k as f32 * TAU / count as f32, 1.0);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Shaders
// ---------------------------------------------------------------------------------------------

const VERT: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
varying highp vec2 uv;
varying lowp vec4 color;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1);
    color = color0 / 255.0;
    uv = texcoord;
}
"#;

/// Plain vertex-colour fragment shader used with additive blending for neon line art.
const ADD_FRAG: &str = r#"#version 100
precision lowp float;
varying lowp vec4 color;
varying highp vec2 uv;
uniform sampler2D Texture;
void main() {
    gl_FragColor = color * texture2D(Texture, uv);
}
"#;

const BG_FRAG: &str = r#"#version 100
precision highp float;
varying vec2 uv;
uniform vec2 u_res;
uniform float u_time;
uniform float u_idle;
uniform float u_scale;
uniform float u_swell;
uniform float u_bass;
uniform float u_scroll;
uniform float u_lv;
uniform float u_warp;
uniform float u_kick;
uniform vec3 u_ripple;
uniform vec3 u_pump;
uniform vec2 u_cam;
uniform vec2 u_arena;
uniform vec3 u_vista;
uniform vec3 u_c0;
uniform vec3 u_c1;
uniform vec3 u_grid;
uniform vec3 u_sunA;
uniform vec3 u_sunB;
// x = backdrop (0 sunset, 1 night, 2 ocean, 3 vortex, 4 starfield), y = weather (0 none, 1 rain, 2 dust, 3 aurora)
uniform vec3 u_mood;
// x = horizon prop (0 none, 1 palms, 2 pyramids, 3 skyline, 4 meteors), y = hat envelope, z = kick envelope
uniform vec3 u_mood2;
// sky transition: x = progress 0..1 (1 = settled), y = the backdrop being left, z = the prop being left
uniform vec3 u_tr;
// x = chime envelope, y = its pitch 0..1, z = ear-candy envelope
uniform vec3 u_fx;

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }

float stars(vec2 p, float cell, float dens) {
    vec2 id = floor(p / cell);
    float h = hash(id);
    if (h < 1.0 - dens) return 0.0;
    vec2 f = p / cell - id - 0.5;
    vec2 o = vec2(hash(id + 3.1), hash(id + 7.7)) - 0.5;
    float d = length(f - o * 0.7);
    float tw = 0.6 + 0.4 * sin(u_time * (0.5 + h * 1.5) + h * 40.0);
    return smoothstep(0.07, 0.0, d) * tw;
}

// Distance from p to the segment a-b.
float sdSeg(vec2 p, vec2 a, vec2 b) {
    vec2 pa = p - a, ba = b - a;
    return length(pa - ba * clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0));
}

float vnoise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1.0, 0.0)), f.x), mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), f.x), f.y);
}

float fbm(vec2 p) {
    float a = 0.5;
    float sum = 0.0;
    for (int i = 0; i < 4; i++) {
        sum += a * vnoise(p);
        p = p * 2.03 + 17.1;
        a *= 0.5;
    }
    return sum;
}

// One layer of falling rain. `g` is x across (aspect-corrected) and y down the screen; streaks
// are brightest at the falling tip and fade up their tail.
float rainLayer(vec2 g, float cols, float speed, float len, float seed) {
    g.x += g.y * (0.16 + 0.05 * sin(u_time * 0.31));
    float cx = g.x * cols;
    float h = hash(vec2(floor(cx), seed));
    float fall = fract(g.y * (1.0 + h) - u_time * speed * (0.8 + 0.5 * h) + h * 17.0);
    float tail = fall < len ? fall / len : 0.0;
    return step(0.4, h) * smoothstep(0.5, 0.1, abs(fract(cx) - 0.5) * 2.0) * tail * tail;
}

// In-game: calm deep space under a floating platform. q = world position on the z = 0 plane.
vec3 topdown(vec2 q) {
    vec3 col = mix(u_c0 * 0.55, u_c1 * 0.42, clamp(uv.y * 0.8 + 0.1, 0.0, 1.0));
    // far striped sun under the Heart (deepest layer, barely moves with the camera)
    float sS = 0.22;
    vec2 sp = (q - u_cam * (1.0 - sS)) / sS - vec2(0.0, 380.0);
    float sr = 1300.0 * (1.0 + 0.015 * u_swell);
    float d = length(sp) / sr;
    float yy = sp.y / sr;
    float band = yy < -0.15 ? 1.0 : step(0.08 + 0.42 * (yy + 0.15), fract(yy * 7.0 - u_time * 0.04));
    vec3 scol = mix(u_sunA, u_sunB, clamp(yy * 0.5 + 0.5, 0.0, 1.0));
    float sc = u_mood.x;
    if (sc > 4.5) {
        if (sc < 5.5) {
            // ringed planet, seen from above
            float pd = length(sp - vec2(-420.0, -200.0)) / (sr * 0.55);
            vec2 rq = (sp - vec2(-420.0, -200.0)) * vec2(1.0, 2.6) / (sr * 0.55);
            float rl = length(rq);
            col += mix(u_sunA, u_sunB, 0.5) * 0.10 * smoothstep(1.0, 0.98, pd);
            col += u_sunA * 0.07 * smoothstep(1.3, 1.36, rl) * smoothstep(2.1, 2.0, rl) * (0.6 + 0.4 * sin(rl * 30.0));
        } else if (sc < 6.5) {
            // black hole: a dark well with a hot ring that breathes with the kick
            col *= 1.0 - 0.6 * smoothstep(0.62, 0.5, d);
            float er = abs(d - 0.64);
            col += mix(vec3(1.0, 0.6, 0.3), vec3(1.0), 0.4) * 0.2 * exp(-er * (30.0 - 10.0 * u_mood2.z));
        } else if (sc < 7.5) {
            // twin suns
            col += scol * band * 0.11 * smoothstep(0.8, 0.79, d);
            float d2 = length(sp - vec2(1500.0, -700.0)) / (sr * 0.45);
            col += mix(u_grid, vec3(1.0), 0.4) * 0.10 * smoothstep(1.0, 0.98, d2);
        } else if (sc < 8.5) {
            // aurora ribbons under the floor
            float ax = sp.x / 500.0 + sin(sp.y / 700.0 - u_time * 0.2) * 1.2;
            float cur = pow(sin(ax + sin(ax * 0.4 + u_time * 0.1) * 2.0) * 0.5 + 0.5, 3.0);
            col += mix(u_grid, u_sunA, sin(sp.y / 900.0) * 0.5 + 0.5) * cur * 0.09 * (1.0 + 0.6 * u_swell);
        } else if (sc > 10.5 && sc < 11.5) {
            // signal: rings leaving a point far below
            float rr = length(sp) / sr;
            float ring = smoothstep(0.42, 0.5, abs(fract(rr * 2.6 - u_time * 0.2) - 0.5));
            col += mix(u_sunA, u_grid, clamp(rr * 0.5, 0.0, 1.0)) * ring * (0.06 + 0.10 * u_mood2.z) * exp(-rr * 0.6);
        } else if (sc > 9.5 && sc < 12.5) {
            // prism and the rising sun: the far striped sun
            col += scol * band * 0.11 * smoothstep(1.0, 0.985, d);
            col += u_sunB * 0.05 * exp(-d * d * 1.2) * (1.0 + 0.6 * u_swell);
        } else {
            // tide: crests rolling under the platform, lit on the kick
            float wy = sp.y / 170.0 + sin(sp.x / 380.0 + u_time * 0.3) * 1.3 - u_time * 0.6;
            float crest = smoothstep(0.82, 1.0, sin(wy * 6.2832));
            col += mix(vec3(1.0, 0.35, 0.75), vec3(0.25, 0.85, 1.0), sin(sp.y / 800.0) * 0.5 + 0.5) * crest * (0.07 + 0.10 * u_mood2.z) * (1.0 + 0.5 * u_swell);
        }
    } else if (sc < 0.5) {
        // sunset: far striped sun
        col += scol * band * 0.12 * smoothstep(1.0, 0.985, d);
        col += u_sunB * 0.05 * exp(-d * d * 1.2) * (1.0 + 0.6 * u_swell);
    } else if (sc < 1.5) {
        // night: a pale moon with a bitten edge
        float md = length(sp - vec2(520.0, -260.0)) / (sr * 0.5);
        float bite = length(sp - vec2(760.0, -380.0)) / (sr * 0.5);
        col += mix(u_sunB, vec3(1.0), 0.55) * 0.11 * smoothstep(1.0, 0.98, md) * smoothstep(0.86, 0.9, bite);
        col += u_sunB * 0.04 * exp(-md * md * 0.8);
    } else if (sc < 2.5) {
        // ocean: long swells rolling under the platform
        float wy = sp.y / 150.0;
        float wv = sin(wy * 6.2832 + sin(sp.x / 420.0 + u_time * 0.25) * 1.4 - u_time * 0.5);
        col += mix(u_sunB, u_grid, 0.5) * 0.07 * smoothstep(0.86, 1.0, wv) * (1.0 + 0.5 * u_swell);
        col += u_sunB * 0.04 * exp(-d * d * 0.8);
    } else if (sc < 3.5) {
        // vortex: rings falling away under the Heart
        float rr = length((q - u_cam * 0.72) / 0.28) / 900.0;
        float ring = smoothstep(0.42, 0.5, abs(fract(rr * 2.2 - u_time * 0.12) - 0.5));
        col += mix(u_sunA, u_sunB, clamp(rr * 0.4, 0.0, 1.0)) * ring * 0.10 * exp(-rr * 0.55) * (1.0 + 0.6 * u_swell);
    } else {
        // starfield: a slow nebula
        vec2 np = (q - u_cam * 0.8) / 1400.0;
        float n1 = sin(np.x * 3.1 + sin(np.y * 2.3 + u_time * 0.03) * 2.0) * 0.5 + 0.5;
        float n2 = sin(np.y * 2.7 - sin(np.x * 1.9 - u_time * 0.02) * 2.0) * 0.5 + 0.5;
        col += mix(u_sunA, u_sunB, n1) * 0.085 * n1 * n2;
        col += vec3(0.8, 0.85, 1.0) * stars((q - u_cam * 0.75) / 0.25, 60.0, 0.16) * 0.4;
    }
    // star layer
    float sT = 0.33;
    vec2 tp = (q - u_cam * (1.0 - sT)) / sT;
    col += vec3(0.75, 0.8, 1.0) * stars(tp, 80.0, 0.12) * 0.5;
    // deep grid plane, slow drift
    float sG = 0.5;
    vec2 gp = (q - u_cam * (1.0 - sG)) / sG + vec2(0.0, u_scroll);
    float px = 1.0 / (u_scale * sG);
    vec2 g = abs(fract(gp / 192.0 + 0.5) - 0.5) * 192.0;
    float ln = 1.0 - smoothstep(0.4 * px, 1.7 * px, min(g.x, g.y));
    float fog = exp(-length(q - u_cam) * 0.0011);
    col += u_grid * ln * (0.03 + 0.03 * u_swell + 0.25 * u_warp + u_pump.z) * fog;
    // kick ripple: a band of brighter grid travelling outward from the Heart
    float rd = (length(q) - u_ripple.x) / u_ripple.z;
    col += u_grid * ln * u_ripple.y * exp(-rd * rd) * fog;
    // weather, outside and under the platform
    float we = u_mood.y;
    if ((we > 0.5 && we < 1.5) || (we > 4.5 && we < 5.5)) {
        // a storm is rain with lightning: the whole deep flashes on the kick
        if (we > 4.5) col += vec3(0.45, 0.55, 0.9) * 0.14 * u_mood2.z;
        vec2 g = vec2(uv.x * u_res.x / u_res.y, uv.y);
        float beat = 0.4 + 0.9 * u_mood2.y + 0.3 * u_swell;
        float rn = rainLayer(g, 120.0, 1.6, 0.10, 5.5) * 0.4 + rainLayer(g, 60.0, 2.5, 0.18, 8.1) * 0.7 + rainLayer(g, 28.0, 3.6, 0.28, 2.7);
        col += mix(u_grid, vec3(0.85, 0.92, 1.0), 0.45) * rn * 0.2 * beat;
        // splashes on the floor plane
        vec2 gp = q / 150.0;
        vec2 cell = floor(gp);
        float ph = fract(u_time * 1.2 + hash(cell) * 9.0);
        vec2 cpos = vec2(hash(cell + 3.1), hash(cell + 7.7)) * 0.5 + 0.25;
        float rd = length(fract(gp) - cpos);
        float ring = smoothstep(0.03, 0.0, abs(rd - ph * 0.3)) * (1.0 - ph) * step(0.3, hash(cell + 1.9));
        col += mix(u_grid, vec3(0.9, 0.95, 1.0), 0.5) * ring * (0.10 + 0.35 * u_mood2.y);
    } else if (we > 1.5 && we < 2.5) {
        vec2 dp = (q - u_cam * 0.4) / 0.6 + vec2(u_time * 22.0, u_time * 9.0);
        col += mix(u_sunA, vec3(1.0), 0.4) * stars(dp, 130.0, 0.35) * 0.3;
    } else if (we > 2.5 && we < 3.5) {
        float ax = uv.x * 5.0 + u_time * 0.12;
        float au = sin(ax + sin(ax * 2.3 - u_time * 0.2) * 1.3) * 0.5 + 0.5;
        float edge = smoothstep(0.55, 1.0, abs(uv.y - 0.5) * 2.0);
        col += mix(u_grid, u_sunA, au) * 0.10 * edge * (0.4 + 0.6 * au) * (1.0 + 0.5 * u_swell);
    } else if (we > 3.5 && we < 4.5) {
        // snow drifting across, two depths; the hats make it sparkle
        vec2 s1 = (q - u_cam * 0.35) / 0.65 + vec2(sin(u_time * 0.5 + q.y * 0.004) * 30.0 + u_time * 12.0, -u_time * 46.0);
        vec2 s2 = (q - u_cam * 0.55) / 0.45 + vec2(sin(u_time * 0.4 + q.y * 0.003) * 40.0 - u_time * 8.0, -u_time * 30.0);
        col += vec3(0.9, 0.95, 1.0) * (stars(s1, 110.0, 0.5) * 0.35 + stars(s2, 170.0, 0.5) * 0.22) * (0.7 + 0.9 * u_mood2.y);
    } else if (we > 5.5 && we < 6.5) {
        // mist: slow banks drifting under the platform, thicker with the bass
        float mst = fbm((q - u_cam * 0.5) / 700.0 + vec2(u_time * 0.03, u_time * 0.011));
        col += mix(u_c1, vec3(0.8, 0.85, 1.0), 0.3) * 0.16 * smoothstep(0.38, 0.78, mst) * (0.7 + 0.5 * u_swell);
    } else if (we > 6.5 && we < 7.5) {
        // wind: long streaks racing across; the hats gust them
        vec2 g2 = vec2(uv.y * 1.8, -uv.x * u_res.x / u_res.y * 0.5);
        float wn = rainLayer(g2, 70.0, 0.5, 0.35, 4.4) * 0.6 + rainLayer(g2 + 0.3, 30.0, 0.8, 0.5, 9.2);
        col += mix(u_grid, vec3(1.0), 0.6) * wn * (0.06 + 0.22 * u_mood2.y);
    } else if (we > 7.5 && we < 8.5) {
        // cloud shadows sliding under the platform, their edges lit
        float cs = fbm((q - u_cam * 0.45) / 620.0 + vec2(u_time * 0.05, u_time * 0.02));
        col = mix(col, u_c0 * 0.25, 0.45 * smoothstep(0.45, 0.75, cs));
        col += mix(u_sunB, u_sunA, 0.4) * 0.05 * smoothstep(0.6, 0.85, cs);
    } else if (we > 8.5 && we < 9.5) {
        // petals drifting past below
        vec2 pp = (q - u_cam * 0.4) / 0.6 + vec2(u_time * 26.0, -u_time * 12.0 + sin(u_time * 0.6 + q.x * 0.004) * 30.0);
        col += mix(vec3(1.0, 0.55, 0.75), u_sunA, 0.35) * stars(pp, 120.0, 0.4) * (0.35 + 0.7 * u_mood2.y);
    } else if (we > 9.5) {
        // shafts of light turning slowly under the floor
        vec2 sq = (q - u_cam * 0.7) / 900.0;
        float sa = atan(sq.y, sq.x);
        float rays = pow(0.5 + 0.5 * sin(sa * 7.0 + u_time * 0.1), 3.0);
        col += mix(u_sunB, u_sunA, 0.5) * rays * exp(-length(sq) * 0.9) * (0.05 + 0.07 * u_swell);
    }
    // platform glass: the arena floor dims what is below it
    vec2 a = abs(q) - u_arena;
    float inside = 1.0 - smoothstep(-1.0, 1.0, max(a.x, a.y));
    col = mix(col, u_c0 * 0.5, inside * 0.5 * u_lv);
    return col;
}

// The sky itself for backdrop `sc`: its own zenith and horizon colours, haze and star density.
// Backdrop 0 (the sunset) is the palette's sky, untouched.
vec3 skyBase(float sc, vec2 p) {
    float hz = u_vista.x;
    float t = clamp(p.y / hz, 0.0, 1.0);
    vec3 pz = u_c0 * 0.5;
    vec3 ph = mix(u_c1, u_sunB, 0.35);
    vec3 z = pz;
    vec3 h = ph;
    float starD = 0.05;
    float haze = 0.0;
    if (sc > 0.5) {
        haze = 0.35;
        if (sc < 1.5) { z = vec3(0.012, 0.022, 0.075); h = vec3(0.07, 0.22, 0.42); starD = 0.10; }
        else if (sc < 2.5) { z = vec3(0.10, 0.03, 0.22); h = vec3(0.95, 0.42, 0.26); starD = 0.02; }
        else if (sc < 3.5) { z = vec3(0.05, 0.015, 0.12); h = vec3(0.60, 0.22, 0.48); starD = 0.09; }
        else if (sc < 4.5) { z = vec3(0.01, 0.01, 0.045); h = vec3(0.12, 0.06, 0.26); starD = 0.10; haze = 0.2; }
        else if (sc < 5.5) { z = vec3(0.02, 0.012, 0.07); h = vec3(0.34, 0.12, 0.40); starD = 0.09; }
        else if (sc < 6.5) { z = vec3(0.0, 0.0, 0.02); h = vec3(0.20, 0.07, 0.10); starD = 0.12; }
        else if (sc < 7.5) { z = vec3(0.13, 0.03, 0.20); h = vec3(0.92, 0.36, 0.46); starD = 0.02; }
        else if (sc < 8.5) { z = vec3(0.0, 0.02, 0.06); h = vec3(0.04, 0.17, 0.22); starD = 0.0; }
        else if (sc < 9.5) { z = vec3(0.0, 0.02, 0.08); h = vec3(0.06, 0.20, 0.38); starD = 0.10; }
        else if (sc < 10.5) { z = vec3(0.05, 0.01, 0.12); h = vec3(0.55, 0.16, 0.50); starD = 0.07; }
        else if (sc < 11.5) { z = vec3(0.0, 0.02, 0.07); h = vec3(0.08, 0.30, 0.42); starD = 0.08; }
        else if (sc < 12.5) { z = vec3(0.07, 0.01, 0.13); h = vec3(0.80, 0.30, 0.42); starD = 0.05; }
        else { z = vec3(0.004, 0.01, 0.05); h = vec3(0.10, 0.12, 0.36); starD = 0.14; }
        // keep some of the palette so the sky still belongs to the land in front of it
        z = mix(z, pz, 0.25);
        h = mix(h, ph, 0.3);
    }
    vec3 col = mix(z, h, pow(t, 2.0));
    col += h * haze * exp(-(1.0 - t) * 10.0);
    col += vec3(0.8, 0.85, 1.0) * stars(p * u_res, 9.0 * u_res.y / 1080.0, starD) * (1.0 - t) * 0.8;
    return col;
}

// The celestial body of backdrop `sc`, drawn over the sky colour `col`. `off` moves it down the
// screen (used to sink the old one and raise the new one during a transition).
vec3 skyBody(float sc, vec2 p, vec3 col, float off) {
    float hz = u_vista.x;
    float t = p.y / hz;
    vec2 asp = vec2(u_res.x / u_res.y, 1.0);
    vec2 q = (p - vec2(0.5, u_vista.y + off)) * asp;
    float r = u_vista.z * (1.0 + 0.035 * u_bass);
    float dd = length(q);
    float yy = q.y / r;
    float px = 1.0 / u_res.y;
    vec3 suncol = mix(u_sunA, u_sunB, clamp(yy * 0.6 + 0.5, 0.0, 1.0));
    if (sc < 0.5) {
        // sunset: the striped sun
        float mask = yy < 0.0 ? 1.0 : step(0.1 + 0.5 * yy, fract(yy * 5.0 - u_time * 0.22));
        col = mix(col, suncol * 0.68, smoothstep(r, r - 0.003, dd) * mask);
        col += u_sunB * 0.16 * exp(-dd * dd * 12.0) * (1.0 + 0.6 * u_bass);
    } else if (sc < 1.5) {
        // moonrise: a great pale disc in the same language as the sun: a flat gradient, scan
        // bars drifting through its lower half, a soft edge and a halo that breathes with the bass
        float R = r * 1.08;
        float ym = q.y / R;
        vec3 mc = mix(vec3(0.97, 0.99, 1.0), vec3(0.52, 0.66, 1.0), clamp(ym * 0.6 + 0.5, 0.0, 1.0));
        float bars = ym < -0.05 ? 1.0 : step(0.12 + 0.5 * (ym + 0.05), fract(ym * 6.0 - u_time * 0.18));
        float disc = smoothstep(R, R - 0.004, dd);
        float outm = max(dd - R, 0.0);
        col += vec3(0.40, 0.58, 1.0) * (0.36 * exp(-outm * 9.0) + 0.14 * exp(-outm * 2.5)) * (1.0 + 0.6 * u_bass);
        col = mix(col, mc * 0.95, disc * bars);
        col += vec3(0.85, 0.9, 1.0) * stars(p * u_res + 40.0, 6.0 * u_res.y / 1080.0, 0.08) * (1.0 - t) * 0.7 * (1.0 - disc);
    } else if (sc < 2.5) {
        // ocean dusk: a huge sun going into the sea, long clouds lit from below
        vec2 oq = (p - vec2(0.5, hz + off)) * asp;
        float od = length(oq);
        float R = r * 1.3 * (1.0 + 0.03 * u_bass);
        float yo = -oq.y / R; // 0 at the horizon, 1 at the top of the sun
        vec3 sc2 = mix(vec3(1.0, 0.30, 0.42), vec3(1.0, 0.86, 0.42), clamp(yo, 0.0, 1.0));
        float bars = yo > 0.55 ? 1.0 : step(0.16 + 0.5 * (0.55 - yo), fract(yo * 9.0 + u_time * 0.15));
        col += vec3(1.0, 0.5, 0.3) * (0.30 * exp(-od * od * 5.0) + 0.25 * exp(-max(od - R, 0.0) * 8.0)) * (1.0 + 0.5 * u_bass);
        col = mix(col, sc2 * 0.92, smoothstep(R, R - 0.003, od) * bars);
        float cl = fbm(vec2(p.x * 3.0 + u_time * 0.01, (p.y - off) * 26.0));
        float cm = smoothstep(0.52, 0.7, cl) * smoothstep(0.15, 0.5, t) * smoothstep(1.0, 0.8, t);
        col = mix(col, mix(vec3(0.20, 0.05, 0.22), vec3(1.0, 0.55, 0.35), smoothstep(0.6, 0.78, cl)), cm * 0.7);
    } else if (sc < 3.5) {
        // crescent: a great striped crescent in the sun's language (a flat gradient, scan bars
        // through its lower half, a soft halo that breathes with the bass) and a star riding in
        // the bite that flashes on the kick
        float R = r * 1.05;
        vec2 c = q - vec2(-0.03, 0.0);
        float d = length(c);
        float bd = length(c - vec2(R * 0.44, -R * 0.18));
        float ym = c.y / R;
        vec3 mc = mix(vec3(1.0, 0.96, 0.84), vec3(1.0, 0.52, 0.74), clamp(ym * 0.6 + 0.5, 0.0, 1.0));
        float bars = ym < -0.5 ? 1.0 : step(0.10 + 0.34 * (ym + 0.5), fract(ym * 7.0 - u_time * 0.2));
        float disc = smoothstep(R, R - 0.004, d) * smoothstep(R * 0.86 - 0.004, R * 0.86, bd);
        float outm = max(d - R, 0.0);
        col += vec3(1.0, 0.55, 0.72) * (0.20 * exp(-outm * 11.0) + 0.07 * exp(-outm * 3.0)) * (1.0 + 0.6 * u_bass) * (0.35 + 0.65 * smoothstep(R * 0.80, R * 0.95, bd));
        col = mix(col, mc * 0.66, disc * bars);
        vec2 s2 = c - vec2(R * 0.50, -R * 0.20);
        float fl = exp(-abs(s2.x) * 200.0) * exp(-abs(s2.y) * 26.0) + exp(-abs(s2.y) * 200.0) * exp(-abs(s2.x) * 26.0);
        col += vec3(1.0, 0.95, 0.85) * (fl * 0.7 + exp(-dot(s2, s2) * 5000.0) * 1.4) * (0.55 + 1.3 * u_mood2.z);
    } else if (sc < 4.5) {
        // nebula: lit gas and dust lanes along a band of stars
        vec2 np = q * 2.2 + vec2(u_time * 0.006, 0.0);
        float f1 = fbm(np + fbm(np * 1.7 + 3.0) * 1.2);
        float f2 = fbm(np * 2.3 - 7.0);
        float across = q.y + q.x * 0.32 + 0.02;
        float along = q.x - q.y * 0.32;
        float bandm = exp(-pow(across / 0.3, 2.0));
        float neb = smoothstep(0.24, 0.66, f1) * (0.3 + 0.7 * bandm);
        vec3 nc = mix(vec3(0.10, 0.22, 0.85), vec3(0.95, 0.22, 0.75), f2);
        nc = mix(nc, vec3(0.45, 0.95, 1.0), smoothstep(0.62, 0.85, f1) * 0.6);
        col *= 1.0 - 0.5 * smoothstep(0.5, 0.75, f2) * bandm * (1.0 - neb);
        col += nc * neb * (1.35 + 0.4 * u_bass);
        // the galaxy's core: a warm bar of light along the band
        float corev = exp(-(along * along / 0.035 + across * across / 0.008));
        float coreh = exp(-(along * along / 0.5 + across * across / 0.035));
        col += vec3(1.0, 0.86, 0.72) * (corev * 0.5 + coreh * 0.3) * (0.6 + 0.8 * f1);
        col += vec3(0.9, 0.93, 1.0) * stars(p * u_res + 90.0, 6.0 * u_res.y / 1080.0, 0.28) * (1.0 - t * 0.5) * (0.5 + 0.8 * bandm);
        for (int i = 0; i < 3; i++) {
            float fi = float(i);
            vec2 s2 = q - vec2(-0.42 + fi * 0.37, -0.20 + 0.11 * sin(fi * 2.4));
            float tw = 0.7 + 0.3 * sin(u_time * (1.3 + fi) + fi * 2.0);
            float fl = exp(-abs(s2.x) * 260.0) * exp(-abs(s2.y) * 22.0) + exp(-abs(s2.y) * 260.0) * exp(-abs(s2.x) * 22.0);
            col += vec3(0.85, 0.92, 1.0) * (fl * 0.8 + exp(-dot(s2, s2) * 4000.0) * 1.5) * tw;
        }
    } else if (sc < 5.5) {
        // ringed planet: a flat gradient disc with scan bars like the sun's, inside three glowing
        // rings that brighten on the kick
        float R = r * 0.95;
        vec2 c = q - vec2(0.0, -0.02);
        float d = length(c);
        float yp = c.y / R;
        vec3 pc = mix(vec3(1.0, 0.84, 0.46), vec3(0.96, 0.30, 0.62), clamp(yp * 0.6 + 0.5, 0.0, 1.0));
        float bars = yp < 0.0 ? 1.0 : step(0.1 + 0.5 * yp, fract(yp * 5.0 - u_time * 0.22));
        vec2 rq = vec2(c.x * 0.94 - c.y * 0.34, (c.y * 0.94 + c.x * 0.34) * 3.8) / R;
        float rl = length(rq);
        float aa = 12.0 * px / R;
        float b1 = smoothstep(1.34, 1.34 + aa, rl) * smoothstep(1.60, 1.60 - aa, rl);
        float b2 = smoothstep(1.70, 1.70 + aa, rl) * smoothstep(2.00, 2.00 - aa, rl);
        float b3 = smoothstep(2.12, 2.12 + aa, rl) * smoothstep(2.24, 2.24 - aa, rl);
        float ring = b1 * 0.7 + b2 * 1.0 + b3 * 0.5;
        // light runs round the rings
        float run = 0.85 + 0.15 * sin(atan(rq.y, rq.x) * 2.0 - u_time * 0.8);
        vec3 rc = mix(vec3(1.0, 0.86, 0.62), vec3(0.80, 0.50, 1.0), smoothstep(1.3, 2.25, rl)) * run * (0.85 + 0.6 * u_mood2.z);
        float disc = smoothstep(R, R - 0.004, d);
        float front = step(0.0, rq.y);
        float outp = max(d - R, 0.0);
        col += vec3(1.0, 0.45, 0.65) * (0.26 * exp(-outp * 9.0) + 0.10 * exp(-outp * 2.5)) * (1.0 + 0.6 * u_bass);
        col += rc * 0.10 * exp(-pow((rl - 1.8) / 0.55, 2.0)) * (1.0 - disc);
        col = mix(col, rc * 0.8, ring * (1.0 - front) * (1.0 - disc) * 0.9);
        col = mix(col, pc * 0.9, disc * bars);
        col = mix(col, rc * 1.1, ring * front * 0.97);
        col += rc * ring * 0.3 * (1.0 - disc * (1.0 - front));
    } else if (sc < 6.5) {
        // black hole: a clean shadow with a photon ring, lensed light around it and an accretion
        // disc crossing in front
        float R = r * 0.62;
        q.y += 0.085;
        dd = length(q);
        vec2 dn = q / max(dd, 0.0001);
        float outc = dd - R;
        float streak = 0.7 + 0.3 * fbm(dn * 5.0 + vec2(u_time * 0.08, 3.0) + dd * 3.0);
        float dop = 0.62 - 0.38 * dn.x;
        float halo = exp(-max(outc, 0.0) * 9.0) * dop * streak;
        vec3 hot = mix(vec3(1.0, 0.93, 0.80), vec3(1.0, 0.45, 0.22), smoothstep(0.0, 0.22, outc));
        col += hot * halo * (0.85 + 0.5 * u_mood2.z);
        col += vec3(1.0, 0.4, 0.3) * 0.16 * exp(-max(outc, 0.0) * 2.4);
        col = mix(col, vec3(0.0), smoothstep(R + px, R - px, dd));
        float ringw = 2.2 * px;
        col += vec3(1.0, 0.95, 0.85) * exp(-pow((outc - ringw) / ringw, 2.0)) * 1.2;
        float ax = q.x / R;
        float ay = (q.y - 0.012 - 0.035 * q.x) / R;
        float thick = 0.04 + 0.022 * abs(ax);
        float bandd = exp(-pow(ay / thick, 2.0)) * smoothstep(3.1, 1.3, abs(ax));
        float flow = 0.62 + 0.38 * vnoise(vec2(ax * 9.0 - u_time * 1.3, 2.0));
        vec3 dc = mix(vec3(1.0, 0.96, 0.86), vec3(1.0, 0.5, 0.2), smoothstep(0.3, 2.6, abs(ax)));
        col += dc * bandd * flow * (0.75 - 0.3 * clamp(ax * 0.6, -1.0, 1.0)) * 1.3;
    } else if (sc < 7.5) {
        // binary sunset: the striped sun, and a small white-hot companion over its shoulder
        vec2 q1 = q - vec2(-0.07, 0.02);
        float d1 = length(q1);
        float y1 = q1.y / r;
        float m1 = y1 < 0.0 ? 1.0 : step(0.1 + 0.5 * y1, fract(y1 * 5.0 - u_time * 0.22));
        vec2 q2 = q - vec2(0.25, -0.15);
        float r2 = r * 0.4;
        float d2b = length(q2);
        float y2 = q2.y / r2;
        vec3 c2 = mix(vec3(1.0, 0.98, 0.92), vec3(1.0, 0.55, 0.7), clamp(y2 * 0.5 + 0.5, 0.0, 1.0));
        float m2 = y2 < 0.25 ? 1.0 : step(0.2 + 0.5 * (y2 - 0.25), fract(y2 * 3.0 - u_time * 0.3));
        col += vec3(1.0, 0.7, 0.75) * (0.30 * exp(-max(d2b - r2, 0.0) * 12.0) + 0.14 * exp(-d2b * d2b * 18.0));
        col = mix(col, c2, smoothstep(r2, r2 - 0.003, d2b) * m2);
        vec3 c1 = mix(u_sunA, u_sunB, clamp(y1 * 0.6 + 0.5, 0.0, 1.0));
        col = mix(col, c1 * 0.72, smoothstep(r, r - 0.003, d1) * m1);
        col += u_sunB * 0.18 * exp(-d1 * d1 * 12.0) * (1.0 + 0.6 * u_bass);
    } else if (sc < 8.5) {
        // aurora: three ribbons, each a wavy hem with rays streaming up from it
        float x = (p.x - 0.5) * asp.x;
        float yv = (hz - p.y) / hz + off; // 0 at the horizon, 1 at the top
        col += vec3(0.85, 0.9, 1.0) * stars(p * u_res + 13.0, 6.0 * u_res.y / 1080.0, 0.12) * (1.0 - t) * 0.6;
        for (int i = 0; i < 3; i++) {
            float fi = float(i);
            float hem = 0.30 + 0.17 * fi + 0.07 * sin(x * (2.1 + fi * 0.7) + u_time * (0.16 + 0.05 * fi) + fi * 2.0) + 0.035 * sin(x * (5.3 + fi) - u_time * 0.23);
            float up = yv - hem;
            float body = smoothstep(-0.012, 0.0, up) * exp(-max(up, 0.0) * (5.5 - fi));
            float rays = 0.55 + 0.45 * vnoise(vec2(x * 46.0 + fi * 13.0 + sin(u_time * 0.2 + fi) * 2.0, fi));
            float fold = 0.6 + 0.4 * sin(x * (3.0 + fi) + u_time * 0.3 + fi);
            vec3 ac = mix(vec3(0.15, 1.0, 0.55), vec3(0.55, 0.3, 1.0), clamp(up * 3.2, 0.0, 1.0));
            col += ac * body * rays * fold * (0.42 - 0.08 * fi) * (1.0 + 0.5 * u_bass);
            col += vec3(0.6, 1.0, 0.8) * smoothstep(0.012, 0.0, abs(up)) * 0.25 * fold;
        }
    } else if (sc > 9.5 && sc < 10.5) {
        // prism: a great striped triangle in the sun's colours, a thin frame of light around
        // it that flares on the kick, and three rays fanning out behind
        float R = r * 1.25 * (1.0 + 0.035 * u_bass);
        vec2 c = q - vec2(0.0, 0.10 * R);
        // distance to an upright triangle: apex at the top, base below
        float k = 1.7320508;
        vec2 tq = vec2(abs(c.x) - R, -c.y + R / k);
        if (tq.x + k * tq.y > 0.0) tq = vec2(tq.x - k * tq.y, -k * tq.x - tq.y) * 0.5;
        tq.x -= clamp(tq.x, -2.0 * R, 0.0);
        float d = -length(tq) * sign(tq.y);
        float yp = (c.y + R * 0.2) / R;
        vec3 pc = mix(u_sunA, u_sunB, clamp(yp * 0.7 + 0.5, 0.0, 1.0));
        float bars = yp < 0.0 ? 1.0 : step(0.1 + 0.5 * yp, fract(yp * 6.0 - u_time * 0.22));
        for (int i = 0; i < 3; i++) {
            float a = -1.5708 + (float(i) - 1.0) * 0.55 + 0.05 * sin(u_time * 0.4 + float(i));
            vec2 dv = vec2(cos(a), sin(a));
            float along = dot(c, dv);
            float off2 = abs(dot(c, vec2(-dv.y, dv.x)));
            col += mix(u_sunA, vec3(1.0), 0.3) * 0.10 * smoothstep(0.05 + 0.10 * along, 0.0, off2) * step(0.0, along) * exp(-along * 2.2) * (1.0 + 0.8 * u_mood2.z);
        }
        col += u_sunB * (0.22 * exp(-max(d, 0.0) * 14.0) + 0.10 * exp(-max(d, 0.0) * 3.0)) * (1.0 + 0.6 * u_bass) * step(0.0, d);
        col = mix(col, pc * 0.72, smoothstep(0.003, 0.0, d) * bars);
        float frame = abs(d - 0.022);
        col += mix(u_grid, vec3(1.0), 0.4) * smoothstep(0.004 + px, 0.004 - px, frame) * (0.55 + 0.9 * u_mood2.z);
    } else if (sc > 10.5 && sc < 11.5) {
        // signal: a small striped core that sends rings out across the sky. They thin as they
        // grow, and every kick lights the whole set.
        float R = r * 0.6 * (1.0 + 0.05 * u_bass);
        float ph = u_time * 0.2;
        float rs = 0.0;
        for (int i = 0; i < 6; i++) {
            float k = fract(ph + float(i) / 6.0);
            float rr = R * (1.12 + k * 3.2);
            float wd = 0.0055 - 0.0035 * k;
            rs += smoothstep(wd + px, wd - px, abs(dd - rr)) * (1.0 - k) * (1.0 - k);
        }
        vec3 rc = mix(u_sunA, u_grid, clamp((dd - R) / (R * 3.0), 0.0, 1.0));
        col += rc * rs * (0.5 + 1.2 * u_mood2.z);
        float ys = q.y / R;
        vec3 cc = mix(u_sunA, u_sunB, clamp(ys * 0.6 + 0.5, 0.0, 1.0));
        float cm = ys < 0.0 ? 1.0 : step(0.1 + 0.5 * ys, fract(ys * 4.0 - u_time * 0.25));
        float outc = max(dd - R, 0.0);
        col += u_sunA * (0.30 * exp(-outc * 12.0) + 0.10 * exp(-outc * 3.0)) * (1.0 + 0.8 * u_bass);
        col = mix(col, cc * 0.8, smoothstep(R, R - 0.003, dd) * cm);
    } else if (sc > 11.5 && sc < 12.5) {
        // rising sun: half a striped disc on the horizon throwing broad rays up the sky. The
        // rays turn slowly, swell with the bass and flare on the kick.
        vec2 c = (p - vec2(0.5, hz + off)) * asp;
        float R = r * 1.15 * (1.0 + 0.03 * u_bass);
        float d = length(c);
        float ang = atan(-c.y, c.x);
        float wedge = smoothstep(0.0, 0.25, abs(fract(ang * 7.0 / 3.14159 + u_time * 0.03) - 0.5) * 2.0 - 0.5);
        float reach = exp(-max(d - R, 0.0) * 1.9) * step(R, d);
        col += mix(u_sunB, u_sunA, clamp(d * 1.4, 0.0, 1.0)) * wedge * reach * (0.20 + 0.16 * u_bass + 0.2 * u_mood2.z);
        float yo = -c.y / R; // 0 at the horizon, 1 at the top of the disc
        vec3 dc = mix(u_sunB, u_sunA, clamp(yo, 0.0, 1.0));
        float bars = yo > 0.5 ? 1.0 : step(0.14 + 0.5 * (0.5 - yo), fract(yo * 8.0 + u_time * 0.18));
        col += u_sunB * 0.26 * exp(-max(d - R, 0.0) * 9.0) * (1.0 + 0.6 * u_bass);
        col = mix(col, dc * 0.72, smoothstep(R, R - 0.004, d) * bars);
    } else if (sc > 12.5) {
        // comet: a striped head with a long tail of light streaming back across the sky. Scan
        // bars flow down the tail and the kick brightens it.
        vec2 hc = q - vec2(-0.30, -0.02);
        float R = r * 0.42 * (1.0 + 0.05 * u_bass);
        vec2 tv = normalize(vec2(1.0, -0.34));
        float al = dot(hc, tv);
        float ac = dot(hc, vec2(-tv.y, tv.x));
        float TL = 1.3;
        float wd = R * (0.8 + 0.7 * al / TL);
        float tail = step(0.0, al) * exp(-pow(ac / (wd * 0.6), 2.0)) * pow(1.0 - smoothstep(0.0, TL, al), 1.6);
        float cut = step(0.3 + 0.5 * al / TL, fract(al * 26.0 - u_time * 0.9));
        vec3 tc = mix(u_sunA, u_grid, clamp(al / TL, 0.0, 1.0));
        col += tc * tail * (cut * (0.42 + 0.6 * u_mood2.z) + 0.10);
        float dh = length(hc);
        float yh = hc.y / R;
        vec3 hcol2 = mix(u_sunA, u_sunB, clamp(yh * 0.6 + 0.5, 0.0, 1.0)) * 0.8;
        float hm = yh < 0.0 ? 1.0 : step(0.1 + 0.5 * yh, fract(yh * 4.0 - u_time * 0.25));
        float outh = max(dh - R, 0.0);
        col += u_sunA * (0.26 * exp(-outh * 12.0) + 0.08 * exp(-outh * 3.5)) * (1.0 + 0.7 * u_bass);
        col = mix(col, hcol2 * 0.95, smoothstep(R, R - 0.003, dh) * hm);
    } else {
        // tide: rank on rank of waves standing up over the horizon under a small moon; they
        // swell with the bass and their crests light on the kick
        float x = (p.x - 0.5) * asp.x;
        float yv = (hz - p.y) / hz + off; // 0 at the horizon, 1 at the top
        vec2 mq = q - vec2(0.30, -0.17);
        float mr = r * 0.34;
        float mdisc = smoothstep(mr + px, mr - px, length(mq));
        col = mix(col, vec3(0.95, 0.97, 1.0), mdisc * 0.95);
        col += vec3(0.45, 0.65, 1.0) * 0.25 * exp(-max(length(mq) - mr, 0.0) * 10.0) * (1.0 - mdisc);
        float found = 0.0;
        for (int i = 0; i < 9; i++) {
            float fi = float(i); // 0 = the nearest wave
            float sp = 0.5 - 0.04 * fi;
            float amp = (0.050 - 0.0035 * fi) * (1.0 + 0.8 * u_bass);
            float w = sin(x * (2.6 + fi * 0.55) + u_time * sp + fi * 1.9) + 0.45 * sin(x * (6.1 + fi) - u_time * sp * 1.4 + fi * 0.7);
            // peaked crests, like waves about to break
            w = w * 0.6 + 0.8 * (1.0 - abs(sin(x * (1.3 + fi * 0.27) + u_time * sp * 0.5 + fi))) - 0.4;
            float cy = 0.07 + 0.065 * fi + amp * w;
            if (found < 0.5 && yv < cy) {
                found = 1.0;
                vec3 lc = mix(vec3(1.0, 0.35, 0.75), vec3(0.25, 0.85, 1.0), fi / 8.0);
                float crest = smoothstep(0.011, 0.0, cy - yv);
                float body = exp(-(cy - yv) * 16.0);
                col = mix(u_c0 * 0.3, lc * 0.5, body * 0.55) + lc * crest * (0.85 + 0.7 * u_mood2.z);
            }
        }
    }
    return col;
}

// Paint a silhouette the way the sun is painted. `d` is the distance to its edge (negative
// inside) and `yb` its height above the horizon: a tinted body that scan bars drift up through
// (the sky shows between them), a soft edge, and a rim of light that answers the kick.
vec3 propPaint(vec3 col, float d, float yb, float px) {
    float soft = 2.2 * px;
    float vis = step(0.0, yb);
    float inside = smoothstep(soft, -soft, d) * vis;
    float bars = 0.74 + 0.26 * step(0.3, fract(yb * 64.0 - u_time * 0.3));
    vec3 rim = mix(u_sunB, u_sunA, 0.4);
    vec3 body = mix(u_c0 * 0.10, mix(u_c1 * 0.34, rim, 0.14), clamp(yb * 2.2, 0.0, 1.0));
    col = mix(col, body, inside * bars);
    col += rim * (smoothstep(3.5 * px, 0.0, abs(d)) * 0.8 + exp(-max(d, 0.0) * 60.0) * 0.16 * (1.0 + u_mood2.z)) * vis;
    return col;
}

// Distance to a stroke from a to b whose half-width runs from ra to rb.
float sdTaper(vec2 p, vec2 a, vec2 b, float ra, float rb) {
    vec2 pa = p - a, ba = b - a;
    float h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h) - mix(ra, rb, h);
}

// Horizon props: silhouettes standing on the horizon, in front of the sky.
vec3 skyProp(float pr, vec2 p, vec3 col, float off) {
    float hz = u_vista.x;
    float px = 1.0 / u_res.y;
    vec2 asp = vec2(u_res.x / u_res.y, 1.0);
    vec2 q = vec2((p.x - 0.5) * asp.x, hz + off - p.y); // x from the centre, y up from the horizon
    vec3 dark = u_c0 * 0.22;
    if (pr > 0.5 && pr < 1.5) {
        // palms: bowed trunks and broad fronds that arch over and hang, painted like the sun
        // (striped, soft-edged, rimmed with light). They sway, lean with the bass, and the
        // hats flick the tips of the fronds.
        float dmin = 1.0;
        for (int i = 0; i < 4; i++) {
            float fi = float(i);
            float side = mod(fi, 2.0) < 0.5 ? -1.0 : 1.0;
            float far = floor(fi * 0.5);
            vec2 base = vec2(side * (0.50 + 0.22 * far), -0.01);
            if (abs(q.x - base.x) > 0.36) continue;
            float h = 0.34 - 0.10 * far;
            float sway = sin(u_time * 0.8 + fi * 1.9) * 0.012 + 0.008 * u_bass * side;
            vec2 top = base + vec2(-side * (0.09 + 0.03 * far) + sway, h);
            vec2 ctl = base + vec2(side * 0.04, h * 0.6);
            vec2 pa = base;
            for (int k = 1; k <= 6; k++) {
                float tt = float(k) / 6.0;
                vec2 pb = mix(mix(base, ctl, tt), mix(ctl, top, tt), tt);
                dmin = min(dmin, sdTaper(q, pa, pb, mix(0.012, 0.0055, tt - 0.1667), mix(0.012, 0.0055, tt)));
                pa = pb;
            }
            for (int f = 0; f < 7; f++) {
                float ff = float(f);
                float a = -0.30 + ff * 0.62 + sway * 5.0;
                vec2 dv = vec2(cos(a), sin(a));
                float L = h * (0.56 + 0.08 * sin(ff * 2.3 + fi));
                float flick = 0.05 * u_mood2.y * sin(ff * 1.7 + fi + u_time * 7.0);
                vec2 c1 = top + dv * L * 0.55 + vec2(0.0, L * 0.24);
                vec2 tip = top + dv * L * 0.98 - vec2(0.0, L * (0.40 + flick));
                vec2 fa = top;
                float wa = 0.002;
                for (int k = 1; k <= 5; k++) {
                    float tt = float(k) / 5.0;
                    vec2 fb = mix(mix(top, c1, tt), mix(c1, tip, tt), tt);
                    // a leaf: narrow at the crown, broad in the middle, drawn to a point
                    float wb = L * 0.085 * sin(3.14159 * pow(tt, 0.85)) + 0.0008;
                    dmin = min(dmin, sdTaper(q, fa, fb, wa, wb));
                    fa = fb;
                    wa = wb;
                }
            }
            dmin = min(dmin, length(q - top + vec2(0.0, 0.008)) - 0.013);
        }
        col = propPaint(col, dmin, q.y, px);
    } else if (pr > 1.5 && pr < 2.5) {
        // radio masts: glowing lattice towers; the beacons on top fire on the kick and a ring
        // of signal leaves them
        for (int i = 0; i < 3; i++) {
            float fi = float(i);
            float cx = -0.66 + fi * 0.62 + (fi == 1.0 ? 0.20 : 0.0);
            float h = 0.30 - 0.05 * fi + (fi == 1.0 ? 0.08 : 0.0);
            float bw = h * 0.16;
            vec2 top = vec2(cx, h);
            float d = min(sdSeg(q, vec2(cx - bw, 0.0), top), sdSeg(q, vec2(cx + bw, 0.0), top));
            for (int k = 0; k < 6; k++) {
                float k0 = float(k) / 6.0;
                float k1 = float(k + 1) / 6.0;
                vec2 l0 = vec2(cx - bw * (1.0 - k0), h * k0);
                vec2 r0 = vec2(cx + bw * (1.0 - k0), h * k0);
                vec2 l1 = vec2(cx - bw * (1.0 - k1), h * k1);
                vec2 r1 = vec2(cx + bw * (1.0 - k1), h * k1);
                d = min(d, min(sdSeg(q, l0, r1), sdSeg(q, r0, l1)));
                d = min(d, sdSeg(q, l0, r0));
            }
            d = min(d, sdSeg(q, top, top + vec2(0.0, h * 0.14)));
            float vis = step(0.0, q.y);
            col = mix(col, dark, smoothstep(0.004, 0.002, d) * vis);
            col += mix(u_grid, u_sunB, 0.4) * (smoothstep(0.0022 + px, 0.0022 - px, d) * 0.55 + exp(-d * 260.0) * 0.12) * vis;
            vec2 bp = q - (top + vec2(0.0, h * 0.14));
            float bl = length(bp);
            col += vec3(1.0, 0.22, 0.34) * (smoothstep(0.007, 0.002, bl) * (0.6 + 1.8 * u_mood2.z) + exp(-bl * 60.0) * 0.5 * u_mood2.z);
            float sig = fract(u_time * 0.5 + fi * 0.33);
            col += vec3(1.0, 0.3, 0.45) * smoothstep(0.004, 0.0, abs(bl - sig * 0.22)) * (1.0 - sig) * 0.35 * step(bp.y, 0.02 + bl);
        }
    } else if (pr > 2.5 && pr < 3.5) {
        // city: a hazy far row and a near row of towers with a lit front and a dark side wall,
        // outlined in light like the mountains. The windows work as a level meter: each tower's
        // lit floors rise and fall with the bass, flare on the hats, and the masts blink on the kick.
        for (int layer = 0; layer < 2; layer++) {
            float fl = float(layer); // 0 = far, 1 = near
            float cw = mix(0.085, 0.064, fl);
            float xx = q.x + fl * 0.031;
            float id = floor(xx / cw);
            float fx = fract(xx / cw);
            float h1 = hash(vec2(id, 9.1 + fl * 4.0));
            float h2 = hash(vec2(id, 2.3 + fl * 7.0));
            float town = 0.55 + 0.45 * cos(clamp(q.x * 1.4, -3.1, 3.1));
            float tall = (mix(0.17, 0.10, fl) + mix(0.15, 0.24, fl) * h1 * h1) * town;
            float wid = 0.50 + 0.32 * h2;
            float lo = 0.5 - wid * 0.5;
            float hi = 0.5 + wid * 0.5;
            float sidew = 0.13;                       // the side wall, as a share of the lot
            float ex = px / cw;
            // front face
            float inx = smoothstep(lo - ex, lo + ex, fx) * smoothstep(hi + ex, hi - ex, fx);
            float front = inx * smoothstep(tall + px, tall - px, q.y);
            // side wall: to the right of the front, its roof line slanting back
            float sx = (fx - hi) / sidew;
            float sroof = tall - sx * 0.022;
            float side = step(0.0, sx) * step(sx, 1.0) * smoothstep(sroof + px, sroof - px, q.y) * step(sx * 0.022, q.y + 0.022);
            float mast = step(0.8, h1) * smoothstep(0.0016 + px, 0.0016 - px, abs((fx - 0.5) * cw)) * step(q.y, tall * 1.36) * step(tall, q.y);
            float vis = step(0.0, q.y);
            vec3 haze = mix(u_c1, u_sunB, 0.3) * 0.5;
            vec3 fcol = mix(haze, mix(dark, u_c1 * 0.55, 0.35 + 0.4 * q.y / max(tall, 0.01)), fl);
            vec3 scol2 = mix(haze * 0.8, dark * 0.6, fl);
            col = mix(col, scol2, side * vis * mix(0.8, 1.0, fl));
            col = mix(col, fcol, max(front, mast) * vis * mix(0.8, 1.0, fl));
            // outline: roof and both edges of the front face glow
            float de = min(min(abs(fx - lo), abs(fx - hi)) * cw, abs(q.y - tall));
            float edge = smoothstep(0.0022 + px, 0.0022 - px, de) * step(q.y, tall + 0.003) * step(lo - 0.02, fx) * step(fx, hi + 0.02) * vis;
            col += mix(u_grid, u_sunB, 0.5) * edge * mix(0.18, 0.55, fl);
            // windows as a level meter
            float ncol = 3.0 + floor(h2 * 3.0);
            vec2 wv = vec2((fx - lo) / wid * ncol, q.y / 0.0135);
            vec2 wf = fract(wv);
            float wm = smoothstep(0.2, 0.28, wf.x) * smoothstep(0.8, 0.72, wf.x) * smoothstep(0.24, 0.34, wf.y) * smoothstep(0.72, 0.62, wf.y);
            float level = tall * (0.25 + 0.75 * clamp(u_bass * (0.6 + 0.8 * h1) + 0.18 * sin(u_time * (1.5 + h2 * 2.0) + id * 1.7), 0.0, 1.0));
            float below = step(q.y, level);
            float stray = step(0.82, hash(floor(wv) + vec2(id * 13.0, fl * 50.0)));
            vec3 wc = mix(vec3(1.0, 0.82, 0.45), vec3(0.45, 0.9, 1.0), step(0.6, h2));
            wc = mix(wc, vec3(1.0, 0.35, 0.6), smoothstep(0.7, 1.0, q.y / max(tall, 0.01)));
            col += wc * front * wm * (below * (0.55 + 0.6 * u_mood2.y) + stray * 0.25) * mix(0.3, 1.0, fl);
            float tip = smoothstep(0.005, 0.0015, length(vec2((fx - 0.5) * cw, q.y - tall * 1.36))) * step(0.8, h1) * fl;
            col += vec3(1.0, 0.2, 0.3) * tip * (0.5 + 1.6 * u_mood2.z);
        }
    } else if (pr > 3.5 && pr < 4.5) {
        // meteors: a few streaks falling across the sky
        for (int i = 0; i < 3; i++) {
            float fi = float(i);
            float slot = floor(u_time * 0.35 + fi * 0.37);
            float hs = hash(vec2(slot, fi * 3.7));
            float ph = fract(u_time * 0.35 + fi * 0.37);
            vec2 a = vec2(-0.7 + 1.4 * hs, 0.5 + 0.12 * fi) + vec2(0.55, -0.3) * ph * 1.6;
            vec2 bq = a - vec2(0.55, -0.3) * 0.16;
            float d = sdSeg(q, a, bq);
            float fade = smoothstep(0.0, 0.1, ph) * smoothstep(0.7, 0.4, ph) * step(0.35, hs);
            col += mix(u_sunA, vec3(1.0), 0.6) * smoothstep(0.004, 0.0, d) * fade * smoothstep(0.0, 0.16, length(q - bq)) * 0.9;
        }
    } else if (pr > 4.5 && pr < 5.5) {
        // sailboats: three of them crossing the horizon, rocking with the swell. The sails are
        // lit like the sun (a gradient cut by scan bars) and catch the kick.
        for (int i = 0; i < 3; i++) {
            float fi = float(i);
            float sz = 0.088 - 0.018 * fi;
            float bx = fract(0.21 + fi * 0.37 + u_time * (0.004 + 0.0015 * fi)) * 2.6 - 1.3;
            float bob = sin(u_time * 1.3 + fi * 2.0) * 0.003 + 0.004 * u_bass;
            vec2 c = (q - vec2(bx, 0.002 + bob)) / sz;
            if (abs(c.x) > 1.4 || c.y > 2.3) continue;
            float rk = sin(u_time * 1.1 + fi * 1.7) * 0.07;
            c = vec2(c.x * cos(rk) - c.y * sin(rk), c.x * sin(rk) + c.y * cos(rk));
            float hull = max(abs(c.y - 0.09) - 0.09, abs(c.x) - (0.62 + c.y * 1.6));
            float mast = max(abs(c.x) - 0.02, abs(c.y - 1.05) - 0.95);
            float mainS = max(max(0.06 - c.x, 0.26 - c.y), ((c.x - 0.06) / 0.85 + (c.y - 0.26) / 1.70 - 1.0) * 0.76);
            float jib = max(max(c.x + 0.06, 0.30 - c.y), ((-c.x - 0.06) / 0.66 + (c.y - 0.30) / 1.35 - 1.0) * 0.6);
            float sail = min(mainS, jib);
            float e2 = 2.2 * px / sz;
            vec3 sc2 = mix(mix(u_sunB, vec3(1.0), 0.45), mix(u_sunA, vec3(1.0), 0.7), clamp(c.y / 2.0, 0.0, 1.0)) * (0.8 + 0.4 * u_mood2.z);
            float bars = c.y > 1.0 ? 1.0 : step(0.14 + 0.36 * (1.0 - c.y), fract(c.y * 5.0 - u_time * 0.2));
            float vis = step(0.0, q.y);
            col += sc2 * 0.16 * exp(-max(sail, 0.0) * 5.0) * vis;
            col = mix(col, u_c0 * 0.12, smoothstep(2.5 * e2, 0.5 * e2, sail) * 0.85 * vis);
            col = mix(col, sc2, smoothstep(e2, -e2, sail + 1.4 * e2) * bars * vis);
            col = mix(col, u_c0 * 0.12, smoothstep(e2, -e2, min(hull, mast)) * vis);
            col += mix(u_sunB, vec3(1.0), 0.3) * smoothstep(1.5 * e2, 0.0, abs(hull)) * 0.5 * vis;
        }
    } else if (pr > 5.5 && pr < 6.5) {
        // lighthouse: a striped tower on a rock. Its lamp swings a beam across the sky and
        // flares as it passes the viewer and on the kick.
        vec2 c = q - vec2(0.60, 0.0);
        float H = 0.26;
        float rock = length(vec2(c.x * 0.45, c.y + 0.012)) - 0.045;
        float wdt = mix(0.026, 0.015, clamp(c.y / H, 0.0, 1.0));
        float tower = max(abs(c.x) - wdt, max(-c.y, c.y - H));
        float room = max(abs(c.x) - 0.022, abs(c.y - H - 0.016) - 0.016);
        float cap = max(abs(c.x) - (0.026 - (c.y - H - 0.032) * 1.1), max(H + 0.032 - c.y, c.y - H - 0.055));
        float d = min(min(rock, tower), cap);
        vec2 lp = c - vec2(0.0, H + 0.016);
        float ang = u_time * 0.7;
        vec2 bd = normalize(vec2(cos(ang), 0.22 * sin(ang) + 0.05));
        float along = dot(lp, bd);
        float across = length(lp - bd * along);
        float facing = abs(cos(ang)); // 1 = swinging sideways, 0 = pointing at the viewer
        float beam = exp(-across / (0.008 + 0.10 * abs(along))) * exp(-abs(along) * 1.6) * (0.25 + 0.75 * facing);
        vec3 lc = mix(vec3(1.0, 0.95, 0.75), u_sunA, 0.3);
        col += lc * beam * (0.5 + 0.5 * u_mood2.z) * step(0.0, q.y);
        col = propPaint(col, d, q.y, px);
        float ll = length(lp);
        col = mix(col, lc, smoothstep(2.0 * px, -2.0 * px, room) * 0.6);
        col += lc * (smoothstep(0.014, 0.004, ll) * (0.8 + 1.5 * (1.0 - facing)) + exp(-ll * 40.0) * (0.25 + 0.9 * (1.0 - facing) + 0.6 * u_mood2.z));
    } else if (pr > 6.5 && pr < 7.5) {
        // big wheel: turning slowly. A light runs round its cabins, the hats flash them all and
        // the hub glows on the kick.
        vec2 hub = vec2(0.50, 0.175);
        float R = 0.14;
        vec2 c = q - hub;
        float rl = length(c);
        float d = abs(rl - R) - 0.004;
        d = min(d, abs(rl - R * 0.86) - 0.0015);
        d = min(d, rl - 0.012);
        float rot = u_time * 0.12;
        for (int k = 0; k < 6; k++) {
            float a = rot + float(k) * 0.5236;
            vec2 dv = vec2(cos(a), sin(a));
            d = min(d, sdSeg(c, -dv * R, dv * R) - 0.0014);
        }
        d = min(d, sdSeg(q, hub, vec2(hub.x - 0.07, 0.0)) - 0.004);
        d = min(d, sdSeg(q, hub, vec2(hub.x + 0.07, 0.0)) - 0.004);
        col = propPaint(col, d, q.y, px);
        for (int k = 0; k < 12; k++) {
            float a = rot + float(k) * 0.5236;
            vec2 cp = c - vec2(cos(a), sin(a)) * R + vec2(0.0, 0.012);
            float cl = length(cp);
            float on = 0.35 + 0.65 * pow(0.5 + 0.5 * sin(u_time * 2.0 - float(k) * 0.5236), 4.0) + u_mood2.y * 0.6;
            vec3 cc = mix(u_sunA, u_sunB, mod(float(k), 2.0));
            col = mix(col, cc * 0.9, smoothstep(0.011, 0.008, cl));
            col += cc * exp(-cl * 70.0) * 0.35 * on;
        }
        col += u_sunA * exp(-rl * 45.0) * (0.2 + 0.8 * u_mood2.z);
    } else if (pr > 7.5 && pr < 8.5) {
        // birds: a loose flight crossing the sky, wings beating
        for (int i = 0; i < 7; i++) {
            float fi = float(i);
            vec2 lead = vec2(fract(u_time * 0.018 + 0.13) * 3.0 - 1.5, 0.30 + 0.05 * sin(u_time * 0.3));
            float row = floor((fi + 1.0) * 0.5);
            float sgn = mod(fi, 2.0) < 0.5 ? 1.0 : -1.0;
            vec2 c = q - (lead + vec2(-row * 0.05, sgn * row * 0.022 + 0.006 * sin(u_time * 1.7 + fi)));
            if (abs(c.x) > 0.05 || abs(c.y) > 0.05) continue;
            float flap = 0.35 + 0.55 * sin(u_time * 5.0 + fi * 0.9);
            vec2 wl = vec2(-0.022 * cos(flap), 0.022 * sin(flap));
            vec2 wr = vec2(0.022 * cos(flap), 0.022 * sin(flap));
            float d = min(sdSeg(c, vec2(0.0), wl), sdSeg(c, vec2(0.0), wr)) - 0.0022;
            col = mix(col, u_c0 * 0.12, smoothstep(1.5 * px, -1.5 * px, d));
            col += mix(u_sunB, vec3(1.0), 0.3) * smoothstep(3.0 * px, 0.0, abs(d)) * 0.3;
        }
    } else if (pr > 8.5) {
        // balloons: striped like the sun, drifting and bobbing. The burner under each one
        // glows on the kick.
        for (int i = 0; i < 4; i++) {
            float fi = float(i);
            float hsh = hash(vec2(fi, 4.2));
            float R = 0.055 - 0.008 * fi;
            vec2 bc = vec2(fract(0.15 + fi * 0.27 + u_time * (0.003 + 0.002 * hsh)) * 2.6 - 1.3, 0.22 + 0.16 * hsh + 0.02 * sin(u_time * 0.5 + fi * 2.0));
            vec2 c = q - bc;
            if (abs(c.x) > 0.09 || abs(c.y) > 0.16) continue;
            // the envelope: a ball drawn in toward its mouth
            float env = length(vec2(c.x * (1.0 + 0.6 * clamp(-c.y / R, 0.0, 1.3)), c.y * (c.y < 0.0 ? 0.8 : 1.0))) - R;
            float yn = c.y / R;
            vec3 col1 = mix(u_sunA, u_sunB, hsh);
            vec3 bcol = mix(col1, mix(u_grid, vec3(1.0), 0.3), clamp(yn * 0.5 + 0.5, 0.0, 1.0));
            float bars = yn > 0.2 ? 1.0 : step(0.12 + 0.3 * (0.2 - yn), fract(yn * 3.5 - u_time * 0.15));
            float e2 = 2.0 * px;
            col += col1 * 0.14 * exp(-max(env, 0.0) * 40.0);
            col = mix(col, bcol * 0.8, smoothstep(e2, -e2, env) * bars);
            vec2 bk = c + vec2(0.0, R * 1.75);
            float basket = max(abs(bk.x) - R * 0.16, abs(bk.y) - R * 0.12);
            col = mix(col, u_c0 * 0.15, smoothstep(e2, -e2, basket));
            col += vec3(1.0, 0.7, 0.3) * exp(-length(c + vec2(0.0, R * 1.35)) * 90.0) * (0.25 + 1.2 * u_mood2.z);
        }
    }
    return col;
}

// Drift: synthwave vista (sky, body, props). The grid, mountains and heartline are geometry.
vec3 vista(vec2 p) {
    float hz = u_vista.x;
    vec3 col;
    if (p.y < hz) {
        float t = p.y / hz;
        float k = u_tr.x;
        if (k >= 0.999) {
            col = skyProp(u_mood2.x, p, skyBody(u_mood.x, p, skyBase(u_mood.x, p), 0.0), 0.0);
        } else {
            // Transition: the old sky sinks behind the horizon while the new one rises out of it.
            float ko = smoothstep(0.0, 0.55, k);
            float kn = smoothstep(0.3, 1.0, k);
            vec3 oldc = skyProp(u_tr.z, p, skyBody(u_tr.y, p, skyBase(u_tr.y, p), ko * ko * 0.75), ko * 0.45);
            vec3 newc = skyProp(u_mood2.x, p, skyBody(u_mood.x, p, skyBase(u_mood.x, p), (1.0 - kn) * (1.0 - kn) * 0.75), (1.0 - kn) * 0.45);
            col = mix(oldc, newc, smoothstep(0.35, 0.65, k));
            // a band of light sweeps up the sky, and scan lines race along it
            float bell = sin(k * 3.14159);
            float sweep = exp(-pow((t - (1.0 - k) * 1.2) * 7.0, 2.0));
            float lines = step(0.55, hash(vec2(floor(p.y * u_res.y / 3.0), floor(u_time * 30.0))));
            col += mix(u_sunB, vec3(1.0), 0.5) * sweep * bell * (0.35 + 0.45 * lines * smoothstep(0.0, 1.0, abs(fract(p.x * 3.0 - u_time * 2.0) - 0.5) * 2.0));
            col += u_sunB * bell * 0.5 * exp(-(hz - p.y) * 14.0);
        }
        // a chime lights a star where its note sits in pitch (low notes left, high notes right);
        // ear candy sends a shimmer through the whole sky
        if (u_fx.x > 0.01) {
            vec2 sq = (p - vec2(0.12 + 0.76 * u_fx.y, 0.07 + 0.16 * fract(u_fx.y * 7.3))) * vec2(u_res.x / u_res.y, 1.0);
            float fl = exp(-abs(sq.x) * 240.0) * exp(-abs(sq.y) * 20.0) + exp(-abs(sq.y) * 240.0) * exp(-abs(sq.x) * 20.0);
            col += mix(u_sunA, vec3(1.0), 0.6) * (fl * 0.9 + exp(-dot(sq, sq) * 3000.0) * 1.6) * u_fx.x;
        }
        if (u_fx.z > 0.01) {
            col += vec3(0.9, 0.95, 1.0) * stars(p * u_res + 5.0, 5.0 * u_res.y / 1080.0, 0.35) * u_fx.z * (1.0 - t);
            col += u_sunA * 0.25 * u_fx.z * exp(-(hz - p.y) * 10.0);
        }
        // rain: three depths of streaks. It thickens with the song's energy and every hat
        // brightens it; a little mist hangs on the horizon.
        float we = u_mood.y;
        vec2 wg = vec2(p.x * u_res.x / u_res.y, p.y);
        if (we > 1.5 && we < 2.5) {
            // motes: embers rising slowly; every hat makes them glint
            float mo = stars((wg + vec2(0.012 * sin(u_time * 0.4 + wg.y * 9.0), u_time * 0.016)) * u_res.y, 44.0 * u_res.y / 1080.0, 0.22)
                + stars((wg + vec2(0.02 * sin(u_time * 0.3 + wg.y * 5.0), u_time * 0.03)) * u_res.y + 31.0, 80.0 * u_res.y / 1080.0, 0.25);
            col += mix(u_sunA, vec3(1.0), 0.35) * mo * (0.5 + 1.2 * u_mood2.y);
        } else if (we > 2.5 && we < 3.5) {
            // veil: a faint curtain of aurora high in the sky
            float ax = p.x * 5.0 + u_time * 0.12;
            float au = sin(ax + sin(ax * 2.3 - u_time * 0.2) * 1.3) * 0.5 + 0.5;
            col += mix(u_grid, u_sunA, au) * 0.16 * smoothstep(0.55, 0.0, t) * (0.4 + 0.6 * au) * (1.0 + 0.6 * u_swell);
        } else if (we > 3.5 && we < 4.5) {
            // snow: three depths of flakes swaying down; the hats make them sparkle
            float sn = 0.0;
            for (int i = 0; i < 3; i++) {
                float fi = float(i);
                vec2 sp2 = wg + vec2(0.02 * sin(u_time * (0.5 + 0.2 * fi) + wg.y * (6.0 - fi)) + fi * 0.37, -u_time * (0.035 + 0.03 * fi));
                sn += stars(sp2 * u_res.y, (26.0 + 22.0 * fi) * u_res.y / 1080.0, 0.5) * (0.5 + 0.3 * fi);
            }
            col += vec3(0.9, 0.95, 1.0) * sn * (0.55 + 0.8 * u_mood2.y);
            col = mix(col, vec3(0.75, 0.82, 0.95) * 0.5, 0.16 * exp(-(hz - p.y) * 8.0));
        } else if (we > 4.5 && we < 5.5) {
            // storm: heavy cloud overhead, and every kick throws a bolt
            float cl = fbm(vec2(p.x * 3.0 + u_time * 0.02, p.y * 9.0 - u_time * 0.01));
            float cover = smoothstep(0.38, 0.7, cl) * smoothstep(0.75, 0.05, t);
            col = mix(col, u_c0 * 0.25 + vec3(0.02, 0.02, 0.05), cover * 0.7);
            float slot = floor(u_time * 1.1);
            float bx = 0.12 + 0.76 * hash(vec2(slot, 3.3));
            float jx = bx + (vnoise(vec2(p.y * 11.0, slot)) - 0.5) * 0.14 + (vnoise(vec2(p.y * 37.0, slot + 7.0)) - 0.5) * 0.03;
            float bd = abs(p.x - jx) * u_res.x / u_res.y;
            float reach = hz * (0.55 + 0.45 * hash(vec2(slot, 9.1)));
            float bolt = (smoothstep(0.0035, 0.0008, bd) + exp(-bd * 70.0) * 0.35) * step(p.y, reach);
            col += vec3(0.78, 0.86, 1.0) * bolt * u_mood2.z * 1.8;
            col += vec3(0.45, 0.55, 0.9) * u_mood2.z * (0.10 + 0.30 * cover) * exp(-abs(p.x - bx) * 2.5);
        } else if (we > 5.5 && we < 6.5) {
            // mist: banks lying along the horizon, thicker with the bass
            float mst = fbm(vec2(p.x * 2.4 + u_time * 0.03, p.y * 13.0 + u_time * 0.01));
            float lay = smoothstep(0.36, 0.72, mst) * exp(-(hz - p.y) * 4.5);
            col = mix(col, mix(u_c1, vec3(0.8, 0.85, 1.0), 0.35) * 0.6, lay * (0.5 + 0.3 * u_bass));
        } else if (we > 6.5 && we < 7.5) {
            // wind: long streaks racing across the sky; the hats gust them
            vec2 g2 = vec2(p.y * 1.8, -wg.x * 0.5);
            float wn = rainLayer(g2, 70.0, 0.5, 0.35, 4.4) * 0.6 + rainLayer(g2 + 0.3, 30.0, 0.8, 0.5, 9.2);
            col += mix(u_grid, vec3(1.0), 0.6) * wn * (0.10 + 0.35 * u_mood2.y);
        } else if (we > 7.5 && we < 8.5) {
            // cloud: long banks drifting across, lit from below by whatever is in the sky
            float cl = fbm(vec2(p.x * 2.6 + u_time * 0.025, p.y * 22.0 - u_time * 0.004));
            float cl2 = fbm(vec2(p.x * 5.0 - u_time * 0.04, p.y * 30.0 + 9.0));
            float cm = smoothstep(0.50, 0.72, cl * 0.75 + cl2 * 0.25) * smoothstep(0.02, 0.35, t) * smoothstep(1.0, 0.75, t);
            vec3 under = mix(u_sunB, u_sunA, 0.4) * (0.75 + 0.5 * u_bass);
            col = mix(col, mix(u_c0 * 0.35, under, smoothstep(0.58, 0.8, cl)), cm * 0.72);
        } else if (we > 8.5 && we < 9.5) {
            // petals: soft flecks tumbling across on the wind; the hats make them flash
            float pt = 0.0;
            for (int i = 0; i < 2; i++) {
                float fi = float(i);
                vec2 pp = wg + vec2(u_time * (0.05 + 0.03 * fi) + 0.03 * sin(u_time * 0.7 + wg.y * 7.0), -u_time * (0.025 + 0.015 * fi) + 0.02 * sin(wg.x * 5.0 + u_time * 0.5)) + fi * 0.41;
                float cell = 0.05 + 0.03 * fi;
                vec2 id = floor(pp / cell);
                vec2 f = fract(pp / cell) - 0.5;
                float h = hash(id + fi * 7.0);
                if (h > 0.55) {
                    float a = u_time * (1.0 + 2.0 * h) + h * 30.0;
                    vec2 r2 = f - (vec2(hash(id + 1.3), hash(id + 2.7)) - 0.5) * 0.5;
                    r2 = vec2(r2.x * cos(a) - r2.y * sin(a), r2.x * sin(a) + r2.y * cos(a));
                    pt += smoothstep(0.10, 0.06, length(r2 * vec2(1.0, 2.6 + 1.5 * sin(a * 1.3)))) * (0.6 + 0.4 * fi);
                }
            }
            col += mix(vec3(1.0, 0.55, 0.75), u_sunA, 0.35) * pt * (0.45 + 0.8 * u_mood2.y);
        } else if (we > 9.5) {
            // shafts: rays of light fanning out across the sky from the body, turning slowly
            // and swelling with the bass
            vec2 sq = (p - vec2(0.5, u_vista.y)) * vec2(u_res.x / u_res.y, 1.0);
            float sa = atan(sq.y, sq.x);
            float sr = length(sq);
            float rays = pow(0.5 + 0.5 * sin(sa * 9.0 + u_time * 0.12), 3.0) * 0.6 + pow(0.5 + 0.5 * sin(sa * 23.0 - u_time * 0.2 + 1.7), 6.0) * 0.4;
            col += mix(u_sunB, u_sunA, 0.5) * rays * exp(-sr * 1.6) * smoothstep(u_vista.z * 0.9, u_vista.z * 1.8, sr) * (0.16 + 0.22 * u_bass + 0.12 * u_mood2.z);
        }
        if ((we > 0.5 && we < 1.5) || (we > 4.5 && we < 5.5)) {
            vec2 g = vec2(p.x * u_res.x / u_res.y, p.y);
            float beat = 0.45 + 0.9 * u_mood2.y + 0.3 * u_swell;
            float rn = rainLayer(g, 150.0, 1.5, 0.10, 5.5) * 0.35 + rainLayer(g, 80.0, 2.3, 0.16, 8.1) * 0.6 + rainLayer(g, 34.0, 3.4, 0.26, 2.7) * 1.0;
            col += mix(u_grid, vec3(0.85, 0.92, 1.0), 0.55) * rn * 0.34 * beat;
            col = mix(col, u_c1 * 0.6 + vec3(0.05), 0.22 * exp(-(hz - p.y) * 9.0));
        }
        col += u_sunB * 0.35 * exp(-(hz - p.y) * 30.0);
    } else {
        float y = (p.y - hz) / (1.0 - hz);
        col = mix(u_c0 * 0.4, u_c1 * 0.45, y);
        col += u_sunB * 0.22 * exp(-y * 6.0) * exp(-abs(p.x - 0.5) * 3.0);
        col += u_sunB * 0.25 * exp(-y * 40.0);
        // the ground takes the sky's colour near the horizon, and the body throws a path of light
        float sett = step(0.999, u_tr.x);
        vec3 hcol = vec3(0.0);
        float pathw = 0.0;
        if (u_mood.x > 0.5 && u_mood.x < 1.5) { hcol = vec3(0.45, 0.65, 1.0); pathw = 0.6; }
        else if (u_mood.x > 1.5 && u_mood.x < 2.5) { hcol = vec3(1.0, 0.55, 0.32); pathw = 1.0; }
        else if (u_mood.x > 4.5 && u_mood.x < 5.5) { hcol = vec3(0.9, 0.6, 0.8); pathw = 0.5; }
        else if (u_mood.x > 6.5 && u_mood.x < 7.5) { hcol = vec3(1.0, 0.5, 0.55); pathw = 0.7; }
        else if (u_mood.x > 7.5 && u_mood.x < 8.5) { hcol = vec3(0.2, 0.9, 0.6); pathw = 0.0; }
        else if (u_mood.x > 2.5 && u_mood.x < 3.5) { hcol = vec3(1.0, 0.6, 0.7); pathw = 0.5; }
        else if (u_mood.x > 10.5 && u_mood.x < 11.5) { hcol = u_sunA; pathw = 0.45; }
        else if (u_mood.x > 11.5 && u_mood.x < 12.5) { hcol = u_sunB; pathw = 0.8; }
        col += hcol * 0.16 * exp(-y * 7.0) * sett;
        if (pathw > 0.0) {
            float wpath = (0.03 + 0.28 * y) * pathw;
            float path = exp(-pow((p.x - 0.5) / wpath, 2.0));
            col += hcol * path * 0.20 * (1.0 - y * 0.7) * (1.0 + 0.5 * u_bass) * sett;
        }
        if ((u_mood.y > 0.5 && u_mood.y < 1.5) || (u_mood.y > 4.5 && u_mood.y < 5.5)) {
            // splashes: rings opening on the ground, in perspective
            vec2 gp = vec2((p.x - 0.5) / (y + 0.06) * 2.2, 0.9 / (y + 0.06));
            vec2 cell = floor(gp);
            float ph = fract(u_time * 1.3 + hash(cell) * 9.0);
            vec2 cpos = vec2(hash(cell + 3.1), hash(cell + 7.7)) * 0.5 + 0.25;
            float rd = length(fract(gp) - cpos);
            float ring = smoothstep(0.05, 0.0, abs(rd - ph * 0.3)) * (1.0 - ph) * step(0.35, hash(cell + 1.9));
            col += mix(u_grid, vec3(0.9, 0.95, 1.0), 0.5) * ring * (0.16 + 0.4 * u_mood2.y) * smoothstep(0.0, 0.08, y);
            vec2 g = vec2(p.x * u_res.x / u_res.y, p.y);
            col += mix(u_grid, vec3(0.85, 0.92, 1.0), 0.55) * rainLayer(g, 34.0, 3.4, 0.26, 2.7) * 0.2 * (0.45 + 0.9 * u_mood2.y);
        }
        col += u_sunB * sin(u_tr.x * 3.14159) * 0.3 * exp(-y * 9.0) * step(u_tr.x, 0.999);
    }
    return col;
}

void main() {
    vec2 q = u_cam + (uv - 0.5) * u_res / u_scale;
    vec3 col = topdown(q);
    if (u_idle > 0.003) {
        col = mix(col, vista(uv), u_idle);
    }
    if (u_warp > 0.01) {
        vec2 c = (uv - 0.5) * vec2(u_res.x / u_res.y, 1.0);
        float r = length(c);
        float ang = atan(c.y, c.x);
        float sa = ang * 38.0;
        float id = floor(sa);
        float h = hash(vec2(id, 1.7));
        float w = abs(fract(sa) - 0.5);
        float s = fract(r * 1.4 - u_time * 2.2 * (0.6 + h) + h * 7.0);
        float st = step(0.55, h) * smoothstep(0.12, 0.02, w) * smoothstep(0.0, 0.03, s) * smoothstep(0.4, 0.0, s) * smoothstep(0.05, 0.45, r);
        col += mix(u_grid, vec3(1.0), 0.4) * st * u_warp * 0.55;
    }
    // music pump: build-up desaturates (x), the drop over-saturates (y), kicks brighten
    float l = dot(col, vec3(0.299, 0.587, 0.114));
    col = max(mix(vec3(l), col, 1.0 - u_pump.x + u_pump.y), 0.0);
    col *= 1.0 + u_kick;
    gl_FragColor = vec4(col, 1.0);
}
"#;

const BLUR_FRAG: &str = r#"#version 100
precision highp float;
varying vec2 uv;
uniform sampler2D Texture;
uniform vec2 u_dir;
uniform float u_thresh;

vec3 samp(vec2 p) {
    vec3 c = texture2D(Texture, p).rgb;
    if (u_thresh > 0.0) {
        float l = max(c.r, max(c.g, c.b));
        c *= smoothstep(u_thresh, u_thresh + 0.4, l);
    }
    return c;
}

void main() {
    vec3 c = samp(uv) * 0.227027;
    c += (samp(uv + u_dir * 1.3846) + samp(uv - u_dir * 1.3846)) * 0.316216;
    c += (samp(uv + u_dir * 3.2308) + samp(uv - u_dir * 3.2308)) * 0.070270;
    gl_FragColor = vec4(c, 1.0);
}
"#;

const COMP_FRAG: &str = r#"#version 100
precision highp float;
varying vec2 uv;
uniform sampler2D Texture;
uniform sampler2D Bloom1;
uniform sampler2D Bloom2;
uniform vec2 u_res;
uniform float u_time;
uniform float u_aberr;
uniform float u_flash;
uniform vec3 u_flashcol;
uniform float u_bloom;
uniform float u_warp;
uniform float u_expo;
uniform float u_vig;

vec3 scene(vec2 p, vec2 off) {
    return vec3(texture2D(Texture, p + off).r, texture2D(Texture, p).g, texture2D(Texture, p - off).b);
}

void main() {
    vec2 c = uv - 0.5;
    float r2 = dot(c, c);
    vec2 off = c * (0.0012 + u_aberr * 0.008 + u_warp * 0.018) * (0.3 + r2 * 2.4);
    vec3 col = scene(uv, off);
    if (u_warp > 0.02) {
        vec2 dz = c * 0.07 * u_warp;
        col = col * 0.4 + (scene(uv - dz * 0.33, off) + scene(uv - dz * 0.66, off) + scene(uv - dz, off)) * 0.2;
    }
    vec3 b1 = texture2D(Bloom1, uv).rgb;
    vec3 b2 = texture2D(Bloom2, uv).rgb;
    col += (b1 * 0.6 + b2 * 0.85) * u_bloom;
    // soft shoulder so stacked glow never clips harshly
    col *= u_expo;
    col = col / (1.0 + max(col - 1.0, 0.0) * 0.6);
    float scan = 0.96 + 0.04 * sin(uv.y * u_res.y * 3.14159);
    col *= scan;
    float vig = smoothstep(1.05 - 0.3 * u_vig, 0.3 - 0.12 * u_vig, sqrt(r2) * 1.3);
    col *= mix(0.55 - 0.2 * u_vig, 1.0, vig);
    col += u_flashcol * u_flash;
    float n = fract(sin(dot(uv * u_res + u_time, vec2(12.9898, 78.233))) * 43758.5453);
    col += (n - 0.5) * 0.02;
    gl_FragColor = vec4(col, 1.0);
}
"#;

fn mat(frag: &str, uniforms: &[(&str, UniformType)], textures: &[&str], pipeline: PipelineParams) -> Material {
    load_material(
        ShaderSource::Glsl { vertex: VERT, fragment: frag },
        MaterialParams {
            uniforms: uniforms.iter().map(|(n, t)| UniformDesc::new(n, *t)).collect(),
            textures: textures.iter().map(|s| s.to_string()).collect(),
            pipeline_params: pipeline,
        },
    )
    .unwrap_or_else(|e| panic!("shader failed to compile: {e:?}"))
}

// ---------------------------------------------------------------------------------------------
// Renderer
// ---------------------------------------------------------------------------------------------

pub struct HudInfo {
    /// Whole-song intensity curve + section marks (None outside file songs).
    pub timeline: Option<std::sync::Arc<Timeline>>,
    pub title: String,
    pub progress: f32,
    pub time_left: f32,
    pub bpm: f32,
    pub mode: String,
    pub status: String,
    pub track_no: String,
    pub show_combat: bool,
    /// Large centred announcement (e.g. "TRACK CLEAR") and its opacity.
    pub banner: String,
    pub banner_a: f32,
}

impl HudInfo {
    pub fn empty() -> Self {
        HudInfo {
            timeline: None,
            title: String::new(),
            progress: 0.0,
            time_left: 0.0,
            bpm: 0.0,
            mode: String::new(),
            status: String::new(),
            track_no: String::new(),
            show_combat: false,
            banner: String::new(),
            banner_a: 0.0,
        }
    }
}

pub struct Renderer {
    pw: u32,
    ph: u32,
    pub ss: u32,
    ss_forced: Option<u32>,
    scene: RenderTarget,
    b1a: RenderTarget,
    b1b: RenderTarget,
    b2a: RenderTarget,
    b2b: RenderTarget,
    bg_mat: Material,
    blur_mat: Material,
    comp_mat: Material,
    add_mat: Material,
    pub pal: Palette,
    pal_from: Palette,
    pal_to: u8,
    pal_t: f32,
    pub time: f32,
    /// Menu-like screen: the vista is shown without the player, the Heart or the big heartline.
    pub menu: bool,
    pub view: View,
    // camera / projection
    cam_s: Vec2,
    /// Where the arena should sit, as an offset from the middle of the screen in screen
    /// fractions, and how far the view is pulled back (the codex shows the arena beside its
    /// lists). `shift_s` / `zoom_s` ease toward them.
    pub show_shift: Vec2,
    pub show_zoom: f32,
    shift_s: Vec2,
    zoom_s: f32,
    cam: Vec2,
    // music state (smoothed + history)
    clock: f32,
    spec_s: [f32; N_SPEC],
    low_s: f32,
    swell: f32,
    hist: [[f32; N_SPEC]; HIST],
    hist_head: usize,
    hist_acc: f32,
    ecg: [f32; ECG_N],
    ecg_head: usize,
    ecg_acc: f32,
    ecg_ev: [(f32, f32); 8],
    ecg_evi: usize,
    prev_hit: [f32; 3],
    last_onset: f32,
    last_rest: f32,
    scroll: f32,
    deep_scroll: f32,
    bar_count: u32,
    prev_bar: f32,
    // wireframes
    solids: Vec<Edges>,
    kinds: Vec<Vec<[Vec3; 2]>>,
    heart_key: (usize, usize),
    heart_from: (Edges, Edges),
    heart_mt: f32,
    /// The look being shown, the one it replaced, and the 0..1 transition between them.
    mood_cur: Mood,
    mood_old: Mood,
    mood_k: f32,
    /// Highway and Rally draw on the grid floor, so the vista keeps its mountains there.
    plain_ground: bool,
    broken_t: f32,
    // pulse layer (see the consts at the top)
    /// Reactive amplitude scale from the VISUALS setting (1 / 0.45 / 0).
    pub fx: f32,
    kick: f32,
    kick_luma: f32,
    last_kick: f32,
    ripples: [(f32, f32); 4],
    ripple_i: usize,
    snare: f32,
    rings: [(f32, f32); SNARE_RINGS],
    ring_i: usize,
    hat: f32,
    hat_n: u32,
    bass_b: f32,
    busy: f32,
    scope: [f32; SCOPE_N],
    scope_ok: bool,
    scope_peak: f32,
    next_big: f32,
    tension: f32,
    tension_pk: f32,
    drop: f32,
    prev_section: (u16, SectionKind),
}

fn auto_ss(forced: Option<u32>, pw: u32, ph: u32) -> u32 {
    match forced {
        Some(s) => s.clamp(1, 2),
        None => {
            if (pw as u64) * (ph as u64) <= 2600 * 1500 {
                2
            } else {
                1
            }
        }
    }
}

fn make_rt(w: u32, h: u32) -> RenderTarget {
    let rt = render_target(w.max(8), h.max(8));
    rt.texture.set_filter(FilterMode::Linear);
    rt
}

impl Renderer {
    /// `ss_forced`: supersampling factor (1 or 2); `None` picks 2x unless the window is huge.
    pub fn new(ss_forced: Option<u32>) -> Self {
        let dpi = screen_dpi_scale();
        let pw = (screen_width() * dpi).round().max(64.0) as u32;
        let ph = (screen_height() * dpi).round().max(64.0) as u32;
        let ss = auto_ss(ss_forced, pw, ph);
        let bg_mat = mat(
            BG_FRAG,
            &[
                ("u_res", UniformType::Float2),
                ("u_time", UniformType::Float1),
                ("u_idle", UniformType::Float1),
                ("u_scale", UniformType::Float1),
                ("u_swell", UniformType::Float1),
                ("u_bass", UniformType::Float1),
                ("u_scroll", UniformType::Float1),
                ("u_lv", UniformType::Float1),
                ("u_warp", UniformType::Float1),
                ("u_kick", UniformType::Float1),
                ("u_ripple", UniformType::Float3),
                ("u_pump", UniformType::Float3),
                ("u_cam", UniformType::Float2),
                ("u_arena", UniformType::Float2),
                ("u_vista", UniformType::Float3),
                ("u_c0", UniformType::Float3),
                ("u_c1", UniformType::Float3),
                ("u_grid", UniformType::Float3),
                ("u_sunA", UniformType::Float3),
                ("u_sunB", UniformType::Float3),
                ("u_mood", UniformType::Float3),
                ("u_mood2", UniformType::Float3),
                ("u_tr", UniformType::Float3),
                ("u_fx", UniformType::Float3),
            ],
            &[],
            PipelineParams::default(),
        );
        let blur_mat = mat(BLUR_FRAG, &[("u_dir", UniformType::Float2), ("u_thresh", UniformType::Float1)], &[], PipelineParams::default());
        let comp_mat = mat(
            COMP_FRAG,
            &[
                ("u_res", UniformType::Float2),
                ("u_time", UniformType::Float1),
                ("u_aberr", UniformType::Float1),
                ("u_flash", UniformType::Float1),
                ("u_flashcol", UniformType::Float3),
                ("u_bloom", UniformType::Float1),
                ("u_warp", UniformType::Float1),
                ("u_expo", UniformType::Float1),
                ("u_vig", UniformType::Float1),
            ],
            &["Bloom1", "Bloom2"],
            PipelineParams::default(),
        );
        let add_mat = mat(
            ADD_FRAG,
            &[],
            &[],
            PipelineParams {
                color_blend: Some(BlendState::new(Equation::Add, BlendFactor::Value(BlendValue::SourceAlpha), BlendFactor::One)),
                ..Default::default()
            },
        );
        let pal = palette(0);
        let solids = build_solids();
        let start = (SH_ICOSA, SH_DODECA);
        let heart_from = (solids[start.0], solids[start.1]);
        let mut r = Renderer {
            pw,
            ph,
            ss,
            ss_forced,
            scene: make_rt(pw * ss, ph * ss),
            b1a: make_rt(pw / 2, ph / 2),
            b1b: make_rt(pw / 2, ph / 2),
            b2a: make_rt(pw / 4, ph / 4),
            b2b: make_rt(pw / 4, ph / 4),
            bg_mat,
            blur_mat,
            comp_mat,
            add_mat,
            pal,
            pal_from: pal,
            pal_to: 0,
            pal_t: 1.0,
            menu: false,
            time: 0.0,
            view: View { sw: screen_width(), sh: screen_height(), scale: 1.0, center: Vec2::ZERO },
            cam_s: Vec2::ZERO,
            show_shift: Vec2::ZERO,
            show_zoom: 1.0,
            shift_s: Vec2::ZERO,
            zoom_s: 1.0,
            cam: Vec2::ZERO,
            clock: 0.0,
            spec_s: [0.0; N_SPEC],
            low_s: 0.0,
            swell: 0.0,
            hist: [[0.0; N_SPEC]; HIST],
            hist_head: 0,
            hist_acc: 0.0,
            ecg: [0.0; ECG_N],
            ecg_head: 0,
            ecg_acc: 0.0,
            ecg_ev: [(-10.0, 0.0); 8],
            ecg_evi: 0,
            prev_hit: [0.0; 3],
            last_onset: -10.0,
            last_rest: -10.0,
            scroll: 0.0,
            deep_scroll: 0.0,
            bar_count: 0,
            prev_bar: 0.0,
            kinds: build_kinds(),
            solids,
            heart_key: start,
            heart_from,
            heart_mt: 1.0,
            mood_cur: Mood::default(),
            mood_old: Mood::default(),
            mood_k: 1.0,
            plain_ground: false,
            broken_t: 0.0,
            fx: 1.0,
            kick: 0.0,
            kick_luma: 1.0,
            last_kick: -10.0,
            ripples: [(-10.0, 0.0); 4],
            ripple_i: 0,
            snare: 0.0,
            rings: [(-10.0, 0.0); SNARE_RINGS],
            ring_i: 0,
            hat: 0.0,
            hat_n: 0,
            bass_b: 0.0,
            busy: 0.0,
            scope: [0.0; SCOPE_N],
            scope_ok: false,
            scope_peak: 0.2,
            next_big: f32::INFINITY,
            tension: 0.0,
            tension_pk: 0.0,
            drop: 0.0,
            prev_section: (u16::MAX, SectionKind::Normal),
        };
        r.bind_bloom();
        r
    }

    fn bind_bloom(&mut self) {
        self.comp_mat.set_texture("Bloom1", self.b1b.texture.clone());
        self.comp_mat.set_texture("Bloom2", self.b2b.texture.clone());
    }

    fn resize_if_needed(&mut self) {
        let dpi = screen_dpi_scale();
        let pw = (screen_width() * dpi).round().max(64.0) as u32;
        let ph = (screen_height() * dpi).round().max(64.0) as u32;
        if pw != self.pw || ph != self.ph {
            self.pw = pw;
            self.ph = ph;
            self.ss = auto_ss(self.ss_forced, pw, ph);
            self.scene = make_rt(pw * self.ss, ph * self.ss);
            self.b1a = make_rt(pw / 2, ph / 2);
            self.b1b = make_rt(pw / 2, ph / 2);
            self.b2a = make_rt(pw / 4, ph / 4);
            self.b2b = make_rt(pw / 4, ph / 4);
            self.bind_bloom();
        }
    }

    pub fn set_palette(&mut self, idx: u8) {
        if idx != self.pal_to {
            self.pal_from = self.pal;
            self.pal_to = idx;
            self.pal_t = 0.0;
        }
    }

    /// Compute the view for this frame (call before reading the mouse in world space).
    pub fn update_view(&mut self, world: &World) -> View {
        let (sw, sh) = (screen_width(), screen_height());
        let scale = (sw / VIEW_W).min(sh / VIEW_H);
        let dt = get_frame_time().clamp(0.0, 0.05);
        let target = world.player.pos * CAM_FOLLOW;
        self.cam_s += (target - self.cam_s) * (1.0 - (-dt * CAM_RATE).exp());
        let sh_amp = world.shake * world.shake * 16.0;
        let t = self.time * 47.0;
        let shake = vec2((t * 1.3).sin() + (t * 2.9).cos(), (t * 1.7).cos() + (t * 3.1).sin()) * sh_amp * 0.5;
        // music pulse: kick punch-in, bass breathing, build-up creep-in, drop snap-out
        let zoom = 1.0 + self.fx * (KICK_ZOOM * self.kick * self.glob() + BASS_ZOOM * self.bass_b + TENSION_ZOOM * self.tension - DROP_ZOOM * self.drop);
        let k = 1.0 - (-dt * 5.0).exp();
        self.shift_s += (self.show_shift - self.shift_s) * k;
        self.zoom_s += (self.show_zoom - self.zoom_s) * k;
        let scale = scale * zoom * self.zoom_s;
        let off = vec2(self.shift_s.x * sw, self.shift_s.y * sh) / scale;
        self.view = View { sw, sh, scale, center: self.cam_s + shake - off };
        self.view
    }

    /// Damping for full-frame effects when the screen is busy with bullets (1 = none).
    #[inline]
    fn glob(&self) -> f32 {
        1.0 - BUSY_DAMP * self.busy
    }

    /// This frame's audible waveform (interleaved stereo `samples` at `sr`, window starting at
    /// `t`) and the seconds until the next build-up target. `None` (live mode) hides the scope.
    /// Copies into a fixed buffer: no allocation.
    pub fn feed_signal(&mut self, track: Option<(&[f32], u32)>, t: f32, next_big: f32) {
        self.next_big = next_big;
        let Some((s, sr)) = track else {
            self.scope_ok = false;
            return;
        };
        let span = SCOPE_N * SCOPE_DECIM;
        let frames = s.len() / 2;
        let start = (t.max(0.0) * sr as f32) as usize;
        if start + span * 2 >= frames {
            self.scope_ok = false;
            return;
        }
        let mono = |i: usize| (s[2 * i] + s[2 * i + 1]) * 0.5;
        // trigger on the first rising zero crossing (of a lightly smoothed signal) so the trace
        // holds still like an oscilloscope instead of sliding every frame
        let mut st = start;
        let mut prev = mono(start);
        for i in start + 1..start + SCOPE_TRIG {
            let v = prev * 0.6 + mono(i) * 0.4;
            if prev <= 0.0 && v > 0.0 {
                st = i;
                break;
            }
            prev = v;
        }
        let mut peak = 0.0f32;
        for k in 0..SCOPE_N {
            let mut a = 0.0;
            for j in 0..SCOPE_DECIM {
                a += mono(st + k * SCOPE_DECIM + j);
            }
            let v = a / SCOPE_DECIM as f32;
            self.scope[k] = v;
            peak = peak.max(v.abs());
        }
        // auto-gain: fast up, quicker-than-the-music down, so the line stays alive after a big hit
        let dt = get_frame_time().clamp(0.0, 0.1);
        let rate = if peak > self.scope_peak { SCOPE_AGC_UP } else { SCOPE_AGC_DN };
        self.scope_peak += (peak - self.scope_peak) * (1.0 - (-dt * rate).exp());
        self.scope_ok = true;
    }

    /// 2.5D projection: world point `p` at height `z` -> position on the z = 0 drawing plane.
    /// Exact identity for z = 0.
    #[inline]
    fn pj(&self, p: Vec2, z: f32) -> Vec2 {
        let s = PERSP_F / (PERSP_F - z.min(PERSP_F * 0.8));
        self.cam + (p - self.cam) * s - vec2(0.0, z * TILT)
    }

    /// Screen fraction (0..1, 0..1) -> world position (drift vista layout).
    #[inline]
    fn vs(&self, fx: f32, fy: f32) -> Vec2 {
        self.view.center + vec2((fx - 0.5) * self.view.sw, (fy - 0.5) * self.view.sh) / self.view.scale
    }

    /// Heartline sample for perimeter index i (0 = oldest, P = newest).
    #[inline]
    fn ecg_at(&self, i: usize) -> f32 {
        self.ecg[(self.ecg_head + i) % ECG_N]
    }

    /// Spectrum layer `m` (0 = live) and its distance in history steps (continuous).
    fn layer(&self, m: usize) -> (&[f32; N_SPEC], f32) {
        if m == 0 {
            (&self.spec_s, 0.0)
        } else {
            let idx = (self.hist_head + HIST * 4 - (m - 1)) % HIST;
            (&self.hist[idx], (m - 1) as f32 + self.hist_acc / HIST_DT + 0.15)
        }
    }

    fn pass(&self, target: &RenderTarget, src: &Texture2D, dir: Vec2, thresh: f32) {
        set_camera(&Camera2D {
            target: vec2(0.5, 0.5),
            zoom: vec2(2.0, 2.0),
            render_target: Some(target.clone()),
            ..Default::default()
        });
        self.blur_mat.set_uniform("u_dir", dir);
        self.blur_mat.set_uniform("u_thresh", thresh);
        gl_use_material(&self.blur_mat);
        draw_texture_ex(src, 0.0, 0.0, WHITE, DrawTextureParams { dest_size: Some(vec2(1.0, 1.0)), ..Default::default() });
        gl_use_default_material();
    }

    /// Advance the renderer's own music state: smoothed spectrum, history, heartline, morphs.
    fn update_music(&mut self, w: &World, dt: f32, idle: f32) {
        let dt = dt.clamp(0.0, 0.1);
        self.clock += dt;
        let silent = w.feat.rms < 0.012;
        let up = 1.0 - (-dt * 26.0).exp();
        let dn = 1.0 - (-dt * 6.5).exp();
        for i in 0..N_SPEC {
            let f = i as f32 / (N_SPEC - 1) as f32;
            let mut v = (w.feat.spec[i] * (1.0 + 0.9 * f)).min(1.3);
            if silent {
                let amb = AMBIENT * (0.55 + 0.45 * (self.clock * 0.6 + i as f32 * 0.7).sin()) * (1.0 - 0.5 * f);
                v = v.max(amb);
            }
            let k = if v > self.spec_s[i] { up } else { dn };
            self.spec_s[i] += (v - self.spec_s[i]) * k;
        }
        self.low_s += (w.feat.low.min(1.2) - self.low_s) * (1.0 - (-dt * 8.0).exp());
        let sw_t = (w.bar_pulse * 0.7 + w.hit[0] * 0.3).min(1.0);
        let rate = if sw_t > self.swell { 10.0 } else { 2.2 };
        self.swell += (sw_t - self.swell) * (1.0 - (-dt * rate).exp());

        self.hist_acc += dt;
        while self.hist_acc >= HIST_DT {
            self.hist_acc -= HIST_DT;
            self.hist_head = (self.hist_head + 1) % HIST;
            self.hist[self.hist_head] = self.spec_s;
        }

        // ---- pulse layer: instant attack, exponential decay ----
        self.kick *= (-dt / KICK_TAU).exp();
        self.snare *= (-dt / SNARE_TAU).exp();
        self.hat *= (-dt / HAT_TAU).exp();
        self.drop *= (-dt / DROP_TAU).exp();
        let bl = w.feat.low.min(1.2);
        let br = if bl > self.bass_b { BASS_RISE } else { BASS_FALL };
        self.bass_b += (bl - self.bass_b) * (1.0 - (-dt * br).exp());
        self.busy = (w.bullets.len() as f32 / BUSY_BULLETS).min(1.0);
        for b in 0..3 {
            if w.hit[b] > self.prev_hit[b] + 0.2 {
                let a = w.hit[b];
                match b {
                    0 => {
                        self.kick = self.kick.max(a);
                        // photosensitivity: luminance bumps shrink when kicks come faster than ~3/s
                        self.kick_luma = ((self.clock - self.last_kick) / LUMA_GAP).clamp(0.25, 1.0);
                        self.last_kick = self.clock;
                        self.ripples[self.ripple_i] = (self.clock, a);
                        self.ripple_i = (self.ripple_i + 1) % self.ripples.len();
                    }
                    1 => {
                        self.snare = self.snare.max(a);
                        self.rings[self.ring_i] = (self.clock, a);
                        self.ring_i = (self.ring_i + 1) % SNARE_RINGS;
                    }
                    _ => {
                        self.hat = self.hat.max(a);
                        self.hat_n = self.hat_n.wrapping_add(1);
                    }
                }
            }
        }
        // anticipation: tension over the last bars before a step up into Intense / Drop
        let win = (BUILD_BARS * 4.0 * w.beat_len).clamp(BUILD_MIN, BUILD_MAX);
        let tt = (1.0 - self.next_big / win).clamp(0.0, 1.0);
        self.tension = tt * tt;
        self.tension_pk = if tt > 0.0 { self.tension_pk.max(self.tension) } else { self.tension_pk };
        let sec = (w.section_idx, w.section_kind);
        if sec.0 != self.prev_section.0 {
            let big = |k: SectionKind| matches!(k, SectionKind::Intense | SectionKind::Drop);
            if big(sec.1) && !big(self.prev_section.1) && self.prev_section.0 != u16::MAX {
                // the release: one big hit on the downbeat
                self.drop = 0.6 + 0.4 * self.tension_pk;
                self.kick = 1.0;
                self.kick_luma = 1.0;
            }
            self.tension_pk = 0.0;
            self.prev_section = sec;
        }

        // heartline: kicks are full beats, snares half beats; a resting pulse in silence
        for b in 0..3 {
            if w.hit[b] > self.prev_hit[b] + 0.2 {
                if b < 2 {
                    let amp = if b == 0 { 0.55 + 0.45 * w.hit[0] } else { 0.45 * w.hit[1] };
                    self.ecg_ev[self.ecg_evi] = (self.clock, amp);
                    self.ecg_evi = (self.ecg_evi + 1) % self.ecg_ev.len();
                }
                self.last_onset = self.clock;
            }
        }
        self.prev_hit = w.hit;
        if self.clock - self.last_onset > 1.6 && self.clock - self.last_rest > REST_BEAT {
            self.last_rest = self.clock;
            self.ecg_ev[self.ecg_evi] = (self.clock, 0.5);
            self.ecg_evi = (self.ecg_evi + 1) % self.ecg_ev.len();
        }
        self.ecg_acc += dt;
        let step = 1.0 / ECG_RATE;
        while self.ecg_acc >= step {
            self.ecg_acc -= step;
            let ts = self.clock - self.ecg_acc;
            let mut v = 0.0;
            for &(te, a) in &self.ecg_ev {
                v += ecg_shape(ts - te) * a;
            }
            v += w.hit[2] * 0.06 * (ts * 260.0).sin();
            self.ecg[self.ecg_head] = v;
            self.ecg_head = (self.ecg_head + 1) % ECG_N;
        }

        self.scroll += dt * VISTA_SCROLL * (1.0 + 1.2 * w.hit[0] + 4.0 * w.warp);
        self.deep_scroll += dt * (DEEP_SCROLL * (1.0 + TENSION_SCROLL * self.tension * self.fx) + WARP_SCROLL * w.warp * w.warp);
        if w.bar_pulse > 0.8 && self.prev_bar <= 0.8 {
            self.bar_count += 1;
        }
        self.prev_bar = w.bar_pulse;
        if w.heart.broken {
            self.broken_t += dt;
        } else {
            self.broken_t = 0.0;
        }

        // Heart solids: section picks the pair; drift settles into calm round forms
        let key = if idle > 0.5 {
            if (self.bar_count / 8) % 2 == 0 {
                (SH_ICOSA, SH_DODECA)
            } else {
                (SH_DODECA, SH_ICOSA)
            }
        } else {
            let alt = w.section_idx % 2 == 1;
            match w.section_kind {
                SectionKind::Calm => if alt { (SH_DODECA, SH_ICOSA) } else { (SH_ICOSA, SH_OCTA) },
                SectionKind::Normal => if alt { (SH_CUBOCT, SH_CUBE) } else { (SH_CUBE, SH_OCTA) },
                SectionKind::Intense => if alt { (SH_STELLA, SH_CUBE) } else { (SH_OCTA, SH_TETRA) },
                SectionKind::Drop => if alt { (SH_STAR, SH_STELLA) } else { (SH_STAR, SH_ICOSA) },
            }
        };
        if key != self.heart_key {
            let t = ease(self.heart_mt);
            let mut a = self.heart_from.0;
            let mut b = self.heart_from.1;
            for k in 0..ME {
                for j in 0..2 {
                    a[k][j] = a[k][j].lerp(self.solids[self.heart_key.0][k][j], t);
                    b[k][j] = b[k][j].lerp(self.solids[self.heart_key.1][k][j], t);
                }
            }
            self.heart_from = (a, b);
            self.heart_key = key;
            self.heart_mt = 0.0;
        }
        self.heart_mt = (self.heart_mt + dt * MORPH_RATE).min(1.0);
    }

    /// Draw the whole scene (background, world, drift visuals) and post-process it to the screen.
    pub fn draw_scene(&mut self, world: &World, dt: f32, idle: f32, progress: f32) {
        self.scene_pass(world, dt, idle, progress, None, None);
    }

    /// Rally mode: the same scene and music layer (`music` has no enemies or bullets), with the
    /// Rally avatars, orbs and end-line ridges in place of the combat entities, then the score HUD.
    /// `me` is the local player (0 = host / left).
    pub fn draw_rally(&mut self, r: &Rally, music: &World, me: usize, dt: f32) {
        self.scene_pass(music, dt, 0.0, 0.0, Some((r, me & 1)), None);
        self.draw_rally_hud(r, me & 1);
    }

    /// Highway mode: the menu vista with the road and everything on it drawn into the scene.
    pub fn draw_runner(&mut self, r: &crate::runner::Runner, music: &World, dt: f32, progress: f32) {
        self.scene_pass(music, dt, 1.0, progress, None, Some(r));
    }

    /// The Highway road, in the vista's own perspective: a point `x` across (in road units) and
    /// `z` ahead (0 = the player's line, 1 = a note that has just appeared) lands where the
    /// floor grid would put it.
    fn draw_runner_world(&self, r: &crate::runner::Runner, w: &World) {
        use crate::runner::{NoteKind, LEAD};
        const D0: f32 = 1.3;
        // A gentle perspective: notes are readable for their whole approach.
        const DZ: f32 = 6.5;
        const LANE: f32 = 0.22;
        let pal = &self.pal;
        let px = self.view.sh / self.view.scale / 1080.0;
        let proj = |x: f32, z: f32, lift: f32| -> (Vec2, f32) {
            let d = D0 + z.max(-0.05) * DZ;
            let s = D0 / d;
            (self.vs(0.5 + x / d, HZ + (1.05 - HZ) / d - lift * s), s)
        };
        let edge = 1.5 * LANE;
        // the road bed and its lane lines
        let (nl, _) = proj(-edge, 0.0, 0.0);
        let (nr, _) = proj(edge, 0.0, 0.0);
        let (fl, _) = proj(-edge, 1.0, 0.0);
        let (fr, _) = proj(edge, 1.0, 0.0);
        quad4([nl, nr, fr, fl], [alpha(pal.bg0, 0.82), alpha(pal.bg0, 0.82), alpha(pal.bg0, 0.45), alpha(pal.bg0, 0.45)]);
        gl_use_material(&self.add_mat);
        for k in 0..4 {
            let x = (k as f32 - 1.5) * LANE;
            let outer = k == 0 || k == 3;
            let (a, _) = proj(x, 0.0, 0.0);
            let (b, _) = proj(x, 1.0, 0.0);
            // in a surge or a drop the road's edges burn in the accent colour and pulse harder
            let hot = r.hot();
            let c = if outer { if hot { mixc(pal.accent, pal.danger, 0.5 + 0.5 * (self.time * 6.0).sin()) } else { pal.primary } } else { pal.grid };
            let wd = if outer { if hot { 5.0 + 3.0 * w.beat_pulse } else { 3.5 } } else { 2.0 };
            seg(a, b, wd * px, alpha(c, 0.9 + 0.1 * w.beat_pulse), alpha(c, if hot && outer { 0.4 } else { 0.12 }));
        }
        // beat lines rolling toward the player
        let i0 = r.beats.partition_point(|&b| b < r.t);
        for (k, &b) in r.beats.iter().enumerate().skip(i0).take_while(|(_, &b)| b < r.t + LEAD) {
            let z = (b - r.t) / LEAD;
            let down = k >= r.downbeat && (k - r.downbeat) % 4 == 0;
            let (a, s) = proj(-edge, z, 0.0);
            let (c, _) = proj(edge, z, 0.0);
            line(a, c, (if down { 2.5 } else { 1.2 }) * px * (0.4 + 0.6 * s), alpha(pal.grid, (if down { 0.55 } else { 0.22 }) * (0.3 + 0.7 * s)));
        }
        gl_use_default_material();
        // notes, far to near
        let lo = r.notes.partition_point(|n| n.t < r.t - 0.12);
        let hi = r.notes.partition_point(|n| n.t < r.t + LEAD);
        for n in r.notes[lo..hi].iter().rev() {
            if n.state == 1 {
                continue;
            }
            let z = (n.t - r.t) / LEAD;
            let fade = (1.0 - z).clamp(0.0, 1.0).powf(0.35) * if z < 0.0 { 0.5 } else { 1.0 };
            let x = n.lane as f32 * LANE;
            match n.kind {
                NoteKind::Wall => {
                    let col = if n.state == 2 { WHITE } else { pal.danger };
                    let (bl, s) = proj(x - 0.44 * LANE, z, 0.0);
                    let (br, _) = proj(x + 0.44 * LANE, z, 0.0);
                    let (tl, _) = proj(x - 0.44 * LANE, z, 0.17);
                    let (tr, _) = proj(x + 0.44 * LANE, z, 0.17);
                    quad4([bl, br, tr, tl], [alpha(pal.bg0, 0.85 * fade), alpha(pal.bg0, 0.85 * fade), alpha(col, 0.45 * fade), alpha(col, 0.45 * fade)]);
                    gl_use_material(&self.add_mat);
                    let wd = 3.0 * px * (0.4 + 0.6 * s);
                    for (p, q) in [(bl, br), (br, tr), (tr, tl), (tl, bl)] {
                        line(p, q, wd, alpha(col, fade));
                    }
                    // scan bars, like the sun's
                    for k in 1..4 {
                        let f = k as f32 / 4.0;
                        line(bl.lerp(tl, f), br.lerp(tr, f), wd * 0.6, alpha(col, 0.5 * fade));
                    }
                    gl_use_default_material();
                }
                NoteKind::Bar => {
                    let col = if n.state == 2 { WHITE } else { pal.accent };
                    let (gl, s) = proj(-edge, z, 0.0);
                    let (gr, _) = proj(edge, z, 0.0);
                    let (a, _) = proj(-edge, z, 0.04);
                    let (b, _) = proj(edge, z, 0.04);
                    line(gl, gr, 5.0 * px * s, alpha(pal.bg0, 0.7 * fade));
                    gl_use_material(&self.add_mat);
                    line(a, b, 7.0 * px * (0.3 + 0.7 * s), alpha(col, fade));
                    line(a, b, 2.5 * px * (0.3 + 0.7 * s), alpha(WHITE, fade));
                    line(gl, a, 3.0 * px * s, alpha(col, 0.8 * fade));
                    line(gr, b, 3.0 * px * s, alpha(col, 0.8 * fade));
                    gl_use_default_material();
                }
                NoteKind::Gem | NoteKind::Star => {
                    if n.state == 2 {
                        continue;
                    }
                    let star = n.kind == NoteKind::Star;
                    let (c, s) = proj(x, z, 0.035 + 0.008 * (self.time * 5.0 + n.t * 9.0).sin());
                    let rr = (if star { 20.0 } else { 12.0 }) * px * (0.25 + 0.75 * s);
                    gl_use_material(&self.add_mat);
                    disc2(c, rr * 2.2, 12, alpha(if star { WHITE } else { pal.accent }, 0.3 * fade), alpha(pal.accent, 0.0));
                    diamond(c, rr, 2.5 * px * s.max(0.4), alpha(if star { WHITE } else { pal.accent }, fade));
                    if star {
                        line(c - vec2(rr * 1.8, 0.0), c + vec2(rr * 1.8, 0.0), 1.5 * px, alpha(WHITE, fade));
                        line(c - vec2(0.0, rr * 1.8), c + vec2(0.0, rr * 1.8), 1.5 * px, alpha(WHITE, fade));
                    }
                    gl_use_default_material();
                    diamond_fill(c, rr * 0.45, alpha(WHITE, fade));
                }
            }
        }
        // the craft
        let hgt = r.height();
        let cx = r.lane_x * LANE;
        let (ground, _) = proj(cx, 0.0, 0.0);
        let bob = 0.006 * w.beat_pulse;
        let (c, _) = proj(cx, 0.0, 0.03 + bob + 0.15 * hgt);
        let blink = if r.invuln > 0.0 { 0.4 + 0.6 * ((self.time * 30.0).sin() * 0.5 + 0.5) } else { 1.0 };
        disc2(ground, 52.0 * px * (1.0 - 0.4 * hgt), 16, alpha(BLACK, 0.6), alpha(BLACK, 0.0));
        let nose = c + vec2(0.0, -52.0) * px;
        let l = c + vec2(-42.0, 24.0) * px;
        let rr = c + vec2(42.0, 24.0) * px;
        let tail = c + vec2(0.0, 9.0) * px;
        quad4([nose, rr, tail, l], [alpha(pal.bg0, 0.9 * blink); 4]);
        gl_use_material(&self.add_mat);
        let col = mixc(pal.primary, WHITE, r.glow);
        for (p, q) in [(nose, rr), (rr, tail), (tail, l), (l, nose)] {
            line(p, q, 3.0 * px, alpha(col, blink));
        }
        // engine: a flame that kicks with the beat and stretches in a jump
        let flame = (18.0 + 26.0 * w.beat_pulse + 30.0 * hgt) * px;
        seg(tail, tail + vec2(0.0, flame), 9.0 * px, alpha(pal.accent, 0.9 * blink), alpha(pal.secondary, 0.0));
        if r.glow > 0.02 {
            arc(c, (40.0 + 50.0 * (1.0 - r.glow)) * px, 0.0, TAU, 3.0 * px, alpha(pal.accent, r.glow));
        }
        if r.flash > 0.02 {
            arc(c, (30.0 + 90.0 * (1.0 - r.flash)) * px, 0.0, TAU, 4.0 * px, alpha(pal.danger, r.flash));
        }
        gl_use_default_material();
    }

    #[allow(clippy::too_many_arguments)]
    fn scene_pass(&mut self, world: &World, dt: f32, idle: f32, progress: f32, rally: Option<(&Rally, usize)>, runner: Option<&crate::runner::Runner>) {
        self.resize_if_needed();
        self.time += dt;
        if self.pal_t < 1.0 {
            self.pal_t = (self.pal_t + dt * 1.4).min(1.0);
            self.pal = lerp_pal(&self.pal_from, &palette(self.pal_to), ease(self.pal_t));
        }
        self.update_music(world, dt, idle);
        // A new look arrives as a transition: the old sky sinks, the new one rises.
        if world.mood != self.mood_cur {
            self.mood_old = self.mood_cur;
            self.mood_cur = world.mood;
            self.mood_k = if self.time < 0.5 { 1.0 } else { 0.0 };
        }
        self.mood_k = (self.mood_k + dt / SKY_TRANSITION).min(1.0);
        self.plain_ground = rally.is_some() || runner.is_some();
        let pal = self.pal;
        let view = self.view;
        self.cam = view.center;
        let (rw, rh) = ((self.pw * self.ss) as f32, (self.ph * self.ss) as f32);
        let px_scale = view.scale * rw / view.sw;
        let e = ease(idle);
        let lv = 1.0 - e;

        // ---- background ----
        set_camera(&Camera2D {
            target: vec2(0.5, 0.5),
            zoom: vec2(2.0, 2.0),
            render_target: Some(self.scene.clone()),
            ..Default::default()
        });
        clear_background(BLACK);
        let m = &self.bg_mat;
        m.set_uniform("u_res", vec2(rw, rh));
        m.set_uniform("u_time", self.time);
        m.set_uniform("u_idle", e);
        m.set_uniform("u_scale", px_scale);
        m.set_uniform("u_swell", self.swell);
        m.set_uniform("u_bass", self.low_s);
        m.set_uniform("u_scroll", self.deep_scroll);
        m.set_uniform("u_lv", lv);
        m.set_uniform("u_warp", world.warp);
        let (fx, g) = (self.fx, self.glob());
        m.set_uniform("u_kick", KICK_BG * self.kick * self.kick_luma * fx * g);
        let (rt0, ra) = self.ripples[(self.ripple_i + self.ripples.len() - 1) % self.ripples.len()];
        let rage = self.clock - rt0;
        let rfade = (1.0 - rage / RIPPLE_LIFE).max(0.0);
        m.set_uniform("u_ripple", vec3(RIPPLE_R0 + rage * RIPPLE_SPEED,RIPPLE_BG * 0.03 * ra * rfade * fx, RIPPLE_W * 1.6));
        m.set_uniform("u_pump", vec3(TENSION_DESAT * self.tension * fx, DROP_SAT * self.drop * fx, TENSION_GRID * self.tension * fx));
        m.set_uniform("u_cam", view.center);
        m.set_uniform("u_arena", vec2(HALF_W, HALF_H));
        m.set_uniform("u_vista", vec3(HZ, SUN_Y, SUN_R));
        m.set_uniform("u_c0", v3(pal.bg0));
        m.set_uniform("u_c1", v3(pal.bg1));
        m.set_uniform("u_grid", v3(pal.grid));
        m.set_uniform("u_sunA", v3(pal.accent));
        m.set_uniform("u_sunB", v3(pal.secondary));
        // In play the weather and floor also follow the section: calm parts drift with dust,
        // drops bring the rain if the song has no weather of its own.
        let weather = if lv > 0.5 {
            match world.section_kind {
                // snow and mist are already calm; anything else settles to drifting motes
                SectionKind::Calm if !matches!(self.mood_cur.weather, 4 | 6 | 8 | 9) => 2,
                SectionKind::Drop if self.mood_cur.weather == 0 => 1,
                _ => self.mood_cur.weather,
            }
        } else {
            self.mood_cur.weather
        };
        m.set_uniform("u_mood", vec3(self.mood_cur.scene as f32, weather as f32, self.mood_cur.floor as f32));
        m.set_uniform("u_mood2", vec3(self.mood_cur.prop as f32, world.hit[2] * self.fx.min(1.0), world.hit[0] * self.fx.min(1.0)));
        m.set_uniform("u_tr", vec3(ease(self.mood_k), self.mood_old.scene as f32, self.mood_old.prop as f32));
        m.set_uniform("u_fx", vec3(world.chime * self.fx.min(1.0), world.chime_pitch, world.candy * self.fx.min(1.0)));
        gl_use_material(m);
        draw_rectangle(0.0, 0.0, 1.0, 1.0, WHITE);
        gl_use_default_material();

        // ---- world ----
        set_camera(&Camera2D {
            target: view.center,
            zoom: vec2(2.0 * view.scale / view.sw, 2.0 * view.scale / view.sh),
            render_target: Some(self.scene.clone()),
            ..Default::default()
        });
        let tscale = px_scale / screen_dpi_scale();
        let armed_k = world
            .tiles
            .iter()
            .filter(|t| t.armed)
            .map(|t| (1.0 - (t.strike_t - world.t) / t.lead.max(0.05)).clamp(0.0, 1.0))
            .fold(0.0f32, f32::max);

        if std::env::var("HL_DBG").is_ok() && (world.t * 4.0).fract() < 0.03 {
            let n = world.tiles.iter().filter(|t| t.armed).count();
            eprintln!("DBG t={:.2} armed={} armk={:.2} lasers={} waves={} pulses={} bullets={} enemies={}", world.t, n, armed_k, world.lasers.len(), world.waves.len(), world.pulses.len(), world.bullets.len(), world.enemies.len());
        }
        self.draw_ridges(world, e);
        gl_use_material(&self.add_mat);
        self.draw_floor(world, e, armed_k);
        self.draw_rim(world, e);
        // The Heart's layers are drawn around the origin and shifted to where it stands.
        let hp = world.heart.pos * lv;
        if lv > 0.003 {
            set_origin(hp);
            self.draw_crown(world, lv);
            self.draw_scope(world, lv);
            set_origin(Vec2::ZERO);
        }
        gl_use_default_material();
        if lv > 0.003 && rally.is_none() {
            self.draw_tiles(world, lv, armed_k);
            self.draw_hazards(world, lv);
        }
        gl_use_material(&self.add_mat);
        if !(self.menu && e > 0.5) {
            set_origin(hp);
            self.draw_heart(world, e, progress);
            set_origin(Vec2::ZERO);
        }
        gl_use_default_material();
        if let Some((r, me)) = rally {
            self.draw_rally_world(r, world, me, tscale);
        } else if lv > 0.003 {
            self.draw_heart_move(world, lv);
            set_origin(hp);
            self.draw_heart_rings(world, lv, progress);
            for p in &world.pulses {
                self.draw_pulse(world, p, lv);
            }
            set_origin(Vec2::ZERO);
        }
        if rally.is_none() {
            self.draw_pickups(world, tscale);
            if lv > 0.003 {
                self.draw_enemies(world, lv);
                self.draw_bullets(world, lv);
            }
            self.draw_shots(world);
            if !self.menu {
                self.draw_player(world);
            }
        }
        gl_use_material(&self.add_mat);
        self.draw_particles(world);
        gl_use_default_material();
        self.draw_texts(world, tscale);
        if let Some(rn) = runner {
            self.draw_runner_world(rn, world);
        }

        // ---- bloom ----
        let (hw, hh) = ((self.pw / 2) as f32, (self.ph / 2) as f32);
        let (qw, qh) = ((self.pw / 4) as f32, (self.ph / 4) as f32);
        self.pass(&self.b1a, &self.scene.texture, vec2(1.0 / hw, 0.0), 0.55);
        self.pass(&self.b1b, &self.b1a.texture, vec2(0.0, 1.0 / hh), 0.0);
        self.pass(&self.b2a, &self.b1b.texture, vec2(1.6 / qw, 0.0), 0.0);
        self.pass(&self.b2b, &self.b2a.texture, vec2(0.0, 1.6 / qh), 0.0);

        // ---- composite to the screen ----
        set_default_camera();
        clear_background(BLACK);
        let c = &self.comp_mat;
        c.set_uniform("u_res", vec2(self.pw as f32, self.ph as f32));
        c.set_uniform("u_time", self.time);
        let pulse_ab = fx * g * (SNARE_ABERR * self.snare + DROP_ABERR * self.drop);
        c.set_uniform("u_aberr", (world.beat_pulse * 0.12 + world.shake * 0.9 + world.flash * 0.6 + pulse_ab).min(1.5) * (1.0 - idle * 0.7));
        c.set_uniform("u_flash", world.flash * 0.5 + world.warp * world.warp * 0.12 + DROP_FLASH * self.drop * self.drop * fx * g);
        c.set_uniform("u_flashcol", v3(mixc(pal.secondary, WHITE, 0.5)));
        let pulse_bl = fx * g * (KICK_BLOOM * self.kick + DROP_BLOOM * self.drop);
        c.set_uniform("u_bloom", (1.0 + 0.15 * self.swell + pulse_bl) * (1.0 - 0.35 * e));
        c.set_uniform("u_warp", world.warp);
        c.set_uniform("u_expo", 1.0 + KICK_EXPO * self.kick * self.kick_luma * fx * g);
        c.set_uniform("u_vig", TENSION_VIG * self.tension * fx);
        gl_use_material(c);
        draw_texture_ex(
            &self.scene.texture,
            0.0,
            0.0,
            WHITE,
            DrawTextureParams { dest_size: Some(vec2(view.sw, view.sh)), ..Default::default() },
        );
        gl_use_default_material();
    }

    // -----------------------------------------------------------------------------------------
    // Environment: floor grid, edge ridgelines / mountains, heartline rim, crown
    // -----------------------------------------------------------------------------------------

    /// Arena floor grid. In drift (`e` -> 1) the same lines unfold into the vista's perspective floor.
    fn draw_floor(&self, w: &World, e: f32, armed_k: f32) {
        let pal = &self.pal;
        let lv = 1.0 - e;
        // In purgatory the floor all but falls away.
        let ga = (FLOOR_A + FLOOR_ARMED_A * armed_k + FLOOR_SWELL_A * self.swell + KICK_GRID * self.kick * self.fx) * lv * (1.0 - 0.75 * w.purg);
        // the sea has no grid on it, and the dunes only a trace
        let (ground, rise) = self.ground_now();
        let grid_k = match ground {
            1 | 4 => 1.0 - rise,
            3 => 1.0 - 0.65 * rise,
            _ => 1.0,
        };
        let va = VISTA_GRID_A * (1.0 + 0.5 * w.hit[0]) * e * grid_k;
        let gw = 1.6 + 1.2 * e;
        let vp = self.vs(0.5, HZ);
        // Floor style (in play only; the vista always shows the full grid):
        // 1 = open grid, inner lines alternate strong and faint; 2 = faint lines, marks on the crossings.
        let style = if e < 0.5 { (w.mood.floor + w.palette) % 3 } else { 0 };
        let line_k = |i: usize, n: usize| -> f32 {
            match style {
                1 if i % 2 == 1 && i != n => 0.3,
                2 if i != 0 && i != n => 0.35,
                _ => 1.0,
            }
        };
        for i in 0..=COLS {
            let x = -HALF_W + i as f32 * TILE;
            let v1 = self.vs(0.5 + (x / HALF_W) * VISTA_FAN, 1.05);
            let v0 = vp + (v1 - vp) * 0.03;
            let p0 = vec2(x, -HALF_H).lerp(v0, e);
            let p1 = vec2(x, HALF_H).lerp(v1, e);
            let k = line_k(i, COLS);
            seg(p0, p1, gw, alpha(pal.grid, ga * k + va * 0.05), alpha(pal.grid, ga * k + va));
        }
        if style == 2 && lv > 0.5 {
            let ma = (ga * 2.6).min(0.5);
            for i in 1..COLS {
                for j in 1..ROWS {
                    let c = vec2(-HALF_W + i as f32 * TILE, -HALF_H + j as f32 * TILE);
                    line(c - vec2(9.0, 0.0), c + vec2(9.0, 0.0), 1.6, alpha(pal.grid, ma));
                    line(c - vec2(0.0, 9.0), c + vec2(0.0, 9.0), 1.6, alpha(pal.grid, ma));
                }
            }
        }
        let phase = self.scroll.fract();
        for j in 0..=ROWS {
            let y = -HALF_H + j as f32 * TILE;
            let d = (ROWS - j) as f32 + 1.35 - phase;
            let fy = HZ + (1.05 - HZ) / d;
            let hs = VISTA_FAN * (fy - HZ) / (1.05 - HZ);
            let fade = smooth(0.0, 0.12, fy - HZ);
            let p0 = vec2(-HALF_W, y).lerp(self.vs(0.5 - hs, fy), e);
            let p1 = vec2(HALF_W, y).lerp(self.vs(0.5 + hs, fy), e);
            line(p0, p1, gw, alpha(pal.grid, ga * line_k(j, ROWS) + va * fade));
        }
        if lv > 0.01 && self.fx > 0.0 {
            self.draw_floor_pulse(lv);
        }
        // platform underside: a dim second rim below the floor + corner struts (breathes with bass)
        if lv > 0.01 {
            let a = 0.16 * lv;
            let pd = PLAT_D * (1.0 + BASS_PLAT * self.bass_b * self.fx);
            let c = [vec2(-HALF_W, -HALF_H), vec2(HALF_W, -HALF_H), vec2(HALF_W, HALF_H), vec2(-HALF_W, HALF_H)];
            for k in 0..4 {
                let (p, q) = (c[k], c[(k + 1) % 4]);
                line(self.pj(p, -pd), self.pj(q, -pd), 2.0, alpha(pal.grid, a));
                line(self.pj(p, -pd * 2.2), self.pj(q, -pd * 2.2), 1.5, alpha(pal.grid, a * 0.4));
                seg(self.pj(p, 0.0), self.pj(p, -pd * 2.2), 2.0, alpha(pal.primary, a * 1.6), alpha(pal.grid, 0.0));
            }
        }
    }

    /// Floor-level music pulse (additive, z = 0, under every threat): kick ripples running out
    /// along the grid lines from the Heart, hat sparkles on intersections, thin snare rings.
    fn draw_floor_pulse(&self, lv: f32) {
        let pal = &self.pal;
        let fx = self.fx;
        const NX: usize = COLS + 1;
        const NY: usize = ROWS + 1;
        // per-vertex ripple intensity (stack array, no allocation)
        let mut rip = [0.0f32; NX * NY];
        let mut any = false;
        for &(t0, amp) in &self.ripples {
            let age = self.clock - t0;
            if age < 0.0 || age > RIPPLE_LIFE {
                continue;
            }
            any = true;
            let r = RIPPLE_R0 + age * RIPPLE_SPEED;
            let a = amp * (1.0 - age / RIPPLE_LIFE) * RIPPLE_A * fx * self.glob() * lv;
            for j in 0..NY {
                for i in 0..NX {
                    let p = vec2(-HALF_W + i as f32 * TILE, -HALF_H + j as f32 * TILE);
                    let d = (p.length() - r) / RIPPLE_W;
                    rip[j * NX + i] += a * (-d * d).exp();
                }
            }
        }
        if any {
            let col = mixc(pal.grid, WHITE, 0.3);
            let vp = |i: usize, j: usize| vec2(-HALF_W + i as f32 * TILE, -HALF_H + j as f32 * TILE);
            for j in 0..NY {
                for i in 0..NX {
                    let a0 = rip[j * NX + i].min(RIPPLE_MAX);
                    if i + 1 < NX {
                        let a1 = rip[j * NX + i + 1].min(RIPPLE_MAX);
                        if a0 + a1 > 0.01 {
                            seg(vp(i, j), vp(i + 1, j), 2.2, alpha(col, a0), alpha(col, a1));
                        }
                    }
                    if j + 1 < NY {
                        let a1 = rip[(j + 1) * NX + i].min(RIPPLE_MAX);
                        if a0 + a1 > 0.01 {
                            seg(vp(i, j), vp(i, j + 1), 2.2, alpha(col, a0), alpha(col, a1));
                        }
                    }
                }
            }
        }
        // hats: a fresh random subset of intersections glints on each hat
        let ha = self.hat * HAT_SPARK_A * fx * lv;
        if ha > 0.01 {
            let col = mixc(pal.primary, WHITE, 0.5);
            let seed = self.hat_n as f32 * 7.31;
            for j in 0..NY {
                for i in 0..NX {
                    let h = ((i as f32 * 12.9898 + j as f32 * 78.233 + seed).sin() * 43758.547).fract().abs();
                    if h < HAT_SPARK_FRAC {
                        let p = vec2(-HALF_W + i as f32 * TILE, -HALF_H + j as f32 * TILE);
                        let l = HAT_SPARK_L * (0.6 + h / HAT_SPARK_FRAC * 0.6);
                        line(p - vec2(l, 0.0), p + vec2(l, 0.0), 1.6, alpha(col, ha));
                        line(p - vec2(0.0, l), p + vec2(0.0, l), 1.6, alpha(col, ha));
                    }
                }
            }
        }
        // snares: thin cool rings expanding off the Heart
        let col = mixc(pal.grid, WHITE, 0.4);
        for &(t0, amp) in &self.rings {
            let age = self.clock - t0;
            if age < 0.0 || age > SNARE_LIFE {
                continue;
            }
            let k = age / SNARE_LIFE;
            let r = SNARE_R0 + age * SNARE_SPEED * (1.0 - 0.35 * k);
            let a = amp * (1.0 - k) * (1.0 - k) * SNARE_A * fx * lv;
            arc(Vec2::ZERO, r, 0.0, TAU, SNARE_W, alpha(col, a));
        }
    }

    /// Heartline around the arena rim (spikes on kicks/snares, travels clockwise). In drift it
    /// straightens out into the ECG trace across the vista.
    fn draw_rim(&self, w: &World, e: f32) {
        // Depth: one more frame of danger-coloured light around the arena for every song deep
        // the run is (up to four), breathing with the bar.
        let depth = (w.song_idx as usize).min(4);
        if depth > 0 && e < 0.99 {
            let lv = 1.0 - e;
            for d in 0..depth {
                let o = 22.0 + 15.0 * d as f32;
                let a = (0.30 - 0.05 * d as f32) * lv * (0.7 + 0.3 * w.bar_pulse);
                let c = [vec2(-HALF_W - o, -HALF_H - o), vec2(HALF_W + o, -HALF_H - o), vec2(HALF_W + o, HALF_H + o), vec2(-HALF_W - o, HALF_H + o)];
                for k in 0..4 {
                    line(c[k], c[(k + 1) % 4], 2.0, alpha(self.pal.danger, a));
                }
            }
        }
        let pal = &self.pal;
        let lv = 1.0 - e;
        let bright = 0.65 + (0.35 * w.bar_pulse + SNARE_RIM * self.snare * self.fx) * lv;
        let mut prev = Vec2::ZERO;
        for i in 0..=P {
            let v = self.ecg_at(i);
            let (pos, n) = perim(i);
            let gp = pos + n * (RIM_OFF + v * RIM_ECG);
            let vp = self.vs(0.02 + 0.96 * i as f32 / P as f32, HL_Y - v * HL_AMP);
            let p = gp.lerp(vp, e);
            if i > 0 {
                let f = i as f32 / P as f32;
                let head = smooth(0.85, 1.0, f);
                let col = mixc(pal.primary, WHITE, head * 0.6 + v.max(0.0) * 0.5);
                let vista_a = if self.menu { 0.0 } else { 1.0 };
                let a = (bright * lv + vista_a * e * (0.25 + 0.75 * smooth(0.0, 0.25, f))) * (0.75 + 0.25 * head);
                line(prev, p, 2.6 + 1.2 * e, alpha(col, a));
                if e > 0.02 {
                    line(prev, p, 9.0, alpha(pal.primary, 0.12 * a * e));
                }
            }
            prev = p;
        }
        // corner brackets on the rim
        if lv > 0.01 {
            let cl = 46.0;
            for (sx, sy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let corner = vec2(sx * (HALF_W + 22.0), sy * (HALF_H + 22.0));
                line(corner, corner - vec2(sx * cl, 0.0), 3.0, alpha(pal.secondary, 0.8 * lv));
                line(corner, corner - vec2(0.0, sy * cl), 3.0, alpha(pal.secondary, 0.8 * lv));
            }
        }
    }

    /// The vista's foreground right now and how far it has risen (0..1). When the look changes
    /// the old ground sinks away before the new one comes up.
    fn ground_now(&self) -> (u8, f32) {
        if self.plain_ground {
            return (0, 1.0);
        }
        let (old, new) = (self.mood_old.ground, self.mood_cur.ground);
        if old == new || self.mood_k >= 1.0 {
            return (new, 1.0);
        }
        if self.mood_k < 0.5 {
            (old, ease(1.0 - self.mood_k * 2.0))
        } else {
            (new, ease(self.mood_k * 2.0 - 1.0))
        }
    }

    /// The vista's foreground. Around the arena it is always the spectrum ridgelines; as the
    /// view drifts off they become whichever ground the song has.
    fn draw_ridges(&self, w: &World, e: f32) {
        let (ground, rise) = self.ground_now();
        if ground == 0 {
            self.draw_ridgelines(w, e, 1.0, rise);
            return;
        }
        self.draw_ridgelines(w, e, 0.0, 1.0);
        if e > 0.02 {
            match ground {
                1 => self.draw_rows(w, e, rise, false),
                2 => self.draw_mesas(e, rise),
                3 => self.draw_dunes(e, rise),
                4 => self.draw_rows(w, e, rise, true),
                _ => self.draw_hills(e, rise),
            }
        }
    }

    /// The spectrum as it was `age` history steps ago. `age` may be fractional: the two nearest
    /// snapshots are blended, so anything shaped by it moves smoothly at any frame rate instead
    /// of stepping each time a snapshot is taken.
    fn spec_ago(&self, age: f32) -> [f32; N_SPEC] {
        let off = self.hist_acc / HIST_DT + 0.15; // age of the newest snapshot
        let (a, b, k) = if age <= off {
            (0, 1, (age / off).clamp(0.0, 1.0))
        } else {
            let x = age - off;
            let m = (x.floor() as usize).min(HIST - 2);
            (m + 1, m + 2, (x - m as f32).clamp(0.0, 1.0))
        };
        let (sa, _) = self.layer(a);
        let (sb, _) = self.layer(b);
        let mut out = [0.0; N_SPEC];
        for i in 0..N_SPEC {
            out[i] = sa[i] + (sb[i] - sa[i]) * k;
        }
        out
    }

    /// The sea of sound (or, with `clouds`, a deck of cloud seen from above): rows rolling in
    /// from the horizon. The swell is the sea's own; the spectrum rides on it (the newest sound
    /// is born at the horizon), the bass lifts the whole surface, the kick lights the crests and
    /// the body in the sky lays a path of light. Everything is a function of continuous time.
    fn draw_rows(&self, w: &World, e: f32, rise: f32, clouds: bool) {
        const SEG: usize = 120;
        let pal = &self.pal;
        let t = self.time;
        // rows, how much farther each is than the one in front, rows passing per second
        let (nr, step, speed) = if clouds { (26usize, 0.19f32, 0.32f32) } else { (44, 0.115, 1.5) };
        let flow = t * speed;
        let (scroll, roll) = (flow.fract(), flow.floor());
        let deep = if clouds { mixc(mixc(pal.bg0, pal.bg1, 0.6), BLACK, 0.25) } else { mixc(pal.bg0, BLACK, 0.5) };
        let kick = w.hit[0] * self.fx.min(1.0);
        let lift = 0.45 + 0.9 * self.low_s;
        let px_h = screen_height();
        // the surface itself: one sheet from the horizon to the viewer, glowing where it is far
        let hz_col = alpha(mixc(deep, pal.secondary, if clouds { 0.55 } else { 0.34 }), 0.97 * e);
        let near_col = alpha(deep, 0.97 * e);
        quad4([self.vs(-0.05, HZ), self.vs(1.05, HZ), self.vs(1.05, 0.80), self.vs(-0.05, 0.80)], [hz_col, hz_col, near_col, near_col]);
        quad4([self.vs(-0.05, 0.80), self.vs(1.05, 0.80), self.vs(1.05, 1.08), self.vs(-0.05, 1.08)], [near_col; 4]);
        for k in (0..nr).rev() {
            let kf = k as f32 - scroll;
            let d = (kf * step).exp();
            let far = (kf / nr as f32).clamp(0.0, 1.0);
            let near = 1.0 - far;
            let fy0 = HZ + (1.10 - HZ) / d;
            // Rows closer together than a few pixels are left to the sheet: lines packed that
            // tightly can only shimmer. The same factor fades a row in as it arrives.
            let gap = (1.10 - HZ) * (1.0 - (-step).exp()) / d;
            let lk = smooth(2.5, 9.0, gap * px_h) * smooth(0.0, 0.1, near);
            if lk <= 0.001 {
                continue;
            }
            let spec = self.spec_ago(near * 11.0);
            let amp = if clouds { 0.105 } else { 0.075 } / d * rise * lk;
            let id = (k as f32 + roll) * if clouds { 1.7 } else { 0.45 };
            let lc = if clouds { mixc(pal.secondary, WHITE, 0.2 + 0.4 * near) } else { mixc(pal.secondary, pal.grid, near) };
            let base = mixc(deep, pal.secondary, if clouds { 0.5 } else { 0.30 } * far * far);
            let wline = (1.0 + 2.4 / d).min(3.4);
            let pathw = 0.035 + 0.30 / d;
            let xs = d.min(6.0) + 1.0;
            let foot = fy0 + gap * 3.2;
            let bottom = alpha(base, 0.0);
            let mut prev: Option<(Vec2, Vec2, Color, f32)> = None;
            for i in 0..=SEG {
                let fx = -0.04 + 1.08 * i as f32 / SEG as f32;
                let xw = (fx - 0.5) * xs * 2.4;
                // the spectrum laid across the surface, bass under the body in the sky
                let tri = 1.0 - ((xw * 0.07 + 0.5).rem_euclid(1.0) * 2.0 - 1.0).abs();
                let v = soft_band(&spec, 1.0 - tri);
                let h = if clouds {
                    // billows: scalloped tops in two sizes, drifting past each other
                    let b1 = (xw * 1.1 + id * 2.3 + t * 0.10).sin().abs().powf(0.6);
                    let b2 = (xw * 2.3 - id * 1.7 - t * 0.07).sin().abs().powf(0.6);
                    (0.62 * b1 + 0.38 * b2) * (0.75 + 0.5 * self.low_s) + v * 0.55
                } else {
                    // two swells crossing and a rank of crests, rounded so nothing has a corner
                    let s1 = (xw * 0.9 + id * 0.6 - t * 1.1).sin();
                    let s2 = (xw * 1.9 - id * 0.95 - t * 1.6 + 1.3).sin();
                    let pk = (0.5 + 0.5 * (xw + id * 0.8 - t * 1.4).cos()).powf(2.5);
                    (0.42 * s1 + 0.18 * s2 + 0.75 * pk - 0.25) * lift + v * 1.5 * (0.7 + 0.3 * s2)
                };
                let p = self.vs(fx, fy0 - h * amp);
                let b = self.vs(fx, foot);
                let crest = smooth(0.25, 1.25, h) * lk;
                let lit = if clouds { 0.30 + 0.45 * crest } else { 0.10 + 0.16 * near + 0.30 * crest };
                let top = alpha(mixc(base, lc, lit), 0.97 * e * (0.35 + 0.65 * lk));
                if let Some((pp, pb, ptop, pc)) = prev {
                    quad4([pp, p, b, pb], [ptop, top, bottom, bottom]);
                    let c = 0.5 * (crest + pc);
                    let col = mixc(lc, WHITE, 0.55 * c + 0.45 * kick * c);
                    let la = if clouds { 0.30 + 0.5 * c } else { 0.16 + 0.84 * c };
                    line(pp, p, wline, alpha(col, la * (0.3 + 0.7 * near) * lk * e));
                    if !clouds {
                        // the path of light: glints that swell and fade, never flicker
                        let g = (-((fx - 0.5) / pathw).powi(2)).exp();
                        if g > 0.06 {
                            let hs = ((i as f32 * 12.9898 + id * 78.233).sin() * 43758.547).fract().abs();
                            let tw = (0.5 + 0.5 * (t * (1.5 + 2.0 * hs) + hs * TAU).sin()).powi(3);
                            line(pp, p, wline * 1.2, alpha(mixc(pal.accent, WHITE, 0.4), g * tw * (0.5 + 0.5 * kick) * (0.35 + 0.65 * near) * lk * e));
                        }
                    }
                }
                prev = Some((p, b, top, crest));
            }
        }
        // the horizon: a line of light where the surface meets the sky
        let hl = mixc(pal.secondary, WHITE, 0.4);
        seg(self.vs(0.0, HZ + 0.001), self.vs(0.5, HZ + 0.001), 2.0, alpha(pal.secondary, 0.1 * e), alpha(hl, 0.75 * e));
        seg(self.vs(0.5, HZ + 0.001), self.vs(1.0, HZ + 0.001), 2.0, alpha(hl, 0.75 * e), alpha(pal.secondary, 0.1 * e));
    }

    /// One layer of a silhouette painted the way the sun is: a body that is lit toward its top,
    /// scan bars drifting up through it, and a rim of light along its edge. `hs` are its heights
    /// (screen fractions) at evenly spaced points across the screen, standing on `base`.
    #[allow(clippy::too_many_arguments)]
    fn striped_layer(&self, hs: &[f32], base: f32, e: f32, body: Color, lit: Color, rim: Color, bar_gap: f32, dim: f32) {
        let n = hs.len() - 1;
        let foot = base + 0.05;
        let fxa = |i: usize| -0.02 + 1.04 * i as f32 / n as f32;
        let hmax = hs.iter().cloned().fold(0.0f32, f32::max);
        let low = alpha(body, 0.0);
        for i in 0..n {
            let (x0, x1) = (fxa(i), fxa(i + 1));
            let (h0, h1) = (hs[i], hs[i + 1]);
            let k0 = (h0 / hmax.max(1e-4)).clamp(0.0, 1.0);
            let k1 = (h1 / hmax.max(1e-4)).clamp(0.0, 1.0);
            let c0 = alpha(mixc(body, lit, 0.10 + 0.42 * k0 * dim), 0.97 * e);
            let c1 = alpha(mixc(body, lit, 0.10 + 0.42 * k1 * dim), 0.97 * e);
            let mid = alpha(body, 0.97 * e);
            quad4([self.vs(x0, base - h0), self.vs(x1, base - h1), self.vs(x1, base), self.vs(x0, base)], [c0, c1, mid, mid]);
            quad4([self.vs(x0, base), self.vs(x1, base), self.vs(x1, foot), self.vs(x0, foot)], [mid, mid, low, low]);
        }
        // scan bars: dark lines at fixed heights that climb slowly, cut to the silhouette
        let cut = alpha(mixc(body, BLACK, 0.6), 0.55 * e);
        let phase = (self.time * 0.12).fract();
        let mut lvl = bar_gap * phase;
        let mut bi = 0;
        while lvl < hmax && bi < 40 {
            // bars thin out toward the top, like the sun's
            let th = (2.6 - 1.6 * lvl / hmax.max(1e-4)).max(0.8);
            for i in 0..n {
                let (h0, h1) = (hs[i], hs[i + 1]);
                if h0 <= lvl && h1 <= lvl {
                    continue;
                }
                let (mut xa, mut xb) = (fxa(i), fxa(i + 1));
                if h0 < lvl {
                    xa += (xb - xa) * (lvl - h0) / (h1 - h0);
                } else if h1 < lvl {
                    xb = xa + (xb - xa) * (h0 - lvl) / (h0 - h1);
                }
                line(self.vs(xa, base - lvl), self.vs(xb, base - lvl), th, cut);
            }
            lvl += bar_gap;
            bi += 1;
        }
        for i in 0..n {
            let a = (0.35 + 0.65 * dim) * e;
            line(self.vs(fxa(i), base - hs[i]), self.vs(fxa(i + 1), base - hs[i + 1]), 1.8, alpha(rim, 0.8 * a));
            line(self.vs(fxa(i), base - hs[i]), self.vs(fxa(i + 1), base - hs[i + 1]), 6.0, alpha(rim, 0.10 * a));
        }
    }

    /// Mesas: flat-topped buttes in three ranks drifting past at different speeds. Their heights
    /// breathe with the part of the spectrum each one listens to.
    fn draw_mesas(&self, e: f32, rise: f32) {
        const SEG: usize = 160;
        let pal = &self.pal;
        let t = self.time;
        let body = mixc(pal.bg0, BLACK, 0.4);
        let lit = mixc(pal.accent, pal.secondary, 0.45);
        let rnd = |a: f32| ((a * 12.9898).sin() * 43758.547).fract().abs();
        let mut hs = [0.0f32; SEG + 1];
        for layer in (0..3usize).rev() {
            let lf = layer as f32 / 2.0; // 1 = the farthest rank
            let spec = self.spec_ago(layer as f32 * 3.0);
            let drift = t * (0.012 - 0.007 * lf);
            let nb = 4 + layer;
            let mut buttes = [(0.0f32, 0.0f32, 0.0f32); 6];
            for (j, bt) in buttes.iter_mut().enumerate().take(nb) {
                let s = j as f32 * 7.31 + layer as f32 * 3.77;
                let c = (rnd(s) * 1.3 + j as f32 * 1.3 / nb as f32 + drift).rem_euclid(1.3) - 0.15;
                let wd = 0.045 + 0.07 * rnd(s + 1.0) * (1.0 - 0.4 * lf);
                let v = band(&spec, rnd(s + 2.0));
                let top = (0.055 + 0.085 * rnd(s + 3.0)) * (1.0 + 0.25 * lf) * (0.8 + 0.45 * v);
                *bt = (c, wd, top);
            }
            for (i, h) in hs.iter_mut().enumerate() {
                let fx = -0.02 + 1.04 * i as f32 / SEG as f32;
                // low ground between the buttes
                let mut y = 0.010 + 0.008 * (fx * 9.0 + layer as f32 * 2.0 + drift * 20.0).sin();
                for bt in buttes.iter().take(nb) {
                    let x = (fx - bt.0).abs();
                    // steep walls with a talus slope at the foot, and a level top
                    let wall = smooth(bt.1, bt.1 * 0.74, x);
                    let talus = 0.22 * smooth(bt.1 * 1.9, bt.1, x);
                    y = y.max(bt.2 * (0.78 * wall + talus));
                }
                *h = y * rise;
            }
            let rim = mixc(pal.secondary, pal.grid, lf * 0.6);
            self.striped_layer(&hs, HZ + 0.012 - 0.012 * lf, e, body, lit, rim, 0.013, 1.0 - 0.45 * lf);
        }
    }

    /// Striped hills: big soft hills in four ranks, each one the sun's stripes continued on the
    /// ground. They roll slowly sideways and the sound lifts them.
    fn draw_hills(&self, e: f32, rise: f32) {
        const SEG: usize = 120;
        let pal = &self.pal;
        let t = self.time;
        let body = mixc(pal.bg0, BLACK, 0.35);
        let lit = mixc(pal.secondary, pal.accent, 0.35);
        let mut hs = [0.0f32; SEG + 1];
        for layer in (0..4usize).rev() {
            let lf = layer as f32 / 3.0; // 1 = the farthest rank
            let spec = self.spec_ago(layer as f32 * 2.5);
            let drift = t * (0.045 - 0.03 * lf) + layer as f32 * 1.9;
            let freq = 3.2 + 2.6 * lf;
            for (i, h) in hs.iter_mut().enumerate() {
                let fx = -0.02 + 1.04 * i as f32 / SEG as f32;
                let x = fx * freq + drift;
                let big = 0.5 + 0.5 * x.sin();
                let mid = 0.5 + 0.5 * (x * 1.9 + 1.3 + layer as f32).sin();
                let v = soft_band(&spec, (fx - 0.5).abs() * 1.7);
                *h = (0.030 + (big * 0.55 + mid * 0.22 + v * 0.5) * MOUNT_H * (0.55 + 0.6 * lf)) * rise;
            }
            let rim = mixc(pal.secondary, pal.grid, lf * 0.7);
            self.striped_layer(&hs, HZ + 0.030 - 0.034 * lf, e, body, lit, rim, 0.011, 1.0 - 0.4 * lf);
        }
    }

    /// Dunes: long soft ridges drifting past, lit on the windward side and dark in the lee, with
    /// ripple lines down the lit face. The sound lifts them and the hats make the crests glint.
    fn draw_dunes(&self, e: f32, rise: f32) {
        const NL: usize = 6;
        const SEG: usize = 144;
        let pal = &self.pal;
        let t = self.time;
        let fill = mixc(pal.bg0, BLACK, 0.42);
        let sand = mixc(pal.accent, pal.secondary, 0.35);
        let glint = self.hat * self.fx.min(1.0);
        for m in (0..NL).rev() {
            let far = m as f32 / (NL - 1) as f32; // 1 = the farthest dune
            let spec = self.spec_ago(m as f32 * 2.0);
            let base = HZ + 0.034 - far * 0.040;
            let drift = t * (0.06 - 0.04 * far) + m as f32 * 2.1;
            let freq = 4.5 + 4.5 * far;
            let hmax = MOUNT_H * (0.50 + 0.45 * far);
            // the foot of a dune fades into the floor instead of ending in a line
            let bottom = alpha(fill, 0.0);
            let mut prev: Option<(Vec2, Vec2, Color, f32, [Vec2; 2])> = None;
            for i in 0..=SEG {
                let fx = i as f32 / SEG as f32;
                let x = fx * freq + drift;
                let ph = x + 0.55 * x.sin();
                let dune = 0.5 + 0.5 * ph.sin();
                let small = 0.5 + 0.5 * (x * 2.3 + 1.0 + m as f32).sin();
                let v = soft_band(&spec, (fx - 0.5).abs() * 1.6);
                let taper = 0.55 + 0.45 * smooth(0.0, 0.1, fx) * smooth(1.0, 0.9, fx);
                let h = (dune * 0.5 + small * 0.14 + v * 0.55 * (0.5 + 0.5 * dune)) * hmax * rise * taper;
                let lit = 0.5 + 0.5 * ph.cos();
                let p = self.vs(fx, base - h);
                let b = self.vs(fx, base + 0.11);
                let top = alpha(mixc(fill, sand, (0.06 + 0.56 * lit * lit) * (1.0 - 0.4 * far)), 0.96 * e);
                let rip = [self.vs(fx, base - h * 0.70), self.vs(fx, base - h * 0.42)];
                if let Some((pp, pb, ptop, plit, prip)) = prev {
                    quad4([pp, p, b, pb], [ptop, top, bottom, bottom]);
                    let l = 0.5 * (lit + plit);
                    // a few grains on each crest catch the light when a hat lands
                    let hs = ((i as f32 * 12.9898 + m as f32 * 78.233).sin() * 43758.547).fract().abs();
                    let spark = smooth(0.8, 1.0, hs) * glint;
                    line(pp, p, 1.8, alpha(mixc(sand, WHITE, 0.25 * l + 0.7 * spark), (0.85 - 0.45 * far) * (0.45 + 0.55 * l) * e));
                    line(prip[0], rip[0], 1.2, alpha(sand, 0.42 * l * l * (1.0 - 0.5 * far) * e));
                    line(prip[1], rip[1], 1.2, alpha(sand, 0.24 * l * l * (1.0 - 0.5 * far) * e));
                }
                prev = Some((p, b, top, lit, rip));
            }
        }
    }

    /// Spectrum ridgelines outside the arena edge (bass at the corners), travelling outward through
    /// time. In drift they become the horizon's mountain range (dark fills occlude older ridges).
    /// `vk` scales how much of them the vista shows, `rise` the height of the mountains.
    fn draw_ridgelines(&self, w: &World, e: f32, vk: f32, rise: f32) {
        let pal = &self.pal;
        let lv = 1.0 - e;
        let fill = mixc(pal.bg0, BLACK, 0.35);
        for m in (0..RIDGES).rev() {
            let (spec, d) = self.layer(m);
            let ff = (d / RIDGES as f32).min(1.0);
            let live = if m == 0 { 1.0 } else { 0.0 };
            let ga = RIDGE_A * (1.0 - ff).powf(1.4) * (1.0 + live * (0.4 * w.hit[0] + HAT_RIDGE * self.hat * self.fx));
            // build-up: ridges stretch outward; hats make the live ridge shimmer
            let stretch = 1.0 + TENSION_RIDGE * self.tension * self.fx * lv;
            let jit = live * HAT_JIT * self.hat * self.fx * lv;
            let va = 0.9 * (1.0 - ff * 0.75) * vk;
            let col = mixc(mixc(pal.secondary, pal.grid, ff), pal.accent, if m == 0 { 0.4 * w.hit[1] } else { 0.0 });
            let a = ga * lv + va * e;
            if a < 0.004 {
                continue;
            }
            let base = HZ - d * 0.011;
            let wsc = 1.0 - d * 0.03;
            let mut prev = (Vec2::ZERO, Vec2::ZERO);
            for i in 0..=P {
                let v = band(spec, perim_band(i));
                let (pos, n) = perim(i);
                let sh = if jit > 0.0 { jit * (i as f32 * 2.37 + self.clock * 83.0).sin() } else { 0.0 };
                let gp = pos + n * (RIM_OFF + RIDGE_BASE + (d * RIDGE_GAP + v * RIDGE_AMP * (1.0 - 0.04 * d)) * stretch + sh);
                let fx = 0.5 + (i as f32 / P as f32 - 0.5) * wsc;
                let xf = i as f32 / P as f32;
                let taper = smooth(0.0, 0.07, xf) * smooth(1.0, 0.93, xf);
                let vp = self.vs(fx, base - v * MOUNT_H * (1.0 - 0.04 * d) * taper * rise);
                let p = gp.lerp(vp, e);
                let b = self.vs(fx, base + 0.004);
                if i > 0 {
                    if e > 0.02 && vk > 0.0 {
                        quad4([prev.0, p, b, prev.1], [alpha(fill, 0.93 * e); 4]);
                    }
                    line(prev.0, p, 1.6 + 0.8 * e, alpha(col, a));
                }
                prev = (p, b);
            }
        }
    }

    /// Spectrum crown around the Heart: a few concentric history rings (bass at the top).
    fn draw_crown(&self, w: &World, lv: f32) {
        let pal = &self.pal;
        let breathe = 0.85 + 0.15 * (w.beat_phase * TAU).cos();
        for m in (0..CROWN_LAYERS).rev() {
            let (spec, d) = self.layer(m);
            let ff = (d / CROWN_LAYERS as f32).min(1.0);
            let a = CROWN_A * (1.0 - ff) * lv * (1.0 + if m == 0 { 0.6 * w.hit[0] } else { 0.0 });
            if a < 0.004 {
                continue;
            }
            let col = mixc(pal.primary, pal.secondary, ff);
            let rb = CROWN_R + d * CROWN_GAP;
            let segs = 96;
            let mut prev = Vec2::ZERO;
            for s in 0..=segs {
                let ang = s as f32 * TAU / segs as f32 - PI * 0.5;
                let f = wrap(ang + PI * 0.5).abs() / PI;
                let v = band(spec, f) * breathe;
                let p = self.pj(dir(ang) * (rb + v * CROWN_AMP), v * 26.0 * (1.0 - ff));
                if s > 0 {
                    line(prev, p, 1.6, alpha(col, a));
                }
                prev = p;
            }
        }
    }

    /// Oscilloscope ring: the actual audible waveform wrapped into a closed loop around the Heart.
    /// Kicks push it outward, build-ups pull it in, and it steps round on every beat.
    fn draw_scope(&self, w: &World, lv: f32) {
        if !self.scope_ok || self.fx <= 0.0 || w.heart.broken {
            return;
        }
        let pal = &self.pal;
        let fx = self.fx;
        let a = SCOPE_A * lv * fx.sqrt();
        if a < 0.004 {
            return;
        }
        let gain = 1.0 / self.scope_peak.max(0.12);
        let r0 = SCOPE_R * (1.0 - SCOPE_TENSE * self.tension * fx) + (SCOPE_KICK * self.kick + 10.0 * self.bass_b) * fx;
        // phase-locked rotation: snaps forward right after each beat, then holds
        let step = 1.0 - (1.0 - w.beat_phase.clamp(0.0, 1.0)).powi(4);
        let rot = (w.beat_count as f32 % 16.0 + step) * BEAT_STEP - PI * 0.5;
        let col = mixc(pal.primary, WHITE, 0.15 + 0.35 * self.kick * fx);
        let glow = alpha(pal.primary, a * 0.14);
        let stride = SCOPE_N / SCOPE_PTS;
        let mut prev = Vec2::ZERO;
        let mut first = Vec2::ZERO;
        for s in 0..=SCOPE_PTS {
            let k = s % SCOPE_PTS;
            let f = k as f32 / SCOPE_PTS as f32;
            // taper the ends to zero so the loop closes without a seam
            let win = smooth(0.0, 0.06, f) * smooth(1.0, 0.94, f);
            let v = (self.scope[k * stride] * gain).clamp(-1.6, 1.6) * win * fx.sqrt();
            let ang = rot + f * TAU;
            let p = if s == SCOPE_PTS { first } else { self.pj(dir(ang) * (r0 + v * SCOPE_AMP), SCOPE_Z + v.abs() * 6.0) };
            if s == 0 {
                first = p;
            } else {
                line(prev, p, SCOPE_W * 3.5, glow);
                line(prev, p, SCOPE_W, alpha(col, a));
            }
            prev = p;
        }
    }

    // -----------------------------------------------------------------------------------------
    // Hazards
    // -----------------------------------------------------------------------------------------

    /// Tile strikes, the star of the floor: exact footprint, a clockwise trace + growing fill that
    /// both complete on the strike, a hammer slab descending to land on the beat, and a slam.
    fn draw_tiles(&self, w: &World, lv: f32, armed_k: f32) {
        let pal = &self.pal;
        let half = TILE * 0.5 - 3.0;
        for r in 0..ROWS {
            for c in 0..COLS {
                let tile = &w.tiles[r * COLS + c];
                let p = tile_center(c, r);
                let corners = [p + vec2(-half, -half), p + vec2(half, -half), p + vec2(half, half), p + vec2(-half, half)];
                if tile.armed {
                    let k = (1.0 - (tile.strike_t - w.t) / tile.lead.max(0.05)).clamp(0.0, 1.0);
                    let fade_in = (k * 6.0).min(1.0);
                    let hot = mixc(pal.danger, WHITE, k * k * 0.5);
                    // growing fill (reaches the edges on the strike)
                    let fk = ease(k);
                    rquad(p, vec2(half * fk, half * fk), 0.0, alpha(pal.danger, (0.05 + 0.16 * k) * lv));
                    // exact footprint
                    for i in 0..4 {
                        line(corners[i], corners[(i + 1) % 4], 2.0, alpha(pal.danger, (0.35 + 0.35 * k) * fade_in * lv));
                    }
                    // clockwise trace from the top-left corner: completes exactly on the strike
                    let mut rem = k * 4.0;
                    for i in 0..4 {
                        if rem <= 0.0 {
                            break;
                        }
                        let f = rem.min(1.0);
                        let (a, b) = (corners[i], corners[(i + 1) % 4]);
                        line(a, a + (b - a) * f, 4.5, alpha(hot, lv));
                        rem -= 1.0;
                    }
                    // hammer slab descending onto the tile
                    let zt = SLAB_H * (1.0 - k).powf(1.5);
                    let sa = fade_in * (0.2 + 0.55 * k) * lv;
                    let top: [Vec2; 4] = [
                        self.pj(corners[0], zt),
                        self.pj(corners[1], zt),
                        self.pj(corners[2], zt),
                        self.pj(corners[3], zt),
                    ];
                    for i in 0..4 {
                        let j = (i + 1) % 4;
                        quad4([corners[i], corners[j], top[j], top[i]], [alpha(pal.danger, 0.07 * k * lv), alpha(pal.danger, 0.07 * k * lv), alpha(pal.danger, 0.0), alpha(pal.danger, 0.0)]);
                        line(top[i], top[j], 2.5, alpha(hot, sa));
                        seg(corners[i], top[i], 1.5, alpha(pal.danger, 0.5 * sa), alpha(pal.danger, 0.15 * sa));
                    }
                } else if armed_k > 0.0 && tile.flash <= 0.0 {
                    // safe tile: cool corner ticks
                    let a = 0.28 * ease(armed_k * 1.5) * lv;
                    let l = 12.0;
                    let h2 = half - 6.0;
                    for (sx, sy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                        let q = p + vec2(sx * h2, sy * h2);
                        line(q, q - vec2(sx * l, 0.0), 2.0, alpha(pal.primary, a));
                        line(q, q - vec2(0.0, sy * l), 2.0, alpha(pal.primary, a));
                    }
                }
                if tile.flash > 0.0 {
                    let f = tile.flash.clamp(0.0, 1.0);
                    rquad(p, vec2(half + 2.0, half + 2.0), 0.0, alpha(mixc(pal.danger, WHITE, f), f * 0.8 * lv));
                    let g = half + (1.0 - f) * 36.0;
                    rect_lines_c(p, g, 3.0, alpha(mixc(pal.danger, WHITE, f * 0.5), f * lv));
                    let h = TILE_COL_H * (0.35 + 0.65 * (1.0 - f));
                    for q in corners {
                        seg(q, self.pj(q, h), 2.5, alpha(WHITE, f * 0.8 * lv), alpha(pal.danger, 0.0));
                    }
                    let top = [self.pj(corners[0], h * 0.6), self.pj(corners[1], h * 0.6), self.pj(corners[2], h * 0.6), self.pj(corners[3], h * 0.6)];
                    for i in 0..4 {
                        line(top[i], top[(i + 1) % 4], 2.0, alpha(pal.danger, f * f * 0.6 * lv));
                    }
                }
            }
        }
    }

    fn draw_hazards(&self, w: &World, lv: f32) {
        let pal = &self.pal;
        let t = w.t;
        // Lasers: a line on the floor (exact) plus a wall of light standing on it
        for l in &w.lasers {
            let span = if l.horizontal { HALF_W + 40.0 } else { HALF_H + 40.0 };
            let pt = |pos: f32, s: f32| if l.horizontal { vec2(s, pos) } else { vec2(pos, s) };
            let wall = |pos: f32, h: f32, ab: f32, at: f32| {
                let (a, b) = (pt(pos, -span), pt(pos, span));
                quad4([a, b, self.pj(b, h), self.pj(a, h)], [alpha(pal.danger, ab), alpha(pal.danger, ab), alpha(pal.danger, 0.0), alpha(pal.danger, 0.0)]);
                line(self.pj(a, h), self.pj(b, h), 1.5, alpha(pal.danger, at));
            };
            if t < l.t0 {
                let k = (1.0 - (l.t0 - t) / l.lead).clamp(0.0, 1.0);
                let blink = 0.5 + 0.5 * (t * 24.0).sin();
                wall(l.from, LASER_H * 0.45 * ease(k), 0.16 * k * lv, 0.35 * k * lv);
                line(pt(l.from, -span), pt(l.from, span), 2.0 + 3.0 * k, alpha(pal.danger, (0.3 + 0.5 * k) * (0.65 + 0.35 * blink) * lv));
                line(pt(l.to, -span), pt(l.to, span), 1.5, alpha(pal.danger, 0.2 * lv));
                line(pt(l.from, -span), pt(l.to, -span), 1.5, alpha(pal.danger, 0.2 * lv));
                let wdt = 60.0 * (1.0 - k) + 14.0;
                line(pt(l.from - wdt, -span), pt(l.from - wdt, span), 1.5, alpha(pal.danger, 0.4 * k * lv));
                line(pt(l.from + wdt, -span), pt(l.from + wdt, span), 1.5, alpha(pal.danger, 0.4 * k * lv));
            } else if l.active(t) {
                let pos = l.pos_at(t);
                let wob = 1.0 + 0.2 * (t * 60.0).sin();
                wall(pos, LASER_H, 0.32 * lv, 0.55 * lv);
                // scrolling verticals in the wall
                let off = (t * 420.0).rem_euclid(96.0);
                let mut s = -span + off;
                while s < span {
                    let b = pt(pos, s);
                    seg(b, self.pj(b, LASER_H), 2.0, alpha(WHITE, 0.35 * lv), alpha(pal.danger, 0.0));
                    s += 96.0;
                }
                line(pt(pos, -span), pt(pos, span), 34.0 * wob, alpha(pal.danger, 0.22 * lv));
                line(pt(pos, -span), pt(pos, span), 18.0 * wob, alpha(pal.danger, 0.75 * lv));
                line(pt(pos, -span), pt(pos, span), 7.0, alpha(WHITE, lv));
            } else {
                let k = (1.0 - (t - l.t0 - l.dur) / 0.3).clamp(0.0, 1.0);
                wall(l.to, LASER_H * k, 0.2 * k * lv, 0.3 * k * lv);
                line(pt(l.to, -span), pt(l.to, span), 10.0 * k, alpha(pal.danger, 0.5 * k * lv));
            }
        }
        // Shockwaves: hot ring with a wall; the safe gaps are framed by cool posts
        for wv in &w.waves {
            set_origin(wv.origin);
            let n = wv.gaps.max(1);
            let half = 0.36;
            if t < wv.t0 {
                let k = ((t - (wv.t0 - wv.lead)) / wv.lead).clamp(0.0, 1.0);
                let rr = HEART_R + 26.0 + 300.0 * (1.0 - k);
                let z = 110.0 * (1.0 - k);
                self.circle3(Vec2::ZERO, rr, z, 64, 3.0, alpha(pal.danger, 0.75 * k * lv));
                for g in 0..n {
                    let cgap = wv.gap + g as f32 * TAU / n as f32;
                    for s in [-half, half] {
                        let d = dir(cgap + s);
                        seg(d * 110.0, d * 900.0, 2.5, alpha(pal.primary, 0.45 * k * lv), alpha(pal.primary, 0.0));
                    }
                    let d = dir(cgap);
                    seg(d * 110.0, d * 900.0, 1.5, alpha(pal.primary, 0.2 * k * lv), alpha(pal.primary, 0.0));
                }
            } else {
                let rr = wv.radius(t);
                for g in 0..n {
                    let c0 = wv.gap + g as f32 * TAU / n as f32 + half;
                    let c1 = wv.gap + (g + 1) as f32 * TAU / n as f32 - half;
                    // wall
                    let steps = (((c1 - c0) * rr / 40.0).ceil() as usize).clamp(4, 160);
                    let mut pb = dir(c0) * rr;
                    let mut pt_ = self.pj(pb, WAVE_H);
                    for s in 1..=steps {
                        let b = dir(c0 + (c1 - c0) * s as f32 / steps as f32) * rr;
                        let tb = self.pj(b, WAVE_H);
                        quad4([pb, b, tb, pt_], [alpha(pal.danger, 0.28 * lv), alpha(pal.danger, 0.28 * lv), alpha(pal.danger, 0.0), alpha(pal.danger, 0.0)]);
                        line(pt_, tb, 1.5, alpha(pal.danger, 0.45 * lv));
                        pb = b;
                        pt_ = tb;
                    }
                    arc(Vec2::ZERO, rr, c0, c1, 26.0, alpha(pal.danger, 0.3 * lv));
                    arc(Vec2::ZERO, rr, c0, c1, 12.0, alpha(pal.danger, 0.9 * lv));
                    arc(Vec2::ZERO, rr, c0, c1, 4.0, alpha(WHITE, lv));
                    // gap posts
                    for a in [c0, c1] {
                        let b = dir(a) * rr;
                        seg(b, self.pj(b, WAVE_H * 1.8), 3.0, alpha(pal.primary, 0.9 * lv), alpha(pal.primary, 0.1 * lv));
                    }
                }
            }
        }
        // Spawn portals: scattered lines converging into the enemy's wireframe
        for p in &w.portals {
            let k = ((t - p.t0) / (p.t1 - p.t0).max(0.05)).clamp(0.0, 1.0);
            let col = if p.kind == EnemyKind::Elite { pal.danger } else { pal.secondary };
            let r = 64.0 * (1.0 - k) + 18.0;
            arc(p.pos, r, 0.0, TAU, 2.0, alpha(col, (0.3 + 0.6 * k) * lv));
            arc(p.pos, r * 0.6, k * TAU, k * TAU + PI, 1.5, alpha(WHITE, 0.5 * k * lv));
            let edges = &self.kinds[kind_index(p.kind)];
            let spread = 1.0 + 3.5 * (1.0 - k) * (1.0 - k);
            let m = Mat3::from_rotation_z(t * 2.0 + (1.0 - k) * 6.0) * Mat3::from_rotation_x(0.7 + (1.0 - k) * 4.0);
            let sc = 18.0;
            let z = 22.0 + 60.0 * (1.0 - k);
            for ed in edges.iter() {
                let mid = (ed[0] + ed[1]) * 0.5;
                let off = mid * (spread - 1.0);
                let a = m * ((ed[0] + off) * sc);
                let b = m * ((ed[1] + off) * sc);
                self.edge3(p.pos, z, a, b, 1.8, col, (0.2 + 0.8 * k) * lv, sc);
            }
            if p.kind == EnemyKind::Darter {
                let d = (w.player.pos - p.pos).normalize_or_zero();
                let e = p.pos + d * 1700.0;
                seg(p.pos, e, 2.0, alpha(pal.accent, 0.45 * k * lv), alpha(pal.accent, 0.0));
            }
        }
        set_origin(Vec2::ZERO);
    }

    /// Announced Heart attack: an approach ring descending and closing onto the Heart (lands on
    /// the fire time), plus the danger zone on the floor: wedges growing out from the Heart over
    /// the region the bullets will cover, with a spoke per bullet. Gaps between wedges are safe.
    fn draw_pulse(&self, w: &World, p: &PendingPulse, lv: f32) {
        let k = (1.0 - (p.t - w.t) / p.lead.max(0.05)).clamp(0.0, 1.0);
        let aim = p.aim;
        self.draw_pulse_zone(p.pattern, aim, p.rot, p.count as usize, p.soft, k, lv);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_pulse_zone(&self, pattern: Pattern, aim: f32, rot: f32, count: usize, soft: bool, k: f32, lv: f32) {
        let pal = &self.pal;
        let col = if count == 0 { pal.secondary } else if soft { WHITE } else { pal.danger };
        let a = lv * (0.25 + 0.75 * ease(k * 1.4));
        // timing: approach ring
        let rr = HEART_R + PULSE_R0 * (1.0 - k).powf(1.3);
        let z = PULSE_Z * (1.0 - k);
        self.circle3(Vec2::ZERO, rr, z, 56, 1.5 + 2.5 * k, alpha(col, a * (0.3 + 0.55 * k)));
        if count == 0 {
            return;
        }
        let mut angs = [(0.0f32, 0.0f32); 64];
        let mut n = 0usize;
        pulse_dirs(pattern, aim, rot, count, |ang, sp| {
            if n < angs.len() {
                angs[n] = (wrap(ang - aim), sp);
                n += 1;
            }
        });
        if n == 0 {
            return;
        }
        angs[..n].sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
        // spokes / trajectory rays
        let reach = HEART_R + ZONE_REACH * ease(k);
        for &(ang, sp) in &angs[..n] {
            let d = dir(ang + aim);
            let _ = sp;
            line(self.pj(d * rr, z), self.pj(d * (rr + 14.0 + 22.0 * k), z), 2.5 + 1.5 * k, alpha(mixc(col, WHITE, 0.4), a * (0.4 + 0.6 * k)));
        }
        // danger wedges: merge neighbouring bullets, the remaining big gaps are the open space
        let mut gi = 0;
        let mut gmax = -1.0;
        for j in 0..n {
            let a1 = if j + 1 < n { angs[j + 1].0 } else { angs[0].0 + TAU };
            if a1 - angs[j].0 > gmax {
                gmax = a1 - angs[j].0;
                gi = j;
            }
        }
        let za = a * (0.16 + 0.5 * k * k);
        if gmax < ZONE_MERGE {
            self.circle3(Vec2::ZERO, reach, 0.0, 72, 2.0, alpha(col, za));
            return;
        }
        let first = (gi + 1) % n;
        let mut start = angs[first].0;
        let mut end = start;
        for m in 1..n {
            let mut ang = angs[(first + m) % n].0;
            while ang < end - 1e-4 {
                ang += TAU;
            }
            if ang - end > ZONE_MERGE {
                self.wedge(aim + start - ZONE_PAD, aim + end + ZONE_PAD, reach, col, za, k);
                start = ang;
            }
            end = ang;
        }
        self.wedge(aim + start - ZONE_PAD, aim + end + ZONE_PAD, reach, col, za, k);
    }

    /// Danger wedge outline + faint fill on the floor between the Heart and `reach`.
    fn wedge(&self, a0: f32, a1: f32, reach: f32, col: Color, a: f32, k: f32) {
        let r0 = HEART_R + 2.0;
        let steps = (((a1 - a0) * reach / 26.0).ceil() as usize).clamp(2, 96);
        let mut prev = a0;
        for s in 1..=steps {
            let ang = a0 + (a1 - a0) * s as f32 / steps as f32;
            let (d0, d1) = (dir(prev), dir(ang));
            quad4([d0 * r0, d1 * r0, d1 * reach, d0 * reach], [alpha(col, 0.34 * a * k), alpha(col, 0.34 * a * k), alpha(col, 0.03 * a), alpha(col, 0.03 * a)]);
            line(d0 * reach, d1 * reach, 1.2, alpha(col, a * 0.3));
            prev = ang;
        }
        seg(dir(a0) * r0, dir(a0) * reach, 1.2, alpha(col, a * 0.35), alpha(col, 0.0));
        seg(dir(a1) * r0, dir(a1) * reach, 1.2, alpha(col, a * 0.35), alpha(col, 0.0));
    }

    // -----------------------------------------------------------------------------------------
    // Wireframe helpers
    // -----------------------------------------------------------------------------------------

    /// One wireframe edge: local (already rotated + scaled) endpoints around (c, z), with depth cue.
    #[allow(clippy::too_many_arguments)]
    fn edge3(&self, c: Vec2, z: f32, a: Vec3, b: Vec3, w: f32, col: Color, al: f32, rad: f32) {
        let pa = self.pj(c + vec2(a.x, a.y), z + a.z);
        let pb = self.pj(c + vec2(b.x, b.y), z + b.z);
        let da = (a.z / rad.max(1.0)).clamp(-1.0, 1.0) * 0.5 + 0.5;
        let db = (b.z / rad.max(1.0)).clamp(-1.0, 1.0) * 0.5 + 0.5;
        seg(pa, pb, w, alpha(col, al * (0.45 + 0.55 * da)), alpha(col, al * (0.45 + 0.55 * db)));
    }

    fn circle3(&self, c: Vec2, r: f32, z: f32, segs: usize, w: f32, col: Color) {
        if col.a <= 0.003 {
            return;
        }
        let mut prev = self.pj(c + vec2(r, 0.0), z);
        for s in 1..=segs {
            let p = self.pj(c + dir(s as f32 * TAU / segs as f32) * r, z);
            line(prev, p, w, col);
            prev = p;
        }
    }

    // -----------------------------------------------------------------------------------------
    // Heart
    // -----------------------------------------------------------------------------------------

    /// The Heart: two nested morphing wireframe solids + gyroscope rings + core. In drift it rises
    /// into the sun as a calm, large wireframe. Shatters into drifting fragments when broken.
    fn draw_heart(&self, w: &World, e: f32, _progress: f32) {
        let pal = &self.pal;
        let h = &w.heart;
        let lv = 1.0 - e;
        let t = self.time;
        let vpos = self.vs(0.5, SUN_Y);
        let pos = Vec2::ZERO.lerp(vpos, e);
        let vscale = SUN_R * self.view.sh / self.view.scale * 0.62 / HEART_SHELL_R;
        let sc = 1.0 + (vscale - 1.0) * e;
        let z = (HEART_Z + 6.0 * (w.beat_phase * TAU).sin()) * lv;
        let punch = h.punch.max(w.hit[0] * 0.6) * lv + w.hit[0] * 0.2 * e;
        let reform = ease(h.reform) * lv;
        let broken = h.broken && lv > 0.5;
        let spin = 1.0 - 0.6 * e;
        let mo = Mat3::from_rotation_z(h.rot * 0.5 * lv + t * 0.15 * spin + reform * 2.0)
            * Mat3::from_rotation_x(0.75 + 0.25 * (t * 0.4).sin())
            * Mat3::from_rotation_y(t * 0.45 * spin + reform * 3.0);
        let mi = Mat3::from_rotation_z(-t * 0.6 * spin) * Mat3::from_rotation_x(-0.5 + reform) * Mat3::from_rotation_y(-t * 0.8 * spin);
        let mt = ease(self.heart_mt);
        let r_out = HEART_SHELL_R * sc * (1.0 + 0.16 * punch + 0.08 * self.low_s * lv + 0.45 * reform);
        let r_in = r_out * (0.46 + 0.06 * punch);
        let flash = h.flash * lv;
        let c_out = mixc(mixc(pal.primary, WHITE, flash * 0.8 + 0.25 * punch), mixc(pal.accent, WHITE, 0.3), e);
        let c_in = mixc(mixc(pal.secondary, WHITE, flash * 0.6), pal.secondary, e);
        // Purgatory: the Heart is a pale ghost of itself, its shell drifting apart.
        let ghost = w.purg * lv * if broken { 0.0 } else { 1.0 };
        let c_out = mixc(c_out, alpha(WHITE, 0.75), ghost);
        let c_in = mixc(c_in, alpha(pal.accent, 0.8), ghost);
        // Overdrive burns hot; a hurt Heart flickers.
        let hot = if h.form == HeartForm::Overdrive { lv * (1.0 - ghost) } else { 0.0 };
        let c_out = mixc(c_out, mixc(pal.danger, pal.accent, 0.5 + 0.5 * (t * 9.0).sin()), 0.55 * hot);
        let hurt = (1.0 - w.phase_frac()) * lv * (1.0 - ghost) * if w.phases.is_empty() { 0.0 } else { 1.0 };
        let base_a = (if broken { 0.5 } else { 0.95 } * lv + 0.15 * e) * (1.0 - 0.45 * ghost);
        let shatter = (self.broken_t * 55.0).min(190.0);
        let drift = 26.0 * ghost;
        for (shell, (from, to), m, rad, col, wdt) in [
            (0, (&self.heart_from.0, &self.solids[self.heart_key.0]), mo, r_out, c_out, 2.6),
            (1, (&self.heart_from.1, &self.solids[self.heart_key.1]), mi, r_in, c_in, 2.0),
        ] {
            for kk in 0..ME {
                let mut a = m * (from[kk][0].lerp(to[kk][0], mt) * rad);
                let mut b = m * (from[kk][1].lerp(to[kk][1], mt) * rad);
                if broken {
                    let mid = (a + b) * 0.5;
                    let off = mid.normalize_or_zero() * shatter * (1.0 + shell as f32 * 0.5) + vec3(0.0, 0.0, -shatter * 0.2);
                    let sr = Mat3::from_rotation_z(self.broken_t * (0.5 + (kk % 5) as f32 * 0.3));
                    a = mid + off + sr * ((a - mid) * 0.8);
                    b = mid + off + sr * ((b - mid) * 0.8);
                } else if drift > 0.1 {
                    // ghost: every edge floats a little way out and bobs on its own
                    let mid = (a + b) * 0.5;
                    let off = mid.normalize_or_zero() * drift * (1.0 + 0.4 * (t * 1.3 + kk as f32 * 1.7).sin());
                    a = mid + off + (a - mid) * 0.82;
                    b = mid + off + (b - mid) * 0.82;
                } else if hurt > 0.25 && shell == 0 {
                    // cracks: as the phase runs down, more of the shell's edges jitter loose
                    let loose = ((kk * 7 + 3) % ME) as f32 / ME as f32;
                    if loose < hurt - 0.25 {
                        let j = (t * 23.0 + kk as f32 * 5.1).sin() * 5.0 * hurt;
                        let mid = (a + b) * 0.5;
                        let off = mid.normalize_or_zero() * j;
                        a += off;
                        b += off;
                    }
                }
                self.edge3(pos, z, a, b, wdt * (1.0 + 0.6 * e), col, base_a, rad);
            }
        }
        if !broken {
            // gyroscope rings
            let gr = r_out * 1.42;
            for (gi, (ax, ay)) in [(t * 0.7, 1.1), (1.9 + t * 0.5, -0.4)].into_iter().enumerate() {
                let m = Mat3::from_rotation_z(ax * spin) * Mat3::from_rotation_x(ay + 0.4 * (t * 0.3 + gi as f32).sin());
                let segs = 48;
                let col = if gi == 0 { pal.primary } else { pal.secondary };
                let mut prev = m * vec3(gr, 0.0, 0.0);
                for s in 1..=segs {
                    let cur = m * (dir(s as f32 * TAU / segs as f32).extend(0.0) * gr);
                    self.edge3(pos, z, prev, cur, 1.5, col, 0.45 * lv * (1.0 + 0.5 * punch), gr);
                    prev = cur;
                }
            }
            // core: a beating light. It is dark while the shell is shut, a hollow ring in purgatory.
            let core = self.pj(pos, z);
            let cr = (5.0 + 8.0 * punch + 3.0 * self.low_s) * (1.0 + 0.6 * e);
            let lit = (0.25 + 0.75 * h.open) * (1.0 - ghost);
            let core_col = mixc(mixc(pal.primary, WHITE, 0.5), mixc(pal.danger, pal.accent, 0.5), hot);
            disc2(core, cr * (3.0 + 2.5 * hot), 20, alpha(core_col, (0.35 * lv + 0.1 * e) * (0.4 + 0.6 * lit)), alpha(pal.primary, 0.0));
            disc(core, cr, 16, alpha(WHITE, (0.6 + 0.4 * punch) * (lv + 0.25 * e) * lit));
            if hot > 0.01 {
                // overdrive: spikes of light thrown out on every kick
                for k in 0..10 {
                    let a = k as f32 * TAU / 10.0 + t * 0.6;
                    let len = r_out * (0.9 + 0.9 * punch + 0.25 * (t * 7.0 + k as f32 * 2.1).sin());
                    seg(core + dir(a) * cr * 1.2, core + dir(a) * len, 3.0, alpha(pal.accent, 0.7 * hot), alpha(pal.danger, 0.0));
                }
            }
            if h.form == HeartForm::Valve && lv > 0.5 {
                // valve: six iris blades that swing shut and open with the bar
                let shut = 1.0 - h.open;
                for k in 0..6 {
                    let a = k as f32 * TAU / 6.0 + t * 0.25;
                    let tip = core + dir(a + 1.1 * shut) * r_out * (0.25 + 0.6 * h.open);
                    seg(core + dir(a) * r_out * 0.98, tip, 2.5, alpha(pal.secondary, 0.85 * lv), alpha(pal.accent, 0.5 * lv));
                }
            }
            if ghost > 0.01 {
                // purgatory: a hollow halo, a clock that drains, and embers circling the void
                let left = (w.purg_until - w.t).min(w.phase_left()).max(0.0);
                let frac = (left / 9.0).clamp(0.0, 1.0);
                disc2(core, r_out * 0.75, 22, alpha(pal.bg0, 0.85 * ghost), alpha(pal.bg0, 0.0));
                arc(core, r_out * 0.55, 0.0, TAU, 2.0, alpha(WHITE, 0.55 * ghost));
                arc(core, r_out * 1.75, -PI * 0.5, -PI * 0.5 + TAU * frac, 4.0, alpha(pal.accent, 0.9 * ghost));
                arc(core, r_out * 1.75, 0.0, TAU, 1.0, alpha(WHITE, 0.2 * ghost));
                for k in 0..9 {
                    let a = k as f32 * TAU / 9.0 - t * (0.5 + 0.07 * k as f32);
                    let rr = r_out * (1.25 + 0.2 * (t * 1.1 + k as f32).sin());
                    let q = core + dir(a) * rr;
                    poly_lines(q, 4, 5.0, a, 1.5, alpha(pal.accent, 0.8 * ghost));
                    seg(q, core + dir(a + 0.25) * rr, 1.5, alpha(pal.accent, 0.4 * ghost), alpha(pal.accent, 0.0));
                }
            }
        }
    }

    /// A relocation: the destination is ringed and a dashed path runs to it, from the moment it
    /// is announced until the Heart arrives.
    fn draw_heart_move(&self, w: &World, lv: f32) {
        let h = &w.heart;
        if !h.relocating(w.t) || h.move_to.distance(h.pos) < 2.0 {
            return;
        }
        let pal = &self.pal;
        if h.charging {
            // a charge: the whole lane it will cross is lit, brightening until it goes
            let d = h.move_to - h.pos;
            let n = d.normalize_or_zero();
            let side = vec2(-n.y, n.x) * (HEART_R + 16.0);
            let go = (1.0 - (h.move_t0 - w.t).max(0.0) / 0.7).clamp(0.2, 1.0);
            let c0 = alpha(pal.danger, 0.34 * go * lv);
            let c1 = alpha(pal.danger, 0.10 * go * lv);
            quad4([h.pos + side, h.move_to + side, h.move_to - side, h.pos - side], [c0, c1, c1, c0]);
            line(h.pos + side, h.move_to + side, 2.5, alpha(pal.danger, 0.9 * go * lv));
            line(h.pos - side, h.move_to - side, 2.5, alpha(pal.danger, 0.9 * go * lv));
        }
        let wait = ((h.move_t0 - w.t) / (h.move_t1 - h.move_t0).max(0.1)).clamp(0.0, 1.0);
        let a = lv * (0.9 - 0.3 * wait);
        let pulse = 0.8 + 0.2 * (self.time * 10.0).sin();
        disc2(h.move_to, HEART_R + 18.0, 24, alpha(pal.danger, 0.14 * a), alpha(pal.danger, 0.0));
        arc(h.move_to, HEART_R + PLAYER_R, 0.0, TAU, 3.0, alpha(pal.danger, a * pulse));
        arc(h.move_to, HEART_R * 0.45, self.time * 2.0, self.time * 2.0 + PI * 1.3, 2.0, alpha(WHITE, 0.7 * a));
        let d = h.move_to - h.pos;
        let len = d.length();
        let n = d / len.max(1.0);
        let mut s = HEART_R + 10.0 + (self.time * 180.0) % 36.0;
        while s < len - HEART_R - 6.0 {
            line(h.pos + n * s, h.pos + n * (s + 16.0).min(len - HEART_R), 2.5, alpha(pal.danger, 0.75 * a));
            s += 36.0;
        }
    }

    /// The Heart's floor markings (z = 0): hitbox ring, shadow, tether, song clock, integrity, charge.
    fn draw_heart_rings(&self, w: &World, lv: f32, progress: f32) {
        let pal = &self.pal;
        let h = &w.heart;
        if h.charge > 0.0 {
            disc2(Vec2::ZERO, HEART_R + 40.0 * h.charge, 24, alpha(pal.danger, 0.3 * h.charge * lv), alpha(pal.danger, 0.0));
            arc(Vec2::ZERO, HEART_R + 40.0 * (1.0 - h.charge) + 8.0, 0.0, TAU, 3.0, alpha(pal.danger, h.charge * lv));
        }
        disc2(Vec2::ZERO, HEART_R, 24, alpha(pal.bg0, 0.35 * lv), alpha(pal.bg0, 0.0));
        arc(Vec2::ZERO, HEART_R, 0.0, TAU, 2.0, alpha(pal.primary, 0.4 * lv));
        let top = self.pj(Vec2::ZERO, HEART_Z - 20.0);
        seg(Vec2::ZERO, top, 2.0, alpha(pal.primary, 0.35 * lv), alpha(pal.primary, 0.0));
        // The beat ring: every pulse is timed to cross it on a beat. It lights with the beat and
        // carries a tick for each beat of the bar.
        let br = BEAT_RING_R;
        arc(Vec2::ZERO, br, 0.0, TAU, 1.5, alpha(WHITE, (0.06 + 0.22 * w.beat_pulse) * lv));
        arc(Vec2::ZERO, br, 0.0, TAU, 5.0, alpha(pal.primary, 0.10 * w.beat_pulse * lv));
        for k in 0..16 {
            let a = k as f32 * TAU / 16.0;
            let long = k % 4 == 0;
            let d = dir(a);
            line(d * (br - if long { 9.0 } else { 4.0 }), d * (br + if long { 9.0 } else { 4.0 }), 1.5, alpha(WHITE, (0.10 + 0.25 * w.beat_pulse) * lv));
        }
        arc(Vec2::ZERO, 132.0, 0.0, TAU, 1.5, alpha(WHITE, 0.12 * lv));
        arc(Vec2::ZERO, 132.0, -PI * 0.5, -PI * 0.5 + TAU * progress.clamp(0.0, 1.0), 3.0, alpha(pal.accent, 0.9 * lv));
        if !h.broken {
            // The arc under the Heart is the current phase (the whole Heart when there are none).
            let frac = if w.phases.is_empty() { (h.hp / h.max_hp.max(1.0)).clamp(0.0, 1.0) } else { w.phase_frac() };
            arc(Vec2::ZERO, 144.0, PI * 0.5 - PI * frac, PI * 0.5 + PI * frac, 4.0, alpha(mixc(pal.danger, WHITE, h.flash), 0.85 * lv));
            // Shell: a ring that closes over the Heart; it parts into four plates as it opens.
            let shut = (1.0 - h.open) * lv * (1.0 - w.purg);
            if shut > 0.02 {
                let r = HEART_R + 12.0;
                let gap = 0.35 * h.open;
                for q in 0..4 {
                    let a0 = self.time * 0.3 + q as f32 * TAU / 4.0 + gap;
                    arc(Vec2::ZERO, r, a0, a0 + TAU / 4.0 - 2.0 * gap - 0.04, 5.0, alpha(pal.secondary, 0.9 * shut));
                }
                disc2(Vec2::ZERO, r, 24, alpha(pal.secondary, 0.16 * shut), alpha(pal.secondary, 0.05 * shut));
            } else if h.form == HeartForm::Valve || h.form == HeartForm::Overdrive {
                let k = 0.5 + 0.5 * (self.time * 8.0).sin();
                arc(Vec2::ZERO, HEART_R + 12.0, 0.0, TAU, 2.0, alpha(pal.accent, (0.35 + 0.3 * k) * lv * h.open));
            }
            // Nodes: shield nodes (Shell) or linked cores (Split).
            let lit = h.lit_node();
            let n = h.nodes.len();
            for k in 0..n {
                let nd = &h.nodes[k];
                if nd.hp <= 0.0 {
                    continue;
                }
                let p = h.node_pos(k) - h.pos;
                let live = h.form == HeartForm::Shell || lit == Some(k);
                let col = mixc(if live { pal.accent } else { alpha(WHITE, 0.45) }, WHITE, nd.flash);
                // tether to the Heart
                let d = p.normalize_or_zero();
                seg(d * (HEART_R + 14.0), p - d * NODE_R, if live { 2.0 } else { 1.0 }, alpha(col, 0.1 * lv), alpha(col, if live { 0.55 } else { 0.25 } * lv));
                if h.form == HeartForm::Split {
                    let q = h.node_pos((k + 1) % n) - h.pos;
                    if h.nodes[(k + 1) % n].hp > 0.0 {
                        line(p, q, 1.0, alpha(WHITE, 0.18 * lv));
                    }
                }
                disc2(p, NODE_R * 1.3, 16, alpha(pal.bg0, 0.6 * lv), alpha(pal.bg0, 0.0));
                arc(p, NODE_R, 0.0, TAU, if live { 2.5 } else { 1.5 }, alpha(col, 0.85 * lv));
                let spin = self.time * if live { 2.2 } else { 0.6 } + k as f32;
                poly_lines(p, 4, NODE_R * 0.72 * (1.0 + 0.2 * nd.flash), spin, 2.5, alpha(col, lv));
                if live {
                    poly_lines(p, 4, NODE_R * 0.36, -spin, 2.0, alpha(WHITE, 0.9 * lv));
                    let f = (nd.hp / nd.max_hp.max(1e-3)).clamp(0.0, 1.0);
                    arc(p, NODE_R + 7.0, -PI * 0.5, -PI * 0.5 + TAU * f, 3.0, alpha(pal.danger, 0.9 * lv));
                }
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Entities
    // -----------------------------------------------------------------------------------------

    fn draw_pickups(&self, w: &World, tscale: f32) {
        let pal = &self.pal;
        for s in &w.shards {
            let fade = (s.life / 1.5).min(1.0) * (s.age * 6.0).min(1.0);
            let spin = self.time * 3.0 + s.pos.x * 0.01;
            if s.heal {
                let c = alpha(pal.primary, fade);
                let beat = 1.0 + 0.18 * w.beat_pulse;
                disc2(s.pos, 30.0 * beat, 18, alpha(pal.primary, 0.28 * fade), alpha(pal.primary, 0.0));
                rquad(s.pos, vec2(11.0, 3.5), 0.0, c);
                rquad(s.pos, vec2(3.5, 11.0), 0.0, c);
                rquad(s.pos, vec2(7.0, 1.4), 0.0, alpha(WHITE, fade));
                rquad(s.pos, vec2(1.4, 7.0), 0.0, alpha(WHITE, fade));
                arc(s.pos, 19.0 * beat, 0.0, TAU, 2.0, alpha(WHITE, fade * 0.85));
                arc(s.pos, 25.0 * beat, self.time * 2.0, self.time * 2.0 + PI * 0.6, 2.0, alpha(pal.primary, fade));
                arc(s.pos, 25.0 * beat, self.time * 2.0 + PI, self.time * 2.0 + PI * 1.6, 2.0, alpha(pal.primary, fade));
                seg(s.pos, self.pj(s.pos, 46.0), 2.0, alpha(pal.primary, 0.5 * fade), alpha(pal.primary, 0.0));
            } else {
                // A crystal: a tall diamond turning on its axis (its width breathes as it turns),
                // a bright facet line, a floor glint.
                let turn = spin.cos();
                let (hw, hh) = (2.0 + 5.0 * turn.abs(), 9.5);
                let top = s.pos + vec2(0.0, -hh);
                let bot = s.pos + vec2(0.0, hh);
                let (l, r) = (s.pos + vec2(-hw, 0.0), s.pos + vec2(hw, 0.0));
                let c0 = alpha(pal.accent, fade);
                let c1 = alpha(mixc(pal.accent, WHITE, 0.65), fade);
                disc2(s.pos, 15.0, 10, alpha(pal.accent, 0.22 * fade), alpha(pal.accent, 0.0));
                quad4([top, r, bot, l], [c1, c0, c0, c1]);
                line(top, bot, 1.2, alpha(WHITE, 0.9 * fade));
                if turn > 0.92 {
                    let g = (turn - 0.92) / 0.08;
                    line(s.pos - vec2(13.0 * g, 0.0), s.pos + vec2(13.0 * g, 0.0), 1.5, alpha(WHITE, g * fade));
                    line(s.pos - vec2(0.0, 15.0 * g), s.pos + vec2(0.0, 15.0 * g), 1.5, alpha(WHITE, g * fade));
                }
            }
        }
        // The charge: a bright prism on a beam, with a pointer toward the Heart
        if let Some(f) = w.fork {
            let pulse = 1.0 + 0.15 * w.beat_pulse;
            disc2(f, 70.0, 22, alpha(pal.accent, 0.22), alpha(pal.accent, 0.0));
            seg(f, self.pj(f, 150.0), 20.0, alpha(pal.accent, 0.22), alpha(pal.accent, 0.0));
            poly_lines(f, 3, 24.0 * pulse, self.time * 2.0, 3.0, WHITE);
            poly_lines(f, 3, 24.0 * pulse, self.time * 2.0 + PI, 3.0, pal.accent);
            arc(f, 40.0 * pulse, 0.0, TAU, 2.0, alpha(pal.accent, 0.8));
            let d = (w.heart.pos - f).normalize_or_zero();
            seg(f + d * 50.0, f + d * 120.0, 3.0, alpha(pal.accent, 0.8), alpha(pal.accent, 0.0));
            wtext("CHARGE", f + vec2(0.0, -58.0), 20.0, pal.accent, tscale);
        }
        if w.carrying {
            // carried: the ring to reach is drawn round the Heart, with a line to it
            let p = w.player.pos;
            let h = w.heart.pos;
            let rr = HEART_R + FORK_DELIVER_R;
            let pulse = 0.6 + 0.4 * (self.time * 8.0).sin();
            arc(h, rr, 0.0, TAU, 3.0, alpha(pal.accent, pulse));
            disc2(h, rr, 28, alpha(pal.accent, 0.0), alpha(pal.accent, 0.10));
            let d = (h - p).normalize_or_zero();
            let len = (p.distance(h) - rr).max(0.0);
            let mut s0 = 34.0 + (self.time * 160.0) % 30.0;
            while s0 < len {
                line(p + d * s0, p + d * (s0 + 14.0).min(len), 2.5, alpha(pal.accent, 0.7));
                s0 += 30.0;
            }
            poly_lines(p, 3, 34.0, -self.time * 3.0, 2.5, alpha(pal.accent, 0.95));
        }
        // Ear-candy comets: a bright head with a ring, on the floor plane.
        for p in &w.painters {
            let tw = 0.7 + 0.3 * (self.time * 18.0).sin();
            disc2(p.pos, 34.0, 18, alpha(pal.accent, 0.35 * tw), alpha(pal.accent, 0.0));
            disc(p.pos, 7.0, 12, WHITE);
            poly_lines(p.pos, 4, 16.0, self.time * 6.0, 2.5, pal.accent);
            poly_lines(p.pos, 4, 24.0, -self.time * 4.0, 1.5, alpha(WHITE, 0.7));
        }
        for o in &w.orbs {
            let fade = (o.life / 2.0).min(1.0) * (o.age * 3.0).min(1.0);
            let pulse = 1.0 + 0.08 * (self.time * 5.0 + o.pos.x).sin() + 0.06 * w.beat_pulse;
            let col = if o.rare { mixc(pal.danger, WHITE, 0.25) } else { pal.accent };
            // floor: glow, a slowly turning hexagon and the time left
            disc2(o.pos, 62.0, 24, alpha(col, 0.20 * fade), alpha(col, 0.0));
            disc(o.pos, 30.0, 20, alpha(pal.bg0, 0.7 * fade));
            poly_lines(o.pos, 6, 40.0 * pulse, -self.time * 0.5, 2.0, alpha(pal.primary, fade * 0.8));
            poly_lines(o.pos, 6, 46.0 * pulse, self.time * 0.3, 1.0, alpha(col, fade * 0.5));
            arc(o.pos, 54.0, -PI * 0.5, -PI * 0.5 + TAU * (o.life / 14.0).clamp(0.0, 1.0), 2.5, alpha(WHITE, 0.6 * fade));
            // a column of light, and the upgrade's emblem floating in it
            let top = self.pj(o.pos, 120.0);
            seg(o.pos, top, 26.0, alpha(col, 0.16 * fade), alpha(col, 0.0));
            seg(o.pos, top, 3.0, alpha(WHITE, 0.35 * fade), alpha(col, 0.0));
            let c = self.pj(o.pos, 30.0 + 5.0 * (self.time * 2.4 + o.pos.x).sin());
            let m = Mat3::from_rotation_z(self.time * 0.7 + o.pos.x) * Mat3::from_rotation_x(0.6) * Mat3::from_rotation_y(self.time * 0.9);
            let sc = 31.0 * pulse;
            for ed in self.solids[if o.rare { SH_STELLA } else { SH_ICOSA }].iter() {
                self.edge3(o.pos, 30.0, m * (ed[0] * sc), m * (ed[1] * sc), 1.2, col, fade * 0.45, sc);
            }
            upgrade_emblem(o.up, c, 15.0 * pulse, 2.6, alpha(WHITE, fade), alpha(col, fade), self.time);
            if o.rare {
                poly_lines(o.pos, 3, 60.0 * pulse, self.time * 0.9, 2.5, alpha(pal.danger, fade));
                poly_lines(o.pos, 3, 60.0 * pulse, self.time * 0.9 + PI, 2.5, alpha(pal.danger, fade * 0.7));
                wtext("RARE", o.pos + vec2(0.0, -96.0), 16.0, alpha(pal.danger, fade), tscale);
            }
            wtext(o.up.name(), o.pos + vec2(0.0, -70.0), 28.0, alpha(if o.rare { WHITE } else { pal.accent }, fade), tscale);
            wtext(o.up.desc(), o.pos + vec2(0.0, 76.0), 18.0, alpha(WHITE, fade * 0.9), tscale);
            let lvl = w.player.up(o.up);
            if lvl > 0 {
                wtext(&format!("lv {} > {}", lvl, lvl + 1), o.pos + vec2(0.0, 98.0), 16.0, alpha(WHITE, fade * 0.8), tscale);
            } else {
                wtext("new", o.pos + vec2(0.0, 98.0), 16.0, alpha(WHITE, fade * 0.8), tscale);
            }
        }
    }

    /// Enemies: hovering wireframe solids (silhouette per kind) over an exact ground ring.
    fn draw_enemies(&self, w: &World, lv: f32) {
        let pal = &self.pal;
        for e in &w.enemies {
            let appear = (e.age * 6.0).min(1.0);
            let a = lv * appear;
            let col = match e.kind {
                EnemyKind::Chaser | EnemyKind::Mini | EnemyKind::Splitter => pal.secondary,
                EnemyKind::Darter => pal.accent,
                EnemyKind::Wisp => mixc(pal.accent, WHITE, 0.5),
                EnemyKind::Sprite => mixc(pal.accent, WHITE, 0.5 + 0.5 * (self.time * 9.0).sin()),
                EnemyKind::Turret => mixc(pal.danger, pal.accent, 0.35),
                EnemyKind::Bomber => if e.state > 0.0 { mixc(pal.danger, WHITE, 0.5 + 0.5 * (self.time * 22.0).sin()) } else { pal.accent },
                EnemyKind::Orbiter => pal.primary,
                // Gunner in the danger colour, Summoner in the secondary, Leaper in the accent
                EnemyKind::Elite => if e.state == ELITE_SUMMONER { mixc(pal.secondary, pal.danger, 0.3) } else if e.state == ELITE_LEAPER { mixc(pal.accent, pal.danger, 0.3) } else { pal.danger },
            };
            let colw = mixc(col, WHITE, e.flash);
            // ground: shadow + exact hitbox ring
            disc2(e.pos, e.r * 1.25, 16, alpha(pal.bg0, 0.55 * a), alpha(pal.bg0, 0.0));
            arc(e.pos, e.r, 0.0, TAU, 2.0, alpha(colw, 0.6 * a));
            let sc = e.r * 1.1 * (1.0 + 0.15 * e.flash) * (1.15 - 0.15 * appear);
            let z = e.r * 1.05 + 4.0 * (self.time * 3.0 + e.pos.x * 0.01).sin();
            let m = match e.kind {
                EnemyKind::Chaser | EnemyKind::Mini => Mat3::from_rotation_z(e.rot) * Mat3::from_rotation_x(e.age * 3.0),
                EnemyKind::Darter => Mat3::from_rotation_z(e.rot) * Mat3::from_rotation_x(e.age * 5.0),
                EnemyKind::Wisp => Mat3::from_rotation_z(e.rot) * Mat3::from_rotation_y(e.age * 4.0),
                EnemyKind::Sprite => Mat3::from_rotation_z(e.rot) * Mat3::from_rotation_x(e.age * 3.0),
                EnemyKind::Turret => Mat3::from_rotation_z(e.rot) * Mat3::from_rotation_x(0.35),
                EnemyKind::Bomber => Mat3::from_rotation_z(e.rot) * Mat3::from_rotation_x(e.age * 2.0),
                EnemyKind::Orbiter => Mat3::from_rotation_z(e.age * 1.2) * Mat3::from_rotation_x(0.6) * Mat3::from_rotation_y(e.age * 0.9),
                EnemyKind::Splitter => Mat3::from_rotation_z(e.age * 0.6) * Mat3::from_rotation_x(0.35),
                EnemyKind::Elite => Mat3::from_rotation_z(e.age * 0.5) * Mat3::from_rotation_x(e.age * 0.7),
            };
            let top = self.pj(e.pos, z - sc * 0.5);
            seg(e.pos, top, 1.5, alpha(colw, 0.45 * a), alpha(colw, 0.1 * a));
            let wdt = if e.kind == EnemyKind::Mini { 2.0 } else { 2.6 };
            // dark core so the wireframe reads over bright floor effects
            disc2(self.pj(e.pos, z), sc * 0.9, 14, alpha(pal.bg0, 0.45 * a), alpha(pal.bg0, 0.0));
            for ed in self.kinds[kind_index(e.kind)].iter() {
                self.edge3(e.pos, z, m * (ed[0] * sc), m * (ed[1] * sc), wdt, colw, a, sc);
            }
            match e.kind {
                EnemyKind::Orbiter => {
                    let rm = Mat3::from_rotation_z(-e.age * 2.0) * Mat3::from_rotation_x(1.1);
                    let gr = sc * 1.45;
                    let mut prev = rm * vec3(gr, 0.0, 0.0);
                    for s in 1..=28 {
                        let cur = rm * (dir(s as f32 * TAU / 28.0).extend(0.0) * gr);
                        self.edge3(e.pos, z, prev, cur, 1.5, colw, a * 0.8, gr);
                        prev = cur;
                    }
                }
                EnemyKind::Elite => {
                    let frac = (e.hp / e.max_hp).clamp(0.0, 1.0);
                    arc(e.pos, e.r * 1.6, -PI * 0.5, -PI * 0.5 + TAU * frac, 3.0, alpha(pal.danger, a));
                }
                EnemyKind::Chaser => {
                    let eye = self.pj(e.pos + dir(e.rot) * sc * 0.6, z);
                    disc(eye, 3.0, 8, alpha(WHITE, a));
                }
                EnemyKind::Turret => {
                    // a ring that fills through the bar: it fires when the ring closes
                    let f = w.bar_phase();
                    arc(e.pos, e.r + 9.0, 0.0, TAU, 1.0, alpha(WHITE, 0.2 * a));
                    arc(e.pos, e.r + 9.0, -PI * 0.5, -PI * 0.5 + TAU * f, 3.5, alpha(pal.danger, (0.5 + 0.5 * f) * a));
                    // its three lanes, brightening as the downbeat comes
                    for off in [-0.24f32, 0.0, 0.24] {
                        let d = dir(e.rot + off);
                        seg(e.pos + d * (e.r + 12.0), e.pos + d * (e.r + 70.0 + 120.0 * f), 2.0, alpha(pal.danger, 0.55 * f * f * a), alpha(pal.danger, 0.0));
                    }
                }
                EnemyKind::Bomber => {
                    if e.state > 0.0 {
                        // armed: a closing ring and the spokes its bullets will fly along
                        let beats = e.state.ceil();
                        let rr = e.r + 14.0 + 22.0 * (e.state - (beats - 1.0));
                        arc(e.pos, rr, 0.0, TAU, 3.0, alpha(pal.danger, a));
                        disc2(e.pos, 46.0, 16, alpha(pal.danger, 0.22 * a), alpha(pal.danger, 0.0));
                        for k in 0..10 {
                            let d = dir(e.rot + k as f32 * TAU / 10.0);
                            seg(e.pos + d * (e.r + 6.0), e.pos + d * 120.0, 1.5, alpha(pal.danger, 0.45 * a), alpha(pal.danger, 0.0));
                        }
                    } else {
                        arc(e.pos, e.r + 7.0, self.time * 3.0, self.time * 3.0 + PI, 1.5, alpha(pal.accent, 0.6 * a));
                    }
                }
                _ => {}
            }
        }
    }

    /// Bullets: on the z = 0 plane at exact size. Hot orbs = danger disc + white core + dark halo
    /// + trail; notes = white square outline; ticks = darts.
    fn draw_bullets(&self, w: &World, lv: f32) {
        let pal = &self.pal;
        let halo = mixc(pal.bg0, BLACK, 0.4);
        for b in &w.bullets {
            let a = b.fade.clamp(0.0, 1.0) * lv * (b.age * 10.0 + 0.3).min(1.0);
            let sp = b.vel.length();
            let d = if sp > 1e-3 { b.vel / sp } else { vec2(1.0, 0.0) };
            if b.soft {
                let rot = b.age * 4.0;
                let r = b.r * 1.25;
                disc(b.pos, b.r * 1.6, 12, alpha(halo, 0.5 * a));
                let p = [rot, rot + PI * 0.5, rot + PI, rot + PI * 1.5].map(|q| b.pos + dir(q) * r);
                quad4(p, [alpha(mixc(pal.bg0, WHITE, 0.2), 0.8 * a); 4]);
                for i in 0..4 {
                    line(p[i], p[(i + 1) % 4], 2.6, alpha(WHITE, a));
                }
                disc(b.pos, b.r * 0.32, 6, alpha(WHITE, a));
            } else if b.kind == B_TICK {
                let n = vec2(-d.y, d.x);
                seg(b.pos, b.pos - d * (b.r * 3.0 + sp * 0.05), b.r * 1.1, alpha(pal.accent, 0.55 * a), alpha(pal.accent, 0.0));
                let (tip, tail) = (b.pos + d * b.r * 2.2, b.pos - d * b.r * 2.2);
                geo(&[vtx(tip, alpha(pal.accent, a)), vtx(b.pos + n * b.r, alpha(pal.danger, a)), vtx(tail, alpha(pal.danger, a)), vtx(b.pos - n * b.r, alpha(pal.danger, a))], &[0, 1, 2, 0, 2, 3]);
            } else {
                seg(b.pos, b.pos - d * (b.r * 2.0 + sp * 0.06), b.r * 1.3, alpha(pal.danger, 0.45 * a), alpha(pal.danger, 0.0));
                disc(b.pos, b.r * 1.75, 14, alpha(halo, 0.55 * a));
                disc2(b.pos, b.r * 1.6, 14, alpha(pal.danger, 0.4 * a), alpha(pal.danger, 0.0));
                disc(b.pos, b.r, 14, alpha(pal.danger, a));
                disc(b.pos, b.r * 0.5, 8, alpha(WHITE, a));
            }
        }
    }

    fn draw_shots(&self, w: &World) {
        let pal = &self.pal;
        for s in &w.shots {
            let d = s.vel.normalize_or_zero();
            seg(s.pos - d * 30.0, s.pos, s.r * 1.6, alpha(pal.primary, 0.0), alpha(pal.primary, 0.8));
            line(s.pos - d * 10.0, s.pos, s.r * 0.9, WHITE);
        }
    }

    fn draw_player(&self, w: &World) {
        let pal = &self.pal;
        let p = &w.player;
        if w.dead {
            return;
        }
        let blink = if p.invuln > 0.0 && p.dash_t <= 0.0 { 0.45 + 0.55 * ((self.time * 30.0).sin() * 0.5 + 0.5) } else { 1.0 };
        // Resonance: rings pulse out from the player on every beat
        if w.res_t > 0.0 {
            let fade = (w.res_t / 0.6).min(1.0);
            for k in 0..2 {
                let ph = (1.0 - w.beat_pulse + k as f32 * 0.5).fract();
                arc(p.pos, 26.0 + 70.0 * ph, 0.0, TAU, 2.5, alpha(pal.accent, 0.7 * (1.0 - ph) * fade));
            }
            poly_lines(p.pos, 6, 30.0, self.time * 2.0, 2.0, alpha(WHITE, 0.6 * fade));
        }
        // ORBIT blades: turning crescents with a faint ring showing their path
        if p.blades() > 0 {
            arc(p.pos, BLADE_ORBIT_R, 0.0, TAU, 1.0, alpha(pal.accent, 0.12));
            for k in 0..p.blades() {
                let bp = p.blade_pos(k);
                let a = self.time * 14.0 + k as f32;
                poly_lines(bp, 3, 13.0, a, 2.5, alpha(WHITE, 0.95));
                poly_lines(bp, 3, 18.0, -a * 0.7, 1.5, alpha(pal.accent, 0.7));
                let tail = p.pos + (bp - p.pos).rotate(dir(0.35));
                seg(bp, tail, 4.0, alpha(pal.accent, 0.5), alpha(pal.accent, 0.0));
            }
        }
        for k in 0..p.drones() {
            let dp = p.drone_pos(k);
            diamond_fill(dp, 5.5, alpha(pal.accent, 0.95));
            poly_lines(dp, 4, 11.0, -self.time * 2.0, 1.5, alpha(pal.accent, 0.6));
        }
        disc(p.pos, 24.0, 18, alpha(pal.bg0, 0.45));
        if p.shield {
            arc(p.pos, 22.0, 0.0, TAU, 2.5, alpha(pal.primary, 0.85));
        }
        if p.dash_cd > 0.0 {
            let frac = 1.0 - (p.dash_cd / p.dash_cd_max.max(0.01)).clamp(0.0, 1.0);
            arc(p.pos, 17.0, -PI * 0.5, -PI * 0.5 + TAU * frac, 1.5, alpha(WHITE, 0.5));
        } else {
            arc(p.pos, 17.0, 0.0, TAU, 1.8, alpha(pal.primary, 0.85));
        }
        let r = PLAYER_R * (1.0 + 0.12 * w.beat_pulse);
        disc2(p.pos, r * 2.4, 18, alpha(pal.primary, 0.25 * blink), alpha(pal.primary, 0.0));
        disc(p.pos, r, 18, alpha(WHITE, blink));
        disc(p.pos, 3.5, 10, alpha(pal.bg0, blink));
        let d = p.aim_dir;
        let n = vec2(-d.y, d.x);
        let tip = p.pos + d * (27.0 + 5.0 * p.muzzle);
        let base = p.pos + d * 18.0;
        let c = alpha(pal.primary, blink);
        geo(&[vtx(tip, c), vtx(base + n * 5.0, c), vtx(base - n * 5.0, c)], &[0, 1, 2]);
    }

    /// Particles as line sparks / streaks (additive).
    fn draw_particles(&self, w: &World) {
        let pal = &self.pal;
        for p in &w.parts {
            let k = (p.life / p.max).clamp(0.0, 1.0);
            let col = slot(pal, p.col);
            let sp = p.vel.length();
            let d = if sp > 1.0 { p.vel / sp } else { dir(p.rot) };
            let len = (sp * 0.035).max(p.size * 0.8) * (0.5 + 0.5 * k);
            seg(p.pos, p.pos - d * len, (p.size * 0.32).max(1.4), alpha(mixc(col, WHITE, 0.3 * k), k), alpha(col, 0.0));
        }
    }

    fn draw_texts(&self, w: &World, tscale: f32) {
        for t in &w.texts {
            let k = (t.life / t.max).clamp(0.0, 1.0);
            let a = (k * 3.0).min(1.0);
            wtext(&t.text, t.pos, t.size, alpha(slot(&self.pal, t.col), a), tscale);
        }
    }

    // -----------------------------------------------------------------------------------------
    // Rally (LAN paddle duel): drawn where the combat entities normally go
    // -----------------------------------------------------------------------------------------

    /// End lines as ridgelines that rise toward each incoming orb's landing point, landing
    /// telegraphs, avatars with swing arcs and mini spectra, waveform orb trails, hit flashes.
    fn draw_rally_world(&self, r: &Rally, w: &World, me: usize, tscale: f32) {
        let pal = self.pal;
        let t = r.t;
        let cols = [rally_col(r.players[0].ch), rally_col(r.players[1].ch)];
        let flying = |i: usize| r.orbs.iter().filter(move |o| o.state == OrbState::Flying && o.target() == i);

        // ---- field: zone boundaries + end-line ridges (additive) ----
        gl_use_material(&self.add_mat);
        for (i, &col) in cols.iter().enumerate() {
            let x = rally::end_x(i) + rally::fwd(i) * ZONE_W;
            let n = 18;
            for k in 0..n {
                let y0 = -HALF_H + (k as f32 + 0.25) * 2.0 * HALF_H / n as f32;
                line(vec2(x, y0), vec2(x, y0 + HALF_H / n as f32), 1.4, alpha(col, 0.16));
            }
            let ex = rally::end_x(i);
            let heat = flying(i).map(|o| o.progress(t)).fold(0.0f32, f32::max);
            let mut prev = Vec2::ZERO;
            for s in 0..=RALLY_RIDGE_PTS {
                let f = s as f32 / RALLY_RIDGE_PTS as f32;
                let y = -HALF_H + 2.0 * HALF_H * f;
                let mut z = band(&self.spec_s, (f - 0.5).abs() * 2.0) * RALLY_RIDGE_SPEC * (1.0 + 0.6 * w.hit[0]);
                for o in flying(i) {
                    let k = o.progress(t);
                    let g = (y - o.land) / RALLY_RIDGE_SIG;
                    z += RALLY_RIDGE_H * k * k * (-g * g).exp();
                }
                let goal = y.abs() <= r.goal_half;
                let col = if goal { mixc(col, WHITE, 0.2 + 0.5 * heat * heat) } else { pal.grid };
                let p = self.pj(vec2(ex, y), z);
                if s > 0 {
                    line(prev, p, if goal { 3.0 } else { 1.5 }, alpha(col, if goal { 0.9 } else { 0.45 }));
                }
                if s % 2 == 0 && z > 3.0 {
                    seg(vec2(ex, y), p, 1.0, alpha(col, 0.05), alpha(col, (z / RALLY_RIDGE_H).min(1.0) * 0.45));
                }
                prev = p;
            }
            for sy in [-1.0, 1.0] {
                let gp = vec2(ex, sy * r.goal_half);
                diamond(gp, 9.0, 2.0, alpha(col, 0.9));
                line(gp, gp + vec2(rally::fwd(i) * 26.0, 0.0), 2.0, alpha(col, 0.6));
            }
        }
        gl_use_default_material();

        // ---- landing telegraphs on the end line ----
        for o in r.orbs.iter().filter(|o| o.state == OrbState::Flying) {
            let k = o.progress(t);
            let lp = vec2(o.xe, o.land);
            let live = o.land.abs() <= r.goal_half;
            let col = if live { mixc(cols[o.owner as usize & 1], WHITE, 0.5 * k) } else { alpha(pal.grid, 0.8) };
            let sz = RALLY_MARK_R * (1.6 - 0.6 * k);
            diamond(lp, sz, 2.5, alpha(col, 0.35 + 0.6 * k));
            if live {
                diamond_fill(lp, sz * 0.4 * k, alpha(col, 0.9));
            }
            let inward = vec2(-o.xe.signum(), 0.0);
            line(lp + inward * sz, lp + inward * (sz + 40.0 * k), 2.0, alpha(col, 0.5 * k));
        }

        // ---- strike lines: pulse on every beat; incoming orbs cross them exactly on a beat ----
        let bp = w.beat_pulse.clamp(0.0, 1.0);
        gl_use_material(&self.add_mat);
        for (i, &col) in cols.iter().enumerate() {
            let x = rally::strike_x(i);
            let g = r.goal_half;
            let lc = mixc(col, WHITE, 0.35 * bp);
            line(vec2(x, -HALF_H), vec2(x, -g), 1.2, alpha(lc, 0.18 + 0.2 * bp));
            line(vec2(x, g), vec2(x, HALF_H), 1.2, alpha(lc, 0.18 + 0.2 * bp));
            line(vec2(x, -g), vec2(x, g), RALLY_STRIKE_W * (1.0 + bp), alpha(lc, 0.10 * (1.0 + bp)));
            line(vec2(x, -g), vec2(x, g), 2.0 + 1.5 * bp, alpha(lc, 0.45 + 0.45 * bp));
        }
        gl_use_default_material();

        // ---- approach rings: close on the strike point exactly on the strike beat ----
        for o in r.orbs.iter().filter(|o| o.state == OrbState::Flying && !(o.extra && o.target() != me)) {
            let lead = RALLY_RING_BEATS * o.blen.max(0.1);
            let dtb = o.tb - t;
            if dtb > lead || dtb < -RALLY_RING_HOLD {
                continue;
            }
            let mine = o.target() == me;
            let sp = o.strike_point();
            let col = mixc(cols[o.target()], WHITE, if mine { 0.5 } else { 0.1 });
            let base = if mine { 1.0 } else { 0.45 };
            if dtb >= 0.0 {
                let k = dtb / lead; // 1 -> 0 as the beat arrives
                arc(sp, ORB_R + 6.0, 0.0, TAU, 2.0, alpha(col, base * 0.7));
                arc(sp, ORB_R + 6.0 + RALLY_RING_R * k, 0.0, TAU, if mine { 3.0 } else { 1.6 }, alpha(col, base * (1.0 - 0.6 * k)));
            } else {
                let k = -dtb / RALLY_RING_HOLD;
                disc2(sp, ORB_R + 18.0, 18, alpha(WHITE, base * 0.5 * (1.0 - k)), alpha(col, 0.0));
            }
        }

        // ---- avatars: reach ghost, live swing arc, mini spectrum, body ----
        for (i, p) in r.players.iter().enumerate() {
            let c = cols[i];
            let aa = p.aim.y.atan2(p.aim.x);
            disc(p.pos, 34.0, 18, alpha(pal.bg0, 0.5));
            arc(p.pos, SWING_R, aa - SWING_HALF, aa + SWING_HALF, 1.2, alpha(c, 0.16));
            let age = p.swing_age(t);
            let life = SWING_ACTIVE + RALLY_SWING_FADE;
            if (0.0..life).contains(&age) {
                let k = 1.0 - age / life;
                let sc = if p.perfect { mixc(c, WHITE, 0.7) } else { c };
                arc(p.pos, SWING_R - 12.0, aa - SWING_HALF, aa + SWING_HALF, 24.0, alpha(sc, 0.35 * k));
                arc(p.pos, SWING_R, aa - SWING_HALF, aa + SWING_HALF, 3.5, alpha(sc, k));
            }
            gl_use_material(&self.add_mat);
            for b in 0..RALLY_SPEC_BARS {
                let f = b as f32 / RALLY_SPEC_BARS as f32;
                let v = band(&self.spec_s, (f - 0.5).abs() * 2.0).min(1.2);
                let d = dir(aa + PI + f * TAU);
                let a0 = p.pos + d * RALLY_SPEC_R;
                line(a0, a0 + d * (2.0 + v * RALLY_SPEC_LEN), 2.4, alpha(c, 0.8));
            }
            gl_use_default_material();
            let rr = AVATAR_R * (1.0 + 0.12 * w.beat_pulse);
            disc2(p.pos, rr * 2.4, 18, alpha(c, 0.3), alpha(c, 0.0));
            disc(p.pos, rr, 18, c);
            disc(p.pos, rr * 0.42, 10, alpha(WHITE, 0.95));
            arc(p.pos, rr + 13.0 + 8.0 * (1.0 - bp), 0.0, TAU, 1.5, alpha(c, 0.45 * bp));
            if p.cd > 0.0 {
                let frac = 1.0 - (p.cd / rally::SWING_CD).clamp(0.0, 1.0);
                arc(p.pos, rr + 5.0, -PI * 0.5, -PI * 0.5 + TAU * frac, 1.5, alpha(WHITE, 0.5));
            }
            if i == me {
                arc(p.pos, rr + 9.0, 0.0, TAU, 1.5, alpha(WHITE, 0.55));
            }
            let n = vec2(-p.aim.y, p.aim.x);
            let (tip, base) = (p.pos + p.aim * (rr + 16.0), p.pos + p.aim * (rr + 7.0));
            geo(&[vtx(tip, c), vtx(base + n * 5.0, c), vtx(base - n * 5.0, c)], &[0, 1, 2]);
        }

        // ---- orbs: waveform trail, dark backing, white core (always the brightest thing) ----
        let gain = 1.0 / self.scope_peak.max(0.12);
        for o in r.orbs.iter().filter(|o| o.state != OrbState::Dead) {
            let c = cols[o.owner as usize & 1];
            if o.state == OrbState::Flying {
                let laser = o.power == Power::Arc;
                let n = if laser { RALLY_LASER_N } else { RALLY_TRAIL_N };
                let mut prev_raw = o.pos;
                let mut prev = o.pos;
                for k in 1..=n {
                    let tk = t - k as f32 * RALLY_TRAIL_DT;
                    if tk < o.t0 {
                        break;
                    }
                    let q = o.eval(tk);
                    let d = (prev_raw - q).normalize_or_zero();
                    let fall = 1.0 - k as f32 / n as f32;
                    let v = if self.scope_ok {
                        (self.scope[(k * SCOPE_N / n).min(SCOPE_N - 1)] * gain).clamp(-1.5, 1.5)
                    } else {
                        0.35 * (k as f32 * 0.9 - self.clock * 18.0).sin()
                    };
                    let pq = q + vec2(-d.y, d.x) * v * RALLY_TRAIL_AMP * fall.sqrt();
                    if laser {
                        line(prev_raw, q, 9.0 * fall, alpha(c, 0.18 * fall));
                        line(prev_raw, q, 2.5, alpha(mixc(c, WHITE, 0.6), 0.85 * fall));
                    }
                    line(prev, pq, 1.0 + 3.0 * fall, alpha(mixc(c, WHITE, 0.25), 0.9 * fall));
                    prev_raw = q;
                    prev = pq;
                }
            }
            let sc = if o.extra { 0.8 } else { 1.0 };
            let orr = ORB_R * sc * (1.0 + 0.1 * w.beat_pulse);
            disc(o.pos, orr * 2.6, 20, alpha(BLACK, 0.55));
            disc2(o.pos, orr * 3.4, 20, alpha(c, 0.45), alpha(c, 0.0));
            disc(o.pos, orr, 20, WHITE);
            arc(o.pos, orr + 5.0 + 4.0 * w.beat_pulse, 0.0, TAU, if o.power == Power::Sub { 4.5 } else { 2.0 }, c);
            if o.power == Power::Tick {
                poly_lines(o.pos, 4, orr + 11.0, self.time * 6.0, 1.5, alpha(c, 0.8));
            }
            if o.state == OrbState::Held {
                let k = ((t - o.t0) / (o.t1 - o.t0).max(0.01)).clamp(0.0, 1.0);
                arc(o.pos, orr + 14.0, -PI * 0.5, -PI * 0.5 + TAU * (1.0 - k), 2.5, alpha(c, 0.9));
            }
        }

        // ---- one-shot flashes ----
        gl_use_material(&self.add_mat);
        for e in r.events() {
            let age = t - e.t;
            let c = cols[e.who as usize & 1];
            let life = match e.kind {
                FxKind::Slam | FxKind::Score => RALLY_SLAM_LIFE,
                _ => RALLY_FLASH_LIFE,
            };
            if age < 0.0 || age >= life {
                continue;
            }
            let k = 1.0 - age / life;
            match e.kind {
                FxKind::Hit => arc(e.pos, 16.0 + age * 260.0, 0.0, TAU, 2.0, alpha(c, 0.6 * k)),
                FxKind::Bounce => arc(e.pos, 14.0 + age * 200.0, 0.0, TAU, 1.5, alpha(pal.grid, k)),
                FxKind::Perfect => {
                    disc2(e.pos, 40.0 + 40.0 * (1.0 - k), 20, alpha(WHITE, 0.5 * k), alpha(c, 0.0));
                    arc(e.pos, 20.0 + age * 520.0, 0.0, TAU, 3.0 * k + 1.0, alpha(WHITE, k));
                }
                FxKind::Slam => {
                    arc(e.pos, age * RALLY_SLAM_SPEED, 0.0, TAU, 8.0 * k, alpha(c, 0.8 * k));
                    arc(e.pos, age * RALLY_SLAM_SPEED * 0.6, 0.0, TAU, 3.0 * k, alpha(WHITE, 0.5 * k));
                }
                FxKind::Split => {
                    arc(e.pos, 10.0 + age * 300.0, -0.6, 0.6, 2.5, alpha(c, k));
                    arc(e.pos, 10.0 + age * 300.0, PI - 0.6, PI + 0.6, 2.5, alpha(c, k));
                }
                FxKind::Score => {
                    line(vec2(e.pos.x, -r.goal_half), vec2(e.pos.x, r.goal_half), 12.0 * k, alpha(c, 0.7 * k));
                    arc(e.pos, 30.0 + age * 900.0, 0.0, TAU, 6.0 * k, alpha(c, k));
                }
                FxKind::Serve => arc(e.pos, ORB_R + 90.0 * k, 0.0, TAU, 2.0, alpha(c, 1.0 - k)),
            }
        }
        gl_use_default_material();
        for e in r.events() {
            let age = t - e.t;
            let timed = matches!(e.kind, FxKind::Perfect | FxKind::Hit) && e.err != rally::NO_ERR;
            if timed && (0.0..RALLY_TEXT_LIFE).contains(&age) {
                let k = 1.0 - age / RALLY_TEXT_LIFE;
                let pos = e.pos - vec2(0.0, 44.0 + 30.0 * (1.0 - k));
                let mine = e.who as usize == me;
                let size = if mine { 24.0 } else { 17.0 };
                if e.kind == FxKind::Perfect {
                    wtext("PERFECT", pos, size, alpha(mixc(WHITE, hex(RALLY_PERFECT_COL), 0.5), k), tscale);
                } else {
                    let (word, col) = if e.err < 0 { ("EARLY", RALLY_EARLY_COL) } else { ("LATE", RALLY_LATE_COL) };
                    wtext(&format!("{word} {}ms", e.err.unsigned_abs()), pos, size, alpha(hex(col), k), tscale);
                }
            }
        }
    }

    /// Score HUD (screen space, vector font): scores in each player's colour, character names,
    /// a YOU tag, and a serve prompt for the local player.
    fn draw_rally_hud(&self, r: &Rally, me: usize) {
        let (sw, sh) = (screen_width(), screen_height());
        let u = sh / 1080.0;
        let cols = [rally_col(r.players[0].ch), rally_col(r.players[1].ch)];
        let (cx, y) = (sw * 0.5, 84.0 * u);
        let s = r.score();
        vtext(&s[0].to_string(), cx - 44.0 * u, y, 64.0 * u, cols[0], 1);
        vtext("-", cx, y - 8.0 * u, 36.0 * u, alpha(WHITE, 0.6), 0);
        // rally length, once it is long enough to have sped up
        if r.heat >= 4 {
            let hot = (r.heat / 4).min(6) as f32 / 6.0;
            let pulse = 0.75 + 0.25 * (self.time * (6.0 + 8.0 * hot)).sin();
            vtext(&format!("RALLY {}", r.heat), cx, y + 64.0 * u, 20.0 * u, alpha(mixc(self.pal.accent, self.pal.danger, hot), pulse), 0);
        }
        vtext(&s[1].to_string(), cx + 44.0 * u, y, 64.0 * u, cols[1], -1);
        for i in 0..2 {
            let x = cx + rally::fwd(i) * -44.0 * u;
            let al = if i == 0 { 1 } else { -1 };
            vtext(r.players[i].ch.name(), x, y + 34.0 * u, 18.0 * u, alpha(cols[i], 0.85), al);
            if i == me {
                vtext("YOU", x, y + 58.0 * u, 15.0 * u, alpha(WHITE, 0.8), al);
            }
        }
        if r.goal_half > GOAL_HALF + 20.0 {
            vtext("GOALS WIDEN", cx, y + 92.0 * u, 18.0 * u, alpha(self.pal.accent, 0.9), 0);
        }
        if r.orbs.iter().any(|o| o.state == OrbState::Held && o.owner as usize == me) {
            let blink = 0.6 + 0.4 * (self.time * 6.0).sin();
            vtext("YOUR SERVE - SWING ON THE BEAT", cx, sh - 60.0 * u, 24.0 * u, alpha(WHITE, blink), 0);
        }
    }

    // -----------------------------------------------------------------------------------------
    // HUD (screen space)
    // -----------------------------------------------------------------------------------------

    pub fn draw_hud(&self, w: &World, info: &HudInfo, idle: f32) {
        let pal = &self.pal;
        let (sw, sh) = (screen_width(), screen_height());
        let u = sh / 1080.0;
        let m = 34.0 * u;
        let hud_a = if info.show_combat { 1.0 } else { 0.0 };
        // Purgatory washes the whole scene in the accent colour, strongest at the edges.
        if w.purg > 0.01 && info.show_combat {
            let c = alpha(pal.accent, 0.07 * w.purg);
            draw_rectangle(0.0, 0.0, sw, sh, c);
            let e = alpha(pal.accent, 0.22 * w.purg);
            let z = alpha(pal.accent, 0.0);
            let band = sh * 0.16;
            quad4([vec2(0.0, 0.0), vec2(sw, 0.0), vec2(sw, band), vec2(0.0, band)], [e, e, z, z]);
            quad4([vec2(0.0, sh - band), vec2(sw, sh - band), vec2(sw, sh), vec2(0.0, sh)], [z, z, e, e]);
        }

        if info.show_combat {
            // Score + multiplier + combo
            vtext(&format!("{:08}", w.score), m, m + 40.0 * u, 42.0 * u, WHITE, -1);
            let mult = w.mult();
            let mc = if mult >= 4.0 { pal.accent } else if mult >= 2.0 { pal.primary } else { alpha(WHITE, 0.7) };
            vtext(&format!("X{:.1}", mult), m, m + 82.0 * u, 26.0 * u, mc, -1);
            let cf = (w.combo % 8.0) / 8.0;
            let (cx, cy, cw) = (m + 100.0 * u, m + 72.0 * u, 150.0 * u);
            line(vec2(cx, cy), vec2(cx + cw, cy), 1.0 * u, alpha(WHITE, 0.25));
            line(vec2(cx, cy), vec2(cx + cw * cf, cy), 3.0 * u, mc);
            line(vec2(cx + cw, cy - 5.0 * u), vec2(cx + cw, cy + 5.0 * u), 1.0 * u, alpha(WHITE, 0.35));
            // Resonance: the meter, or the time left while it rings
            let ry = cy + 20.0 * u;
            let ringing = w.res_t > 0.0;
            let rf = if ringing { w.res_t / RESONANCE_SECS } else { w.resonance.clamp(0.0, 1.0) };
            let rc = if ringing { mixc(pal.accent, WHITE, 0.5 + 0.5 * (self.time * 12.0).sin()) } else { alpha(pal.secondary, 0.9) };
            line(vec2(cx, ry), vec2(cx + cw, ry), 1.0 * u, alpha(WHITE, 0.2));
            line(vec2(cx, ry), vec2(cx + cw * rf, ry), if ringing { 4.0 } else { 2.5 } * u, rc);
            vtext(if ringing { "RESONANCE" } else { "RES" }, cx + cw + 10.0 * u, ry + 6.0 * u, 13.0 * u, alpha(rc, 0.95), -1);

            // HP diamonds
            let y = sh - m - 14.0 * u;
            for i in 0..w.player.max_hp {
                let c = vec2(m + 14.0 * u + i as f32 * 34.0 * u, y);
                if i < w.player.hp {
                    diamond(c, 13.0 * u, 2.5 * u, pal.primary);
                    diamond_fill(c, 6.0 * u, alpha(WHITE, 0.9));
                } else {
                    diamond(c, 13.0 * u, 1.5 * u, alpha(pal.primary, 0.35));
                }
            }
            if w.player.shield {
                let c = vec2(m + 14.0 * u + w.player.max_hp as f32 * 34.0 * u + 6.0 * u, y);
                diamond(c, 15.0 * u, 2.5 * u, pal.accent);
                diamond(c, 9.0 * u, 1.5 * u, alpha(pal.accent, 0.6));
            }
            // XP line with ticks
            let xf = (w.player.xp as f32 / w.player.xp_next.max(1) as f32).clamp(0.0, 1.0);
            let (xx, xy, xw) = (m, y - 30.0 * u, 260.0 * u);
            line(vec2(xx, xy), vec2(xx + xw, xy), 1.0 * u, alpha(WHITE, 0.25));
            for k in 1..10 {
                let tx = xx + xw * k as f32 / 10.0;
                line(vec2(tx, xy - 3.0 * u), vec2(tx, xy + 3.0 * u), 1.0 * u, alpha(WHITE, 0.2));
            }
            line(vec2(xx, xy), vec2(xx + xw * xf, xy), 3.0 * u, pal.accent);
            vtext(&format!("LV {}", w.player.level + 1), xx + xw + 12.0 * u, xy + 7.0 * u, 18.0 * u, alpha(pal.accent, 0.9), -1);

            // Upgrades
            let mut uy = sh - m;
            for up in UpgradeId::ALL.iter().rev() {
                let lvl = w.player.up(*up);
                if lvl > 0 {
                    text_shadow(&format!("{} {}", up.name(), roman(lvl)), sw - m, uy, 20.0 * u, alpha(pal.accent, 0.85), 1);
                    uy -= 22.0 * u;
                }
            }

            // Run modifiers of this depth, above the upgrades
            for (k, md) in MODS.iter().enumerate() {
                if w.mods >> k & 1 == 1 {
                    text_shadow(md.0, sw - m, uy, 18.0 * u, alpha(pal.danger, 0.9), 1);
                    uy -= 20.0 * u;
                }
            }

            // Heart integrity (DODGE has no Heart to break: it shows its own line instead)
            if w.pacifist {
                vtext("DODGE", sw * 0.5, m + 60.0 * u, 20.0 * u, alpha(pal.accent, 0.9), 0);
                text_shadow(&format!("{} grazes", w.stats.grazes), sw * 0.5, m + 84.0 * u, 20.0 * u, alpha(WHITE, 0.8), 0);
            }
            if !w.pacifist {
            let bw = 520.0 * u;
            let bx = sw * 0.5 - bw * 0.5;
            let by = m + 54.0 * u;
            let frac = (w.heart.hp / w.heart.max_hp.max(1.0)).clamp(0.0, 1.0);
            line(vec2(bx, by), vec2(bx + bw, by), 1.0 * u, alpha(WHITE, 0.22));
            line(vec2(bx, by - 4.0 * u), vec2(bx, by + 4.0 * u), 1.0 * u, alpha(WHITE, 0.35));
            line(vec2(bx + bw, by - 4.0 * u), vec2(bx + bw, by + 4.0 * u), 1.0 * u, alpha(WHITE, 0.35));
            if w.heart.broken {
                vtext("HEART BROKEN", sw * 0.5, by + 26.0 * u, 18.0 * u, pal.accent, 0);
            } else {
                let hc = mixc(pal.danger, WHITE, w.heart.flash);
                line(vec2(bx, by), vec2(bx + bw * frac, by), 4.0 * u, hc);
                diamond_fill(vec2(bx + bw * frac, by), 5.0 * u, WHITE);
                vtext("HEART", bx - 12.0 * u, by + 7.0 * u, 16.0 * u, alpha(pal.danger, 0.9), 1);
                // One notch per phase still ahead: the bar stops at the next one until the song moves on.
                for (i, ph) in w.phases.iter().enumerate() {
                    if i + 1 < w.phases.len() && ph.floor > 0.0 {
                        let fx = bx + bw * (ph.floor / w.heart.max_hp.max(1.0)).clamp(0.0, 1.0);
                        let next = i == w.phase_i;
                        line(vec2(fx, by - 6.0 * u), vec2(fx, by + 6.0 * u), if next { 2.5 } else { 1.0 } * u, alpha(if next { pal.accent } else { WHITE }, if next { 0.95 } else { 0.4 }));
                    }
                }
                if w.rage > 0 {
                    vtext(&format!("RAGE {}", roman(w.rage as u8)), bx + bw + 12.0 * u, by + 27.0 * u, 15.0 * u, alpha(pal.danger, 0.95), -1);
                }
                if w.purgatory {
                    let pulse = 0.75 + 0.25 * (self.time * 5.0).sin();
                    let left = w.phase_left().min((w.purg_until - w.t).max(0.0));
                    vtext(&format!("PURGATORY  {:.0}", left.ceil()), sw * 0.5, by + 28.0 * u, 20.0 * u, alpha(pal.accent, pulse), 0);
                } else if w.heart.form != HeartForm::Core {
                    vtext(w.heart.form.name(), bx + bw + 12.0 * u, by + 7.0 * u, 16.0 * u, alpha(pal.accent, 0.9), -1);
                    if w.form_banner > 0.0 {
                        let a = (w.form_banner * 5.0).min(1.0);
                        text_shadow(w.heart.form.hint(), sw * 0.5, by + 26.0 * u, 20.0 * u, alpha(WHITE, 0.9 * a), 0);
                    }
                }
            }

            }

            // Section label
            let label = section_label(w.section_kind);
            vtext(label, sw - m, m + 26.0 * u, 26.0 * u, pal.secondary, 1);
            if info.bpm > 1.0 {
                let depth = if w.song_idx > 0 { format!("DEPTH {}   ", w.song_idx + 1) } else { String::new() };
                text_shadow(&format!("{depth}{:.0} BPM", info.bpm), sw - m, m + 52.0 * u, 20.0 * u, alpha(WHITE, 0.6), 1);
            }
            // Musical layers driving attacks right now: lit = armed.
            if w.armed != 0 {
                const NAMES: [&str; 6] = ["BEAT", "BASS", "SNARE", "HATS", "LEAD", "CHIME"];
                let mut x = sw - m;
                for k in (0..NAMES.len()).rev() {
                    let g = w.layer_glow[k];
                    let col = mixc(alpha(WHITE, 0.22), pal.accent, g);
                    text_shadow(NAMES[k], x, m + 78.0 * u, 16.0 * u, col, 1);
                    let tw = text_w(NAMES[k], 16.0 * u);
                    if g > 0.05 {
                        line(vec2(x - tw, m + 84.0 * u), vec2(x, m + 84.0 * u), 2.0 * u, alpha(pal.accent, g));
                    }
                    x -= tw + 14.0 * u;
                }
            }
            if w.section_banner > 0.0 && idle < 0.5 {
                let k = (w.section_banner / 2.4).clamp(0.0, 1.0);
                let a = (k * 4.0).min(1.0) * ((1.0 - k) * 6.0).min(1.0);
                let s = spaced(label);
                let tw = vtext_w(&s, 44.0 * u);
                let y = sh * 0.2;
                vtext(&s, sw * 0.5, y, 44.0 * u, alpha(pal.secondary, a), 0);
                let l = 120.0 * u * ease(a);
                for sx in [-1.0f32, 1.0] {
                    let x0 = sw * 0.5 + sx * (tw * 0.5 + 24.0 * u);
                    seg(vec2(x0, y - 16.0 * u), vec2(x0 + sx * l, y - 16.0 * u), 2.0 * u, alpha(pal.secondary, a), alpha(pal.secondary, 0.0));
                }
            }
        }

        // Title + song overview heartline (shown in drift mode too)
        if !info.title.is_empty() {
            let ta = 0.55 + 0.45 * hud_a;
            text_shadow(&crate::ui::fit(&info.title, 22.0 * u, 760.0 * u), sw * 0.5, m + 8.0 * u, 22.0 * u, alpha(WHITE, 0.85 * ta), 0);
            if info.progress > 0.0 || info.show_combat {
                let bw = 520.0 * u;
                let bx = sw * 0.5 - bw * 0.5;
                let by = m + 36.0 * u;
                self.draw_overview(info, bx, by, bw, 18.0 * u, u, ta);
                if info.time_left > 0.0 {
                    let tl = info.time_left.max(0.0) as u32;
                    text_shadow(&format!("{}:{:02}", tl / 60, tl % 60), bx + bw + 12.0 * u, by, 20.0 * u, alpha(WHITE, 0.7), -1);
                }
                if !info.track_no.is_empty() {
                    text_shadow(&info.track_no, bx - 12.0 * u, by, 20.0 * u, alpha(WHITE, 0.7), 1);
                }
            }
        }
        if !info.banner.is_empty() && info.banner_a > 0.0 {
            let s = spaced(&info.banner);
            let size = 60.0 * u;
            let bw = vtext_w(&s, size) + 90.0 * u;
            let (x0, y0, h) = (sw * 0.5 - bw * 0.5, sh * 0.13, 92.0 * u);
            draw_rectangle(x0, y0, bw, h, Color::new(0.0, 0.0, 0.06, 0.5 * info.banner_a));
            line(vec2(x0, y0), vec2(x0 + bw, y0), 1.5 * u, alpha(pal.accent, 0.7 * info.banner_a));
            line(vec2(x0, y0 + h), vec2(x0 + bw, y0 + h), 1.5 * u, alpha(pal.accent, 0.7 * info.banner_a));
            brackets(x0 - 8.0 * u, y0 - 8.0 * u, bw + 16.0 * u, h + 16.0 * u, 18.0 * u, 2.0 * u, alpha(pal.secondary, info.banner_a));
            vtext(&s, sw * 0.5, y0 + 68.0 * u, size, alpha(pal.accent, info.banner_a), 0);
        }
        if !info.mode.is_empty() {
            text_shadow(&info.mode, sw * 0.5, sh - m - 30.0 * u, 20.0 * u, alpha(pal.primary, 0.85), 0);
        }
        if !info.status.is_empty() {
            let pulse = 0.65 + 0.35 * (self.time * 3.0).sin();
            text_shadow(&info.status, sw * 0.5, sh - m, 24.0 * u, alpha(WHITE, pulse), 0);
        }
    }

    /// Song-overview heartline: the whole song's intensity as a trace, section ticks, playhead.
    #[allow(clippy::too_many_arguments)]
    fn draw_overview(&self, info: &HudInfo, bx: f32, by: f32, bw: f32, bh: f32, u: f32, ta: f32) {
        let pal = &self.pal;
        let prog = info.progress.clamp(0.0, 1.0);
        let px = bx + bw * prog;
        match &info.timeline {
            Some(tl) if tl.curve.len() > 1 => {
                for &(f, kind) in &tl.marks {
                    let x = bx + bw * f.clamp(0.0, 1.0);
                    line(vec2(x, by + 3.0 * u), vec2(x, by - bh - 3.0 * u), 1.0 * u, alpha(kind_col(pal, kind), 0.55 * ta));
                }
                let n = tl.curve.len();
                let mut prev = Vec2::ZERO;
                let mut head_y = by;
                for (i, &v) in tl.curve.iter().enumerate() {
                    let f = i as f32 / (n - 1) as f32;
                    let p = vec2(bx + bw * f, by - v.clamp(0.0, 1.0) * bh);
                    if i > 0 {
                        let past = p.x <= px;
                        let c = if past { alpha(pal.accent, 0.95 * ta) } else { alpha(WHITE, 0.3 * ta) };
                        line(prev, p, if past { 2.0 * u } else { 1.2 * u }, c);
                        if prev.x <= px && p.x >= px {
                            let t = (px - prev.x) / (p.x - prev.x).max(1e-3);
                            head_y = prev.y + (p.y - prev.y) * t;
                        }
                    }
                    prev = p;
                }
                line(vec2(px, by + 4.0 * u), vec2(px, by - bh - 4.0 * u), 1.5 * u, alpha(WHITE, 0.8 * ta));
                disc2(vec2(px, head_y), 9.0 * u, 12, alpha(pal.accent, 0.5 * ta), alpha(pal.accent, 0.0));
                diamond_fill(vec2(px, head_y), 4.0 * u, alpha(WHITE, ta));
            }
            _ => {
                line(vec2(bx, by), vec2(bx + bw, by), 1.0 * u, alpha(WHITE, 0.2 * ta));
                line(vec2(bx, by), vec2(px, by), 2.5 * u, alpha(pal.accent, ta));
                diamond_fill(vec2(px, by), 4.0 * u, alpha(WHITE, ta));
            }
        }
    }

    /// Small live heartline trace in screen space (last `n` samples, newest at the right).
    #[allow(clippy::too_many_arguments)]
    fn ecg_trace(&self, x: f32, y: f32, w: f32, amp: f32, n: usize, th: f32, col: Color) {
        let n = n.clamp(2, ECG_N);
        let mut prev = Vec2::ZERO;
        for k in 0..n {
            let v = self.ecg_at(ECG_N - n + k);
            let f = k as f32 / (n - 1) as f32;
            let p = vec2(x + w * f, y - v * amp);
            if k > 0 {
                line(prev, p, th, alpha(col, col.a * (0.2 + 0.8 * f)));
            }
            prev = p;
        }
    }
}

fn rect_lines_c(c: Vec2, half: f32, th: f32, col: Color) {
    rect_lines(c.x - half, c.y - half, half * 2.0, half * 2.0, th, col);
}

// Rally look
const RALLY_COLS: [u32; 4] = [0xff3d8b, 0xffb22e, 0x3dffc5, 0x5aa9ff]; // Sub, Crack, Tick, Arc
const RALLY_RIDGE_PTS: usize = 72; // vertices per end-line ridge
const RALLY_RIDGE_H: f32 = 120.0; // px of rise at an orb's landing point as it arrives
const RALLY_RIDGE_SIG: f32 = 110.0; // px, width of that rise
const RALLY_RIDGE_SPEC: f32 = 16.0; // px, spectrum texture on the end lines
const RALLY_MARK_R: f32 = 16.0; // px, landing telegraph diamond (at arrival)
const RALLY_SWING_FADE: f32 = 0.08; // s the swing arc lingers after it closes
const RALLY_SPEC_BARS: usize = 16; // mini spectrum around each avatar
const RALLY_SPEC_R: f32 = 24.0;
const RALLY_SPEC_LEN: f32 = 18.0;
const RALLY_TRAIL_N: usize = 36; // orb trail samples
const RALLY_LASER_N: usize = 90; // Arc's longer laser trail
const RALLY_TRAIL_DT: f32 = 0.008; // s between trail samples
const RALLY_TRAIL_AMP: f32 = 14.0; // px, waveform displacement of the trail
const RALLY_FLASH_LIFE: f32 = 0.35; // s, hit / perfect flashes
const RALLY_SLAM_LIFE: f32 = 0.6; // s, Sub shockwave and score flash
const RALLY_SLAM_SPEED: f32 = 900.0; // px/s
const RALLY_STRIKE_W: f32 = 10.0; // px, glow width of a strike line (doubles on the beat)
const RALLY_RING_BEATS: f32 = 2.0; // approach ring starts closing this many beats before the strike beat
const RALLY_RING_R: f32 = 110.0; // px, approach ring extra radius at its start
const RALLY_RING_HOLD: f32 = 0.12; // s, flash at the strike point after the beat
const RALLY_TEXT_LIFE: f32 = 0.7; // s, PERFECT / EARLY / LATE text
const RALLY_PERFECT_COL: u32 = 0xfff07a;
const RALLY_EARLY_COL: u32 = 0x5ad1ff;
const RALLY_LATE_COL: u32 = 0xff9a3d;

fn rally_col(c: Character) -> Color {
    hex(RALLY_COLS[c.index() as usize % RALLY_COLS.len()])
}

fn slot(pal: &Palette, c: u8) -> Color {
    match c {
        C_PRIMARY => pal.primary,
        C_SECONDARY => pal.secondary,
        C_ACCENT => pal.accent,
        C_DANGER => pal.danger,
        _ => WHITE,
    }
}

fn section_label(k: SectionKind) -> &'static str {
    match k {
        SectionKind::Calm => "CALM",
        SectionKind::Normal => "GROOVE",
        SectionKind::Intense => "SURGE",
        SectionKind::Drop => "DROP",
    }
}

fn kind_col(pal: &Palette, k: SectionKind) -> Color {
    match k {
        SectionKind::Calm => pal.primary,
        SectionKind::Normal => pal.secondary,
        SectionKind::Intense => pal.accent,
        SectionKind::Drop => pal.danger,
    }
}

// ---------------------------------------------------------------------------------------------
// Menu / UI widgets (screen space). `u` = screen_height / 1080.
// ---------------------------------------------------------------------------------------------

impl Renderer {
    /// Animated game logo with its top-left corner at (x, y).
    pub fn draw_logo(&self, x: f32, y: f32, u: f32, w: &World) {
        let pal = &self.pal;
        let kick = w.hit[0];
        let size = 96.0 * u;
        let base = y + 92.0 * u - 3.0 * u * kick;
        let s = "HEARTLINE";
        let tw = vtext_w(s, size);
        // extrusion: stacked offset copies give the letters depth
        for k in (1..=5).rev() {
            let o = k as f32 * 2.2 * u;
            let f = k as f32 / 5.0;
            vtext_draw(s, x + o * 0.6, base + o, size, alpha(mixc(pal.secondary, pal.bg0, 0.3 + 0.5 * f), 0.9), alpha(mixc(pal.secondary, pal.bg0, 0.5 + 0.4 * f), 0.9), false);
        }
        vtext_draw(s, x, base, size, mixc(pal.accent, WHITE, 0.35 + 0.3 * kick), pal.secondary, true);
        // chrome horizon line through the letters
        let hy = base - 30.0 * u;
        seg(vec2(x - 10.0 * u, hy), vec2(x + tw + 20.0 * u, hy), 1.5 * u, alpha(WHITE, 0.0), alpha(WHITE, 0.35));
        // heartline underline
        let uy = base + 26.0 * u;
        self.ecg_trace(x, uy, tw, 26.0 * u, 140, 2.5 * u, alpha(pal.primary, 0.95));
        self.ecg_trace(x, uy, tw, 26.0 * u, 140, 8.0 * u, alpha(pal.primary, 0.15));
        vtext("YOUR MUSIC IS THE LEVEL", x + 4.0 * u, y + 148.0 * u, 22.0 * u, pal.primary, -1);
    }

    /// One menu row: line-art frame, selection highlight and label.
    pub fn draw_button(&self, x: f32, y: f32, w: f32, h: f32, label: &str, selected: bool, u: f32) {
        let pal = &self.pal;
        if selected {
            let pulse = 0.85 + 0.15 * (self.time * 4.0).sin();
            crate::ui::rrect(x, y, w, h, 10.0 * u, alpha(pal.secondary, 0.42));
            crate::ui::rrect_lines(x, y, w, h, 10.0 * u, 1.5 * u, alpha(pal.accent, 0.55 + 0.35 * pulse));
            let cx = x + 12.0 * u;
            let cy = y + h * 0.5;
            line(vec2(cx, cy - 7.0 * u), vec2(cx + 7.0 * u, cy), 2.5 * u, pal.accent);
            line(vec2(cx + 7.0 * u, cy), vec2(cx, cy + 7.0 * u), 2.5 * u, pal.accent);
        } else {
            crate::ui::rrect(x, y, w, h, 10.0 * u, Color::new(0.02, 0.0, 0.07, 0.55));
            crate::ui::rrect_lines(x, y, w, h, 10.0 * u, 1.0 * u, alpha(WHITE, 0.1));
        }
        text_left(label, x + 30.0 * u, y + h * 0.68, 28.0 * u, if selected { WHITE } else { Color::new(1.0, 1.0, 1.0, 0.75) });
    }

    /// Jukebox widget: what the menu is playing. `progress` is 0..1 through the track.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_now_playing(&self, x: f32, y: f32, w: f32, u: f32, world: &World, title: &str, hint: &str, progress: f32) {
        let pal = &self.pal;
        let h = 96.0 * u;
        let rad = 14.0 * u;
        crate::ui::rrect(x, y, w, h, rad, Color::new(0.02, 0.0, 0.07, 0.74));
        crate::ui::rrect_lines(x, y, w, h, rad, 1.5 * u, alpha(pal.accent, 0.45 + 0.35 * world.hit[0]));
        vtext("NOW PLAYING", x + 16.0 * u, y + 26.0 * u, 15.0 * u, alpha(pal.primary, 0.9), -1);
        // a long title slides inside the space left of the trace
        crate::ui::marquee(title, x + 16.0 * u, y + 58.0 * u, 28.0 * u, w - 16.0 * u - 170.0 * u - 34.0 * u, WHITE, self.time);
        text_left(hint, x + 16.0 * u, y + 80.0 * u, 17.0 * u, Color::new(1.0, 1.0, 1.0, 0.55));
        // live heartline + mini spectrum on the right
        let tw = 170.0 * u;
        let tx = x + w - tw - 16.0 * u;
        let ty = y + 46.0 * u;
        let mut prev = Vec2::ZERO;
        for k in 0..=32 {
            let f = k as f32 / 32.0;
            let v = band(&self.spec_s, f);
            let p = vec2(tx + tw * f, y + h - 14.0 * u - v * 22.0 * u);
            if k > 0 {
                line(prev, p, 1.2 * u, alpha(pal.secondary, 0.55));
            }
            prev = p;
        }
        self.ecg_trace(tx, ty, tw, 26.0 * u, 120, 2.0 * u, alpha(pal.primary, 0.95));
        let k = world.hit[0];
        disc2(vec2(tx + tw, ty - self.ecg_at(P) * 26.0 * u), (4.0 + 6.0 * k) * u, 12, alpha(WHITE, 0.9), alpha(pal.primary, 0.0));
        // progress
        let pr = progress.clamp(0.0, 1.0);
        let (px, pw, py) = (x + rad, w - 2.0 * rad, y + h - 7.0 * u);
        crate::ui::rrect(px, py - 2.0 * u, pw, 4.0 * u, 2.0 * u, alpha(WHITE, 0.15));
        if pr > 0.0 {
            crate::ui::rrect(px, py - 2.0 * u, (pw * pr).max(4.0 * u), 4.0 * u, 2.0 * u, pal.accent);
            disc(vec2(px + pw * pr, py), 4.5 * u, 12, WHITE);
        }
    }

    /// Backing panel for a sub-screen region.
    #[allow(dead_code)]
    pub fn draw_panel(&self, x: f32, y: f32, w: f32, h: f32, u: f32) {
        crate::ui::panel(x, y, w, h, u);
    }
}

impl Renderer {
    /// An enemy's wireframe, turning, drawn straight to the screen (the codex's lists and card).
    pub fn draw_kind_icon(&self, kind: EnemyKind, c: Vec2, size: f32, col: Color) {
        let m = Mat3::from_rotation_z(self.time * 0.7) * Mat3::from_rotation_x(0.9 + 0.3 * (self.time * 0.5).sin()) * Mat3::from_rotation_y(self.time * 0.9);
        let th = (size * 0.07).max(1.5);
        for ed in self.kinds[kind_index(kind)].iter() {
            let (a, b) = (m * (ed[0] * size), m * (ed[1] * size));
            let depth = ((a.z + b.z) * 0.5 / size).clamp(-1.0, 1.0) * 0.5 + 0.5;
            draw_line(c.x + a.x, c.y + a.y - a.z * 0.25, c.x + b.x, c.y + b.y - b.z * 0.25, th, alpha(col, 0.4 + 0.6 * depth));
        }
    }

    /// A Heart pulse as a diagram: the Heart, the player below it, and where the bullets go.
    /// They breathe outward so the shape reads as something that moves.
    pub fn draw_pattern_icon(&self, pattern: Pattern, c: Vec2, r: f32, col: Color, col2: Color) {
        let count = match pattern {
            Pattern::Burst => 5,
            Pattern::Fan | Pattern::Sweep => 7,
            Pattern::Pincer => 8,
            Pattern::Comb => 15,
            _ => 14,
        };
        let top = c + vec2(0.0, -r * 0.55);
        let ph = (self.time * 0.6).fract();
        draw_circle_lines(top.x, top.y, r * 0.16, (r * 0.05).max(1.2), col2);
        let dot = (r * 0.065).max(1.5);
        pulse_dirs(pattern, PI * 0.5, 0.0, count, |ang, sp| {
            let reach = r * (0.32 + 0.85 * ph) * sp.clamp(0.4, 1.6);
            let p = top + dir(ang) * reach;
            draw_circle(p.x, p.y, dot, alpha(col, 1.0 - 0.6 * ph));
        });
        // the player
        let pp = c + vec2(0.0, r * 0.85);
        draw_triangle(pp + vec2(0.0, -r * 0.14), pp + vec2(-r * 0.11, r * 0.09), pp + vec2(r * 0.11, r * 0.09), WHITE);
    }
}

/// Each upgrade's emblem, drawn with lines around `c` at half-size `r`.
pub fn upgrade_emblem(up: UpgradeId, c: Vec2, r: f32, th: f32, col: Color, col2: Color, time: f32) {
    let p = |x: f32, y: f32| c + vec2(x, y) * r;
    let l = |a: Vec2, b: Vec2| line(a, b, th, col);
    match up {
        // TEMPO: two chevrons racing right
        UpgradeId::Tempo => {
            for x in [-0.6, 0.15] {
                l(p(x, -0.8), p(x + 0.6, 0.0));
                l(p(x + 0.6, 0.0), p(x, 0.8));
            }
        }
        // CHORD: three shots in a fan
        UpgradeId::Chord => {
            for a in [-0.45f32, 0.0, 0.45] {
                let d = vec2(a.sin(), -a.cos());
                l(c + d * r * 0.2, c + d * r);
                disc(c + d * r, th * 1.1, 8, col2);
            }
        }
        // GAIN: an arrow pointing up out of a bar
        UpgradeId::Gain => {
            l(p(0.0, 0.9), p(0.0, -0.9));
            l(p(-0.6, -0.3), p(0.0, -0.9));
            l(p(0.6, -0.3), p(0.0, -0.9));
            line(p(-0.7, 0.9), p(0.7, 0.9), th, col2);
        }
        // PIERCE: an arrow through a ring
        UpgradeId::Pierce => {
            arc(c, r * 0.55, 0.0, TAU, th, col2);
            l(p(-1.0, 0.0), p(1.0, 0.0));
            l(p(0.55, -0.4), p(1.0, 0.0));
            l(p(0.55, 0.4), p(1.0, 0.0));
        }
        // SUSTAIN: a cross
        UpgradeId::Sustain => {
            rquad(c, vec2(r * 0.9, r * 0.28), 0.0, col);
            rquad(c, vec2(r * 0.28, r * 0.9), 0.0, col);
        }
        // GLIDE: three speed lines behind a dot
        UpgradeId::Glide => {
            disc(p(0.6, 0.0), r * 0.3, 10, col);
            for (y, x0) in [(-0.55, -0.9), (0.0, -1.1), (0.55, -0.9)] {
                line(p(x0, y), p(0.1, y), th, col2);
            }
        }
        // PHASE: a broken circle (the dash)
        UpgradeId::Phase => {
            for k in 0..4 {
                let a0 = time * 1.5 + k as f32 * TAU / 4.0;
                arc(c, r * 0.85, a0, a0 + TAU / 4.0 - 0.5, th, col);
            }
            disc(c, r * 0.22, 10, col2);
        }
        // MAGNET: a horseshoe
        UpgradeId::Magnet => {
            arc(c + vec2(0.0, -r * 0.1), r * 0.7, PI, TAU, th * 1.4, col);
            line(p(-0.7, -0.1), p(-0.7, 0.9), th * 1.4, col);
            line(p(0.7, -0.1), p(0.7, 0.9), th * 1.4, col);
            line(p(-0.95, 0.9), p(-0.45, 0.9), th * 1.4, col2);
            line(p(0.45, 0.9), p(0.95, 0.9), th * 1.4, col2);
        }
        // ECHO: a dot with a moon circling it
        UpgradeId::Echo => {
            disc(c, r * 0.3, 10, col);
            arc(c, r * 0.85, 0.0, TAU, th * 0.6, col2);
            disc(c + dir(time * 3.0) * r * 0.85, r * 0.2, 8, col);
        }
        // ENCORE: a shield
        UpgradeId::Encore => {
            let pts = [p(-0.8, -0.8), p(0.8, -0.8), p(0.8, 0.1), p(0.0, 1.0), p(-0.8, 0.1)];
            for k in 0..5 {
                l(pts[k], pts[(k + 1) % 5]);
            }
            line(p(0.0, -0.45), p(0.0, 0.45), th, col2);
        }
        // RICOCHET: a shot bouncing off a wall
        UpgradeId::Ricochet => {
            line(p(-1.0, 0.95), p(1.0, 0.95), th, col2);
            l(p(-0.9, -0.8), p(-0.1, 0.9));
            l(p(-0.1, 0.9), p(0.8, -0.6));
            l(p(0.8, -0.6), p(0.35, -0.5));
            l(p(0.8, -0.6), p(0.75, -0.1));
        }
        // DOWNBEAT: a bar line with the heavy first beat
        UpgradeId::Downbeat => {
            line(p(-0.7, -0.9), p(-0.7, 0.9), th * 2.4, col);
            for x in [-0.1, 0.4, 0.9] {
                line(p(x, -0.5), p(x, 0.5), th * 0.8, col2);
            }
        }
        // TRANSPOSE: a round bullet turning into a square note
        UpgradeId::Transpose => {
            arc(p(-0.55, 0.0), r * 0.38, 0.0, TAU, th, col2);
            l(p(-0.05, 0.0), p(0.25, 0.0));
            poly_lines(p(0.68, 0.0), 4, r * 0.48, PI * 0.25, th, col);
        }
        // ORBIT: two blades on a ring
        UpgradeId::Orbit => {
            arc(c, r * 0.8, 0.0, TAU, th * 0.6, col2);
            for k in 0..2 {
                let q = c + dir(time * 3.0 + k as f32 * PI) * r * 0.8;
                poly_lines(q, 3, r * 0.34, time * 6.0, th, col);
            }
            disc(c, r * 0.16, 8, col);
        }
        // NOVA: a burst of rays
        UpgradeId::Nova => {
            for k in 0..8 {
                let d = dir(k as f32 * TAU / 8.0);
                let far = if k % 2 == 0 { 1.0 } else { 0.65 };
                line(c + d * r * 0.3, c + d * r * far, th, if k % 2 == 0 { col } else { col2 });
            }
        }
        // CRESCENDO: the hairpin, opening to the right
        UpgradeId::Crescendo => {
            l(p(-0.9, 0.0), p(0.9, -0.7));
            l(p(-0.9, 0.0), p(0.9, 0.7));
        }
        // ACCENT: the accent mark over a note
        UpgradeId::Accent => {
            l(p(-0.8, -0.9), p(0.7, -0.45));
            l(p(0.7, -0.45), p(-0.8, 0.0));
            disc(p(0.0, 0.6), r * 0.3, 10, col2);
        }
        // CODA: the coda sign
        UpgradeId::Coda => {
            arc(c, r * 0.6, 0.0, TAU, th, col);
            line(p(0.0, -1.0), p(0.0, 1.0), th, col2);
            line(p(-1.0, 0.0), p(1.0, 0.0), th, col2);
        }
        // SYNCOPE: a dash leaving a beat tick
        UpgradeId::Syncope => {
            line(p(-0.8, -0.7), p(-0.8, 0.7), th * 1.6, col2);
            l(p(-0.4, 0.0), p(0.9, 0.0));
            l(p(0.45, -0.4), p(0.9, 0.0));
            l(p(0.45, 0.4), p(0.9, 0.0));
        }
    }
}

fn roman(n: u8) -> &'static str {
    match n {
        1 => "I",
        2 => "II",
        3 => "III",
        4 => "IV",
        5 => "V",
        6 => "VI",
        _ => "",
    }
}

/// Centered text in world space, rasterised at device resolution.
fn wtext(s: &str, pos: Vec2, size: f32, color: Color, tscale: f32) {
    let has = UI_FONT.with(|c| c.borrow().is_some());
    let size = if has { size * FONT_SCALE } else { size };
    let fs = (size * tscale).clamp(6.0, 200.0);
    let font_size = fs.round().max(1.0) as u16;
    let font_scale = size / font_size as f32;
    UI_FONT.with(|c| {
        let f = c.borrow();
        let dim = measure_text(s, f.as_ref(), font_size, font_scale);
        draw_text_ex(
            s,
            pos.x - dim.width * 0.5,
            pos.y,
            TextParams { font: f.as_ref(), font_size, font_scale, color, ..Default::default() },
        );
    });
}
