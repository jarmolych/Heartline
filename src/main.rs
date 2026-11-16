//! Heartline — your music becomes a top-down vaporwave bullet-hell.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod analysis;
mod decode;
mod display;
mod engine;
mod game;
mod level;
mod net;
mod rally;
mod render;
mod rng;
mod runner;
mod synth;
mod tools;
mod ui;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

use macroquad::prelude::*;

use analysis::{Analysis, Band, Feat, SectionKind, StreamAnalyzer, StreamOut};
use decode::Track;
use engine::{Audio, Sfx};
use game::{Input, UpgradeId, World};
use level::{EnemyKind, Ev, Level, LevelEvent, LiveGen, Pattern, Timeline};
use rally::{Character, FxKind, Rally, RallyInput, RallyMusic};
use render::{text_center, text_left, text_shadow, text_w, HudInfo, Renderer};
use ui::Icon;

// ---------------------------------------------------------------------------------------------
// Command line
// ---------------------------------------------------------------------------------------------

#[derive(Default, Clone)]
pub struct Args {
    files: Vec<PathBuf>,
    analyze: Option<PathBuf>,
    gen_test: Option<PathBuf>,
    live_probe: Option<f32>,
    selftest: Option<PathBuf>,
    shots: Vec<f32>,
    secs: f32,
    start: f32,
    seed: u64,
    ss: Option<u32>,
    diff: usize,
    mute: bool,
    size: Option<(i32, i32)>,
    live: bool,
    give: u32,
    wall: bool,
    mood: Option<(u8, u8, u8, u8, u8)>,
    mood_to: Option<(u8, u8, u8, u8, u8)>,
    purgatory: bool,
    mode: usize,
    mutators: u8,
    mortal: bool,
    /// Debug: keep the drift (idle) scene up while the song plays, with no attacks.
    drift: bool,
    /// Headless balance simulation over `files`.
    sim: bool,
    sample: usize,
    runs: u32,
    bot_delay: f32,
    song_idx: u32,
    diff_set: bool,
    screen: String,
    /// Debug: use this folder or playlist as the music library for this launch (not saved).
    library: Option<PathBuf>,
    /// Debug: two seconds into the first song, trade it as if E (1) or Q (-1) was pressed.
    swap: i32,
    live_in: String,
    live_out: String,
    /// LAN test: host a Rally match with this song.
    lan_host: Option<PathBuf>,
    /// LAN test: join the host at this IP (optionally `ip:port`).
    lan_join: Option<String>,
    /// LAN test: the local Rally player is the bot (auto character, auto ready).
    rally_bot: bool,
    /// Offline Rally practice with this song: you (or `--rally-bot`) vs the bot, no network.
    rally_local: Option<PathBuf>,
}

fn parse_args() -> Args {
    let mut a = Args { diff: 1, secs: 30.0, runs: 1, bot_delay: 140.0, ..Default::default() };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut val = || it.next().unwrap_or_default();
        match arg.as_str() {
            "--analyze" => a.analyze = Some(PathBuf::from(val())),
            "--gen-test" => a.gen_test = Some(PathBuf::from(val())),
            "--live-probe" => a.live_probe = Some(val().parse().unwrap_or(4.0)),
            "--selftest" => a.selftest = Some(PathBuf::from(val())),
            "--shots" => a.shots = val().split(',').filter_map(|s| s.trim().parse().ok()).collect(),
            "--secs" => a.secs = val().parse().unwrap_or(30.0),
            "--start" => a.start = val().parse().unwrap_or(0.0),
            "--seed" => a.seed = val().parse().unwrap_or(0),
            // Debug: start in a run mode (index into `SETUP_MODES`: 3 = DROP RUSH, 4 = DODGE).
            "--mode" => a.mode = val().parse().unwrap_or(0),
            // Debug: mutator bits (`game::MOD_*`).
            "--mutators" => a.mutators = val().parse().unwrap_or(0),
            // Debug: force the look, as "scene,weather,floor,prop" (see `level::Mood`).
            "--mood" => {
                let v: Vec<u8> = val().split(',').filter_map(|x| x.trim().parse().ok()).collect();
                if v.len() >= 4 {
                    a.mood = Some((v[0], v[1], v[2], v[3], v.get(4).copied().unwrap_or(0)));
                }
            }
            // Debug: hold the Heart in purgatory, to look at it.
            "--purgatory" => a.purgatory = true,
            // Debug: switch to this look after 3 s, to watch the sky transition.
            "--mood-to" => {
                let v: Vec<u8> = val().split(',').filter_map(|x| x.trim().parse().ok()).collect();
                if v.len() >= 4 {
                    a.mood_to = Some((v[0], v[1], v[2], v[3], v.get(4).copied().unwrap_or(0)));
                }
            }
            "--ss" => a.ss = val().parse().ok(),
            "--give" => a.give = val().parse().unwrap_or(0),
            "--sim" => a.sim = true,
            "--sample" => a.sample = val().parse().unwrap_or(0),
            "--runs" => a.runs = val().parse().unwrap_or(1),
            "--bot-delay" => a.bot_delay = val().parse().unwrap_or(140.0),
            "--song-idx" => a.song_idx = val().parse().unwrap_or(0),
            "--diff" => {
                a.diff_set = true;
                a.diff = match val().to_ascii_lowercase().as_str() {
                    "chill" | "0" => 0,
                    "hard" | "2" | "pulse" => 2,
                    "palpitation" | "3" => 3,
                    "fever" | "4" => 4,
                    "arrhythmia" | "5" => 5,
                    "flatline" | "6" => 6,
                    _ => 1,
                }
            }
            "--mute" => a.mute = true,
            "--wall" => a.wall = true,
            "--mortal" => a.mortal = true,
            "--drift" => a.drift = true,
            "--live-in" => {
                a.live_in = val();
                a.live = true;
            }
            "--live-out" => {
                a.live_out = val();
                a.live = true;
            }
            "--screen" => a.screen = val(),
            "--library" => a.library = Some(PathBuf::from(val())),
            "--swap" => a.swap = val().parse().unwrap_or(0),
            "--live" => a.live = true,
            "--lan-host" => a.lan_host = Some(PathBuf::from(val())),
            "--lan-join" => a.lan_join = Some(val()),
            "--rally-bot" => a.rally_bot = true,
            "--rally-local" => a.rally_local = Some(PathBuf::from(val())),
            "--size" => {
                let v = val();
                let mut p = v.split('x');
                if let (Some(w), Some(h)) = (p.next().and_then(|s| s.parse().ok()), p.next().and_then(|s| s.parse().ok())) {
                    a.size = Some((w, h));
                }
            }
            _ => {
                let p = PathBuf::from(&arg);
                if p.exists() {
                    a.files.extend(expand_paths(&[p]));
                }
            }
        }
    }
    a
}

/// Expand folders into the audio files they contain (sorted, recursive, capped).
fn is_playlist(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("m3u") || e.eq_ignore_ascii_case("m3u8")).unwrap_or(false)
}

/// Read an .m3u / .m3u8 playlist: one path per line, `#` lines are comments, relative paths are
/// resolved against the playlist's own folder. Only existing audio files are kept, in order.
fn read_playlist(p: &Path) -> Vec<PathBuf> {
    let Ok(bytes) = std::fs::read(p) else { return Vec::new() };
    let text = String::from_utf8_lossy(&bytes);
    let base = p.parent().unwrap_or(Path::new("."));
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with('#') || out.len() > 5000 {
            continue;
        }
        let line = line.strip_prefix("file:///").unwrap_or(line);
        let mut f = PathBuf::from(line);
        if f.is_relative() {
            f = base.join(f);
        }
        if decode::is_audio_file(&f) && f.is_file() {
            out.push(f);
        }
    }
    out
}

fn expand_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>, depth: u32) {
        if depth > 4 || out.len() > 2000 {
            return;
        }
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                walk(&p, out, depth + 1);
            } else if decode::is_audio_file(&p) {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() {
            walk(p, &mut out, 0);
        } else if is_playlist(p) {
            out.extend(read_playlist(p));
        } else if decode::is_audio_file(p) {
            out.push(p.clone());
        }
    }
    out
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn AttachConsole(process_id: u32) -> i32;
}

fn main() {
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("Heartline crashed:\n{info}\n");
        eprintln!("{msg}");
        if let Ok(exe) = std::env::current_exe() {
            let _ = std::fs::write(exe.with_file_name("heartline-crash.log"), &msg);
        }
    }));
    let args = parse_args();
    let cli = args.analyze.is_some() || args.gen_test.is_some() || args.live_probe.is_some() || args.sim;
    if cli {
        #[cfg(windows)]
        unsafe {
            AttachConsole(u32::MAX);
        }
        tools::run_cli(&args);
        return;
    }
    let (w, h) = args.size.unwrap_or((1600, 900));
    let conf = Conf {
        window_title: "Heartline".to_owned(),
        window_width: w,
        window_height: h,
        high_dpi: true,
        window_resizable: true,
        sample_count: 1,
        ..Default::default()
    };
    macroquad::Window::from_config(conf, run(args));
}

// ---------------------------------------------------------------------------------------------
// Loading
// ---------------------------------------------------------------------------------------------

pub struct Loaded {
    pub track: Arc<Track>,
    pub an: Arc<Analysis>,
    pub level: Level,
}

struct Loader {
    idx: usize,
    rx: Receiver<Result<Loaded, String>>,
    progress: Arc<AtomicU32>,
    stage: Arc<AtomicU32>,
}

pub fn load_track(path: &Path, seed: u64, diff: f32, progress: &AtomicU32, stage: &AtomicU32) -> Result<Loaded, String> {
    stage.store(0, Ordering::Relaxed);
    let track = match theme_index(path) {
        Some(i) => synth::theme(i),
        None => decode::decode_file(path, progress)?,
    };
    stage.store(1, Ordering::Relaxed);
    progress.store(0, Ordering::Relaxed);
    let mono = track.mono();
    let an = analysis::analyze(&mono, track.sr, progress);
    let level = level::generate(&an, seed, diff);
    Ok(Loaded { track: Arc::new(track), an: Arc::new(an), level })
}

fn spawn_loader(idx: usize, path: PathBuf, seed: u64, diff: f32) -> Loader {
    let (tx, rx) = channel();
    let progress = Arc::new(AtomicU32::new(0));
    let stage = Arc::new(AtomicU32::new(0));
    let (p2, s2) = (progress.clone(), stage.clone());
    std::thread::spawn(move || {
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| load_track(&path, seed, diff, &p2, &s2)))
            .unwrap_or_else(|_| Err("analysis crashed".to_string()));
        let _ = tx.send(res);
    });
    Loader { idx, rx, progress, stage }
}

// ---------------------------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Menu,
    /// Settings sub-screen of the menu (volumes, sync offset, visuals, display).
    Options,
    /// Run setup, reached from PLAY: mode, difficulty, starting track, other sources.
    Setup,
    /// Highway: the three-lane side mode (`runner.rs`).
    Runner,
    /// Codex: what everything in the game is, and the best scores.
    Codex,
    /// Track list: pick the starting track from the whole library (opened from run setup).
    Tracks,
    LiveSetup,
    /// LAN Rally: host/join, lobby, countdown, match and results (`App::lan` holds the stage).
    Lan,
    Play,
    Paused,
    Results,
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Loading,
    Song,
    Intermission(f32),
    Dying(f32),
}

/// One Highway run.
struct RunSession {
    r: runner::Runner,
    an: Arc<Analysis>,
    title: String,
    /// The cosmetic level events (beats, hits, sections, chimes) that make the vista react.
    events: Vec<LevelEvent>,
    cursor: usize,
    key: String,
    saved: bool,
    best: bool,
}

struct Song {
    track: Arc<Track>,
    an: Arc<Analysis>,
    level: Level,
    timeline: Arc<Timeline>,
    cursor: usize,
    last_t: f32,
    /// The song is cut off here (DROP RUSH); `f32::MAX` plays it to the end.
    end_t: f32,
}

struct LiveRun {
    an: Option<StreamAnalyzer>,
    gen: LiveGen,
    feats: VecDeque<(f32, Feat, f32)>,
    pending: Vec<LevelEvent>,
    buf: Vec<f32>,
    out: StreamOut,
    silent_for: f32,
    was_idle: bool,
    last_beat_t: f32,
    beat_n: u32,
    last_clock: f32,
    /// Difficulty multiplier the generator was built with.
    gen_diff: f32,
    /// Capture-timeline time of the last beat a pulse was announced for (reactive live mode).
    last_pred: f32,
    delay: f32,
    hearts: u32,
    /// Estimated seconds per beat (median of recent kick-to-kick gaps).
    period: f32,
    ibis: VecDeque<f32>,
}

/// For this long after a song starts in a run, Q / E trade it for another one.
const SWAP_SECS: f32 = 8.0;
/// Seconds the launch into a run takes (`ui::heartbeat_iris`).
const LAUNCH_SECS: f32 = 1.3;
/// Menu music plays at this fraction of the in-run music volume.
const MENU_MUSIC_GAIN: f32 = 0.5;
// Rows of the title menu: (icon, label, tooltip).
const M_PLAY: usize = 0;
const M_HIGHWAY: usize = 1;
const M_LAN: usize = 2;
const M_CODEX: usize = 3;
const M_OPTIONS: usize = 4;
const M_QUIT: usize = 5;
/// The title menu is in groups with a line between them: this many rows in each.
const MENU_GROUPS: [usize; 3] = [3, 2, 1];
const CODEX_TABS: [(Icon, &str); 7] = [
    (Icon::Heart, "THE HEART"),
    (Icon::Target, "ENEMIES"),
    (Icon::Bolt, "ATTACKS"),
    (Icon::Shard, "UPGRADES"),
    (Icon::Note, "THE MUSIC"),
    (Icon::Trophy, "RECORDS"),
    (Icon::Combo, "MILESTONES"),
];

/// What the codex's live arena shows for an entry.
#[derive(Clone, Copy, PartialEq)]
enum Demo {
    None,
    /// The Heart in the form it takes for this kind of section.
    Form(SectionKind),
    Charge,
    Finale,
    Purgatory,
    Relocation,
    Enemy(EnemyKind),
    Pattern(Pattern),
    Darts,
    Laser,
    Tiles,
    Wave,
    Upgrade(UpgradeId),
    /// Entry `n` of `CODEX_MUSIC`.
    Music(u8),
}

/// One entry of a codex page.
struct CodexItem {
    name: String,
    tag: String,
    body: String,
    demo: Demo,
}

/// The Heart's attacks: (name, what to do about it, what the arena shows).
const CODEX_ATTACKS: [(&str, &str, Demo); 15] = [
    ("RING", "A full ring leaving the Heart. Find the gap between two bullets and let it pass.", Demo::Pattern(Pattern::Ring)),
    ("FAN", "A cone aimed at where you stand when it fires. Step to the side.", Demo::Pattern(Pattern::Fan)),
    ("SPIRAL", "Two curved arms. Move with the turn, not against it.", Demo::Pattern(Pattern::Spiral)),
    ("CROSS", "Four lines of bullets, one of them aimed at you.", Demo::Pattern(Pattern::Cross)),
    ("FLOWER", "A ring whose bullets fly at different speeds, so it opens like petals. Slip between two.", Demo::Pattern(Pattern::Flower)),
    ("ARC", "Three quarters of a ring. The missing quarter is the way out.", Demo::Pattern(Pattern::Arc)),
    ("WALL", "A dense arc with one gap in it. Go through the gap.", Demo::Pattern(Pattern::Wall)),
    ("BURST", "A fast line straight at you. One step is enough.", Demo::Pattern(Pattern::Burst)),
    ("PINCER", "Two cones, one either side of you. The lane you stand in stays open: hold still.", Demo::Pattern(Pattern::Pincer)),
    ("COMB", "Five short lines fanned around you. Step into a gap between two.", Demo::Pattern(Pattern::Comb)),
    ("SWEEP", "A cone that arrives one side first, like a scythe. Go with it, or dash through.", Demo::Pattern(Pattern::Sweep)),
    ("DARTS", "Single fast darts on the hi-hats in the hottest bars.", Demo::Darts),
    ("LASER WALL", "A beam that sweeps the floor on a sustained note. Its path is shown first.", Demo::Laser),
    ("TILE STRIKE", "Floor tiles light a bar ahead, then slam on the beat. Stand on a dark one.", Demo::Tiles),
    ("SHOCKWAVE", "A wall of light leaving the Heart with gaps in it. Line up with a gap.", Demo::Wave),
];

/// The entries of codex page `tab` (empty for the pages that are tables).
fn codex_items(tab: usize) -> Vec<CodexItem> {
    let mk = |name: &str, tag: &str, body: &str, demo: Demo| CodexItem { name: name.to_string(), tag: tag.to_string(), body: body.to_string(), demo };
    match tab {
        0 => {
            let demos = [
                Demo::Charge,
                Demo::Form(SectionKind::Calm),
                Demo::Form(SectionKind::Normal),
                Demo::Form(SectionKind::Intense),
                Demo::Form(SectionKind::Drop),
                Demo::Finale,
                Demo::Purgatory,
                Demo::Form(SectionKind::Drop),
                Demo::Relocation,
                Demo::Form(SectionKind::Normal),
            ];
            CODEX_HEART.iter().zip(demos).map(|(e, d)| mk(e.0, "the Heart", e.1, d)).collect()
        }
        1 => {
            let kinds = [
                EnemyKind::Chaser,
                EnemyKind::Darter,
                EnemyKind::Orbiter,
                EnemyKind::Splitter,
                EnemyKind::Turret,
                EnemyKind::Bomber,
                EnemyKind::Wisp,
                EnemyKind::Elite,
                EnemyKind::Sprite,
            ];
            CODEX_ENEMIES.iter().zip(kinds).map(|(e, k)| mk(e.0, "enemy", e.1, Demo::Enemy(k))).collect()
        }
        2 => CODEX_ATTACKS.iter().map(|e| mk(e.0, "attack", e.1, e.2)).collect(),
        3 => UpgradeId::ALL
            .iter()
            .map(|up| {
                let lv = if up.max() > 1 { format!("levels 1-{}", up.max()) } else { "one level".to_string() };
                mk(up.name(), &format!("{}upgrade, {lv}", if up.rare() { "rare " } else { "" }), up.desc(), Demo::Upgrade(*up))
            })
            .collect(),
        4 => CODEX_MUSIC.iter().enumerate().map(|(i, e)| mk(e.0, "the music", e.1, Demo::Music(i as u8))).collect(),
        _ => Vec::new(),
    }
}
/// Milestones: (id kept in the scores file, name, how to earn it).
const MILESTONES: [(&str, &str, &str); 12] = [
    ("heart", "HEARTBREAKER", "Break a Heart."),
    ("depth3", "GOING DEEPER", "Reach depth 5 in one run."),
    ("depth5", "THE ABYSS", "Reach depth 10 in one run."),
    ("clean", "UNTOUCHED", "Clear a track without taking a hit."),
    ("graze", "PAPER THIN", "Graze 150 bullets in one run."),
    ("breaker", "IMPATIENT", "Break five Hearts in one run."),
    ("resonant", "IN TUNE", "Set resonance ringing three times in one run."),
    ("glass", "GLASS CANNON", "Clear a track with the GLASS mutator on."),
    ("flatline", "FLATLINER", "Clear a track on FLATLINE."),
    ("dodge", "PACIFIST", "Finish a track in DODGE."),
    ("road", "ROAD WARRIOR", "Finish a Highway road with rank A or S."),
    ("jumps", "PERFECT TIMING", "Land 20 perfect jumps in one Highway road."),
];
const CODEX_HEART: [(&str, &str); 10] = [
    ("CHARGE", "While the body of the Heart can be hurt, a charge appears far from it. Carry it into the ring around the Heart to take a tenth of its health at once. A hit knocks it loose."),
    ("SHELL", "Calm sections. The Heart's body is immune: break the three shield nodes circling the arena. With all three down before the section ends, it stands exposed."),
    ("VALVE", "Groove sections. It opens for the first half of every bar; while shut, shots do a fifth."),
    ("SPLIT", "Surge sections. Three linked cores orbit it and only the lit one can be hurt. Break them in order; with all three down it stands exposed until the section changes."),
    ("OVERDRIVE", "Drops. Exposed, taking half again as much damage, and it charges down a lit lane: step out of it."),
    ("FINALE", "The last section of a song. Every pulse has an echo half a beat later, made of shootable notes."),
    ("PURGATORY", "When the Heart breaks, everything hostile becomes shootable for ten seconds or more and your rewards are on the field. Then a new Heart rises at the depth you dived to."),
    ("THE DIVE", "Depth is the goal. Every Heart has a clock: the song. Break it with half the clock left and you go three depths down with five rewards; with a quarter left, two depths and three rewards; later, one and one. Never break it and you sink one depth with nothing."),
    ("RELOCATION", "From PULSE up it moves on bar lines. A ring shows where; it holds its fire on the way."),
    ("BEAT RING", "The ticked ring around the Heart. Every pulse is timed to cross it on a beat."),
];
const CODEX_ENEMIES: [(&str, &str); 9] = [
    ("CHASER", "Walks at you. Arrives on the snare."),
    ("DARTER", "Flies straight through where you were when it appeared, then leaves."),
    ("ORBITER", "Circles the arena and shoots at you every few beats."),
    ("SPLITTER", "Slow and tough; breaks into three fast minis."),
    ("METRONOME", "Plants itself and fires a three-shot fan on every downbeat. Its ring fills through the bar."),
    ("CYMBAL", "Drifts in, arms when close, and bursts into a ring two beats later. Kill it first."),
    ("WISP", "Appears on the song's chimes, where the note sits in pitch. Light, weaving, fades if ignored."),
    ("ELITE", "Arrives with a drop, with a lot of health. Gunners (red) fire rings; from depth 2 Summoners (pink) call minis; from depth 3 Leapers (gold) blink at you on the downbeat."),
    ("SPRITE", "Ear candy made visible. Harmless and golden: catch it for a heal."),
];
const CODEX_MUSIC: [(&str, &str); 10] = [
    ("BEAT", "The kick drives the Heart's pulses."),
    ("BASS", "A sustained low end brings tile strikes: pillars that rise for a bar, then slam on the beat."),
    ("SNARE", "Snares open the portals enemies arrive through."),
    ("HATS", "Hi-hats leave shards to collect, and dart streams in the hottest bars."),
    ("LEAD", "Sustained notes sweep laser walls across the floor."),
    ("CHIMES", "Pitched hits high in the mix send wisps."),
    ("EAR CANDY", "A flourish the song rarely plays: a comet trailing shards, a keypad of shards, or a sprite."),
    ("GRAZE", "Slip past a bullet at close range without being hit: score, combo, and it feeds resonance. Dash straight through one for a THREAD, worth three."),
    ("RESONANCE", "The meter under the combo bar. Full, it rings for eight seconds: more damage, double score."),
    ("GRADES", "Calm sections grade you from C to S on the hits you took and your combo since the last one. Better grades offer more, and rarer, upgrades."),
];
const MENU_ROWS: [(Icon, &str, &str); 6] = [
    (Icon::Play, "PLAY", "Set up a run on the track that is playing: the mode, the difficulty, mutators."),
    (Icon::Road, "HIGHWAY", "A side mode on this track: race down the grid. Switch lanes around the walls (kicks), jump the bars (snares), collect the gems (hats). Uses the difficulty chosen in PLAY."),
    (Icon::Lan, "LAN RALLY", "A beat-locked paddle duel against a second PC on your network. The host picks the song."),
    (Icon::Book, "CODEX", "What everything is: the Heart's forms, the enemies, every upgrade, how the music becomes the level, and your best scores."),
    (Icon::Options, "OPTIONS", "Volumes, sync offset, how hard the scene pulses with the music, windowed or fullscreen."),
    (Icon::Quit, "QUIT", "Close Heartline."),
];
// Rows of the run setup screen.
const S_MODE: usize = 0;
const S_REMIX: usize = 1;
const S_TRACK: usize = 2;
const S_DIFF: usize = 3;
const S_MUT: usize = 4;
const S_START: usize = 5;
const SETUP_MODES: [(Icon, &str, &str); 5] = [
    (Icon::Endless, "ENDLESS", "This track, then the library keeps coming. Break each Heart to dive: depth, HP and upgrades carry over until you fall."),
    (Icon::Single, "SINGLE", "Just this song. You can still carry on into more songs from the results screen."),
    (Icon::Calendar, "DAILY", "An endless run on today's seed. The same songs give everyone the same levels and upgrade offers today."),
    (Icon::Bolt, "DROP RUSH", "Every track is cut down to its biggest section. Short, dense, endless."),
    (Icon::Visuals, "DODGE", "No gun, and the Heart cannot be hurt. Score by slipping close past bullets and by staying alive."),
];
/// The buttons under the track preview in the top right corner: (icon, name, what it does).
const CORNER: [(Icon, &str, &str); 8] = [
    (Icon::Prev, "PREVIOUS TRACK  (Q)", "Step back through your library."),
    (Icon::Next, "NEXT TRACK  (E)", "Step forward through your library."),
    (Icon::List, "TRACK LIST  (T)", "Pick from the whole library. Type a letter to jump to it."),
    (Icon::Files, "PLAY FILES", "Pick audio files or an .m3u playlist and play them in order. Dropping files on the window does the same."),
    (Icon::Folder, "PLAY A FOLDER", "Pick a folder and play everything in it, without changing your library."),
    (Icon::Live, "LIVE AUDIO", "Play against whatever your PC is playing right now. The level is built on the fly."),
    (Icon::Folder, "LIBRARY FROM A FOLDER", "Choose the folder that runs and this menu draw from."),
    (Icon::Playlist, "LIBRARY FROM A PLAYLIST", "Choose an .m3u playlist that runs and this menu draw from."),
];
/// How far below the top of the track preview the run-setup panel starts.
const CORNER_H: f32 = 232.0;
const MODE_DAILY: usize = 2;
const MODE_RUSH: usize = 3;
const MODE_DODGE: usize = 4;
/// Score multiplier of each difficulty tier.
const TIER_SCORE: [f32; 7] = [0.5, 0.8, 1.0, 1.25, 1.5, 1.8, 2.2];
// Rows of the options screen: (icon, label, tooltip).
const O_MUSIC: usize = 0;
const O_SFX: usize = 1;
const O_SYNC: usize = 2;
const O_VISUALS: usize = 3;
#[allow(dead_code)]
const O_DISPLAY: usize = 4;
const O_BACK: usize = 5;
const OPTION_ROWS: [(Icon, &str, &str); 5] = [
    (Icon::Music, "MUSIC VOLUME", "How loud the song is in a run. The menu plays it at half of this."),
    (Icon::Sfx, "EFFECTS VOLUME", "How loud hits, pickups and menu sounds are."),
    (Icon::Sync, "SYNC OFFSET", "Raise it if attacks land before you hear the beat (soundbar or Bluetooth lag); lower it if they land late."),
    (Icon::Visuals, "VISUALS", "How hard the scene pulses with the music: zoom punches, flashes, rings. SUBTLE or OFF for comfort."),
    (Icon::Display, "DISPLAY", "Windowed or borderless fullscreen. F11 switches anywhere."),
];
/// How long the menu stays on one track before cross-fading to the next.
const MENU_TRACK_SECS: f32 = 75.0;

/// Difficulty tiers: (budget multiplier, name, what changes). Steps are small on purpose.
/// Tier 0 is no-fail; the last tier starts with a single life (see `App::apply_diff_rules`).
const DIFFS: [(f32, &str, &str); 7] = [
    (0.7, "LULLABY", "No-fail. Sparse attacks, slow bullets, long warnings."),
    (1.0, "RESTING", "A gentle run: room to learn every pattern."),
    (1.35, "PULSE", "The intended run. Denser patterns, lasers and tile strikes may overlap."),
    (1.75, "PALPITATION", "Attacks come faster and enemies arrive in numbers."),
    (2.2, "FEVER", "Big patterns on every beat. Little room to rest."),
    (2.7, "ARRHYTHMIA", "Near-constant pressure and the widest patterns."),
    (3.2, "FLATLINE", "One life. Everything at full force. SUSTAIN upgrades are the only way to earn more."),
];
/// Menu VISUALS setting: scale of the renderer's music-reactive pulse layer.
const VISUALS: [(f32, &str); 3] = [(1.0, "FULL"), (0.45, "SUBTLE"), (0.0, "OFF")];

struct App {
    args: Args,
    audio: Audio,
    rend: Renderer,
    world: World,
    screen: Screen,
    phase: Phase,
    queue: Vec<PathBuf>,
    /// The run keeps drawing tracks from the library after its queue is used up.
    endless: bool,
    qi: usize,
    loader: Option<Loader>,
    ready: Option<(usize, Loaded)>,
    song: Option<Song>,
    live: Option<LiveRun>,
    idle: f32,
    diff: usize,
    volume: f32,
    sfx_volume: f32,
    /// Smoothed music gain (menu duck, death fade).
    music_gain: f32,
    /// Extra audio latency in ms (soundbars, Bluetooth): events are delayed by this much.
    offset_ms: f32,
    music_dir: Option<PathBuf>,
    juke: Jukebox,
    /// Where endless runs are up to in the library list.
    juke_seq: usize,
    autofire: bool,
    fullscreen: bool,
    /// Index into `VISUALS`.
    visuals: usize,
    menu_sel: usize,
    /// Pause menu: QUIT was pressed once and needs a second press.
    quit_armed: bool,
    /// Run setup: ENDLESS RUN (true) or SINGLE TRACK, and the focused button of the last row.
    /// Index into `SETUP_MODES`.
    setup_mode: usize,
    /// Track list: the highlighted row.
    track_sel: usize,
    /// The screen the track list goes back to.
    tracks_back: Screen,
    /// The song was in a silent stretch last frame (attacks are cleared once when one begins).
    was_silent: bool,
    /// Seconds the current song has been playing. For the first `SWAP_SECS` of them Q / E
    /// trade it for another.
    song_age: f32,
    /// The next load happens with the arena on screen (a song was traded mid-run).
    load_in_arena: bool,
    /// Files dropped on the title menu: run setup opens and START plays them.
    staged: Vec<PathBuf>,
    /// Seconds since a run was launched; the heartbeat opening plays while it is under
    /// `LAUNCH_SECS`.
    launch: f32,
    /// Codex: the chosen entry, the first row in view, and the live arena beside the lists
    /// (what it should show, what it is showing, its own clock, when the next thing is due
    /// and how many it has sent).
    codex_sel: usize,
    codex_top: usize,
    show_want: Demo,
    show_demo: Demo,
    show_t: f32,
    show_next: f32,
    show_n: usize,
    /// Where the difficulty strip is scrolled to (eases toward `diff`).
    diff_scroll: f32,
    /// Remix number chosen in run setup: added to the seed, so the same song deals a different
    /// level (0 = the song's own).
    remix: u64,
    /// Mutators chosen in run setup (`game::MOD_*` bits), and the focused one.
    mutators: u8,
    mut_col: usize,
    /// Best scores, keyed by `score_key`: (score, depth reached).
    scores: std::collections::HashMap<String, (u64, u32)>,
    /// The key this run will be recorded under, and whether it has been.
    run_key: String,
    run_saved: bool,
    run_best: bool,
    /// The seed from the command line (DAILY SEED replaces `args.seed` for one run).
    base_seed: u64,
    setup_col: usize,
    dialog: Option<Receiver<Vec<PathBuf>>>,
    /// True when the open folder dialog is choosing the music library (not starting a run).
    dialog_library: bool,
    /// Escape / P was consumed this frame (so pausing does not instantly un-pause).
    esc_used: bool,
    message: String,
    message_t: f32,
    died: bool,
    // live setup
    cap_sel: usize,
    out_sel: usize,
    delay: f32,
    // selftest
    shots_done: usize,
    frames: u64,
    wall: f32,
    max_bullets: usize,
    max_enemies: usize,
    log: Vec<String>,
    /// Highway session (Some while `screen == Screen::Runner`).
    runner: Option<Box<RunSession>>,
    /// LAN Rally session (Some while `screen == Screen::Lan`).
    lan: Option<Box<Lan>>,
}

/// Built-in synthesised tracks travel through playlists as pseudo paths ("::theme0").
fn theme_path(i: usize) -> PathBuf {
    PathBuf::from(format!("::theme{i}"))
}

fn theme_index(p: &Path) -> Option<usize> {
    p.to_str()?.strip_prefix("::theme")?.parse().ok()
}

/// Where the menu starts previewing a track: a bar before its first big section.
fn preview_start(an: &Analysis) -> f32 {
    let beat = if an.bpm > 1.0 { 60.0 / an.bpm } else { 0.5 };
    an.sections
        .iter()
        .find(|s| matches!(s.kind, SectionKind::Intense | SectionKind::Drop) && s.t0 > 1.0 && s.t0 < an.duration * 0.7)
        .map(|s| (s.t0 - 4.0 * beat).max(0.0))
        .unwrap_or(0.0)
}

/// The menu's music: plays through the library (or the built-in tracks), fully analysed so
/// the menu scene reacts to it and so pressing PLAY starts instantly.
struct Jukebox {
    list: Vec<PathBuf>,
    idx: usize,
    loaded: Option<(PathBuf, Loaded)>,
    loader: Option<(PathBuf, Loader)>,
    cursor: usize,
    last_t: f32,
    playing: bool,
    fails: u32,
    /// Seconds the current menu track has been playing (the menu moves on by itself).
    played: f32,
}

// ---------------------------------------------------------------------------------------------
// Settings (heartline.cfg next to the exe)
// ---------------------------------------------------------------------------------------------

fn scores_path() -> Option<PathBuf> {
    std::env::current_exe().ok().map(|e| e.with_file_name("heartline-scores.txt"))
}

/// Best scores: one `key<TAB>score<TAB>depth` line each.
fn load_scores() -> std::collections::HashMap<String, (u64, u32)> {
    let mut m = std::collections::HashMap::new();
    if let Some(text) = scores_path().and_then(|p| std::fs::read_to_string(p).ok()) {
        for line in text.lines() {
            let mut it = line.split('\t');
            if let (Some(k), Some(s), Some(d)) = (it.next(), it.next(), it.next()) {
                if let (Ok(s), Ok(d)) = (s.parse(), d.parse()) {
                    m.insert(k.to_string(), (s, d));
                }
            }
        }
    }
    m
}

fn settings_path() -> Option<PathBuf> {
    std::env::current_exe().ok().map(|e| e.with_file_name("heartline.cfg"))
}

fn load_settings() -> std::collections::HashMap<String, String> {
    let mut m = std::collections::HashMap::new();
    if let Some(text) = settings_path().and_then(|p| std::fs::read_to_string(p).ok()) {
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                m.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    m
}

/// The user's Music folder (honours a relocated library).
#[cfg(windows)]
fn music_folder() -> Option<PathBuf> {
    use std::ffi::c_void;
    #[repr(C)]
    struct Guid(u32, u16, u16, [u8; 8]);
    #[link(name = "shell32")]
    extern "system" {
        fn SHGetKnownFolderPath(rfid: *const Guid, flags: u32, token: *mut c_void, path: *mut *mut u16) -> i32;
    }
    #[link(name = "ole32")]
    extern "system" {
        fn CoTaskMemFree(p: *mut c_void);
    }
    // FOLDERID_Music {4BD8D571-6D19-48D3-BE97-422220080E43}
    let id = Guid(0x4BD8_D571, 0x6D19, 0x48D3, [0xBE, 0x97, 0x42, 0x22, 0x20, 0x08, 0x0E, 0x43]);
    let mut raw: *mut u16 = std::ptr::null_mut();
    let hr = unsafe { SHGetKnownFolderPath(&id, 0, std::ptr::null_mut(), &mut raw) };
    if hr != 0 || raw.is_null() {
        return None;
    }
    let mut len = 0usize;
    unsafe {
        while *raw.add(len) != 0 && len < 4096 {
            len += 1;
        }
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(raw, len));
        CoTaskMemFree(raw as *mut c_void);
        Some(PathBuf::from(s))
    }
}

#[cfg(not(windows))]
fn music_folder() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Music"))
}

/// A name for the music library that the font can draw. A playlist loses its extension; a name
/// made only of symbols the font has no shapes for (emoji, say) is described by where it is.
fn library_label(p: &Path) -> String {
    let raw = if is_playlist(p) { p.file_stem() } else { p.file_name() };
    let name: String = raw.and_then(|s| s.to_str()).unwrap_or("").chars().filter(|c| (*c as u32) < 0x2000).collect();
    let name = name.trim();
    if !name.is_empty() {
        return name.to_string();
    }
    let parent = p.parent().and_then(|d| d.file_name()).and_then(|s| s.to_str()).unwrap_or("");
    match (is_playlist(p), parent.is_empty()) {
        (true, false) => format!("playlist in {parent}"),
        (true, true) => "playlist".to_string(),
        (false, false) => format!("folder in {parent}"),
        (false, true) => "music folder".to_string(),
    }
}

fn track_title(p: &Path) -> String {
    if let Some(i) = theme_index(p) {
        return synth::theme_name(i).to_string();
    }
    p.file_stem().and_then(|s| s.to_str()).unwrap_or("untitled").to_string()
}

fn feat_at(an: &Analysis, t: f32) -> Feat {
    if an.feats.is_empty() {
        return Feat::default();
    }
    let x = (t / an.hop_dt).max(0.0);
    let i = (x as usize).min(an.feats.len() - 1);
    let j = (i + 1).min(an.feats.len() - 1);
    Feat::lerp(&an.feats[i], &an.feats[j], x.fract())
}

/// Seconds from `t` until the next section that steps UP into Intense / Drop (infinite if none).
/// The renderer builds tension over the bars before it and releases it on the downbeat.
fn next_build(an: &Analysis, t: f32) -> f32 {
    let big = |k: SectionKind| matches!(k, SectionKind::Intense | SectionKind::Drop);
    let mut prev_big = false;
    for s in &an.sections {
        if s.t0 > t + 0.02 && big(s.kind) && !prev_big {
            return s.t0 - t;
        }
        prev_big = big(s.kind);
    }
    f32::INFINITY
}

fn beat_pos(beats: &[f32], t: f32) -> f32 {
    let n = beats.len();
    if n < 2 {
        return t * 2.0;
    }
    let i = beats.partition_point(|&b| b <= t);
    if i == 0 {
        return (t - beats[0]) / (beats[1] - beats[0]).max(0.05);
    }
    if i >= n {
        return (n - 1) as f32 + (t - beats[n - 1]) / (beats[n - 1] - beats[n - 2]).max(0.05);
    }
    (i - 1) as f32 + (t - beats[i - 1]) / (beats[i] - beats[i - 1]).max(0.05)
}

impl App {
    /// Tier-specific rules beyond the budget multiplier.
    fn apply_diff_rules(&mut self) {
        if self.diff == DIFFS.len() - 1 {
            self.world.player.hp = 1;
            self.world.player.max_hp = 1;
        }
    }

    fn diff(&self) -> f32 {
        DIFFS[self.diff].0
    }

    fn say(&mut self, s: impl Into<String>) {
        self.message = s.into();
        self.message_t = 5.0;
    }

    /// Hand the renderer this frame's audible waveform window and the time to the next big
    /// section (file / menu playback only; live mode gets neither).
    fn feed_signal(&mut self) {
        self.rend.fx = VISUALS[self.visuals].0;
        let t = self.world.t;
        let src = if self.live.is_some() {
            None
        } else if matches!(self.screen, Screen::Play | Screen::Paused) {
            match (self.phase, self.song.as_ref()) {
                (Phase::Song, Some(s)) => Some((&s.track, &s.an)),
                _ => None,
            }
        } else if self.juke.playing {
            self.juke.loaded.as_ref().map(|(_, l)| (&l.track, &l.an))
        } else {
            None
        };
        match src {
            Some((tr, an)) => self.rend.feed_signal(Some((&tr.samples, tr.sr)), t, next_build(an, t)),
            None => self.rend.feed_signal(None, t, f32::INFINITY),
        }
    }

    fn save_settings(&self) {
        if self.args.selftest.is_some() {
            return;
        }
        let Some(path) = settings_path() else { return };
        let mut text = format!(
            "diff={}\nvolume={:.2}\nsfx={:.2}\noffset_ms={:.0}\nautofire={}\nfullscreen={}\n",
            self.diff, self.volume, self.sfx_volume, self.offset_ms, self.autofire as u8, self.fullscreen as u8
        );
        text.push_str(&format!("visuals={}\n", self.visuals));
        text.push_str(&format!("mutators={}\n", self.mutators));
        if let Some(d) = &self.music_dir {
            text.push_str(&format!("music_dir={}\n", d.display()));
        }
        let _ = std::fs::write(path, text);
    }

    /// Rebuild the menu playlist: the library, shuffled. The built-in tracks only stand in
    /// while the library is empty; they are never mixed in with the player's own music.
    fn juke_rescan(&mut self) {
        let mut lib = self.music_dir.as_ref().map(|d| expand_paths(&[d.clone()])).unwrap_or_default();
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
        let mut rng = rng::Rng::new(if self.args.selftest.is_some() { 7 } else { seed });
        for i in (1..lib.len()).rev() {
            lib.swap(i, rng.below(i + 1));
        }
        if lib.is_empty() {
            let first_theme = rng.below(synth::THEME_COUNT);
            lib.extend((0..synth::THEME_COUNT).map(|k| theme_path((first_theme + k) % synth::THEME_COUNT)));
        }
        self.juke = Jukebox { list: lib, idx: 0, loaded: None, loader: None, cursor: 0, last_t: -1.0, playing: false, fails: 0, played: 0.0 };
    }

    /// What the menu jukebox is playing: (title, hint line).
    fn juke_title(&self) -> (String, String) {
        let (title, hint) = match (&self.juke.loaded, self.juke.list.get(self.juke.idx)) {
            (_, None) => ("no music found".to_string(), "set a music library below, or drop files on the window".to_string()),
            (Some((p, l)), Some(want)) if p == want => (l.track.name.clone(), "Q / E  change track".to_string()),
            (_, Some(want)) => (track_title(want), "reading the music...".to_string()),
        };
        (title, hint)
    }

    /// Ask for audio files or a playlist to play (the answer arrives through `dialog`).
    fn open_files_dialog(&mut self) {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let mut exts: Vec<&str> = decode::AUDIO_EXTS.to_vec();
            exts.extend(["m3u", "m3u8"]);
            let r = rfd::FileDialog::new().set_title("Heartline - choose songs or a playlist").add_filter("Audio and playlists", &exts).pick_files();
            let _ = tx.send(r.unwrap_or_default());
        });
        self.dialog = Some(rx);
        self.dialog_library = false;
    }

    /// Ask for a folder to play, or (with `library`) for the music library: a folder, or an
    /// .m3u playlist when `playlist` is set.
    fn open_folder_dialog(&mut self, library: bool, playlist: bool) {
        let (tx, rx) = channel();
        let start_dir = self.music_dir.clone().map(|d| if d.is_dir() { d } else { d.parent().map(Path::to_path_buf).unwrap_or(d) });
        std::thread::spawn(move || {
            let mut d = rfd::FileDialog::new().set_title(if library { "Heartline - choose your music library folder" } else { "Heartline - choose a music folder to play" });
            if let Some(sd) = start_dir {
                d = d.set_directory(sd);
            }
            let picked = if playlist { d.add_filter("Playlist", &["m3u", "m3u8"]).pick_file() } else { d.pick_folder() };
            let _ = tx.send(picked.map(|p| vec![p]).unwrap_or_default());
        });
        self.dialog = Some(rx);
        self.dialog_library = library;
    }

    fn juke_next(&mut self, dir: i32) {
        let n = self.juke.list.len() as i32;
        if n > 1 {
            self.juke.idx = (self.juke.idx as i32 + dir).rem_euclid(n) as usize;
            self.juke.loader = None;
            self.juke.fails = 0;
            self.audio.sfx(Sfx::UiMove, 0.8, 0.0);
        }
    }

    /// Menu-like screens: keep the menu music loaded and playing, and drive the world's music
    /// signals from it so the scene is a visualiser. Returns progress through the track.
    fn juke_step(&mut self, dt: f32, input: &Input) -> f32 {
        let mut done = None;
        if let Some((path, l)) = &self.juke.loader {
            if let Ok(res) = l.rx.try_recv() {
                done = Some((path.clone(), res));
            }
        }
        if let Some((path, res)) = done {
            self.juke.loader = None;
            match res {
                Ok(loaded) => {
                    if self.juke.list.get(self.juke.idx) == Some(&path) {
                        self.world.mood = loaded.level.mood;
                        self.juke.loaded = Some((path, loaded));
                        self.juke.playing = false;
                        self.juke.fails = 0;
                    }
                }
                Err(_) => {
                    self.juke.fails += 1;
                    self.juke.list.retain(|p| *p != path);
                    if self.juke.idx >= self.juke.list.len() {
                        self.juke.idx = 0;
                    }
                }
            }
        }
        let n = self.juke.list.len();
        if n > 0 && self.juke.loader.is_none() && self.juke.fails < 6 {
            let want = self.juke.list[self.juke.idx % n].clone();
            if self.juke.loaded.as_ref().map(|l| &l.0) != Some(&want) {
                let l = spawn_loader(0, want.clone(), self.args.seed, self.diff());
                self.juke.loader = Some((want, l));
            }
        }
        if !self.juke.playing {
            if let Some((path, loaded)) = &self.juke.loaded {
                let start = if theme_index(path).is_some() { 0.0 } else { preview_start(&loaded.an) };
                self.audio.play_ex(loaded.track.clone(), start as f64, true, 0.9);
                self.juke.playing = true;
                self.juke.played = 0.0;
                self.juke.last_t = -1.0;
                self.world.warp = 1.0;
                self.world.beat_len = if loaded.level.bpm > 1.0 { 60.0 / loaded.level.bpm } else { 0.5 };
            }
        }
        // The menu is a radio, not a loop: move to the next track after a while.
        if self.juke.playing {
            self.juke.played += dt;
            if self.juke.played > MENU_TRACK_SECS && self.juke.list.len() > 1 && self.juke.loader.is_none() && self.dialog.is_none() {
                self.juke.idx = (self.juke.idx + 1) % self.juke.list.len();
                self.juke.played = 0.0;
            }
        }
        let mut feat = Feat::default();
        let mut bp = self.rend.time * 100.0 / 60.0;
        let mut intensity = 0.0;
        let mut progress = 0.0;
        if self.juke.playing {
            if let Some((_, loaded)) = &self.juke.loaded {
                let t = self.audio.song_time() as f32;
                let ev = &loaded.level.events;
                if self.juke.last_t < 0.0 || t < self.juke.last_t - 0.3 {
                    // Started or looped: find our place and restore the section look.
                    self.juke.cursor = ev.partition_point(|e| e.at() <= t);
                    if let Some(e) = ev[..self.juke.cursor].iter().rev().find(|e| matches!(e.ev, Ev::Section { .. })) {
                        self.world.apply(e);
                    }
                }
                self.juke.last_t = t;
                let show = self.show_demo != Demo::None;
                if !show {
                    self.world.t = t;
                }
                while self.juke.cursor < ev.len() && ev[self.juke.cursor].at() <= t {
                    let e = ev[self.juke.cursor];
                    self.juke.cursor += 1;
                    if show {
                        // only the beat and the drum hits reach the showcase, on its own clock
                        match e.ev {
                            Ev::Beat { .. } | Ev::Hit { .. } => self.world.apply(&LevelEvent { t: self.world.t, lead: 0.0, ev: e.ev }),
                            Ev::Section { palette, .. } => self.world.palette = palette,
                            _ => {}
                        }
                        continue;
                    }
                    if let Ev::Candy { .. } = e.ev {
                        self.world.candy = 1.0;
                    }
                    if matches!(e.ev, Ev::Beat { .. } | Ev::Hit { .. } | Ev::Section { .. } | Ev::Chime { .. }) {
                        self.world.apply(&e);
                    }
                }
                feat = feat_at(&loaded.an, t);
                bp = beat_pos(&loaded.level.beats, t);
                intensity = loaded.an.intensity_at(t);
                progress = t / loaded.level.duration.max(0.1);
            }
        }
        if self.show_demo != Demo::None {
            self.show_t += dt;
            let t = self.show_t;
            self.world.update(dt, t, input, bp, &feat, intensity, true);
        } else {
            let t = self.world.t;
            self.world.update(dt, t, input, bp, &feat, intensity, false);
        }
        progress
    }

    /// Codex: build the little world that shows `demo`.
    fn codex_demo_reset(&mut self, demo: Demo) {
        let (mood, palette) = (self.world.mood, self.world.palette);
        let bl = self.juke.loaded.as_ref().map(|(_, l)| if l.level.bpm > 1.0 { 60.0 / l.level.bpm } else { 0.5 }).unwrap_or(0.5);
        let mut w = World::new(DIFFS[2].0.max(self.diff()), true);
        w.demo = true;
        w.begin_song(900.0, 0, 7);
        w.mood = mood;
        w.palette = palette;
        w.beat_len = bl;
        let kind = match demo {
            Demo::Form(k) => k,
            Demo::Charge | Demo::Finale | Demo::Purgatory | Demo::Upgrade(_) => SectionKind::Drop,
            Demo::Relocation => SectionKind::Intense,
            _ => SectionKind::Normal,
        };
        w.demo_setup(kind, demo == Demo::Finale, demo == Demo::Charge);
        // a steady gun on the beat grid, so the player in the showcase can shoot back
        let step = bl.clamp(0.3, 1.0);
        w.gun = [1.0f32, 2.0, 4.0, 4.0, 8.0].iter().map(|d| (0..8000).map(|i| i as f32 * step / d).collect()).collect();
        if let Demo::Upgrade(up) = demo {
            w.player.ups[up as usize] = up.max();
        }
        // the showcase is one long song, so its Heart's clock never runs out
        w.set_heart_window(0.0, 900.0, 900.0);
        self.world = w;
        self.show_demo = demo;
        self.show_t = 0.0;
        self.show_next = 0.5;
        self.show_n = 0;
    }

    /// Codex: keep the showcase running. It is rebuilt when the entry changes or it has run
    /// long, and every few beats it is sent the next thing to show, timed to land on a beat.
    fn codex_demo_step(&mut self) {
        let demo = self.show_want;
        if demo == Demo::None {
            return;
        }
        let over = self.world.heart.broken || (self.world.purgatory && demo != Demo::Purgatory);
        if self.show_demo != demo || self.show_t > 50.0 || over {
            self.codex_demo_reset(demo);
        }
        if demo == Demo::Purgatory {
            self.world.purgatory = true;
            self.world.purg_until = self.world.t + 4.5;
        }
        if self.show_t < self.show_next {
            return;
        }
        let w = &mut self.world;
        let bl = w.beat_len.clamp(0.3, 1.0);
        let n = self.show_n;
        self.show_n += 1;
        // land on the beat after next, announced from now
        let t_on = self.show_t + (1.0 - w.beat_phase) * bl + bl;
        let lead = t_on - self.show_t;
        let ev = |ev: Ev, extra: f32| LevelEvent { t: t_on + extra, lead: lead + extra, ev };
        let speed = level::beat_speed(175.0, bl);
        let pulse = |p: Pattern, count: u16, n: usize| Ev::Pulse { pattern: p, count, speed, rot: n as f32 * 0.45, soft: false };
        let spots = [(300.0f32, -160.0f32), (320.0, 150.0), (-280.0, 170.0), (-300.0, -150.0)];
        let (sx, sy) = spots[n % spots.len()];
        let mut gap = 4.0;
        match demo {
            Demo::None => {}
            Demo::Form(_) | Demo::Charge | Demo::Finale | Demo::Purgatory | Demo::Relocation => {
                let p = [Pattern::Ring, Pattern::Fan, Pattern::Arc, Pattern::Flower][n % 4];
                w.apply(&ev(pulse(p, 14, n), 0.0));
                gap = 2.0;
            }
            Demo::Enemy(kind) => {
                if w.enemies.len() + w.portals.len() < 2 {
                    w.apply(&ev(Ev::Spawn { kind, x: sx, y: sy }, 0.0));
                }
                gap = 3.0;
            }
            Demo::Pattern(p) => {
                let count = match p {
                    Pattern::Burst => 6,
                    Pattern::Fan | Pattern::Sweep => 9,
                    Pattern::Pincer => 10,
                    Pattern::Comb => 15,
                    _ => 18,
                };
                w.apply(&ev(pulse(p, count, n), 0.0));
            }
            Demo::Darts => {
                w.apply(&ev(Ev::Tick { speed: 420.0 }, 0.0));
                gap = 0.5;
            }
            Demo::Laser => {
                let horizontal = n % 2 == 0;
                let reach = if horizontal { 240.0 } else { 440.0 };
                w.apply(&ev(Ev::Laser { horizontal, from: -reach, to: reach, dur: 4.0 * bl }, bl));
                gap = 8.0;
            }
            Demo::Tiles => {
                let pattern = [level::TilePattern::Checker(0), level::TilePattern::Ring(1), level::TilePattern::Checker(1), level::TilePattern::Scatter(n as u32, 9)][n % 4];
                w.apply(&ev(Ev::Tiles { pattern }, 2.0 * bl));
                gap = 6.0;
            }
            Demo::Wave => w.apply(&ev(Ev::Shockwave { gap: 0.5, gaps: 3 }, bl)),
            Demo::Upgrade(_) => {
                // something to shoot at, and something to dodge
                w.apply(&ev(pulse(Pattern::Fan, 7, n), 0.0));
                if n % 2 == 0 && w.enemies.len() + w.portals.len() < 3 {
                    w.apply(&ev(Ev::Spawn { kind: EnemyKind::Chaser, x: sx, y: sy }, 0.0));
                }
                gap = 3.0;
            }
            Demo::Music(k) => match k {
                1 => {
                    w.apply(&ev(Ev::Tiles { pattern: level::TilePattern::Checker((n % 2) as u8) }, 2.0 * bl));
                    gap = 6.0;
                }
                2 => {
                    if w.enemies.len() + w.portals.len() < 3 {
                        w.apply(&ev(Ev::Spawn { kind: EnemyKind::Chaser, x: sx, y: sy }, 0.0));
                    }
                    gap = 2.0;
                }
                3 => {
                    w.apply(&LevelEvent { t: self.show_t, lead: 0.0, ev: Ev::Shards { x: sx * 0.8, y: sy * 0.8, n: 3 } });
                    w.apply(&ev(Ev::Tick { speed: 420.0 }, 0.0));
                    gap = 1.0;
                }
                4 => {
                    w.apply(&ev(Ev::Laser { horizontal: n % 2 == 0, from: -240.0, to: 240.0, dur: 4.0 * bl }, bl));
                    gap = 8.0;
                }
                5 => {
                    let pitch = 0.15 + 0.2 * (n % 4) as f32;
                    let (cx, cy) = level::chime_point(pitch);
                    w.apply(&LevelEvent { t: self.show_t, lead: 0.0, ev: Ev::Chime { pitch, strength: 1.0 } });
                    if w.enemies.len() + w.portals.len() < 4 {
                        w.apply(&ev(Ev::Spawn { kind: EnemyKind::Wisp, x: cx, y: cy }, 0.0));
                    }
                    gap = 2.0;
                }
                6 => {
                    let pitch = 0.15 + 0.25 * (n % 3) as f32;
                    w.apply(&LevelEvent { t: self.show_t, lead: 0.0, ev: Ev::Candy { kind: (n % 3) as u8, pitch, pitch_end: 0.9 - pitch * 0.5, dur: 2.0 } });
                    gap = 8.0;
                }
                8 => {
                    if w.res_t <= 0.0 {
                        w.res_t = 8.0;
                    }
                    w.apply(&ev(pulse(Pattern::Ring, 14, n), 0.0));
                    gap = 2.0;
                }
                _ => {
                    let p = if k == 7 { Pattern::Fan } else { Pattern::Ring };
                    w.apply(&ev(pulse(p, if k == 7 { 11 } else { 14 }, n), 0.0));
                    gap = 2.0;
                }
            },
        }
        self.show_next = self.show_t + gap * bl;
    }

    /// Leave the codex: the showcase world goes, the menu's own scene comes back.
    fn codex_leave(&mut self) {
        self.show_want = Demo::None;
        if self.show_demo != Demo::None {
            let mood = self.world.mood;
            self.show_demo = Demo::None;
            self.world = World::new(self.diff(), self.diff == 0);
            self.world.mood = mood;
            self.juke.last_t = -1.0;
        }
        self.screen = Screen::Menu;
        self.menu_sel = M_CODEX;
    }

    /// The track preview in the top right corner with its buttons and the library bar. It is
    /// the same on the title menu and in run setup, so the song never moves. `focus` is the
    /// button the keyboard is on, `enter` presses it.
    fn track_corner(&mut self, tip: &mut ui::Tip, progress: f32, focus: Option<usize>, enter: bool, full: bool, u: f32, sw: f32) {
        let pal = self.rend.pal;
        let (x, y, w) = (sw - 700.0 * u, 110.0 * u, 590.0 * u);
        let (title, hint) = self.juke_title();
        self.rend.draw_now_playing(x, y, w, u, &self.world, &title, &hint, progress);
        let busy = self.dialog.is_some();
        let lib_name = self.music_dir.as_deref().map(library_label).unwrap_or_else(|| "none chosen".to_string());
        let n_lib = self.juke.list.iter().filter(|p| theme_index(p).is_none()).count();
        let (ts, gap) = (54.0 * u, 10.0 * u);
        let ty = y + 108.0 * u;
        let mut act: Option<usize> = None;
        let soft = |a: f32| Color::new(1.0, 1.0, 1.0, a);
        // row one: the track on the left, other things to play on the right
        // (files, a folder and live audio are choices for a run, so only run setup shows them)
        for (i, c) in CORNER.iter().enumerate().take(if full { 6 } else { 3 }) {
            let tx = if i < 3 { x + i as f32 * (ts + gap) } else { x + w - (6 - i) as f32 * (ts + gap) + gap };
            let f = focus == Some(i);
            let (hv, click) = ui::tile(tx, ty, ts, false, f, pal.accent, 1.0, u);
            ui::icon(c.0, tx + ts * 0.5, ty + ts * 0.5, ts * 0.25, if hv || f { pal.accent } else { soft(0.82) });
            if hv || f {
                tip.set(c.1, c.2, hv);
            }
            if click && !busy {
                act = Some(i);
            }
        }
        if full {
            render::text_right("or play", x + w - 3.0 * (ts + gap) - 6.0 * u, ty + ts * 0.5 + 6.0 * u, 17.0 * u, soft(0.45));
        }
        // row two: the library, with room for a long name
        let (ly, lh) = (ty + ts + 10.0 * u, 48.0 * u);
        ui::rrect(x, ly, w, lh, ui::RADIUS * u, Color::new(0.02, 0.0, 0.07, 0.7));
        ui::rrect_lines(x, ly, w, lh, ui::RADIUS * u, 1.2 * u, soft(0.14));
        ui::icon(Icon::Library, x + 26.0 * u, ly + lh * 0.5, 11.0 * u, pal.primary);
        let bs = 38.0 * u;
        let count = format!("{n_lib} tracks");
        let cw = text_w(&count, 17.0 * u);
        let bx0 = x + w - 2.0 * (bs + 6.0 * u);
        render::text_right(&count, bx0 - 12.0 * u, ly + lh * 0.5 + 6.0 * u, 17.0 * u, soft(0.5));
        let name_w = bx0 - 24.0 * u - cw - (x + 52.0 * u);
        ui::marquee(&lib_name, x + 52.0 * u, ly + lh * 0.5 + 8.0 * u, 22.0 * u, name_w, soft(0.9), self.rend.time);
        if ui::hover(x, ly, bx0 - x, lh) {
            tip.set("MUSIC LIBRARY", &format!("{lib_name}, {n_lib} tracks. Runs and this menu draw from it. The two buttons choose a folder or an .m3u playlist."), true);
        }
        for i in 6..8 {
            let c = &CORNER[i];
            let tx = bx0 + (i - 6) as f32 * (bs + 6.0 * u);
            let f = focus == Some(i);
            let (hv, click) = ui::tile(tx, ly + (lh - bs) * 0.5, bs, false, f, pal.accent, 1.0, u);
            ui::icon(c.0, tx + bs * 0.5, ly + lh * 0.5, bs * 0.26, if hv || f { pal.accent } else { soft(0.82) });
            if hv || f {
                tip.set(c.1, c.2, hv);
            }
            if click && !busy {
                act = Some(i);
            }
        }
        if !busy {
            if enter {
                act = focus;
            }
            if is_key_pressed(KeyCode::Q) {
                act = Some(0);
            } else if is_key_pressed(KeyCode::E) {
                act = Some(1);
            } else if is_key_pressed(KeyCode::T) {
                act = Some(2);
            }
        }
        match act {
            Some(0) => self.juke_next(-1),
            Some(1) => self.juke_next(1),
            Some(2) => {
                self.track_sel = self.juke.idx;
                // drop the key that opened the list so it does not count as a letter jump
                while get_char_pressed().is_some() {}
                self.tracks_back = self.screen;
                self.screen = Screen::Tracks;
                self.audio.sfx(Sfx::UiSelect, 0.8, 0.0);
            }
            Some(3) => self.open_files_dialog(),
            Some(4) => self.open_folder_dialog(false, false),
            Some(5) => {
                self.audio.sfx(Sfx::UiSelect, 0.8, 0.0);
                self.audio.refresh_devices();
                self.screen = Screen::LiveSetup;
                self.menu_sel = 0;
            }
            Some(6) => self.open_folder_dialog(true, false),
            Some(7) => self.open_folder_dialog(true, true),
            _ => {}
        }
    }

    /// Start a run with the track the menu is playing, plus `extra` more from the playlist.
    fn start_from_juke(&mut self, extra: usize, endless: bool) {
        let n = self.juke.list.len();
        if n == 0 {
            self.say("no music found - choose a MUSIC LIBRARY folder or drop files on the window");
            return;
        }
        let mut paths = vec![self.juke.list[self.juke.idx % n].clone()];
        for k in 1..=extra {
            let p = self.juke.list[(self.juke.idx + k) % n].clone();
            if !paths.contains(&p) {
                paths.push(p);
            }
        }
        // The menu already analysed this track: reuse it so the level starts instantly.
        let reuse = self.juke.loaded.as_ref().filter(|(p, _)| *p == paths[0]).map(|(_, l)| (l.track.clone(), l.an.clone()));
        self.endless = endless;
        self.juke_seq = self.juke.idx % n;
        self.start_run(paths);
        if let Some((track, an)) = reuse {
            let level = level::generate(&an, self.args.seed, self.diff());
            self.loader = None;
            self.ready = Some((0, Loaded { track, an, level }));
        }
    }

    /// Earn a milestone (once): it is kept with the scores and announced.
    fn earn(&mut self, id: &str) {
        let key = format!("ACH|{id}");
        if self.scores.contains_key(&key) || self.args.selftest.is_some() {
            return;
        }
        self.scores.insert(key, (1, 0));
        self.save_scores();
        if let Some(m) = MILESTONES.iter().find(|m| m.0 == id) {
            self.say(format!("MILESTONE   {}", m.1));
            self.audio.sfx(Sfx::Upgrade, 0.9, 0.0);
        }
    }

    /// At the end of a run: which milestones did it earn?
    fn check_milestones(&mut self) {
        let st = self.world.stats;
        let cleared = st.songs >= 1;
        let depth = self.world.depth + 1;
        let mut got: Vec<&str> = Vec::new();
        if st.hearts_broken >= 1 { got.push("heart"); }
        if depth >= 5 { got.push("depth3"); }
        if depth >= 10 { got.push("depth5"); }
        if cleared && st.hits == 0 { got.push("clean"); }
        if st.grazes >= 150 { got.push("graze"); }
        if st.hearts_broken >= 5 { got.push("breaker"); }
        if st.resonances >= 3 { got.push("resonant"); }
        if cleared && self.world.base_mods & game::MOD_GLASS != 0 { got.push("glass"); }
        if cleared && self.diff == DIFFS.len() - 1 { got.push("flatline"); }
        if cleared && self.world.pacifist { got.push("dodge"); }
        for id in got {
            self.earn(id);
        }
    }

    fn save_scores(&self) {
        if let Some(path) = scores_path() {
            let mut text = String::new();
            for (k, v) in &self.scores {
                text.push_str(&format!("{k}\t{}\t{}\n", v.0, v.1));
            }
            let _ = std::fs::write(path, text);
        }
    }

    /// Start Highway on the track the menu is playing (it is already analysed).
    fn start_highway(&mut self) {
        let Some((path, loaded)) = self.juke.loaded.as_ref().filter(|(p, _)| Some(p) == self.juke.list.get(self.juke.idx)) else {
            self.say("still reading the music: try again in a moment");
            return;
        };
        let title = track_title(path);
        let events: Vec<LevelEvent> = loaded.level.events.iter().copied().filter(|e| matches!(e.ev, Ev::Beat { .. } | Ev::Hit { .. } | Ev::Section { .. } | Ev::Chime { .. })).collect();
        let sess = RunSession {
            r: runner::Runner::new(&loaded.an, self.diff()),
            an: loaded.an.clone(),
            key: format!("{}|{}|HIGHWAY", title.replace(['\t', '\n', '|'], " "), DIFFS[self.diff].1),
            title,
            events,
            cursor: 0,
            saved: false,
            best: false,
        };
        let (track, mood, bpm) = (loaded.track.clone(), loaded.level.mood, loaded.level.bpm);
        self.world = World::new(self.diff(), true);
        self.world.mood = mood;
        self.world.beat_len = if bpm > 1.0 { 60.0 / bpm } else { 0.5 };
        self.audio.play_ex(track, 0.0, false, 0.35);
        self.juke.playing = false;
        self.juke.last_t = -1.0;
        self.runner = Some(Box::new(sess));
        self.screen = Screen::Runner;
    }

    /// One frame of Highway. Returns 0..1 through the song.
    fn step_highway(&mut self, dt: f32) -> f32 {
        let auto = self.args.selftest.is_some();
        let t = (self.audio.song_time() as f32 - self.offset_ms / 1000.0).max(0.0);
        let finished = self.audio.finished();
        let Some(s) = self.runner.as_mut() else {
            self.to_menu();
            return 0.0;
        };
        while s.cursor < s.events.len() && s.events[s.cursor].at() <= t {
            let e = s.events[s.cursor];
            s.cursor += 1;
            self.world.apply(&e);
        }
        let feat = feat_at(&s.an, t);
        let bp = beat_pos(&s.r.beats, t);
        self.world.t = t;
        self.world.update(dt, t, &Input::default(), bp, &feat, s.an.intensity_at(t), false);
        let (steer, jump) = if auto {
            s.r.autopilot()
        } else {
            let right = is_key_pressed(KeyCode::D) || is_key_pressed(KeyCode::Right);
            let left = is_key_pressed(KeyCode::A) || is_key_pressed(KeyCode::Left);
            (right as i32 - left as i32, is_key_pressed(KeyCode::Space) || is_key_pressed(KeyCode::W) || is_key_pressed(KeyCode::Up))
        };
        let was_done = s.r.done;
        let mut earned: Vec<&str> = Vec::new();
        s.r.step(dt, t, steer, jump);
        if finished && t > 3.0 {
            s.r.done = true;
        }
        for fx in s.r.fx.clone() {
            let (sfx, gain) = match fx {
                runner::RunFx::Gem => (Sfx::Shard, 0.5),
                runner::RunFx::Star => (Sfx::Upgrade, 0.6),
                runner::RunFx::Clear => (Sfx::NoteShot, 0.7),
                runner::RunFx::Perfect => (Sfx::Shield, 0.6),
                runner::RunFx::Hit => (Sfx::Hurt, 1.0),
                runner::RunFx::Jump => (Sfx::Dash, 0.4),
                runner::RunFx::Lane => (Sfx::UiMove, 0.4),
            };
            self.audio.sfx(sfx, gain, s.r.lane_x * 0.6);
        }
        if s.r.done && !was_done {
            if s.r.dead {
                self.audio.fade_out(0.8);
            }
            if !s.saved && !auto {
                if matches!(s.r.rank(), "S" | "A") {
                    earned.push("road");
                }
                if s.r.perfects >= 20 {
                    earned.push("jumps");
                }
                s.saved = true;
                if s.r.score > 0 && self.scores.get(&s.key).map_or(true, |b| s.r.score > b.0) {
                    s.best = true;
                    self.scores.insert(s.key.clone(), (s.r.score, 1));
                }
            }
        }
        let p = t / s.r.duration.max(0.1);
        if self.runner.as_ref().is_some_and(|s| s.best && s.saved) {
            self.save_scores();
        }
        for id in earned {
            self.earn(id);
        }
        p
    }

    fn to_menu(&mut self) {
        // The menu moves on to the track after the one the run (or the road) ended on.
        let n = self.juke.list.len();
        if n > 1 && self.live.is_none() && (self.runner.is_some() || !self.queue.is_empty()) {
            let last = self.queue.get(self.qi.min(self.queue.len().saturating_sub(1)));
            let at = last.and_then(|p| self.juke.list.iter().position(|q| q == p)).unwrap_or(self.juke.idx);
            self.juke.idx = (at + 1) % n;
            self.juke.loader = None;
            self.juke.fails = 0;
            self.juke.played = 0.0;
        }
        self.queue.clear();
        self.qi = 0;
        self.runner = None;
        self.juke.playing = false;
        self.audio.stop();
        self.audio.stop_live();
        self.song = None;
        self.live = None;
        self.loader = None;
        self.ready = None;
        self.screen = Screen::Menu;
        self.world = World::new(self.diff(), self.diff == 0);
        self.rend.set_palette(0);
    }

    /// Tier and mutator score multiplier of the run being set up.
    fn score_mult(&self) -> f32 {
        TIER_SCORE[self.diff.min(TIER_SCORE.len() - 1)] * (1.0 + game::MUTATOR_SCORE * self.mutators.count_ones() as f32)
    }

    /// Best scores are kept per starting track, tier and mode.
    fn score_key(&self, title: &str) -> String {
        format!("{}|{}|{}", title.replace(['\t', '\n', '|'], " "), DIFFS[self.diff].1, SETUP_MODES[self.setup_mode.min(SETUP_MODES.len() - 1)].1)
    }

    /// At the end of a run: record the score if it is the best for its key.
    fn record_score(&mut self) {
        if self.run_saved {
            return;
        }
        self.run_saved = true;
        if self.args.selftest.is_some() || self.run_key.is_empty() || self.live.is_some() {
            return;
        }
        self.check_milestones();
        let (score, depth) = (self.world.score, self.world.depth + 1);
        // Depth is the goal: a deeper run is the better one, score breaks ties.
        if self.scores.get(&self.run_key).map_or(true, |b| depth > b.1 || (depth == b.1 && score > b.0)) && score > 0 {
            self.run_best = true;
            self.scores.insert(self.run_key.clone(), (score, depth));
            if let Some(path) = scores_path() {
                let mut text = String::new();
                for (k, v) in &self.scores {
                    text.push_str(&format!("{k}\t{}\t{}\n", v.0, v.1));
                }
                let _ = std::fs::write(path, text);
            }
        }
    }

    fn start_run(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            self.say("no playable audio files found");
            return;
        }
        self.run_key = self.score_key(&track_title(&paths[0]));
        self.run_saved = false;
        self.run_best = false;
        // Menu music keeps playing through the loading screen and cross-fades into the level.
        if !self.juke.playing {
            self.audio.stop();
        }
        self.juke.playing = false;
        self.audio.stop_live();
        self.live = None;
        self.song = None;
        self.ready = None;
        self.queue = paths;
        self.qi = 0;
        let god = self.args.selftest.is_some() && !self.args.mortal;
        self.world = World::new(self.diff(), self.diff == 0 || god);
        self.world.base_mods = self.mutators;
        self.world.score_mult = self.score_mult();
        self.world.pacifist = self.setup_mode == MODE_DODGE;
        self.apply_diff_rules();
        if self.mutators & game::MOD_GLASS != 0 && self.world.player.max_hp > 2 {
            self.world.player.max_hp = 2;
            self.world.player.hp = 2;
        }
        for _ in 0..self.args.give {
            let up = UpgradeId::ALL[self.world.rng.below(UpgradeId::ALL.len())];
            self.world.player.ups[up as usize] = (self.world.player.ups[up as usize] + 1).min(up.max());
        }
        self.died = false;
        self.load_in_arena = false;
        self.loader = Some(spawn_loader(0, self.queue[0].clone(), self.track_seed(0), self.diff()));
        self.screen = Screen::Play;
        self.phase = Phase::Loading;
    }

    fn start_live(&mut self) {
        let devs = self.audio.devices();
        let capture = devs.get(self.cap_sel).map(|d| d.id.clone());
        // out_sel 0 = none (reactive)
        let playback = if self.out_sel == 0 { None } else { devs.get(self.out_sel - 1).map(|d| d.id.clone()) };
        let delayed = playback.is_some() && playback != capture;
        let delay = if delayed { self.delay } else { 0.0 };
        self.juke.playing = false;
        self.audio.stop();
        self.audio.start_live(capture, playback, delay);
        self.song = None;
        self.world = World::new(self.diff(), self.diff == 0);
        self.apply_diff_rules();
        self.world.begin_song(game::LIVE_HEART_SECS, 0, 0xC0FFEE);
        self.world.live_phases = true;
        let lookahead = if delayed { (delay - 0.25).max(0.0) } else { 0.0 };
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(7);
        self.live = Some(LiveRun {
            an: None,
            gen: LiveGen::new(seed ^ self.args.seed, self.diff(), lookahead),
            feats: VecDeque::new(),
            pending: Vec::new(),
            buf: Vec::new(),
            out: StreamOut::default(),
            silent_for: 10.0,
            was_idle: true,
            last_beat_t: -1.0,
            beat_n: 0,
            last_clock: 0.0,
            gen_diff: self.diff(),
            last_pred: -10.0,
            delay,
            hearts: 0,
            period: 0.5,
            ibis: VecDeque::new(),
        });
        self.died = false;
        self.screen = Screen::Play;
        self.phase = Phase::Song;
    }

    fn poll_loader(&mut self) {
        let mut done = None;
        if let Some(l) = &self.loader {
            if let Ok(res) = l.rx.try_recv() {
                done = Some((l.idx, res));
            }
        }
        if let Some((idx, res)) = done {
            self.loader = None;
            match res {
                Ok(loaded) => self.ready = Some((idx, loaded)),
                Err(e) => {
                    let name = self.queue.get(idx).map(|p| track_title(p)).unwrap_or_default();
                    self.say(format!("skipped {name}: {e}"));
                    if idx < self.queue.len() {
                        self.queue.remove(idx);
                    }
                    if self.queue.is_empty() || (self.qi >= self.queue.len() && self.song.is_none()) {
                        if self.world.stats.songs > 0 {
                            self.screen = Screen::Results;
                        } else {
                            self.to_menu();
                        }
                    }
                }
            }
        }
        // Keep the next track warming up in the background.
        if self.loader.is_none() && !matches!(self.screen, Screen::Menu | Screen::Options | Screen::Setup | Screen::Codex | Screen::Tracks) && self.live.is_none() {
            let want = if self.song.is_some() && self.phase == Phase::Song { self.qi + 1 } else { self.qi };
            let have = self.ready.as_ref().map(|r| r.0);
            if want < self.queue.len() && have != Some(want) {
                self.loader = Some(spawn_loader(want, self.queue[want].clone(), self.track_seed(want), self.diff()));
            }
        }
    }

    /// The next library track that is not already in this run (wraps around when all are used).
    fn next_library_track(&mut self) -> Option<PathBuf> {
        let n = self.juke.list.len();
        if n == 0 {
            // No library at all: go round what the run already played.
            return self.queue.first().cloned();
        }
        for _ in 0..n {
            self.juke_seq = (self.juke_seq + 1) % n;
            let p = &self.juke.list[self.juke_seq];
            if !self.queue.contains(p) {
                return Some(p.clone());
            }
        }
        self.juke.list.get(self.juke_seq).cloned()
    }

    /// The level seed of queue entry `idx`. A track that comes round again in the same run is
    /// dealt differently each time.
    fn track_seed(&self, idx: usize) -> u64 {
        let laps = self.queue.get(idx).map_or(0, |p| self.queue[..idx].iter().filter(|q| *q == p).count()) as u64;
        self.args.seed.wrapping_add(laps.wrapping_mul(0xA24B_AED4_963E_E407))
    }

    /// Trade the song that just started for another: forward (`dir` > 0) skips to what comes
    /// next, backward goes to the library track before it. The build, the score and the depth
    /// stay. Returns false when there is nothing to trade it for.
    fn swap_song(&mut self, dir: i32) -> bool {
        if self.qi >= self.queue.len() {
            return false;
        }
        let n = self.juke.list.len();
        if dir > 0 {
            if self.qi + 1 >= self.queue.len() {
                match self.next_library_track() {
                    Some(p) if p != self.queue[self.qi] => self.queue.push(p),
                    _ => return false,
                }
            }
            self.queue.remove(self.qi);
        } else {
            let cur = &self.queue[self.qi];
            let at = self.juke.list.iter().position(|p| p == cur).unwrap_or(self.juke_seq.min(n.saturating_sub(1)));
            let mut found = None;
            let mut k = at;
            for _ in 0..n {
                k = (k + n - 1) % n;
                if !self.queue.contains(&self.juke.list[k]) {
                    found = Some(self.juke.list[k].clone());
                    break;
                }
            }
            match found {
                Some(p) => self.queue[self.qi] = p,
                None => return false,
            }
        }
        // The track that was warming up as "next" is now the current one; anything else is stale.
        self.ready = match self.ready.take() {
            Some((i, l)) if dir > 0 && i == self.qi + 1 => Some((self.qi, l)),
            _ => None,
        };
        self.loader = None;
        self.song = None;
        self.world.calm_down();
        self.load_in_arena = true;
        self.phase = Phase::Loading;
        if self.qi == 0 {
            self.run_key = self.score_key(&track_title(&self.queue[0]));
        }
        self.audio.sfx(Sfx::UiSelect, 0.8, 0.0);
        true
    }

    /// Carry on after the last queued track: same build, next track from the library.
    fn continue_run(&mut self) {
        self.endless = true;
        if let Some(p) = self.next_library_track() {
            self.queue.push(p);
            self.qi = self.queue.len() - 1;
            self.juke.playing = false;
            self.audio.stop();
            self.screen = Screen::Play;
            self.phase = Phase::Intermission(0.0);
        }
    }

    fn begin_song(&mut self, loaded: Loaded) {
        // Endless runs always have the next track queued, so it is analysed while this one plays.
        if self.endless && self.qi + 1 >= self.queue.len() {
            if let Some(p) = self.next_library_track() {
                self.queue.push(p);
            }
        }
        let mut start = if self.qi == 0 { self.args.start.max(0.0) } else { 0.0 };
        let mut end_t = f32::MAX;
        let mut active = loaded.level.active_secs;
        if self.setup_mode == MODE_RUSH {
            // DROP RUSH: the first drop, or failing that the most intense section, with a
            // short run-up.
            let secs = &loaded.an.sections;
            let pick = secs.iter().skip(1).find(|s| s.kind == SectionKind::Drop).or_else(|| {
                secs.iter().skip(1).max_by(|a, b| a.intensity.partial_cmp(&b.intensity).unwrap_or(std::cmp::Ordering::Equal))
            });
            if let Some(s) = pick {
                // come in on a beat, two bars before the section
                let beats = &loaded.level.beats;
                let k = beats.partition_point(|b| *b < s.t0 - 0.05);
                start = if k >= 8 { beats[k - 8] } else { (s.t0 - 4.0).max(0.0) };
                end_t = s.t1;
                active = (end_t - start).max(8.0);
            }
        }
        self.world.begin_song(active, self.qi as u32, loaded.level.seed);
        // The Heart's clock is this song (or, in DROP RUSH, the part of it that is played).
        let due = if end_t < f32::MAX { end_t } else { loaded.level.duration };
        if start > 0.0 && end_t == f32::MAX {
            // started part-way in (a debug start): the Heart is sized for what is left
            active *= ((due - start) / due.max(1.0)).clamp(0.05, 1.0);
            let hp = self.world.heart.max_hp * ((due - start) / due.max(1.0)).clamp(0.05, 1.0);
            self.world.heart.hp = hp;
            self.world.heart.max_hp = hp;
        }
        self.world.set_heart_window(start, due, active);
        self.world.gun = loaded.level.gun.clone();
        if !self.world.pacifist {
            self.world.set_phases(&loaded.level.phases);
        }
        self.world.mood = loaded.level.mood;
        if self.qi > 0 {
            let title = loaded.track.name.clone();
            self.world.announce_depth(&title);
        }
        self.world.beat_len = if loaded.level.bpm > 1.0 { 60.0 / loaded.level.bpm } else { 0.5 };
        let timeline = Arc::new(Timeline::from_analysis(&loaded.an));
        let mut song = Song { track: loaded.track, an: loaded.an, level: loaded.level, timeline, cursor: 0, last_t: start, end_t };
        if start > 0.0 {
            // Skip what is already in the past, but keep the latest section so the look is right.
            let mut last_section = None;
            while song.cursor < song.level.events.len() && song.level.events[song.cursor].at() < start {
                if matches!(song.level.events[song.cursor].ev, Ev::Section { .. }) {
                    last_section = Some(song.level.events[song.cursor]);
                }
                song.cursor += 1;
            }
            if let Some(e) = last_section {
                self.world.t = start;
                self.world.apply(&e);
            }
        }
        self.audio.set_volume(if self.args.mute { 0.0 } else { self.volume });
        let fade = if !self.audio.has_track() {
            0.0
        } else if self.setup_mode == MODE_RUSH && self.qi > 0 {
            1.3
        } else {
            0.45
        };
        self.was_silent = false;
        self.song_age = 0.0;
        // The first song of a run opens with the heartbeat, not the speed lines that mark a
        // change of song everywhere else.
        if self.qi == 0 && !self.load_in_arena && (self.args.selftest.is_none() || self.args.screen == "launch") {
            self.launch = 0.0;
            self.world.warp = 0.0;
        }
        self.load_in_arena = false;
        self.audio.play_ex(song.track.clone(), start as f64, false, fade);
        self.juke.playing = false;
        self.song = Some(song);
        self.phase = Phase::Song;
    }

    fn gather_input(&self, allow_fire: bool) -> Input {
        let mut mv = Vec2::ZERO;
        if is_key_down(KeyCode::A) || is_key_down(KeyCode::Left) {
            mv.x -= 1.0;
        }
        if is_key_down(KeyCode::D) || is_key_down(KeyCode::Right) {
            mv.x += 1.0;
        }
        if is_key_down(KeyCode::W) || is_key_down(KeyCode::Up) {
            mv.y -= 1.0;
        }
        if is_key_down(KeyCode::S) || is_key_down(KeyCode::Down) {
            mv.y += 1.0;
        }
        let (mx, my) = mouse_position();
        let aim = self.rend.view.to_world(vec2(mx, my));
        Input {
            mv,
            aim,
            fire: allow_fire && (self.autofire || is_mouse_button_down(MouseButton::Left)),
            dash: is_key_pressed(KeyCode::Space) || is_key_pressed(KeyCode::LeftShift) || is_mouse_button_pressed(MouseButton::Right),
        }
    }

    // -----------------------------------------------------------------------------------------
    // Per-frame simulation for each mode. Returns (idle_target, hud).
    // -----------------------------------------------------------------------------------------

    fn step_files(&mut self, dt: f32, input: &Input) -> (f32, HudInfo, f32) {
        let mut hud = HudInfo::empty();
        let total = self.queue.len();
        if total > 1 {
            hud.track_no = format!("{}/{}", (self.qi + 1).min(total), total);
        }
        let idle_beat = self.rend.time * 100.0 / 60.0;
        match self.phase {
            Phase::Loading => {
                if let Some((idx, _)) = &self.ready {
                    if *idx == self.qi {
                        let (_, loaded) = self.ready.take().unwrap();
                        self.begin_song(loaded);
                        return self.step_files(dt, input);
                    }
                }
                let name = self.queue.get(self.qi).map(|p| track_title(p)).unwrap_or_default();
                hud.title = name;
                if let Some(l) = &self.loader {
                    let p = l.progress.load(Ordering::Relaxed) as f32 / 10.0;
                    let stage = if l.stage.load(Ordering::Relaxed) == 0 { "DECODING" } else { "READING THE MUSIC" };
                    hud.status = format!("{stage}  {p:.0}%");
                } else {
                    hud.status = "PREPARING".into();
                }
                let t = self.world.t;
                let mut inp = *input;
                inp.fire = false;
                self.world.update(dt, t, &inp, idle_beat, &Feat::default(), 0.0, false);
                (if self.qi == 0 && !self.load_in_arena { 1.0 } else { 0.0 }, hud, 0.0)
            }
            Phase::Song => {
                // Not feeling this one? In its first seconds Q / E trade it for another.
                self.song_age += dt;
                let debug_swap = if self.args.swap != 0 && self.song_age > 2.0 { std::mem::take(&mut self.args.swap) } else { 0 };
                if (self.song_age < SWAP_SECS && !self.world.dead && self.args.selftest.is_none()) || debug_swap != 0 {
                    let dir = is_key_pressed(KeyCode::E) as i32 - is_key_pressed(KeyCode::Q) as i32 + debug_swap;
                    if dir != 0 && self.swap_song(dir) {
                        return self.step_files(dt, input);
                    }
                }
                let Some(song) = self.song.as_mut() else {
                    self.phase = Phase::Loading;
                    return (1.0, hud, 0.0);
                };
                let mut t = (self.audio.song_time() as f32 - self.offset_ms / 1000.0).max(0.0);
                if t < song.last_t {
                    t = song.last_t;
                }
                // Never let a hitch fast-forward the simulation by more than a moment.
                t = t.min(song.last_t + 0.25);
                song.last_t = t;
                self.world.t = t;
                let drift = self.args.drift;
                while song.cursor < song.level.events.len() && song.level.events[song.cursor].at() <= t {
                    let e = song.level.events[song.cursor];
                    song.cursor += 1;
                    if !drift || matches!(e.ev, Ev::Beat { .. } | Ev::Hit { .. } | Ev::Section { .. } | Ev::End) {
                        self.world.apply(&e);
                    }
                }
                let silent = drift || (song.an.in_silence(t) && song.an.in_silence(t + 0.7));
                // A quiet stretch clears the attacks once. The camera does not move: the arena
                // stays the arena until the run is over.
                if silent && !self.was_silent && !drift {
                    self.world.calm_down();
                }
                self.was_silent = silent;
                let feat = feat_at(&song.an, t);
                let intensity = song.an.intensity_at(t);
                let bp = beat_pos(&song.level.beats, t);
                self.world.update(dt, t, input, bp, &feat, intensity, !silent);
                let dur = song.level.duration.max(0.1);
                hud.title = song.track.name.clone();
                if self.song_age < SWAP_SECS && self.args.selftest.is_none() {
                    hud.status = format!("Q / E   change song   {:.0}", (SWAP_SECS - self.song_age).ceil());
                }
                hud.progress = t / dur;
                hud.time_left = dur - t;
                hud.bpm = song.level.bpm;
                hud.show_combat = !drift;
                hud.timeline = Some(song.timeline.clone());
                let finished = self.world.ended || (self.audio.finished() && t >= dur - 0.3) || t >= song.end_t;
                if self.world.dead {
                    self.phase = Phase::Dying(0.0);
                    self.audio.set_volume(if self.args.mute { 0.0 } else { self.volume * 0.35 });
                } else if finished {
                    if !self.world.ended {
                        self.world.apply(&LevelEvent { t, lead: 0.0, ev: Ev::End });
                    }
                    // DROP RUSH never goes quiet between tracks: this one plays on past its
                    // section until the next one fades in over it.
                    let rush_on = self.setup_mode == MODE_RUSH && self.qi + 1 < self.queue.len();
                    if !rush_on {
                        self.audio.stop();
                    }
                    self.world.calm_down();
                    self.qi += 1;
                    self.song = None;
                    if self.qi < self.queue.len() {
                        self.phase = Phase::Intermission(0.0);
                    } else {
                        self.screen = Screen::Results;
                    }
                }
                (if drift { 1.0 } else { 0.0 }, hud, hud_progress(t, dur))
            }
            Phase::Intermission(mut it) => {
                it += dt;
                self.phase = Phase::Intermission(it);
                let t = self.world.t;
                let mut inp = *input;
                inp.fire = false;
                self.world.update(dt, t, &inp, idle_beat, &Feat::default(), 0.0, false);
                hud.title = self.queue.get(self.qi).map(|p| format!("NEXT  {}", track_title(p))).unwrap_or_default();
                let rush = self.setup_mode == MODE_RUSH;
                hud.banner = if rush { "DROP CLEAR".into() } else { "TRACK CLEAR".into() };
                hud.banner_a = (it * 3.0).min(1.0) * ((2.2 - it) * 2.5).clamp(0.0, 1.0);
                let ready = self.ready.as_ref().map(|r| r.0 == self.qi).unwrap_or(false);
                hud.status = if !self.world.orbs.is_empty() {
                    "FLY INTO AN UPGRADE".into()
                } else if !ready {
                    "READING THE MUSIC".into()
                } else {
                    String::new()
                };
                // The next track rolls in after a short breath; upgrade orbs stay on the field
                // through its opening seconds (nothing attacks yet).
                if ready && it > if rush { 1.1 } else { 2.0 } {
                    let (_, loaded) = self.ready.take().unwrap();
                    self.begin_song(loaded);
                }
                // The view stays on the arena between tracks.
                (0.0, hud, 0.0)
            }
            Phase::Dying(mut dtm) => {
                dtm += dt;
                self.phase = Phase::Dying(dtm);
                let t = self.world.t;
                self.world.update(dt * 0.35, t, input, 0.0, &Feat::default(), 0.0, false);
                if dtm > 1.8 {
                    self.audio.stop();
                    self.audio.stop_live();
                    self.died = true;
                    self.screen = Screen::Results;
                }
                hud.show_combat = true;
                (0.0, hud, 0.0)
            }
        }
    }

    /// While live mode is paused: keep the analyser's clock in step with the capture, but throw
    /// away everything it hears.
    fn live_discard(&mut self) {
        let Some(live) = self.live.as_mut() else { return };
        live.buf.clear();
        self.audio.live_take(&mut live.buf);
        if let Some(an) = live.an.as_mut() {
            live.out.feats.clear();
            live.out.onsets.clear();
            an.push(&live.buf, &mut live.out);
        }
        live.pending.clear();
    }

    /// Leave the live-mode pause: nothing heard during it may fire.
    fn live_resume(&mut self) {
        let clock = self.audio.live_time() as f32;
        let Some(live) = self.live.as_mut() else { return };
        live.pending.clear();
        live.feats.clear();
        live.last_clock = live.last_clock.max(clock);
        live.last_beat_t = live.last_beat_t.max(clock - live.period);
        live.silent_for = 0.0;
        live.gen.resume();
    }

    fn step_live(&mut self, dt: f32, input: &Input) -> (f32, HudInfo, f32) {
        let mut hud = HudInfo::empty();
        hud.title = "LIVE AUDIO".into();
        hud.show_combat = true;
        if let Phase::Dying(mut dtm) = self.phase {
            dtm += dt;
            self.phase = Phase::Dying(dtm);
            let t = self.world.t;
            self.world.update(dt * 0.35, t, input, 0.0, &Feat::default(), 0.0, false);
            if dtm > 1.8 {
                self.audio.stop_live();
                self.died = true;
                self.live = None;
                self.screen = Screen::Results;
            }
            return (0.0, hud, 0.0);
        }
        let info = self.audio.live_info();
        let status = self.audio.status();
        let Some(live) = self.live.as_mut() else { return (1.0, hud, 0.0) };
        if !info.running {
            hud.status = if status.is_empty() { "OPENING AUDIO DEVICE".into() } else { status };
            let t = self.world.t;
            self.world.update(dt, t, input, self.rend.time * 100.0 / 60.0, &Feat::default(), 0.0, false);
            return (1.0, hud, 0.0);
        }
        let an = live.an.get_or_insert_with(|| StreamAnalyzer::new(info.sr));

        // Analyse whatever was captured since the last frame.
        live.buf.clear();
        self.audio.live_take(&mut live.buf);
        live.out.feats.clear();
        live.out.onsets.clear();
        an.push(&live.buf, &mut live.out);
        let live_beat = an.live_beat();
        let mut fresh: Vec<LevelEvent> = Vec::new();
        for &(ft, f, raw) in &live.out.feats {
            live.gen.feed(ft, f.rms, &mut fresh);
            live.feats.push_back((ft, f, raw));
        }
        for o in &live.out.onsets {
            live.gen.on_onset(o, &mut fresh);
            if o.band == Band::Low && o.strength > 0.3 && o.t - live.last_beat_t > 0.28 {
                // Fold the gap into the 0.33..0.9 s range (about 67..180 BPM) and keep a running median.
                let mut gap = o.t - live.last_beat_t;
                while gap > 0.9 {
                    gap *= 0.5;
                }
                if gap >= 0.3 {
                    live.ibis.push_back(gap);
                    if live.ibis.len() > 15 {
                        live.ibis.pop_front();
                    }
                    let mut v: Vec<f32> = live.ibis.iter().copied().collect();
                    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                    live.period = v[v.len() / 2];
                    live.gen.beat_len = live.period;
                }
                live.last_beat_t = o.t;
                live.beat_n += 1;
                fresh.push(LevelEvent { t: o.t, lead: 0.0, ev: Ev::Beat { down: live.beat_n % 4 == 1 } });
            }
        }
        live.pending.extend(fresh);

        let clock = (self.audio.live_time() as f32).max(live.last_clock);
        live.last_clock = clock;
        while live.feats.len() > 2 && live.feats[1].0 < clock - 2.0 {
            live.feats.pop_front();
        }

        // Is there sound around the audible moment?
        let mut heard = false;
        let mut feat = Feat::default();
        let mut best = f32::MAX;
        for &(ft, f, raw) in live.feats.iter() {
            if ft > clock - 1.2 && ft < clock + 0.6 && raw > 0.003 {
                heard = true;
            }
            let d = (ft - clock).abs();
            if d < best {
                best = d;
                feat = f;
            }
        }
        if best > 0.25 {
            feat = Feat::default();
        }
        let stalled = !info.delayed && info.since_packet > 0.35;
        let signal = heard && !info.rebuffering && !stalled;
        if signal {
            live.silent_for = 0.0;
        } else {
            live.silent_for += dt;
        }
        let idle_now = live.silent_for > 1.2;
        if idle_now && !live.was_idle {
            self.world.calm_down();
            live.pending.clear();
        }
        if !idle_now && live.was_idle {
            live.gen.resume();
            if self.world.heart.broken {
                live.hearts += 1;
                self.world.begin_song(210.0, live.hearts, 0xC0FFEE + live.hearts as u64);
            }
        }
        live.was_idle = idle_now;

        // Fire due events.
        self.world.t = clock;
        if !live.pending.is_empty() {
            let mut i = 0;
            while i < live.pending.len() {
                if live.pending[i].at() <= clock {
                    let e = live.pending.swap_remove(i);
                    // Drop anything hopelessly stale (e.g. after a stall).
                    if clock - e.t < 1.0 {
                        self.world.apply(&e);
                    }
                } else {
                    i += 1;
                }
            }
        }
        let intensity = live.gen.intensity;
        // Prefer the analyser's tempo tracker once it is confident; until then follow the kicks.
        // Reactive mode has no look-ahead, so a pulse fired on a kick as it is heard gets no
        // warning. With a confident beat the next one can be predicted: announce its pulse a
        // full warning ahead, and stop reacting to kicks.
        let predict = !info.delayed && live_beat.conf > 0.4 && live_beat.period > 0.2;
        live.gen.predicted = predict;
        if predict && !idle_now {
            let per = live_beat.period;
            let next = live_beat.phase_t + ((clock - live_beat.phase_t) / per).ceil() * per;
            let lead = level::pulse_lead(per, live.gen_diff);
            if next - clock <= lead + 0.04 && next > live.last_pred + 0.5 * per {
                live.last_pred = next;
                let mut ahead: Vec<LevelEvent> = Vec::new();
                live.gen.on_predicted_beat(next, &mut ahead);
                live.pending.extend(ahead);
            }
        }
        let live_bp = if live_beat.conf > 0.4 && live_beat.period > 0.2 {
            live.period = live_beat.period;
            live.gen.beat_len = live.period;
            (clock - live_beat.phase_t) / live_beat.period
        } else {
            live.beat_n as f32 + ((clock - live.last_beat_t) / live.period.max(0.2)).clamp(0.0, 0.999)
        };
        self.world.beat_len = live.period;
        self.world.update(dt, clock, input, live_bp, &feat, intensity, !idle_now);

        hud.mode = if info.delayed {
            format!("LIVE  -  DELAYED {:.1}s LOOK-AHEAD", live.delay)
        } else {
            "LIVE  -  REACTIVE".into()
        };
        if info.rebuffering {
            hud.status = format!("BUFFERING  {:.1} / {:.1}s", info.buffered.clamp(0.0, info.delay), info.delay);
        } else if idle_now {
            hud.status = "WAITING FOR AUDIO".into();
        } else if !status.is_empty() {
            hud.status = status;
        }
        if self.world.dead {
            self.phase = Phase::Dying(0.0);
        }
        // A gap clears the attacks, but the view only drifts off once the music has really stopped.
        (if live.silent_for > 8.0 { 1.0 } else { 0.0 }, hud, 0.0)
    }
}

fn hud_progress(t: f32, dur: f32) -> f32 {
    (t / dur.max(0.1)).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------------------------
// LAN Rally: lobby and match flow (sockets, protocol and threads live in net.rs)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LanStage {
    Choose,
    Join,
    Lobby,
    Countdown,
    Match,
    Results,
}

/// Inputs (client) and snapshots (host) go out at this fixed rate, whatever the frame rate.
const LAN_SEND_DT: f32 = 1.0 / 60.0;
const LAN_COUNTDOWN: f64 = 3.0;
const LAN_HIST: usize = 128;
const CHAR_DESC: [&str; 4] = [
    "bass: perfect returns slam the other player back",
    "snare: perfect returns split the orb in two",
    "hat: perfect returns zig-zag every half beat",
    "lead: perfect returns curve and leave a laser trail",
];

struct LanMatch {
    track: Arc<Track>,
    an: Arc<Analysis>,
    events: Vec<LevelEvent>,
    beats: Vec<f32>,
    bars: Vec<f32>,
    duration: f32,
    cursor: usize,
    /// Song time of this frame (negative during the countdown).
    t: f32,
    /// Local `net::now()` at which both PCs start the song.
    start_local: f64,
    audio_on: bool,
    /// Host: the authoritative state. Client: the latest snapshot.
    rally: Rally,
    /// Client: the snapshot before `rally`, and the interpolated + predicted state that is drawn.
    prev: Rally,
    view: Rally,
    prev_at: f64,
    cur_at: f64,
    snaps: u32,
    bad_snaps: u32,
    snap_t: f32,
    buf: Vec<u8>,
    send_acc: f32,
    // host: the client's latest input
    peer_in: RallyInput,
    peer_swings: u8,
    peer_pulse: bool,
    peer_seq: u32,
    // client: own inputs, kept until the host acknowledges them
    seq: u32,
    swings: u8,
    down: bool,
    ack: u32,
    hist: VecDeque<(u32, RallyInput, f32)>,
    fx_seen: f32,
    score_seen: [u32; 2],
    confirm: bool,
    traffic0: (u64, u64),
    wall0: f64,
}

/// Name of the inbound firewall rule Heartline offers to add for LAN Rally.
const FIREWALL_RULE: &str = "Heartline LAN Rally";

/// Ask Windows (read-only, no elevation) whether the Heartline firewall rule exists. The answer
/// arrives on the channel after `delay` seconds plus however long `netsh` takes.
fn firewall_check(delay: f32) -> Receiver<bool> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs_f32(delay));
        let ok = std::process::Command::new("netsh")
            .args(["advfirewall", "firewall", "show", "rule", &format!("name={FIREWALL_RULE}")])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        let _ = tx.send(ok);
    });
    rx
}

/// Ask Windows to add an inbound allow rule for this exe on Private networks. This starts an
/// elevated `netsh`, so Windows shows its administrator prompt and nothing changes unless the
/// player approves it there. Only ever called from the button on the LAN screen.
fn firewall_allow() {
    let Ok(exe) = std::env::current_exe() else { return };
    let rule = format!(
        "advfirewall firewall add rule name=\"{FIREWALL_RULE}\" dir=in action=allow program=\"{}\" profile=private enable=yes",
        exe.display()
    );
    let ps = format!("Start-Process netsh -Verb RunAs -WindowStyle Hidden -ArgumentList '{}'", rule.replace('\'', "''"));
    let _ = std::process::Command::new("powershell").args(["-NoProfile", "-Command", &ps]).spawn();
}

struct Lan {
    /// Pending answer of `firewall_check`, and the last answer.
    firewall: Option<Receiver<bool>>,
    firewall_ok: Option<bool>,
    host: bool,
    stage: LanStage,
    net: Option<net::Net>,
    disc: Option<net::Discovery>,
    sel: usize,
    ip: String,
    my_ip: String,
    status: String,
    peer: String,
    connected: bool,
    my_char: u8,
    peer_char: u8,
    my_ready: bool,
    peer_ready: bool,
    /// Host: the chosen song. Client: the received file (or built-in theme path).
    song: Option<PathBuf>,
    song_title: String,
    /// Host: the song last sent to the client, and whether it has fully gone out.
    sent: Option<PathBuf>,
    song_sent: bool,
    xfer: (u64, u64),
    loader: Option<(PathBuf, Loader)>,
    loaded: Option<(PathBuf, Loaded)>,
    peer_loaded: bool,
    told_loaded: bool,
    /// (offset = host clock - local clock, best RTT, samples)
    clock: (f64, f64, u32),
    dialog: Option<Receiver<Vec<PathBuf>>>,
    m: Option<Box<LanMatch>>,
    result: [u32; 2],
    result_t: f32,
    log_path: Option<PathBuf>,
    log: Vec<String>,
    /// Offline practice (`--rally-local`): no network, the peer is the bot.
    local: bool,
}

impl Lan {
    fn new(log_dir: Option<&Path>) -> Lan {
        Lan {
            // Automated runs skip the firewall question.
            firewall: if log_dir.is_none() { Some(firewall_check(0.0)) } else { None },
            firewall_ok: if log_dir.is_none() { None } else { Some(true) },
            local: false,
            host: false,
            stage: LanStage::Choose,
            net: None,
            disc: None,
            sel: 0,
            ip: String::new(),
            my_ip: String::new(),
            status: String::new(),
            peer: String::new(),
            connected: false,
            my_char: 0,
            peer_char: 0,
            my_ready: false,
            peer_ready: false,
            song: None,
            song_title: String::new(),
            sent: None,
            song_sent: false,
            xfer: (0, 0),
            loader: None,
            loaded: None,
            peer_loaded: false,
            told_loaded: false,
            clock: (0.0, 0.0, 0),
            dialog: None,
            m: None,
            result: [0; 2],
            result_t: 0.0,
            log_path: log_dir.map(|d| d.join("lan.log")),
            log: Vec::new(),
        }
    }

    /// Record a state change (selftest runs write them to `lan.log`).
    fn note(&mut self, s: String) {
        if let Some(p) = &self.log_path {
            self.log.push(format!("{:9.3} {} {}", net::now(), if self.host { "host" } else { "client" }, s));
            let _ = std::fs::write(p, self.log.join("\n") + "\n");
        }
    }

    fn send(&self, m: &net::Msg) {
        if let Some(n) = &self.net {
            n.send(m);
        }
    }

    fn send_lobby(&self) {
        self.send(&net::Msg::Lobby { ch: self.my_char, ready: self.my_ready });
    }

    fn in_match(&self) -> bool {
        matches!(self.stage, LanStage::Countdown | LanStage::Match) && self.m.is_some()
    }

    fn set_song(&mut self, p: Option<PathBuf>) {
        self.song_title = p.as_deref().map(track_title).unwrap_or_default();
        self.song = p;
    }

    fn is_loaded(&self) -> bool {
        matches!((&self.loaded, &self.song), (Some((a, _)), Some(b)) if a == b)
    }
}

/// 0..1 build-up ramp before the next Intense / Drop section (same window as the renderer).
fn build_ramp(an: &Analysis, t: f32) -> f32 {
    let beat = if an.bpm > 1.0 { 60.0 / an.bpm } else { 0.5 };
    let win = (16.0 * beat).clamp(3.0, 10.0);
    (1.0 - next_build(an, t) / win).clamp(0.0, 1.0)
}

/// Local player's Rally input from keyboard / mouse. `aim` is a direction from the avatar.
fn rally_input(rend: &Renderer, me_pos: Vec2) -> RallyInput {
    let k = |a: KeyCode, b: KeyCode| (is_key_down(a) || is_key_down(b)) as i32 as f32;
    let mv = vec2(k(KeyCode::D, KeyCode::Right) - k(KeyCode::A, KeyCode::Left), k(KeyCode::S, KeyCode::Down) - k(KeyCode::W, KeyCode::Up));
    let (mx, my) = mouse_position();
    let aim = rend.view.to_world(vec2(mx, my)) - me_pos;
    RallyInput { mv, aim, swing: is_mouse_button_down(MouseButton::Left) || is_key_down(KeyCode::Space) }
}

impl App {
    fn lan_open(&mut self) {
        self.lan = Some(Box::new(Lan::new(self.args.selftest.as_deref())));
        self.screen = Screen::Lan;
    }

    /// Leave the LAN session (dropping `Net` tells the other side) and go back to the menu.
    fn lan_leave(&mut self, why: Option<String>) {
        if let Some(mut lan) = self.lan.take() {
            lan.note(format!("leave: {}", why.as_deref().unwrap_or("by choice")));
        }
        self.to_menu();
        if let Some(w) = why {
            self.say(w);
        }
    }

    fn lan_start_host(&mut self, lan: &mut Lan, song: Option<PathBuf>) {
        match net::Net::host(&net::local_name(), net::TCP_PORT, true) {
            Ok(n) => {
                let port = n.port;
                lan.host = true;
                lan.net = Some(n);
                lan.disc = None;
                lan.stage = LanStage::Lobby;
                lan.my_ip = net::local_ip().map(|i| i.to_string()).unwrap_or_else(|| "unknown".into());
                let song = song.or_else(|| self.juke.list.get(self.juke.idx).cloned());
                lan.set_song(song);
                lan.note(format!("hosting port={port} song='{}'", lan.song_title));
            }
            Err(e) => {
                lan.note(format!("host failed: {e}"));
                self.say(e);
            }
        }
    }

    fn lan_start_join(&mut self, lan: &mut Lan, addr: std::net::SocketAddr) {
        lan.host = false;
        lan.net = Some(net::Net::join(addr, &net::local_name()));
        lan.disc = None;
        lan.stage = LanStage::Lobby;
        lan.note(format!("joining {addr}"));
    }

    /// Command-line LAN start (`--lan-host FILE` / `--lan-join IP`).
    fn lan_cli(&mut self) {
        self.lan_open();
        let Some(mut lan) = self.lan.take() else { return };
        if let Some(f) = self.args.rally_local.clone() {
            lan.host = true;
            lan.local = true;
            lan.stage = LanStage::Lobby;
            lan.set_song(Some(f));
            lan.note("local practice vs bot".into());
        } else if let Some(f) = self.args.lan_host.clone() {
            self.lan_start_host(&mut lan, Some(f));
        } else if let Some(ip) = self.args.lan_join.clone() {
            match net::parse_addr(&ip) {
                Some(a) => self.lan_start_join(&mut lan, a),
                None => self.say(format!("not an IP address: {ip}")),
            }
        }
        self.lan = Some(lan);
    }

    /// Make sure `path` is analysed (reusing the menu's copy when it is the same track).
    fn lan_load(&mut self, lan: &mut Lan, path: PathBuf) {
        if lan.loaded.as_ref().map(|l| &l.0) == Some(&path) || lan.loader.as_ref().map(|l| &l.0) == Some(&path) {
            return;
        }
        if let Some((p, l)) = &self.juke.loaded {
            if *p == path {
                let level = level::generate(&l.an, self.args.seed, self.diff());
                lan.loaded = Some((path, Loaded { track: l.track.clone(), an: l.an.clone(), level }));
                lan.loader = None;
                return;
            }
        }
        lan.loader = Some((path.clone(), spawn_loader(0, path, self.args.seed, self.diff())));
    }

    /// Per-frame LAN housekeeping on every LAN screen: network events, song loading, start.
    fn lan_poll(&mut self, dt: f32) {
        let Some(mut lan) = self.lan.take() else { return };
        let mut leave: Option<String> = None;
        if let Some(d) = &mut lan.disc {
            d.poll(dt);
        }
        while leave.is_none() {
            let Some(ev) = lan.net.as_ref().and_then(|n| n.poll()) else { break };
            match ev {
                net::Event::Status(s) => lan.status = s,
                net::Event::Connected(name) => {
                    lan.connected = true;
                    lan.status = format!("connected to {name}");
                    lan.note(format!("connected peer={name}"));
                    lan.peer = name;
                    if self.args.rally_bot {
                        lan.my_char = if lan.host { 1 } else { 3 };
                        lan.my_ready = true;
                        lan.note(format!("ready char={}", lan.my_char));
                    }
                    lan.send_lobby();
                }
                net::Event::Msg(m) => self.lan_msg(&mut lan, m, &mut leave),
                net::Event::SongProgress(d, t) => lan.xfer = (d, t),
                net::Event::SongSaved { path, title, theme } => {
                    let p = path.or_else(|| ((theme as usize) < synth::THEME_COUNT).then(|| theme_path(theme as usize)));
                    match p {
                        Some(p) => {
                            lan.note(format!("song received '{title}'"));
                            lan.song_title = title;
                            lan.song = Some(p.clone());
                            lan.loaded = None;
                            lan.loader = None;
                            lan.told_loaded = false;
                            self.lan_load(&mut lan, p);
                        }
                        None => leave = Some("the host picked a song this version does not have".into()),
                    }
                }
                net::Event::SongSent => {
                    lan.song_sent = true;
                    lan.note(format!("song sent bytes={}", lan.xfer.1));
                }
                net::Event::Clock { offset, rtt, samples } => lan.clock = (offset, rtt, samples),
                net::Event::Disconnected(why) => leave = Some(why),
            }
        }
        // song analysis
        let mut done = None;
        if let Some((p, l)) = &lan.loader {
            if let Ok(r) = l.rx.try_recv() {
                done = Some((p.clone(), r));
            }
        }
        if let Some((p, r)) = done {
            lan.loader = None;
            match r {
                Ok(l) => {
                    lan.note(format!("song analysed beats={} bpm={:.1}", l.an.beats.len(), l.an.bpm));
                    lan.loaded = Some((p, l));
                }
                Err(e) if lan.host => {
                    self.say(format!("cannot read that song: {e}"));
                    lan.set_song(None);
                    lan.my_ready = false;
                    lan.send_lobby();
                }
                Err(e) => leave = Some(format!("could not read the host's song: {e}")),
            }
        }
        if let Some(why) = leave {
            self.lan = Some(lan);
            self.lan_leave(Some(why));
            return;
        }
        if lan.host {
            if let Some(s) = lan.song.clone() {
                self.lan_load(&mut lan, s);
            }
            // The song goes out once the host locks in with Ready.
            if lan.connected && lan.my_ready && lan.sent != lan.song {
                if let (Some(s), Some(n)) = (lan.song.clone(), &lan.net) {
                    match theme_index(&s) {
                        Some(i) => n.send_song(None, &lan.song_title, i as u8, ""),
                        None => {
                            let ext = s.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
                            n.send_song(Some(s.clone()), &lan.song_title, net::NO_THEME, &ext);
                        }
                    }
                    lan.sent = Some(s);
                    lan.song_sent = false;
                    lan.peer_loaded = false;
                    lan.xfer = (0, 0);
                    lan.note(format!("sending song '{}'", lan.song_title));
                }
            }
            if lan.local && lan.stage == LanStage::Lobby && lan.is_loaded() {
                let seed = if self.args.seed != 0 { self.args.seed } else { 1 };
                let chars = [lan.my_char, 2];
                self.lan_begin(&mut lan, net::now() + LAN_COUNTDOWN, seed, chars);
            }
            let go = lan.stage == LanStage::Lobby
                && lan.connected
                && lan.my_ready
                && lan.peer_ready
                && lan.song_sent
                && lan.peer_loaded
                && lan.sent == lan.song
                && lan.is_loaded();
            if go {
                let seed = if self.args.seed != 0 {
                    self.args.seed
                } else {
                    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(9)
                };
                let chars = [lan.my_char, lan.peer_char];
                let host_time = net::now() + LAN_COUNTDOWN;
                lan.send(&net::Msg::Start { host_time, seed, chars });
                lan.note(format!("start host_time={host_time:.3} seed={seed} chars={chars:?}"));
                self.lan_begin(&mut lan, host_time, seed, chars);
            }
        } else if !lan.told_loaded && lan.is_loaded() && lan.clock.2 >= net::SYNC_SAMPLES as u32 {
            lan.send(&net::Msg::Loaded);
            lan.told_loaded = true;
            lan.note(format!("loaded; clock offset={:.4}s rtt={:.2}ms", lan.clock.0, lan.clock.1 * 1000.0));
        }
        if lan.stage == LanStage::Results {
            lan.result_t += dt;
            if self.args.rally_bot && self.args.selftest.is_none() && lan.result_t > 5.0 {
                lan_to_lobby(&mut lan, true);
            }
        }
        self.lan = Some(lan);
    }

    fn lan_msg(&mut self, lan: &mut Lan, m: net::Msg, leave: &mut Option<String>) {
        use net::Msg;
        match m {
            Msg::Lobby { ch, ready } => {
                let ch = ch.min(Character::ALL.len() as u8 - 1);
                if (ch, ready) != (lan.peer_char, lan.peer_ready) {
                    lan.note(format!("peer char={ch} ready={ready}"));
                }
                lan.peer_char = ch;
                lan.peer_ready = ready;
            }
            Msg::Loaded if lan.host => {
                lan.peer_loaded = true;
                lan.note("peer loaded".into());
            }
            Msg::Start { host_time, seed, chars } if !lan.host => {
                if !lan.is_loaded() {
                    *leave = Some("the host started before the song was ready".into());
                    return;
                }
                let local = host_time - lan.clock.0;
                lan.note(format!(
                    "start host_time={host_time:.3} local={local:.3} in={:.3}s offset={:.4} rtt={:.2}ms seed={seed} chars={chars:?}",
                    local - net::now(),
                    lan.clock.0,
                    lan.clock.1 * 1000.0
                ));
                self.lan_begin(lan, local, seed, chars);
            }
            Msg::Input(i) if lan.host => {
                if let Some(m) = &mut lan.m {
                    if i.seq > m.peer_seq {
                        m.peer_seq = i.seq;
                        m.peer_in = RallyInput { mv: vec2(i.mv[0], i.mv[1]), aim: vec2(i.aim[0], i.aim[1]), swing: false };
                        if i.swings != m.peer_swings {
                            m.peer_swings = i.swings;
                            m.peer_pulse = true;
                        }
                    }
                }
            }
            Msg::Snap { t, ack, data } if !lan.host => {
                if let Some(m) = &mut lan.m {
                    std::mem::swap(&mut m.prev, &mut m.rally);
                    if m.rally.decode(&data) {
                        m.prev_at = m.cur_at;
                        m.cur_at = net::now();
                        m.snaps += 1;
                        m.snap_t = t;
                        m.ack = ack;
                        while m.hist.front().is_some_and(|h| h.0 <= ack) {
                            m.hist.pop_front();
                        }
                        if m.snaps == 1 {
                            m.prev.clone_from(&m.rally);
                        }
                    } else {
                        std::mem::swap(&mut m.prev, &mut m.rally);
                        m.bad_snaps += 1;
                    }
                }
                if let Some(n) = &lan.net {
                    n.recycle(data);
                }
            }
            Msg::End { score } if !lan.host => self.lan_end(lan, score),
            Msg::Snap { data, .. } => {
                if let Some(n) = &lan.net {
                    n.recycle(data);
                }
            }
            _ => {}
        }
    }

    /// Countdown: both PCs start the song at `start_local` on their own clock.
    fn lan_begin(&mut self, lan: &mut Lan, start_local: f64, seed: u64, chars: [u8; 2]) {
        let Some((_, l)) = &lan.loaded else { return };
        let ch = |i: u8| Character::from_index(i).unwrap_or(Character::Sub);
        let mut rally = Rally::new(seed, [ch(chars[0]), ch(chars[1])]);
        // The practice bot is as sharp as the chosen difficulty tier.
        rally.bot_miss = [0.40, 0.32, 0.22, 0.16, 0.11, 0.07, 0.04][self.diff.min(6)];
        let an = l.an.clone();
        let bars: Vec<f32> = if an.beats.is_empty() { Vec::new() } else { an.beats[an.downbeat.min(an.beats.len() - 1)..].iter().step_by(4).copied().collect() };
        let beat_len = if l.level.bpm > 1.0 { 60.0 / l.level.bpm } else { 0.5 };
        let traffic0 = lan.net.as_ref().map(|n| n.traffic()).unwrap_or_default();
        let m = LanMatch {
            track: l.track.clone(),
            events: l.level.events.clone(),
            beats: an.beats.clone(),
            bars,
            duration: an.duration.max(1.0),
            an,
            cursor: 0,
            t: (net::now() - start_local) as f32,
            start_local,
            audio_on: false,
            prev: rally.clone(),
            view: rally.clone(),
            rally,
            prev_at: 0.0,
            cur_at: 0.0,
            snaps: 0,
            bad_snaps: 0,
            snap_t: 0.0,
            buf: Vec::with_capacity(1024),
            send_acc: 0.0,
            peer_in: RallyInput::default(),
            peer_swings: 0,
            peer_pulse: false,
            peer_seq: 0,
            seq: 0,
            swings: 0,
            down: false,
            ack: 0,
            hist: VecDeque::with_capacity(LAN_HIST),
            fx_seen: -1.0e9,
            score_seen: [0; 2],
            confirm: false,
            traffic0,
            wall0: net::now(),
        };
        lan.m = Some(Box::new(m));
        lan.stage = LanStage::Countdown;
        self.juke.playing = false;
        self.audio.fade_out(0.4);
        self.world = World::new(self.diff(), true);
        self.world.beat_len = beat_len;
    }

    fn lan_end(&mut self, lan: &mut Lan, score: [u32; 2]) {
        lan.result = score;
        lan.result_t = 0.0;
        lan.stage = LanStage::Results;
        lan.my_ready = false;
        lan.peer_ready = false;
        self.audio.fade_out(1.0);
        if let Some(m) = lan.m.take() {
            let secs = (net::now() - m.wall0).max(0.001);
            let (s0, r0) = m.traffic0;
            let (s1, r1) = lan.net.as_ref().map(|n| n.traffic()).unwrap_or((s0, r0));
            lan.note(format!(
                "match over t={:.2} snapshot_score={}-{} snaps={} bad_snaps={} up={:.1}KB/s down={:.1}KB/s",
                m.t,
                m.rally.score()[0],
                m.rally.score()[1],
                m.snaps,
                m.bad_snaps,
                (s1 - s0) as f64 / secs / 1024.0,
                (r1 - r0) as f64 / secs / 1024.0
            ));
        }
        lan.note(format!("final score {}-{}", score[0], score[1]));
        lan.send_lobby();
    }

    /// Countdown and match: host steps Rally, client predicts + interpolates. Returns progress.
    fn lan_step(&mut self, dt: f32) -> f32 {
        let Some(mut lan) = self.lan.take() else { return 0.0 };
        let host = lan.host;
        let me = if host { 0 } else { 1 };
        let now = net::now();
        let mut notes: [Option<String>; 2] = [None, None];
        let mut end_score = None;
        let progress;
        {
            let Some(mb) = lan.m.as_mut() else {
                self.lan = Some(lan);
                return 0.0;
            };
            let m: &mut LanMatch = mb;
            if !m.audio_on && now >= m.start_local {
                let late = now - m.start_local;
                self.audio.set_volume(if self.args.mute { 0.0 } else { self.volume });
                self.audio.play_ex(m.track.clone(), late, false, 0.0);
                m.audio_on = true;
                notes[0] = Some(format!("audio start late={:.2}ms", late * 1000.0));
            }
            let t = if m.audio_on {
                let a = self.audio.song_time() as f32 - self.offset_ms / 1000.0;
                a.max(m.t).min(m.t + 0.25)
            } else {
                (now - m.start_local) as f32
            };
            m.t = t;
            let tt = t.max(0.0);
            let playing = m.audio_on && t >= 0.0;
            let section = m.an.section_at(tt).map(|s| s.kind).unwrap_or(SectionKind::Normal);
            let music = RallyMusic { beats: &m.beats, bars: &m.bars, section, next_build: build_ramp(&m.an, tt) };
            let mine = if host { &m.rally } else { &m.view };
            let mut inp = if self.args.rally_bot { mine.bot_input(me, t, &music) } else { rally_input(&self.rend, mine.players[me].pos) };
            if !playing || m.confirm {
                inp = RallyInput { aim: inp.aim, ..RallyInput::default() };
            }
            if host {
                if playing {
                    let mut pin = m.peer_in;
                    pin.swing = m.peer_pulse;
                    m.peer_pulse = false;
                    if lan.local {
                        pin = m.rally.bot_input(1, t, &music);
                    }
                    m.rally.step(dt, t, [inp, pin], &music);
                }
                m.send_acc += dt;
                let over = playing && (self.audio.finished() || t >= m.duration - 0.05);
                let test_over = self.args.selftest.is_some() && self.wall >= self.args.secs - 4.0;
                if m.send_acc >= LAN_SEND_DT || over || test_over {
                    m.send_acc = (m.send_acc - LAN_SEND_DT).clamp(0.0, LAN_SEND_DT);
                    m.buf.clear();
                    m.rally.encode(&mut m.buf);
                    if let Some(n) = &lan.net {
                        n.send_snap(t, m.peer_seq, &m.buf);
                    }
                }
                if over || test_over {
                    let score = m.rally.score();
                    if let Some(n) = &lan.net {
                        n.send(&net::Msg::End { score });
                    }
                    end_score = Some(score);
                }
            } else {
                // Swing presses are counted, so one is never lost between 60 Hz input packets.
                if inp.swing && !m.down {
                    m.swings = m.swings.wrapping_add(1);
                }
                m.down = inp.swing;
                m.send_acc += dt;
                if m.send_acc >= LAN_SEND_DT {
                    m.send_acc = (m.send_acc - LAN_SEND_DT).clamp(0.0, LAN_SEND_DT);
                    let q = |v: f32| (v.clamp(-1.0, 1.0) * 127.0).round() / 127.0;
                    let sent = RallyInput { mv: vec2(q(inp.mv.x), q(inp.mv.y)), aim: inp.aim, swing: false };
                    m.seq += 1;
                    if let Some(n) = &lan.net {
                        n.send(&net::Msg::Input(net::NetInput { seq: m.seq, mv: [sent.mv.x, sent.mv.y], aim: [sent.aim.x, sent.aim.y], swings: m.swings }));
                    }
                    if m.hist.len() >= LAN_HIST {
                        m.hist.pop_front();
                    }
                    m.hist.push_back((m.seq, sent, LAN_SEND_DT));
                }
                // Draw one snapshot behind (interpolated), with our own avatar predicted.
                if m.snaps >= 2 {
                    let span = (m.cur_at - m.prev_at).clamp(0.004, 0.1);
                    m.view.interpolate(&m.prev, &m.rally, ((now - m.cur_at) / span) as f32);
                } else {
                    m.view.clone_from(&m.rally);
                }
                if playing && m.snaps > 0 {
                    m.view.players[me] = m.rally.players[me];
                    for (s, i, d) in m.hist.iter() {
                        if *s > m.ack {
                            m.view.predict_move(me, *d, i);
                        }
                    }
                    m.view.predict_move(me, m.send_acc, &inp);
                }
            }
            // Sounds and screen kicks for new Rally events.
            let shown = if host { &m.rally } else { &m.view };
            let mut newest = m.fx_seen;
            for f in shown.events() {
                if f.t <= m.fx_seen {
                    continue;
                }
                newest = newest.max(f.t);
                let pan = (f.pos.x / level::HALF_W).clamp(-1.0, 1.0);
                match f.kind {
                    FxKind::Hit => self.audio.sfx(Sfx::NoteShot, 0.6, pan),
                    FxKind::Perfect => self.audio.sfx(Sfx::Shard, 0.9, pan),
                    FxKind::Slam => {
                        self.audio.sfx(Sfx::Slam, 1.0, pan);
                        self.world.shake = self.world.shake.max(0.6);
                    }
                    FxKind::Split => self.audio.sfx(Sfx::Kill, 0.7, pan),
                    FxKind::Score => {
                        self.audio.sfx(Sfx::HeartBreak, 0.9, pan);
                        self.world.shake = self.world.shake.max(0.8);
                        self.world.flash = self.world.flash.max(0.5);
                    }
                    FxKind::Serve => self.audio.sfx(Sfx::UiMove, 0.4, pan),
                    FxKind::Bounce => {}
                }
            }
            m.fx_seen = newest;
            let sc = m.rally.score();
            if sc != m.score_seen {
                m.score_seen = sc;
                notes[1] = Some(if host {
                    format!("point t={:.3} score {}-{}", m.rally.t, sc[0], sc[1])
                } else {
                    format!("point host_t={:.3} local_t={:.3} score {}-{}", m.snap_t, t, sc[0], sc[1])
                });
            }
            // The visual layer reacts to the music exactly like the menu: beats, hits, sections.
            while m.cursor < m.events.len() && m.events[m.cursor].at() <= tt {
                let e = m.events[m.cursor];
                m.cursor += 1;
                if matches!(e.ev, Ev::Beat { .. } | Ev::Hit { .. } | Ev::Section { .. }) {
                    self.world.apply(&e);
                }
            }
            self.world.t = tt;
            let feat = if playing { feat_at(&m.an, tt) } else { Feat::default() };
            let bp = beat_pos(&m.beats, tt);
            self.world.update(dt, tt, &Input::default(), bp, &feat, m.an.intensity_at(tt), false);
            self.world.player.pos = Vec2::ZERO;
            if m.audio_on && lan.stage == LanStage::Countdown {
                lan.stage = LanStage::Match;
            }
            progress = tt / m.duration;
        }
        for n in notes.iter_mut() {
            if let Some(s) = n.take() {
                lan.note(s);
            }
        }
        if let Some(sc) = end_score {
            self.lan_end(&mut lan, sc);
        }
        self.lan = Some(lan);
        progress
    }

    fn lan_draw(&mut self, dt: f32) {
        let Some(lan) = self.lan.as_ref() else { return };
        let Some(m) = lan.m.as_ref() else { return };
        let me = if lan.host { 0 } else { 1 };
        self.rend.fx = VISUALS[self.visuals].0;
        let t = m.t.max(0.0);
        if m.audio_on {
            self.rend.feed_signal(Some((&m.track.samples, m.track.sr)), t, next_build(&m.an, t));
        } else {
            self.rend.feed_signal(None, t, f32::INFINITY);
        }
        let r = if lan.host { &m.rally } else { &m.view };
        self.rend.draw_rally(r, &self.world, me, dt);
    }

    /// LAN screens (drawn over the scene) and their input.
    fn lan_ui(&mut self, u: f32, sw: f32, sh: f32) {
        let Some(mut lan) = self.lan.take() else {
            self.screen = Screen::Menu;
            return;
        };
        let pal = self.rend.pal;
        let x = 110.0 * u;
        let esc = is_key_pressed(KeyCode::Escape);
        let enter = is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter);
        let mut leave: Option<Option<String>> = None;
        let soft = Color::new(1.0, 1.0, 1.0, 0.7);
        match lan.stage {
            LanStage::Choose => {
                dim(0.6);
                render::vtext("LAN RALLY", x, 130.0 * u, 64.0 * u, WHITE, -1);
                text_left("A beat-locked paddle duel for two PCs on the same network.", x, 175.0 * u, 24.0 * u, pal.primary);
                let song = self.juke.list.get(self.juke.idx).map(|p| track_title(p)).unwrap_or_default();
                let items = vec![
                    format!("HOST A MATCH   ({song})"),
                    "JOIN A MATCH".to_string(),
                    format!("PRACTICE VS BOT   ({song})"),
                    "BACK".to_string(),
                ];
                let act = menu_list(&items, &mut lan.sel, x, 260.0 * u, u, &self.rend, &self.audio);
                let fy = sh - 150.0 * u;
                // Hosting needs an inbound firewall rule. Heartline only adds one when asked to
                // here, and Windows shows its own permission prompt before anything changes.
                let fw = lan.firewall.as_ref().and_then(|rx| rx.try_recv().ok());
                if let Some(ok) = fw {
                    lan.firewall_ok = Some(ok);
                    lan.firewall = None;
                }
                let (fmsg, fcol) = match lan.firewall_ok {
                    Some(true) => ("Windows Firewall: Heartline is allowed on private networks.", pal.primary),
                    Some(false) => ("Windows Firewall: no rule for Heartline yet, so other PCs cannot reach a match you host.", pal.accent),
                    None => ("Windows Firewall: checking...", soft),
                };
                text_left(fmsg, x, fy - 62.0 * u, 22.0 * u, fcol);
                if lan.firewall_ok == Some(false) {
                    let (hv, click) = ui::button(x, fy - 48.0 * u, 470.0 * u, 44.0 * u, None, "ALLOW HEARTLINE (Windows will ask)", false, false, u, &pal);
                    if hv {
                        text_left("Adds one inbound rule for this exe on Private networks. Windows asks for administrator approval first.", x + 490.0 * u, fy - 20.0 * u, 20.0 * u, soft);
                    }
                    if click {
                        firewall_allow();
                        lan.firewall_ok = None;
                        lan.firewall = Some(firewall_check(3.0));
                    }
                }
                text_left("Both PCs must be on the same network. Ports: TCP 47820 (game), UDP 47821 (finding hosts).", x, fy + 28.0 * u, 22.0 * u, soft);
                text_left("If hosts are not found, type the host's IP on the JOIN screen.", x, fy + 56.0 * u, 22.0 * u, soft);
                match act {
                    Some(0) => self.lan_start_host(&mut lan, None),
                    Some(1) => {
                        lan.stage = LanStage::Join;
                        lan.sel = 0;
                        lan.disc = Some(net::Discovery::start());
                    }
                    Some(2) => {
                        // Offline practice against the bot on the menu's current track.
                        let cur = self.juke.list.get(self.juke.idx).cloned();
                        if cur.is_some() {
                            lan.host = true;
                            lan.local = true;
                            lan.stage = LanStage::Lobby;
                            lan.set_song(cur);
                            lan.note("local practice vs bot".into());
                        } else {
                            self.say("no track selected: pick one in the main menu first");
                        }
                    }
                    Some(3) => leave = Some(None),
                    _ => {}
                }
                if esc {
                    leave = Some(None);
                }
            }
            LanStage::Join => {
                dim(0.6);
                render::vtext("JOIN A MATCH", x, 130.0 * u, 64.0 * u, WHITE, -1);
                let (hosts, err) = match &lan.disc {
                    Some(d) => (d.hosts.clone(), d.error.clone()),
                    None => (Vec::new(), None),
                };
                let msg = err.unwrap_or_else(|| if hosts.is_empty() { "looking for hosts on your network...".into() } else { "hosts on your network:".into() });
                text_left(&msg, x, 175.0 * u, 24.0 * u, pal.primary);
                while let Some(c) = get_char_pressed() {
                    if (c.is_ascii_digit() || c == '.' || c == ':') && lan.ip.len() < 21 {
                        lan.ip.push(c);
                    }
                }
                if is_key_pressed(KeyCode::Backspace) {
                    lan.ip.pop();
                }
                let mut items: Vec<String> = hosts
                    .iter()
                    .map(|h| format!("{}   {}{}", h.name, h.addr.ip(), if h.compatible { "" } else { "   (different version)" }))
                    .collect();
                items.push(format!("TYPE AN IP:   {}_", lan.ip));
                items.push("BACK".to_string());
                lan.sel = lan.sel.min(items.len() - 1);
                let act = menu_list(&items, &mut lan.sel, x, 230.0 * u, u, &self.rend, &self.audio);
                text_left("type the host's IP (shown on its lobby screen) and press ENTER on that row", x, sh - 90.0 * u, 22.0 * u, soft);
                let nh = hosts.len();
                match act {
                    Some(i) if i < nh => {
                        if hosts[i].compatible {
                            self.lan_start_join(&mut lan, hosts[i].addr);
                        } else {
                            self.say("that host is on a different version of Heartline");
                        }
                    }
                    Some(i) if i == nh => match net::parse_addr(&lan.ip) {
                        Some(a) => self.lan_start_join(&mut lan, a),
                        None => self.say("type an IP like 192.168.1.20"),
                    },
                    Some(_) => {
                        lan.stage = LanStage::Choose;
                        lan.disc = None;
                    }
                    None => {}
                }
                if esc {
                    lan.stage = LanStage::Choose;
                    lan.disc = None;
                }
            }
            LanStage::Lobby => {
                dim(0.62);
                render::vtext(if lan.host { "HOSTING" } else { "LOBBY" }, x, 130.0 * u, 64.0 * u, WHITE, -1);
                let line = if lan.host && !lan.connected {
                    format!("your IP  {}   -   waiting for a player to join", lan.my_ip)
                } else if lan.connected {
                    format!("{}   -   ping {:.1} ms", lan.status, lan.clock.1 * 1000.0)
                } else {
                    lan.status.clone()
                };
                text_left(&line, x, 175.0 * u, 24.0 * u, pal.primary);
                // player cards
                let peer_name = if lan.connected { lan.peer.clone() } else { "waiting...".to_string() };
                let cards = [("YOU", net::local_name(), lan.my_char, lan.my_ready, true), ("THEM", peer_name, lan.peer_char, lan.peer_ready, lan.connected)];
                for (k, (who, name, ch, ready, on)) in cards.iter().enumerate() {
                    let cx = x + k as f32 * 600.0 * u;
                    let cy = 220.0 * u;
                    ui::rrect(cx, cy, 560.0 * u, 170.0 * u, 8.0 * u, Color::new(0.0, 0.0, 0.05, 0.5));
                    ui::rrect_lines(cx, cy, 560.0 * u, 170.0 * u, 8.0 * u, 2.0 * u, if *ready { pal.accent } else { Color::new(1.0, 1.0, 1.0, 0.25) });
                    text_left(&format!("{who}   {name}"), cx + 20.0 * u, cy + 36.0 * u, 22.0 * u, soft);
                    if *on {
                        let c = Character::from_index(*ch).unwrap_or(Character::Sub);
                        let label = if k == 0 && !*ready { format!("<  {}  >", c.name()) } else { c.name().to_string() };
                        render::vtext(&label, cx + 20.0 * u, cy + 92.0 * u, 44.0 * u, WHITE, -1);
                        text_left(CHAR_DESC[(*ch as usize).min(3)], cx + 20.0 * u, cy + 126.0 * u, 20.0 * u, pal.primary);
                    }
                    text_left(if *ready { "READY" } else { "not ready" }, cx + 20.0 * u, cy + 156.0 * u, 22.0 * u, if *ready { pal.accent } else { soft });
                }
                // song + progress
                let mut y = 440.0 * u;
                let title = if lan.song_title.is_empty() { "(none)".to_string() } else { lan.song_title.clone() };
                text_left(&format!("SONG   {title}"), x, y, 28.0 * u, WHITE);
                if lan.host && !lan.my_ready {
                    text_left("Q / E  change track      O  open a file", x + 760.0 * u, y, 22.0 * u, soft);
                }
                let bar = |y: f32, label: &str, f: f32| {
                    let w = 700.0 * u;
                    text_left(label, x, y, 22.0 * u, soft);
                    ui::rrect(x + 300.0 * u, y - 16.0 * u, w, 18.0 * u, 8.0 * u, Color::new(0.0, 0.0, 0.05, 0.6));
                    ui::rrect(x + 300.0 * u, y - 16.0 * u, w * f.clamp(0.0, 1.0), 8.0 * u, 18.0 * u, pal.secondary);
                };
                y += 44.0 * u;
                if lan.xfer.1 > 0 {
                    let f = lan.xfer.0 as f32 / lan.xfer.1 as f32;
                    let label = format!("{}  {:.1} / {:.1} MB", if lan.host { "SENDING SONG" } else { "RECEIVING SONG" }, lan.xfer.0 as f32 / 1048576.0, lan.xfer.1 as f32 / 1048576.0);
                    bar(y, &label, f);
                    y += 36.0 * u;
                }
                if let Some((_, l)) = &lan.loader {
                    let p = l.progress.load(Ordering::Relaxed) as f32 / 1000.0;
                    let label = if l.stage.load(Ordering::Relaxed) == 0 { "DECODING" } else { "READING THE MUSIC" };
                    bar(y, label, p);
                    y += 36.0 * u;
                } else if lan.is_loaded() {
                    text_left("song ready on this PC", x, y, 22.0 * u, pal.accent);
                    y += 36.0 * u;
                }
                if lan.host && lan.connected {
                    let s = if !lan.my_ready {
                        "press ENTER when ready: the song is sent to the other player"
                    } else if lan.peer_loaded {
                        "their copy is ready"
                    } else {
                        "their PC is getting the song ready..."
                    };
                    text_left(s, x, y, 22.0 * u, soft);
                }
                text_left("LEFT / RIGHT  character      ENTER  ready      ESC  leave", x, sh - 90.0 * u, 24.0 * u, pal.accent);
                // input
                if !lan.my_ready {
                    let d = is_key_pressed(KeyCode::Right) as i32 - is_key_pressed(KeyCode::Left) as i32;
                    if d != 0 {
                        lan.my_char = (lan.my_char as i32 + d).rem_euclid(Character::ALL.len() as i32) as u8;
                        self.audio.sfx(Sfx::UiMove, 0.6, 0.0);
                        lan.send_lobby();
                    }
                    if lan.host {
                        let td = is_key_pressed(KeyCode::E) as i32 - is_key_pressed(KeyCode::Q) as i32;
                        if td != 0 {
                            self.juke_next(td);
                            let p = self.juke.list.get(self.juke.idx).cloned();
                            lan.set_song(p);
                        }
                        if is_key_pressed(KeyCode::O) && lan.dialog.is_none() {
                            let (tx, rx) = channel();
                            std::thread::spawn(move || {
                                let exts: Vec<&str> = decode::AUDIO_EXTS.to_vec();
                                let r = rfd::FileDialog::new().set_title("Heartline - choose the Rally song").add_filter("Audio", &exts).pick_file();
                                let _ = tx.send(r.into_iter().collect());
                            });
                            lan.dialog = Some(rx);
                        }
                    }
                }
                let picked = lan.dialog.as_ref().and_then(|rx| match rx.try_recv() {
                    Ok(p) => Some(p),
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Vec::new()),
                    Err(_) => None,
                });
                if let Some(p) = picked {
                    lan.dialog = None;
                    if let Some(f) = p.into_iter().next() {
                        lan.set_song(Some(f));
                    }
                }
                if enter {
                    if lan.host && lan.song.is_none() {
                        self.say("pick a song first (Q / E or O)");
                    } else {
                        lan.my_ready = !lan.my_ready;
                        self.audio.sfx(Sfx::UiSelect, 0.8, 0.0);
                        lan.note(format!("ready={} char={}", lan.my_ready, lan.my_char));
                        lan.send_lobby();
                    }
                }
                if esc {
                    leave = Some(None);
                }
            }
            LanStage::Countdown | LanStage::Match => {
                let mut confirm_leave = false;
                if let Some(m) = &mut lan.m {
                    let left = m.start_local - net::now();
                    if left > 0.0 {
                        let n = ["1", "2", "3"][(left.ceil() as usize).clamp(1, 3) - 1];
                        render::vtext(n, sw * 0.5, sh * 0.45, 180.0 * u, WHITE, 0);
                    } else if left > -0.6 {
                        render::vtext("GO", sw * 0.5, sh * 0.45, 180.0 * u, pal.accent, 0);
                    }
                    if esc {
                        m.confirm = !m.confirm;
                    } else if m.confirm {
                        dim(0.55);
                        render::vtext("LEAVE MATCH?", sw * 0.5, sh * 0.4, 72.0 * u, WHITE, 0);
                        text_center("there is no pause in LAN play", sw * 0.5, sh * 0.47, 26.0 * u, soft);
                        text_center("ENTER  leave          ESC  keep playing", sw * 0.5, sh * 0.54, 28.0 * u, pal.accent);
                        confirm_leave = enter;
                    }
                }
                if confirm_leave {
                    leave = Some(None);
                }
            }
            LanStage::Results => {
                dim(0.6);
                let me = if lan.host { 0 } else { 1 };
                let (a, b) = (lan.result[me], lan.result[1 - me]);
                let (head, col) = if a > b {
                    ("YOU WIN", pal.accent)
                } else if a < b {
                    ("YOU LOSE", pal.danger)
                } else {
                    ("DRAW", WHITE)
                };
                render::vtext(head, sw * 0.5, sh * 0.3, 84.0 * u, col, 0);
                text_center(&format!("{a}   :   {b}"), sw * 0.5, sh * 0.44, 96.0 * u, WHITE);
                text_center(&format!("you vs {}", lan.peer), sw * 0.5, sh * 0.52, 28.0 * u, soft);
                text_center("ENTER  back to the lobby          ESC  leave", sw * 0.5, sh * 0.7, 28.0 * u, pal.accent);
                if enter {
                    lan_to_lobby(&mut lan, false);
                } else if esc {
                    leave = Some(None);
                }
            }
        }
        self.lan = Some(lan);
        if let Some(why) = leave {
            self.lan_leave(why);
        }
    }
}

fn lan_to_lobby(lan: &mut Lan, ready: bool) {
    lan.stage = LanStage::Lobby;
    lan.my_ready = ready;
    lan.note("back to lobby".into());
    lan.send_lobby();
}

// ---------------------------------------------------------------------------------------------
// Simple immediate-mode UI
// ---------------------------------------------------------------------------------------------

/// Draws a vertical list of buttons. Returns the index that was activated (click / Enter).
fn menu_list(items: &[String], sel: &mut usize, x: f32, y: f32, u: f32, rend: &Renderer, audio: &Audio) -> Option<usize> {
    let ys: Vec<f32> = (0..items.len()).map(|i| y + i as f32 * 54.0 * u).collect();
    menu_list_at(items, &ys, sel, x, u, rend, audio)
}

/// A vertical list of buttons with the top of every row given in `ys` (lets rows sit in groups).
fn menu_list_at(items: &[String], ys: &[f32], sel: &mut usize, x: f32, u: f32, rend: &Renderer, audio: &Audio) -> Option<usize> {
    let before = *sel;
    let (mx, my) = mouse_position();
    let h = 46.0 * u;
    let w = 520.0 * u;
    let mut activated = None;
    if is_key_pressed(KeyCode::Down) {
        *sel = (*sel + 1) % items.len();
    }
    if is_key_pressed(KeyCode::Up) {
        *sel = (*sel + items.len() - 1) % items.len();
    }
    for (i, it) in items.iter().enumerate() {
        let by = ys[i];
        let hover = mx >= x && mx <= x + w && my >= by && my <= by + h;
        if hover && mouse_delta_position() != Vec2::ZERO {
            *sel = i;
        }
        rend.draw_button(x, by, w, h, it, *sel == i, u);
        if hover && is_mouse_button_pressed(MouseButton::Left) {
            *sel = i;
            activated = Some(i);
        }
    }
    if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
        activated = Some(*sel);
    }
    if activated.is_some() {
        audio.sfx(Sfx::UiSelect, 0.8, 0.0);
    } else if *sel != before {
        audio.sfx(Sfx::UiMove, 0.6, 0.0);
    }
    activated
}

fn dim(a: f32) {
    draw_rectangle(0.0, 0.0, screen_width(), screen_height(), Color::new(0.02, 0.0, 0.06, a));
}


// ---------------------------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------------------------

async fn run(args: Args) {
    render::init_font();
    let audio = Audio::new();
    let rend = Renderer::new(args.ss);
    let diff = args.diff;
    let mut app = App {
        audio,
        rend,
        world: World::new(DIFFS[diff].0, diff == 0),
        screen: Screen::Menu,
        phase: Phase::Loading,
        queue: Vec::new(),
        endless: false,
        qi: 0,
        loader: None,
        ready: None,
        song: None,
        live: None,
        idle: 1.0,
        diff,
        volume: 0.8,
        sfx_volume: 0.5,
        music_gain: MENU_MUSIC_GAIN,
        offset_ms: 0.0,
        music_dir: None,
        juke_seq: 0,
        juke: Jukebox { list: Vec::new(), idx: 0, loaded: None, loader: None, cursor: 0, last_t: -1.0, playing: false, fails: 0, played: 0.0 },
        autofire: true,
        fullscreen: false,
        visuals: 0,
        menu_sel: 0,
        quit_armed: false,
        runner: None,
        track_sel: 0,
        tracks_back: Screen::Setup,
        was_silent: false,
        song_age: 0.0,
        load_in_arena: false,
        staged: Vec::new(),
        launch: LAUNCH_SECS,
        codex_sel: 0,
        codex_top: 0,
        show_want: Demo::None,
        show_demo: Demo::None,
        show_t: 0.0,
        show_next: 0.0,
        show_n: 0,
        diff_scroll: diff as f32,
        remix: 0,
        setup_mode: 0,
        mutators: 0,
        mut_col: 0,
        scores: load_scores(),
        run_key: String::new(),
        run_saved: false,
        run_best: false,
        base_seed: args.seed,
        setup_col: 0,
        dialog: None,
        dialog_library: false,
        esc_used: false,
        message: String::new(),
        message_t: 0.0,
        died: false,
        cap_sel: 0,
        out_sel: 0,
        delay: 3.0,
        shots_done: 0,
        frames: 0,
        wall: 0.0,
        max_bullets: 0,
        max_enemies: 0,
        log: Vec::new(),
        lan: None,
        args: args.clone(),
    };
    // ---- settings ----
    let cfg = load_settings();
    let getf = |k: &str, d: f32| -> f32 { cfg.get(k).and_then(|v| v.parse().ok()).unwrap_or(d) };
    if !args.diff_set {
        app.diff = (getf("diff", 2.0) as usize).min(DIFFS.len() - 1);
    }
    app.volume = getf("volume", 0.8).clamp(0.0, 1.0);
    app.sfx_volume = getf("sfx", 0.5).clamp(0.0, 1.0);
    app.offset_ms = getf("offset_ms", 0.0).clamp(-100.0, 300.0);
    app.autofire = getf("autofire", 1.0) > 0.5;
    app.visuals = (getf("visuals", 0.0).max(0.0) as usize).min(VISUALS.len() - 1);
    app.mutators = (getf("mutators", 0.0).max(0.0) as u32 & 31) as u8;
    if args.selftest.is_some() {
        app.setup_mode = args.mode.min(SETUP_MODES.len() - 1);
        app.mutators = args.mutators & 31;
    }
    // Borderless fullscreen is remembered between launches (never for bot or sized test runs).
    if args.selftest.is_none() && args.size.is_none() && getf("fullscreen", 0.0) > 0.5 {
        app.fullscreen = true;
        display::set_borderless(true);
    }
    app.music_dir = cfg.get("music_dir").map(PathBuf::from).filter(|p| p.is_dir() || is_playlist(p)).or_else(|| music_folder().filter(|p| p.is_dir()));
    if let Some(lib) = args.library.clone() {
        app.music_dir = Some(lib);
    }
    app.world = World::new(app.diff(), app.diff == 0);
    app.juke_rescan();
    app.audio.set_volume(if args.mute { 0.0 } else { app.volume });
    app.audio.set_sfx_volume(if args.mute { 0.0 } else { app.sfx_volume });
    app.audio.ensure_output();
    if let Some(dir) = &args.selftest {
        let _ = std::fs::create_dir_all(dir);
    }
    if args.screen == "live" {
        app.audio.refresh_devices();
        app.screen = Screen::LiveSetup;
    } else if args.screen == "setup" {
        app.screen = Screen::Setup;
        app.menu_sel = S_START;
    } else if args.screen == "results" {
        app.screen = Screen::Results;
        app.world.score = 1_234_560;
    } else if args.screen == "options" {
        app.screen = Screen::Options;
    } else if args.screen == "codex" {
        app.screen = Screen::Codex;
        app.menu_sel = args.mode.min(CODEX_TABS.len() - 1);
        // debug: `--give N` picks the entry
        app.codex_sel = args.give as usize;
        app.show_want = codex_items(app.menu_sel).get(app.codex_sel).map_or(Demo::None, |i| i.demo);
    } else if args.live {
        // Devices are enumerated on the audio thread; give it a moment.
        for _ in 0..60 {
            if !app.audio.devices().is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let devs = app.audio.devices();
        let find = |needle: &str| devs.iter().position(|d| d.name.to_lowercase().contains(&needle.to_lowercase()));
        if !args.live_in.is_empty() {
            if let Some(i) = find(&args.live_in) {
                app.cap_sel = i;
            }
        }
        if !args.live_out.is_empty() {
            if let Some(i) = find(&args.live_out) {
                app.out_sel = i + 1;
            }
        }
        app.start_live();
    } else if args.lan_host.is_some() || args.lan_join.is_some() || args.rally_local.is_some() {
        app.lan_cli();
    } else if !args.files.is_empty() {
        app.start_run(args.files.clone());
    }

    loop {
        let dt = get_frame_time().clamp(0.0005, 0.05);
        app.frames += 1;
        app.wall += dt;
        app.message_t = (app.message_t - dt).max(0.0);
        app.esc_used = false;

        // ---- global keys ----
        if is_key_pressed(KeyCode::F11) {
            app.fullscreen = !app.fullscreen;
            display::set_borderless(app.fullscreen);
            app.save_settings();
        }
        if is_key_pressed(KeyCode::F) && app.screen == Screen::Play {
            app.autofire = !app.autofire;
            let p = app.world.player.pos + vec2(0.0, -40.0);
            app.world.text(p, if app.autofire { "AUTO-FIRE ON" } else { "AUTO-FIRE OFF (hold LMB)" }, game::C_WHITE, 20.0, 1.2);
        }

        // ---- dropped files / dialog results ----
        let dropped: Vec<PathBuf> = get_dropped_files().into_iter().filter_map(|f| f.path).collect();
        if !dropped.is_empty() && matches!(app.screen, Screen::Menu | Screen::Options | Screen::Setup | Screen::Results | Screen::LiveSetup) {
            let paths = expand_paths(&dropped);
            if app.screen == Screen::Menu {
                // Difficulty, mode and mutators come first: open run setup with these waiting.
                if !paths.is_empty() {
                    app.staged = paths;
                    app.screen = Screen::Setup;
                    app.menu_sel = S_START;
                }
            } else {
                app.endless = app.setup_mode != 1;
                app.start_run(paths);
            }
        }
        let mut picked: Option<Vec<PathBuf>> = None;
        if let Some(rx) = &app.dialog {
            match rx.try_recv() {
                Ok(p) => picked = Some(p),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => picked = Some(vec![]),
                Err(_) => {}
            }
        }
        if let Some(p) = picked {
            app.dialog = None;
            if app.dialog_library {
                app.dialog_library = false;
                if let Some(dir) = p.into_iter().next() {
                    app.music_dir = Some(dir);
                    app.juke_rescan();
                    app.save_settings();
                }
            } else if !p.is_empty() {
                // Loose files follow the chosen mode: anything but SINGLE carries on into the
                // library when they run out.
                app.endless = app.setup_mode != 1;
                let paths = expand_paths(&p);
                app.start_run(paths);
            }
        }

        app.poll_loader();
        if app.screen == Screen::Lan {
            app.lan_poll(dt);
        }
        let lan_match = app.screen == Screen::Lan && app.lan.as_ref().is_some_and(|l| l.in_match());
        // The codex shows its entries live in the arena, set off to the right of its lists.
        let codex_live = app.screen == Screen::Codex && app.show_want != Demo::None;
        app.rend.show_shift = if codex_live { vec2(0.17, -0.045) } else { Vec2::ZERO };
        app.rend.show_zoom = if codex_live { 0.6 } else { 1.0 };
        app.rend.update_view(&app.world);

        // ---- simulate ----
        let bot = app.args.selftest.is_some();
        let mut hud = HudInfo::empty();
        let mut idle_target = 1.0;
        let mut progress = 0.0;
        match app.screen {
            Screen::Lan if lan_match => {
                idle_target = 0.0;
                progress = app.lan_step(dt);
            }
            Screen::Play => {
                let mut input = if bot { tools::bot_input(&app.world) } else { app.gather_input(true) };
                if bot && app.args.mortal {
                    input.mv = Vec2::ZERO;
                    input.dash = false;
                }
                let (it, h, pr) = if app.live.is_some() { app.step_live(dt, &input) } else { app.step_files(dt, &input) };
                idle_target = it;
                hud = h;
                progress = pr;
                let esc = is_key_pressed(KeyCode::Escape);
                let pause_key = esc || is_key_pressed(KeyCode::P);
                if pause_key && app.screen == Screen::Play {
                    app.esc_used = true;
                    // Live audio cannot be paused at the source: the world freezes and what is
                    // captured meanwhile is thrown away (`live_discard`).
                    if app.live.is_none() {
                        app.audio.set_paused(true);
                    }
                    app.screen = Screen::Paused;
                    app.menu_sel = 0;
                    app.quit_armed = false;
                    app.audio.sfx(Sfx::UiSelect, 0.7, 0.0);
                }
            }
            Screen::Paused => {
                idle_target = app.idle;
                if app.live.is_some() {
                    app.live_discard();
                    hud.title = "LIVE AUDIO".into();
                    hud.show_combat = true;
                }
                if let Some(s) = &app.song {
                    hud.title = s.track.name.clone();
                    hud.progress = s.last_t / s.level.duration.max(0.1);
                    hud.show_combat = true;
                    progress = hud.progress;
                }
            }
            Screen::Runner => {
                idle_target = 1.0;
                progress = app.step_highway(dt);
            }
            _ if codex_live => {
                // Codex: a showcase in the arena, played by the bot, in time with the menu music.
                app.codex_demo_step();
                let input = tools::bot_input(&app.world);
                progress = app.juke_step(dt, &input);
                idle_target = 0.0;
            }
            _ => {
                // Menu-like screens: drift mode with the menu music; the player can still fly around.
                if app.show_demo != Demo::None && app.screen != Screen::Codex {
                    app.codex_leave();
                }
                let mut input = app.gather_input(false);
                input.dash = false;
                progress = app.juke_step(dt, &input);
            }
        }
        // Sound effects the simulation asked for (not in live mode: the capture would hear them).
        let quiet = app.live.is_some() || app.show_demo != Demo::None;
        for (s, gain, pan) in app.world.sounds.drain(..) {
            if !quiet {
                app.audio.sfx(s, gain, pan);
            }
        }

        // ---- idle blend ----
        let rate = if idle_target > app.idle { 1.3 } else { 1.0 };
        let prev_idle = app.idle;
        app.idle += (idle_target - app.idle).clamp(-rate * dt, rate * dt);
        // Launching a run: once the screen is dark the view is already on the arena, so the
        // opening shows it settled.
        if app.launch < LAUNCH_SECS {
            app.launch += dt;
            if app.launch > 0.2 * LAUNCH_SECS && app.screen == Screen::Play {
                app.idle = idle_target;
            }
        }
        if prev_idle <= 0.5 && app.idle > 0.5 && app.screen == Screen::Play {
            app.world.calm_down();
        }

        // ---- draw ----
        if app.args.screen == "highway" && app.screen == Screen::Menu && app.juke.loaded.as_ref().is_some_and(|(p, _)| Some(p) == app.juke.list.get(app.juke.idx)) {
            app.args.screen.clear();
            app.start_highway();
        }
        if let Some((scene, weather, floor, prop, ground)) = app.args.mood {
            app.world.mood = level::Mood { scene, weather, floor, prop, ground };
        }
        if app.args.purgatory && app.screen == Screen::Play {
            app.world.purgatory = true;
            app.world.purg_until = app.world.t + 4.5;
        }
        if let Some((scene, weather, floor, prop, ground)) = app.args.mood_to.filter(|_| app.wall > 3.0) {
            app.world.mood = level::Mood { scene, weather, floor, prop, ground };
        }
        app.rend.set_palette(app.world.palette);
        if lan_match && app.lan.as_ref().is_some_and(|l| l.in_match()) {
            // Rally draws its own field and score HUD over the music-reactive scene.
            app.lan_draw(dt);
        } else {
            app.feed_signal();
            // Menu-like screens: no player or Heart in the vista, and the music sits lower than in a run.
            let in_run = matches!(app.screen, Screen::Play | Screen::Paused);
            app.rend.menu = !in_run && !codex_live;
            let duck_to = if in_run { if matches!(app.phase, Phase::Dying(_)) { 0.35 } else { 1.0 } } else if app.screen == Screen::Runner { 1.0 } else { MENU_MUSIC_GAIN };
            app.music_gain += (duck_to - app.music_gain).clamp(-1.5 * dt, 1.5 * dt);
            app.audio.set_volume(if app.args.mute { 0.0 } else { app.volume * app.music_gain });
            if let (Screen::Runner, Some(s)) = (app.screen, app.runner.as_ref()) {
                app.rend.draw_runner(&s.r, &app.world, dt, progress);
            } else {
                app.rend.draw_scene(&app.world, dt, app.idle, progress);
                app.rend.draw_hud(&app.world, &hud, app.idle);
            }
        }
        let pal = app.rend.pal;
        let (sw, sh) = (screen_width(), screen_height());
        let u = sh / 1080.0;

        match app.screen {
            Screen::Menu => {
                app.rend.draw_logo(110.0 * u, 96.0 * u, u, &app.world);
                let mut tip = ui::Tip::default();
                let items: Vec<(Icon, String)> = MENU_ROWS.iter().map(|r| (r.0, r.1.to_string())).collect();
                let x = 110.0 * u;
                let (rw, rh) = (430.0 * u, 54.0 * u);
                // rows in groups, with a line between the groups
                let mut ys: Vec<f32> = Vec::new();
                let mut y = 300.0 * u;
                for (g, n) in MENU_GROUPS.iter().enumerate() {
                    if g > 0 {
                        ui::sep(x, y + 9.0 * u, rw, u);
                        y += 20.0 * u;
                    }
                    for _ in 0..*n {
                        ys.push(y);
                        y += rh + 8.0 * u;
                    }
                }
                app.menu_sel = app.menu_sel.min(items.len() - 1);
                let before = app.menu_sel;
                let (act, hov) = if app.dialog.is_none() { ui::icon_list(&items, &ys, &mut app.menu_sel, x, rw, rh, u, &pal, app.rend.time) } else { (None, None) };
                if act.is_some() {
                    app.audio.sfx(Sfx::UiSelect, 0.8, 0.0);
                } else if app.menu_sel != before {
                    app.audio.sfx(Sfx::UiMove, 0.6, 0.0);
                }
                let shown = hov.unwrap_or(app.menu_sel);
                tip.set(MENU_ROWS[shown].1, MENU_ROWS[shown].2, hov.is_some());
                match act {
                    Some(M_PLAY) => {
                        app.screen = Screen::Setup;
                        app.menu_sel = S_START;
                    }
                    Some(M_HIGHWAY) => app.start_highway(),
                    Some(M_CODEX) => {
                        app.screen = Screen::Codex;
                        app.menu_sel = 0;
                        app.codex_sel = 0;
                        app.codex_top = 0;
                        app.show_want = codex_items(0).first().map_or(Demo::None, |i| i.demo);
                    }
                    Some(M_LAN) => app.lan_open(),
                    Some(M_OPTIONS) => {
                        app.screen = Screen::Options;
                        app.menu_sel = 0;
                    }
                    Some(M_QUIT) => break,
                    _ => {}
                }
                app.track_corner(&mut tip, progress, None, false, false, u, sw);
                let hy = sh - 62.0 * u;
                let mut kx = 110.0 * u;
                for (k, what) in [("WASD", "move"), ("MOUSE", "aim"), ("SPACE", "dash"), ("F", "auto-fire"), ("ESC", "pause"), ("F11", "fullscreen")] {
                    kx = ui::keycap(k, what, kx, hy + 34.0 * u, u);
                }
                tip.draw(110.0 * u, hy, u, &pal);
                if app.dialog.is_some() {
                    text_shadow("waiting for the file dialog...", sw * 0.5, sh * 0.5, 28.0 * u, pal.accent, 0);
                }
            }
            Screen::Setup => {
                dim(0.45);
                let mut tip = ui::Tip::default();
                let busy = app.dialog.is_some();
                let nd = DIFFS.len();
                let time = app.rend.time;
                let dim_w = |a: f32| Color::new(1.0, 1.0, 1.0, a);
                let mut dirk = 0i32;
                if !busy {
                    let dv = is_key_pressed(KeyCode::Down) as i32 - is_key_pressed(KeyCode::Up) as i32;
                    if dv != 0 {
                        app.menu_sel = (app.menu_sel as i32 + dv).rem_euclid(S_START as i32 + 1) as usize;
                        app.audio.sfx(Sfx::UiMove, 0.6, 0.0);
                    }
                    dirk = is_key_pressed(KeyCode::Right) as i32 - is_key_pressed(KeyCode::Left) as i32;
                }
                let row = app.menu_sel.min(S_START);
                let enter = !busy && (is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter));
                let mut changed = false;
                let mut pick = false;

                // =============== left: what kind of run ===============
                let (lx, ly, lw) = (110.0 * u, 190.0 * u, 600.0 * u);
                let pad = 22.0 * u;
                render::vtext("RUN SETUP", lx, 142.0 * u, 54.0 * u, WHITE, -1);
                ui::panel(lx, ly, lw, 548.0 * u, u);
                // the modes, one tile each, like a roster
                let nm = SETUP_MODES.len();
                let ms = 96.0 * u;
                let mgap = (lw - 2.0 * pad - nm as f32 * ms) / (nm as f32 - 1.0);
                let cur = app.setup_mode.min(nm - 1);
                let mut shown_mode = cur;
                for (i, m) in SETUP_MODES.iter().enumerate() {
                    let tx = lx + pad + i as f32 * (ms + mgap);
                    let on = i == cur;
                    let (hv, click) = ui::tile(tx, ly + pad, ms, on, row == S_MODE && on, pal.primary, 1.0, u);
                    ui::icon(m.0, tx + ms * 0.5, ly + pad + ms * 0.5, ms * 0.27, if on { WHITE } else if hv { pal.primary } else { dim_w(0.7) });
                    if hv {
                        shown_mode = i;
                    }
                    if click && !busy {
                        app.setup_mode = i;
                        app.menu_sel = S_MODE;
                        pick = true;
                    }
                }
                if row == S_MODE && dirk != 0 {
                    app.setup_mode = (cur as i32 + dirk).rem_euclid(nm as i32) as usize;
                    shown_mode = app.setup_mode;
                    pick = true;
                }
                let my = ly + pad + ms + 60.0 * u;
                text_center(SETUP_MODES[shown_mode].1, lx + lw * 0.5, my, 46.0 * u, WHITE);
                for (i, l) in ui::wrap(SETUP_MODES[shown_mode].2, 21.0 * u, lw - 2.0 * pad - 20.0 * u).iter().take(3).enumerate() {
                    text_center(l, lx + lw * 0.5, my + 38.0 * u + i as f32 * 27.0 * u, 21.0 * u, dim_w(0.8));
                }
                let mut ry = my + 38.0 * u + 3.0 * 27.0 * u;
                ui::sep(lx + pad, ry - 8.0 * u, lw - 2.0 * pad, u);
                // rows: a label cell, then what it holds
                let (lab_w, row_h) = (150.0 * u, 66.0 * u);
                let cell = |label: &str, y: f32, focus: bool| {
                    ui::rrect(lx + pad, y, lab_w, row_h, ui::RADIUS * u, Color::new(1.0, 1.0, 1.0, 0.06));
                    if focus {
                        ui::rrect_lines(lx + pad, y, lab_w, row_h, ui::RADIUS * u, 1.5 * u, dim_w(0.75));
                    }
                    text_center(label, lx + pad + lab_w * 0.5, y + row_h * 0.5 + 8.0 * u, 23.0 * u, pal.primary);
                };
                let cx0 = lx + pad + lab_w + 14.0 * u;
                // ---- remix: the same song, dealt differently ----
                ry += 8.0 * u;
                cell("REMIX", ry, row == S_REMIX);
                let (h0, c0) = ui::tile(cx0, ry, row_h, app.remix == 0, false, pal.accent, 1.0, u);
                ui::icon(Icon::Note, cx0 + row_h * 0.5, ry + row_h * 0.5, row_h * 0.26, if app.remix == 0 { WHITE } else { dim_w(0.7) });
                let dx = cx0 + row_h + 12.0 * u;
                let (h1, c1) = ui::tile(dx, ry, row_h, app.remix > 0, false, pal.accent, 1.0, u);
                ui::icon(Icon::Dice, dx + row_h * 0.5, ry + row_h * 0.5, row_h * 0.26, if app.remix > 0 { WHITE } else { dim_w(0.7) });
                let remix_s = if app.remix == 0 { "the song's own deal".to_string() } else { format!("deal no. {}", app.remix) };
                text_left(&remix_s, dx + row_h + 18.0 * u, ry + row_h * 0.5 + 8.0 * u, 23.0 * u, dim_w(0.85));
                if h0 || h1 || row == S_REMIX {
                    tip.set("REMIX", "The same song, dealt differently: other patterns, enemies and upgrade offers on the same beats. R or the dice for the next deal, the note for the song's own. Scores share one best.", h0 || h1);
                }
                if !busy {
                    let mut r = app.remix as i32;
                    if c1 || is_key_pressed(KeyCode::R) || (enter && row == S_REMIX) {
                        r = (r + 1) % 100;
                    }
                    if row == S_REMIX {
                        r = (r + dirk).clamp(0, 99);
                    }
                    if c0 {
                        r = 0;
                    }
                    if r as u64 != app.remix {
                        app.remix = r as u64;
                        pick = true;
                    }
                    if c0 || c1 {
                        app.menu_sel = S_REMIX;
                    }
                }
                // ---- what the run is worth, and the best so far ----
                ry += row_h + 10.0 * u;
                cell("SCORE", ry, false);
                let mult = format!("x{:.2}", app.score_mult());
                text_left(&mult, cx0 + 4.0 * u, ry + row_h * 0.5 + 12.0 * u, 36.0 * u, pal.accent);
                let nmut = app.mutators.count_ones();
                let why = format!("{}{}", DIFFS[app.diff].1, if nmut > 0 { format!("  +  {nmut} mutator{}", if nmut > 1 { "s" } else { "" }) } else { String::new() });
                text_left(&why, cx0 + 14.0 * u + text_w(&mult, 36.0 * u), ry + row_h * 0.5 + 8.0 * u, 20.0 * u, dim_w(0.6));
                ry += row_h + 10.0 * u;
                cell("BEST", ry, false);
                let key = app.score_key(&app.juke.list.get(app.juke.idx).map(|p| track_title(p)).unwrap_or_default());
                match app.scores.get(&key).copied() {
                    Some((s, d)) => {
                        ui::icon(Icon::Trophy, cx0 + 20.0 * u, ry + row_h * 0.5, 15.0 * u, pal.accent);
                        let ds = format!("DEPTH {d}");
                        text_left(&ds, cx0 + 48.0 * u, ry + row_h * 0.5 + 11.0 * u, 32.0 * u, WHITE);
                        text_left(&format!("score {s}"), cx0 + 62.0 * u + text_w(&ds, 32.0 * u), ry + row_h * 0.5 + 8.0 * u, 20.0 * u, dim_w(0.6));
                    }
                    None => text_left("none yet for this track, tier and mode", cx0 + 4.0 * u, ry + row_h * 0.5 + 8.0 * u, 20.0 * u, dim_w(0.55)),
                }

                // =============== right: the track, then the rules ===============
                if row == S_TRACK && dirk != 0 {
                    app.setup_col = (app.setup_col as i32 + dirk).rem_euclid(CORNER.len() as i32) as usize;
                    pick = true;
                }
                let corner_focus = if row == S_TRACK { Some(app.setup_col.min(CORNER.len() - 1)) } else { None };
                app.track_corner(&mut tip, progress, corner_focus, enter && row == S_TRACK, true, u, sw);
                let (rx, rw) = (sw - 700.0 * u, 590.0 * u);
                let py = (110.0 + CORNER_H) * u;
                let inset = 14.0 * u;
                ui::panel(rx, py, rw, 490.0 * u, u);
                // ---- difficulty: a strip that scrolls; tiers away from the chosen one slide out of view ----
                let dcol = ui::diff_color(app.diff, nd, &pal);
                ui::header(rx + inset, py + inset, rw - 2.0 * inset, 44.0 * u, Icon::Heart, "DIFFICULTY", dcol, row == S_DIFF, u);
                let (vx, vy, vw, vh) = (rx + inset, py + inset + 54.0 * u, rw - 2.0 * inset, 128.0 * u);
                ui::rrect(vx, vy, vw, vh, ui::RADIUS * u, Color::new(0.0, 0.0, 0.03, 0.5));
                app.diff_scroll += (app.diff as f32 - app.diff_scroll) * (1.0 - (-14.0 * dt).exp());
                let (ts, pitch) = (96.0 * u, 132.0 * u);
                let in_strip = ui::hover(vx, vy, vw, vh);
                let mut shown = app.diff;
                let mut diff_to: Option<usize> = None;
                ui::clip(Some((vx, vy, vw, vh)));
                for i in 0..nd {
                    let off = i as f32 - app.diff_scroll;
                    let tx = vx + vw * 0.5 + off * pitch - ts * 0.5;
                    if tx > vx + vw || tx + ts < vx {
                        continue;
                    }
                    let fade = (1.0 - (off.abs() - 1.1).max(0.0) / 1.2).clamp(0.0, 1.0);
                    let on = i == app.diff;
                    let col = ui::diff_color(i, nd, &pal);
                    let (hv, click) = ui::tile(tx, vy + 16.0 * u, ts, on, false, col, fade, u);
                    let hv = hv && in_strip;
                    ui::diff_icon(i, nd, tx + ts * 0.5, vy + 16.0 * u + ts * 0.5, ts * 0.32, Color::new(col.r, col.g, col.b, fade * if on || hv { 1.0 } else { 0.75 }));
                    if hv {
                        shown = i;
                        tip.set(DIFFS[i].1, DIFFS[i].2, true);
                        if click && !busy {
                            diff_to = Some(i);
                        }
                    }
                }
                ui::clip(None);
                // arrows at the ends of the strip, and the wheel, scroll it
                for (side, ic) in [(-1i32, Icon::Prev), (1, Icon::Next)] {
                    let can = if side < 0 { app.diff > 0 } else { app.diff + 1 < nd };
                    let ax = if side < 0 { vx + 4.0 * u } else { vx + vw - 34.0 * u };
                    let hv = ui::hover(ax, vy, 30.0 * u, vh);
                    if can {
                        ui::icon(ic, ax + 15.0 * u, vy + vh * 0.5, 9.0 * u, dim_w(if hv { 1.0 } else { 0.45 }));
                        if hv && is_mouse_button_pressed(MouseButton::Left) && !busy {
                            diff_to = Some((app.diff as i32 + side) as usize);
                        }
                    }
                }
                if !busy {
                    let (_, wheel) = mouse_wheel();
                    let mut step = if row == S_DIFF { dirk } else { 0 };
                    if in_strip && wheel != 0.0 {
                        step -= wheel.signum() as i32;
                    }
                    if step != 0 {
                        diff_to = Some((app.diff as i32 + step).clamp(0, nd as i32 - 1) as usize);
                    }
                }
                if let Some(d) = diff_to.filter(|d| *d != app.diff) {
                    app.diff = d;
                    shown = d;
                    changed = true;
                    pick = true;
                    if in_strip {
                        app.menu_sel = S_DIFF;
                    }
                }
                let scol = ui::diff_color(shown, nd, &pal);
                let ny = vy + vh + 40.0 * u;
                text_left(DIFFS[shown].1, vx + 6.0 * u, ny, 34.0 * u, scol);
                text_left(&format!("{} / {}", shown + 1, nd), vx + 20.0 * u + text_w(DIFFS[shown].1, 34.0 * u), ny, 20.0 * u, dim_w(0.5));
                render::text_right(&format!("score x{:.2}", TIER_SCORE[shown.min(TIER_SCORE.len() - 1)]), vx + vw - 6.0 * u, ny, 20.0 * u, dim_w(0.6));
                for (i, l) in ui::wrap(DIFFS[shown].2, 20.0 * u, vw - 12.0 * u).iter().take(2).enumerate() {
                    text_left(l, vx + 6.0 * u, ny + 30.0 * u + i as f32 * 25.0 * u, 20.0 * u, dim_w(0.82));
                }
                if row == S_DIFF {
                    tip.set(DIFFS[app.diff].1, DIFFS[app.diff].2, false);
                }
                // ---- mutators: optional rules, each worth more score ----
                let hy2 = ny + 30.0 * u + 2.0 * 25.0 * u + 4.0 * u;
                ui::header(rx + inset, hy2, rw - 2.0 * inset, 44.0 * u, Icon::Bolt, "MUTATORS", pal.secondary, row == S_MUT, u);
                let nmods = game::MODS.len();
                let mts = 76.0 * u;
                let mg = (vw - nmods as f32 * mts) / nmods as f32;
                if row == S_MUT && dirk != 0 {
                    app.mut_col = (app.mut_col as i32 + dirk).rem_euclid(nmods as i32) as usize;
                    pick = true;
                }
                let mut toggle: Option<usize> = None;
                for (i, m) in game::MODS.iter().enumerate() {
                    let on = app.mutators >> i & 1 == 1;
                    let focus = row == S_MUT && app.mut_col == i;
                    let tx = vx + mg * 0.5 + i as f32 * (mts + mg);
                    let tyy = hy2 + 56.0 * u;
                    let (hv, click) = ui::tile(tx, tyy, mts, on, focus, pal.accent, 1.0, u);
                    ui::icon(ui::mod_icon(i), tx + mts * 0.5, tyy + mts * 0.5, mts * 0.27, if on { WHITE } else if hv { pal.accent } else { dim_w(0.6) });
                    text_center(m.0, tx + mts * 0.5, tyy + mts + 24.0 * u, 16.0 * u, if on { pal.accent } else { dim_w(0.55) });
                    if hv || focus {
                        tip.set(m.0, &format!("{}. +{:.0}% score. {}", m.1, game::MUTATOR_SCORE * 100.0, if on { "ON" } else { "off" }), hv);
                    }
                    if click && !busy {
                        toggle = Some(i);
                        app.menu_sel = S_MUT;
                        app.mut_col = i;
                    }
                }
                if enter && row == S_MUT {
                    toggle = Some(app.mut_col);
                }
                if let Some(i) = toggle {
                    app.mutators ^= 1 << i;
                    changed = true;
                    pick = true;
                }

                // =============== bottom: go, or back ===============
                let (hs, cs) = ui::go_button(sw * 0.5 - 180.0 * u, 880.0 * u, 360.0 * u, 80.0 * u, Icon::Play, "START", row == S_START, pal.primary, u, time);
                if hs || row == S_START {
                    tip.set(&format!("START  {}", SETUP_MODES[cur].1), SETUP_MODES[cur].2, hs);
                }
                if !app.staged.is_empty() {
                    // dropped files are waiting: say so, and offer to forget them
                    let n = app.staged.len();
                    let label = format!("START plays the {n} dropped file{}", if n == 1 { "" } else { "s" });
                    let lw2 = text_w(&label, 21.0 * u);
                    text_shadow(&label, sw * 0.5 - 22.0 * u, 862.0 * u, 21.0 * u, pal.accent, 0);
                    let bx = sw * 0.5 + lw2 * 0.5 - 8.0 * u;
                    let (hx, cx) = ui::tile(bx, 840.0 * u, 30.0 * u, false, false, pal.danger, 1.0, u);
                    ui::icon(Icon::Cross, bx + 15.0 * u, 855.0 * u, 6.0 * u, if hx { pal.danger } else { dim_w(0.7) });
                    if hx {
                        tip.set("FORGET THEM", "Go back to starting on the track that is playing.", true);
                    }
                    if cx && !busy {
                        app.staged.clear();
                    }
                }
                let (_, cb) = ui::button(sw - 110.0 * u - 190.0 * u, 892.0 * u, 190.0 * u, 58.0 * u, Some(Icon::Back), "BACK", false, false, u, &pal);
                let start = (cs && !busy) || (enter && matches!(row, S_START | S_MODE | S_DIFF));
                if pick {
                    app.audio.sfx(Sfx::UiMove, 0.6, 0.0);
                }
                if changed {
                    app.save_settings();
                }
                if start {
                    app.audio.sfx(Sfx::UiSelect, 0.8, 0.0);
                    let endless = app.setup_mode != 1;
                    // DAILY SEED: days since 1970-01-01 (UTC) seeds the levels and the offers.
                    app.args.seed = if app.setup_mode == MODE_DAILY {
                        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() / 86_400).unwrap_or(0)
                    } else {
                        app.base_seed.wrapping_add(app.remix.wrapping_mul(0x9E37_79B9))
                    };
                    if app.staged.is_empty() {
                        app.start_from_juke(0, endless);
                    } else {
                        app.endless = endless;
                        let paths = std::mem::take(&mut app.staged);
                        app.start_run(paths);
                    }
                } else if (is_key_pressed(KeyCode::Escape) || cb) && !busy && app.screen == Screen::Setup {
                    app.staged.clear();
                    app.screen = Screen::Menu;
                    app.menu_sel = M_PLAY;
                }
                let hy = sh - 62.0 * u;
                let mut kx = 110.0 * u;
                for (k, what) in [("ARROWS", "choose"), ("ENTER", "start"), ("Q  E", "track"), ("R", "remix"), ("ESC", "back")] {
                    kx = ui::keycap(k, what, kx, hy + 34.0 * u, u);
                }
                tip.draw(110.0 * u, hy, u, &pal);
                if busy {
                    text_shadow("waiting for the file dialog...", sw * 0.5, sh * 0.5, 28.0 * u, pal.accent, 0);
                }
            }
            Screen::Options => {
                dim(0.5);
                let x = 110.0 * u;
                render::vtext("OPTIONS", x, 130.0 * u, 64.0 * u, WHITE, -1);
                let mut tip = ui::Tip::default();
                let dv = is_key_pressed(KeyCode::Down) as i32 - is_key_pressed(KeyCode::Up) as i32;
                if dv != 0 {
                    app.menu_sel = (app.menu_sel as i32 + dv).rem_euclid(O_BACK as i32 + 1) as usize;
                    app.audio.sfx(Sfx::UiMove, 0.6, 0.0);
                }
                let dirk = is_key_pressed(KeyCode::Right) as i32 - is_key_pressed(KeyCode::Left) as i32;
                let (cx, cw, rh) = (x + 330.0 * u, 430.0 * u, 56.0 * u);
                let mut changed = false;
                let old_sfx = app.sfx_volume;
                for (r, row) in OPTION_ROWS.iter().enumerate() {
                    let y = 250.0 * u + r as f32 * 74.0 * u;
                    let hv = ui::hover(x, y, 330.0 * u + cw + 110.0 * u, rh);
                    if hv && mouse_delta_position() != Vec2::ZERO {
                        app.menu_sel = r;
                    }
                    let focus = app.menu_sel == r;
                    let col = if focus { WHITE } else { Color::new(1.0, 1.0, 1.0, 0.72) };
                    ui::icon(row.0, x + 26.0 * u, y + rh * 0.5, 15.0 * u, if focus { pal.accent } else { col });
                    text_left(row.1, x + 60.0 * u, y + rh * 0.5 + 9.0 * u, 26.0 * u, col);
                    if hv || focus {
                        tip.set(row.1, row.2, hv);
                    }
                    let k = if focus { dirk } else { 0 };
                    let note = |s: &str| text_left(s, cx + cw + 22.0 * u, y + rh * 0.5 + 8.0 * u, 22.0 * u, Color::new(1.0, 1.0, 1.0, 0.85));
                    match r {
                        O_MUSIC | O_SFX => {
                            let v = if r == O_SFX { app.sfx_volume } else { app.volume };
                            let mut nv = (v + 0.1 * k as f32).clamp(0.0, 1.0);
                            if let Some(d) = ui::slider(cx, y, cw, rh, v, focus, u, &pal) {
                                nv = (d * 20.0).round() / 20.0;
                            }
                            if (nv - v).abs() > 0.001 {
                                if r == O_SFX { app.sfx_volume = nv } else { app.volume = nv }
                                changed = true;
                            }
                            note(&format!("{:.0}%", nv * 100.0));
                        }
                        O_SYNC => {
                            let v = app.offset_ms;
                            let mut nv = (v + 10.0 * k as f32).clamp(-100.0, 300.0);
                            if let Some(d) = ui::slider(cx, y, cw, rh, (v + 100.0) / 400.0, focus, u, &pal) {
                                nv = ((d * 400.0 - 100.0) / 10.0).round() * 10.0;
                            }
                            if (nv - v).abs() > 0.5 {
                                app.offset_ms = nv;
                                changed = true;
                            }
                            note(&format!("{:+.0} ms", nv));
                        }
                        O_VISUALS => {
                            let labels: Vec<(Option<Icon>, &str)> = VISUALS.iter().map(|v| (None, v.1)).collect();
                            let (click, _) = ui::segments(cx, y, cw, rh, &labels, app.visuals, focus, u, &pal);
                            let nv = click.unwrap_or(((app.visuals as i32 + k).rem_euclid(VISUALS.len() as i32)) as usize);
                            if nv != app.visuals {
                                app.visuals = nv;
                                changed = true;
                            }
                        }
                        _ => {
                            let (click, _) = ui::segments(cx, y, cw, rh, &[(None, "WINDOWED"), (None, "BORDERLESS")], app.fullscreen as usize, focus, u, &pal);
                            let nv = match click {
                                Some(i) => i == 1,
                                None => app.fullscreen ^ (k != 0),
                            };
                            if nv != app.fullscreen {
                                app.fullscreen = nv;
                                display::set_borderless(nv);
                                changed = true;
                            }
                        }
                    }
                }
                let by = 250.0 * u + O_BACK as f32 * 74.0 * u + 10.0 * u;
                let (hb, cb) = ui::button(x, by, 240.0 * u, rh, Some(Icon::Back), "BACK", app.menu_sel == O_BACK, false, u, &pal);
                if hb || app.menu_sel == O_BACK {
                    tip.set("BACK", "Return to the menu. ESC does the same. Settings are saved as you change them.", hb);
                }
                let enter = is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter);
                if changed {
                    let mute = app.args.mute;
                    app.audio.set_sfx_volume(if mute { 0.0 } else { app.sfx_volume });
                    if (app.sfx_volume - old_sfx).abs() > 0.001 {
                        app.audio.sfx(Sfx::Shard, 1.0, 0.0);
                    }
                    app.save_settings();
                }
                tip.draw(x, sh - 60.0 * u, u, &pal);
                if cb || is_key_pressed(KeyCode::Escape) || (enter && app.menu_sel == O_BACK) {
                    app.audio.sfx(Sfx::UiSelect, 0.8, 0.0);
                    app.screen = Screen::Menu;
                    app.menu_sel = M_OPTIONS;
                }
            }
            Screen::LiveSetup => {
                dim(0.66);
                let devs = app.audio.devices();
                let x = 110.0 * u;
                render::vtext("LIVE AUDIO", x, 130.0 * u, 64.0 * u, WHITE, -1);
                text_left("Heartline listens to an output device and builds the level on the fly.", x, 170.0 * u, 24.0 * u, pal.primary);

                let (mx, my) = mouse_position();
                let click = is_mouse_button_pressed(MouseButton::Left);
                let row_h = 34.0 * u;
                let col_w = 800.0 * u;
                // capture column
                let mut y = 230.0 * u;
                text_left("LISTEN TO", x, y, 26.0 * u, pal.accent);
                y += 14.0 * u;
                for (i, d) in devs.iter().enumerate().take(9) {
                    let hover = mx >= x && mx <= x + col_w && my >= y && my <= y + row_h;
                    let on = app.cap_sel == i;
                    ui::rrect(x, y, col_w, row_h - 4.0 * u, 8.0 * u, if on { Color::new(pal.secondary.r, pal.secondary.g, pal.secondary.b, 0.3) } else { Color::new(0.0, 0.0, 0.05, 0.45) });
                    let label = format!("{}{}", d.name, if d.is_default { "   (default)" } else { "" });
                    text_left(&label, x + 14.0 * u, y + row_h * 0.62, 22.0 * u, if on { WHITE } else { Color::new(1.0, 1.0, 1.0, 0.7) });
                    if hover && click {
                        app.cap_sel = i;
                    }
                    y += row_h;
                }
                y += 34.0 * u;
                text_left("PLAY IT BACK ON  (delayed look-ahead mode)", x, y, 26.0 * u, pal.accent);
                y += 14.0 * u;
                let mut outs: Vec<String> = vec!["nowhere - reactive mode, zero setup".to_string()];
                outs.extend(devs.iter().map(|d| d.name.clone()));
                for (i, name) in outs.iter().enumerate().take(10) {
                    let disabled = i > 0 && i - 1 == app.cap_sel;
                    let hover = mx >= x && mx <= x + col_w && my >= y && my <= y + row_h;
                    let on = app.out_sel == i;
                    ui::rrect(x, y, col_w, row_h - 4.0 * u, 8.0 * u, if on { Color::new(pal.secondary.r, pal.secondary.g, pal.secondary.b, 0.3) } else { Color::new(0.0, 0.0, 0.05, 0.45) });
                    let col = if disabled { Color::new(1.0, 1.0, 1.0, 0.25) } else if on { WHITE } else { Color::new(1.0, 1.0, 1.0, 0.7) };
                    text_left(name, x + 14.0 * u, y + row_h * 0.62, 22.0 * u, col);
                    if hover && click && !disabled {
                        app.out_sel = i;
                    }
                    y += row_h;
                }
                if app.out_sel > 0 && app.out_sel - 1 == app.cap_sel {
                    app.out_sel = 0;
                }
                // explanation
                let ex = x + col_w + 60.0 * u;
                let mut ey = 244.0 * u;
                let lines: [&str; 12] = [
                    "REACTIVE",
                    "Pick the device your music plays on and",
                    "leave playback on 'nowhere'. The Heart",
                    "reacts the instant a hit is heard.",
                    "",
                    "DELAYED LOOK-AHEAD",
                    "Send your music app to a second or virtual",
                    "output (e.g. a virtual cable or unused HDMI),",
                    "listen to that here, and play it back on your",
                    "real speakers. You hear it a few seconds late",
                    "and every spawn is telegraphed to land on",
                    "the beat, like a fully analysed song.",
                ];
                for l in lines {
                    let head = l == "REACTIVE" || l == "DELAYED LOOK-AHEAD";
                    text_left(l, ex, ey, if head { 24.0 * u } else { 21.0 * u }, if head { pal.accent } else { Color::new(1.0, 1.0, 1.0, 0.8) });
                    ey += 28.0 * u;
                }
                ey += 20.0 * u;
                text_left(&format!("DELAY   {:.1}s     ( - / + )", app.delay), ex, ey, 24.0 * u, pal.primary);
                if is_key_pressed(KeyCode::Minus) || is_key_pressed(KeyCode::KpSubtract) {
                    app.delay = (app.delay - 0.5).max(1.5);
                }
                if is_key_pressed(KeyCode::Equal) || is_key_pressed(KeyCode::KpAdd) {
                    app.delay = (app.delay + 0.5).min(6.0);
                }
                // buttons
                let by = sh - 130.0 * u;
                let start_hover = mx >= x && mx <= x + 300.0 * u && my >= by && my <= by + 54.0 * u;
                ui::rrect(x, by, 300.0 * u, 54.0 * u, 8.0 * u, Color::new(pal.secondary.r, pal.secondary.g, pal.secondary.b, if start_hover { 0.5 } else { 0.3 }));
                ui::rrect_lines(x, by, 300.0 * u, 54.0 * u, 8.0 * u, 2.0 * u, pal.accent);
                text_center("START   [ENTER]", x + 150.0 * u, by + 36.0 * u, 28.0 * u, WHITE);
                text_left("ESC  back", x + 330.0 * u, by + 36.0 * u, 24.0 * u, Color::new(1.0, 1.0, 1.0, 0.7));
                if devs.is_empty() {
                    text_left("no output devices found", x, by - 20.0 * u, 24.0 * u, pal.danger);
                } else if (start_hover && click) || is_key_pressed(KeyCode::Enter) {
                    app.start_live();
                }
                if is_key_pressed(KeyCode::Escape) {
                    app.screen = Screen::Setup;
                    app.menu_sel = S_TRACK;
                }
            }
            Screen::Paused => {
                dim(0.55);
                render::vtext("PAUSED", sw * 0.5, sh * 0.3, 80.0 * u, WHITE, 0);
                // Quitting throws the run away, so it takes a second press.
                let live = app.live.is_some();
                let quit_label = if app.quit_armed { "REALLY QUIT?  THE RUN IS LOST" } else { "QUIT TO MENU" };
                let items: Vec<String> = if live {
                    vec!["RESUME".to_string(), quit_label.to_string()]
                } else {
                    vec!["RESUME".to_string(), "RESTART RUN".to_string(), quit_label.to_string()]
                };
                let quit_row = items.len() - 1;
                let before = app.menu_sel;
                let act = menu_list(&items, &mut app.menu_sel, sw * 0.5 - 260.0 * u, sh * 0.42, u, &app.rend, &app.audio);
                if app.menu_sel != before {
                    app.quit_armed = false;
                }
                if live {
                    text_center("the music keeps playing; nothing can hurt you while paused", sw * 0.5, sh * 0.42 + 150.0 * u, 22.0 * u, Color::new(1.0, 1.0, 1.0, 0.7));
                }
                if ((is_key_pressed(KeyCode::Escape) || is_key_pressed(KeyCode::P)) && !app.esc_used) || act == Some(0) {
                    if live {
                        app.live_resume();
                    } else {
                        app.audio.set_paused(false);
                    }
                    app.screen = Screen::Play;
                } else if act == Some(quit_row) {
                    if app.quit_armed {
                        app.to_menu();
                    } else {
                        app.quit_armed = true;
                    }
                } else if act == Some(1) {
                    let q = app.queue.clone();
                    app.start_run(q);
                }
            }
            Screen::Results => {
                app.world.orbs.clear();
                dim(0.6);
                let st = app.world.stats;
                let dim_w = |a: f32| Color::new(1.0, 1.0, 1.0, a);
                let (pw, ph) = (1000.0 * u, 724.0 * u);
                let (px, py) = (sw * 0.5 - pw * 0.5, sh * 0.5 - ph * 0.5 - 10.0 * u);
                let pad = 30.0 * u;
                ui::panel(px, py, pw, ph, u);
                let hcol = if app.died { pal.danger } else { pal.accent };
                render::vtext(if app.died { "SIGNAL LOST" } else { "RUN COMPLETE" }, sw * 0.5, py + 86.0 * u, 62.0 * u, hcol, 0);
                let depth = app.world.depth + 1;
                text_center(&format!("DEPTH {depth}"), sw * 0.5, py + 180.0 * u, 88.0 * u, WHITE);
                text_center(&format!("{:08}", app.world.score), sw * 0.5, py + 224.0 * u, 32.0 * u, dim_w(0.7));
                // the rank is how fast the run went down: depths per track
                let pace = app.world.depth as f32 / st.songs.max(1) as f32;
                let rank = if pace >= 2.5 {
                    "S"
                } else if pace >= 1.75 {
                    "A"
                } else if pace >= 1.0 && !(app.died && st.songs == 0) {
                    "B"
                } else {
                    "C"
                };
                // the rank in its own tile, the multiplier and the best on the other side
                let rs = 104.0 * u;
                let (rx, ry) = (px + pw - pad - rs, py + 112.0 * u);
                ui::rrect(rx, ry, rs, rs, 14.0 * u, Color::new(pal.primary.r * 0.35, pal.primary.g * 0.35, pal.primary.b * 0.4, 0.85));
                ui::rrect_lines(rx, ry, rs, rs, 14.0 * u, 2.0 * u, pal.primary);
                text_center(rank, rx + rs * 0.5, ry + rs * 0.5 + 26.0 * u, 84.0 * u, WHITE);
                text_center("RANK", rx + rs * 0.5, ry - 10.0 * u, 18.0 * u, pal.primary);
                app.record_score();
                ui::icon(Icon::Trophy, px + pad + 16.0 * u, py + 148.0 * u, 14.0 * u, pal.accent);
                if app.run_best {
                    let pulse = 0.7 + 0.3 * (app.rend.time * 6.0).sin();
                    text_left("NEW BEST", px + pad + 42.0 * u, py + 158.0 * u, 30.0 * u, Color::new(pal.accent.r, pal.accent.g, pal.accent.b, pulse));
                } else if let Some((_, d)) = app.scores.get(&app.run_key) {
                    text_left(&format!("best depth {d}"), px + pad + 42.0 * u, py + 156.0 * u, 24.0 * u, dim_w(0.7));
                } else {
                    text_left("no best kept", px + pad + 42.0 * u, py + 156.0 * u, 22.0 * u, dim_w(0.5));
                }
                text_left(&format!("score x{:.2}", app.world.score_mult), px + pad + 2.0 * u, py + 196.0 * u, 22.0 * u, dim_w(0.6));
                ui::sep(px + pad, py + 240.0 * u, pw - 2.0 * pad, u);
                // what happened, three to a row: an icon, the number, what it counts
                let cells = [
                    (Icon::Note, "tracks cleared", format!("{}", st.songs)),
                    (Icon::Heart, "hearts broken", format!("{}", st.hearts_broken)),
                    (Icon::Target, "enemies destroyed", format!("{}", st.kills)),
                    (Icon::Sfx, "notes shot down", format!("{}", st.notes_shot)),
                    (Icon::Visuals, "grazes  (threads)", format!("{}  ({})", st.grazes, st.threads)),
                    (Icon::Bolt, "extra depths dived", format!("{}", st.dives)),
                    (Icon::Shard, "shards collected", format!("{}", st.shards)),
                    (Icon::Combo, "best combo", format!("{:.0}", st.max_combo)),
                    (Icon::Cross, "hits taken", format!("{}", st.hits)),
                ];
                let gap = 14.0 * u;
                let cw = (pw - 2.0 * pad - 2.0 * gap) / 3.0;
                let chh = 86.0 * u;
                for (i, (ic, label, val)) in cells.iter().enumerate() {
                    let cx = px + pad + (i % 3) as f32 * (cw + gap);
                    let cy = py + 262.0 * u + (i / 3) as f32 * (chh + gap);
                    ui::rrect(cx, cy, cw, chh, ui::RADIUS * u, Color::new(1.0, 1.0, 1.0, 0.05));
                    ui::rrect(cx + 10.0 * u, cy + 10.0 * u, chh - 20.0 * u, chh - 20.0 * u, 8.0 * u, Color::new(1.0, 1.0, 1.0, 0.07));
                    ui::icon(*ic, cx + chh * 0.5, cy + chh * 0.5, 17.0 * u, if *ic == Icon::Cross { pal.danger } else { pal.primary });
                    text_left(val, cx + chh + 4.0 * u, cy + 42.0 * u, 34.0 * u, WHITE);
                    text_left(label, cx + chh + 4.0 * u, cy + 68.0 * u, 18.0 * u, dim_w(0.6));
                }
                // what next: a run that was not lost can always carry on
                let can_retry = !app.queue.is_empty();
                let can_continue = !app.died;
                let by = py + ph - 116.0 * u;
                let (mut go, mut again, mut menu) = (false, false, false);
                let bw = 250.0 * u;
                let total = if can_continue { 340.0 * u + 20.0 * u } else { 0.0 } + if can_retry { bw + 20.0 * u } else { 0.0 } + bw;
                let mut bx = sw * 0.5 - total * 0.5;
                if can_continue {
                    go = ui::go_button(bx, by, 340.0 * u, 76.0 * u, Icon::Next, "KEEP GOING", false, pal.primary, u, app.rend.time).1;
                    bx += 360.0 * u;
                }
                if can_retry {
                    again = ui::button(bx, by + 8.0 * u, bw, 60.0 * u, Some(Icon::Retry), "NEW RUN", false, false, u, &pal).1;
                    bx += bw + 20.0 * u;
                }
                menu = ui::button(bx, by + 8.0 * u, bw, 60.0 * u, Some(Icon::Home), "MENU", false, false, u, &pal).1 || menu;
                let mut kx = sw * 0.5 - 270.0 * u;
                if can_continue {
                    kx = ui::keycap("SPACE", "keep going with more songs", kx, py + ph - 20.0 * u, u);
                } else {
                    kx += 130.0 * u;
                }
                if can_retry {
                    kx = ui::keycap("ENTER", "new run", kx, py + ph - 20.0 * u, u);
                }
                ui::keycap("ESC", "menu", kx, py + ph - 20.0 * u, u);
                if is_key_pressed(KeyCode::Escape) || menu {
                    app.to_menu();
                } else if can_continue && (is_key_pressed(KeyCode::Space) || go) {
                    app.continue_run();
                } else if can_retry && (is_key_pressed(KeyCode::Enter) || again) {
                    let q = app.queue.clone();
                    app.start_run(q);
                }
            }
            Screen::Runner => {
                let mut again = false;
                let mut leave = is_key_pressed(KeyCode::Escape);
                if let Some(s) = app.runner.as_ref() {
                    let r = &s.r;
                    let m = 34.0 * u;
                    render::vtext(&format!("{:08}", r.score), m, m + 40.0 * u, 42.0 * u, WHITE, -1);
                    let mult = 1 + (r.combo / 20).min(7);
                    render::vtext(&format!("X{mult}"), m, m + 82.0 * u, 26.0 * u, if mult >= 4 { pal.accent } else { pal.primary }, -1);
                    text_left(&format!("combo {}", r.combo), m + 70.0 * u, m + 80.0 * u, 22.0 * u, Color::new(1.0, 1.0, 1.0, 0.75));
                    text_shadow(&s.title, sw * 0.5, m + 8.0 * u, 22.0 * u, Color::new(1.0, 1.0, 1.0, 0.85), 0);
                    let (bx, bw) = (sw * 0.5 - 260.0 * u, 520.0 * u);
                    ui::rrect(bx, m + 22.0 * u, bw, 3.0 * u, 8.0 * u, Color::new(1.0, 1.0, 1.0, 0.2));
                    ui::rrect(bx, m + 22.0 * u, bw * progress.clamp(0.0, 1.0), 8.0 * u, 3.0 * u, pal.accent);
                    render::vtext("HIGHWAY", sw - m, m + 26.0 * u, 26.0 * u, pal.secondary, 1);
                    if r.hot() && !r.done {
                        let pulse = 0.7 + 0.3 * (app.rend.time * 8.0).sin();
                        render::vtext("OVERDRIVE  X2", sw - m, m + 54.0 * u, 18.0 * u, Color::new(pal.accent.r, pal.accent.g, pal.accent.b, pulse), 1);
                    }
                    for i in 0..r.max_hp {
                        let (cx, cy) = (m + 16.0 * u + i as f32 * 34.0 * u, sh - m - 14.0 * u);
                        if i < r.hp {
                            draw_poly(cx, cy, 4, 12.0 * u, 0.0, pal.primary);
                        } else {
                            draw_poly_lines(cx, cy, 4, 12.0 * u, 0.0, 1.5 * u, Color::new(pal.primary.r, pal.primary.g, pal.primary.b, 0.4));
                        }
                    }
                    if r.label_t > 0.0 && !r.done {
                        let a = (r.label_t / 0.25).min(1.0);
                        let col = if r.label == "HIT" { pal.danger } else if r.label == "CLOSE" { pal.secondary } else { pal.accent };
                        render::vtext(r.label, sw * 0.5, sh * 0.70, 40.0 * u, Color::new(col.r, col.g, col.b, a), 0);
                    }
                    if r.t < 6.0 && !r.done {
                        text_shadow("A / D  change lane        SPACE  jump the bars        ESC  menu", sw * 0.5, sh * 0.62, 24.0 * u, Color::new(1.0, 1.0, 1.0, 0.8), 0);
                    }
                    if r.done {
                        dim(0.6);
                        render::vtext(if r.dead { "CRASHED" } else { "ROAD COMPLETE" }, sw * 0.5, sh * 0.24, 80.0 * u, if r.dead { pal.danger } else { pal.accent }, 0);
                        text_center(&format!("{:08}", r.score), sw * 0.5, sh * 0.36, 96.0 * u, WHITE);
                        text_center(&format!("RANK  {}", r.rank()), sw * 0.5 - 330.0 * u, sh * 0.34, 40.0 * u, pal.primary);
                        if s.best {
                            text_center("NEW BEST", sw * 0.5, sh * 0.42, 34.0 * u, pal.accent);
                        } else if let Some((b, _)) = app.scores.get(&s.key) {
                            text_center(&format!("best {b}"), sw * 0.5, sh * 0.42, 24.0 * u, Color::new(1.0, 1.0, 1.0, 0.6));
                        }
                        let rows = [
                            ("obstacles cleared", format!("{}", r.cleared)),
                            ("perfect jumps", format!("{} of {}", r.perfects, r.bars_total)),
                            ("close calls", format!("{}", r.close_calls)),
                            ("gems", format!("{}", r.gems)),
                            ("best combo", format!("{}", r.best_combo)),
                            ("hits taken", format!("{}", r.hits)),
                        ];
                        let mut y = sh * 0.50;
                        for (k, v) in rows {
                            text_left(k, sw * 0.5 - 230.0 * u, y, 28.0 * u, Color::new(1.0, 1.0, 1.0, 0.75));
                            let w = text_w(&v, 28.0 * u);
                            text_left(&v, sw * 0.5 + 230.0 * u - w, y, 28.0 * u, WHITE);
                            y += 36.0 * u;
                        }
                        text_center("ENTER  again        ESC  menu", sw * 0.5, sh * 0.82, 28.0 * u, pal.accent);
                        again = is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter);
                    }
                } else {
                    leave = true;
                }
                if leave {
                    app.to_menu();
                } else if again {
                    app.runner = None;
                    app.start_highway();
                }
            }
            Screen::Codex => {
                let nt = CODEX_TABS.len();
                let mut tab = app.menu_sel.min(nt - 1);
                let x = 110.0 * u;
                let time = app.rend.time;
                let dim_w = |a: f32| Color::new(1.0, 1.0, 1.0, a);
                let mut tip = ui::Tip::default();
                // ---- pages: an icon each; Q / E or LEFT / RIGHT or a click ----
                let mut to_tab: Option<usize> = None;
                let dirk = (is_key_pressed(KeyCode::Right) || is_key_pressed(KeyCode::E)) as i32 - (is_key_pressed(KeyCode::Left) || is_key_pressed(KeyCode::Q)) as i32;
                if dirk != 0 {
                    to_tab = Some((tab as i32 + dirk).rem_euclid(nt as i32) as usize);
                }
                if codex_items(tab).is_empty() {
                    dim(0.7);
                }
                render::vtext("CODEX", x, 118.0 * u, 54.0 * u, WHITE, -1);
                for (i, t) in CODEX_TABS.iter().enumerate() {
                    let tx = x + i as f32 * 72.0 * u;
                    let (hv, click) = ui::tile(tx, 142.0 * u, 62.0 * u, i == tab, false, pal.primary, 1.0, u);
                    ui::icon(t.0, tx + 31.0 * u, 173.0 * u, 15.0 * u, if i == tab { WHITE } else if hv { pal.primary } else { dim_w(0.7) });
                    if hv {
                        tip.set(t.1, "", true);
                    }
                    if click {
                        to_tab = Some(i);
                    }
                }
                if let Some(i) = to_tab.filter(|i| *i != tab) {
                    tab = i;
                    app.menu_sel = i;
                    app.codex_sel = 0;
                    app.codex_top = 0;
                    app.audio.sfx(Sfx::UiMove, 0.6, 0.0);
                }
                text_left(CODEX_TABS[tab].1, x, 246.0 * u, 30.0 * u, pal.primary);
                let items = codex_items(tab);
                let soft = dim_w(0.85);
                if !items.is_empty() {
                    // ---- the entries: click one, or UP / DOWN ----
                    let n = items.len();
                    let (lx, ly, lw, rh) = (x, 266.0 * u, 500.0 * u, 48.0 * u);
                    let fit = (((sh - 120.0 * u - ly - 20.0 * u) / rh).floor().max(4.0) as usize).min(n);
                    ui::panel(lx, ly, lw, fit as f32 * rh + 20.0 * u, u);
                    let over_list = ui::hover(lx, ly, lw, fit as f32 * rh + 20.0 * u);
                    let before = app.codex_sel.min(n - 1);
                    let mut sel = before;
                    let dv = is_key_pressed(KeyCode::Down) as i32 - is_key_pressed(KeyCode::Up) as i32;
                    sel = (sel as i32 + dv).clamp(0, n as i32 - 1) as usize;
                    let mut top = app.codex_top.min(n - fit);
                    let (_, wheel) = mouse_wheel();
                    if over_list && wheel != 0.0 {
                        top = (top as i32 - wheel.signum() as i32).clamp(0, (n - fit) as i32) as usize;
                    } else if dv != 0 {
                        // keep the chosen entry in view
                        top = top.min(sel).max((sel + 1).saturating_sub(fit));
                    }
                    for (r, k) in (top..top + fit).enumerate() {
                        let ry = ly + 10.0 * u + r as f32 * rh;
                        let on = k == sel;
                        let hv = ui::hover(lx + 8.0 * u, ry, lw - 16.0 * u, rh - 4.0 * u);
                        if on {
                            ui::rrect(lx + 8.0 * u, ry, lw - 16.0 * u, rh - 4.0 * u, 8.0 * u, Color::new(pal.secondary.r, pal.secondary.g, pal.secondary.b, 0.5));
                            ui::rrect_lines(lx + 8.0 * u, ry, lw - 16.0 * u, rh - 4.0 * u, 8.0 * u, 1.5 * u, Color::new(pal.accent.r, pal.accent.g, pal.accent.b, 0.8));
                        } else if hv {
                            ui::rrect(lx + 8.0 * u, ry, lw - 16.0 * u, rh - 4.0 * u, 8.0 * u, dim_w(0.07));
                        }
                        let col = if on { WHITE } else { dim_w(if hv { 0.95 } else { 0.72 }) };
                        let ic = vec2(lx + 36.0 * u, ry + rh * 0.5 - 2.0 * u);
                        match items[k].demo {
                            Demo::Upgrade(up) => render::upgrade_emblem(up, ic, 12.0 * u, 2.0 * u, col, if up.rare() { pal.danger } else { pal.accent }, time),
                            Demo::Enemy(kind) => app.rend.draw_kind_icon(kind, ic, 12.0 * u, if on { pal.accent } else { col }),
                            _ => ui::icon(CODEX_TABS[tab].0, ic.x, ic.y, 10.0 * u, if on { pal.accent } else { col }),
                        }
                        text_left(&items[k].name, lx + 66.0 * u, ry + rh * 0.5 + 6.0 * u, 24.0 * u, col);
                        if let Demo::Upgrade(up) = items[k].demo {
                            if up.rare() {
                                render::text_right("rare", lx + lw - 24.0 * u, ry + rh * 0.5 + 5.0 * u, 17.0 * u, pal.danger);
                            }
                        }
                        if hv && is_mouse_button_pressed(MouseButton::Left) {
                            sel = k;
                        }
                    }
                    if n > fit {
                        // where the list is scrolled to
                        let track = fit as f32 * rh;
                        let (bh, by) = (track * fit as f32 / n as f32, track * top as f32 / n as f32);
                        ui::rrect(lx + lw - 9.0 * u, ly + 10.0 * u + by, 4.0 * u, bh, 2.0 * u, dim_w(0.35));
                    }
                    if sel != before {
                        app.audio.sfx(Sfx::UiMove, 0.6, 0.0);
                    }
                    app.codex_sel = sel;
                    app.codex_top = top;
                    // ---- the chosen entry: what it is, under the arena where it is being shown ----
                    let it = &items[sel];
                    let (cw, ch) = (780.0 * u, 196.0 * u);
                    let (cx, cy) = (sw - 110.0 * u - cw, sh - 120.0 * u - ch);
                    ui::panel(cx, cy, cw, ch, u);
                    let cell = 112.0 * u;
                    ui::rrect(cx + 20.0 * u, cy + 20.0 * u, cell, cell, 12.0 * u, dim_w(0.06));
                    let cc = vec2(cx + 20.0 * u + cell * 0.5, cy + 20.0 * u + cell * 0.5);
                    match it.demo {
                        Demo::Upgrade(up) => render::upgrade_emblem(up, cc, 34.0 * u, 3.5 * u, WHITE, if up.rare() { pal.danger } else { pal.accent }, time),
                        Demo::Enemy(kind) => {
                            let c = match kind {
                                EnemyKind::Darter | EnemyKind::Bomber => pal.accent,
                                EnemyKind::Orbiter => pal.primary,
                                EnemyKind::Turret | EnemyKind::Elite => pal.danger,
                                EnemyKind::Wisp | EnemyKind::Sprite => Color::new(1.0, 0.95, 0.75, 1.0),
                                _ => pal.secondary,
                            };
                            app.rend.draw_kind_icon(kind, cc, 38.0 * u, c);
                        }
                        Demo::Pattern(p) => app.rend.draw_pattern_icon(p, cc, 44.0 * u, pal.danger, pal.primary),
                        _ => ui::icon(CODEX_TABS[tab].0, cc.x, cc.y, 30.0 * u, pal.accent),
                    }
                    let tx = cx + 40.0 * u + cell;
                    text_left(&it.name, tx, cy + 52.0 * u, 36.0 * u, pal.accent);
                    render::text_right(&it.tag, cx + cw - 24.0 * u, cy + 48.0 * u, 18.0 * u, dim_w(0.5));
                    for (i, l) in ui::wrap(&it.body, 22.0 * u, cw - cell - 70.0 * u).iter().take(5).enumerate() {
                        text_left(l, tx, cy + 88.0 * u + i as f32 * 27.0 * u, 22.0 * u, soft);
                    }
                    text_shadow("live: the bot is playing it", cx + cw - 24.0 * u, cy - 14.0 * u, 18.0 * u, dim_w(0.5), 1);
                    app.show_want = it.demo;
                } else {
                    app.show_want = Demo::None;
                    let y0 = 330.0 * u;
                    ui::panel(x - 24.0 * u, 266.0 * u, sw - 2.0 * x + 48.0 * u, sh - 266.0 * u - 120.0 * u, u);
                    if tab == 6 {
                        let have = MILESTONES.iter().filter(|m| app.scores.contains_key(&format!("ACH|{}", m.0))).count();
                        render::text_right(&format!("{have} of {} earned", MILESTONES.len()), sw - x, 246.0 * u, 20.0 * u, dim_w(0.6));
                        for (k, m) in MILESTONES.iter().enumerate() {
                            let y = y0 + k as f32 * 48.0 * u;
                            let done = app.scores.contains_key(&format!("ACH|{}", m.0));
                            let c = if done { pal.accent } else { dim_w(0.35) };
                            draw_poly_lines(x + 14.0 * u, y - 8.0 * u, 4, 13.0 * u, 0.0, 2.0 * u, c);
                            if done {
                                draw_poly(x + 14.0 * u, y - 8.0 * u, 4, 7.0 * u, 0.0, c);
                            }
                            text_left(m.1, x + 44.0 * u, y, 26.0 * u, c);
                            text_left(m.2, x + 380.0 * u, y, 22.0 * u, if done { soft } else { dim_w(0.5) });
                        }
                    } else {
                        let mut rows: Vec<(&String, &(u64, u32))> = app.scores.iter().filter(|(k, _)| !k.starts_with("ACH|")).collect();
                        rows.sort_by(|a, b| (b.1 .1, b.1 .0).cmp(&(a.1 .1, a.1 .0)));
                        if rows.is_empty() {
                            text_left("No scores yet. Finish a run or a Highway road and it will be kept here.", x, y0, 24.0 * u, soft);
                        }
                        text_left("track", x, y0 - 34.0 * u, 18.0 * u, dim_w(0.5));
                        for (k, (key, v)) in rows.iter().take(13).enumerate() {
                            let y = y0 + k as f32 * 40.0 * u;
                            let mut it = key.split('|');
                            let (title, tier, mode) = (it.next().unwrap_or(""), it.next().unwrap_or(""), it.next().unwrap_or(""));
                            let title = ui::fit(title, 24.0 * u, 620.0 * u);
                            text_left(&title, x, y, 24.0 * u, WHITE);
                            text_left(tier, x + 660.0 * u, y, 22.0 * u, pal.primary);
                            text_left(mode, x + 880.0 * u, y, 22.0 * u, pal.secondary);
                            text_left(&format!("{}", v.0), x + 1090.0 * u, y, 26.0 * u, pal.accent);
                            if mode != "HIGHWAY" {
                                text_left(&format!("depth {}", v.1), x + 1280.0 * u, y, 20.0 * u, soft);
                            }
                        }
                    }
                }
                let hy = sh - 62.0 * u;
                let mut kx = x;
                for (k, what) in [("Q  E", "page"), ("UP  DOWN", "entry"), ("ESC", "back")] {
                    kx = ui::keycap(k, what, kx, hy + 34.0 * u, u);
                }
                tip.draw(x, hy, u, &pal);
                if is_key_pressed(KeyCode::Escape) {
                    app.codex_leave();
                }
            }
            Screen::Tracks => {
                dim(0.75);
                let x = 110.0 * u;
                render::vtext("PICK A TRACK", x, 130.0 * u, 64.0 * u, WHITE, -1);
                let n = app.juke.list.len();
                let rows = 16usize;
                let mut pick: Option<usize> = None;
                if n > 0 {
                    let step = is_key_pressed(KeyCode::Down) as i32 - is_key_pressed(KeyCode::Up) as i32 + (is_key_pressed(KeyCode::PageDown) as i32 - is_key_pressed(KeyCode::PageUp) as i32) * rows as i32;
                    let (_, wheel) = mouse_wheel();
                    let step = step - wheel.signum() as i32 * 3 * (wheel != 0.0) as i32;
                    if step != 0 {
                        app.track_sel = (app.track_sel as i32 + step).clamp(0, n as i32 - 1) as usize;
                    }
                    // typing a letter jumps to the next title that starts with it
                    while let Some(c) = get_char_pressed() {
                        if c.is_alphanumeric() {
                            let want = c.to_ascii_lowercase();
                            if let Some(k) = (1..=n).map(|d| (app.track_sel + d) % n).find(|&k| track_title(&app.juke.list[k]).to_lowercase().starts_with(want)) {
                                app.track_sel = k;
                            }
                        }
                    }
                    let top = app.track_sel.saturating_sub(rows / 2).min(n.saturating_sub(rows));
                    for (r, k) in (top..(top + rows).min(n)).enumerate() {
                        let y = 190.0 * u + r as f32 * 46.0 * u;
                        let hv = ui::hover(x, y, 1300.0 * u, 42.0 * u);
                        if hv && mouse_delta_position() != Vec2::ZERO {
                            app.track_sel = k;
                        }
                        let on = k == app.track_sel;
                        if on {
                            ui::rrect(x, y, 1300.0 * u, 42.0 * u, 8.0 * u, Color::new(pal.secondary.r, pal.secondary.g, pal.secondary.b, 0.38));
                            ui::rrect_lines(x, y, 1300.0 * u, 42.0 * u, 8.0 * u, 1.5 * u, Color::new(pal.accent.r, pal.accent.g, pal.accent.b, 0.7));
                        }
                        let title = track_title(&app.juke.list[k]);
                        let playing = k == app.juke.idx;
                        text_left(&ui::fit(&title, 24.0 * u, 930.0 * u), x + 18.0 * u, y + 29.0 * u, 24.0 * u, if playing { pal.accent } else if on { WHITE } else { Color::new(1.0, 1.0, 1.0, 0.72) });
                        if let Some((s, d)) = app.scores.get(&app.score_key(&title)) {
                            text_left(&format!("best {s}   depth {d}"), x + 980.0 * u, y + 29.0 * u, 20.0 * u, Color::new(1.0, 1.0, 1.0, 0.6));
                        }
                        if hv && is_mouse_button_pressed(MouseButton::Left) {
                            pick = Some(k);
                        }
                    }
                    text_left(&format!("{} of {}", app.track_sel + 1, n), x + 1100.0 * u, 130.0 * u, 22.0 * u, Color::new(1.0, 1.0, 1.0, 0.6));
                    if is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
                        pick = Some(app.track_sel);
                    }
                } else {
                    text_left("No music found. Set a music library with the note button under the track preview.", x, 220.0 * u, 24.0 * u, WHITE);
                }
                text_shadow("UP / DOWN  move     type a letter to jump     ENTER  choose     ESC  back", x, sh - 60.0 * u, 22.0 * u, Color::new(1.0, 1.0, 1.0, 0.7), -1);
                if let Some(k) = pick {
                    if k != app.juke.idx {
                        app.juke.idx = k;
                        app.juke.loader = None;
                        app.juke.fails = 0;
                        app.juke.played = 0.0;
                    }
                    app.audio.sfx(Sfx::UiSelect, 0.8, 0.0);
                    app.screen = app.tracks_back;
                    app.menu_sel = if app.tracks_back == Screen::Setup { S_TRACK } else { M_PLAY };
                } else if is_key_pressed(KeyCode::Escape) {
                    app.screen = app.tracks_back;
                    app.menu_sel = if app.tracks_back == Screen::Setup { S_TRACK } else { M_PLAY };
                }
            }
            Screen::Lan => app.lan_ui(u, sw, sh),
            Screen::Play => {
                if app.launch < LAUNCH_SECS {
                    ui::heartbeat_iris(app.launch / LAUNCH_SECS, &pal);
                }
            }
        }

        if app.message_t > 0.0 && !app.message.is_empty() {
            let a = app.message_t.min(1.0);
            text_shadow(&app.message, sw * 0.5, sh * 0.92, 24.0 * u, Color::new(pal.danger.r, pal.danger.g, pal.danger.b, a), 0);
        }

        // ---- self-test hooks ----
        if let Some(dir) = app.args.selftest.clone() {
            app.max_bullets = app.max_bullets.max(app.world.bullets.len());
            app.max_enemies = app.max_enemies.max(app.world.enemies.len());
            let st = app.song.as_ref().map(|s| s.last_t).unwrap_or(-1.0);
            let key = if app.song.is_some() || app.live.is_some() { if app.live.is_some() { app.wall } else { st } } else { -1.0 };
            let next = app.args.shots.get(app.shots_done).copied();
            let menu_shot = app.args.wall || (app.args.files.is_empty() && !app.args.live);
            let due = match next {
                Some(tn) => (menu_shot && app.wall >= tn) || (!menu_shot && key >= tn),
                None => false,
            };
            if due {
                let tn = next.unwrap();
                let path = dir.join(format!("shot_{:03}_{:06.2}.png", app.shots_done, tn));
                get_screen_data().export_png(&path.to_string_lossy());
                app.log.push(format!(
                    "shot {} at t={:.2} wall={:.2} idle={:.2} bullets={} enemies={} score={} hp={} fps={:.0}",
                    app.shots_done,
                    key,
                    app.wall,
                    app.idle,
                    app.world.bullets.len(),
                    app.world.enemies.len(),
                    app.world.score,
                    app.world.player.hp,
                    get_fps()
                ));
                app.shots_done += 1;
            }
            // A LAN client waits (a little) for the host's End so both logs hold the final score.
            let lan_wait = app.lan.as_ref().is_some_and(|l| !l.host && l.in_match()) && app.wall < app.args.secs + 10.0;
            let over = (app.wall >= app.args.secs && !lan_wait) || app.screen == Screen::Results && app.wall > 3.0 && app.shots_done >= app.args.shots.len();
            if over {
                if let Some(lan) = app.lan.as_mut() {
                    let stage = lan.stage;
                    lan.note(format!("selftest end stage={stage:?} result={}-{}", lan.result[0], lan.result[1]));
                }
                app.log.push(format!(
                    "done wall={:.1}s frames={} avg_fps={:.1} max_bullets={} max_enemies={} score={} kills={} hits={} screen={} status='{}'",
                    app.wall,
                    app.frames,
                    app.frames as f32 / app.wall.max(0.01),
                    app.max_bullets,
                    app.max_enemies,
                    app.world.score,
                    app.world.stats.kills,
                    app.world.stats.hits,
                    app.screen as i32,
                    app.audio.status()
                ));
                let _ = std::fs::write(dir.join("selftest.log"), app.log.join("\n"));
                break;
            }
        }

        next_frame().await;
    }
    app.save_settings();
    app.audio.stop();
    app.audio.stop_live();
}
