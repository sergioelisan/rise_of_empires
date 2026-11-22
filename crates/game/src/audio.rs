use std::f32::consts::TAU;
use std::path::PathBuf;
use std::process::{Command as ProcCommand, Stdio};

use bevy::prelude::*;
use roeRender::{PlayerControlState, SimState, WorldResetCounter};

const SAMPLE_RATE: u32 = 22_050;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sfx {
    Click,
    Order,
    Build,
    Complete,
    Combat,
    AgeUp,
    Victory,
}

#[derive(Resource)]
pub struct SoundBank {
    pub muted: bool,
    playerBin: Option<&'static str>,
    dir: PathBuf,
    lastPlayed: [f32; 7],
}

impl SoundBank {
    pub fn init(muted: bool) -> Self {
        let playerBin = if PathBuf::from("/usr/bin/pw-play").exists() {
            Some("/usr/bin/pw-play")
        } else if PathBuf::from("/usr/bin/aplay").exists() {
            Some("/usr/bin/aplay")
        } else {
            None
        };

        let dir = PathBuf::from("/tmp/roe_sfx");
        let _ = std::fs::create_dir_all(&dir);
        if playerBin.is_some() {
            let _ = writeWav(&dir.join("click.wav"), &synthClick());
            let _ = writeWav(&dir.join("order.wav"), &synthOrder());
            let _ = writeWav(&dir.join("build.wav"), &synthBuild());
            let _ = writeWav(&dir.join("complete.wav"), &synthComplete());
            let _ = writeWav(&dir.join("combat.wav"), &synthCombat());
            let _ = writeWav(&dir.join("age.wav"), &synthFanfare(false));
            let _ = writeWav(&dir.join("victory.wav"), &synthFanfare(true));
        }

        Self { muted, playerBin, dir, lastPlayed: [-10.0; 7] }
    }

    pub fn play(&mut self, sfx: Sfx, nowSecs: f32) {
        if self.muted {
            return;
        }
        let Some(bin) = self.playerBin else { return };
        let idx = sfx as usize;
        let minGap = match sfx {
            Sfx::Combat => 0.35,
            Sfx::Click => 0.06,
            _ => 0.14,
        };
        if nowSecs - self.lastPlayed[idx] < minGap {
            return;
        }
        self.lastPlayed[idx] = nowSecs;

        let file = match sfx {
            Sfx::Click => "click.wav",
            Sfx::Order => "order.wav",
            Sfx::Build => "build.wav",
            Sfx::Complete => "complete.wav",
            Sfx::Combat => "combat.wav",
            Sfx::AgeUp => "age.wav",
            Sfx::Victory => "victory.wav",
        };
        let path = self.dir.join(file);
        if !path.exists() {
            return;
        }
        let mut cmd = ProcCommand::new(bin);
        if bin.ends_with("aplay") {
            cmd.arg("-q");
        }
        let _ = cmd
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}

/// Mensagem breve de aviso no topo da tela (save, era, maravilha, cidade capturada).
#[derive(Resource, Default)]
pub struct ToastNotification {
    pub message: String,
    pub remainingSecs: f32,
}

impl ToastNotification {
    pub fn show(&mut self, msg: impl Into<String>) {
        self.message = msg.into();
        self.remainingSecs = 3.5;
    }
}

#[derive(Resource, Default)]
struct AudioTracker {
    lastReset: u64,
    lastAge: u8,
    lastTechCount: usize,
    lastCompletedBuildings: usize,
    lastCityCount: usize,
    lastCombatTick: u64,
    hadWinner: bool,
}

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        let muted = std::env::var("ROE_SCREENSHOT").is_ok();
        app.insert_resource(SoundBank::init(muted))
            .init_resource::<ToastNotification>()
            .init_resource::<AudioTracker>()
            .add_systems(Update, (handleAudioHotkeys, observeSimAudioEvents, tickToast));
    }
}

fn handleAudioHotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut bank: ResMut<SoundBank>,
    mut toast: ResMut<ToastNotification>,
) {
    if keys.just_pressed(KeyCode::KeyM) {
        bank.muted = !bank.muted;
        if !bank.muted {
            bank.play(Sfx::Click, time.elapsed_secs());
            toast.show("Som: ATIVADO");
        } else {
            toast.show("Som: MUDO");
        }
    }
}

fn observeSimAudioEvents(
    sim: Res<SimState>,
    reset: Res<WorldResetCounter>,
    control: Res<PlayerControlState>,
    time: Res<Time>,
    mut bank: ResMut<SoundBank>,
    mut tracker: ResMut<AudioTracker>,
    mut toast: ResMut<ToastNotification>,
) {
    let world = &sim.0;
    let now = time.elapsed_secs();
    let p = world.player(control.localPlayer);
    let age = p.map_or(0, |x| x.age);
    let techs = p.map_or(0, |x| x.researchedTechs.len());
    let completed = world
        .buildings
        .iter()
        .filter(|b| b.owner == control.localPlayer && b.isComplete())
        .count();
    let cities = world.cityCount(control.localPlayer);

    if tracker.lastReset != reset.0 || world.tick <= 1 {
        tracker.lastReset = reset.0;
        tracker.lastAge = age;
        tracker.lastTechCount = techs;
        tracker.lastCompletedBuildings = completed;
        tracker.lastCityCount = cities;
        tracker.lastCombatTick = world.tick;
        tracker.hadWinner = world.winner.is_some();
        return;
    }

    if world.winner.is_some() && !tracker.hadWinner {
        tracker.hadWinner = true;
        bank.play(Sfx::Victory, now);
    }

    if age > tracker.lastAge {
        tracker.lastAge = age;
        bank.play(Sfx::AgeUp, now);
        if let Some(ageDef) = world.data.age(age) {
            toast.show(format!("Nova Era alcancada: {}!", ageDef.name));
        }
    }

    if techs > tracker.lastTechCount {
        tracker.lastTechCount = techs;
        bank.play(Sfx::Complete, now);
    }

    if cities > tracker.lastCityCount {
        tracker.lastCityCount = cities;
        bank.play(Sfx::AgeUp, now);
        toast.show("Nova cidade sob seu controle!");
    } else if completed > tracker.lastCompletedBuildings {
        tracker.lastCompletedBuildings = completed;
        bank.play(Sfx::Complete, now);
    } else {
        tracker.lastCompletedBuildings = completed;
        tracker.lastCityCount = cities;
    }

    let latestCombat = world.combatEvents.iter().map(|e| e.tick).max().unwrap_or(0);
    if latestCombat > tracker.lastCombatTick {
        tracker.lastCombatTick = latestCombat;
        bank.play(Sfx::Combat, now);
    }
}

fn tickToast(time: Res<Time>, mut toast: ResMut<ToastNotification>) {
    if toast.remainingSecs > 0.0 {
        toast.remainingSecs = (toast.remainingSecs - time.delta_secs()).max(0.0);
    }
}

// --- Síntese procedural de efeitos sonoros (WAV 16-bit mono, zero dependências externas) ---

fn writeWav(path: &PathBuf, samples: &[i16]) -> std::io::Result<()> {
    let dataLen = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + dataLen as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + dataLen).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes()); // PCM
    bytes.extend_from_slice(&1u16.to_le_bytes()); // Mono
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&dataLen.to_le_bytes());
    for &s in samples {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, bytes)
}

fn synthClick() -> Vec<i16> {
    let len = (SAMPLE_RATE as f32 * 0.045) as usize;
    (0..len)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let env = (-t * 75.0).exp();
            let s = (TAU * 720.0 * t).sin() * 0.6 + (TAU * 1440.0 * t).sin() * 0.25;
            (s * env * 9_000.0) as i16
        })
        .collect()
}

fn synthOrder() -> Vec<i16> {
    let len = (SAMPLE_RATE as f32 * 0.14) as usize;
    (0..len)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let freq = if t < 0.06 { 392.0 } else { 523.25 };
            let env = (t * 40.0).min(1.0) * (-(t * 14.0)).exp();
            let s = (TAU * freq * t).sin() * 0.7 + (TAU * freq * 2.0 * t).sin() * 0.2;
            (s * env * 9_500.0) as i16
        })
        .collect()
}

fn synthBuild() -> Vec<i16> {
    let len = (SAMPLE_RATE as f32 * 0.13) as usize;
    (0..len)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let env = (-t * 28.0).exp();
            let s = (TAU * 180.0 * t).sin() * 0.75 + (TAU * 360.0 * t).sin() * 0.25;
            (s * env * 11_000.0) as i16
        })
        .collect()
}

fn synthComplete() -> Vec<i16> {
    let len = (SAMPLE_RATE as f32 * 0.28) as usize;
    (0..len)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let freq = if t < 0.12 { 523.25 } else { 659.25 };
            let env = (t * 50.0).min(1.0) * (-t * 7.5).exp();
            let s = (TAU * freq * t).sin() * 0.75 + (TAU * freq * 1.5 * t).sin() * 0.2;
            (s * env * 9_000.0) as i16
        })
        .collect()
}

fn synthCombat() -> Vec<i16> {
    let len = (SAMPLE_RATE as f32 * 0.11) as usize;
    let mut rng = 0x1234_5678u32;
    (0..len)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = ((rng >> 16) as f32 / 32768.0) - 1.0;
            let env = (-t * 32.0).exp();
            let clang = (TAU * 980.0 * t).sin() * 0.45 + noise * 0.45;
            (clang * env * 8_000.0) as i16
        })
        .collect()
}

fn synthFanfare(grand: bool) -> Vec<i16> {
    let dur = if grand { 0.85 } else { 0.55 };
    let len = (SAMPLE_RATE as f32 * dur) as usize;
    let notes = if grand {
        [392.0, 493.88, 587.33, 783.99]
    } else {
        [329.63, 392.0, 493.88, 659.25]
    };
    let step = dur / notes.len() as f32;
    (0..len)
        .map(|i| {
            let t = i as f32 / SAMPLE_RATE as f32;
            let idx = ((t / step) as usize).min(notes.len() - 1);
            let localT = t - idx as f32 * step;
            let freq = notes[idx];
            let env = (localT * 45.0).min(1.0) * (-(localT * 3.8)).exp();
            let brass = (TAU * freq * t).sin() * 0.65
                + (TAU * freq * 2.0 * t).sin() * 0.25
                + (TAU * freq * 3.0 * t).sin() * 0.10;
            (brass * env * 10_500.0) as i16
        })
        .collect()
}
