//! The simulation: player, the Heart, bullets, enemies, tiles, lasers, shockwaves, pickups,
//! upgrades and scoring. No rendering here.

use macroquad::math::{vec2, Vec2};
use std::f32::consts::TAU;

use crate::analysis::{Feat, SectionKind, N_LAYERS};
use crate::engine::Sfx;
use crate::level::*;
use crate::rng::Rng;

pub const HEART_R: f32 = 62.0;
pub const PLAYER_R: f32 = 11.0;
const PLAYER_HIT_R: f32 = 5.0;
const MAX_BULLETS: usize = 1500;
const MAX_ENEMIES: usize = 30;
const MAX_PARTS: usize = 2600;

// Palette slots used by particles / floating text.
pub const C_PRIMARY: u8 = 0;
pub const C_SECONDARY: u8 = 1;
pub const C_ACCENT: u8 = 2;
pub const C_DANGER: u8 = 3;
pub const C_WHITE: u8 = 4;

#[derive(Clone, Copy, Default)]
pub struct Input {
    pub mv: Vec2,
    pub aim: Vec2,
    pub fire: bool,
    pub dash: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpgradeId {
    Tempo,
    Chord,
    Gain,
    Pierce,
    Sustain,
    Glide,
    Phase,
    Magnet,
    Echo,
    Encore,
    /// Rare: shots bounce off the arena walls.
    Ricochet,
    /// Rare: the first shot after every downbeat is charged.
    Downbeat,
    /// Rare: dashing turns nearby bullets into shootable notes.
    Transpose,
    /// Rare: blades circle the player, cutting enemies and shootable notes.
    Orbit,
    /// Being hit clears every bullet nearby and hurts enemies.
    Nova,
    /// Damage grows with the combo multiplier.
    Crescendo,
    /// A chance for a shot to land much harder.
    Accent,
    /// Breaking a Heart phase heals.
    Coda,
    /// A dash started on the beat costs no cooldown.
    Syncope,
}

/// Number of upgrades (`Player::ups` is indexed by `UpgradeId as usize`).
pub const N_UPGRADES: usize = 19;

impl UpgradeId {
    pub const ALL: [UpgradeId; N_UPGRADES] = [
        UpgradeId::Tempo,
        UpgradeId::Chord,
        UpgradeId::Gain,
        UpgradeId::Pierce,
        UpgradeId::Sustain,
        UpgradeId::Glide,
        UpgradeId::Phase,
        UpgradeId::Magnet,
        UpgradeId::Echo,
        UpgradeId::Encore,
        UpgradeId::Ricochet,
        UpgradeId::Downbeat,
        UpgradeId::Transpose,
        UpgradeId::Orbit,
        UpgradeId::Nova,
        UpgradeId::Crescendo,
        UpgradeId::Accent,
        UpgradeId::Coda,
        UpgradeId::Syncope,
    ];
    /// Rare upgrades are only offered for an A or S grade (`World::spawn_reward`).
    pub fn rare(self) -> bool {
        matches!(self, UpgradeId::Ricochet | UpgradeId::Downbeat | UpgradeId::Transpose | UpgradeId::Orbit)
    }
    pub fn name(self) -> &'static str {
        match self {
            UpgradeId::Tempo => "TEMPO",
            UpgradeId::Chord => "CHORD",
            UpgradeId::Gain => "GAIN",
            UpgradeId::Pierce => "PIERCE",
            UpgradeId::Sustain => "SUSTAIN",
            UpgradeId::Glide => "GLIDE",
            UpgradeId::Phase => "PHASE",
            UpgradeId::Magnet => "MAGNET",
            UpgradeId::Echo => "ECHO",
            UpgradeId::Encore => "ENCORE",
            UpgradeId::Ricochet => "RICOCHET",
            UpgradeId::Downbeat => "DOWNBEAT",
            UpgradeId::Transpose => "TRANSPOSE",
            UpgradeId::Orbit => "ORBIT",
            UpgradeId::Nova => "NOVA",
            UpgradeId::Crescendo => "CRESCENDO",
            UpgradeId::Accent => "ACCENT",
            UpgradeId::Coda => "CODA",
            UpgradeId::Syncope => "SYNCOPE",
        }
    }
    pub fn desc(self) -> &'static str {
        match self {
            UpgradeId::Tempo => "fire on faster beat divisions",
            UpgradeId::Chord => "+1 shot per volley",
            UpgradeId::Gain => "+50% shot damage",
            UpgradeId::Pierce => "shots pierce +1",
            UpgradeId::Sustain => "+1 max HP, heal 2",
            UpgradeId::Glide => "+12% move speed",
            UpgradeId::Phase => "dash recharges faster",
            UpgradeId::Magnet => "bigger pickup range",
            UpgradeId::Echo => "drone that fires on the beat",
            UpgradeId::Encore => "shield, recharges each section",
            UpgradeId::Ricochet => "shots bounce off the walls",
            UpgradeId::Downbeat => "first shot of every bar is charged",
            UpgradeId::Transpose => "dash turns nearby bullets to notes",
            UpgradeId::Orbit => "blades circle you and cut what they touch",
            UpgradeId::Nova => "being hit clears bullets and hurts enemies",
            UpgradeId::Crescendo => "more damage the higher your multiplier",
            UpgradeId::Accent => "chance for a shot to hit 2.5x as hard",
            UpgradeId::Coda => "breaking a Heart phase heals you",
            UpgradeId::Syncope => "a dash on the beat has no cooldown",
        }
    }
    pub fn max(self) -> u8 {
        match self {
            UpgradeId::Tempo => 4,
            UpgradeId::Chord => 4,
            UpgradeId::Gain => 6,
            UpgradeId::Pierce => 3,
            UpgradeId::Sustain => 5,
            UpgradeId::Glide => 4,
            UpgradeId::Phase => 3,
            UpgradeId::Magnet => 3,
            UpgradeId::Echo => 3,
            UpgradeId::Encore => 1,
            UpgradeId::Ricochet => 2,
            UpgradeId::Downbeat => 3,
            UpgradeId::Transpose => 1,
            UpgradeId::Orbit => 2,
            UpgradeId::Nova => 2,
            UpgradeId::Crescendo => 3,
            UpgradeId::Accent => 3,
            UpgradeId::Coda => 2,
            UpgradeId::Syncope => 2,
        }
    }
}

/// Beat subdivisions the gun may lock to when there is no gun track (live audio, drift). The
/// target seconds between shots per TEMPO level are `level::GUN_INTERVALS`.
const FIRE_DIVS: [f32; 9] = [0.5, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0];
/// Shots at least this much heavier than nominal get a soft tick (`Sfx::Shot`).
const GUN_TICK_FROM: f32 = 1.2;

// ---- difficulty ramp ("pressure") -----------------------------------------------------------
// The level generator plans every song at full density for the chosen difficulty. At run time
// the World scales that plan by `pressure()`: a ramp that eases the player into each song,
// grows a little with every cleared track, and backs off for a few seconds after a hit.

/// (starting pressure, seconds of play until full pressure) for the 1st, 2nd and later songs.
const RAMP: [(f32, f32); 3] = [(0.40, 75.0), (0.70, 30.0), (0.85, 15.0)];
/// Extra pressure per cleared song in a run.
const RUN_ESCALATION: f32 = 0.14;
/// Extra pressure per player level: the level keeps up with a strong build.
const LEVEL_PRESSURE: f32 = 0.02;
/// Extra pressure per rage step (each time a Heart phase is broken early in this song).
const RAGE_PRESSURE: f32 = 0.10;
const RAGE_MAX: u32 = 5;
/// Live audio: seconds of play a phase is sized for, out of a Heart sized for this many.
const LIVE_PHASE_SECS: f32 = 30.0;
pub const LIVE_HEART_SECS: f32 = 210.0;
/// Purgatory lasts this long at most; if the section is still going the Heart returns.
const PURGATORY_SECS: f32 = 9.0;
/// How much of the player's measured damage rate the Heart's return and its later phases are
/// sized for (below 1 so a strong player still gets ahead, just not by a whole phase).
const RAGE_KEEP_UP: f32 = 0.8;
/// Shards needed for the next level: `XP_BASE + XP_LIN * level + XP_QUAD * level^2`.
const XP_BASE: u32 = 18;
const XP_LIN: u32 = 14;
const XP_QUAD: u32 = 3;
/// The Heart's health grows with the player's damage output to this power (1 = fully keeps up).
const HEART_POWER_EXP: f32 = 0.8;
/// Pressure multiplier right after the player is hit, and how fast it recovers (per second).
const MERCY_DROP: f32 = 0.55;
const MERCY_RECOVER: f32 = 0.05;
/// Mechanics switch on as pressure passes these thresholds (so a run introduces them in order).
/// Harder tiers unlock them sooner: see `World::gate`.
const P_SPAWN: f32 = 0.44;
const P_TILES: f32 = 0.55;
const P_WAVE: f32 = 0.60;
const P_LASER: f32 = 0.66;
const P_ELITE: f32 = 0.75;
const P_TICK: f32 = 0.80;
/// Seconds of invulnerability after a hit, and the radius in which bullets are cleared.
const HIT_INVULN: f32 = 1.6;
const HIT_CLEAR_R: f32 = 220.0;

// Hit sources (index into `Stats::by_src`).
pub const SRC_BULLET: usize = 0;
pub const SRC_ENEMY: usize = 1;
pub const SRC_TILE: usize = 2;
pub const SRC_LASER: usize = 3;
pub const SRC_WAVE: usize = 4;
pub const SRC_HEART: usize = 5;

fn tile_tag(p: TilePattern) -> u8 {
    match p {
        TilePattern::Checker(..) => 0,
        TilePattern::Rows(..) => 1,
        TilePattern::Cols(..) => 2,
        TilePattern::Ring(..) => 3,
        TilePattern::Cross(..) => 4,
        TilePattern::Diag(..) => 5,
        TilePattern::Scatter(..) => 6,
        TilePattern::ColBand(..) => 7,
        TilePattern::RowBand(..) => 8,
    }
}

fn smooth01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Decay rate (per second) of the `World::hit` drum envelopes.
const HIT_DECAY: f32 = 5.5;
/// Groove envelope: speed = 1 + amt * (PEAK * exp(-DECAY * beat_phase) - MEAN). MEAN is the
/// average of the exponential term over one beat, so the average speed stays exactly 1.
const GROOVE_PEAK: f32 = 2.0;
const GROOVE_DECAY: f32 = 5.0;
const GROOVE_MEAN: f32 = GROOVE_PEAK * 0.198_652; // PEAK * (1 - e^-5) / 5

pub struct Player {
    pub pos: Vec2,
    pub vel: Vec2,
    pub aim_dir: Vec2,
    pub hp: i32,
    pub max_hp: i32,
    pub invuln: f32,
    pub dash_t: f32,
    pub dash_cd: f32,
    pub dash_cd_max: f32,
    pub dash_dir: Vec2,
    pub shield: bool,
    pub ups: [u8; N_UPGRADES],
    pub xp: u32,
    pub xp_next: u32,
    pub level: u32,
    fire_phase: i64,
    pub drone_ang: f32,
    pub muzzle: f32,
}

impl Player {
    fn new() -> Self {
        Player {
            pos: vec2(0.0, 250.0),
            vel: Vec2::ZERO,
            aim_dir: vec2(0.0, -1.0),
            hp: 6,
            max_hp: 6,
            invuln: 0.0,
            dash_t: 0.0,
            dash_cd: 0.0,
            dash_cd_max: 1.1,
            dash_dir: vec2(0.0, -1.0),
            shield: false,
            ups: [0; N_UPGRADES],
            xp: 0,
            xp_next: XP_BASE,
            level: 0,
            fire_phase: 0,
            drone_ang: 0.0,
            muzzle: 0.0,
        }
    }
    pub fn up(&self, id: UpgradeId) -> u8 {
        self.ups[id as usize]
    }
    pub fn magnet_r(&self) -> f32 {
        150.0 * (1.0 + 0.6 * self.up(UpgradeId::Magnet) as f32)
    }
    pub fn drones(&self) -> usize {
        self.up(UpgradeId::Echo) as usize
    }
    /// ORBIT: how many blades circle the player, and where blade `k` is.
    pub fn blades(&self) -> usize {
        2 * self.up(UpgradeId::Orbit) as usize
    }
    pub fn blade_pos(&self, k: usize) -> Vec2 {
        let a = -self.drone_ang * 1.7 + k as f32 * TAU / self.blades().max(1) as f32;
        self.pos + vec2(a.cos(), a.sin()) * BLADE_ORBIT_R
    }
    pub fn drone_pos(&self, k: usize) -> Vec2 {
        let n = self.drones().max(1) as f32;
        let a = self.drone_ang + k as f32 * TAU / n;
        self.pos + vec2(a.cos(), a.sin()) * 44.0
    }
}

/// Hot round bullet: must be dodged.
pub const B_ORB: u8 = 0;
/// White square "note": can be shot down (`soft == true`).
pub const B_NOTE: u8 = 1;
/// Small fast dart fired on hi-hats: must be dodged.
pub const B_TICK: u8 = 2;

pub struct Bullet {
    pub pos: Vec2,
    pub vel: Vec2,
    pub r: f32,
    pub soft: bool,
    pub age: f32,
    pub fade: f32,
    /// One of `B_ORB`, `B_NOTE`, `B_TICK` (visual identity; `soft` decides whether it can be shot).
    pub kind: u8,
    /// What fired it (pattern index, 9 = hi-hat dart, 10 = orbiter, 11 = elite); for balance stats.
    pub pat: u8,
    /// The player has already been paid for slipping past this one.
    pub grazed: bool,
}

/// ORBIT blades: orbit radius, reach, and damage per second to what they touch.
pub const BLADE_ORBIT_R: f32 = 68.0;
const BLADE_REACH: f32 = 17.0;
const BLADE_DPS: f32 = 7.0;
/// ACCENT: chance per level, and what a hit is multiplied by.
const ACCENT_CHANCE: f32 = 0.12;
const ACCENT_MUL: f32 = 2.5;
/// SYNCOPE: a dash started within this many seconds of a beat is "on the beat".
const SYNCOPE_WINDOW: f32 = 0.09;
/// Cymbal (bomber): arms within this distance; its ring has this many bullets.
const BOMBER_ARM_R: f32 = 175.0;
const BOMBER_RING: usize = 10;

/// Elite kinds, kept in `Enemy::state`: the Gunner fires a ring every couple of beats, the
/// Summoner calls two minis on every other downbeat, the Leaper blinks toward the player on
/// every downbeat.
pub const ELITE_GUNNER: f32 = 0.0;
pub const ELITE_SUMMONER: f32 = 1.0;
pub const ELITE_LEAPER: f32 = 2.0;

/// Slipping past a hostile bullet this close (without being hit) is a graze.
const GRAZE_R: f32 = 34.0;
const GRAZE_SCORE: f32 = 30.0;

/// A Heart pulse that has been announced but has not fired yet. It fires at `t`; the announcement
/// started at `t - lead`. Draw it as an approach ring closing on the Heart.
pub struct PendingPulse {
    pub t: f32,
    pub lead: f32,
    pub pattern: Pattern,
    pub count: u16,
    pub speed: f32,
    pub rot: f32,
    pub soft: bool,
    /// Angle (radians) from the Heart to the player, locked when the pulse was announced.
    /// Aimed patterns (`Pattern::aimed`) are laid out around it.
    pub aim: f32,
}

/// Performance since the last graded reward.
#[derive(Clone, Copy, Default)]
pub struct GradeWindow {
    pub hits: u32,
    pub broke_phase: bool,
    pub peak_mult: f32,
}

/// Grades, worst to best, and what each one offers.
pub const GRADES: [&str; 4] = ["C", "B", "A", "S"];

/// Run modifiers: one more is switched on with every depth (song) of a run.
pub const MODS: [(&str, &str); 5] = [
    ("ALLEGRO", "bullets fly 15% faster"),
    ("FORTE", "pulses carry 20% more bullets"),
    ("SWARM", "more enemies at once"),
    ("NO MERCY", "no breather after a hit"),
    ("GLASS", "two lives, 60% more damage"),
];
/// How many of `MODS` depth hands out by itself (the rest are only chosen in run setup).
pub const DEPTH_MODS: usize = 4;
pub const MOD_GLASS: u8 = 16;
/// Each mutator chosen in run setup multiplies the score by this much more.
pub const MUTATOR_SCORE: f32 = 0.15;
pub const MOD_ALLEGRO: u8 = 1;
pub const MOD_FORTE: u8 = 2;
pub const MOD_SWARM: u8 = 4;
pub const MOD_NO_MERCY: u8 = 8;

/// A run of notes drawn across the arena: a comet that leaves shards behind it.
#[derive(Clone, Copy, Debug)]
pub struct Painter {
    pub t0: f32,
    pub t1: f32,
    pub from: Vec2,
    pub to: Vec2,
    pub pos: Vec2,
    next_drop: f32,
}

pub struct Shot {
    /// Wall bounces left (RICOCHET).
    pub bounce: u8,
    pub pos: Vec2,
    pub vel: Vec2,
    pub dmg: f32,
    pub pierce: u8,
    pub life: f32,
    pub r: f32,
}

pub struct Enemy {
    pub kind: EnemyKind,
    pub pos: Vec2,
    pub vel: Vec2,
    pub hp: f32,
    pub max_hp: f32,
    pub r: f32,
    pub rot: f32,
    pub age: f32,
    pub flash: f32,
    pub state: f32,
}

pub struct Portal {
    pub kind: EnemyKind,
    pub pos: Vec2,
    pub t0: f32,
    pub t1: f32,
}

pub struct Laser {
    pub horizontal: bool,
    pub from: f32,
    pub to: f32,
    pub t0: f32,
    pub dur: f32,
    pub lead: f32,
}

impl Laser {
    pub fn pos_at(&self, t: f32) -> f32 {
        let k = ((t - self.t0) / self.dur.max(0.01)).clamp(0.0, 1.0);
        self.from + (self.to - self.from) * k
    }
    pub fn active(&self, t: f32) -> bool {
        t >= self.t0 && t <= self.t0 + self.dur
    }
}

pub struct Wave {
    /// Where the Heart was when it let the wave go.
    pub origin: Vec2,
    pub t0: f32,
    pub lead: f32,
    pub gap: f32,
    pub gaps: u8,
    pub speed: f32,
}

impl Wave {
    pub fn radius(&self, t: f32) -> f32 {
        HEART_R + (t - self.t0).max(0.0) * self.speed
    }
    pub fn in_gap(&self, ang: f32) -> bool {
        let n = self.gaps.max(1) as f32;
        let half = 0.36;
        for k in 0..self.gaps.max(1) {
            let c = self.gap + k as f32 * TAU / n;
            let mut d = (ang - c).rem_euclid(TAU);
            if d > std::f32::consts::PI {
                d -= TAU;
            }
            if d.abs() < half {
                return true;
            }
        }
        false
    }
}

pub struct Shard {
    pub pos: Vec2,
    pub vel: Vec2,
    pub life: f32,
    pub value: u8,
    pub heal: bool,
    pub age: f32,
}

pub struct Orb {
    pub pos: Vec2,
    pub up: UpgradeId,
    pub rare: bool,
    pub life: f32,
    pub age: f32,
}

pub struct Particle {
    pub pos: Vec2,
    pub vel: Vec2,
    pub life: f32,
    pub max: f32,
    pub size: f32,
    pub col: u8,
    pub rot: f32,
}

pub struct FloatText {
    pub pos: Vec2,
    pub text: String,
    pub life: f32,
    pub max: f32,
    pub col: u8,
    pub size: f32,
}

#[derive(Clone, Copy)]
pub struct Tile {
    pub strike_t: f32,
    pub lead: f32,
    pub armed: bool,
    pub flash: f32,
    /// Which kind of tile pattern armed it (balance stats only).
    pub tag: u8,
}

/// What the Heart is in the current phase of the song, and so what the player has to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeartForm {
    /// No phases known (live audio, drift): one plain health bar.
    Core,
    /// Calm sections: the shell is closed; break the shield nodes circling the arena.
    Shell,
    /// Normal sections: it only opens for the first half of every bar.
    Valve,
    /// Intense sections: three linked cores orbit it and break in order.
    Split,
    /// Drops: fully exposed, takes extra damage.
    Overdrive,
}

impl HeartForm {
    pub fn of(kind: SectionKind) -> HeartForm {
        match kind {
            SectionKind::Calm => HeartForm::Shell,
            SectionKind::Normal => HeartForm::Valve,
            SectionKind::Intense => HeartForm::Split,
            SectionKind::Drop => HeartForm::Overdrive,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            HeartForm::Core => "",
            HeartForm::Shell => "SHELL",
            HeartForm::Valve => "VALVE",
            HeartForm::Split => "SPLIT",
            HeartForm::Overdrive => "OVERDRIVE",
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            HeartForm::Core => "",
            HeartForm::Shell => "the shell is closed: break the shield nodes",
            HeartForm::Valve => "it opens on the downbeat",
            HeartForm::Split => "break the lit core, then the next",
            HeartForm::Overdrive => "exposed: pour it on",
        }
    }
    /// How much of the player's damage realistically lands in this form. A phase's share of the
    /// Heart's health is its length times this, so every form takes about as long to beat.
    fn yield_(self) -> f32 {
        match self {
            HeartForm::Core => 1.0,
            HeartForm::Shell => 0.55,
            HeartForm::Valve => 0.65,
            HeartForm::Split => 0.6,
            HeartForm::Overdrive => 1.3,
        }
    }
    fn has_nodes(self) -> bool {
        matches!(self, HeartForm::Shell | HeartForm::Split)
    }
}

/// A shield node (Shell) or a core (Split): a target that orbits the Heart.
#[derive(Clone, Copy, Debug)]
pub struct Node {
    pub ang: f32,
    pub hp: f32,
    pub max_hp: f32,
    pub flash: f32,
}

pub const NODE_R: f32 = 22.0;
/// Damage multipliers: Overdrive, Valve open, Valve closed.
const OVERDRIVE_DMG: f32 = 1.5;
const VALVE_OPEN_DMG: f32 = 1.25;
const VALVE_SHUT_DMG: f32 = 0.2;
/// Pressure multiplier while a phase is beaten and the song has not moved on yet.
const PURGATORY_PRESSURE: f32 = 0.35;

/// One phase of the Heart: a section of the song.
#[derive(Clone, Copy, Debug)]
pub struct HeartPhase {
    pub t0: f32,
    pub t1: f32,
    pub kind: SectionKind,
    /// Heart health at which this phase is beaten (0 for the last one).
    pub floor: f32,
}

/// Places the Heart may stand (the centre first).
pub const HEART_ANCHORS: [(f32, f32); 7] = [(0.0, 0.0), (-230.0, -110.0), (230.0, -110.0), (-230.0, 110.0), (230.0, 110.0), (-300.0, 0.0), (300.0, 0.0)];
/// The Heart stays at least this long in one place.
const HEART_STAY_SECS: f32 = 7.0;
/// In Overdrive the Heart charges at most once per this many beats.
const CHARGE_EVERY_BEATS: f32 = 8.0;
/// Resonance: seconds it lasts, and what it multiplies damage and score by.
pub const RESONANCE_SECS: f32 = 8.0;
const RESONANCE_DMG: f32 = 1.5;
const RESONANCE_SCORE: f32 = 2.0;
/// The Heart relocates from this hardness up (PULSE and harder).
const HEART_MOVES_FROM: f32 = 0.15;

pub struct Heart {
    /// Where the Heart is. Everything it does starts here.
    pub pos: Vec2,
    /// A relocation in progress or announced: it leaves `move_from` at `move_t0` and arrives at
    /// `move_to` at `move_t1`. Before `move_t0` only the destination is shown.
    pub move_from: Vec2,
    pub move_to: Vec2,
    pub move_t0: f32,
    pub move_t1: f32,
    /// The relocation under way is a charge: a fast straight run through where the player stood.
    pub charging: bool,
    pub hp: f32,
    pub max_hp: f32,
    pub form: HeartForm,
    /// Health this phase started with above its floor (for the HUD).
    pub phase_max: f32,
    pub nodes: Vec<Node>,
    /// 0 = shut, 1 = open (Valve form; 1 in the exposed forms).
    pub open: f32,
    pub broken: bool,
    pub punch: f32,
    pub flash: f32,
    pub rot: f32,
    pub reform: f32,
    pub charge: f32,
}

impl Heart {
    fn new(hp: f32, reform: f32) -> Heart {
        Heart { pos: Vec2::ZERO, move_from: Vec2::ZERO, move_to: Vec2::ZERO, move_t0: -1.0, move_t1: -1.0, charging: false, hp, max_hp: hp, form: HeartForm::Core, phase_max: hp, nodes: Vec::new(), open: 1.0, broken: false, punch: 0.0, flash: 0.0, rot: 0.0, reform, charge: 0.0 }
    }
    /// Where node `k` is on the play field.
    pub fn node_pos(&self, k: usize) -> Vec2 {
        let a = self.nodes.get(k).map_or(0.0, |n| n.ang);
        match self.form {
            HeartForm::Shell => vec2(a.cos() * HALF_W * 0.72, a.sin() * HALF_H * 0.66),
            _ => self.pos + vec2(a.cos(), a.sin()) * 178.0,
        }
    }
    /// True from the moment a relocation is announced until the Heart has arrived.
    pub fn relocating(&self, t: f32) -> bool {
        t < self.move_t1
    }
    /// Index of the node that can be damaged in Split form: the first one still alive.
    pub fn lit_node(&self) -> Option<usize> {
        self.nodes.iter().position(|n| n.hp > 0.0)
    }
}

#[derive(Default, Clone, Copy)]
pub struct Stats {
    pub kills: u32,
    pub shards: u32,
    pub hits: u32,
    pub max_combo: f32,
    pub hearts_broken: u32,
    pub songs: u32,
    pub notes_shot: u32,
    /// Heart phases beaten before their section ended.
    pub phases_broken: u32,
    /// Hostile bullets slipped past at close range.
    pub grazes: u32,
    /// Sum of the damage multipliers of every shot the gun fired (1.0 = one nominal shot).
    pub gun_dmg: f32,
    /// Hits taken per source (`SRC_*`).
    pub by_src: [u32; 6],
    /// Bullet hits per `Bullet::pat`.
    pub by_pat: [u32; 12],
    /// Tile hits per tile-pattern kind (see `tile_tag`).
    pub by_tile: [u32; 9],
}

pub struct World {
    pub t: f32,
    pub player: Player,
    pub heart: Heart,
    pub bullets: Vec<Bullet>,
    pub shots: Vec<Shot>,
    pub enemies: Vec<Enemy>,
    pub portals: Vec<Portal>,
    pub lasers: Vec<Laser>,
    pub waves: Vec<Wave>,
    pub shards: Vec<Shard>,
    pub orbs: Vec<Orb>,
    pub parts: Vec<Particle>,
    pub texts: Vec<FloatText>,
    pub tiles: Vec<Tile>,
    /// Announced Heart pulses that have not fired yet (sorted by nothing in particular).
    pub pulses: Vec<PendingPulse>,
    /// Decaying 0..1 envelopes of the most recent kick / snare / hat onset (index = band).
    pub hit: [f32; 3],
    /// Position inside the current beat, 0..1 (0 = on the beat).
    pub beat_phase: f32,
    /// Seconds per beat of the current song (0.5 when unknown).
    pub beat_len: f32,
    /// Current speed multiplier applied to bullets and enemies: they surge on every beat and
    /// ease off between beats (averages 1.0 over a beat).
    pub groove: f32,
    /// How strongly `groove` swings, 0..1 (set per section).
    pub groove_amt: f32,
    /// 0..1 transition envelope: kicked to 1 when a song / menu track starts, decays over ~1 s.
    pub warp: f32,
    /// 0..1 difficulty ramp of the current moment (for HUD / visuals).
    pub threat: f32,
    pub shake: f32,
    pub flash: f32,
    pub beat_pulse: f32,
    pub bar_pulse: f32,
    pub beat_count: u32,
    pub score: u64,
    pub combo: f32,
    combo_idle: f32,
    pub stats: Stats,
    pub section_kind: SectionKind,
    pub section_idx: u16,
    pub section_banner: f32,
    pub palette: u8,
    pub intensity: f32,
    pub feat: Feat,
    pub rng: Rng,
    pub no_fail: bool,
    pub diff: f32,
    pub ended: bool,
    pub dead: bool,
    pub song_idx: u32,
    /// Offers waiting for the current one to be taken: (choices, rare).
    pending_orbs: Vec<(usize, bool)>,
    /// Performance since the last graded reward.
    pub window: GradeWindow,
    /// Grade of the last graded reward (index into `GRADES`) and a 0..1 envelope for the HUD.
    pub last_grade: usize,
    pub grade_banner: f32,
    /// Run modifiers in force (`MOD_*` bits), and which one this run starts with.
    pub mods: u8,
    mod_offset: u8,
    downbeat_ready: bool,
    /// Seconds of actual play (music audible) in the current song.
    pub active_t: f32,
    /// Sound effects requested this frame: (which, gain 0..1, pan -1..1). Drained by the app.
    pub sounds: Vec<(Sfx, f32, f32)>,
    shard_snd_cd: f32,
    /// Age (seconds) of the bullet that last touched the player (balance stats).
    pub last_hit_age: f32,
    /// 1.0 normally; drops after a hit and recovers (see `MERCY_*`).
    pub mercy: f32,
    /// Shot times of the current song per TEMPO level (`level::gun_track`). Empty when the song
    /// is not known ahead of time (live audio, drift): the gun then locks to a beat subdivision.
    pub gun: Vec<Vec<f32>>,
    /// Resonance: a meter (0..1) filled by playing close to the music (grazes, kills, on-beat
    /// dashes, hurting the Heart). Full, it rings for `RESONANCE_SECS` (`res_t` counts down):
    /// more damage, double score, a wider magnet.
    pub resonance: f32,
    pub res_t: f32,
    /// Live audio: the song is not known ahead, so each section the generator announces becomes
    /// a Heart phase on the spot (`push_live_phase`).
    pub live_phases: bool,
    /// DODGE mode: no gun, the Heart cannot be hurt, grazes score triple.
    pub pacifist: bool,
    /// Mutators chosen in run setup (`MOD_*` bits); depth adds more on top.
    pub base_mods: u8,
    /// Score multiplier of the tier and the chosen mutators.
    pub score_mult: f32,
    survive_acc: f32,
    /// Ear-candy comets in flight.
    pub painters: Vec<Painter>,
    /// 0..1 envelope kicked by every piece of ear candy (visual flourish).
    pub candy: f32,
    /// The current song's look (set by the app; the renderer reads it).
    pub mood: Mood,
    /// The Heart's phases for this song (empty when the song is not known ahead of time).
    pub phases: Vec<HeartPhase>,
    pub phase_i: usize,
    /// True between beating a phase early and the start of the next section.
    pub purgatory: bool,
    /// 0..1 envelope following `purgatory` (visuals).
    pub purg: f32,
    /// 0..1 envelope kicked when a new Heart form appears (HUD hint).
    pub form_banner: f32,
    /// Beats since the last downbeat.
    bar_beat: u32,
    /// How many Heart phases were broken early in this song (raises pressure).
    pub rage: u32,
    /// When the current phase (or the Heart's return) began, and when purgatory runs out.
    phase_t0: f32,
    pub purg_until: f32,
    /// Damage per second the player dealt in the phase they last broke.
    phase_dps: f32,
    /// Musical layers currently driving attacks (bit = `LayerKind as u8`), for the HUD.
    pub armed: u8,
    /// 0..1 glow per layer: rises while the layer is armed.
    pub layer_glow: [f32; N_LAYERS],
    /// Decaying 0..1 envelope of the last chime, and its pitch (0..1).
    pub chime: f32,
    pub chime_pitch: f32,
    /// TEMPO level `Player::fire_phase` was counted at (a level change must not fire a shot).
    gun_lv: usize,
}

pub fn tile_center(c: usize, r: usize) -> Vec2 {
    vec2(
        -HALF_W + (c as f32 + 0.5) * TILE,
        -HALF_H + (r as f32 + 0.5) * TILE,
    )
}

pub fn tile_of(p: Vec2) -> Option<(usize, usize)> {
    let c = ((p.x + HALF_W) / TILE).floor();
    let r = ((p.y + HALF_H) / TILE).floor();
    if c >= 0.0 && r >= 0.0 && (c as usize) < COLS && (r as usize) < ROWS {
        Some((c as usize, r as usize))
    } else {
        None
    }
}

fn dir(a: f32) -> Vec2 {
    vec2(a.cos(), a.sin())
}

impl World {
    pub fn new(diff: f32, no_fail: bool) -> Self {
        World {
            t: 0.0,
            player: Player::new(),
            heart: Heart::new(100.0, 0.0),
            bullets: Vec::with_capacity(MAX_BULLETS),
            shots: Vec::with_capacity(128),
            enemies: Vec::with_capacity(MAX_ENEMIES),
            portals: Vec::new(),
            lasers: Vec::new(),
            waves: Vec::new(),
            shards: Vec::with_capacity(256),
            orbs: Vec::new(),
            parts: Vec::with_capacity(MAX_PARTS),
            texts: Vec::new(),
            tiles: vec![Tile { strike_t: -1.0, lead: 1.0, armed: false, flash: 0.0, tag: 0 }; COLS * ROWS],
            pulses: Vec::with_capacity(16),
            hit: [0.0; 3],
            beat_phase: 0.0,
            beat_len: 0.5,
            groove: 1.0,
            groove_amt: 0.28,
            warp: 0.0,
            threat: 0.0,
            shake: 0.0,
            flash: 0.0,
            beat_pulse: 0.0,
            bar_pulse: 0.0,
            beat_count: 0,
            score: 0,
            combo: 0.0,
            combo_idle: 0.0,
            stats: Stats::default(),
            section_kind: SectionKind::Normal,
            section_idx: 0,
            section_banner: 0.0,
            palette: 0,
            intensity: 0.0,
            feat: Feat::default(),
            rng: Rng::new(0x51AB_C0DE),
            no_fail,
            diff,
            ended: false,
            dead: false,
            song_idx: 0,
            pending_orbs: Vec::new(),
            window: GradeWindow::default(),
            last_grade: 1,
            grade_banner: 0.0,
            mods: 0,
            mod_offset: 0,
            downbeat_ready: false,
            active_t: 0.0,
            sounds: Vec::with_capacity(16),
            shard_snd_cd: 0.0,
            last_hit_age: 0.0,
            mercy: 1.0,
            gun: Vec::new(),
            resonance: 0.0,
            res_t: 0.0,
            live_phases: false,
            pacifist: false,
            base_mods: 0,
            score_mult: 1.0,
            survive_acc: 0.0,
            painters: Vec::new(),
            candy: 0.0,
            mood: Mood::default(),
            phases: Vec::new(),
            phase_i: 0,
            purgatory: false,
            purg: 0.0,
            form_banner: 0.0,
            bar_beat: 0,
            rage: 0,
            phase_t0: 0.0,
            purg_until: 0.0,
            phase_dps: 0.0,
            armed: 0,
            layer_glow: [0.0; N_LAYERS],
            chime: 0.0,
            chime_pitch: 0.5,
            gun_lv: 0,
        }
    }

    /// Current difficulty multiplier applied to the planned level (about 0.2 .. 1.3).
    pub fn pressure(&self) -> f32 {
        let (r0, secs) = RAMP[(self.song_idx as usize).min(RAMP.len() - 1)];
        // Harder tiers start nearer full pressure and get there sooner.
        let h = hardness(self.diff);
        let r0 = (r0 + 0.45 * h).min(0.95);
        let k = smooth01(self.active_t / (secs * (1.0 - 0.6 * h)));
        (r0 + (1.0 - r0) * k)
            * (1.0 + RUN_ESCALATION * self.song_idx as f32)
            * (1.0 + LEVEL_PRESSURE * self.player.level as f32)
            * (1.0 + RAGE_PRESSURE * self.rage as f32)
            * self.mercy
            * if self.purgatory { PURGATORY_PRESSURE } else { 1.0 }
    }

    /// Give the Heart one phase per section of the song. Call after `begin_song`.
    pub fn set_phases(&mut self, sections: &[(f32, f32, SectionKind)]) {
        let weight = |s: &(f32, f32, SectionKind)| (s.1 - s.0).max(0.0) * HeartForm::of(s.2).yield_();
        let total: f32 = sections.iter().map(weight).sum::<f32>().max(1e-3);
        let mut left = self.heart.max_hp;
        self.phases.clear();
        for (i, s) in sections.iter().enumerate() {
            left -= self.heart.max_hp * weight(s) / total;
            let floor = if i + 1 == sections.len() { 0.0 } else { left.max(0.0) };
            self.phases.push(HeartPhase { t0: s.0, t1: s.1, kind: s.2, floor });
        }
        self.phase_i = 0;
        self.purgatory = false;
    }

    /// The Heart is in the last phase of a song that is known ahead of time.
    pub fn in_finale(&self) -> bool {
        !self.live_phases && self.phases.len() > 2 && self.phase_i + 1 == self.phases.len() && !self.purgatory && !self.heart.broken
    }

    /// Heart health at which the current phase is beaten.
    pub fn phase_floor(&self) -> f32 {
        self.phases.get(self.phase_i).map_or(0.0, |p| p.floor)
    }

    /// 0..1 of the current phase's health that is left.
    pub fn phase_frac(&self) -> f32 {
        ((self.heart.hp - self.phase_floor()) / self.heart.phase_max.max(1e-3)).clamp(0.0, 1.0)
    }

    /// Seconds until the current phase's section ends.
    pub fn phase_left(&self) -> f32 {
        self.phases.get(self.phase_i).map_or(0.0, |p| (p.t1 - self.t).max(0.0))
    }

    /// The player's damage output relative to a fresh build (1.0 = no upgrades).
    pub fn gun_power(&self) -> f32 {
        let p = &self.player;
        let tempo = (p.up(UpgradeId::Tempo) as usize).min(GUN_INTERVALS.len() - 1);
        (1.0 + 0.5 * p.up(UpgradeId::Gain) as f32)
            * (1.0 + 0.7 * p.up(UpgradeId::Chord) as f32)
            * (GUN_INTERVALS[0] / GUN_INTERVALS[tempo])
            * (1.0 + 0.2 * p.up(UpgradeId::Echo) as f32)
            * (1.0 + 0.1 * p.up(UpgradeId::Pierce) as f32)
            * (1.0 + 0.1 * p.up(UpgradeId::Downbeat) as f32)
            * (1.0 + ACCENT_CHANCE * (ACCENT_MUL - 1.0) * p.up(UpgradeId::Accent) as f32)
            * (1.0 + 0.1 * p.up(UpgradeId::Crescendo) as f32)
    }

    /// On a bar line: decide whether the Heart relocates. It goes home for Shell (the shield
    /// nodes need the centre), otherwise to an anchor away from both itself and the player. The
    /// destination is shown for one beat, then the Heart glides there over two.
    fn maybe_move_heart(&mut self) {
        let t = self.t;
        if self.phases.is_empty() || self.heart.broken || self.purgatory || self.heart.relocating(t) {
            return;
        }
        // The two gentlest tiers keep the Heart where it is: one thing less to read.
        if hardness(self.diff) < HEART_MOVES_FROM {
            return;
        }
        let home = self.heart.form == HeartForm::Shell;
        let stay = HEART_STAY_SECS * (2.2 - 1.5 * hardness(self.diff));
        let to = if home {
            if self.heart.pos.length() < 1.0 {
                return;
            }
            Vec2::ZERO
        } else if self.heart.form == HeartForm::Overdrive {
            // Overdrive: it charges. The lane is drawn for a beat, then it crosses in one.
            let bl = self.beat_len.clamp(0.25, 1.0);
            let here = self.heart.pos;
            let d = self.player.pos - here;
            let dist = d.length();
            if t - self.heart.move_t1 < CHARGE_EVERY_BEATS * bl || dist < 140.0 {
                return;
            }
            let mut to = here + d / dist * (dist + 150.0).min(600.0);
            to.x = to.x.clamp(-HALF_W + 120.0, HALF_W - 120.0);
            to.y = to.y.clamp(-HALF_H + 120.0, HALF_H - 120.0);
            let wait = bl.max(0.6);
            self.heart.move_from = here;
            self.heart.move_to = to;
            self.heart.move_t0 = t + wait;
            self.heart.move_t1 = t + wait + bl.clamp(0.35, 0.7);
            self.heart.charging = true;
            self.snd(Sfx::Slam, 0.7, to.x);
            return;
        } else {
            if t - self.heart.move_t1 < stay || self.pressure() < 0.5 {
                return;
            }
            let here = self.heart.pos;
            let ppos = self.player.pos;
            let ok: Vec<Vec2> = HEART_ANCHORS.iter().map(|a| vec2(a.0, a.1)).filter(|a| a.distance(here) > 180.0 && a.distance(ppos) > 260.0).collect();
            if ok.is_empty() {
                return;
            }
            ok[self.rng.below(ok.len())]
        };
        let bl = self.beat_len.clamp(0.25, 1.0);
        self.heart.move_from = self.heart.pos;
        self.heart.move_to = to;
        // Leave only after every pulse already announced has fired from here.
        let wait = bl.max(0.7);
        self.heart.move_t0 = t + wait;
        self.heart.move_t1 = t + wait + (2.0 * bl).clamp(0.7, 1.4);
        self.snd(Sfx::Dash, 0.6, to.x);
    }

    /// Purgatory ran out while the section is still playing: the Heart comes back, sized for
    /// the damage the player has just shown they can do.
    fn encore(&mut self) {
        let left = if self.live_phases { self.phase_left().max(12.0) } else { self.phase_left() };
        let bonus = (self.phase_dps * left * RAGE_KEEP_UP).max(self.heart.max_hp * 0.03);
        self.heart.hp += bonus;
        self.heart.max_hp += bonus;
        self.purgatory = false;
        self.phase_t0 = self.t;
        self.heart.phase_max = bonus;
        self.heart.reform = 1.0;
        self.heart.nodes.clear();
        if self.heart.form.has_nodes() {
            let a0 = self.rng.angle();
            for k in 0..3 {
                self.heart.nodes.push(Node { ang: a0 + k as f32 * TAU / 3.0, hp: bonus / 3.0, max_hp: bonus / 3.0, flash: 0.0 });
            }
        }
        self.form_banner = 1.0;
        self.shake = self.shake.max(0.5);
        self.snd(Sfx::Slam, 0.9, 0.0);
        self.burst(self.heart.pos, 50, 620.0, C_DANGER, 7.0);
        self.text(vec2(0.0, -120.0), "THE HEART RETURNS", C_DANGER, 46.0, 2.4);
        self.text(vec2(0.0, -82.0), &format!("rage {}: it hits harder now", self.rage), C_WHITE, 22.0, 2.4);
    }

    fn enter_phase(&mut self, idx: usize) {
        if self.phases.is_empty() || self.heart.broken {
            return;
        }
        self.phase_t0 = self.t;
        let idx = idx.min(self.phases.len() - 1);
        let form = HeartForm::of(self.phases[idx].kind);
        if form != self.heart.form {
            self.form_banner = 1.0;
        }
        self.phase_i = idx;
        self.purgatory = false;
        self.heart.form = form;
        if !self.live_phases && self.phases.len() > 2 && idx + 1 == self.phases.len() {
            self.text(vec2(0.0, -170.0), "FINALE", C_DANGER, 58.0, 2.8);
            self.text(vec2(0.0, -128.0), "every pulse has an echo", C_WHITE, 22.0, 2.8);
            self.flash = self.flash.max(0.25);
        }
        self.heart.phase_max = (self.heart.hp - self.phases[idx].floor).max(1.0);
        self.heart.nodes.clear();
        if form.has_nodes() {
            let a0 = self.rng.angle();
            let hp = self.heart.phase_max / 3.0;
            for k in 0..3 {
                self.heart.nodes.push(Node { ang: a0 + k as f32 * TAU / 3.0, hp, max_hp: hp, flash: 0.0 });
            }
        }
    }

    /// Live audio: start a Heart phase for the section that has just begun. It is sized for
    /// `LIVE_PHASE_SECS` of play; if the music stays put longer the Heart returns after purgatory.
    fn push_live_phase(&mut self, kind: SectionKind) {
        if self.heart.broken {
            return;
        }
        let quota = self.heart.max_hp * LIVE_PHASE_SECS / LIVE_HEART_SECS * HeartForm::of(kind).yield_();
        let floor = (self.heart.hp - quota).max(0.0);
        self.phases.push(HeartPhase { t0: self.t, t1: self.t + LIVE_PHASE_SECS, kind, floor });
        self.enter_phase(self.phases.len() - 1);
        // A new sky every third section: live audio has no song to give it an identity.
        if self.phases.len() % 3 == 0 {
            self.mood.scene = (self.mood.scene + 1 + self.rng.below(3) as u8) % N_SCENES;
            self.mood.prop = self.rng.below(N_PROPS as usize) as u8;
        }
    }

    /// The point a shot should go to hurt the Heart from `from`, if anything can be hurt now.
    pub fn heart_target(&self, from: Vec2) -> Option<Vec2> {
        if self.heart.broken || self.purgatory || self.pacifist {
            return None;
        }
        match self.heart.form {
            HeartForm::Shell => (0..self.heart.nodes.len())
                .filter(|&k| self.heart.nodes[k].hp > 0.0)
                .map(|k| self.heart.node_pos(k))
                .min_by(|a, b| a.distance(from).partial_cmp(&b.distance(from)).unwrap_or(std::cmp::Ordering::Equal)),
            HeartForm::Split => self.heart.lit_node().map(|k| self.heart.node_pos(k)),
            _ => Some(self.heart.pos),
        }
    }

    /// Take `dmg` off the Heart, stopping at the current phase's floor.
    fn damage_heart(&mut self, dmg: f32) {
        if dmg <= 0.0 || self.heart.broken || self.purgatory || self.pacifist {
            return;
        }
        let floor = self.phase_floor();
        self.resonate(1.2 * dmg / self.heart.max_hp.max(1.0));
        self.heart.hp = (self.heart.hp - dmg).max(floor);
        self.heart.flash = 1.0;
        self.add_score(dmg * 4.0);
        let nodes_gone = self.heart.form.has_nodes() && !self.heart.nodes.is_empty() && self.heart.nodes.iter().all(|n| n.hp <= 0.0);
        if nodes_gone {
            self.heart.hp = floor;
        }
        if self.heart.hp <= floor + 1e-3 {
            // Live phases are made as the music goes, so "the last one so far" is not the end.
            let more_to_come = self.live_phases && floor > 0.0;
            if self.phases.is_empty() || (self.phase_i + 1 >= self.phases.len() && !more_to_come) {
                self.break_heart();
            } else {
                self.enter_purgatory();
            }
        }
    }

    /// The phase was beaten before its section ended: a breather with a clock.
    fn enter_purgatory(&mut self) {
        self.purgatory = true;
        self.stats.phases_broken += 1;
        // Rage: the faster the phase fell, the more the rest of the Heart grows, so a strong
        // build is answered with a tougher Heart instead of purgatory after purgatory.
        let took = (self.t - self.phase_t0).max(1.0);
        let planned = self.phases.get(self.phase_i).map_or(took, |p| (p.t1 - self.phase_t0).max(took));
        self.phase_dps = self.heart.phase_max / took;
        let grow = 1.0 + ((planned / took).min(3.0) - 1.0) * RAGE_KEEP_UP;
        let before = self.heart.hp;
        for ph in self.phases.iter_mut().skip(self.phase_i) {
            ph.floor *= grow;
        }
        self.heart.hp = before * grow;
        self.heart.max_hp += before * (grow - 1.0);
        self.rage = (self.rage + 1).min(RAGE_MAX);
        self.purg_until = self.t + PURGATORY_SECS;
        self.heart.nodes.clear();
        self.pulses.clear();
        // Everything hostile in the air becomes a shootable note.
        for b in self.bullets.iter_mut() {
            b.soft = true;
            b.kind = B_NOTE;
            b.r = 8.0;
        }
        self.add_score(1500.0);
        self.snd(Sfx::Clear, 0.9, 0.0);
        self.shake = self.shake.max(0.6);
        self.flash = self.flash.max(0.5);
        self.burst(self.heart.pos, 60, 700.0, C_PRIMARY, 7.0);
        self.text(vec2(0.0, -120.0), "PURGATORY", C_ACCENT, 54.0, 2.6);
        self.text(vec2(0.0, -78.0), "phase broken early - everything is shootable", C_WHITE, 22.0, 2.6);
        self.window.broke_phase = true;
        self.coda();
        // The first two breaks of a song are rewarded with a graded offer, later ones with shards.
        let left = self.phase_left().min(PURGATORY_SECS);
        if left > 5.0 && self.rage <= 2 {
            self.spawn_reward(left.min(14.0));
        } else {
            self.drop_shards(vec2(0.0, 150.0), 6, false);
        }
    }

    /// CODA: breaking a Heart phase heals.
    fn coda(&mut self) {
        let n = self.player.up(UpgradeId::Coda) as i32;
        if n > 0 && self.player.hp < self.player.max_hp {
            self.player.hp = (self.player.hp + n).min(self.player.max_hp);
            let pos = self.player.pos;
            self.text(pos + vec2(0.0, -40.0), "CODA", C_PRIMARY, 24.0, 1.2);
            self.snd(Sfx::Heal, 0.8, pos.x);
        }
    }

    fn break_heart(&mut self) {
        self.coda();
        self.heart.hp = 0.0;
        self.heart.broken = true;
        self.window.broke_phase = true;
        self.heart.nodes.clear();
        self.stats.hearts_broken += 1;
        self.snd(Sfx::HeartBreak, 1.0, 0.0);
        self.add_score(5000.0);
        self.shake = 1.0;
        self.flash = 0.8;
        self.burst(self.heart.pos, 90, 900.0, C_PRIMARY, 8.0);
        self.burst(self.heart.pos, 50, 600.0, C_WHITE, 6.0);
        self.text(vec2(0.0, -120.0), "HEART BROKEN", C_ACCENT, 54.0, 3.0);
        self.text(vec2(0.0, -78.0), "victory lap - everything is shootable", C_WHITE, 22.0, 3.0);
        // Every bullet on the field becomes a reward.
        let pts: Vec<Vec2> = self.bullets.iter().map(|b| b.pos).collect();
        self.bullets.clear();
        for (n, p) in pts.into_iter().enumerate() {
            if n % 3 == 0 {
                self.drop_shards(p, 1, false);
            }
        }
    }

    /// The pressure at which a mechanic with base threshold `p` unlocks on this difficulty.
    fn gate(&self, p: f32) -> f32 {
        p * (1.0 - 0.5 * hardness(self.diff))
    }

    /// Reset per-song state; player upgrades, HP and score carry over.
    pub fn begin_song(&mut self, active_secs: f32, song_idx: u32, seed: u64) {
        self.t = 0.0;
        self.bullets.clear();
        self.shots.clear();
        self.enemies.clear();
        self.portals.clear();
        self.lasers.clear();
        self.waves.clear();
        self.shards.clear();
        self.pulses.clear();
        self.hit = [0.0; 3];
        self.warp = 1.0;
        self.active_t = 0.0;
        self.mercy = 1.0;
        for t in self.tiles.iter_mut() {
            *t = Tile { strike_t: -1.0, lead: 1.0, armed: false, flash: 0.0, tag: 0 };
        }
        self.song_idx = song_idx;
        self.rng = Rng::new(seed ^ 0xA5A5_5A5A);
        // The Heart is sized for the damage this build can do, so upgrades help without
        // turning the next song into a walkover.
        let hp = active_secs * 2.6 * (1.0 + 0.15 * song_idx as f32) * (0.75 + 0.25 * self.diff) * self.gun_power().powf(HEART_POWER_EXP);
        self.rage = 0;
        self.phase_t0 = 0.0;
        self.heart = Heart::new(hp, 1.0);
        // Depth: every song after the first switches one more modifier on.
        if song_idx == 0 {
            self.mod_offset = (seed % DEPTH_MODS as u64) as u8;
        }
        self.mods = self.base_mods;
        for k in 0..(song_idx as usize).min(DEPTH_MODS) {
            self.mods |= 1 << ((self.mod_offset as usize + k) % DEPTH_MODS);
        }
        self.phases.clear();
        self.phase_i = 0;
        self.purgatory = false;
        self.bar_beat = 0;
        self.ended = false;
        self.dead = false;
        self.beat_count = 0;
        self.section_banner = 0.0;
        self.player.fire_phase = 0;
        self.gun.clear();
        self.painters.clear();
        self.res_t = 0.0;
        self.armed = 0;
        if self.player.up(UpgradeId::Encore) > 0 {
            self.player.shield = true;
        }
    }

    fn snd(&mut self, s: Sfx, gain: f32, x: f32) {
        if self.sounds.len() < 16 {
            self.sounds.push((s, gain, (x / HALF_W).clamp(-1.0, 1.0)));
        }
    }

    pub fn mult(&self) -> f32 {
        (1.0 + (self.combo / 8.0).floor() * 0.5).min(8.0)
    }

    fn add_combo(&mut self, v: f32) {
        self.combo += v;
        self.combo_idle = 0.0;
        if self.combo > self.stats.max_combo {
            self.stats.max_combo = self.combo;
        }
    }

    fn add_score(&mut self, base: f32) {
        let res = if self.res_t > 0.0 { RESONANCE_SCORE } else { 1.0 };
        self.score += (base * self.mult() * self.score_mult * res).round().max(0.0) as u64;
    }

    /// Feed the resonance meter; when it fills, resonance starts.
    fn resonate(&mut self, amount: f32) {
        if self.res_t > 0.0 || self.dead {
            return;
        }
        self.resonance += amount;
        if self.resonance >= 1.0 {
            self.resonance = 0.0;
            self.res_t = RESONANCE_SECS;
            let pos = self.player.pos;
            self.text(pos + vec2(0.0, -56.0), "RESONANCE", C_ACCENT, 34.0, 1.8);
            self.burst(pos, 40, 620.0, C_ACCENT, 5.0);
            self.snd(Sfx::Upgrade, 0.8, pos.x);
            self.flash = self.flash.max(0.2);
        }
    }

    pub fn burst(&mut self, pos: Vec2, n: usize, speed: f32, col: u8, size: f32) {
        for _ in 0..n {
            if self.parts.len() >= MAX_PARTS {
                break;
            }
            let a = self.rng.angle();
            let s = speed * self.rng.range(0.25, 1.0);
            let life = self.rng.range(0.25, 0.7);
            self.parts.push(Particle {
                pos,
                vel: dir(a) * s,
                life,
                max: life,
                size: size * self.rng.range(0.5, 1.2),
                col,
                rot: a,
            });
        }
    }

    pub fn text(&mut self, pos: Vec2, s: &str, col: u8, size: f32, life: f32) {
        self.texts.push(FloatText { pos, text: s.to_string(), life, max: life, col, size });
    }

    fn drop_shards(&mut self, pos: Vec2, n: usize, heal: bool) {
        for k in 0..n {
            let a = self.rng.angle();
            let s = self.rng.range(60.0, 220.0);
            self.shards.push(Shard { pos, vel: dir(a) * s, life: 9.0, value: 1, heal: heal && k == 0, age: 0.0 });
        }
    }

    // -----------------------------------------------------------------------------------------
    // Level events
    // -----------------------------------------------------------------------------------------

    pub fn apply(&mut self, e: &LevelEvent) {
        // Purgatory is a breather: only the Heart's (now harmless) pulses keep coming.
        if self.purgatory && matches!(e.ev, Ev::Tick { .. } | Ev::Spawn { .. } | Ev::Laser { .. } | Ev::Tiles { .. } | Ev::Shockwave { .. }) {
            return;
        }
        let late = (self.t - e.t).clamp(0.0, 0.25);
        let p = self.pressure();
        match e.ev {
            Ev::Beat { down } => self.on_beat(down),
            Ev::Hit { band, strength } => {
                let b = (band as usize).min(2);
                self.hit[b] = self.hit[b].max(0.45 + 0.55 * strength.clamp(0.0, 1.0));
                if b == 0 {
                    self.heart.punch = self.heart.punch.max(0.6);
                }
            }
            // The Heart holds its fire from the moment it announces a move until it arrives, so
            // no warning zone ever slides across the floor.
            Ev::Pulse { .. } if self.heart.relocating(self.t) => {}
            Ev::Pulse { pattern, count, speed, rot, soft } => {
                let soft = soft || self.purgatory;
                // Scale the planned pulse by the current pressure.
                let forte = if self.mods & MOD_FORTE != 0 { 1.2 } else { 1.0 };
                let allegro = if self.mods & MOD_ALLEGRO != 0 { 1.15 } else { 1.0 };
                let count = if count == 0 { 0 } else { ((count as f32 * p * forte).round() as u16).clamp(3, 64) };
                // Speed is not scaled by pressure: it was chosen so the pulse crosses the beat
                // ring on a beat (`level::beat_speed`).
                let speed = speed * allegro;
                if e.lead > 0.02 && self.t < e.t - 0.01 {
                    if self.pulses.len() < 24 {
                        let to_p = self.player.pos - self.heart.pos;
                        let aim = to_p.y.atan2(to_p.x);
                        self.pulses.push(PendingPulse { t: e.t, lead: e.lead, pattern, count, speed, rot, soft, aim });
                        // Finale: in its last phase every pulse has an echo half a beat later,
                        // off to one side and made of shootable notes.
                        if self.in_finale() && count >= 5 && self.pulses.len() < 24 {
                            let half = 0.5 * self.beat_len.clamp(0.25, 1.0);
                            let side = if self.beat_count % 2 == 0 { 0.5 } else { -0.5 };
                            let n = ((count as f32 * 0.6).round() as u16).max(3);
                            self.pulses.push(PendingPulse { t: e.t + half, lead: e.lead + half, pattern, count: n, speed, rot: rot + 0.3, soft: true, aim: aim + side });
                        }
                    }
                } else {
                    let to_p = self.player.pos - self.heart.pos;
                    let aim = to_p.y.atan2(to_p.x);
                    self.emit_pulse(pattern, count as usize, speed, rot, soft, late, aim);
                }
            }
            Ev::Tick { speed } => {
                if p >= self.gate(P_TICK) {
                    self.emit_tick(speed, late);
                }
            }
            Ev::Spawn { kind, x, y } => {
                let cap = (2.0 + 5.0 * p * self.diff).round() as usize + if self.mods & MOD_SWARM != 0 { 3 } else { 0 };
                if p >= self.gate(P_SPAWN) && self.enemies.len() + self.portals.len() < cap.min(MAX_ENEMIES) {
                    // Tougher kinds only appear once the run has warmed up.
                    let kind = match kind {
                        EnemyKind::Wisp => EnemyKind::Wisp,
                        EnemyKind::Turret if p < 0.65 => EnemyKind::Chaser,
                        EnemyKind::Bomber if p < 0.75 => EnemyKind::Chaser,
                        EnemyKind::Elite if p < self.gate(P_ELITE) => EnemyKind::Orbiter,
                        EnemyKind::Splitter if p < 0.8 => EnemyKind::Chaser,
                        EnemyKind::Darter if p < 0.7 => EnemyKind::Chaser,
                        EnemyKind::Orbiter if p < 0.6 => EnemyKind::Chaser,
                        k => k,
                    };
                    // Never open a portal right on top of the player.
                    let mut pos = vec2(x, y);
                    if pos.distance(self.player.pos) < 230.0 {
                        pos = -pos;
                    }
                    self.portals.push(Portal { kind, pos, t0: e.at(), t1: e.t });
                }
            }
            Ev::Shards { x, y, n } => {
                for _ in 0..n {
                    let j = vec2(self.rng.range(-14.0, 14.0), self.rng.range(-14.0, 14.0));
                    self.shards.push(Shard { pos: vec2(x, y) + j, vel: Vec2::ZERO, life: 7.0, value: 1, heal: false, age: 0.0 });
                }
            }
            Ev::Laser { horizontal, from, to, dur } => {
                if p >= self.gate(P_LASER) && self.lasers.len() < 2 {
                    self.lasers.push(Laser { horizontal, from, to, t0: e.t, dur, lead: e.lead.max(0.3) });
                }
            }
            Ev::Tiles { pattern } if p < self.gate(P_TILES) => {
                let _ = pattern;
            }
            Ev::Tiles { pattern } => {
                for r in 0..ROWS {
                    for c in 0..COLS {
                        if tile_in_pattern(pattern, c, r) {
                            let tile = &mut self.tiles[r * COLS + c];
                            if !tile.armed {
                                tile.armed = true;
                                tile.strike_t = e.t;
                                tile.lead = e.lead.max(0.3);
                                tile.tag = tile_tag(pattern);
                            }
                        }
                    }
                }
            }
            Ev::Shockwave { gap, gaps } => {
                if p >= self.gate(P_WAVE) {
                    self.waves.push(Wave { origin: self.heart.pos, t0: e.t, lead: e.lead.max(0.3), gap, gaps, speed: 430.0 });
                }
            }
            Ev::Section { idx, kind, intensity: _, palette } => {
                self.section_idx = idx;
                self.section_kind = kind;
                self.palette = palette;
                self.section_banner = 2.4;
                self.heart.reform = 1.0;
                if self.live_phases {
                    self.push_live_phase(kind);
                } else {
                    self.enter_phase(idx as usize);
                }
                self.groove_amt = match kind {
                    SectionKind::Calm => 0.15,
                    SectionKind::Normal => 0.28,
                    SectionKind::Intense => 0.38,
                    SectionKind::Drop => 0.45,
                };
                if self.player.up(UpgradeId::Encore) > 0 {
                    self.player.shield = true;
                }
                if kind == SectionKind::Drop {
                    self.flash = self.flash.max(0.35);
                }
            }
            Ev::Layers { armed } => self.armed = armed,
            Ev::Chime { pitch, strength } => {
                self.chime = self.chime.max(0.5 + 0.5 * strength.clamp(0.0, 1.0));
                self.chime_pitch = pitch;
                let (x, y) = chime_point(pitch);
                self.burst(vec2(x, y), 3, 120.0, C_ACCENT, 3.0);
            }
            Ev::Candy { kind, pitch, pitch_end, dur } => {
                let (x, y) = chime_point(pitch);
                let at = vec2(x, y);
                self.candy = 1.0;
                self.add_score(250.0);
                self.snd(Sfx::Shield, 0.5, x);
                self.burst(at, 18, 300.0, C_ACCENT, 4.0);
                self.text(at + vec2(0.0, -46.0), "EAR CANDY", C_ACCENT, 22.0, 1.8);
                match kind {
                    // A run of notes: a comet crosses the arena and leaves a trail to collect.
                    0 => {
                        let (x2, y2) = chime_point(pitch_end);
                        let mut to = vec2(x2, y2);
                        if to.distance(at) < 500.0 {
                            to = vec2(-at.x, HALF_H * 0.55);
                        }
                        let t1 = e.t + dur.max(0.9) + 0.8;
                        self.painters.push(Painter { t0: e.t, t1, from: at, to, pos: at, next_drop: e.t });
                    }
                    // Keypad tones: a 3 x 4 keypad of shards.
                    1 => {
                        let c = vec2(x.clamp(-HALF_W + 120.0, HALF_W - 120.0), y.clamp(-HALF_H + 150.0, HALF_H - 150.0));
                        for r in 0..4 {
                            for k in 0..3 {
                                let p = c + vec2((k as f32 - 1.0) * 44.0, (r as f32 - 1.5) * 44.0);
                                self.shards.push(Shard { pos: p, vel: Vec2::ZERO, life: 9.0, value: 1, heal: false, age: 0.0 });
                            }
                        }
                    }
                    // A one-off sound: a sprite to catch before it leaves.
                    _ => self.make_enemy(EnemyKind::Sprite, at),
                }
            }
            Ev::Orbs => self.spawn_reward(14.0),
            Ev::Heal { x, y } => {
                self.shards.push(Shard { pos: vec2(x, y), vel: Vec2::ZERO, life: 12.0, value: 2, heal: true, age: 0.0 });
            }
            Ev::End => {
                if !self.ended {
                    self.ended = true;
                    let bonus = 2000.0 + self.player.hp.max(0) as f32 * 500.0;
                    self.score += bonus as u64;
                    self.stats.songs += 1;
                    self.snd(Sfx::Clear, 0.9, 0.0);
                }
            }
        }
    }

    fn on_beat(&mut self, down: bool) {
        self.beat_pulse = 1.0;
        self.bar_beat = if down { 0 } else { self.bar_beat + 1 };
        if down {
            self.maybe_move_heart();
        }
        if down && self.player.up(UpgradeId::Downbeat) > 0 {
            self.downbeat_ready = true;
        }
        if down {
            self.bar_pulse = 1.0;
        }
        self.beat_count += 1;
        // Enemy fire cadence is set in seconds and snapped to whole beats, so tempo does not
        // change how often they shoot.
        let every = |secs: f32| -> u32 { (secs / self.beat_len.max(0.2)).round().max(1.0) as u32 };
        let even = self.beat_count % every(1.2) == 0;
        let elite_fire = self.beat_count % every(2.2) == 0;
        let ppos = self.player.pos;
        let speed = 210.0 + 90.0 * self.intensity;
        let mut new_bullets: Vec<Bullet> = Vec::new();
        let mut blasts: Vec<Vec2> = Vec::new();
        let mut calls: Vec<Vec2> = Vec::new();
        for en in self.enemies.iter_mut() {
            match en.kind {
                // Metronome: a three-shot fan at the player on every downbeat.
                EnemyKind::Turret if down && en.age > 0.6 => {
                    let d = (ppos - en.pos).normalize_or_zero();
                    let a0 = d.y.atan2(d.x);
                    for off in [-0.24f32, 0.0, 0.24] {
                        let v = dir(a0 + off);
                        new_bullets.push(Bullet { pos: en.pos + v * en.r, vel: v * 235.0, r: 7.0, soft: false, age: 0.0, fade: 1.0, kind: B_ORB, pat: 10, grazed: false });
                    }
                    en.flash = 0.8;
                }
                // Cymbal: armed ones count the beats down and burst.
                EnemyKind::Bomber if en.state > 0.0 => {
                    en.state -= 1.0;
                    en.flash = 1.0;
                    if en.state < 1.0 {
                        for k in 0..BOMBER_RING {
                            let v = dir(en.rot + k as f32 * TAU / BOMBER_RING as f32);
                            new_bullets.push(Bullet { pos: en.pos + v * en.r, vel: v * 240.0, r: 7.0, soft: false, age: 0.0, fade: 1.0, kind: B_ORB, pat: 10, grazed: false });
                        }
                        blasts.push(en.pos);
                        en.hp = -1.0;
                    }
                }
                EnemyKind::Orbiter if even => {
                    let d = (ppos - en.pos).normalize_or_zero();
                    new_bullets.push(Bullet { pos: en.pos + d * en.r, vel: d * speed, r: 7.0, soft: false, age: 0.0, fade: 1.0, kind: B_ORB, pat: 10, grazed: false });
                    en.flash = 0.6;
                }
                EnemyKind::Elite if en.state == ELITE_SUMMONER => {
                    if down && self.beat_count % 8 < 4 && en.age > 1.0 {
                        calls.push(en.pos);
                        en.flash = 1.0;
                    }
                }
                EnemyKind::Elite if en.state == ELITE_LEAPER => {
                    if down && en.age > 1.0 {
                        let to = ppos - en.pos;
                        let dist = to.length();
                        if dist > 200.0 {
                            blasts.push(en.pos);
                            en.pos += to / dist * (dist - 170.0).min(190.0);
                            en.flash = 1.0;
                        }
                        // it lands with a short fan
                        let a0 = to.y.atan2(to.x);
                        for off in [-0.3f32, 0.0, 0.3] {
                            let v = dir(a0 + off);
                            new_bullets.push(Bullet { pos: en.pos + v * en.r, vel: v * speed * 0.9, r: 7.0, soft: off == 0.0, age: 0.0, fade: 1.0, kind: if off == 0.0 { B_NOTE } else { B_ORB }, pat: 11, grazed: false });
                        }
                    }
                }
                EnemyKind::Elite if elite_fire => {
                    let base = en.age;
                    for k in 0..8 {
                        let d = dir(base + k as f32 * TAU / 8.0);
                        let soft = k % 2 == 0;
                        new_bullets.push(Bullet { pos: en.pos + d * en.r, vel: d * speed * 0.85, r: 7.0, soft, age: 0.0, fade: 1.0, kind: if soft { B_NOTE } else { B_ORB }, pat: 11, grazed: false });
                    }
                    en.flash = 0.6;
                }
                _ => {}
            }
        }
        if self.bullets.len() + new_bullets.len() < MAX_BULLETS {
            self.bullets.extend(new_bullets);
        }
        for b in blasts {
            self.burst(b, 26, 520.0, C_DANGER, 5.0);
            self.snd(Sfx::Slam, 0.6, b.x);
        }
        // Summoners: two minis each, flung out to either side.
        for c in calls {
            for side in [-1.0f32, 1.0] {
                if self.enemies.len() < MAX_ENEMIES + 4 {
                    let a = self.rng.angle();
                    self.make_enemy(EnemyKind::Mini, c + dir(a) * 30.0);
                    if let Some(m) = self.enemies.last_mut() {
                        m.vel = dir(a + side) * 240.0;
                    }
                }
            }
            self.burst(c, 14, 320.0, C_SECONDARY, 4.0);
        }
        self.enemies.retain(|e| e.hp > -0.5);
        // Echo drones fire on every beat at the nearest target.
        let n = self.player.drones();
        let dmg = (1.0 + 0.5 * self.player.up(UpgradeId::Gain) as f32) * (self.beat_len / 0.5).clamp(0.5, 2.0);
        for k in 0..n {
            let from = self.player.drone_pos(k);
            let mut target = self.heart_target(from);
            let mut best = 520.0f32;
            for en in &self.enemies {
                let d = en.pos.distance(from);
                if d < best {
                    best = d;
                    target = Some(en.pos);
                }
            }
            if let Some(tp) = target {
                let d = (tp - from).normalize_or_zero();
                self.shots.push(Shot { bounce: 0, pos: from, vel: d * 1150.0, dmg, pierce: 0, life: 0.8, r: 4.0 });
            }
        }
    }

    fn emit_pulse(&mut self, pattern: Pattern, mut count: usize, speed: f32, rot: f32, mut soft: bool, late: f32, aim: f32) {
        self.heart.punch = 1.0;
        if count == 0 {
            return;
        }
        if self.heart.broken || self.purgatory {
            count = (count / 2).max(2);
            soft = true;
        }
        if self.bullets.len() + count > MAX_BULLETS {
            return;
        }
        let r = if soft { 8.0 } else { 7.0 };
        let kind = if soft { B_NOTE } else { B_ORB };
        let pat = pattern as u8;
        let origin = self.heart.pos;
        let push = |b: &mut Vec<Bullet>, a: f32, sp: f32| {
            let d = dir(a);
            let vel = d * sp;
            b.push(Bullet { pos: origin + d * (HEART_R * 0.85) + vel * late, vel, r, soft, age: 0.0, fade: 1.0, kind, pat, grazed: false });
        };
        let b = &mut self.bullets;
        match pattern {
            Pattern::Ring => {
                for k in 0..count {
                    push(b, rot + k as f32 * TAU / count as f32, speed);
                }
            }
            Pattern::Fan => {
                let n = count.max(1) | 1;
                for k in 0..n {
                    push(b, aim + (k as f32 - (n as f32 - 1.0) * 0.5) * FAN_STEP, speed);
                }
            }
            Pattern::Spiral => {
                let arms = 2usize;
                let per = (count / arms).max(1);
                for a in 0..arms {
                    for j in 0..per {
                        push(b, rot + a as f32 * TAU / arms as f32 + j as f32 * 0.12, speed * (1.0 - 0.09 * j as f32).max(0.4));
                    }
                }
            }
            Pattern::Cross => {
                let per = (count / 4).max(1);
                for a in 0..4 {
                    for j in 0..per {
                        push(b, aim + a as f32 * TAU / 4.0, speed * (1.0 - 0.11 * j as f32).max(0.35));
                    }
                }
            }
            Pattern::Wall => {
                // Dense arc centred on the player with one gap at `aim + rot`.
                let step = 2.0 * WALL_HALF / (count.max(2) - 1) as f32;
                for k in 0..count {
                    let off = -WALL_HALF + k as f32 * step;
                    if (off - rot).abs() < WALL_GAP_HALF {
                        continue;
                    }
                    push(b, aim + off, speed);
                }
            }
            Pattern::Burst => {
                for j in 0..count {
                    push(b, aim, speed * (1.0 - 0.085 * j as f32).max(0.4));
                }
            }
            Pattern::Pincer => {
                let per = (count / 2).max(1);
                for side in [-1.0f32, 1.0] {
                    for k in 0..per {
                        let f = if per > 1 { k as f32 / (per - 1) as f32 - 0.5 } else { 0.0 };
                        push(b, aim + side * PINCER_OFFSET + f * PINCER_SPREAD, speed);
                    }
                }
            }
            Pattern::Flower => {
                for k in 0..count {
                    let a = k as f32 * TAU / count as f32;
                    push(b, rot + a, speed * (0.78 + 0.22 * (5.0 * a).cos()));
                }
            }
            Pattern::Arc => {
                let span = TAU * 0.74;
                for k in 0..count {
                    let f = if count > 1 { k as f32 / (count - 1) as f32 } else { 0.5 };
                    push(b, rot + TAU * 0.13 + f * span, speed);
                }
            }
        }
    }

    /// One small fast dart from the Heart aimed at where the player is (hi-hat streams).
    fn emit_tick(&mut self, speed: f32, late: f32) {
        if self.heart.broken || self.bullets.len() >= MAX_BULLETS {
            return;
        }
        let d = (self.player.pos - self.heart.pos).normalize_or_zero();
        if d == Vec2::ZERO {
            return;
        }
        let vel = d * speed;
        self.bullets.push(Bullet { pos: self.heart.pos + d * (HEART_R * 0.9) + vel * late, vel, r: 4.5, soft: false, age: 0.0, fade: 1.0, kind: B_TICK, pat: 9, grazed: false });
    }

    /// The newest modifier of this depth, if the depth added one: (name, what it does).
    pub fn new_mod(&self) -> Option<(&'static str, &'static str)> {
        let n = self.song_idx as usize;
        if n == 0 || n > DEPTH_MODS {
            return None;
        }
        let m = (self.mod_offset as usize + n - 1) % DEPTH_MODS;
        // Already switched on in run setup: nothing new to announce.
        if self.base_mods >> m & 1 == 1 {
            return None;
        }
        Some(MODS[m])
    }

    /// Announce the depth the run has reached (call after `begin_song`).
    pub fn announce_depth(&mut self, title: &str) {
        self.text(vec2(0.0, -170.0), &format!("DEPTH {}", self.song_idx + 1), C_ACCENT, 58.0, 3.2);
        self.text(vec2(0.0, -126.0), title, C_WHITE, 24.0, 3.2);
        if let Some((name, what)) = self.new_mod() {
            self.text(vec2(0.0, 150.0), &format!("+ {name}"), C_DANGER, 30.0, 3.6);
            self.text(vec2(0.0, 180.0), what, C_WHITE, 20.0, 3.6);
        }
    }

    /// A reward the song placed (calm section, purgatory, track clear): the offer depends on
    /// how the player did since the last one. C = 1 choice, B = 3, A = 3 with a rare, S = rare
    /// and a heal.
    pub fn spawn_reward(&mut self, life: f32) {
        let w = self.window;
        let mut pts = match w.hits {
            0 => 2,
            1 => 1,
            _ => 0,
        };
        pts += w.broke_phase as usize + (w.peak_mult >= 3.0) as usize;
        let grade = match pts {
            0 => 0,
            1 | 2 => 1,
            3 => 2,
            _ => 3,
        };
        self.window = GradeWindow::default();
        self.last_grade = grade;
        self.grade_banner = 1.0;
        let why = format!(
            "{}{}{}",
            match w.hits {
                0 => "no hits".to_string(),
                1 => "1 hit".to_string(),
                n => format!("{n} hits"),
            },
            if w.broke_phase { "  +  phase broken" } else { "" },
            if w.peak_mult >= 3.0 { "  +  combo" } else { "" }
        );
        self.text(vec2(0.0, 96.0), &format!("GRADE {}", GRADES[grade]), if grade >= 2 { C_ACCENT } else { C_WHITE }, 40.0, 2.6);
        self.text(vec2(0.0, 126.0), &why, C_WHITE, 18.0, 2.6);
        if grade == 3 {
            self.drop_shards(vec2(0.0, 200.0), 1, true);
        }
        self.spawn_offer(life, if grade == 0 { 1 } else { 3 }, grade >= 2);
    }

    /// An ungraded offer of three common upgrades (level-ups).
    pub fn spawn_orbs(&mut self, life: f32) {
        self.spawn_offer(life, 3, false);
    }

    fn spawn_offer(&mut self, life: f32, choices: usize, rare: bool) {
        if !self.orbs.is_empty() {
            self.pending_orbs.push((choices, rare));
            return;
        }
        let open = |u: &UpgradeId| self.player.up(*u) < u.max();
        let mut pool: Vec<UpgradeId> = UpgradeId::ALL.iter().copied().filter(|u| !u.rare() && open(u)).collect();
        let rares: Vec<UpgradeId> = UpgradeId::ALL.iter().copied().filter(|u| u.rare() && open(u)).collect();
        let mut picks: Vec<(UpgradeId, bool)> = Vec::new();
        if rare && !rares.is_empty() {
            picks.push((rares[self.rng.below(rares.len())], true));
        }
        while picks.len() < choices && !pool.is_empty() {
            let i = self.rng.below(pool.len());
            picks.push((pool.swap_remove(i), false));
        }
        if picks.is_empty() {
            self.drop_shards(vec2(0.0, 250.0), 10, true);
            return;
        }
        let base = self.rng.angle();
        let n = picks.len();
        for (k, (up, rare)) in picks.into_iter().enumerate() {
            let a = base + k as f32 * TAU / n as f32;
            let pos = vec2(a.cos() * 430.0, a.sin() * 270.0);
            self.orbs.push(Orb { pos, up, rare, life, age: 0.0 });
        }
        let p = self.player.pos + vec2(0.0, -60.0);
        self.text(p, "UPGRADE READY", C_ACCENT, 26.0, 1.6);
    }

    fn apply_upgrade(&mut self, up: UpgradeId) {
        let p = &mut self.player;
        p.ups[up as usize] = (p.ups[up as usize] + 1).min(up.max());
        match up {
            UpgradeId::Sustain => {
                p.max_hp += 1;
                p.hp = (p.hp + 2).min(p.max_hp);
            }
            UpgradeId::Phase => p.dash_cd_max = 1.1 * 0.75f32.powi(p.ups[up as usize] as i32),
            UpgradeId::Encore => p.shield = true,
            _ => {}
        }
        // Every upgrade also patches you up a little.
        if up != UpgradeId::Sustain && self.player.hp < self.player.max_hp {
            self.player.hp += 1;
        }
        let pos = self.player.pos;
        self.text(pos + vec2(0.0, -50.0), up.name(), C_ACCENT, 34.0, 1.8);
        self.text(pos + vec2(0.0, -22.0), up.desc(), C_WHITE, 18.0, 1.8);
        self.burst(pos, 26, 420.0, C_ACCENT, 5.0);
        self.flash = self.flash.max(0.15);
        self.snd(Sfx::Upgrade, 0.9, pos.x);
    }

    fn hurt_player(&mut self, src: usize) {
        let p = &mut self.player;
        if p.invuln > 0.0 || p.dash_t > 0.0 || self.dead {
            return;
        }
        if p.shield {
            p.shield = false;
            p.invuln = 0.9;
            let pos = p.pos;
            self.burst(pos, 18, 380.0, C_PRIMARY, 4.0);
            self.text(pos + vec2(0.0, -40.0), "ENCORE", C_PRIMARY, 22.0, 1.0);
            self.shake = self.shake.max(0.3);
            self.snd(Sfx::Shield, 0.8, pos.x);
            return;
        }
        p.hp -= 1;
        // The breather after a hit shrinks with the tier; the hardest has no mercy window.
        let h = if self.mods & MOD_NO_MERCY != 0 { 1.0 } else { hardness(self.diff) };
        self.window.hits += 1;
        p.invuln = HIT_INVULN * (1.0 - 0.4 * h);
        let pos = p.pos;
        self.stats.hits += 1;
        self.stats.by_src[src.min(5)] += 1;
        self.snd(Sfx::Hurt, 1.0, pos.x);
        self.mercy = MERCY_DROP + (1.0 - MERCY_DROP) * h;
        self.combo *= 0.5;
        self.shake = self.shake.max(0.7);
        self.flash = self.flash.max(0.45);
        self.burst(pos, 22, 460.0, C_DANGER, 5.0);
        // NOVA: the hit throws out a blast that clears far more and hurts what it reaches.
        let nova = self.player.up(UpgradeId::Nova);
        if nova > 0 {
            let reach = 300.0 + 180.0 * nova as f32;
            for b in self.bullets.iter_mut() {
                if b.pos.distance(pos) < reach {
                    b.fade = b.fade.min(0.99);
                }
            }
            for en in self.enemies.iter_mut() {
                if en.pos.distance(pos) < reach && en.kind != EnemyKind::Sprite {
                    en.hp -= 3.0 * nova as f32;
                    en.flash = 1.0;
                }
            }
            self.burst(pos, 50, 900.0, C_ACCENT, 6.0);
            self.text(pos + vec2(0.0, -64.0), "NOVA", C_ACCENT, 26.0, 1.0);
        }
        // Mercy: clear nearby bullets and shove enemies away.
        for b in self.bullets.iter_mut() {
            if b.pos.distance(pos) < HIT_CLEAR_R {
                b.fade = b.fade.min(0.99);
            }
        }
        for en in self.enemies.iter_mut() {
            let d = en.pos - pos;
            let l = d.length();
            if l < HIT_CLEAR_R && l > 0.01 {
                en.vel = d / l * 420.0;
                en.pos += d / l * (HIT_CLEAR_R - l) * 0.5;
            }
        }
        if self.player.hp <= 0 {
            if self.no_fail {
                self.player.hp = self.player.max_hp;
                self.score = (self.score as f64 * 0.85) as u64;
                self.combo = 0.0;
                self.text(pos + vec2(0.0, -46.0), "REBOOT  -15% SCORE", C_DANGER, 24.0, 1.8);
            } else {
                self.dead = true;
                self.burst(pos, 60, 700.0, C_WHITE, 6.0);
                self.snd(Sfx::Death, 1.0, pos.x);
            }
        }
    }

    fn make_enemy(&mut self, kind: EnemyKind, pos: Vec2) {
        let (hp, r) = match kind {
            EnemyKind::Chaser => (2.0, 15.0),
            EnemyKind::Darter => (1.0, 13.0),
            EnemyKind::Orbiter => (5.0, 17.0),
            EnemyKind::Splitter => (5.0, 22.0),
            EnemyKind::Mini => (1.0, 9.0),
            EnemyKind::Wisp => (1.0, 11.0),
            EnemyKind::Sprite => (3.0, 16.0),
            EnemyKind::Turret => (6.0, 19.0),
            EnemyKind::Bomber => (3.0, 16.0),
            EnemyKind::Elite => (22.0 + 8.0 * self.diff + 6.0 * self.song_idx as f32, 34.0),
        };
        // Ordinary enemies toughen with depth (elites already scale).
        let hp = if matches!(kind, EnemyKind::Elite | EnemyKind::Wisp | EnemyKind::Mini | EnemyKind::Sprite) { hp } else { hp * (1.0 + 0.3 * self.song_idx as f32) };
        let mut vel = Vec2::ZERO;
        let mut state = 0.0;
        match kind {
            EnemyKind::Darter => {
                let d = (self.player.pos - pos).normalize_or_zero();
                vel = d * 640.0;
            }
            EnemyKind::Orbiter => state = pos.y.atan2(pos.x / 1.1),
            EnemyKind::Wisp | EnemyKind::Sprite => state = self.rng.angle(),
            // Elites come in three kinds (see `ELITE_*`); the run's depth decides which are in play.
            EnemyKind::Elite => state = self.rng.below((1 + self.song_idx as usize).min(3)) as f32,
            _ => {}
        }
        self.enemies.push(Enemy { kind, pos, vel, hp, max_hp: hp, r, rot: 0.0, age: 0.0, flash: 0.0, state });
    }

    fn kill_enemy(&mut self, kind: EnemyKind, pos: Vec2) {
        let (score, shards, col) = match kind {
            EnemyKind::Chaser => (50.0, 2, C_SECONDARY),
            EnemyKind::Darter => (40.0, 1, C_ACCENT),
            EnemyKind::Orbiter => (120.0, 3, C_PRIMARY),
            EnemyKind::Splitter => (150.0, 3, C_SECONDARY),
            EnemyKind::Mini => (20.0, 1, C_SECONDARY),
            EnemyKind::Wisp => (30.0, 2, C_ACCENT),
            EnemyKind::Sprite => (500.0, 7, C_ACCENT),
            EnemyKind::Turret => (140.0, 3, C_DANGER),
            EnemyKind::Bomber => (110.0, 3, C_ACCENT),
            EnemyKind::Elite => (600.0, 9, C_DANGER),
        };
        self.add_score(score);
        self.add_combo(1.0);
        self.stats.kills += 1;
        self.resonate(0.035);
        self.snd(Sfx::Kill, if kind == EnemyKind::Elite { 1.0 } else { 0.55 }, pos.x);
        self.drop_shards(pos, shards, kind == EnemyKind::Elite);
        self.burst(pos, 10 + shards * 4, 360.0, col, 5.0);
        if kind == EnemyKind::Elite {
            self.shake = self.shake.max(0.5);
        }
        if kind == EnemyKind::Sprite {
            self.drop_shards(pos, 1, true);
            self.text(pos + vec2(0.0, -40.0), "CAUGHT IT", C_ACCENT, 26.0, 1.6);
        }
        if kind == EnemyKind::Splitter {
            for k in 0..3 {
                let a = k as f32 * TAU / 3.0 + self.rng.angle() * 0.2;
                if self.enemies.len() < MAX_ENEMIES + 6 {
                    self.make_enemy(EnemyKind::Mini, pos + dir(a) * 18.0);
                    if let Some(m) = self.enemies.last_mut() {
                        m.vel = dir(a) * 260.0;
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Frame update
    // -----------------------------------------------------------------------------------------

    /// `beat_pos` is the continuous beat index at time `t`; `active` is false in drift mode.
    pub fn update(&mut self, dt: f32, t: f32, input: &Input, beat_pos: f32, feat: &Feat, intensity: f32, active: bool) {
        self.t = t;
        self.feat = *feat;
        self.intensity = intensity;
        if active {
            self.active_t += dt;
        }
        self.mercy = (self.mercy + dt * MERCY_RECOVER).min(1.0);
        self.chime = (self.chime - dt * 3.0).max(0.0);
        self.candy = (self.candy - dt * 0.7).max(0.0);
        // Ear-candy comets: sweep from one side to the other on an arc, dropping shards.
        let mut drops: Vec<Vec2> = Vec::new();
        for p in self.painters.iter_mut() {
            let k = ((t - p.t0) / (p.t1 - p.t0).max(0.1)).clamp(0.0, 1.0);
            let mid = (p.from + p.to) * 0.5 + vec2(0.0, 260.0);
            let a = p.from.lerp(mid, k);
            let b = mid.lerp(p.to, k);
            p.pos = a.lerp(b, k);
            if t >= p.next_drop && k < 1.0 {
                p.next_drop = t + 0.11;
                drops.push(p.pos);
            }
        }
        self.painters.retain(|p| t < p.t1);
        for d in drops {
            if (d - self.heart.pos).length() > HEART_R + 30.0 {
                self.shards.push(Shard { pos: d, vel: Vec2::ZERO, life: 8.0, value: 1, heal: false, age: 0.0 });
            }
            if self.parts.len() < MAX_PARTS {
                self.parts.push(Particle { pos: d, vel: Vec2::ZERO, life: 0.5, max: 0.5, size: 12.0, col: C_ACCENT, rot: 0.0 });
            }
        }
        for (k, g) in self.layer_glow.iter_mut().enumerate() {
            let want = if self.armed >> k & 1 == 1 { 1.0 } else { 0.0 };
            *g += (want - *g) * (dt * 4.0).min(1.0);
        }
        self.threat = self.pressure().min(1.0);

        // ---- decays ----
        self.beat_pulse = (self.beat_pulse - dt * 4.5).max(0.0);
        self.bar_pulse = (self.bar_pulse - dt * 2.2).max(0.0);
        self.shake = (self.shake - dt * 2.4).max(0.0);
        self.flash = (self.flash - dt * 2.0).max(0.0);
        self.section_banner = (self.section_banner - dt).max(0.0);
        self.warp = (self.warp - dt * 0.9).max(0.0);
        for h in self.hit.iter_mut() {
            *h = (*h - dt * HIT_DECAY).max(0.0);
        }
        // Groove: everything hostile surges on the beat and eases off before the next one.
        self.beat_phase = beat_pos.rem_euclid(1.0);
        self.groove = if active {
            1.0 + self.groove_amt * (GROOVE_PEAK * (-GROOVE_DECAY * self.beat_phase).exp() - GROOVE_MEAN)
        } else {
            1.0
        };
        // Announced pulses fire exactly on their kick.
        let mut pi = 0;
        while pi < self.pulses.len() {
            if t >= self.pulses[pi].t {
                let p = self.pulses.swap_remove(pi);
                if active {
                    self.emit_pulse(p.pattern, p.count as usize, p.speed, p.rot, p.soft, (t - p.t).clamp(0.0, 0.25), p.aim);
                }
            } else {
                pi += 1;
            }
        }
        self.heart.punch = (self.heart.punch - dt * 6.0).max(0.0);
        self.heart.flash = (self.heart.flash - dt * 8.0).max(0.0);
        self.heart.reform = (self.heart.reform - dt * 0.9).max(0.0);
        self.heart.rot += dt * (0.25 + 0.9 * intensity);
        if t >= self.heart.move_t0 && self.heart.move_t1 > self.heart.move_t0 {
            let k = ((t - self.heart.move_t0) / (self.heart.move_t1 - self.heart.move_t0)).clamp(0.0, 1.0);
            self.heart.pos = self.heart.move_from.lerp(self.heart.move_to, smooth01(k));
            if k >= 1.0 && self.heart.charging {
                self.heart.charging = false;
                self.shake = self.shake.max(0.35);
                let at = self.heart.pos;
                self.burst(at, 24, 520.0, C_DANGER, 5.0);
            }
        }
        // Valve: open for the first half of every bar. The other forms are simply open or shut.
        let open_to = match self.heart.form {
            HeartForm::Valve => (self.bar_beat < 2) as u8 as f32,
            HeartForm::Shell | HeartForm::Split => 0.0,
            _ => 1.0,
        };
        self.heart.open += (open_to - self.heart.open) * (dt * 12.0).min(1.0);
        let spin = if self.heart.form == HeartForm::Shell { 0.12 } else { 0.55 };
        for n in self.heart.nodes.iter_mut() {
            n.ang += dt * spin * if active { 1.0 } else { 0.2 };
            n.flash = (n.flash - dt * 7.0).max(0.0);
        }
        self.purg += ((self.purgatory as u8 as f32) - self.purg) * (dt * 3.0).min(1.0);
        if self.purgatory && !self.heart.broken && self.t >= self.purg_until && (self.phase_left() > 4.0 || self.live_phases) {
            self.encore();
        }
        self.window.peak_mult = self.window.peak_mult.max(self.mult());
        self.res_t = (self.res_t - dt).max(0.0);
        // DODGE: staying alive is the score.
        if self.pacifist && active && !self.dead {
            self.survive_acc += dt * 20.0;
            if self.survive_acc >= 10.0 {
                self.survive_acc -= 10.0;
                self.add_score(10.0);
            }
        }
        self.grade_banner = (self.grade_banner - dt / 3.0).max(0.0);
        self.form_banner = (self.form_banner - dt / 3.5).max(0.0);
        self.combo_idle += dt;
        if self.combo_idle > 3.0 {
            self.combo = (self.combo - dt * (0.8 + self.combo * 0.05)).max(0.0);
        }

        if !self.dead {
            self.update_player(dt, t, input, beat_pos, active);
        }

        // ---- shots ----
        let mut i = 0;
        while i < self.shots.len() {
            let s = &mut self.shots[i];
            s.pos += s.vel * dt;
            s.life -= dt;
            if s.bounce > 0 && (s.pos.x.abs() > HALF_W || s.pos.y.abs() > HALF_H) {
                if s.pos.x.abs() > HALF_W {
                    s.vel.x = -s.vel.x;
                }
                if s.pos.y.abs() > HALF_H {
                    s.vel.y = -s.vel.y;
                }
                s.pos.x = s.pos.x.clamp(-HALF_W, HALF_W);
                s.pos.y = s.pos.y.clamp(-HALF_H, HALF_H);
                s.bounce -= 1;
                s.life += 0.5;
            }
            let out = s.pos.x.abs() > HALF_W + 60.0 || s.pos.y.abs() > HALF_H + 60.0;
            if s.life <= 0.0 || out {
                self.shots.swap_remove(i);
            } else {
                i += 1;
            }
        }
        self.collide_shots();
        self.update_blades(dt);

        // ---- portals -> enemies ----
        let mut k = 0;
        while k < self.portals.len() {
            if t >= self.portals[k].t1 {
                let p = self.portals.swap_remove(k);
                self.make_enemy(p.kind, p.pos);
                self.burst(p.pos, 8, 260.0, C_SECONDARY, 4.0);
            } else {
                k += 1;
            }
        }

        self.update_enemies(dt, active);
        self.update_bullets(dt, active);
        self.update_hazards(dt, t, active);
        self.update_pickups(dt);

        // ---- particles / texts ----
        for p in self.parts.iter_mut() {
            p.pos += p.vel * dt;
            p.vel *= 1.0 - (3.0 * dt).min(0.9);
            p.life -= dt;
        }
        self.parts.retain(|p| p.life > 0.0);
        for tx in self.texts.iter_mut() {
            tx.pos.y -= 26.0 * dt;
            tx.life -= dt;
        }
        self.texts.retain(|t| t.life > 0.0);
    }

    fn update_player(&mut self, dt: f32, t: f32, input: &Input, beat_pos: f32, active: bool) {
        let glide = self.player.up(UpgradeId::Glide) as f32;
        let p = &mut self.player;
        p.invuln = (p.invuln - dt).max(0.0);
        p.dash_cd = (p.dash_cd - dt).max(0.0);
        p.muzzle = (p.muzzle - dt * 9.0).max(0.0);
        p.drone_ang += dt * 2.4;
        let aim = input.aim - p.pos;
        if aim.length_squared() > 4.0 {
            p.aim_dir = aim.normalize();
        }
        let mv = if input.mv.length_squared() > 1.0 { input.mv.normalize() } else { input.mv };
        let mut dashed = false;
        let mut on_beat_dash = false;
        if input.dash && p.dash_cd <= 0.0 && p.dash_t <= 0.0 {
            p.dash_t = 0.16;
            p.dash_cd = p.dash_cd_max;
            // SYNCOPE: on the beat the dash is free (half price at level 1).
            let syn = p.ups[UpgradeId::Syncope as usize];
            let off_beat = self.beat_phase.min(1.0 - self.beat_phase) * self.beat_len;
            if syn > 0 && off_beat < SYNCOPE_WINDOW {
                p.dash_cd = if syn >= 2 { 0.0 } else { p.dash_cd_max * 0.35 };
                on_beat_dash = true;
            }
            p.dash_dir = if mv.length_squared() > 0.01 { mv.normalize() } else { p.aim_dir };
            dashed = true;
        }
        if dashed {
            let pos = p.pos;
            self.snd(Sfx::Dash, 0.5, pos.x);
            if on_beat_dash {
                self.resonate(0.06);
                self.text(pos + vec2(0.0, -34.0), "ON BEAT", C_ACCENT, 18.0, 0.7);
                self.snd(Sfx::Shard, 0.7, pos.x);
            }
            // TRANSPOSE: the dash rewrites nearby bullets into shootable notes.
            if self.player.up(UpgradeId::Transpose) > 0 {
                let mut n = 0;
                for b in self.bullets.iter_mut() {
                    if !b.soft && b.pos.distance(pos) < 240.0 {
                        b.soft = true;
                        b.kind = B_NOTE;
                        b.r = 8.0;
                        n += 1;
                    }
                }
                if n > 0 {
                    self.burst(pos, 14, 520.0, C_ACCENT, 4.0);
                }
            }
        }
        let p = &mut self.player;
        let mut spawn_after = false;
        if p.dash_t > 0.0 {
            p.dash_t -= dt;
            p.vel = p.dash_dir * 1450.0;
            p.invuln = p.invuln.max(0.1);
            spawn_after = true;
        } else {
            let speed = 430.0 * (1.0 + 0.12 * glide);
            let target = mv * speed;
            let k = (16.0 * dt).min(1.0);
            p.vel += (target - p.vel) * k;
        }
        p.pos += p.vel * dt;
        p.pos.x = p.pos.x.clamp(-HALF_W + PLAYER_R, HALF_W - PLAYER_R);
        p.pos.y = p.pos.y.clamp(-HALF_H + PLAYER_R, HALF_H - PLAYER_R);
        // The Heart is solid.
        let hpos = self.heart.pos;
        let rel = p.pos - hpos;
        let d = rel.length();
        let min_d = HEART_R + PLAYER_R;
        let mut touched = false;
        if d < min_d {
            let n = if d > 0.01 { rel / d } else { vec2(0.0, 1.0) };
            p.pos = hpos + n * min_d;
            touched = true;
        }
        let pos = p.pos;
        if spawn_after && self.parts.len() < MAX_PARTS {
            self.parts.push(Particle { pos, vel: Vec2::ZERO, life: 0.22, max: 0.22, size: PLAYER_R * 1.6, col: C_PRIMARY, rot: 0.0 });
        }
        if touched && active && !self.heart.broken {
            self.hurt_player(SRC_HEART);
        }

        // ---- the gun ----
        // With a gun track the gun plays the song's own rhythm (drum hits, filled on the beat
        // grid). Without one it locks to the beat subdivision nearest the target interval. Either
        // way a shot's damage is proportional to the gap before it, so the damage per second
        // depends only on the TEMPO level, never on the song.
        let lv = (self.player.up(UpgradeId::Tempo) as usize).min(GUN_INTERVALS.len() - 1);
        let target = GUN_INTERVALS[lv];
        let mut shot: Option<f32> = None;
        let idx;
        if let Some(track) = self.gun.get(lv).filter(|v| !v.is_empty()) {
            idx = track.partition_point(|&x| x <= t) as i64;
            if idx > self.player.fire_phase && idx > 0 {
                let k = idx as usize - 1;
                let gap = if k > 0 { track[k] - track[k - 1] } else { target };
                shot = Some(gun_damage(gap, target));
            }
        } else {
            let bl = self.beat_len.clamp(0.2, 2.0);
            let mut div = 1.0f32;
            let mut best = f32::MAX;
            for &d in FIRE_DIVS.iter() {
                let err = ((bl / d) / target).ln().abs();
                if err < best {
                    best = err;
                    div = d;
                }
            }
            idx = (beat_pos * div).floor() as i64;
            if idx != self.player.fire_phase {
                shot = Some(gun_damage(bl / div, target));
            }
        }
        let same_lv = lv == self.gun_lv;
        self.gun_lv = lv;
        self.player.fire_phase = idx;
        if let Some(scale) = shot {
            if input.fire && same_lv {
                self.fire(scale);
            }
        }
    }

    /// Fire the gun. `scale` is the shot's damage multiplier (`gun_damage`): heavier shots are
    /// drawn larger and get a soft tick.
    fn fire(&mut self, scale: f32) {
        if self.pacifist {
            return;
        }
        // Resonance adds damage on top of the shot (it is not part of the gun's own rhythm).
        let res_boost = if self.res_t > 0.0 { RESONANCE_DMG } else { 1.0 };
        // GLASS: fragile, but it hits hard.
        let scale = if self.mods & MOD_GLASS != 0 { scale * 1.6 } else { scale };
        // DOWNBEAT: the first shot after each downbeat is charged.
        let charged = std::mem::take(&mut self.downbeat_ready);
        let scale = if charged { scale * (1.0 + 1.5 * self.player.up(UpgradeId::Downbeat) as f32) } else { scale };
        // CRESCENDO: the combo multiplier feeds the gun. ACCENT: some shots land much harder.
        let scale = scale * (1.0 + 0.06 * self.player.up(UpgradeId::Crescendo) as f32 * (self.mult() - 1.0));
        let accent = self.player.up(UpgradeId::Accent);
        let crit = accent > 0 && self.rng.chance(ACCENT_CHANCE * accent as f32);
        let scale = if crit { scale * ACCENT_MUL } else { scale };
        let p = &mut self.player;
        let bounce = p.ups[UpgradeId::Ricochet as usize];
        let n = 1 + p.ups[UpgradeId::Chord as usize] as usize;
        let dmg = (1.0 + 0.5 * p.ups[UpgradeId::Gain as usize] as f32) * scale * res_boost;
        let pierce = p.ups[UpgradeId::Pierce as usize];
        let base = p.aim_dir.y.atan2(p.aim_dir.x);
        p.muzzle = scale.clamp(0.7, 1.6);
        let r = 5.0 * scale.sqrt().clamp(0.8, 1.5);
        let x = p.pos.x;
        for k in 0..n {
            let off = (k as f32 - (n as f32 - 1.0) * 0.5) * 0.11;
            let d = dir(base + off);
            self.shots.push(Shot { bounce, pos: p.pos + d * 15.0, vel: d * 1250.0, dmg, pierce, life: 0.85, r });
        }
        self.stats.gun_dmg += scale;
        if scale >= GUN_TICK_FROM {
            self.snd(Sfx::Shot, (0.3 + 0.3 * (scale - GUN_TICK_FROM)).min(0.6), x);
        }
    }

    /// ORBIT: the blades cut enemies they touch and pop shootable notes.
    fn update_blades(&mut self, dt: f32) {
        let n = self.player.blades();
        if n == 0 || self.dead {
            return;
        }
        let pts: Vec<Vec2> = (0..n).map(|k| self.player.blade_pos(k)).collect();
        let mut killed: Vec<(EnemyKind, Vec2)> = Vec::new();
        let mut i = 0;
        while i < self.enemies.len() {
            let en = &mut self.enemies[i];
            if en.kind != EnemyKind::Sprite && pts.iter().any(|p| p.distance(en.pos) < BLADE_REACH + en.r) {
                en.hp -= BLADE_DPS * dt;
                en.flash = en.flash.max(0.5);
                if en.hp <= 0.0 {
                    let dead = self.enemies.swap_remove(i);
                    killed.push((dead.kind, dead.pos));
                    continue;
                }
            }
            i += 1;
        }
        for (kind, pos) in killed {
            self.kill_enemy(kind, pos);
        }
        let mut popped = 0u32;
        for b in self.bullets.iter_mut() {
            if b.soft && b.fade >= 1.0 && pts.iter().any(|p| p.distance(b.pos) < BLADE_REACH + b.r) {
                b.fade = 0.99;
                popped += 1;
            }
        }
        if popped > 0 {
            self.stats.notes_shot += popped;
            self.add_score(15.0 * popped as f32);
        }
    }

    /// 0..1 through the current bar.
    pub fn bar_phase(&self) -> f32 {
        ((self.bar_beat as f32 + self.beat_phase) / 4.0).clamp(0.0, 1.0)
    }

    fn collide_shots(&mut self) {
        let mut killed: Vec<(EnemyKind, Vec2)> = Vec::new();
        let mut heart_dmg = 0.0f32;
        let mut node_hits: Vec<(usize, f32)> = Vec::new();
        let mut sparks: Vec<(Vec2, u8)> = Vec::new();
        let mut notes: Vec<Vec2> = Vec::new();
        let mut si = 0;
        'shots: while si < self.shots.len() {
            let (sp, sr, dmg) = {
                let s = &self.shots[si];
                (s.pos, s.r, s.dmg)
            };
            // Heart: its nodes first, then the body (how much lands depends on the form).
            if !self.heart.broken && !self.purgatory {
                let lit = self.heart.lit_node();
                for k in 0..self.heart.nodes.len() {
                    if self.heart.nodes[k].hp > 0.0 && self.heart.node_pos(k).distance(sp) < NODE_R + sr {
                        if self.heart.form == HeartForm::Shell || lit == Some(k) {
                            node_hits.push((k, dmg));
                            sparks.push((sp, C_WHITE));
                        } else {
                            sparks.push((sp, C_SECONDARY));
                        }
                        self.shots.swap_remove(si);
                        continue 'shots;
                    }
                }
                if (sp - self.heart.pos).length() < HEART_R + sr {
                    let mul = match self.heart.form {
                        HeartForm::Core => 1.0,
                        HeartForm::Overdrive => OVERDRIVE_DMG,
                        HeartForm::Valve => if self.heart.open > 0.5 { VALVE_OPEN_DMG } else { VALVE_SHUT_DMG },
                        HeartForm::Shell | HeartForm::Split => 0.0,
                    };
                    heart_dmg += dmg * mul;
                    sparks.push((sp, if mul >= 1.0 { C_WHITE } else { C_SECONDARY }));
                    self.shots.swap_remove(si);
                    continue 'shots;
                }
            }
            // Enemies
            let mut consumed = false;
            for ei in 0..self.enemies.len() {
                let en = &mut self.enemies[ei];
                if en.pos.distance(sp) < en.r + sr + 2.0 {
                    en.hp -= dmg;
                    en.flash = 1.0;
                    sparks.push((sp, C_WHITE));
                    if en.hp <= 0.0 {
                        let dead = self.enemies.swap_remove(ei);
                        killed.push((dead.kind, dead.pos));
                    }
                    let s = &mut self.shots[si];
                    if s.pierce > 0 {
                        s.pierce -= 1;
                        // Nudge forward so the same target is not hit again next frame.
                        s.pos += s.vel.normalize_or_zero() * 34.0;
                    } else {
                        consumed = true;
                    }
                    break;
                }
            }
            if consumed {
                self.shots.swap_remove(si);
                continue 'shots;
            }
            // Soft (destructible) bullets
            for bi in 0..self.bullets.len() {
                let b = &self.bullets[bi];
                if b.soft && b.fade >= 1.0 && b.pos.distance(sp) < b.r + sr + 5.0 {
                    notes.push(b.pos);
                    self.bullets.swap_remove(bi);
                    let s = &mut self.shots[si];
                    if s.pierce > 0 {
                        s.pierce -= 1;
                    } else {
                        self.shots.swap_remove(si);
                        continue 'shots;
                    }
                    break;
                }
            }
            si += 1;
        }
        for (pos, col) in sparks {
            self.burst(pos, 3, 240.0, col, 3.0);
        }
        if !notes.is_empty() {
            let x = notes[0].x;
            self.snd(Sfx::NoteShot, 0.5, x);
        }
        for pos in notes {
            self.stats.notes_shot += 1;
            self.add_score(15.0);
            self.add_combo(0.25);
            self.burst(pos, 5, 220.0, C_WHITE, 3.5);
            if self.rng.chance(0.4) {
                self.drop_shards(pos, 1, false);
            }
        }
        for (kind, pos) in killed {
            self.kill_enemy(kind, pos);
        }
        for (k, dmg) in node_hits {
            let Some(n) = self.heart.nodes.get_mut(k) else { continue };
            let take = dmg.min(n.hp);
            n.hp -= take;
            n.flash = 1.0;
            let dead = n.hp <= 0.0;
            if dead {
                let pos = self.heart.node_pos(k);
                self.burst(pos, 26, 480.0, C_PRIMARY, 6.0);
                self.drop_shards(pos, 3, false);
                self.snd(Sfx::Kill, 0.9, pos.x);
                self.add_score(400.0);
                self.shake = self.shake.max(0.35);
            }
            self.damage_heart(take);
        }
        self.damage_heart(heart_dmg);
    }

    fn update_enemies(&mut self, dt: f32, active: bool) {
        let ppos = self.player.pos;
        let hpos = self.heart.pos;
        let slow = if active { self.groove } else { 0.15 };
        let intensity = self.intensity;
        let n = self.enemies.len();
        // Simple separation so chasers do not stack.
        let positions: Vec<(Vec2, f32)> = self.enemies.iter().map(|e| (e.pos, e.r)).collect();
        let mut touched = false;
        let mut trails: Vec<Vec2> = Vec::new();
        for (i, en) in self.enemies.iter_mut().enumerate() {
            en.age += dt;
            en.flash = (en.flash - dt * 7.0).max(0.0);
            let to_p = ppos - en.pos;
            let dist = to_p.length().max(0.001);
            match en.kind {
                EnemyKind::Chaser | EnemyKind::Mini | EnemyKind::Splitter | EnemyKind::Elite => {
                    let max = match en.kind {
                        EnemyKind::Chaser => 110.0 + 60.0 * intensity,
                        EnemyKind::Mini => 215.0,
                        EnemyKind::Splitter => 88.0,
                        _ => 58.0,
                    };
                    let mut acc = to_p / dist * max * 3.0;
                    for (j, &(op, or)) in positions.iter().enumerate() {
                        if j == i || j >= n {
                            continue;
                        }
                        let d = en.pos - op;
                        let l = d.length();
                        let min = en.r + or + 6.0;
                        if l < min && l > 0.01 {
                            acc += d / l * (min - l) * 30.0;
                        }
                    }
                    en.vel += acc * dt;
                    let sp = en.vel.length();
                    if sp > max {
                        en.vel *= max / sp;
                    }
                    en.pos += en.vel * dt * slow;
                    en.rot = en.vel.y.atan2(en.vel.x);
                    // Stay off the Heart.
                    let hd = (en.pos - hpos).length();
                    if hd < HEART_R + en.r + 6.0 && hd > 0.01 {
                        en.pos = hpos + (en.pos - hpos) / hd * (HEART_R + en.r + 6.0);
                    }
                }
                EnemyKind::Darter => {
                    en.pos += en.vel * dt * slow;
                    en.rot = en.vel.y.atan2(en.vel.x);
                    if (en.age * 60.0) as i32 % 2 == 0 {
                        trails.push(en.pos);
                    }
                    if en.pos.x.abs() > HALF_W + 90.0 || en.pos.y.abs() > HALF_H + 90.0 {
                        en.hp = -1.0; // left the arena
                    }
                }
                EnemyKind::Turret => {
                    // Stands its ground and turns to face the player.
                    en.rot = to_p.y.atan2(to_p.x);
                    en.pos.x = en.pos.x.clamp(-HALF_W + 40.0, HALF_W - 40.0);
                    en.pos.y = en.pos.y.clamp(-HALF_H + 40.0, HALF_H - 40.0);
                }
                EnemyKind::Bomber => {
                    // Drifts in; once close it stops and arms (`state` counts the beats left).
                    if en.state <= 0.0 {
                        en.pos += to_p / dist * 100.0 * dt * slow;
                        if dist < BOMBER_ARM_R && active {
                            en.state = 2.99;
                            en.flash = 1.0;
                        }
                    }
                    en.rot += dt * (2.0 + 6.0 * en.state);
                }
                EnemyKind::Sprite => {
                    // Loops around the arena on a figure of eight, then leaves.
                    let a = en.state + en.age * 1.5;
                    let target = vec2(a.cos() * HALF_W * 0.62, (2.0 * a).sin() * HALF_H * 0.5);
                    en.pos += (target - en.pos) * (2.5 * dt).min(1.0);
                    en.rot += dt * 5.0;
                    if (en.age * 30.0) as i32 % 2 == 0 {
                        trails.push(en.pos);
                    }
                    if en.age > 8.0 {
                        en.hp = -1.0;
                    }
                }
                EnemyKind::Wisp => {
                    // Drifts at the player on a weaving path and fades out if it is ignored.
                    let side = vec2(-to_p.y, to_p.x) / dist;
                    let v = to_p / dist * 85.0 + side * (en.age * 3.0 + en.state).sin() * 120.0;
                    en.pos += v * dt * slow;
                    en.rot += dt * 2.0;
                    if en.age > 10.0 {
                        en.hp = -1.0;
                    }
                }
                EnemyKind::Orbiter => {
                    en.state += dt * 0.55 * slow;
                    let target = vec2(en.state.cos() * 363.0, en.state.sin() * 300.0);
                    en.pos += (target - en.pos) * (3.0 * dt).min(1.0);
                    en.rot += dt * 1.5;
                }
            }
            if active && en.kind != EnemyKind::Sprite && dist < en.r + PLAYER_HIT_R + 4.0 {
                touched = true;
                if matches!(en.kind, EnemyKind::Mini | EnemyKind::Darter | EnemyKind::Wisp) {
                    en.hp = -1.0;
                }
            }
        }
        for p in trails {
            if self.parts.len() < MAX_PARTS {
                self.parts.push(Particle { pos: p, vel: Vec2::ZERO, life: 0.25, max: 0.25, size: 9.0, col: C_ACCENT, rot: 0.0 });
            }
        }
        self.enemies.retain(|e| e.hp > -0.5);
        if touched {
            self.hurt_player(SRC_ENEMY);
        }
    }

    fn update_bullets(&mut self, dt: f32, active: bool) {
        let ppos = self.player.pos;
        let mut hit = false;
        let mut hit_pat = 0u8;
        let mut grazes = 0u32;
        let vulnerable = self.player.invuln <= 0.0 && self.player.dash_t <= 0.0 && !self.dead;
        let groove = self.groove;
        for b in self.bullets.iter_mut() {
            // Fresh bullets leave at their plain speed; the beat surge fades in as they travel.
            let g = 1.0 + (groove - 1.0) * (b.age / 0.35).min(1.0);
            b.pos += b.vel * dt * g;
            b.age += dt;
            if !active {
                b.fade = b.fade.min(0.99);
            }
            if b.fade < 1.0 {
                b.fade -= dt * 3.5;
                continue;
            }
            if b.pos.x.abs() > HALF_W + 140.0 || b.pos.y.abs() > HALF_H + 140.0 {
                b.fade = -1.0;
                continue;
            }
            let d2 = b.pos.distance_squared(ppos);
            if vulnerable && d2 < (b.r + PLAYER_HIT_R) * (b.r + PLAYER_HIT_R) {
                hit = true;
                hit_pat = b.pat;
                self.last_hit_age = b.age;
                b.fade = -1.0;
            } else if !b.grazed && !b.soft && !self.dead && d2 < (b.r + GRAZE_R) * (b.r + GRAZE_R) {
                b.grazed = true;
                grazes += 1;
            }
        }
        if grazes > 0 && !hit {
            self.stats.grazes += grazes;
            self.resonate(0.022 * grazes as f32);
            let k = grazes as f32 * if self.pacifist { 3.0 } else { 1.0 };
            self.add_score(GRAZE_SCORE * k);
            self.add_combo(0.25 * grazes as f32);
            if self.parts.len() < MAX_PARTS {
                self.burst(ppos, 3, 220.0, C_WHITE, 2.5);
            }
            if self.shard_snd_cd <= 0.0 {
                self.shard_snd_cd = 0.06;
                self.snd(Sfx::NoteShot, 0.35, ppos.x);
            }
        }
        self.bullets.retain(|b| b.fade > 0.0);
        if hit {
            let before = self.stats.hits;
            self.hurt_player(SRC_BULLET);
            if self.stats.hits > before {
                self.stats.by_pat[(hit_pat as usize).min(11)] += 1;
            }
        }
    }

    fn update_hazards(&mut self, dt: f32, t: f32, active: bool) {
        let ppos = self.player.pos;
        let mut hurt: Option<usize> = None;
        let mut tile_tag_hit = 0u8;
        let mut slammed = false;

        // Tiles
        let ptile = tile_of(ppos);
        for r in 0..ROWS {
            for c in 0..COLS {
                let tile = &mut self.tiles[r * COLS + c];
                tile.flash = (tile.flash - dt * 3.5).max(0.0);
                if tile.armed && t >= tile.strike_t {
                    tile.armed = false;
                    // Ignore strikes that are badly stale (e.g. after a hitch).
                    if t - tile.strike_t < 0.3 {
                        tile.flash = 1.0;
                        slammed = true;
                        if active && ptile == Some((c, r)) {
                            tile_tag_hit = tile.tag;
                            hurt = Some(SRC_TILE);
                        }
                    }
                }
            }
        }

        if slammed && active {
            self.snd(Sfx::Slam, 0.5, 0.0);
        }

        // Lasers
        for l in &self.lasers {
            if active && l.active(t) {
                let pos = l.pos_at(t);
                let d = if l.horizontal { ppos.y - pos } else { ppos.x - pos };
                if d.abs() < 13.0 + PLAYER_HIT_R {
                    hurt = Some(SRC_LASER);
                }
            }
        }
        self.lasers.retain(|l| t < l.t0 + l.dur + 0.3 && t > l.t0 - l.lead - 1.0);

        // Shockwaves
        self.heart.charge = 0.0;
        for w in &self.waves {
            if t < w.t0 {
                self.heart.charge = self.heart.charge.max(((t - (w.t0 - w.lead)) / w.lead).clamp(0.0, 1.0));
            } else if active {
                let rad = w.radius(t);
                let rel = ppos - w.origin;
                let d = rel.length();
                if (d - rad).abs() < 12.0 + PLAYER_HIT_R && !w.in_gap(rel.y.atan2(rel.x)) {
                    hurt = Some(SRC_WAVE);
                }
            }
        }
        self.waves.retain(|w| w.radius(t) < 980.0);
        if let Some(src) = hurt {
            let before = self.stats.hits;
            self.hurt_player(src);
            if src == SRC_TILE && self.stats.hits > before {
                self.stats.by_tile[(tile_tag_hit as usize).min(8)] += 1;
            }
        }
    }

    fn update_pickups(&mut self, dt: f32) {
        let ppos = self.player.pos;
        let mag = self.player.magnet_r() * if self.res_t > 0.0 { 2.0 } else { 1.0 };
        let mut got = 0u32;
        let mut healed = false;
        for s in self.shards.iter_mut() {
            s.age += dt;
            s.life -= dt;
            let to = ppos - s.pos;
            let d = to.length().max(0.001);
            if d < mag && !self.dead {
                let pull = 2400.0 * (1.0 - d / mag).max(0.15);
                s.vel += to / d * pull * dt;
            }
            s.vel *= 1.0 - (2.2 * dt).min(0.9);
            s.pos += s.vel * dt;
            if d < 22.0 && !self.dead {
                s.life = -1.0;
                got += s.value as u32;
                healed |= s.heal;
            }
        }
        self.shards.retain(|s| s.life > 0.0);
        self.shard_snd_cd = (self.shard_snd_cd - dt).max(0.0);
        if got > 0 && self.shard_snd_cd <= 0.0 {
            self.shard_snd_cd = 0.07;
            self.snd(Sfx::Shard, 0.45, ppos.x);
        }
        if got > 0 {
            self.stats.shards += got;
            self.player.xp += got;
            self.add_score(10.0 * got as f32);
            self.add_combo(0.34 * got as f32);
            if self.parts.len() < MAX_PARTS {
                self.burst(ppos, 2, 160.0, C_ACCENT, 3.0);
            }
        }
        if healed && self.player.hp < self.player.max_hp {
            self.player.hp += 1;
            self.text(ppos + vec2(0.0, -40.0), "+1 HP", C_PRIMARY, 24.0, 1.2);
            self.snd(Sfx::Heal, 0.8, ppos.x);
        }
        if self.player.xp >= self.player.xp_next {
            self.player.xp -= self.player.xp_next;
            self.player.level += 1;
            let l = self.player.level;
            self.player.xp_next = XP_BASE + XP_LIN * l + XP_QUAD * l * l;
            self.spawn_orbs(14.0);
        }

        // Upgrade orbs
        let mut taken: Option<UpgradeId> = None;
        for o in self.orbs.iter_mut() {
            o.age += dt;
            o.life -= dt;
            if o.age > 0.4 && o.pos.distance(ppos) < 36.0 && !self.dead {
                taken = Some(o.up);
            }
        }
        if let Some(up) = taken {
            self.orbs.clear();
            self.apply_upgrade(up);
        }
        self.orbs.retain(|o| o.life > 0.0);
        if self.orbs.is_empty() && !self.pending_orbs.is_empty() {
            let (choices, rare) = self.pending_orbs.remove(0);
            self.spawn_offer(14.0, choices, rare);
        }
    }

    /// Wipe hostile things (used when the music stops or a song ends).
    pub fn calm_down(&mut self) {
        for b in self.bullets.iter_mut() {
            b.fade = b.fade.min(0.99);
        }
        self.lasers.clear();
        self.waves.clear();
        self.portals.clear();
        self.pulses.clear();
        for t in self.tiles.iter_mut() {
            t.armed = false;
        }
        let dead: Vec<(EnemyKind, Vec2)> = self.enemies.iter().map(|e| (e.kind, e.pos)).collect();
        self.enemies.clear();
        for (_, pos) in dead {
            self.burst(pos, 8, 300.0, C_SECONDARY, 4.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::{Band, Onset};

    /// Hold fire for 60 s of a steady song at `bpm` and return the damage dealt, in nominal shots.
    fn gun_minute(bpm: f32, tempo_lv: u8) -> f32 {
        let bl = 60.0 / bpm;
        let beats: Vec<f32> = (0..(70.0 / bl) as usize).map(|i| 0.2 + i as f32 * bl).collect();
        // Hats on the off-beats and a snare on every second beat, a little off the grid.
        let mut onsets: Vec<Onset> = Vec::new();
        for (i, &b) in beats.iter().enumerate() {
            onsets.push(Onset { t: b + 0.5 * bl + 0.004, band: Band::High, strength: 0.6 });
            if i % 2 == 1 {
                onsets.push(Onset { t: b - 0.003, band: Band::Mid, strength: 1.0 });
            }
        }
        let mut w = World::new(1.0, true);
        w.begin_song(60.0, 0, 1);
        w.beat_len = bl;
        w.gun = gun_track(&beats, &onsets, 70.0);
        w.player.ups[UpgradeId::Tempo as usize] = tempo_lv;
        let input = Input { fire: true, ..Input::default() };
        let dt = 1.0 / 120.0;
        let mut t = 0.0f32;
        while t < 60.0 {
            w.update(dt, t, &input, beat_pos(&beats, t), &Feat::default(), 0.5, true);
            t += dt;
        }
        w.stats.gun_dmg
    }

    #[test]
    fn gun_dps_is_the_same_at_every_tempo() {
        for lv in 0..GUN_INTERVALS.len() as u8 {
            let want = 60.0 / GUN_INTERVALS[lv as usize];
            for bpm in [70.0, 110.0, 170.0] {
                let got = gun_minute(bpm, lv);
                assert!((got / want - 1.0).abs() < 0.02, "level {lv} at {bpm} BPM: {got} vs {want}");
            }
        }
    }
}
