mod audio;
mod hud;
mod icons;
mod menu;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use roeRender::{PlayerControlState, RoeRenderPlugin, SimState};
use roeSim::{AiDifficulty, GameData, MapGrid, SimWorld};

use crate::menu::AppMenuState;

const MAP_SIZE: u32 = 64;
const DEFAULT_MAP_SEED: u64 = 42;

fn main() {
    let mapSeed = std::env::var("ROE_SEED")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_MAP_SEED);
    let data = GameData::loadDefault();

    let playerNationId = std::env::var("ROE_NATION")
        .ok()
        .map(|s| s.to_lowercase())
        .filter(|id| data.nation(id).is_some())
        .unwrap_or_else(|| "roma".to_string());
    let enemyNationId = std::env::var("ROE_ENEMY")
        .ok()
        .map(|s| s.to_lowercase())
        .filter(|id| data.nation(id).is_some() && id != &playerNationId)
        .unwrap_or_else(|| {
            if playerNationId == "grecia" { "persia".to_string() } else { "grecia".to_string() }
        });

    let aiDifficulty = match std::env::var("ROE_AI").ok().as_deref() {
        Some("passive" | "passiva") => AiDifficulty::Passive,
        Some("easy" | "facil") => AiDifficulty::Easy,
        Some("hard" | "dificil") => AiDifficulty::Hard,
        _ => AiDifficulty::Normal,
    };

    let playerIdx = data
        .nations
        .iter()
        .position(|n| n.id == playerNationId)
        .unwrap_or(0);
    let enemyIdx = data
        .nations
        .iter()
        .position(|n| n.id == enemyNationId)
        .unwrap_or(1);

    // Inicia no Menu Principal por padrão; pula direto para a partida com `ROE_QUICKSTART=1` ou `ROE_NATION=...`.
    let startInMenu = std::env::var("ROE_MENU").is_ok()
        || (std::env::var("ROE_QUICKSTART").is_err() && std::env::var("ROE_NATION").is_err());

    let map = MapGrid::generate(MAP_SIZE, MAP_SIZE, mapSeed);
    let world = SimWorld::newMatchWithAi(map, data, &playerNationId, &enemyNationId, aiDifficulty);
    let noFog = std::env::var("ROE_NO_FOG").is_ok();

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Rise of Empires".into(), ..default() }),
            ..default()
        }))
        .insert_resource(SimState(world))
        .insert_resource(AppMenuState::new(
            startInMenu,
            playerIdx,
            enemyIdx,
            aiDifficulty,
            mapSeed,
        ))
        .add_plugins((RoeRenderPlugin, audio::AudioPlugin, menu::MenuPlugin, hud::HudPlugin))
        .add_systems(Startup, move |mut control: ResMut<PlayerControlState>| {
            if noFog {
                control.showFog = false;
            }
        })
        .add_systems(Update, captureScreenshot)
        .run();
}

/// Ferramenta de desenvolvimento: `ROE_SCREENSHOT=arquivo.png` salva um quadro e encerra.
fn captureScreenshot(
    mut commands: Commands,
    time: Res<Time>,
    mut taken: Local<bool>,
    mut exit: MessageWriter<AppExit>,
) {
    let Ok(path) = std::env::var("ROE_SCREENSHOT") else { return };
    if !*taken && time.elapsed_secs() > 2.0 {
        *taken = true;
        commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
    if time.elapsed_secs() > 3.2 {
        exit.write(AppExit::Success);
    }
}
