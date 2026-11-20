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
    }
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
        draw_rectangle(bx, by, w, h, Color::new(0.02, 0.0, 0.07, 0.94));
        draw_rectangle_lines(bx, by, w, h, 1.5 * u, a(pal.accent, 0.9));
        draw_rectangle(bx, by, 4.0 * u, h, pal.accent);
        text_left(&self.title, bx + pad, by + pad + 18.0 * u, 24.0 * u, pal.accent);
        for (i, l) in lines.iter().enumerate() {
            text_left(l, bx + pad, by + pad + 44.0 * u + i as f32 * 24.0 * u, 20.0 * u, Color::new(1.0, 1.0, 1.0, 0.88));
        }
    }
}

fn frame(x: f32, y: f32, w: f32, h: f32, on: bool, focus: bool, u: f32, pal: &Palette) {
    if on {
        draw_rectangle(x, y, w, h, a(pal.secondary, 0.38));
        draw_rectangle_lines(x, y, w, h, 2.0 * u, pal.accent);
    } else {
        draw_rectangle(x, y, w, h, Color::new(0.0, 0.0, 0.05, if focus { 0.6 } else { 0.42 }));
        draw_rectangle_lines(x, y, w, h, 1.0 * u, Color::new(1.0, 1.0, 1.0, if focus { 0.55 } else { 0.18 }));
    }
}

/// A row with an icon and a label. Returns (hovered, clicked).
#[allow(clippy::too_many_arguments)]
pub fn icon_row(x: f32, y: f32, w: f32, h: f32, ic: Icon, label: &str, selected: bool, u: f32, pal: &Palette, time: f32) -> (bool, bool) {
    let hv = hover(x, y, w, h);
    if selected {
        let pulse = 0.85 + 0.15 * (time * 4.0).sin();
        draw_rectangle(x, y, w, h, a(pal.secondary, 0.3));
        draw_rectangle_lines(x, y, w, h, 1.0 * u, a(pal.secondary, 0.6));
        draw_rectangle(x, y, 4.0 * u, h, a(pal.accent, pulse));
    } else {
        draw_rectangle(x, y, w, h, Color::new(0.0, 0.0, 0.05, 0.42));
        draw_line(x, y + h, x + w * 0.7, y + h, 1.0 * u, Color::new(1.0, 1.0, 1.0, 0.14));
    }
    let col = if selected { WHITE } else { Color::new(1.0, 1.0, 1.0, 0.75) };
    icon(ic, x + 36.0 * u, y + h * 0.5, h * 0.27, if selected { pal.accent } else { col });
    text_left(label, x + 70.0 * u, y + h * 0.5 + 9.0 * u, 28.0 * u, col);
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
        draw_rectangle(x - 10.0 * u, y, 3.0 * u, h, pal.accent);
    }
    (click, hov)
}

/// A horizontal slider over 0..1. Returns the new value while it is being dragged.
#[allow(clippy::too_many_arguments)]
pub fn slider(x: f32, y: f32, w: f32, h: f32, v: f32, focus: bool, u: f32, pal: &Palette) -> Option<f32> {
    let cy = y + h * 0.5;
    let col = if focus { pal.accent } else { pal.primary };
    draw_rectangle(x, cy - 1.5 * u, w, 3.0 * u, Color::new(1.0, 1.0, 1.0, 0.22));
    draw_rectangle(x, cy - 1.5 * u, w * v, 3.0 * u, col);
    draw_circle(x + w * v, cy, if focus { 8.0 } else { 6.0 } * u, WHITE);
    draw_circle_lines(x + w * v, cy, if focus { 11.0 } else { 9.0 } * u, 1.5 * u, col);
    if focus {
        draw_rectangle(x - 10.0 * u, y, 3.0 * u, h, pal.accent);
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
    let hv = hover(x, y, s, s);
    let col = diff_color(tier, n_tiers, pal);
    draw_rectangle(x, y, s, s, if on { a(col, 0.28) } else { Color::new(0.0, 0.0, 0.05, 0.5) });
    draw_rectangle_lines(x, y, s, s, if on { 3.0 } else { 1.0 } * u, if on { col } else { a(col, if hv { 0.9 } else { 0.4 }) });
    diff_icon(tier, n_tiers, x + s * 0.5, y + s * 0.5, s * 0.32, if on || hv { col } else { a(col, 0.7) });
    if on && focus {
        draw_rectangle(x, y + s + 5.0 * u, s, 3.0 * u, pal.accent);
    }
    (hv, hv && is_mouse_button_pressed(MouseButton::Left))
}

/// A plain button with an optional icon. Returns (hovered, clicked).
#[allow(clippy::too_many_arguments)]
pub fn button(x: f32, y: f32, w: f32, h: f32, ic: Option<Icon>, label: &str, focus: bool, strong: bool, u: f32, pal: &Palette) -> (bool, bool) {
    let hv = hover(x, y, w, h);
    if strong {
        draw_rectangle(x, y, w, h, a(pal.secondary, if hv || focus { 0.55 } else { 0.32 }));
        draw_rectangle_lines(x, y, w, h, 2.0 * u, pal.accent);
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
        draw_rectangle(x - 10.0 * u, y, 3.0 * u, h, pal.accent);
    }
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
