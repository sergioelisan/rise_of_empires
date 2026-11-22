use std::path::Path;

use serde::{Deserialize, Serialize};

const DEFAULT_UNITS_RON: &str = include_str!("../../../data/units.ron");
const DEFAULT_BUILDINGS_RON: &str = include_str!("../../../data/buildings.ron");
const DEFAULT_TECHS_RON: &str = include_str!("../../../data/techs.ron");
const DEFAULT_NATIONS_RON: &str = include_str!("../../../data/nations.ron");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ResourceKind {
    Food,
    Wood,
    Metal,
    Wealth,
    Knowledge,
}

impl ResourceKind {
    pub const ALL: [Self; 5] = [
        Self::Food,
        Self::Wood,
        Self::Metal,
        Self::Wealth,
        Self::Knowledge,
    ];
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceBundle {
    pub food: i32,
    pub wood: i32,
    pub metal: i32,
    pub wealth: i32,
    pub knowledge: i32,
}

impl ResourceBundle {
    pub const ZERO: Self = Self { food: 0, wood: 0, metal: 0, wealth: 0, knowledge: 0 };

    pub fn get(&self, kind: ResourceKind) -> i32 {
        match kind {
            ResourceKind::Food => self.food,
            ResourceKind::Wood => self.wood,
            ResourceKind::Metal => self.metal,
            ResourceKind::Wealth => self.wealth,
            ResourceKind::Knowledge => self.knowledge,
        }
    }

    pub fn addOne(&mut self, kind: ResourceKind, amount: i32) {
        match kind {
            ResourceKind::Food => self.food += amount,
            ResourceKind::Wood => self.wood += amount,
            ResourceKind::Metal => self.metal += amount,
            ResourceKind::Wealth => self.wealth += amount,
            ResourceKind::Knowledge => self.knowledge += amount,
        }
    }

    pub fn canAfford(&self, cost: &Self) -> bool {
        self.food >= cost.food
            && self.wood >= cost.wood
            && self.metal >= cost.metal
            && self.wealth >= cost.wealth
            && self.knowledge >= cost.knowledge
    }

    pub fn subtract(&mut self, cost: &Self) {
        self.food -= cost.food;
        self.wood -= cost.wood;
        self.metal -= cost.metal;
        self.wealth -= cost.wealth;
        self.knowledge -= cost.knowledge;
    }

    pub fn add(&mut self, other: &Self) {
        self.food += other.food;
        self.wood += other.wood;
        self.metal += other.metal;
        self.wealth += other.wealth;
        self.knowledge += other.knowledge;
    }

    /// Aplica um desconto percentual inteiro (ex.: 20 -> paga 80%).
    pub fn discounted(&self, discountPct: i32) -> Self {
        let factor = (100 - discountPct).clamp(25, 100);
        Self {
            food: (self.food * factor) / 100,
            wood: (self.wood * factor) / 100,
            metal: (self.metal * factor) / 100,
            wealth: (self.wealth * factor) / 100,
            knowledge: (self.knowledge * factor) / 100,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum UnitClass {
    Citizen,
    Infantry,
    Archer,
    Cavalry,
    Siege,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FormationKind {
    #[default]
    Box,
    Line,
    Wedge,
}

impl FormationKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Box => "Caixa",
            Self::Line => "Linha",
            Self::Wedge => "Cunha",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TechBranch {
    Military,
    Civic,
    Commerce,
    Science,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TechEffect {
    MilitaryAttackPct(i32),
    MilitaryHpPct(i32),
    AttritionReductionPct(i32),
    BorderRadiusBonus(u32),
    EnemyAttritionPct(i32),
    CommerceCapBonus(i32),
    GatherRatePct(i32),
    TechDiscountPct(i32),
    VisionBonus(u32),
    RevealMap,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnitDef {
    pub id: String,
    pub name: String,
    pub class: UnitClass,
    pub minAge: u8,
    pub nationOnly: Option<String>,
    pub trainedAt: String,
    pub hp: i32,
    pub attack: i32,
    pub armor: i32,
    pub rangeHundredths: i32,
    pub speedHundredths: i32,
    pub attackCooldownTicks: u32,
    pub visionTiles: u32,
    pub trainTicks: u32,
    pub cost: ResourceBundle,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildingDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub minAge: u8,
    pub footprint: u32,
    pub hp: i32,
    pub armor: i32,
    pub buildTicks: u32,
    pub visionTiles: u32,
    pub borderRadius: u32,
    pub isCityCenter: bool,
    pub isWonder: bool,
    pub requiresBorder: bool,
    pub producesResource: Option<ResourceKind>,
    pub baseGatherRate: i32,
    pub maxWorkers: u32,
    /// Limite máximo desta edificação por cidade controlada (ex.: 3 Fazendas, 1 Mercado, 1 Templo, 1 Universidade).
    pub maxPerCity: Option<u32>,
    pub attack: i32,
    pub attackRangeHundredths: i32,
    pub attackCooldownTicks: u32,
    pub cost: ResourceBundle,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgeDef {
    pub index: u8,
    pub id: String,
    pub name: String,
    pub requiredTechs: usize,
    pub unitHpBonusPct: i32,
    pub unitAttackBonusPct: i32,
    pub borderBonus: u32,
    pub cost: ResourceBundle,
    pub infantryTitle: String,
    pub archerTitle: String,
    pub cavalryTitle: String,
    pub siegeTitle: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TechDef {
    pub id: String,
    pub name: String,
    pub branch: TechBranch,
    pub level: u8,
    pub minAge: u8,
    pub requires: Option<String>,
    pub cost: ResourceBundle,
    pub effect: TechEffect,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NationDef {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub primaryColorRgb: (u8, u8, u8),
    pub accentColorRgb: (u8, u8, u8),
    pub uniqueUnitId: String,
    pub cityNames: Vec<String>,
    pub borderRadiusBonus: u32,
    pub foodGatherBonusPct: i32,
    pub knowledgeGatherBonusPct: i32,
    pub commerceCapBonus: i32,
    pub techCostDiscountPct: i32,
    pub wonderCostDiscountPct: i32,
    pub infantryHpBonusPct: i32,
    pub attritionReductionPct: i32,
    pub citizenTrainSpeedBonusPct: i32,
    pub extraStartingCitizens: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WonderDef {
    pub id: String,
    pub name: String,
    pub minAge: u8,
    pub summary: String,
    pub cost: ResourceBundle,
    pub borderBonus: u32,
    pub foodGatherBonusPct: i32,
    pub wealthGatherBonusPct: i32,
    pub commerceCapBonus: i32,
    pub techDiscountPct: i32,
    pub passiveKnowledgePerMin: i32,
    pub enemyAttritionBonusPct: i32,
    pub militaryAttackBonusPct: i32,
}

#[derive(Clone, Debug, Deserialize)]
struct TechFile {
    ages: Vec<AgeDef>,
    techs: Vec<TechDef>,
}

#[derive(Clone, Debug, Deserialize)]
struct NationsFile {
    nations: Vec<NationDef>,
    wonders: Vec<WonderDef>,
}

/// Catálogo completo carregado dos arquivos RON em `data/`.
#[derive(Clone, Debug)]
pub struct GameData {
    pub units: Vec<UnitDef>,
    pub buildings: Vec<BuildingDef>,
    pub ages: Vec<AgeDef>,
    pub techs: Vec<TechDef>,
    pub nations: Vec<NationDef>,
    pub wonders: Vec<WonderDef>,
}

impl GameData {
    /// Carrega de `data/*.ron` no diretório atual se existir; caso contrário usa os dados embutidos.
    pub fn loadDefault() -> Self {
        let dataDir = Path::new("data");
        if dataDir.join("units.ron").exists() {
            if let Ok(loaded) = Self::loadFromDir(dataDir) {
                return loaded;
            }
        }
        Self::parseStrings(
            DEFAULT_UNITS_RON,
            DEFAULT_BUILDINGS_RON,
            DEFAULT_TECHS_RON,
            DEFAULT_NATIONS_RON,
        )
        .expect("dados RON embutidos devem ser válidos")
    }

    pub fn loadFromDir(dir: &Path) -> Result<Self, String> {
        let read = |file: &str| {
            std::fs::read_to_string(dir.join(file))
                .map_err(|e| format!("falha ao ler {file}: {e}"))
        };
        Self::parseStrings(
            &read("units.ron")?,
            &read("buildings.ron")?,
            &read("techs.ron")?,
            &read("nations.ron")?,
        )
    }

    fn parseStrings(
        unitsRon: &str,
        buildingsRon: &str,
        techsRon: &str,
        nationsRon: &str,
    ) -> Result<Self, String> {
        let units: Vec<UnitDef> =
            ron::from_str(unitsRon).map_err(|e| format!("erro em units.ron: {e}"))?;
        let buildings: Vec<BuildingDef> =
            ron::from_str(buildingsRon).map_err(|e| format!("erro em buildings.ron: {e}"))?;
        let techFile: TechFile =
            ron::from_str(techsRon).map_err(|e| format!("erro em techs.ron: {e}"))?;
        let nationsFile: NationsFile =
            ron::from_str(nationsRon).map_err(|e| format!("erro em nations.ron: {e}"))?;
        Ok(Self {
            units,
            buildings,
            ages: techFile.ages,
            techs: techFile.techs,
            nations: nationsFile.nations,
            wonders: nationsFile.wonders,
        })
    }

    pub fn unit(&self, id: &str) -> Option<&UnitDef> {
        self.units.iter().find(|u| u.id == id)
    }

    pub fn building(&self, id: &str) -> Option<&BuildingDef> {
        self.buildings.iter().find(|b| b.id == id)
    }

    pub fn age(&self, index: u8) -> Option<&AgeDef> {
        self.ages.iter().find(|a| a.index == index)
    }

    pub fn tech(&self, id: &str) -> Option<&TechDef> {
        self.techs.iter().find(|t| t.id == id)
    }

    pub fn nation(&self, id: &str) -> Option<&NationDef> {
        self.nations.iter().find(|n| n.id == id)
    }

    pub fn wonder(&self, id: &str) -> Option<&WonderDef> {
        self.wonders.iter().find(|w| w.id == id)
    }

    /// Nome da unidade adaptado à era atual (para unidades genéricas) ou nome próprio da unidade única.
    pub fn unitDisplayName<'a>(&'a self, unitId: &'a str, age: u8) -> &'a str {
        let Some(def) = self.unit(unitId) else { return unitId };
        if def.nationOnly.is_some() || def.class == UnitClass::Citizen {
            return &def.name;
        }
        let Some(ageDef) = self.age(age) else { return &def.name };
        match def.class {
            UnitClass::Infantry => &ageDef.infantryTitle,
            UnitClass::Archer => &ageDef.archerTitle,
            UnitClass::Cavalry => &ageDef.cavalryTitle,
            UnitClass::Siege => &ageDef.siegeTitle,
            UnitClass::Citizen => &def.name,
        }
    }

    /// Lista unidades treináveis numa construção para a nação e era do jogador.
    pub fn trainableUnitsFor(&self, buildingId: &str, nationId: &str, age: u8) -> Vec<&UnitDef> {
        self.units
            .iter()
            .filter(|u| {
                u.trainedAt == buildingId
                    && u.minAge <= age
                    && u.nationOnly.as_deref().is_none_or(|n| n == nationId)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loadsAllRonDataDeterministically() {
        let data = GameData::loadDefault();
        assert!(data.units.len() >= 10);
        assert!(data.buildings.len() >= 10);
        assert_eq!(data.ages.len(), 4);
        assert_eq!(data.techs.len(), 12);
        assert_eq!(data.nations.len(), 5);
        assert_eq!(data.wonders.len(), 5);

        // Toda nação referencia uma unidade única existente.
        for nation in &data.nations {
            let unique = data.unit(&nation.uniqueUnitId).expect("unidade única ausente");
            assert_eq!(unique.nationOnly.as_deref(), Some(nation.id.as_str()));
        }

        // Antes da criação de Universidades (Era Clássica), pesquisas e avanço para Era 1 não cobram Conhecimento.
        assert_eq!(data.building("library").unwrap().minAge, 1);
        assert_eq!(data.age(1).unwrap().cost.knowledge, 0);
        for tech in data.techs.iter().filter(|t| t.minAge == 0) {
            assert_eq!(tech.cost.knowledge, 0, "pesquisa inicial {} não deve cobrar conhecimento", tech.id);
        }

        // Regras anti-travamento da economia: Madeireira custa Comida (nunca Madeira);
        // Fazenda e Mina custam apenas Madeira; limites de trabalhadores e por cidade.
        let lumber = &data.building("lumber_camp").unwrap().cost;
        assert_eq!(lumber.wood, 0);
        assert!(lumber.food > 0);
        for id in ["farm", "mine"] {
            let c = &data.building(id).unwrap().cost;
            assert!(c.wood > 0 && c.food == 0 && c.metal == 0 && c.wealth == 0 && c.knowledge == 0);
        }
        assert_eq!(data.building("farm").unwrap().maxWorkers, 1);
        assert_eq!(data.building("farm").unwrap().maxPerCity, Some(3));
        assert_eq!(data.building("market").unwrap().maxPerCity, Some(1));
        assert_eq!(data.building("temple").unwrap().maxPerCity, Some(1));
        assert_eq!(data.building("library").unwrap().maxPerCity, Some(1));
    }
}
