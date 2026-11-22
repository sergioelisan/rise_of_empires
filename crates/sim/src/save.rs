use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::data::GameData;
use crate::world::SimWorld;

const SAVE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct SaveEnvelope {
    version: u32,
    world: SimWorld,
}

/// Serializa o estado completo da simulação em RON determinístico (ponto fixo bit-a-bit).
pub fn serializeWorld(world: &SimWorld) -> Result<String, String> {
    let envelope = SaveEnvelope { version: SAVE_VERSION, world: world.clone() };
    ron::ser::to_string_pretty(&envelope, ron::ser::PrettyConfig::default())
        .map_err(|e| format!("falha ao serializar save: {e}"))
}

/// Desserializa um save em RON e recompõe os caches derivados (`GameData`, `blockedTiles`, flow cache).
pub fn deserializeWorld(ronStr: &str, data: GameData) -> Result<SimWorld, String> {
    let envelope: SaveEnvelope =
        ron::from_str(ronStr).map_err(|e| format!("falha ao ler save RON: {e}"))?;
    if envelope.version != SAVE_VERSION {
        return Err(format!("versão de save incompatível: {}", envelope.version));
    }
    let mut world = envelope.world;
    world.restoreTransientState(data);
    Ok(world)
}

pub fn saveWorldToFile(world: &SimWorld, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("falha ao criar pasta de saves: {e}"))?;
    }
    let content = serializeWorld(world)?;
    std::fs::write(path, content).map_err(|e| format!("falha ao gravar save: {e}"))
}

pub fn loadWorldFromFile(path: &Path, data: GameData) -> Result<SimWorld, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("falha ao abrir save: {e}"))?;
    deserializeWorld(&content, data)
}
