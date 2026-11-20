//! Menu widgets: line-art icons, icon rows, segmented choices, sliders, tiles and tooltips.
//! Everything is immediate-mode: a widget draws itself and reports hover / click this frame.

use macroquad::prelude::*;

use crate::render::{text_center, text_left, text_w, Palette};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Icon {
    Play,
    Lan,
    Library,
    Options,
    Quit,
    Endless,
    Single,
    Files,
    Folder,
    Live,
    Music,
    Sfx,
    Sync,
    Visuals,
    Display,
    Back,
    Road,
    Prev,
    Next,
    List,
    Dice,
    Book,
    Heart,
    Skull,
    Glass,
    Allegro,
    Forte,
    Swarm,
    Calendar,
    Bolt,
    Shard,
    Combo,
    Cross,
    Retry,
    Home,
    Trophy,
    Target,
    Note,
    Playlist,
}

fn a(c: Color, alpha: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * alpha)
}

fn poly(pts: &[(f32, f32)], cx: f32, cy: f32, s: f32, th: f32, col: Color, closed: bool) {
    let n = pts.len();
    let last = if closed { n } else { n - 1 };
    for i in 0..last {
        let (p, q) = (pts[i], pts[(i + 1) % n]);
        draw_line(cx + p.0 * s, cy + p.1 * s, cx + q.0 * s, cy + q.1 * s, th, col);
    }
}

fn arc(cx: f32, cy: f32, r: f32, a0: f32, a1: f32, th: f32, col: Color) {
    let n = 14;
    for i in 0..n {
        let t0 = a0 + (a1 - a0) * i as f32 / n as f32;
        let t1 = a0 + (a1 - a0) * (i + 1) as f32 / n as f32;
        draw_line(cx + t0.cos() * r, cy + t0.sin() * r, cx + t1.cos() * r, cy + t1.sin() * r, th, col);
    }
}

/// Draw a line-art icon centred on (cx, cy); `s` is its half-size in pixels.
pub fn icon(k: Icon, cx: f32, cy: f32, s: f32, col: Color) {
    let th = (s * 0.16).max(1.5);
    let tau = std::f32::consts::TAU;
    match k {
        Icon::Play => poly(&[(-0.6, -0.85), (0.85, 0.0), (-0.6, 0.85)], cx, cy, s, th, col, true),
        Icon::Lan => {
            draw_rectangle_lines(cx - s, cy - 0.7 * s, 0.75 * s, 0.6 * s, th, col);
            draw_rectangle_lines(cx + 0.25 * s, cy + 0.1 * s, 0.75 * s, 0.6 * s, th, col);
            poly(&[(-0.62, -0.1), (-0.62, 0.4), (0.25, 0.4)], cx, cy, s, th, col, false);
        }
        Icon::Library | Icon::Music => {
            draw_circle_lines(cx - 0.45 * s, cy + 0.55 * s, 0.3 * s, th, col);
            draw_circle_lines(cx + 0.5 * s, cy + 0.35 * s, 0.3 * s, th, col);
            poly(&[(-0.15, 0.55), (-0.15, -0.7), (0.8, -0.9), (0.8, 0.35)], cx, cy, s, th, col, false);
        }
        Icon::Options => {
            draw_circle_lines(cx, cy, 0.42 * s, th, col);
            for i in 0..8 {
                let t = tau * i as f32 / 8.0;
                draw_line(cx + t.cos() * 0.62 * s, cy + t.sin() * 0.62 * s, cx + t.cos() * 0.95 * s, cy + t.sin() * 0.95 * s, th * 1.3, col);
            }
        }
        Icon::Quit => {
            arc(cx, cy + 0.05 * s, 0.75 * s, -tau * 0.17, tau * 0.67, th, col);
            draw_line(cx, cy - s, cx, cy - 0.1 * s, th, col);
        }
        Icon::Endless => {
            let n = 28;
            for i in 0..n {
                let f = |j: usize| {
                    let t = tau * j as f32 / n as f32;
                    (cx + t.sin() * s, cy + (2.0 * t).sin() * 0.45 * s)
                };
                let (p, q) = (f(i), f(i + 1));
                draw_line(p.0, p.1, q.0, q.1, th, col);
            }
        }
        Icon::Single => {
            draw_circle_lines(cx, cy, 0.85 * s, th, col);
            draw_circle_lines(cx, cy, 0.22 * s, th, col);
        }
        Icon::Files => {
            poly(&[(-0.6, -0.9), (0.25, -0.9), (0.65, -0.5), (0.65, 0.9), (-0.6, 0.9)], cx, cy, s, th, col, true);
            poly(&[(0.25, -0.9), (0.25, -0.5), (0.65, -0.5)], cx, cy, s, th, col, false);
        }
        Icon::Folder => poly(&[(-0.9, -0.6), (-0.3, -0.6), (-0.1, -0.35), (0.9, -0.35), (0.9, 0.7), (-0.9, 0.7)], cx, cy, s, th, col, true),
        Icon::Live => {
            draw_circle(cx, cy, 0.18 * s, col);
            for r in [0.5, 0.9] {
                arc(cx, cy, r * s, -tau * 0.11, tau * 0.11, th, col);
                arc(cx, cy, r * s, tau * 0.39, tau * 0.61, th, col);
            }
        }
        Icon::Sfx => {
            for i in 0..8 {
                let t = tau * i as f32 / 8.0;
                let r = if i % 2 == 0 { 0.95 } else { 0.55 };
                draw_line(cx + t.cos() * 0.2 * s, cy + t.sin() * 0.2 * s, cx + t.cos() * r * s, cy + t.sin() * r * s, th, col);
            }
        }
        Icon::Sync => {
            draw_circle_lines(cx, cy, 0.85 * s, th, col);
            poly(&[(0.0, -0.5), (0.0, 0.0), (0.4, 0.25)], cx, cy, s, th, col, false);
        }
        Icon::Visuals => {
            arc(cx, cy + 0.75 * s, 1.15 * s, -tau * 0.36, -tau * 0.14, th, col);
            arc(cx, cy - 0.75 * s, 1.15 * s, tau * 0.14, tau * 0.36, th, col);
            draw_circle_lines(cx, cy, 0.28 * s, th, col);
        }
        Icon::Display => {
            draw_rectangle_lines(cx - 0.9 * s, cy - 0.75 * s, 1.8 * s, 1.15 * s, th, col);
            poly(&[(-0.4, 0.85), (0.4, 0.85)], cx, cy, s, th, col, false);
            poly(&[(0.0, 0.4), (0.0, 0.85)], cx, cy, s, th, col, false);
        }
        Icon::Road => {
            poly(&[(-0.95, 0.9), (-0.2, -0.9)], cx, cy, s, th, col, false);
            poly(&[(0.95, 0.9), (0.2, -0.9)], cx, cy, s, th, col, false);
            poly(&[(0.0, 0.9), (0.0, 0.45)], cx, cy, s, th, col, false);
            poly(&[(0.0, 0.1), (0.0, -0.25)], cx, cy, s, th, col, false);
            poly(&[(0.0, -0.5), (0.0, -0.75)], cx, cy, s, th, col, false);
        }
        Icon::Back => {
            poly(&[(0.9, 0.0), (-0.8, 0.0)], cx, cy, s, th, col, false);
            poly(&[(-0.2, -0.6), (-0.8, 0.0), (-0.2, 0.6)], cx, cy, s, th, col, false);
        }
        Icon::Prev => {
            poly(&[(0.55, -0.8), (-0.45, 0.0), (0.55, 0.8)], cx, cy, s, th, col, true);
            poly(&[(-0.8, -0.8), (-0.8, 0.8)], cx, cy, s, th, col, false);
        }
        Icon::Next => {
            poly(&[(-0.55, -0.8), (0.45, 0.0), (-0.55, 0.8)], cx, cy, s, th, col, true);
            poly(&[(0.8, -0.8), (0.8, 0.8)], cx, cy, s, th, col, false);
        }
        Icon::List => {
            for y in [-0.6, 0.0, 0.6] {
                draw_circle(cx - 0.75 * s, cy + y * s, th * 0.8, col);
                poly(&[(-0.35, y), (0.9, y)], cx, cy, s, th, col, false);
            }
        }
        Icon::Dice => {
            poly(&[(-0.8, -0.8), (0.8, -0.8), (0.8, 0.8), (-0.8, 0.8)], cx, cy, s, th, col, true);
            for (x, y) in [(-0.38, -0.38), (0.38, 0.38), (0.0, 0.0), (0.38, -0.38), (-0.38, 0.38)] {
                draw_circle(cx + x * s, cy + y * s, th * 0.75, col);
            }
        }
        Icon::Book => {
            poly(&[(0.0, -0.6), (-0.9, -0.8), (-0.9, 0.65), (0.0, 0.85), (0.9, 0.65), (0.9, -0.8)], cx, cy, s, th, col, true);
            poly(&[(0.0, -0.6), (0.0, 0.85)], cx, cy, s, th, col, false);
        }
        Icon::Heart => poly(
            &[(0.0, 0.9), (-0.9, 0.0), (-0.9, -0.5), (-0.5, -0.85), (0.0, -0.45), (0.5, -0.85), (0.9, -0.5), (0.9, 0.0)],
            cx,
            cy,
            s,
            th,
            col,
            true,
        ),
        Icon::Skull => {
            arc(cx, cy - 0.15 * s, 0.75 * s, tau * 0.42, tau * 1.08, th, col);
            poly(&[(-0.62, 0.28), (-0.4, 0.85), (0.4, 0.85), (0.62, 0.28)], cx, cy, s, th, col, false);
            draw_circle(cx - 0.3 * s, cy - 0.15 * s, th * 1.1, col);
            draw_circle(cx + 0.3 * s, cy - 0.15 * s, th * 1.1, col);
            poly(&[(0.0, 0.5), (0.0, 0.85)], cx, cy, s, th * 0.8, col, false);
        }
        Icon::Glass => {
            poly(&[(0.0, -0.95), (0.75, 0.0), (0.0, 0.95), (-0.75, 0.0)], cx, cy, s, th, col, true);
            poly(&[(0.0, -0.95), (-0.15, -0.2), (0.2, 0.1), (0.0, 0.95)], cx, cy, s, th * 0.8, col, false);
        }
        Icon::Allegro => {
            for x in [-0.75, -0.05] {
                poly(&[(x, -0.75), (x + 0.7, 0.0), (x, 0.75)], cx, cy, s, th, col, false);
            }
        }
        Icon::Forte => {
            for (i, hh) in [0.45f32, 0.95, 1.45].iter().enumerate() {
                let x = -0.65 + 0.65 * i as f32;
                poly(&[(x, 0.8), (x, 0.8 - hh)], cx, cy, s, th * 1.6, col, false);
            }
        }
        Icon::Swarm => {
            for (x, y) in [(0.0, -0.5), (-0.55, 0.45), (0.55, 0.45)] {
                poly(&[(x, y - 0.38), (x + 0.36, y + 0.3), (x - 0.36, y + 0.3)], cx, cy, s, th, col, true);
            }
        }
        Icon::Calendar => {
            poly(&[(-0.85, -0.65), (0.85, -0.65), (0.85, 0.85), (-0.85, 0.85)], cx, cy, s, th, col, true);
            poly(&[(-0.85, -0.2), (0.85, -0.2)], cx, cy, s, th, col, false);
            poly(&[(-0.4, -0.95), (-0.4, -0.5)], cx, cy, s, th, col, false);
            poly(&[(0.4, -0.95), (0.4, -0.5)], cx, cy, s, th, col, false);
            draw_circle(cx, cy + 0.33 * s, th * 1.0, col);
        }
        Icon::Bolt => poly(&[(0.3, -0.95), (-0.55, 0.15), (0.0, 0.15), (-0.3, 0.95), (0.55, -0.15), (0.0, -0.15)], cx, cy, s, th, col, true),
        Icon::Shard => {
            poly(&[(0.0, -0.95), (0.6, -0.2), (0.0, 0.95), (-0.6, -0.2)], cx, cy, s, th, col, true);
            poly(&[(-0.6, -0.2), (0.6, -0.2)], cx, cy, s, th * 0.8, col, false);
        }
        Icon::Combo => {
            for y in [-0.55, 0.0, 0.55] {
                poly(&[(-0.7, y + 0.3), (0.0, y - 0.3), (0.7, y + 0.3)], cx, cy, s, th, col, false);
            }
        }
        Icon::Cross => {
            poly(&[(-0.7, -0.7), (0.7, 0.7)], cx, cy, s, th * 1.2, col, false);
            poly(&[(0.7, -0.7), (-0.7, 0.7)], cx, cy, s, th * 1.2, col, false);
        }
        Icon::Retry => {
            arc(cx, cy, 0.75 * s, -tau * 0.15, tau * 0.65, th, col);
            poly(&[(0.2, -0.95), (0.62, -0.55), (0.1, -0.35)], cx, cy, s, th, col, false);
        }
        Icon::Home => {
            poly(&[(-0.9, 0.0), (0.0, -0.85), (0.9, 0.0)], cx, cy, s, th, col, false);
            poly(&[(-0.6, -0.1), (-0.6, 0.85), (0.6, 0.85), (0.6, -0.1)], cx, cy, s, th, col, false);
        }
        Icon::Trophy => {
            poly(&[(-0.55, -0.85), (0.55, -0.85), (0.45, 0.0), (0.0, 0.3), (-0.45, 0.0)], cx, cy, s, th, col, true);
            poly(&[(0.0, 0.3), (0.0, 0.75)], cx, cy, s, th, col, false);
            poly(&[(-0.45, 0.85), (0.45, 0.85)], cx, cy, s, th, col, false);
            arc(cx - 0.6 * s, cy - 0.5 * s, 0.3 * s, tau * 0.25, tau * 0.75, th * 0.8, col);
            arc(cx + 0.6 * s, cy - 0.5 * s, 0.3 * s, -tau * 0.25, tau * 0.25, th * 0.8, col);
        }
        Icon::Target => {
            draw_circle_lines(cx, cy, 0.7 * s, th, col);
            draw_circle(cx, cy, th * 1.0, col);
            for (x, y) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                poly(&[(x * 0.45, y * 0.45), (x, y)], cx, cy, s, th, col, false);
            }
        }
        Icon::Note => {
            draw_circle_lines(cx - 0.25 * s, cy + 0.55 * s, 0.32 * s, th, col);
            poly(&[(0.07, 0.55), (0.07, -0.85), (0.65, -0.45)], cx, cy, s, th, col, false);
        }
        Icon::Playlist => {
            for (y, x1) in [(-0.7, 0.9), (-0.15, 0.25), (0.4, 0.05)] {
                poly(&[(-0.9, y), (x1, y)], cx, cy, s, th, col, false);
            }
            draw_circle_lines(cx + 0.45 * s, cy + 0.6 * s, 0.26 * s, th, col);
            poly(&[(0.71, 0.6), (0.71, -0.3), (0.95, -0.1)], cx, cy, s, th, col, false);
        }
    }
}

/// The icon of mutator `i` (the order of `game::MODS`).
pub fn mod_icon(i: usize) -> Icon {
    [Icon::Allegro, Icon::Forte, Icon::Swarm, Icon::Skull, Icon::Glass][i.min(4)]
}

// ---------------------------------------------------------------------------------------------
// Soft-edged shapes
// ---------------------------------------------------------------------------------------------

/// Corner radius of panels and buttons, in 1080p pixels (multiply by `u`).
pub const RADIUS: f32 = 10.0;

fn rounded_pts(x: f32, y: f32, w: f32, h: f32, r: f32) -> Vec<Vec2> {
    let r = r.min(w * 0.5).min(h * 0.5).max(0.0);
    let q = std::f32::consts::FRAC_PI_2;
    let n = 6;
    let mut pts = Vec::with_capacity(4 * (n + 1));
    for (cx, cy, a0) in [(x + w - r, y + r, -q), (x + w - r, y + h - r, 0.0), (x + r, y + h - r, q), (x + r, y + r, 2.0 * q)] {
        for i in 0..=n {
            let t = a0 + q * i as f32 / n as f32;
            pts.push(vec2(cx + t.cos() * r, cy + t.sin() * r));
        }
    }
    pts
}

/// A filled rectangle with rounded corners.
pub fn rrect(x: f32, y: f32, w: f32, h: f32, r: f32, col: Color) {
    let pts = rounded_pts(x, y, w, h, r);
    let c = vec2(x + w * 0.5, y + h * 0.5);
    for i in 0..pts.len() {
        draw_triangle(c, pts[i], pts[(i + 1) % pts.len()], col);
    }
}

/// The outline of a rounded rectangle.
pub fn rrect_lines(x: f32, y: f32, w: f32, h: f32, r: f32, th: f32, col: Color) {
    let pts = rounded_pts(x, y, w, h, r);
    for i in 0..pts.len() {
        let (p, q) = (pts[i], pts[(i + 1) % pts.len()]);
        draw_line(p.x, p.y, q.x, q.y, th, col);
    }
}

/// The dark backing of a group of controls.
pub fn panel(x: f32, y: f32, w: f32, h: f32, u: f32) {
    rrect(x, y, w, h, 14.0 * u, Color::new(0.02, 0.0, 0.07, 0.74));
    rrect_lines(x, y, w, h, 14.0 * u, 1.2 * u, Color::new(1.0, 1.0, 1.0, 0.12));
}

/// A coloured bar that names the group under it.
#[allow(clippy::too_many_arguments)]
pub fn header(x: f32, y: f32, w: f32, h: f32, ic: Icon, label: &str, col: Color, focus: bool, u: f32) {
    rrect(x, y, w, h, 8.0 * u, Color::new(col.r * 0.55, col.g * 0.55, col.b * 0.6, 0.9));
    rrect_lines(x, y, w, h, 8.0 * u, if focus { 2.5 } else { 1.5 } * u, if focus { WHITE } else { a(col, 0.95) });
    icon(ic, x + h * 0.6, y + h * 0.5, h * 0.26, WHITE);
    text_center(label, x + w * 0.5, y + h * 0.5 + 8.0 * u, 25.0 * u, WHITE);
}

/// A thin line between groups.
pub fn sep(x: f32, y: f32, w: f32, u: f32) {
    draw_line(x, y, x + w, y, 1.0 * u, Color::new(1.0, 1.0, 1.0, 0.16));
}

/// A square tile for one icon. The caller draws the icon. Returns (hovered, clicked).
#[allow(clippy::too_many_arguments)]
pub fn tile(x: f32, y: f32, s: f32, on: bool, focus: bool, col: Color, alpha: f32, u: f32) -> (bool, bool) {
    let hv = hover(x, y, s, s);
    let r = RADIUS * u;
    let fill = if on { Color::new(col.r * 0.4, col.g * 0.4, col.b * 0.45, 0.85 * alpha) } else { Color::new(0.03, 0.02, 0.09, if hv { 0.85 } else { 0.7 } * alpha) };
    rrect(x, y, s, s, r, fill);
    let edge = if on { a(col, alpha) } else { Color::new(1.0, 1.0, 1.0, if hv || focus { 0.6 } else { 0.2 } * alpha) };
    rrect_lines(x, y, s, s, r, if on { 2.5 } else { 1.2 } * u, edge);
    if on {
        // the small marker under the chosen tile
        let (cx, cy, d) = (x + s * 0.5, y + s, 6.0 * u);
        draw_triangle(vec2(cx - d, cy), vec2(cx, cy - d), vec2(cx + d, cy), a(WHITE, alpha));
        draw_triangle(vec2(cx - d, cy), vec2(cx, cy + d), vec2(cx + d, cy), a(WHITE, alpha));
    }
    if focus {
        rrect_lines(x - 4.0 * u, y - 4.0 * u, s + 8.0 * u, s + 8.0 * u, r + 3.0 * u, 1.5 * u, a(WHITE, 0.75 * alpha));
    }
    (hv, hv && is_mouse_button_pressed(MouseButton::Left))
}

/// A key cap with what the key does after it. Returns the x where the next one can start.
pub fn keycap(key: &str, what: &str, x: f32, y: f32, u: f32) -> f32 {
    let size = 17.0 * u;
    let kw = text_w(key, size) + 16.0 * u;
    let h = 26.0 * u;
    rrect(x, y - h * 0.5, kw, h, 6.0 * u, Color::new(1.0, 1.0, 1.0, 0.1));
    rrect_lines(x, y - h * 0.5, kw, h, 6.0 * u, 1.0 * u, Color::new(1.0, 1.0, 1.0, 0.35));
    text_left(key, x + 8.0 * u, y + 6.0 * u, size, Color::new(1.0, 1.0, 1.0, 0.9));
    text_left(what, x + kw + 8.0 * u, y + 6.0 * u, 18.0 * u, Color::new(1.0, 1.0, 1.0, 0.6));
    x + kw + 8.0 * u + text_w(what, 18.0 * u) + 26.0 * u
}

/// `s` cut down to fit `max_w`, ending in "..." when something was cut.
pub fn fit(s: &str, size: f32, max_w: f32) -> String {
    if text_w(s, size) <= max_w {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        let t: String = chars[..mid].iter().chain("...".chars().collect::<Vec<_>>().iter()).collect();
        if text_w(&t, size) <= max_w {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    chars[..lo].iter().collect::<String>().trim_end().to_string() + "..."
}

/// One line of text in a space `max_w` wide. Text that is too long slides back and forth so
/// all of it can be read. Do not call it while a `clip` is set.
pub fn marquee(s: &str, x: f32, y: f32, size: f32, max_w: f32, col: Color, time: f32) {
    let tw = text_w(s, size);
    if tw <= max_w {
        text_left(s, x, y, size, col);
        return;
    }
    let over = tw - max_w;
    let speed = size * 1.5;
    let (hold, travel) = (1.6, over / speed);
    let p = time % (2.0 * (hold + travel));
    let off = if p < hold {
        0.0
    } else if p < hold + travel {
        (p - hold) * speed
    } else if p < 2.0 * hold + travel {
        over
    } else {
        over - (p - 2.0 * hold - travel) * speed
    };
    clip(Some((x, y - size, max_w, size * 1.45)));
    text_left(s, x - off, y, size, col);
    clip(None);
}

/// The launch into a run: the screen goes dark behind a heartline, then opens on the arena in
/// two beats (a small opening, a hold, then all the way), like a heart. `k` runs 0..1.
pub fn heartbeat_iris(k: f32, pal: &Palette) {
    let (sw, sh) = (screen_width(), screen_height());
    let u = sh / 1080.0;
    let c = vec2(sw * 0.5, sh * 0.5);
    let rmax = (sw * sw + sh * sh).sqrt() * 0.52;
    let ease = |x: f32| {
        let x = x.clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    };
    let dark = Color::new(0.01, 0.0, 0.03, 1.0);
    // how far the iris is open
    let kb = ((k - 0.2) / 0.8).clamp(0.0, 1.0);
    let r = if k < 0.2 { 0.0 } else { rmax * (0.16 * ease(kb / 0.22) + 0.84 * ease((kb - 0.42) / 0.58)) };
    if k < 0.2 {
        draw_rectangle(0.0, 0.0, sw, sh, a(dark, ease(k / 0.2)));
    } else {
        let n = 64;
        let tau = std::f32::consts::TAU;
        for i in 0..n {
            let (a0, a1) = (tau * i as f32 / n as f32, tau * (i + 1) as f32 / n as f32);
            let (d0, d1) = (vec2(a0.cos(), a0.sin()), vec2(a1.cos(), a1.sin()));
            draw_triangle(c + d0 * r, c + d0 * rmax * 1.1, c + d1 * rmax * 1.1, dark);
            draw_triangle(c + d0 * r, c + d1 * rmax * 1.1, c + d1 * r, dark);
            // the lip of the opening glows
            let fade = 1.0 - ease((kb - 0.7) / 0.3);
            draw_line(c.x + d0.x * r, c.y + d0.y * r, c.x + d1.x * r, c.y + d1.y * r, 3.0 * u, a(pal.accent, 0.9 * fade));
            draw_line(c.x + d0.x * r, c.y + d0.y * r, c.x + d1.x * r, c.y + d1.y * r, 12.0 * u, a(pal.accent, 0.14 * fade));
        }
    }
    // the heartline: it runs in from the left, spikes in the middle, and stays outside the opening
    let draw_to = (k / 0.2).min(1.0);
    let la = 1.0 - ease((kb - 0.35) / 0.4);
    if la > 0.01 {
        let n = 160;
        let amp = 70.0 * u;
        let y_at = |f: f32| {
            let x = (f - 0.5) * 22.0;
            // a flat line with one complex: a small rise, a sharp spike, a dip, a bump
            let v = 0.18 * (-(x + 1.6) * (x + 1.6) * 4.0).exp() + 1.0 * (-(x * x) * 30.0).exp() - 0.38 * (-(x - 0.35) * (x - 0.35) * 26.0).exp() + 0.22 * (-(x - 1.5) * (x - 1.5) * 3.0).exp();
            c.y - v * amp
        };
        for i in 0..n {
            let (f0, f1) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
            if f1 > draw_to {
                break;
            }
            let (x0, x1) = (f0 * sw, f1 * sw);
            if ((x0 + x1) * 0.5 - c.x).abs() < r {
                continue;
            }
            draw_line(x0, y_at(f0), x1, y_at(f1), 3.0 * u, a(pal.primary, la));
            draw_line(x0, y_at(f0), x1, y_at(f1), 10.0 * u, a(pal.primary, 0.15 * la));
        }
    }
}

/// Limit drawing to a rectangle (screen coordinates), or lift the limit with `None`.
pub fn clip(r: Option<(f32, f32, f32, f32)>) {
    let s = screen_dpi_scale();
    let gl = unsafe { get_internal_gl() };
    gl.quad_gl.scissor(r.map(|(x, y, w, h)| ((x * s) as i32, (y * s) as i32, (w * s) as i32, (h * s) as i32)));
}

/// Difficulty icon: a heartbeat trace that gets taller and busier with the tier; the last tier
/// is a flat line.
pub fn diff_icon(tier: usize, n_tiers: usize, cx: f32, cy: f32, s: f32, col: Color) {
    let th = (s * 0.12).max(1.5);
    if tier + 1 >= n_tiers {
        draw_line(cx - s, cy, cx + s, cy, th * 1.4, col);
        draw_line(cx - s, cy - 0.3 * s, cx - s, cy + 0.3 * s, th, col);
        draw_line(cx + s, cy - 0.3 * s, cx + s, cy + 0.3 * s, th, col);
        return;
    }
    let k = tier as f32 / (n_tiers.max(3) - 2) as f32; // 0 .. 1 over the beating tiers
    let spikes = 1 + tier / 2;
    let amp = (0.3 + 0.65 * k) * s;
    let seg = 2.0 * s / spikes as f32;
    let mut pts: Vec<(f32, f32)> = vec![(cx - s, cy)];
    for i in 0..spikes {
        let x0 = cx - s + seg * i as f32;
        pts.push((x0 + seg * 0.30, cy));
        pts.push((x0 + seg * 0.42, cy - amp));
        pts.push((x0 + seg * 0.58, cy + amp * 0.7));
        pts.push((x0 + seg * 0.70, cy));
    }
    pts.push((cx + s, cy));
    for w in pts.windows(2) {
        draw_line(w[0].0, w[0].1, w[1].0, w[1].1, th, col);
    }
}

/// Colour of a difficulty tier, from calm to dangerous.
pub fn diff_color(tier: usize, n_tiers: usize, pal: &Palette) -> Color {
    let k = tier as f32 / (n_tiers.max(2) - 1) as f32;
    let (c0, c1) = (pal.primary, pal.danger);
    Color::new(c0.r + (c1.r - c0.r) * k, c0.g + (c1.g - c0.g) * k, c0.b + (c1.b - c0.b) * k, 1.0)
}

pub fn hover(x: f32, y: f32, w: f32, h: f32) -> bool {
    let (mx, my) = mouse_position();
    mx >= x && mx <= x + w && my >= y && my <= y + h
}

fn mouse_moved() -> bool {
    mouse_delta_position() != Vec2::ZERO
}

/// Break `text` into lines no wider than `max_w`.
pub fn wrap(text: &str, size: f32, max_w: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let trial = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if !cur.is_empty() && text_w(&trial, size) > max_w {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
        } else {
            cur = trial;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// What the pointer (or the keyboard selection) is on this frame: shown as a tooltip.
#[derive(Default)]
pub struct Tip {
    pub title: String,
    pub body: String,
    /// True when the mouse is over the element (the tooltip then follows the pointer).
    pub hover: bool,
}

impl Tip {
    /// Offer a tip. A hovered element always wins over a keyboard-selected one.
    pub fn set(&mut self, title: &str, body: &str, hover: bool) {
        if hover || !self.hover {
            self.title = title.to_string();
            self.body = body.to_string();
            self.hover = hover;
        }
    }

    /// Draw the tip: a box at the pointer when hovering, otherwise a line at (x, y).
    pub fn draw(&self, x: f32, y: f32, u: f32, pal: &Palette) {
        if self.title.is_empty() && self.body.is_empty() {
            return;
        }
        if !self.hover {
            let s = if self.body.is_empty() { self.title.clone() } else { format!("{}: {}", self.title, self.body) };
            crate::render::text_shadow(&s, x, y, 22.0 * u, Color::new(1.0, 1.0, 1.0, 0.8), -1);
            return;
        }
        let w = 400.0 * u;
        let pad = 14.0 * u;
        let lines = wrap(&self.body, 20.0 * u, w - 2.0 * pad);
        let h = pad * 2.0 + 26.0 * u + lines.len() as f32 * 24.0 * u;
        let (mx, my) = mouse_position();
        let bx = (mx + 22.0 * u).min(screen_width() - w - 8.0 * u);
        let by = if my + 26.0 * u + h > screen_height() { my - h - 12.0 * u } else { my + 26.0 * u };
        rrect(bx, by, w, h, RADIUS * u, Color::new(0.02, 0.0, 0.07, 0.95));
        rrect_lines(bx, by, w, h, RADIUS * u, 1.5 * u, a(pal.accent, 0.9));
        text_left(&self.title, bx + pad, by + pad + 18.0 * u, 24.0 * u, pal.accent);
        for (i, l) in lines.iter().enumerate() {
            text_left(l, bx + pad, by + pad + 44.0 * u + i as f32 * 24.0 * u, 20.0 * u, Color::new(1.0, 1.0, 1.0, 0.88));
        }
    }
}

fn frame(x: f32, y: f32, w: f32, h: f32, on: bool, focus: bool, u: f32, pal: &Palette) {
    let r = RADIUS * u;
    if on {
        rrect(x, y, w, h, r, a(pal.secondary, 0.42));
        rrect_lines(x, y, w, h, r, 2.0 * u, pal.accent);
    } else {
        rrect(x, y, w, h, r, Color::new(0.02, 0.0, 0.07, if focus { 0.72 } else { 0.55 }));
        rrect_lines(x, y, w, h, r, 1.2 * u, Color::new(1.0, 1.0, 1.0, if focus { 0.55 } else { 0.18 }));
    }
}

/// A row with an icon and a label. Returns (hovered, clicked).
#[allow(clippy::too_many_arguments)]
pub fn icon_row(x: f32, y: f32, w: f32, h: f32, ic: Icon, label: &str, selected: bool, u: f32, pal: &Palette, time: f32) -> (bool, bool) {
    let hv = hover(x, y, w, h);
    let r = RADIUS * u;
    if selected {
        let pulse = 0.85 + 0.15 * (time * 4.0).sin();
        // a dark backing first, so the row reads over a bright sky
        rrect(x, y, w, h, r, Color::new(0.02, 0.0, 0.07, 0.6));
        rrect(x, y, w, h, r, a(pal.secondary, 0.5));
        rrect_lines(x, y, w, h, r, 1.5 * u, a(pal.accent, 0.55 + 0.35 * pulse));
    } else {
        rrect(x, y, w, h, r, Color::new(0.02, 0.0, 0.07, if hv { 0.8 } else { 0.66 }));
        rrect_lines(x, y, w, h, r, 1.0 * u, Color::new(1.0, 1.0, 1.0, 0.12));
    }
    let col = if selected { WHITE } else { Color::new(1.0, 1.0, 1.0, 0.78) };
    // the icon sits in its own cell, like a portrait
    rrect(x + 6.0 * u, y + 6.0 * u, h - 12.0 * u, h - 12.0 * u, r * 0.7, Color::new(1.0, 1.0, 1.0, if selected { 0.12 } else { 0.05 }));
    icon(ic, x + h * 0.5, y + h * 0.5, h * 0.24, if selected { pal.accent } else { col });
    text_left(label, x + h + 12.0 * u, y + h * 0.5 + 9.0 * u, 28.0 * u, col);
    (hv, hv && is_mouse_button_pressed(MouseButton::Left))
}

/// A vertical list of icon rows with keyboard and mouse selection.
/// Returns (activated row, row under the mouse).
#[allow(clippy::too_many_arguments)]
pub fn icon_list(items: &[(Icon, String)], ys: &[f32], sel: &mut usize, x: f32, w: f32, h: f32, u: f32, pal: &Palette, time: f32) -> (Option<usize>, Option<usize>) {
    if is_key_pressed(KeyCode::Down) {
        *sel = (*sel + 1) % items.len();
    }
    if is_key_pressed(KeyCode::Up) {
        *sel = (*sel + items.len() - 1) % items.len();
    }
    let mut activated = None;
    let mut hovered = None;
    for (i, (ic, label)) in items.iter().enumerate() {
        let (hv, click) = icon_row(x, ys[i], w, h, *ic, label, *sel == i, u, pal, time);
        if hv {
            hovered = Some(i);
            if mouse_moved() {
                *sel = i;
            }
        }
        if click {
            *sel = i;
            activated = Some(i);
        }
    }
    if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
        activated = Some(*sel);
    }
    (activated, hovered)
}

/// A row of mutually exclusive choices, all visible. `focus` marks keyboard focus on the row.
/// Returns (clicked choice, hovered choice).
#[allow(clippy::too_many_arguments)]
pub fn segments(x: f32, y: f32, w: f32, h: f32, labels: &[(Option<Icon>, &str)], current: usize, focus: bool, u: f32, pal: &Palette) -> (Option<usize>, Option<usize>) {
    let n = labels.len().max(1);
    let gap = 8.0 * u;
    let sw = (w - gap * (n as f32 - 1.0)) / n as f32;
    let (mut click, mut hov) = (None, None);
    for (i, (ic, label)) in labels.iter().enumerate() {
        let sx = x + i as f32 * (sw + gap);
        let on = i == current;
        let hv = hover(sx, y, sw, h);
        frame(sx, y, sw, h, on, hv || (focus && on), u, pal);
        let col = if on { WHITE } else { Color::new(1.0, 1.0, 1.0, if hv { 0.9 } else { 0.6 }) };
        let size = 24.0 * u;
        let tw = text_w(label, size);
        let iw = if ic.is_some() { h * 0.5 + 10.0 * u } else { 0.0 };
        let x0 = sx + (sw - tw - iw) * 0.5;
        if let Some(ic) = ic {
            icon(*ic, x0 + h * 0.22, y + h * 0.5, h * 0.22, if on { pal.accent } else { col });
        }
        text_left(label, x0 + iw, y + h * 0.5 + 8.0 * u, size, col);
        if hv {
            hov = Some(i);
            if is_mouse_button_pressed(MouseButton::Left) {
                click = Some(i);
            }
        }
    }
    if focus {
        rrect(x - 11.0 * u, y + 4.0 * u, 4.0 * u, h - 8.0 * u, 2.0 * u, pal.accent);
    }
    (click, hov)
}

/// A horizontal slider over 0..1. Returns the new value while it is being dragged.
#[allow(clippy::too_many_arguments)]
pub fn slider(x: f32, y: f32, w: f32, h: f32, v: f32, focus: bool, u: f32, pal: &Palette) -> Option<f32> {
    let cy = y + h * 0.5;
    let col = if focus { pal.accent } else { pal.primary };
    rrect(x, cy - 3.0 * u, w, 6.0 * u, 3.0 * u, Color::new(1.0, 1.0, 1.0, 0.2));
    rrect(x, cy - 3.0 * u, (w * v).max(6.0 * u), 6.0 * u, 3.0 * u, col);
    draw_circle(x + w * v, cy, if focus { 9.0 } else { 7.0 } * u, WHITE);
    draw_circle_lines(x + w * v, cy, if focus { 12.0 } else { 10.0 } * u, 1.5 * u, col);
    if focus {
        rrect(x - 11.0 * u, y + 4.0 * u, 4.0 * u, h - 8.0 * u, 2.0 * u, pal.accent);
    }
    if is_mouse_button_down(MouseButton::Left) && hover(x - 14.0 * u, y, w + 28.0 * u, h) {
        let (mx, _) = mouse_position();
        return Some(((mx - x) / w).clamp(0.0, 1.0));
    }
    None
}

/// A square difficulty tile. Returns (hovered, clicked).
#[allow(clippy::too_many_arguments)]
pub fn diff_tile(x: f32, y: f32, s: f32, tier: usize, n_tiers: usize, on: bool, focus: bool, u: f32, pal: &Palette) -> (bool, bool) {
    let col = diff_color(tier, n_tiers, pal);
    let (hv, click) = tile(x, y, s, on, on && focus, col, 1.0, u);
    diff_icon(tier, n_tiers, x + s * 0.5, y + s * 0.5, s * 0.32, if on || hv { col } else { a(col, 0.7) });
    (hv, click)
}

/// A plain button with an optional icon. Returns (hovered, clicked).
#[allow(clippy::too_many_arguments)]
pub fn button(x: f32, y: f32, w: f32, h: f32, ic: Option<Icon>, label: &str, focus: bool, strong: bool, u: f32, pal: &Palette) -> (bool, bool) {
    let hv = hover(x, y, w, h);
    if strong {
        rrect(x, y, w, h, RADIUS * u, a(pal.secondary, if hv || focus { 0.62 } else { 0.4 }));
        rrect_lines(x, y, w, h, RADIUS * u, 2.0 * u, pal.accent);
    } else {
        frame(x, y, w, h, false, hv || focus, u, pal);
    }
    let size = if strong { 30.0 } else { 24.0 } * u;
    let tw = text_w(label, size);
    let iw = if ic.is_some() { h * 0.5 + 8.0 * u } else { 0.0 };
    let x0 = x + (w - tw - iw) * 0.5;
    let col = if hv || focus || strong { WHITE } else { Color::new(1.0, 1.0, 1.0, 0.7) };
    if let Some(ic) = ic {
        icon(ic, x0 + h * 0.22, y + h * 0.5, h * 0.22, if focus || hv { pal.accent } else { col });
    }
    text_left(label, x0 + iw, y + h * 0.5 + size * 0.33, size, col);
    if focus {
        rrect_lines(x - 4.0 * u, y - 4.0 * u, w + 8.0 * u, h + 8.0 * u, (RADIUS + 3.0) * u, 1.5 * u, Color::new(1.0, 1.0, 1.0, 0.75));
    }
    (hv, hv && is_mouse_button_pressed(MouseButton::Left))
}

/// The big go button: a solid colour with a soft glow that breathes.
#[allow(clippy::too_many_arguments)]
pub fn go_button(x: f32, y: f32, w: f32, h: f32, ic: Icon, label: &str, focus: bool, col: Color, u: f32, time: f32) -> (bool, bool) {
    let hv = hover(x, y, w, h);
    let breathe = 0.5 + 0.5 * (time * 3.0).sin();
    for k in 1..=3 {
        let g = k as f32 * 5.0 * u;
        rrect(x - g, y - g, w + 2.0 * g, h + 2.0 * g, 14.0 * u + g, a(col, (0.05 + 0.04 * breathe) * if hv || focus { 1.6 } else { 1.0 }));
    }
    let k = if hv { 0.95 } else { 0.8 };
    rrect(x, y, w, h, 14.0 * u, Color::new(col.r * k, col.g * k, col.b * k, 0.95));
    rrect(x + 4.0 * u, y + 4.0 * u, w - 8.0 * u, h * 0.42, 10.0 * u, Color::new(1.0, 1.0, 1.0, 0.12));
    rrect_lines(x, y, w, h, 14.0 * u, 2.0 * u, if focus || hv { WHITE } else { Color::new(1.0, 1.0, 1.0, 0.55) });
    let size = 38.0 * u;
    let tw = text_w(label, size);
    let iw = h * 0.5 + 6.0 * u;
    let x0 = x + (w - tw - iw) * 0.5;
    let ink = Color::new(0.02, 0.0, 0.07, 1.0);
    icon(ic, x0 + h * 0.2, y + h * 0.5, h * 0.2, ink);
    text_left(label, x0 + iw + 1.5 * u, y + h * 0.5 + size * 0.3 + 1.5 * u, size, Color::new(1.0, 1.0, 1.0, 0.35));
    text_left(label, x0 + iw, y + h * 0.5 + size * 0.3, size, ink);
    (hv, hv && is_mouse_button_pressed(MouseButton::Left))
}

/// A small heading above a group of controls.
pub fn heading(s: &str, x: f32, y: f32, u: f32, pal: &Palette) {
    text_left(&crate::render::spaced(s), x, y, 19.0 * u, Color::new(pal.primary.r, pal.primary.g, pal.primary.b, 0.95));
}

#[allow(dead_code)]
pub fn label_center(s: &str, x: f32, y: f32, size: f32, col: Color) {
    text_center(s, x, y, size, col);
}
