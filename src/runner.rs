//! Highway: a side mode. A three-lane run down the grid toward the horizon, built from the same
//! analysis as the main game: kicks put walls in a lane (move out of it), snares lay a bar across
//! the road (jump it), hats string gems along a lane, chimes drop stars. Everything arrives at
//! the player's line exactly on its sound.

use crate::analysis::{Analysis, Band};
use crate::rng::Rng;

/// Seconds a note is visible before it reaches the player.
pub const LEAD: f32 = 2.1;
/// A jump keeps the craft off the road this long.
pub const AIR: f32 = 0.46;
/// A jump started within this many seconds of the ideal moment is "perfect".
const PERFECT: f32 = 0.085;
const START_HP: i32 = 5;
const INVULN: f32 = 0.9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoteKind {
    /// Fills one lane: be in another.
    Wall,
    /// Lies across the whole road: be in the air.
    Bar,
    /// Collect by being in its lane.
    Gem,
    /// A chime: a gem worth much more.
    Star,
}

#[derive(Clone, Copy, Debug)]
pub struct Note {
    pub t: f32,
    /// -1, 0 or 1 (ignored by `Bar`).
    pub lane: i8,
    pub kind: NoteKind,
    /// 0 = coming, 1 = cleared or collected, 2 = hit or missed.
    pub state: u8,
}

/// Sounds the app should play this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunFx {
    Gem,
    Star,
    Clear,
    Perfect,
    Hit,
    Jump,
    Lane,
}

pub struct Runner {
    pub notes: Vec<Note>,
    pub beats: Vec<f32>,
    pub downbeat: usize,
    pub duration: f32,
    pub t: f32,
    /// Lane the craft is heading for (-1, 0, 1) and where it is drawn (eased).
    pub lane: i8,
    pub lane_x: f32,
    /// Song time the current jump started, if airborne.
    pub jump_t0: Option<f32>,
    pub hp: i32,
    pub max_hp: i32,
    pub invuln: f32,
    pub score: u64,
    pub combo: u32,
    pub best_combo: u32,
    pub gems: u32,
    pub perfects: u32,
    pub hits: u32,
    pub cleared: u32,
    /// 0..1 envelopes for the renderer.
    pub flash: f32,
    pub glow: f32,
    pub dead: bool,
    pub done: bool,
    pub fx: Vec<RunFx>,
    /// Last judgement shown over the craft, and how long it has left.
    pub label: &'static str,
    pub label_t: f32,
    cursor: usize,
}

impl Runner {
    /// Build the run for a song. `diff` is the usual difficulty multiplier (0.7 .. 3.2).
    pub fn new(an: &Analysis, diff: f32) -> Runner {
        let mut rng = Rng::new(an.seed ^ 0x4849_4748_5741_59);
        let beat = if an.bpm > 1.0 { 60.0 / an.bpm } else { 0.5 };
        let k = diff.clamp(0.7, 3.2).sqrt();
        let t_start = an.silences.first().filter(|s| s.0 <= 0.01).map(|s| s.1).unwrap_or(0.0) + 3.0;
        let ok = |t: f32| t > t_start && t < an.duration - 2.0 && !an.in_silence(t);
        let mut notes: Vec<Note> = Vec::new();

        // Snares: bars to jump. Never closer together than a jump and its landing.
        let bar_gap = (0.95 / k).max(AIR + 0.22);
        let mut last = -10.0f32;
        for o in an.onsets.iter().filter(|o| o.band == Band::Mid && o.strength > 0.45) {
            if ok(o.t) && o.t - last >= bar_gap {
                last = o.t;
                notes.push(Note { t: o.t, lane: 0, kind: NoteKind::Bar, state: 0 });
            }
        }
        let bars: Vec<f32> = notes.iter().map(|n| n.t).collect();
        let near_bar = |t: f32, w: f32| bars.iter().any(|&b| (b - t).abs() < w);

        // Kicks: a wall in one lane. Two in a row never share a lane, so there is always a way
        // through without crossing the whole road in one beat.
        let wall_gap = (0.62 / k).max(0.3);
        let mut last = -10.0f32;
        let mut prev_lane = 0i8;
        let mut walls: Vec<(f32, i8)> = Vec::new();
        for o in an.onsets.iter().filter(|o| o.band == Band::Low && o.strength > 0.3) {
            if ok(o.t) && o.t - last >= wall_gap && !near_bar(o.t, 0.2) {
                last = o.t;
                let mut lane = rng.below(3) as i8 - 1;
                if lane == prev_lane {
                    lane = if lane == 1 { 0 } else { lane + 1 };
                }
                prev_lane = lane;
                walls.push((o.t, lane));
                notes.push(Note { t: o.t, lane, kind: NoteKind::Wall, state: 0 });
            }
        }
        // On the harder tiers the downbeat wall doubles up, leaving one lane open.
        if diff >= 1.7 {
            let extra: Vec<Note> = walls
                .iter()
                .enumerate()
                .filter(|(i, _)| i % 4 == 0)
                .map(|(_, &(t, lane))| Note { t, lane: if lane == 0 { 1 } else { -lane }, kind: NoteKind::Wall, state: 0 })
                .collect();
            notes.extend(extra);
        }
        let wall_at = |t: f32, lane: i8| notes.iter().any(|n| n.kind == NoteKind::Wall && n.lane == lane && (n.t - t).abs() < 0.16);

        // Hats: gems in runs along one lane, changing lane every few so the player weaves.
        let mut gems: Vec<Note> = Vec::new();
        let mut lane = 0i8;
        let mut run = 0;
        let mut last = -10.0f32;
        for o in an.onsets.iter().filter(|o| o.band == Band::High) {
            if !ok(o.t) || o.t - last < (0.5 * beat).clamp(0.16, 0.3) {
                continue;
            }
            last = o.t;
            if run == 0 {
                run = 4 + rng.below(5);
                lane = (lane + if rng.chance(0.5) { 1 } else { 2 } + 1).rem_euclid(3) - 1;
            }
            run -= 1;
            if !wall_at(o.t, lane) {
                gems.push(Note { t: o.t, lane, kind: NoteKind::Gem, state: 0 });
            }
        }
        // Chimes: stars, off to whichever side the note's pitch leans.
        let mut last = -10.0f32;
        for c in &an.chimes {
            if ok(c.t) && c.t - last > 1.2 && c.strength > 0.6 {
                last = c.t;
                let lane = if c.pitch < 0.33 { -1 } else if c.pitch > 0.66 { 1 } else { 0 };
                if !wall_at(c.t, lane) {
                    gems.push(Note { t: c.t, lane, kind: NoteKind::Star, state: 0 });
                }
            }
        }
        notes.extend(gems);
        notes.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
        Runner {
            notes,
            beats: an.beats.clone(),
            downbeat: an.downbeat,
            duration: an.duration,
            t: 0.0,
            lane: 0,
            lane_x: 0.0,
            jump_t0: None,
            hp: START_HP,
            max_hp: START_HP,
            invuln: 0.0,
            score: 0,
            combo: 0,
            best_combo: 0,
            gems: 0,
            perfects: 0,
            hits: 0,
            cleared: 0,
            flash: 0.0,
            glow: 0.0,
            dead: false,
            done: false,
            fx: Vec::new(),
            label: "",
            label_t: 0.0,
            cursor: 0,
        }
    }

    /// 0 on the road .. 1 at the top of a jump.
    pub fn height(&self) -> f32 {
        match self.jump_t0 {
            Some(t0) => (std::f32::consts::PI * ((self.t - t0) / AIR).clamp(0.0, 1.0)).sin(),
            None => 0.0,
        }
    }

    fn airborne_at(&self, t: f32) -> bool {
        self.jump_t0.is_some_and(|t0| t >= t0 + 0.04 && t <= t0 + AIR - 0.04)
    }

    fn mult(&self) -> u64 {
        1 + (self.combo / 20).min(7) as u64
    }

    fn say(&mut self, s: &'static str) {
        self.label = s;
        self.label_t = 0.7;
    }

    fn hurt(&mut self) {
        if self.invuln > 0.0 {
            return;
        }
        self.hp -= 1;
        self.hits += 1;
        self.combo = 0;
        self.invuln = INVULN;
        self.flash = 1.0;
        self.fx.push(RunFx::Hit);
        self.say("HIT");
        if self.hp <= 0 {
            self.dead = true;
            self.done = true;
        }
    }

    /// Advance to song time `t`. `steer` is -1 / 0 / +1 for a lane change pressed this frame,
    /// `jump` is true on the frame the jump key went down.
    pub fn step(&mut self, dt: f32, t: f32, steer: i32, jump: bool) {
        self.fx.clear();
        if self.done {
            return;
        }
        self.t = t;
        self.invuln = (self.invuln - dt).max(0.0);
        self.flash = (self.flash - dt * 3.0).max(0.0);
        self.glow = (self.glow - dt * 4.0).max(0.0);
        self.label_t = (self.label_t - dt).max(0.0);
        if steer != 0 {
            let to = (self.lane as i32 + steer).clamp(-1, 1) as i8;
            if to != self.lane {
                self.lane = to;
                self.fx.push(RunFx::Lane);
            }
        }
        self.lane_x += (self.lane as f32 - self.lane_x) * (dt * 22.0).min(1.0);
        if self.jump_t0.is_some_and(|t0| t > t0 + AIR) {
            self.jump_t0 = None;
        }
        if jump && self.jump_t0.is_none() {
            self.jump_t0 = Some(t);
            self.fx.push(RunFx::Jump);
        }
        // Judge every note whose moment has come.
        while self.cursor < self.notes.len() && self.notes[self.cursor].t <= t {
            let i = self.cursor;
            self.cursor += 1;
            let n = self.notes[i];
            // Which lane the craft is actually in: it counts as moved once it is most of the way.
            let in_lane = (self.lane_x - n.lane as f32).abs() < 0.55;
            match n.kind {
                NoteKind::Wall => {
                    if in_lane {
                        self.notes[i].state = 2;
                        self.hurt();
                    } else {
                        self.notes[i].state = 1;
                        self.cleared += 1;
                        self.combo += 1;
                        self.score += 20 * self.mult();
                    }
                }
                NoteKind::Bar => {
                    if self.airborne_at(n.t) {
                        self.notes[i].state = 1;
                        self.cleared += 1;
                        self.combo += 2;
                        // Perfect: the bar passes under the top of the jump.
                        let apex = self.jump_t0.unwrap_or(n.t) + AIR * 0.5;
                        if (apex - n.t).abs() <= PERFECT {
                            self.perfects += 1;
                            self.score += 250 * self.mult();
                            self.glow = 1.0;
                            self.fx.push(RunFx::Perfect);
                            self.say("PERFECT");
                        } else {
                            self.score += 100 * self.mult();
                            self.fx.push(RunFx::Clear);
                            self.say("CLEAR");
                        }
                    } else {
                        self.notes[i].state = 2;
                        self.hurt();
                    }
                }
                NoteKind::Gem | NoteKind::Star => {
                    if in_lane {
                        self.notes[i].state = 1;
                        self.gems += 1;
                        self.combo += 1;
                        let star = n.kind == NoteKind::Star;
                        self.score += if star { 400 } else { 50 } * self.mult();
                        self.glow = self.glow.max(if star { 1.0 } else { 0.4 });
                        self.fx.push(if star { RunFx::Star } else { RunFx::Gem });
                    } else {
                        self.notes[i].state = 2;
                    }
                }
            }
            self.best_combo = self.best_combo.max(self.combo);
        }
        if t >= self.duration - 0.5 {
            self.done = true;
        }
    }

    /// A simple autopilot for screenshots and tests: steer out of the next wall's lane toward
    /// the next gem, jump so the next bar passes under the top of the jump.
    pub fn autopilot(&self) -> (i32, bool) {
        let mut want = self.lane;
        let ahead: Vec<&Note> = self.notes[self.cursor..].iter().take_while(|n| n.t < self.t + 0.9).collect();
        if let Some(g) = ahead.iter().find(|n| matches!(n.kind, NoteKind::Gem | NoteKind::Star)) {
            want = g.lane;
        }
        let blocked = |lane: i8| ahead.iter().any(|n| n.kind == NoteKind::Wall && n.lane == lane && n.t < self.t + 0.45);
        if blocked(want) {
            want = [self.lane, 0, -1, 1].into_iter().find(|&l| !blocked(l)).unwrap_or(want);
        }
        let steer = (want as i32 - self.lane as i32).signum();
        let jump = ahead.iter().any(|n| n.kind == NoteKind::Bar && (n.t - self.t - AIR * 0.5).abs() < 0.02);
        (steer, jump)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    /// A 120 BPM groove: kick on every beat, a snare burst on 2 and 4, hats on the off-beats.
    fn groove(secs: f32) -> Vec<f32> {
        let sr = 44_100.0f32;
        let tau = std::f32::consts::TAU;
        let mut seed = 99u32;
        let mut x = vec![0.0f32; (secs * sr) as usize];
        for (i, s) in x.iter_mut().enumerate() {
            let t = i as f32 / sr;
            let tb = t % 0.5;
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (seed >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
            *s += (tau * 55.0 * tb).sin() * (-tb * 18.0).exp() * 0.7;
            let ts = (t + 0.5) % 1.0;
            *s += noise * (-ts * 30.0).exp() * 0.4 * (tau * 900.0 * ts).sin().abs();
            let th = (t + 0.25) % 0.5;
            *s += noise * (-th * 90.0).exp() * 0.12;
        }
        x
    }

    #[test]
    fn the_road_is_always_passable_and_the_autopilot_survives() {
        let an = crate::analysis::analyze(&groove(40.0), 44_100, &AtomicU32::new(0));
        for diff in [0.7f32, 1.35, 3.2] {
            let mut r = Runner::new(&an, diff);
            assert!(r.notes.iter().any(|n| n.kind == NoteKind::Wall) && r.notes.iter().any(|n| n.kind == NoteKind::Bar), "no obstacles at {diff}");
            // never all three lanes walled at once, never a gem inside a wall
            for n in r.notes.iter().filter(|n| n.kind == NoteKind::Wall) {
                let lanes: std::collections::HashSet<i8> = r.notes.iter().filter(|m| m.kind == NoteKind::Wall && (m.t - n.t).abs() < 0.1).map(|m| m.lane).collect();
                assert!(lanes.len() < 3, "all lanes walled at {}", n.t);
            }
            for g in r.notes.iter().filter(|n| n.kind == NoteKind::Gem) {
                assert!(!r.notes.iter().any(|m| m.kind == NoteKind::Wall && m.lane == g.lane && (m.t - g.t).abs() < 0.1), "gem in a wall at {}", g.t);
            }
            let dt = 1.0 / 120.0;
            let mut t = 0.0;
            while t < an.duration && !r.done {
                let (steer, jump) = r.autopilot();
                r.step(dt, t, steer, jump);
                t += dt;
            }
            assert!(!r.dead, "the autopilot died at difficulty {diff} (hits {})", r.hits);
            assert!(r.cleared > 20 && r.score > 0, "cleared {} score {}", r.cleared, r.score);
        }
    }
}
