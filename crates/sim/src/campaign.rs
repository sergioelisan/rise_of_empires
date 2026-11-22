use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::ai::AiDifficulty;
use crate::data::ResourceBundle;

/// Bônus permanente concedido ao império do jogador ao conquistar uma província.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProvinceBonus {
    StartingFood(i32),
    StartingWood(i32),
    StartingMetal(i32),
    StartingWealth(i32),
    ExtraInfantry(u32),
    ExtraCavalry(u32),
    ExtraCitizen(u32),
}

impl ProvinceBonus {
    pub fn label(&self) -> String {
        match self {
            Self::StartingFood(v) => format!("+{} Alimento inicial", v),
            Self::StartingWood(v) => format!("+{} Madeira inicial", v),
            Self::StartingMetal(v) => format!("+{} Ferro inicial", v),
            Self::StartingWealth(v) => format!("+{} Riqueza inicial", v),
            Self::ExtraInfantry(n) => format!("+{} Infantaria inicial", n),
            Self::ExtraCavalry(n) => format!("+{} Cavalaria inicial", n),
            Self::ExtraCitizen(n) => format!("+{} Cidadao inicial", n),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CampaignProvince {
    pub id: usize,
    pub name: String,
    /// Posição normalizada (0..1000) no mapa estratégico.
    pub posNorm: (u16, u16),
    /// `true` se controlada pelo jogador.
    pub playerOwned: bool,
    /// Nação defensora da província (quando inimiga).
    pub nationId: String,
    pub difficulty: AiDifficulty,
    pub mapSeed: u64,
    pub neighbors: Vec<usize>,
    pub bonus: ProvinceBonus,
}

/// Campanha de conquista em mapa estratégico por turnos (8 províncias interligadas).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CampaignState {
    pub playerNation: String,
    pub turn: u32,
    pub provinces: Vec<CampaignProvince>,
    pub activeBattleProvince: Option<usize>,
}

impl CampaignState {
    pub fn new(playerNation: impl Into<String>) -> Self {
        let pNation = playerNation.into();
        let rival = |preferred: &str, fallback: &str| -> String {
            if pNation == preferred { fallback.to_string() } else { preferred.to_string() }
        };

        let provinces = vec![
            CampaignProvince {
                id: 0,
                name: "Italia".into(),
                posNorm: (160, 420),
                playerOwned: true,
                nationId: pNation.clone(),
                difficulty: AiDifficulty::Easy,
                mapSeed: 101,
                neighbors: vec![1, 2, 4],
                bonus: ProvinceBonus::StartingFood(80),
            },
            CampaignProvince {
                id: 1,
                name: "Galia".into(),
                posNorm: (240, 200),
                playerOwned: false,
                nationId: rival("grecia", "roma"),
                difficulty: AiDifficulty::Easy,
                mapSeed: 202,
                neighbors: vec![0, 2, 3],
                bonus: ProvinceBonus::StartingWood(110),
            },
            CampaignProvince {
                id: 2,
                name: "Grecia".into(),
                posNorm: (400, 430),
                playerOwned: false,
                nationId: rival("grecia", "persia"),
                difficulty: AiDifficulty::Easy,
                mapSeed: 303,
                neighbors: vec![0, 1, 3, 4],
                bonus: ProvinceBonus::ExtraInfantry(2),
            },
            CampaignProvince {
                id: 3,
                name: "Anatolia".into(),
                posNorm: (560, 250),
                playerOwned: false,
                nationId: rival("persia", "grecia"),
                difficulty: AiDifficulty::Normal,
                mapSeed: 404,
                neighbors: vec![1, 2, 5, 6],
                bonus: ProvinceBonus::StartingMetal(110),
            },
            CampaignProvince {
                id: 4,
                name: "Egito".into(),
                posNorm: (420, 700),
                playerOwned: false,
                nationId: rival("egito", "roma"),
                difficulty: AiDifficulty::Normal,
                mapSeed: 505,
                neighbors: vec![0, 2, 5],
                bonus: ProvinceBonus::StartingFood(130),
            },
            CampaignProvince {
                id: 5,
                name: "Mesopotamia".into(),
                posNorm: (640, 560),
                playerOwned: false,
                nationId: rival("persia", "egito"),
                difficulty: AiDifficulty::Normal,
                mapSeed: 606,
                neighbors: vec![3, 4, 6, 7],
                bonus: ProvinceBonus::StartingWealth(130),
            },
            CampaignProvince {
                id: 6,
                name: "Persia".into(),
                posNorm: (780, 320),
                playerOwned: false,
                nationId: rival("persia", "china"),
                difficulty: AiDifficulty::Hard,
                mapSeed: 707,
                neighbors: vec![3, 5, 7],
                bonus: ProvinceBonus::ExtraCavalry(2),
            },
            CampaignProvince {
                id: 7,
                name: "Oriente".into(),
                posNorm: (860, 560),
                playerOwned: false,
                nationId: rival("china", "egito"),
                difficulty: AiDifficulty::Hard,
                mapSeed: 808,
                neighbors: vec![5, 6],
                bonus: ProvinceBonus::ExtraCitizen(2),
            },
        ];

        Self { playerNation: pNation, turn: 1, provinces, activeBattleProvince: None }
    }

    pub fn ownedCount(&self) -> usize {
        self.provinces.iter().filter(|p| p.playerOwned).count()
    }

    pub fn isComplete(&self) -> bool {
        self.ownedCount() == self.provinces.len()
    }

    /// Uma província inimiga pode ser invadida se faz fronteira com alguma província já conquistada.
    pub fn canInvade(&self, provinceId: usize) -> bool {
        let Some(target) = self.provinces.get(provinceId) else { return false };
        if target.playerOwned {
            return false;
        }
        target
            .neighbors
            .iter()
            .any(|&nid| self.provinces.get(nid).is_some_and(|n| n.playerOwned))
    }

    /// Conquista a província após vitória na batalha em tempo real e avança o turno estratégico.
    pub fn conquerProvince(&mut self, provinceId: usize) {
        if let Some(p) = self.provinces.get_mut(provinceId) {
            p.playerOwned = true;
            p.nationId = self.playerNation.clone();
        }
        self.turn += 1;
        self.activeBattleProvince = None;
    }

    /// Soma todos os bônus de recursos e tropas extras acumulados pelas províncias controladas.
    pub fn accumulatedBonuses(&self) -> (ResourceBundle, u32, u32, u32) {
        let mut res = ResourceBundle::ZERO;
        let mut extraInf = 0u32;
        let mut extraCav = 0u32;
        let mut extraCit = 0u32;
        for p in self.provinces.iter().filter(|p| p.playerOwned) {
            match p.bonus {
                ProvinceBonus::StartingFood(v) => res.food += v,
                ProvinceBonus::StartingWood(v) => res.wood += v,
                ProvinceBonus::StartingMetal(v) => res.metal += v,
                ProvinceBonus::StartingWealth(v) => res.wealth += v,
                ProvinceBonus::ExtraInfantry(n) => extraInf += n,
                ProvinceBonus::ExtraCavalry(n) => extraCav += n,
                ProvinceBonus::ExtraCitizen(n) => extraCit += n,
            }
        }
        (res, extraInf, extraCav, extraCit)
    }

    pub fn saveToFile(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("criar pasta campanha: {e}"))?;
        }
        let s = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|e| format!("serializar campanha: {e}"))?;
        std::fs::write(path, s).map_err(|e| format!("gravar campanha: {e}"))
    }

    pub fn loadFromFile(path: &Path) -> Result<Self, String> {
        let s = std::fs::read_to_string(path).map_err(|e| format!("ler campanha: {e}"))?;
        ron::from_str(&s).map_err(|e| format!("parse campanha: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn campaignAdjacencyAndConquestProgressionWork() {
        let mut camp = CampaignState::new("roma");
        assert_eq!(camp.ownedCount(), 1);
        assert!(camp.canInvade(1), "Galia faz fronteira com Italia");
        assert!(camp.canInvade(2), "Grecia faz fronteira com Italia");
        assert!(!camp.canInvade(7), "Oriente nao faz fronteira inicial");

        camp.conquerProvince(2);
        assert_eq!(camp.ownedCount(), 2);
        assert_eq!(camp.turn, 2);
        let (res, inf, _, _) = camp.accumulatedBonuses();
        assert!(res.food >= 80);
        assert_eq!(inf, 2);
        assert!(camp.canInvade(3), "Anatolia agora acessivel via Grecia");
    }
}
