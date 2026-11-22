use serde::{Deserialize, Serialize};

use crate::command::{BuildingId, CityId, PlayerId};
use crate::map::MapGrid;
use crate::math::Vec2Fx;

/// Estado de visibilidade de um tile na névoa de guerra.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FogState {
    #[default]
    Unexplored,
    Explored,
    Visible,
}

/// Cidade âncora da economia e das fronteiras nacionais.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct City {
    pub id: CityId,
    pub owner: PlayerId,
    pub name: String,
    pub centerTile: (u32, u32),
    pub buildingId: BuildingId,
    /// Raio base da cidade (cresce com eras, tecnologias e bônus de nação).
    pub baseRadius: u32,
    /// Progresso de captura quando a cidade está com 0 HP (0..100).
    pub captureProgress: u32,
    pub capturingPlayer: Option<PlayerId>,
}

/// Mapa de fronteiras nacionais por tile.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerritoryMap {
    pub width: u32,
    pub height: u32,
    owners: Vec<Option<PlayerId>>,
    pub version: u64,
}

impl TerritoryMap {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height, owners: vec![None; (width * height) as usize], version: 1 }
    }

    /// Recalcula a posse de cada tile a partir de todas as fontes de fronteira
    /// `(owner, centerTile, effectiveRadius)`. O dono mais próximo dentro do raio vence.
    pub fn recompute(&mut self, sources: &[(PlayerId, (u32, u32), u32)]) {
        self.owners.fill(None);
        for ty in 0..self.height {
            for tx in 0..self.width {
                let mut best: Option<(i64, PlayerId)> = None;
                for &(owner, (sx, sy), radius) in sources {
                    if radius == 0 {
                        continue;
                    }
                    let dx = tx as i64 - sx as i64;
                    let dy = ty as i64 - sy as i64;
                    let distSq = dx * dx + dy * dy;
                    let rSq = i64::from(radius) * i64::from(radius);
                    if distSq <= rSq {
                        // Desempate determinístico pelo PlayerId menor.
                        let cand = (distSq, owner);
                        if best.is_none_or(|b| cand < b) {
                            best = Some(cand);
                        }
                    }
                }
                if let Some((_, owner)) = best {
                    self.owners[(ty * self.width + tx) as usize] = Some(owner);
                }
            }
        }
        self.version += 1;
    }

    pub fn ownerAtTile(&self, tx: u32, ty: u32) -> Option<PlayerId> {
        if tx >= self.width || ty >= self.height {
            return None;
        }
        self.owners[(ty * self.width + tx) as usize]
    }

    pub fn ownerAtPos(&self, map: &MapGrid, pos: Vec2Fx) -> Option<PlayerId> {
        if self.width == 0 || self.height == 0 {
            return None;
        }
        let clamped = map.clampPoint(pos);
        let tx = (clamped.x.floor().to_num::<i32>()).clamp(0, self.width as i32 - 1) as u32;
        let ty = (clamped.y.floor().to_num::<i32>()).clamp(0, self.height as i32 - 1) as u32;
        self.ownerAtTile(tx, ty)
    }

    pub fn owners(&self) -> &[Option<PlayerId>] {
        &self.owners
    }
}

/// Atualiza a névoa de guerra de um jogador com base nas fontes de visão `(centerTile, radius)`.
pub fn updateFog(
    width: u32,
    height: u32,
    fog: &mut [FogState],
    sources: &[((u32, u32), u32)],
    revealAll: bool,
) {
    for state in fog.iter_mut() {
        if revealAll {
            if *state == FogState::Unexplored {
                *state = FogState::Explored;
            }
            if *state == FogState::Visible {
                *state = FogState::Explored;
            }
        } else if *state == FogState::Visible {
            *state = FogState::Explored;
        }
    }

    for &((cx, cy), radius) in sources {
        let r = radius as i32;
        let rSq = r * r;
        let minX = (cx as i32 - r).max(0);
        let maxX = (cx as i32 + r).min(width as i32 - 1);
        let minY = (cy as i32 - r).max(0);
        let maxY = (cy as i32 + r).min(height as i32 - 1);
        for ty in minY..=maxY {
            for tx in minX..=maxX {
                let dx = tx - cx as i32;
                let dy = ty - cy as i32;
                if dx * dx + dy * dy <= rSq {
                    fog[(ty as u32 * width + tx as u32) as usize] = FogState::Visible;
                }
            }
        }
    }
}
