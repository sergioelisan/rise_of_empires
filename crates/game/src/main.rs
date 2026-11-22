mod audio;
mod campaign_ui;
mod hud;
mod icons;
mod menu;
mod netplay;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use roeRender::{PlayerControlState, RoeRenderPlugin, SimState};
use roeSim::{
    AiDifficulty, CampaignState, Command, GameData, MapGrid, PlayerId, SimWorld, TerrainType,
    Vec2Fx,
};

use crate::campaign_ui::ActiveCampaignState;
use crate::menu::{AppMenuState, ScreenMode};

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

    let startInCampaign = std::env::var("ROE_CAMPAIGN").is_ok();
    let demoPhase10 = std::env::var("ROE_DEMO_P10").is_ok();
    let initialMode = if startInCampaign {
        ScreenMode::CampaignMap
    } else if demoPhase10
        || (std::env::var("ROE_MENU").is_err()
            && (std::env::var("ROE_QUICKSTART").is_ok() || std::env::var("ROE_NATION").is_ok()))
    {
        ScreenMode::Playing
    } else {
        ScreenMode::MainMenu
    };

    let map = MapGrid::generate(MAP_SIZE, MAP_SIZE, mapSeed);
    let world = if demoPhase10 {
        // Cena demonstrativa da Fase 10: enseada costeira com Estaleiro/navios + Aerodromo/aeronave.
        let mut w = SimWorld::newMatchWithAi(
            map,
            data,
            &playerNationId,
            &enemyNationId,
            AiDifficulty::Passive,
        );
        let cap = w.cities.iter().find(|c| c.owner == PlayerId(0)).unwrap().centerTile;
        for dy in 2..=9u32 {
            for dx in 2..=10u32 {
                let x = (cap.0 + dx).min(w.map.width - 2);
                let y = (cap.1 + dy).min(w.map.height - 2);
                let tile = if dx <= 4 || dy <= 3 { TerrainType::Sand } else { TerrainType::Water };
                w.map.setTile(x, y, tile);
            }
        }
        if let Some(p) = w.playerMut(PlayerId(0)) {
            p.age = 2;
            p.resources.food = 1500;
            p.resources.wood = 1500;
            p.resources.metal = 1500;
            p.resources.wealth = 1500;
            p.resources.knowledge = 1500;
        }
        let waterPos1 = Vec2Fx::tileCenter(cap.0 + 6, cap.1 + 6);
        let waterPos2 = Vec2Fx::tileCenter(cap.0 + 8, cap.1 + 7);
        let airPos = Vec2Fx::tileCenter(cap.0 + 4, cap.1 + 4);
        w.step(&[
            Command::SpawnTypedUnit { owner: PlayerId(0), kind: "galley".into(), pos: waterPos1 },
            Command::SpawnTypedUnit { owner: PlayerId(0), kind: "warship".into(), pos: waterPos2 },
            Command::SpawnTypedUnit { owner: PlayerId(0), kind: "airship".into(), pos: airPos },
        ]);
        w
    } else {
        SimWorld::newMatchWithAi(map, data, &playerNationId, &enemyNationId, aiDifficulty)
    };
    let noFog = std::env::var("ROE_NO_FOG").is_ok() || demoPhase10;

    let mut initialCampaign = ActiveCampaignState::default();
    if startInCampaign {
        initialCampaign.campaign = Some(CampaignState::new(&playerNationId));
        initialCampaign.dirty = true;
    }

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title: "Rise of Empires".into(), ..default() }),
            ..default()
        }))
        .insert_resource(SimState(world))
        .insert_resource(AppMenuState::new(
            initialMode,
            playerIdx,
            enemyIdx,
            aiDifficulty,
            mapSeed,
        ))
        .add_plugins((
            RoeRenderPlugin,
            audio::AudioPlugin,
            netplay::NetPlayPlugin,
            campaign_ui::CampaignPlugin,
            menu::MenuPlugin,
            hud::HudPlugin,
        ))
        .insert_resource(initialCampaign)
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
