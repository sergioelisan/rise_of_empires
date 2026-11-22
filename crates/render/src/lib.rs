//! Apresentação: lê o estado da simulação e desenha. Nunca altera a simulação
//! diretamente — só enfileira `Command`s em `CommandQueue`.

pub mod bridge;
pub mod buildings;
pub mod camera;
pub mod input;
pub mod minimap;
pub mod models;
pub mod terrain;
pub mod units;

use bevy::prelude::*;
use roeSim::{MapGrid, Vec2Fx};

pub use bridge::{CommandQueue, HudVisible, SimPaused, SimState, WorldResetCounter};
pub use camera::IsoCamera;
pub use input::{CommandMode, PlayerControlState};

pub struct RoeRenderPlugin;

impl Plugin for RoeRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            bridge::BridgePlugin,
            input::InputPlugin,
            camera::IsoCameraPlugin,
            terrain::TerrainPlugin,
            buildings::BuildingsPlugin,
            units::UnitsPlugin,
            minimap::MinimapPlugin,
        ));
    }
}

/// Simulação (x, y) em tiles -> mundo Bevy (x, altura, z). 1 tile = 1 unidade.
pub fn simToWorld(map: &MapGrid, p: Vec2Fx) -> Vec3 {
    let (x, z) = p.toF32();
    let y = map.heightAt(p).to_num::<f32>();
    Vec3::new(x, y, z)
}

/// Altura visual do terreno em `(x, z)` por interpolação bilinear (somente apresentação).
pub fn terrainHeightF32(map: &MapGrid, x: f32, z: f32) -> f32 {
    if map.width == 0 || map.height == 0 {
        return 0.0;
    }
    let cx = x.clamp(0.0, map.width as f32);
    let cz = z.clamp(0.0, map.height as f32);

    let tx = (cx.floor() as u32).min(map.width - 1);
    let tz = (cz.floor() as u32).min(map.height - 1);
    let fx = (cx - tx as f32).clamp(0.0, 1.0);
    let fz = (cz - tz as f32).clamp(0.0, 1.0);

    let h00 = map.vertexHeight(tx, tz).to_num::<f32>();
    let h10 = map.vertexHeight(tx + 1, tz).to_num::<f32>();
    let h01 = map.vertexHeight(tx, tz + 1).to_num::<f32>();
    let h11 = map.vertexHeight(tx + 1, tz + 1).to_num::<f32>();

    let top = h00 + (h10 - h00) * fx;
    let bottom = h01 + (h11 - h01) * fx;
    top + (bottom - top) * fz
}
