use std::collections::BTreeSet;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

use crate::TICKS_PER_SECOND;
use crate::ai::{AiDifficulty, decideAiCommands};
use crate::combat::{
    CombatEvent, buildingCounterMultiplierPct, computeDamage, counterMultiplierPct,
};
use crate::command::{BuildingId, CityId, Command, PlayerId, UnitId};
use crate::data::{
    FormationKind, GameData, NationDef, ResourceBundle, ResourceKind, TechEffect, UnitClass,
    UnitDef,
};
use crate::map::{MapGrid, TerrainType};
use crate::math::{Fx, Vec2Fx, fxFromHundredths, fxSerde};
use crate::pathfinding::{FlowFieldCache, computeFormationTargets, snapWalkable};
use crate::territory::{City, FogState, TerritoryMap, updateFog};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkAnim {
    #[default]
    None,
    Farming,
    Chopping,
    Mining,
    Building,
    Market,
    Working,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitOrder {
    Idle,
    Move { target: Vec2Fx },
    AttackUnit(UnitId),
    AttackBuilding(BuildingId),
    Gather { resource: ResourceKind, tile: (u32, u32), building: Option<BuildingId> },
    Build(BuildingId),
    Repair(BuildingId),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Unit {
    pub id: UnitId,
    pub owner: PlayerId,
    pub kind: String,
    pub class: UnitClass,
    pub pos: Vec2Fx,
    /// Posição no tick anterior; a renderização interpola entre `prevPos` e `pos`.
    pub prevPos: Vec2Fx,
    /// Direção de olhar normalizada (para orientar o modelo low-poly).
    pub facing: Vec2Fx,
    /// Campo legado de compatibilidade para a fase 0/1 (reflete o destino de `Move`).
    pub target: Option<Vec2Fx>,
    /// Tiles por segundo.
    #[serde(with = "fxSerde")]
    pub speed: Fx,
    pub hp: i32,
    pub maxHp: i32,
    pub attack: i32,
    pub armor: i32,
    #[serde(with = "fxSerde")]
    pub range: Fx,
    pub attackCooldown: u32,
    pub attackCooldownMax: u32,
    pub visionTiles: u32,
    pub order: UnitOrder,
    pub formation: FormationKind,
    pub eraTier: u8,
    pub inEnemyTerritory: bool,
    pub attritionTimer: u32,
    /// Estado visual determinístico do trabalho do cidadão (arar, cortar, minerar, construir).
    pub workAnim: WorkAnim,
    /// Contador de ciclo de coleta (usado por lenhadores para levar toras à madeireira).
    pub workCycle: u32,
    /// Quando verdadeiro, o lenhador está levando madeira da floresta até a madeireira associada.
    pub returningToDropoff: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainOrder {
    pub unitKind: String,
    pub remainingTicks: u32,
    pub totalTicks: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Building {
    pub id: BuildingId,
    pub owner: PlayerId,
    pub kind: String,
    pub wonderId: Option<String>,
    /// Canto superior-esquerdo `(tx, ty)` ocupado pela construção.
    pub tile: (u32, u32),
    pub footprint: u32,
    pub hp: i32,
    pub maxHp: i32,
    pub armor: i32,
    pub buildProgress: u32,
    pub buildRequired: u32,
    /// Máximo de trabalhadores permitidos nesta construção (dinâmico na Madeireira conforme a mata próxima).
    #[serde(default)]
    pub workerSlots: u32,
    pub trainQueue: Vec<TrainOrder>,
    pub attackCooldown: u32,
    pub cityId: Option<CityId>,
}

impl Building {
    pub fn isComplete(&self) -> bool {
        self.buildProgress >= self.buildRequired
    }

    pub fn centerPos(&self) -> Vec2Fx {
        let half = Fx::from_num(self.footprint) / Fx::from_num(2);
        Vec2Fx::new(Fx::from_num(self.tile.0) + half, Fx::from_num(self.tile.1) + half)
    }

    pub fn centerTile(&self) -> (u32, u32) {
        (self.tile.0 + self.footprint / 2, self.tile.1 + self.footprint / 2)
    }

    pub fn occupiesTile(&self, tx: u32, ty: u32) -> bool {
        tx >= self.tile.0
            && tx < self.tile.0 + self.footprint
            && ty >= self.tile.1
            && ty < self.tile.1 + self.footprint
    }

    /// Distância aproximada de um ponto até a borda do footprint.
    pub fn edgeDistance(&self, p: Vec2Fx) -> Fx {
        let minX = Fx::from_num(self.tile.0);
        let maxX = Fx::from_num(self.tile.0 + self.footprint);
        let minY = Fx::from_num(self.tile.1);
        let maxY = Fx::from_num(self.tile.1 + self.footprint);
        let cx = p.x.clamp(minX, maxX);
        let cy = p.y.clamp(minY, maxY);
        p.distance(Vec2Fx::new(cx, cy))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerState {
    pub id: PlayerId,
    pub nationId: String,
    pub age: u8,
    pub resources: ResourceBundle,
    /// Taxa efetiva atual (por minuto), já limitada pelo limite de coleta (`commerceCap`).
    pub gatherRates: ResourceBundle,
    /// Limite de coleta por minuto (escala com cidades, Comércio e maravilhas).
    pub commerceCap: i32,
    /// Acumuladores fracionários (em 1/60 de recurso) para converter taxa/min em inteiros sem perda.
    gatherAccum: ResourceBundle,
    pub researchedTechs: BTreeSet<String>,
    pub wondersBuilt: Vec<String>,
    pub fog: Vec<FogState>,
    pub fogVersion: u64,
    pub citiesFounded: u32,
    pub mapRevealed: bool,
    /// Se preenchido, este jogador é controlado pela IA determinística da fase 8.
    pub aiDifficulty: Option<AiDifficulty>,
}

impl PlayerState {
    pub fn new(id: PlayerId, nationId: impl Into<String>, mapWidth: u32, mapHeight: u32) -> Self {
        Self {
            id,
            nationId: nationId.into(),
            age: 0,
            resources: ResourceBundle::ZERO,
            gatherRates: ResourceBundle::ZERO,
            commerceCap: 90,
            gatherAccum: ResourceBundle::ZERO,
            researchedTechs: BTreeSet::new(),
            wondersBuilt: Vec::new(),
            fog: vec![FogState::Unexplored; (mapWidth * mapHeight) as usize],
            fogVersion: 1,
            citiesFounded: 0,
            mapRevealed: false,
            aiDifficulty: None,
        }
    }

    pub fn nation<'a>(&self, data: &'a GameData) -> Option<&'a NationDef> {
        data.nation(&self.nationId)
    }

    pub fn borderRadiusBonus(&self, data: &GameData) -> u32 {
        let mut bonus = self.nation(data).map_or(0, |n| n.borderRadiusBonus);
        bonus += data.age(self.age).map_or(0, |a| a.borderBonus);
        for techId in &self.researchedTechs {
            if let Some(t) = data.tech(techId) {
                if let TechEffect::BorderRadiusBonus(b) = t.effect {
                    bonus += b;
                }
            }
        }
        for wId in &self.wondersBuilt {
            if let Some(w) = data.wonder(wId) {
                bonus += w.borderBonus;
            }
        }
        bonus
    }

    pub fn effectiveCommerceCap(&self, data: &GameData, cityCount: usize) -> i32 {
        let mut cap = 40 + (cityCount.max(1) as i32) * 55;
        if let Some(n) = self.nation(data) {
            cap += n.commerceCapBonus;
        }
        for techId in &self.researchedTechs {
            if let Some(t) = data.tech(techId) {
                if let TechEffect::CommerceCapBonus(b) = t.effect {
                    cap += b;
                }
            }
        }
        for wId in &self.wondersBuilt {
            if let Some(w) = data.wonder(wId) {
                cap += w.commerceCapBonus;
            }
        }
        cap
    }

    pub fn techDiscountPct(&self, data: &GameData) -> i32 {
        let mut pct = self.nation(data).map_or(0, |n| n.techCostDiscountPct);
        for techId in &self.researchedTechs {
            if let Some(t) = data.tech(techId) {
                if let TechEffect::TechDiscountPct(b) = t.effect {
                    pct += b;
                }
            }
        }
        for wId in &self.wondersBuilt {
            if let Some(w) = data.wonder(wId) {
                pct += w.techDiscountPct;
            }
        }
        pct.min(60)
    }

    pub fn wonderDiscountPct(&self, data: &GameData) -> i32 {
        self.nation(data).map_or(0, |n| n.wonderCostDiscountPct).min(50)
    }

    pub fn gatherBonusPct(&self, data: &GameData, resource: ResourceKind) -> i32 {
        let mut pct = 0;
        if let Some(n) = self.nation(data) {
            match resource {
                ResourceKind::Food => pct += n.foodGatherBonusPct,
                ResourceKind::Knowledge => pct += n.knowledgeGatherBonusPct,
                _ => {}
            }
        }
        for techId in &self.researchedTechs {
            if let Some(t) = data.tech(techId) {
                if let TechEffect::GatherRatePct(b) = t.effect {
                    pct += b;
                }
            }
        }
        for wId in &self.wondersBuilt {
            if let Some(w) = data.wonder(wId) {
                match resource {
                    ResourceKind::Food => pct += w.foodGatherBonusPct,
                    ResourceKind::Wealth => pct += w.wealthGatherBonusPct,
                    _ => {}
                }
            }
        }
        pct
    }

    pub fn passiveKnowledgePerMin(&self, data: &GameData) -> i32 {
        let mut total = 0;
        for wId in &self.wondersBuilt {
            if let Some(w) = data.wonder(wId) {
                total += w.passiveKnowledgePerMin;
            }
        }
        total
    }

    pub fn militaryAttackBonusPct(&self, data: &GameData) -> i32 {
        let mut pct = data.age(self.age).map_or(0, |a| a.unitAttackBonusPct);
        for techId in &self.researchedTechs {
            if let Some(t) = data.tech(techId) {
                if let TechEffect::MilitaryAttackPct(b) = t.effect {
                    pct += b;
                }
            }
        }
        for wId in &self.wondersBuilt {
            if let Some(w) = data.wonder(wId) {
                pct += w.militaryAttackBonusPct;
            }
        }
        pct
    }

    pub fn militaryHpBonusPct(&self, data: &GameData, class: UnitClass) -> i32 {
        let mut pct = data.age(self.age).map_or(0, |a| a.unitHpBonusPct);
        if class == UnitClass::Infantry {
            pct += self.nation(data).map_or(0, |n| n.infantryHpBonusPct);
        }
        for techId in &self.researchedTechs {
            if let Some(t) = data.tech(techId) {
                if let TechEffect::MilitaryHpPct(b) = t.effect {
                    pct += b;
                }
            }
        }
        pct
    }

    pub fn attritionReductionPct(&self, data: &GameData) -> i32 {
        let mut pct = self.nation(data).map_or(0, |n| n.attritionReductionPct);
        for techId in &self.researchedTechs {
            if let Some(t) = data.tech(techId) {
                if let TechEffect::AttritionReductionPct(b) = t.effect {
                    pct += b;
                }
            }
        }
        pct.min(85)
    }

    pub fn enemyAttritionBonusPct(&self, data: &GameData) -> i32 {
        let mut pct = 0;
        for techId in &self.researchedTechs {
            if let Some(t) = data.tech(techId) {
                if let TechEffect::EnemyAttritionPct(b) = t.effect {
                    pct += b;
                }
            }
        }
        for wId in &self.wondersBuilt {
            if let Some(w) = data.wonder(wId) {
                pct += w.enemyAttritionBonusPct;
            }
        }
        pct
    }

    pub fn visionBonus(&self, data: &GameData) -> u32 {
        let mut bonus = 0;
        for techId in &self.researchedTechs {
            if let Some(t) = data.tech(techId) {
                if let TechEffect::VisionBonus(b) = t.effect {
                    bonus += b;
                }
            }
        }
        bonus
    }

    pub fn effectiveUnitStats(&self, data: &GameData, def: &UnitDef) -> (i32, i32, i32) {
        if def.class == UnitClass::Citizen {
            return (def.hp, def.attack, def.armor);
        }
        let hpPct = 100 + self.militaryHpBonusPct(data, def.class);
        let atkPct = 100 + self.militaryAttackBonusPct(data);
        let hp = (def.hp * hpPct) / 100;
        let atk = (def.attack * atkPct) / 100;
        let armor = def.armor + i32::from(self.age);
        (hp, atk, armor)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SimWorld {
    pub tick: u64,
    pub map: MapGrid,
    #[serde(skip, default = "GameData::loadDefault")]
    pub data: GameData,
    pub players: Vec<PlayerState>,
    /// Sempre em ordem crescente de id, mantendo a iteração determinística.
    pub units: Vec<Unit>,
    pub buildings: Vec<Building>,
    pub cities: Vec<City>,
    pub territory: TerritoryMap,
    pub combatEvents: Vec<CombatEvent>,
    pub winner: Option<PlayerId>,
    #[serde(skip)]
    flowCache: FlowFieldCache,
    #[serde(skip)]
    blockedTiles: Vec<bool>,
    pub buildingVersion: u64,
    nextUnitId: u32,
    nextBuildingId: u32,
    nextCityId: u32,
}

impl SimWorld {
    /// Mundo mínimo sem cidades/jogadores extras (usado pelos testes da fase 0/1).
    pub fn new(map: MapGrid) -> Self {
        let data = GameData::loadDefault();
        let (w, h) = (map.width, map.height);
        let players = vec![PlayerState::new(PlayerId(0), "roma", w, h)];
        Self {
            tick: 0,
            map,
            data,
            players,
            units: Vec::new(),
            buildings: Vec::new(),
            cities: Vec::new(),
            territory: TerritoryMap::new(w, h),
            combatEvents: Vec::new(),
            winner: None,
            flowCache: FlowFieldCache::default(),
            blockedTiles: vec![false; (w * h) as usize],
            buildingVersion: 1,
            nextUnitId: 0,
            nextBuildingId: 0,
            nextCityId: 0,
        }
    }

    /// Cria uma partida entre duas civilizações (inimigo passivo por padrão para testes).
    pub fn newMatch(
        map: MapGrid,
        data: GameData,
        player0Nation: &str,
        player1Nation: &str,
    ) -> Self {
        Self::newMatchWithAi(map, data, player0Nation, player1Nation, AiDifficulty::Passive)
    }

    /// Cria uma partida completa com IA configurável para o oponente (`PlayerId(1)`).
    pub fn newMatchWithAi(
        map: MapGrid,
        data: GameData,
        player0Nation: &str,
        player1Nation: &str,
        enemyAi: AiDifficulty,
    ) -> Self {
        let (w, h) = (map.width, map.height);
        let mut p0 = PlayerState::new(PlayerId(0), player0Nation, w, h);
        p0.resources = ResourceBundle {
            food: 260,
            wood: 260,
            metal: 140,
            wealth: 140,
            knowledge: 0,
        };
        let mut p1 = PlayerState::new(PlayerId(1), player1Nation, w, h);
        p1.aiDifficulty = Some(enemyAi);
        p1.resources = ResourceBundle {
            food: 220,
            wood: 220,
            metal: 120,
            wealth: 120,
            knowledge: 0,
        };

        let mut world = Self {
            tick: 0,
            map,
            data,
            players: vec![p0, p1],
            units: Vec::new(),
            buildings: Vec::new(),
            cities: Vec::new(),
            territory: TerritoryMap::new(w, h),
            combatEvents: Vec::new(),
            winner: None,
            flowCache: FlowFieldCache::default(),
            blockedTiles: vec![false; (w * h) as usize],
            buildingVersion: 1,
            nextUnitId: 0,
            nextBuildingId: 0,
            nextCityId: 0,
        };

        // Encontra locais passáveis espaçosos em quadrantes opostos para as duas capitais.
        let p0Site = world.findSpaciousCapitalSite((w / 4, (h * 3) / 4));
        let p1Site = world.findSpaciousCapitalSite(((w * 3) / 4, h / 4));

        world.spawnCompletedBuilding(PlayerId(0), "city_center", p0Site, None);
        world.spawnCompletedBuilding(PlayerId(1), "city_center", p1Site, None);
        world.recomputeTerritory();

        // Estruturas iniciais perto da capital do jogador 0 (Fazenda + Quartel).
        let farm0 = world.findBuildSiteNear((p0Site.0 + 4, p0Site.1.saturating_sub(2)), 2);
        let farm0Id = world.spawnCompletedBuilding(PlayerId(0), "farm", farm0, None);
        let barracks0 = world.findBuildSiteNear((p0Site.0.saturating_sub(3), p0Site.1 + 3), 2);
        world.spawnCompletedBuilding(PlayerId(0), "barracks", barracks0, None);

        // Estruturas iniciais da capital inimiga (Fazenda + Quartel + Torre).
        let farm1 = world.findBuildSiteNear((p1Site.0.saturating_sub(4), p1Site.1 + 2), 2);
        let farm1Id = world.spawnCompletedBuilding(PlayerId(1), "farm", farm1, None);
        let barracks1 = world.findBuildSiteNear((p1Site.0 + 4, p1Site.1), 2);
        world.spawnCompletedBuilding(PlayerId(1), "barracks", barracks1, None);
        let tower1 = world.findBuildSiteNear((p1Site.0.saturating_sub(3), p1Site.1 + 4), 1);
        world.spawnCompletedBuilding(PlayerId(1), "tower", tower1, None);

        world.recomputeTerritory();

        // Cidadãos iniciais + guarnição inicial do Jogador 0.
        let extraCit0 = world
            .player(PlayerId(0))
            .and_then(|p| p.nation(&world.data))
            .map_or(0, |n| n.extraStartingCitizens);
        let p0Center = Vec2Fx::tileCenter(p0Site.0 + 1, p0Site.1 + 1);
        for i in 0..(3 + extraCit0) {
            let spawn = snapWalkable(
                &world.map,
                &world.blockedTiles,
                Vec2Fx::new(p0Center.x + Fx::from_num(i as i32 - 1), p0Center.y + Fx::from_num(3)),
            );
            let uid = world.spawnUnitInternal(PlayerId(0), "citizen", spawn);
            if i == 0 {
                if let Some(fid) = farm0Id {
                    if let Some(u) = world.unitMut(uid) {
                        u.order = UnitOrder::Gather {
                            resource: ResourceKind::Food,
                            tile: farm0,
                            building: Some(fid),
                        };
                    }
                }
            }
        }
        // Pequena força inicial para explorar, testar formações e combate.
        let escortKinds = ["infantry", "infantry", "archer", "cavalry"];
        for (i, kind) in escortKinds.iter().enumerate() {
            let spawn = snapWalkable(
                &world.map,
                &world.blockedTiles,
                Vec2Fx::new(
                    p0Center.x + Fx::from_num(i as i32 + 2),
                    p0Center.y - Fx::from_num(3),
                ),
            );
            world.spawnUnitInternal(PlayerId(0), kind, spawn);
        }

        // Cidadãos + guarnição defensiva passiva do Jogador 1.
        let p1Center = Vec2Fx::tileCenter(p1Site.0 + 1, p1Site.1 + 1);
        for i in 0..2u32 {
            let spawn = snapWalkable(
                &world.map,
                &world.blockedTiles,
                Vec2Fx::new(p1Center.x + Fx::from_num(i as i32), p1Center.y + Fx::from_num(3)),
            );
            let uid = world.spawnUnitInternal(PlayerId(1), "citizen", spawn);
            if i == 0 {
                if let Some(fid) = farm1Id {
                    if let Some(u) = world.unitMut(uid) {
                        u.order = UnitOrder::Gather {
                            resource: ResourceKind::Food,
                            tile: farm1,
                            building: Some(fid),
                        };
                    }
                }
            }
        }
        let enemyKinds = ["infantry", "infantry", "archer", "cavalry"];
        for (i, kind) in enemyKinds.iter().enumerate() {
            let spawn = snapWalkable(
                &world.map,
                &world.blockedTiles,
                Vec2Fx::new(
                    p1Center.x - Fx::from_num(i as i32 + 2),
                    p1Center.y + Fx::from_num(3),
                ),
            );
            world.spawnUnitInternal(PlayerId(1), kind, spawn);
        }

        world.refreshAllFog();
        world
    }

    pub fn player(&self, id: PlayerId) -> Option<&PlayerState> {
        self.players.iter().find(|p| p.id == id)
    }

    pub fn playerMut(&mut self, id: PlayerId) -> Option<&mut PlayerState> {
        self.players.iter_mut().find(|p| p.id == id)
    }

    pub fn unit(&self, id: UnitId) -> Option<&Unit> {
        self.units.binary_search_by_key(&id, |u| u.id).ok().map(|i| &self.units[i])
    }

    fn unitMut(&mut self, id: UnitId) -> Option<&mut Unit> {
        self.units.binary_search_by_key(&id, |u| u.id).ok().map(|i| &mut self.units[i])
    }

    pub fn building(&self, id: BuildingId) -> Option<&Building> {
        self.buildings.binary_search_by_key(&id, |b| b.id).ok().map(|i| &self.buildings[i])
    }

    pub fn buildingAtTile(&self, tx: u32, ty: u32) -> Option<&Building> {
        self.buildings.iter().find(|b| b.occupiesTile(tx, ty))
    }

    pub fn city(&self, id: CityId) -> Option<&City> {
        self.cities.iter().find(|c| c.id == id)
    }

    pub fn cityCount(&self, owner: PlayerId) -> usize {
        self.cities.iter().filter(|c| c.owner == owner).count()
    }

    pub fn buildingCountOfKind(&self, owner: PlayerId, kind: &str) -> u32 {
        self.buildings
            .iter()
            .filter(|b| b.owner == owner && b.kind == kind)
            .count() as u32
    }

    /// Limite máximo desta construção para o jogador, escalado pelo número de cidades.
    pub fn buildingLimitFor(&self, owner: PlayerId, kind: &str) -> Option<u32> {
        let maxPerCity = self.data.building(kind)?.maxPerCity?;
        let cities = self.cityCount(owner).max(1) as u32;
        Some(maxPerCity * cities)
    }

    /// Quantos cidadãos estão atualmente designados para trabalhar nesta construção.
    pub fn buildingAssignedWorkers(&self, bid: BuildingId) -> u32 {
        self.units
            .iter()
            .filter(|u| matches!(u.order, UnitOrder::Gather { building: Some(id), .. } if id == bid))
            .count() as u32
    }

    /// Calcula as vagas de trabalhadores de uma construção (Madeireira escala com o tamanho da mata próxima).
    pub fn computeWorkerSlots(&self, kind: &str, tile: (u32, u32), footprint: u32) -> u32 {
        let base = self.data.building(kind).map_or(0, |d| d.maxWorkers);
        if kind != "lumber_camp" {
            return base;
        }
        let cx = (tile.0 + footprint / 2) as i32;
        let cy = (tile.1 + footprint / 2) as i32;
        let radius = 5i32;
        let mut forestTiles = 0u32;
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let nx = cx + dx;
                let ny = cy + dy;
                if nx >= 0 && ny >= 0 && self.map.tile(nx as u32, ny as u32) == Some(TerrainType::Forest) {
                    forestTiles += 1;
                }
            }
        }
        // 1 vaga base + 1 vaga a cada 5 tiles de floresta próxima (entre 1 e 5 lenhadores).
        (1 + forestTiles / 5).clamp(1, base.max(1))
    }

    pub fn isTileWalkable(&self, tx: u32, ty: u32) -> bool {
        if tx >= self.map.width || ty >= self.map.height {
            return false;
        }
        self.map.isPassableTile(tx, ty) && !self.blockedTiles[(ty * self.map.width + tx) as usize]
    }

    pub fn isPosWalkable(&self, pos: Vec2Fx) -> bool {
        if self.map.width == 0 || self.map.height == 0 {
            return false;
        }
        let clamped = self.map.clampPoint(pos);
        let tx = (clamped.x.floor().to_num::<i32>()).clamp(0, self.map.width as i32 - 1) as u32;
        let ty = (clamped.y.floor().to_num::<i32>()).clamp(0, self.map.height as i32 - 1) as u32;
        self.isTileWalkable(tx, ty)
    }

    pub fn fogAtTile(&self, player: PlayerId, tx: u32, ty: u32) -> FogState {
        if tx >= self.map.width || ty >= self.map.height {
            return FogState::Unexplored;
        }
        self.player(player)
            .and_then(|p| p.fog.get((ty * self.map.width + tx) as usize).copied())
            .unwrap_or(FogState::Unexplored)
    }

    pub fn isPosVisibleTo(&self, player: PlayerId, pos: Vec2Fx) -> bool {
        let clamped = self.map.clampPoint(pos);
        let tx = (clamped.x.floor().to_num::<i32>()).clamp(0, self.map.width as i32 - 1) as u32;
        let ty = (clamped.y.floor().to_num::<i32>()).clamp(0, self.map.height as i32 - 1) as u32;
        self.fogAtTile(player, tx, ty) == FogState::Visible
    }

    pub fn isWonderClaimed(&self, wonderId: &str) -> bool {
        self.players.iter().any(|p| p.wondersBuilt.iter().any(|w| w == wonderId))
            || self.buildings.iter().any(|b| b.wonderId.as_deref() == Some(wonderId))
    }

    pub fn effectiveTechCost(&self, owner: PlayerId, techId: &str) -> Option<ResourceBundle> {
        let tech = self.data.tech(techId)?;
        let discount = self.player(owner)?.techDiscountPct(&self.data);
        Some(tech.cost.discounted(discount))
    }

    pub fn canResearchTech(&self, owner: PlayerId, techId: &str) -> bool {
        let Some(player) = self.player(owner) else { return false };
        let Some(tech) = self.data.tech(techId) else { return false };
        if player.researchedTechs.contains(techId) || player.age < tech.minAge {
            return false;
        }
        if let Some(req) = &tech.requires {
            if !player.researchedTechs.contains(req) {
                return false;
            }
        }
        let cost = tech.cost.discounted(player.techDiscountPct(&self.data));
        player.resources.canAfford(&cost)
    }

    pub fn effectiveAgeCost(&self, owner: PlayerId) -> Option<ResourceBundle> {
        let player = self.player(owner)?;
        let next = self.data.age(player.age + 1)?;
        Some(next.cost.discounted(player.techDiscountPct(&self.data)))
    }

    pub fn canAdvanceAge(&self, owner: PlayerId) -> bool {
        let Some(player) = self.player(owner) else { return false };
        let Some(next) = self.data.age(player.age + 1) else { return false };
        if player.researchedTechs.len() < next.requiredTechs {
            return false;
        }
        let cost = next.cost.discounted(player.techDiscountPct(&self.data));
        player.resources.canAfford(&cost)
    }

    pub fn effectiveBuildingCost(
        &self,
        owner: PlayerId,
        kind: &str,
        wonderId: Option<&str>,
    ) -> Option<ResourceBundle> {
        let player = self.player(owner)?;
        if let Some(wId) = wonderId {
            let wonder = self.data.wonder(wId)?;
            return Some(wonder.cost.discounted(player.wonderDiscountPct(&self.data)));
        }
        Some(self.data.building(kind)?.cost)
    }

    /// Verifica se uma construção (ou maravilha) pode ser posicionada no tile `(tx, ty)`.
    pub fn canPlaceBuilding(
        &self,
        owner: PlayerId,
        kind: &str,
        tile: (u32, u32),
        wonderId: Option<&str>,
    ) -> bool {
        let Some(player) = self.player(owner) else { return false };
        let Some(def) = self.data.building(kind) else { return false };
        if player.age < def.minAge {
            return false;
        }
        if let Some(limit) = self.buildingLimitFor(owner, kind) {
            if self.buildingCountOfKind(owner, kind) >= limit {
                return false;
            }
        }
        if def.isWonder {
            let Some(wId) = wonderId else { return false };
            let Some(wDef) = self.data.wonder(wId) else { return false };
            if player.age < wDef.minAge || self.isWonderClaimed(wId) {
                return false;
            }
        }
        if tile.0 + def.footprint > self.map.width || tile.1 + def.footprint > self.map.height {
            return false;
        }
        for dy in 0..def.footprint {
            for dx in 0..def.footprint {
                let (cx, cy) = (tile.0 + dx, tile.1 + dy);
                if !self.isTileWalkable(cx, cy) || self.buildingAtTile(cx, cy).is_some() {
                    return false;
                }
            }
        }

        let centerTile = (tile.0 + def.footprint / 2, tile.1 + def.footprint / 2);
        let tileOwner = self.territory.ownerAtTile(centerTile.0, centerTile.1);
        if def.requiresBorder {
            if tileOwner != Some(owner) {
                return false;
            }
        } else if def.isCityCenter {
            // Não pode fundar cidade em território inimigo nem colado em outra cidade.
            if tileOwner.is_some_and(|o| o != owner) {
                return false;
            }
            for city in &self.cities {
                let dx = centerTile.0 as i32 - city.centerTile.0 as i32;
                let dy = centerTile.1 as i32 - city.centerTile.1 as i32;
                if dx * dx + dy * dy < 10 * 10 {
                    return false;
                }
            }
        }

        let Some(cost) = self.effectiveBuildingCost(owner, kind, wonderId) else { return false };
        player.resources.canAfford(&cost)
    }

    /// Recompõe caches derivados (`GameData`, `blockedTiles`, `flowCache`) após carregar um save.
    pub fn restoreTransientState(&mut self, data: GameData) {
        self.data = data;
        self.blockedTiles = vec![false; (self.map.width * self.map.height) as usize];
        let buildings = self.buildings.clone();
        for b in &buildings {
            self.markFootprint(b, true);
        }
        self.flowCache = FlowFieldCache::default();
    }

    /// Avança um tick aplicando os comandos recebidos e as decisões determinísticas da IA.
    pub fn step(&mut self, commands: &[Command]) {
        self.combatEvents.retain(|e| self.tick.saturating_sub(e.tick) < 8);

        for command in commands {
            self.applyCommand(command);
        }

        // Coleta comandos das IAs ativas de forma determinística antes de avançar a simulação.
        let aiPlayers: Vec<(PlayerId, AiDifficulty)> = self
            .players
            .iter()
            .filter_map(|p| p.aiDifficulty.map(|d| (p.id, d)))
            .collect();
        let mut aiCommands = Vec::new();
        for (pid, diff) in aiPlayers {
            aiCommands.extend(decideAiCommands(self, pid, diff));
        }
        for cmd in &aiCommands {
            self.applyCommand(cmd);
        }

        self.stepProduction();
        self.stepEconomy();
        self.stepMovement();
        self.stepCombatAndCapture();
        self.stepAttrition();

        if self.tick % 4 == 0 {
            self.refreshAllFog();
        }
        self.tick += 1;
    }

    /// Resumo do estado, para detectar dessincronização entre máquinas e em replays.
    pub fn checksum(&self) -> u64 {
        let mut hasher = Fnv1a::default();
        self.tick.hash(&mut hasher);
        self.map.width.hash(&mut hasher);
        self.map.height.hash(&mut hasher);
        self.map.seed.hash(&mut hasher);
        for p in &self.players {
            p.id.hash(&mut hasher);
            p.age.hash(&mut hasher);
            p.resources.hash(&mut hasher);
            p.researchedTechs.len().hash(&mut hasher);
            p.wondersBuilt.len().hash(&mut hasher);
        }
        for unit in &self.units {
            unit.id.hash(&mut hasher);
            unit.pos.hash(&mut hasher);
            unit.hp.hash(&mut hasher);
            unit.target.hash(&mut hasher);
        }
        for b in &self.buildings {
            b.id.hash(&mut hasher);
            b.owner.hash(&mut hasher);
            b.hp.hash(&mut hasher);
            b.buildProgress.hash(&mut hasher);
        }
        hasher.finish()
    }

    // --- Aplicação de Comandos ---

    fn applyCommand(&mut self, command: &Command) {
        match command {
            Command::SpawnUnit { pos } => {
                let clamped = self.map.clampPoint(*pos);
                let pos = if self.isPosWalkable(clamped) {
                    clamped
                } else {
                    self.map.findNearestPassable(clamped).unwrap_or(clamped)
                };
                self.spawnUnitInternal(PlayerId(0), "infantry", pos);
            }
            Command::SpawnTypedUnit { owner, kind, pos } => {
                let clamped = self.map.clampPoint(*pos);
                let pos = snapWalkable(&self.map, &self.blockedTiles, clamped);
                self.spawnUnitInternal(*owner, kind, pos);
            }
            Command::MoveUnit { unit, target } => {
                let target = self.map.clampPoint(*target);
                if let Some(u) = self.unitMut(*unit) {
                    u.target = Some(target);
                    u.order = UnitOrder::Move { target };
                }
            }
            Command::MoveUnits { owner, units, target, formation } => {
                let validIds: Vec<UnitId> = units
                    .iter()
                    .copied()
                    .filter(|id| self.unit(*id).is_some_and(|u| u.owner == *owner))
                    .collect();
                if validIds.is_empty() {
                    return;
                }
                let mut sumX = Fx::ZERO;
                let mut sumY = Fx::ZERO;
                for &id in &validIds {
                    if let Some(u) = self.unit(id) {
                        sumX += u.pos.x;
                        sumY += u.pos.y;
                    }
                }
                let countFx = Fx::from_num(validIds.len() as i32);
                let groupCenter = Vec2Fx::new(sumX / countFx, sumY / countFx);
                let slots = computeFormationTargets(
                    &self.map,
                    &self.blockedTiles,
                    validIds.len(),
                    groupCenter,
                    *target,
                    *formation,
                );
                for (id, slot) in validIds.into_iter().zip(slots.into_iter()) {
                    if let Some(u) = self.unitMut(id) {
                        u.formation = *formation;
                        u.target = Some(slot);
                        u.order = UnitOrder::Move { target: slot };
                    }
                }
            }
            Command::AttackUnit { owner, units, target } => {
                if self.unit(*target).is_none_or(|t| t.owner == *owner) {
                    return;
                }
                for &id in units {
                    if let Some(u) = self.unitMut(id) {
                        if u.owner == *owner {
                            u.target = None;
                            u.order = UnitOrder::AttackUnit(*target);
                        }
                    }
                }
            }
            Command::AttackBuilding { owner, units, target } => {
                if self.building(*target).is_none_or(|b| b.owner == *owner) {
                    return;
                }
                for &id in units {
                    if let Some(u) = self.unitMut(id) {
                        if u.owner == *owner {
                            u.target = None;
                            u.order = UnitOrder::AttackBuilding(*target);
                        }
                    }
                }
            }
            Command::GatherAt { owner, units, tile } => {
                // Primeiro checa se há construção aliada no tile (inacabada -> construir; de recurso -> coletar).
                if let Some(b) = self.buildingAtTile(tile.0, tile.1).cloned() {
                    if b.owner == *owner {
                        if !b.isComplete() {
                            for &id in units {
                                if let Some(u) = self.unitMut(id) {
                                    if u.owner == *owner && u.class == UnitClass::Citizen {
                                        u.target = None;
                                        u.returningToDropoff = false;
                                        u.workCycle = 0;
                                        u.order = UnitOrder::Build(b.id);
                                    }
                                }
                            }
                            return;
                        }
                        if let Some(res) =
                            self.data.building(&b.kind).and_then(|d| d.producesResource)
                        {
                            let mut claimedTrees = self.claimedForestTiles(*owner);
                            for &id in units {
                                let isCitizen = self
                                    .unit(id)
                                    .is_some_and(|u| u.owner == *owner && u.class == UnitClass::Citizen);
                                if !isCitizen {
                                    continue;
                                }
                                // Respeita o limite de trabalhadores do prédio; se lotado, tenta outro prédio do mesmo tipo com vaga.
                                let targetBid = self
                                    .pickBuildingWithFreeSlot(*owner, &b.kind, Some(b.id), id);
                                let Some(chosenBid) = targetBid else { continue };
                                let Some(chosenB) = self.building(chosenBid).cloned() else {
                                    continue;
                                };
                                let workTile = if chosenB.kind == "lumber_camp" {
                                    let chosen = self
                                        .findSpacedForestTile(chosenB.centerTile(), 14, &claimedTrees)
                                        .unwrap_or_else(|| chosenB.centerTile());
                                    claimedTrees.push(chosen);
                                    chosen
                                } else {
                                    chosenB.centerTile()
                                };
                                if let Some(u) = self.unitMut(id) {
                                    u.target = None;
                                    u.returningToDropoff = false;
                                    u.workCycle = 0;
                                    u.order = UnitOrder::Gather {
                                        resource: res,
                                        tile: workTile,
                                        building: Some(chosenBid),
                                    };
                                }
                            }
                            return;
                        }
                        // Se clicou num prédio aliado danificado que não é de recurso, envia cidadãos para reformar!
                        if b.hp < b.maxHp {
                            for &id in units {
                                if let Some(u) = self.unitMut(id) {
                                    if u.owner == *owner && u.class == UnitClass::Citizen {
                                        u.target = None;
                                        u.returningToDropoff = false;
                                        u.order = UnitOrder::Repair(b.id);
                                    }
                                }
                            }
                            return;
                        }
                    }
                }

                // Caso contrário, coleta direta em terreno bruto (Floresta -> Madeira; Montanha/Colina -> Metal).
                let rawResource = match self.map.tile(tile.0, tile.1) {
                    Some(TerrainType::Forest) => Some(ResourceKind::Wood),
                    Some(TerrainType::Mountain | TerrainType::Hill) => Some(ResourceKind::Metal),
                    _ => None,
                };
                if let Some(resource) = rawResource {
                    let mut claimedTrees = if resource == ResourceKind::Wood {
                        self.claimedForestTiles(*owner)
                    } else {
                        Vec::new()
                    };
                    for &id in units {
                        let isCitizen = self
                            .unit(id)
                            .is_some_and(|u| u.owner == *owner && u.class == UnitClass::Citizen);
                        if !isCitizen {
                            continue;
                        }
                        // Se clicou na floresta e existe madeireira próxima com vaga, associa a ela.
                        let linkedCamp = if resource == ResourceKind::Wood {
                            self.findNearestFriendlyBuilding(*owner, "lumber_camp", *tile, 14)
                                .filter(|&cid| {
                                    self.building(cid).is_some_and(|cb| {
                                        self.buildingAssignedWorkers(cid) < cb.workerSlots
                                    })
                                })
                        } else {
                            None
                        };
                        let workTile = if resource == ResourceKind::Wood {
                            let chosen = self
                                .findSpacedForestTile(*tile, 8, &claimedTrees)
                                .unwrap_or(*tile);
                            claimedTrees.push(chosen);
                            chosen
                        } else {
                            *tile
                        };
                        if let Some(u) = self.unitMut(id) {
                            u.target = None;
                            u.returningToDropoff = false;
                            u.workCycle = 0;
                            u.order = UnitOrder::Gather {
                                resource,
                                tile: workTile,
                                building: linkedCamp,
                            };
                        }
                    }
                }
            }
            Command::RepairBuilding { owner, builders, target } => {
                let Some(b) = self.building(*target) else { return };
                if b.owner != *owner {
                    return;
                }
                let isIncomplete = !b.isComplete();
                if !isIncomplete && b.hp >= b.maxHp {
                    return;
                }
                for &uid in builders {
                    if let Some(u) = self.unitMut(uid) {
                        if u.owner == *owner && u.class == UnitClass::Citizen {
                            u.target = None;
                            u.returningToDropoff = false;
                            u.order = if isIncomplete {
                                UnitOrder::Build(*target)
                            } else {
                                UnitOrder::Repair(*target)
                            };
                        }
                    }
                }
            }
            Command::ConstructBuilding { owner, builders, kind, tile, wonderId } => {
                if !self.canPlaceBuilding(*owner, kind, *tile, wonderId.as_deref()) {
                    return;
                }
                let Some(def) = self.data.building(kind).cloned() else { return };
                let Some(cost) = self.effectiveBuildingCost(*owner, kind, wonderId.as_deref()) else {
                    return;
                };
                if let Some(p) = self.playerMut(*owner) {
                    p.resources.subtract(&cost);
                }

                let bid = BuildingId(self.nextBuildingId);
                self.nextBuildingId += 1;
                let workerSlots = self.computeWorkerSlots(&def.id, *tile, def.footprint);
                let building = Building {
                    id: bid,
                    owner: *owner,
                    kind: def.id.clone(),
                    wonderId: wonderId.clone(),
                    tile: *tile,
                    footprint: def.footprint,
                    hp: (def.hp / 5).max(20),
                    maxHp: def.hp,
                    armor: def.armor,
                    buildProgress: 0,
                    buildRequired: def.buildTicks,
                    workerSlots,
                    trainQueue: Vec::new(),
                    attackCooldown: 0,
                    cityId: None,
                };
                self.markFootprint(&building, true);
                self.buildings.push(building);

                for &uid in builders {
                    if let Some(u) = self.unitMut(uid) {
                        if u.owner == *owner && u.class == UnitClass::Citizen {
                            u.target = None;
                            u.order = UnitOrder::Build(bid);
                        }
                    }
                }
            }
            Command::TrainUnit { owner, building, unitKind } => {
                let Some(b) = self.building(*building).cloned() else { return };
                if b.owner != *owner || !b.isComplete() || b.hp <= 0 || b.trainQueue.len() >= 6 {
                    return;
                }
                let Some(uDef) = self.data.unit(unitKind).cloned() else { return };
                if uDef.trainedAt != b.kind {
                    return;
                }
                let Some(player) = self.player(*owner) else { return };
                if player.age < uDef.minAge {
                    return;
                }
                if let Some(reqNation) = &uDef.nationOnly {
                    if reqNation != &player.nationId {
                        return;
                    }
                }
                if !player.resources.canAfford(&uDef.cost) {
                    return;
                }
                let speedBonusPct = if uDef.class == UnitClass::Citizen {
                    player.nation(&self.data).map_or(0, |n| n.citizenTrainSpeedBonusPct)
                } else {
                    0
                };
                let trainTicks = ((uDef.trainTicks as i32 * 100) / (100 + speedBonusPct)).max(20) as u32;

                if let Some(p) = self.playerMut(*owner) {
                    p.resources.subtract(&uDef.cost);
                }
                if let Ok(idx) = self.buildings.binary_search_by_key(&b.id, |x| x.id) {
                    self.buildings[idx].trainQueue.push(TrainOrder {
                        unitKind: uDef.id,
                        remainingTicks: trainTicks,
                        totalTicks: trainTicks,
                    });
                }
            }
            Command::ResearchTech { owner, techId } => {
                if !self.canResearchTech(*owner, techId) {
                    return;
                }
                let Some(tech) = self.data.tech(techId).cloned() else { return };
                let Some(cost) = self.effectiveTechCost(*owner, techId) else { return };
                if let Some(p) = self.playerMut(*owner) {
                    p.resources.subtract(&cost);
                    p.researchedTechs.insert(tech.id.clone());
                    if matches!(tech.effect, TechEffect::RevealMap) {
                        p.mapRevealed = true;
                    }
                }
                self.reapplyPlayerUpgrades(*owner);
                self.recomputeTerritory();
                self.refreshAllFog();
            }
            Command::AdvanceAge { owner } => {
                if !self.canAdvanceAge(*owner) {
                    return;
                }
                let Some(cost) = self.effectiveAgeCost(*owner) else { return };
                if let Some(p) = self.playerMut(*owner) {
                    p.resources.subtract(&cost);
                    p.age += 1;
                }
                self.reapplyPlayerUpgrades(*owner);
                self.recomputeTerritory();
                self.refreshAllFog();
            }
            Command::SetFormation { owner, units, formation } => {
                for &uid in units {
                    if let Some(u) = self.unitMut(uid) {
                        if u.owner == *owner {
                            u.formation = *formation;
                        }
                    }
                }
            }
        }
    }

    // --- Etapas do Tick ---

    fn stepProduction(&mut self) {
        let mut spawned: Vec<(PlayerId, String, Vec2Fx)> = Vec::new();
        for b in &mut self.buildings {
            if !b.isComplete() || b.hp <= 0 || b.trainQueue.is_empty() {
                continue;
            }
            if b.trainQueue[0].remainingTicks > 0 {
                b.trainQueue[0].remainingTicks -= 1;
            }
            if b.trainQueue[0].remainingTicks == 0 {
                let order = b.trainQueue.remove(0);
                let spawnGoal = Vec2Fx::new(
                    b.centerPos().x,
                    Fx::from_num(b.tile.1 + b.footprint) + Fx::ONE / Fx::from_num(2),
                );
                spawned.push((b.owner, order.unitKind, spawnGoal));
            }
        }
        for (owner, kind, goal) in spawned {
            let pos = snapWalkable(&self.map, &self.blockedTiles, goal);
            self.spawnUnitInternal(owner, &kind, pos);
        }
    }

    fn stepEconomy(&mut self) {
        // Reseta estado visual de trabalho antes de reavaliar neste tick.
        for u in &mut self.units {
            u.workAnim = WorkAnim::None;
        }

        // 1. Construção e Reforma/Reparo por cidadãos adjacentes.
        let mut buildWork: Vec<(BuildingId, u32)> = Vec::new();
        let mut repairWork: Vec<(BuildingId, i32)> = Vec::new();
        for i in 0..self.units.len() {
            match self.units[i].order {
                UnitOrder::Build(bid) => {
                    let targetInfo = self
                        .building(bid)
                        .filter(|b| !b.isComplete())
                        .map(|b| (b.edgeDistance(self.units[i].pos), b.centerPos()));
                    if let Some((edgeDist, bCenter)) = targetInfo {
                        if edgeDist <= Fx::from_num(2) {
                            buildWork.push((bid, 2));
                            self.units[i].workAnim = WorkAnim::Building;
                            self.units[i].facing = (bCenter - self.units[i].pos).normalizeOrZero();
                        }
                    }
                }
                UnitOrder::Repair(bid) => {
                    let targetInfo = self
                        .building(bid)
                        .filter(|b| b.owner == self.units[i].owner && b.isComplete() && b.hp < b.maxHp)
                        .map(|b| (b.edgeDistance(self.units[i].pos), b.centerPos()));
                    if let Some((edgeDist, bCenter)) = targetInfo {
                        if edgeDist <= Fx::from_num(2) {
                            repairWork.push((bid, 4));
                            self.units[i].workAnim = WorkAnim::Building;
                            self.units[i].facing = (bCenter - self.units[i].pos).normalizeOrZero();
                        }
                    } else {
                        self.units[i].order = UnitOrder::Idle;
                    }
                }
                _ => {}
            }
        }
        for (bid, heal) in repairWork {
            if let Ok(idx) = self.buildings.binary_search_by_key(&bid, |x| x.id) {
                let b = &mut self.buildings[idx];
                b.hp = (b.hp + heal).min(b.maxHp);
            }
        }
        // Aura passiva do Templo: restaura lentamente unidades/prédios aliados próximos.
        if self.tick % 40 == 0 {
            let temples: Vec<(PlayerId, Vec2Fx)> = self
                .buildings
                .iter()
                .filter(|b| b.kind == "temple" && b.isComplete() && b.hp > 0)
                .map(|b| (b.owner, b.centerPos()))
                .collect();
            let auraSq = Fx::from_num(8) * Fx::from_num(8);
            for (tOwner, tPos) in temples {
                for u in &mut self.units {
                    if u.owner == tOwner && u.hp < u.maxHp && u.pos.distanceSquared(tPos) <= auraSq {
                        u.hp = (u.hp + 2).min(u.maxHp);
                    }
                }
                for b in &mut self.buildings {
                    if b.owner == tOwner
                        && b.isComplete()
                        && b.hp > 0
                        && b.hp < b.maxHp
                        && b.centerPos().distanceSquared(tPos) <= auraSq
                    {
                        b.hp = (b.hp + 4).min(b.maxHp);
                    }
                }
            }
        }
        let mut newlyCompleted: Vec<BuildingId> = Vec::new();
        for (bid, amount) in buildWork {
            if let Ok(idx) = self.buildings.binary_search_by_key(&bid, |x| x.id) {
                let b = &mut self.buildings[idx];
                if !b.isComplete() {
                    b.buildProgress = (b.buildProgress + amount).min(b.buildRequired);
                    let targetHp = ((b.maxHp as u64 * b.buildProgress as u64)
                        / b.buildRequired.max(1) as u64) as i32;
                    b.hp = b.hp.max(targetHp).min(b.maxHp);
                    if b.isComplete() {
                        b.hp = b.maxHp;
                        newlyCompleted.push(bid);
                    }
                }
            }
        }
        if !newlyCompleted.is_empty() {
            for bid in newlyCompleted {
                self.onBuildingCompleted(bid);
            }
            self.recomputeTerritory();
            self.refreshAllFog();
        }

        // Limpa ordens de construção já finalizadas (se for prédio de recurso com vaga, cidadão começa a coletar!).
        for i in 0..self.units.len() {
            if let UnitOrder::Build(bid) = self.units[i].order {
                match self.building(bid).cloned() {
                    None => self.units[i].order = UnitOrder::Idle,
                    Some(b) if b.isComplete() => {
                        if let Some(res) =
                            self.data.building(&b.kind).and_then(|d| d.producesResource)
                        {
                            let uid = self.units[i].id;
                            let hasSlot = self.buildingAssignedWorkers(b.id) < b.workerSlots;
                            if hasSlot {
                                let workTile = if b.kind == "lumber_camp" {
                                    let claimed = self.claimedForestTiles(b.owner);
                                    self.findSpacedForestTile(b.centerTile(), 14, &claimed)
                                        .unwrap_or_else(|| b.centerTile())
                                } else {
                                    b.centerTile()
                                };
                                self.units[i].returningToDropoff = false;
                                self.units[i].workCycle = 0;
                                self.units[i].order = UnitOrder::Gather {
                                    resource: res,
                                    tile: workTile,
                                    building: Some(b.id),
                                };
                            } else if let Some(altBid) =
                                self.pickBuildingWithFreeSlot(b.owner, &b.kind, None, uid)
                            {
                                let altTile = self.building(altBid).unwrap().centerTile();
                                self.units[i].returningToDropoff = false;
                                self.units[i].workCycle = 0;
                                self.units[i].order = UnitOrder::Gather {
                                    resource: res,
                                    tile: altTile,
                                    building: Some(altBid),
                                };
                            } else {
                                self.units[i].order = UnitOrder::Idle;
                            }
                        } else {
                            self.units[i].order = UnitOrder::Idle;
                        }
                    }
                    _ => {}
                }
            }
        }

        // 2. Atualiza animações e ciclo de trabalho (fazendeiro na fazenda, mercador no mercado, lenhador na floresta).
        for i in 0..self.units.len() {
            let UnitOrder::Gather { resource, tile, building } = self.units[i].order else {
                continue;
            };
            let uPos = self.units[i].pos;
            let uId = self.units[i].id;

            let linkedBuilding = building.and_then(|bid| {
                self.building(bid)
                    .filter(|b| b.owner == self.units[i].owner && b.isComplete())
                    .cloned()
            });

            if let Some(b) = linkedBuilding {
                if b.kind == "farm" {
                    let spot = farmWorkSpot(&b, uId, self.tick);
                    if uPos.distance(spot) <= Fx::from_num(35) / Fx::from_num(100) {
                        self.units[i].workAnim = WorkAnim::Farming;
                        self.units[i].facing = (b.centerPos() - uPos).normalizeOrZero();
                    }
                } else if b.kind == "market" {
                    // Funcionários do mercado trabalham DENTRO do pátio comercial com animação!
                    let spot = marketWorkSpot(&b, uId, self.tick);
                    if uPos.distance(spot) <= Fx::from_num(35) / Fx::from_num(100) {
                        self.units[i].workAnim = WorkAnim::Market;
                        self.units[i].facing = (b.centerPos() - uPos).normalizeOrZero();
                    }
                } else if b.kind == "lumber_camp" && self.map.tile(tile.0, tile.1) == Some(TerrainType::Forest) {
                    if self.units[i].returningToDropoff {
                        if b.edgeDistance(uPos) <= Fx::from_num(13) / Fx::from_num(10) {
                            self.units[i].returningToDropoff = false;
                            self.units[i].workCycle = 0;
                        }
                    } else {
                        let treePos = Vec2Fx::tileCenter(tile.0, tile.1);
                        if uPos.distance(treePos) <= Fx::from_num(18) / Fx::from_num(10) {
                            self.units[i].workAnim = WorkAnim::Chopping;
                            self.units[i].facing = (treePos - uPos).normalizeOrZero();
                            self.units[i].workCycle += 1;
                            if self.units[i].workCycle >= 70 {
                                self.units[i].returningToDropoff = true;
                                self.units[i].workCycle = 0;
                            }
                        }
                    }
                } else if b.edgeDistance(uPos) <= Fx::from_num(18) / Fx::from_num(10) {
                    self.units[i].facing = (b.centerPos() - uPos).normalizeOrZero();
                    self.units[i].workAnim = match resource {
                        ResourceKind::Metal => WorkAnim::Mining,
                        ResourceKind::Wood => WorkAnim::Chopping,
                        ResourceKind::Food => WorkAnim::Farming,
                        ResourceKind::Wealth => WorkAnim::Market,
                        _ => WorkAnim::Working,
                    };
                }
            } else {
                let targetPos = Vec2Fx::tileCenter(tile.0, tile.1);
                if uPos.distance(targetPos) <= Fx::from_num(18) / Fx::from_num(10) {
                    self.units[i].facing = (targetPos - uPos).normalizeOrZero();
                    self.units[i].workAnim = match resource {
                        ResourceKind::Wood => WorkAnim::Chopping,
                        ResourceKind::Metal => WorkAnim::Mining,
                        _ => WorkAnim::Working,
                    };
                }
            }
        }

        // 3. Coleta periódica a cada 1 segundo (20 ticks) respeitando vagas por prédio e limite por cidade.
        if self.tick % u64::from(TICKS_PER_SECOND) != 0 {
            return;
        }

        let playerIds: Vec<PlayerId> = self.players.iter().map(|p| p.id).collect();
        for pid in playerIds {
            let mut rawRates = ResourceBundle::ZERO;
            let mut activePerBuilding: Vec<(BuildingId, u32)> = Vec::new();

            for u in &self.units {
                if u.owner != pid {
                    continue;
                }
                if let UnitOrder::Gather { resource, tile, building } = u.order {
                    let active = if let Some(bid) = building {
                        self.building(bid).is_some_and(|b| {
                            if b.owner != pid || !b.isComplete() {
                                return false;
                            }
                            let used = activePerBuilding
                                .iter()
                                .find(|&&(id, _)| id == bid)
                                .map_or(0, |&(_, c)| c);
                            if used >= b.workerSlots {
                                return false;
                            }
                            if b.kind == "lumber_camp"
                                && self.map.tile(tile.0, tile.1) == Some(TerrainType::Forest)
                            {
                                let treePos = Vec2Fx::tileCenter(tile.0, tile.1);
                                u.pos.distance(treePos) <= Fx::from_num(16)
                                    || b.edgeDistance(u.pos) <= Fx::from_num(16)
                            } else {
                                b.edgeDistance(u.pos) <= Fx::from_num(2)
                            }
                        })
                    } else {
                        u.pos.distance(Vec2Fx::tileCenter(tile.0, tile.1)) <= Fx::from_num(2)
                    };
                    if active {
                        if let Some(bid) = building {
                            if let Some(entry) = activePerBuilding.iter_mut().find(|(id, _)| *id == bid) {
                                entry.1 += 1;
                            } else {
                                activePerBuilding.push((bid, 1));
                            }
                        }
                        let basePerMin = if building.is_some() { 24 } else { 15 };
                        rawRates.addOne(resource, basePerMin);
                    }
                }
            }

            let cityCount = self.cityCount(pid);
            let Some(player) = self.player(pid) else { continue };
            let cap = player.effectiveCommerceCap(&self.data, cityCount);
            let passiveKnow = player.passiveKnowledgePerMin(&self.data);

            let mut finalRates = ResourceBundle::ZERO;
            for kind in ResourceKind::ALL {
                let raw = rawRates.get(kind);
                let bonusPct = player.gatherBonusPct(&self.data, kind);
                let mut boosted = (raw * (100 + bonusPct)) / 100;
                if kind == ResourceKind::Knowledge {
                    boosted += passiveKnow;
                }
                let capped = boosted.min(cap);
                finalRates.addOne(kind, capped);
            }

            if let Some(p) = self.playerMut(pid) {
                p.commerceCap = cap;
                p.gatherRates = finalRates;
                // Acumula `ratePerMin` por segundo e converte cada 60 unidades em 1 recurso inteiro.
                for kind in ResourceKind::ALL {
                    let rate = finalRates.get(kind);
                    p.gatherAccum.addOne(kind, rate);
                    let whole = p.gatherAccum.get(kind) / 60;
                    if whole > 0 {
                        p.resources.addOne(kind, whole);
                        p.gatherAccum.addOne(kind, -whole * 60);
                    }
                }
            }
        }
    }

    fn stepMovement(&mut self) {
        let count = self.units.len();
        if count == 0 {
            return;
        }
        let positions: Vec<Vec2Fx> = self.units.iter().map(|u| u.pos).collect();
        let stepSepRadius = Fx::from_num(68) / Fx::from_num(100);
        let sepRadiusSq = stepSepRadius * stepSepRadius;

        for i in 0..count {
            self.units[i].prevPos = self.units[i].pos;

            let moveGoal = self.computeMoveGoal(i);
            let Some((goalPos, stopDist)) = moveGoal else { continue };

            let unitPos = self.units[i].pos;
            let distToGoal = unitPos.distance(goalPos);
            let stepLength = self.units[i].speed / Fx::from_num(TICKS_PER_SECOND);

            if distToGoal <= stopDist {
                if let UnitOrder::Move { target } = self.units[i].order {
                    if distToGoal <= stepLength {
                        if self.isPosWalkable(target) {
                            self.units[i].pos = target;
                        }
                        self.units[i].target = None;
                        self.units[i].order = UnitOrder::Idle;
                        continue;
                    }
                } else {
                    continue;
                }
            }

            // Se está próximo (< 1.5 tiles) e o caminho direto está livre, anda direto ao ponto exato;
            // senão consulta o Flow Field determinístico até o tile alvo.
            let dir = if distToGoal <= Fx::from_num(3) / Fx::from_num(2)
                && self.isPosWalkable(goalPos)
            {
                (goalPos - unitPos).normalizeOrZero()
            } else {
                let goalTile = self.posToTile(goalPos);
                let curTile = self.posToTile(unitPos);
                let flowDir = {
                    let field = self.flowCache.getOrCompute(
                        self.buildingVersion,
                        &self.map,
                        &self.blockedTiles,
                        goalTile,
                    );
                    field.directionAt(curTile.0, curTile.1)
                };
                if flowDir == (0, 0) {
                    (goalPos - unitPos).normalizeOrZero()
                } else {
                    let nextCenter = Vec2Fx::tileCenter(
                        (curTile.0 as i32 + i32::from(flowDir.0)) as u32,
                        (curTile.1 as i32 + i32::from(flowDir.1)) as u32,
                    );
                    (nextCenter - unitPos).normalizeOrZero()
                }
            };

            // Força leve de separação entre unidades próximas (evita empilhamento).
            let mut sep = Vec2Fx::ZERO;
            for (j, &otherPos) in positions.iter().enumerate() {
                if i == j {
                    continue;
                }
                let diff = unitPos - otherPos;
                let dSq = diff.lengthSquared();
                if dSq > Fx::ZERO && dSq < sepRadiusSq {
                    let d = dSq.sqrt();
                    let push = (stepSepRadius - d) / stepSepRadius;
                    sep = sep + (diff * (push / d));
                }
            }
            let combinedDir = (dir + sep * (Fx::from_num(35) / Fx::from_num(100))).normalizeOrZero();
            if combinedDir == Vec2Fx::ZERO {
                continue;
            }

            let step = if distToGoal <= stepLength && matches!(self.units[i].order, UnitOrder::Move { .. }) {
                goalPos - unitPos
            } else {
                combinedDir * stepLength
            };
            let nextPos = self.map.clampPoint(unitPos + step);

            if self.isPosWalkable(nextPos) {
                self.units[i].pos = nextPos;
                self.units[i].facing = combinedDir;
                if matches!(self.units[i].order, UnitOrder::Move { .. })
                    && nextPos == goalPos
                {
                    self.units[i].target = None;
                    self.units[i].order = UnitOrder::Idle;
                }
            } else {
                // Tenta deslizar por eixo antes de desistir.
                let slideX = self.map.clampPoint(Vec2Fx::new(nextPos.x, unitPos.y));
                let slideY = self.map.clampPoint(Vec2Fx::new(unitPos.x, nextPos.y));
                if step.x != Fx::ZERO && self.isPosWalkable(slideX) {
                    self.units[i].pos = slideX;
                    self.units[i].facing = (slideX - unitPos).normalizeOrZero();
                } else if step.y != Fx::ZERO && self.isPosWalkable(slideY) {
                    self.units[i].pos = slideY;
                    self.units[i].facing = (slideY - unitPos).normalizeOrZero();
                } else if matches!(self.units[i].order, UnitOrder::Move { .. }) {
                    self.units[i].target = None;
                    self.units[i].order = UnitOrder::Idle;
                }
            }
        }
    }

    fn computeMoveGoal(&self, unitIndex: usize) -> Option<(Vec2Fx, Fx)> {
        let u = &self.units[unitIndex];
        match u.order {
            UnitOrder::Idle => None,
            UnitOrder::Move { target } => Some((target, Fx::from_num(15) / Fx::from_num(100))),
            UnitOrder::AttackUnit(targetId) => {
                let target = self.unit(targetId)?;
                Some((target.pos, u.range))
            }
            UnitOrder::AttackBuilding(bid) => {
                let b = self.building(bid)?;
                let stop = u.range + Fx::from_num(b.footprint) / Fx::from_num(2);
                Some((b.centerPos(), stop))
            }
            UnitOrder::Build(bid) | UnitOrder::Repair(bid) => {
                let b = self.building(bid)?;
                let stop = Fx::from_num(b.footprint) / Fx::from_num(2) + Fx::from_num(85) / Fx::from_num(100);
                Some((b.centerPos(), stop))
            }
            UnitOrder::Gather { tile, building, .. } => {
                if let Some(bid) = building {
                    let b = self.building(bid)?;
                    if b.kind == "farm" {
                        let spot = farmWorkSpot(b, u.id, self.tick);
                        Some((spot, Fx::from_num(20) / Fx::from_num(100)))
                    } else if b.kind == "market" {
                        let spot = marketWorkSpot(b, u.id, self.tick);
                        Some((spot, Fx::from_num(20) / Fx::from_num(100)))
                    } else if b.kind == "lumber_camp"
                        && self.map.tile(tile.0, tile.1) == Some(TerrainType::Forest)
                    {
                        if u.returningToDropoff {
                            let stop = Fx::from_num(b.footprint) / Fx::from_num(2)
                                + Fx::from_num(55) / Fx::from_num(100);
                            Some((b.centerPos(), stop))
                        } else {
                            Some((
                                Vec2Fx::tileCenter(tile.0, tile.1),
                                Fx::from_num(145) / Fx::from_num(100),
                            ))
                        }
                    } else {
                        let stop = Fx::from_num(b.footprint) / Fx::from_num(2)
                            + Fx::from_num(65) / Fx::from_num(100);
                        Some((b.centerPos(), stop))
                    }
                } else {
                    Some((Vec2Fx::tileCenter(tile.0, tile.1), Fx::from_num(145) / Fx::from_num(100)))
                }
            }
        }
    }

    fn stepCombatAndCapture(&mut self) {
        // 1. Auto-aquisição de alvos inimigos próximos para unidades militares ociosas.
        let aggroRange = Fx::from_num(7);
        for i in 0..self.units.len() {
            if self.units[i].attackCooldown > 0 {
                self.units[i].attackCooldown -= 1;
            }
            if self.units[i].class == UnitClass::Citizen {
                continue;
            }
            // Limpa alvo morto/convertido.
            match self.units[i].order {
                UnitOrder::AttackUnit(tid) => {
                    if self.unit(tid).is_none_or(|t| t.owner == self.units[i].owner) {
                        self.units[i].order = UnitOrder::Idle;
                    }
                }
                UnitOrder::AttackBuilding(bid) => {
                    if self.building(bid).is_none_or(|b| b.owner == self.units[i].owner) {
                        self.units[i].order = UnitOrder::Idle;
                    }
                }
                UnitOrder::Idle => {
                    let owner = self.units[i].owner;
                    let pos = self.units[i].pos;
                    let searchRadius = aggroRange.max(self.units[i].range);
                    let searchSq = searchRadius * searchRadius;
                    let mut best: Option<(Fx, UnitId)> = None;
                    for other in &self.units {
                        if other.owner == owner || other.hp <= 0 {
                            continue;
                        }
                        let dSq = pos.distanceSquared(other.pos);
                        if dSq <= searchSq && best.is_none_or(|(bd, _)| dSq < bd) {
                            best = Some((dSq, other.id));
                        }
                    }
                    if let Some((_, tid)) = best {
                        self.units[i].order = UnitOrder::AttackUnit(tid);
                    }
                }
                _ => {}
            }
        }

        // 2. Ataques de unidades.
        let mut unitDmg: Vec<(UnitId, i32)> = Vec::new();
        let mut bldDmg: Vec<(BuildingId, i32)> = Vec::new();

        for i in 0..self.units.len() {
            if self.units[i].attackCooldown > 0 {
                continue;
            }
            let attacker = self.units[i].clone();
            match attacker.order {
                UnitOrder::AttackUnit(tid) => {
                    let targetInfo = self
                        .unit(tid)
                        .filter(|t| t.owner != attacker.owner)
                        .map(|t| (t.pos, t.class, t.armor));
                    if let Some((tPos, tClass, tArmor)) = targetInfo {
                        if attacker.pos.distance(tPos)
                            <= attacker.range + Fx::from_num(25) / Fx::from_num(100)
                        {
                            let mult = counterMultiplierPct(attacker.class, tClass);
                            let dmg = computeDamage(attacker.attack, mult, tArmor);
                            unitDmg.push((tid, dmg));
                            self.units[i].attackCooldown = attacker.attackCooldownMax;
                            self.units[i].facing = (tPos - attacker.pos).normalizeOrZero();
                            self.combatEvents.push(CombatEvent {
                                tick: self.tick,
                                owner: attacker.owner,
                                from: attacker.pos,
                                to: tPos,
                                isRanged: attacker.range > Fx::from_num(2),
                                isSiege: attacker.class == UnitClass::Siege,
                            });
                        }
                    }
                }
                UnitOrder::AttackBuilding(bid) => {
                    let targetInfo = self
                        .building(bid)
                        .filter(|t| t.owner != attacker.owner && t.hp > 0)
                        .map(|t| (t.centerPos(), t.edgeDistance(attacker.pos), t.armor));
                    if let Some((tCenter, edgeDist, tArmor)) = targetInfo {
                        if edgeDist <= attacker.range + Fx::from_num(45) / Fx::from_num(100) {
                            let mult = buildingCounterMultiplierPct(attacker.class);
                            let dmg = computeDamage(attacker.attack, mult, tArmor);
                            bldDmg.push((bid, dmg));
                            self.units[i].attackCooldown = attacker.attackCooldownMax;
                            self.units[i].facing = (tCenter - attacker.pos).normalizeOrZero();
                            self.combatEvents.push(CombatEvent {
                                tick: self.tick,
                                owner: attacker.owner,
                                from: attacker.pos,
                                to: tCenter,
                                isRanged: attacker.range > Fx::from_num(2),
                                isSiege: attacker.class == UnitClass::Siege,
                            });
                        }
                    }
                }
                _ => {}
            }
        }

        // 3. Fogo defensivo de Torres e Centros Urbanos saudáveis.
        for bi in 0..self.buildings.len() {
            if self.buildings[bi].attackCooldown > 0 {
                self.buildings[bi].attackCooldown -= 1;
            }
            if !self.buildings[bi].isComplete()
                || self.buildings[bi].hp <= 0
                || self.buildings[bi].attackCooldown > 0
            {
                continue;
            }
            let Some(def) = self.data.building(&self.buildings[bi].kind).cloned() else {
                continue;
            };
            if def.attack <= 0 {
                continue;
            }
            let bCenter = self.buildings[bi].centerPos();
            let bOwner = self.buildings[bi].owner;
            let range = fxFromHundredths(def.attackRangeHundredths);
            let rangeSq = range * range;
            let mut best: Option<(Fx, UnitId, Vec2Fx, i32)> = None;
            for u in &self.units {
                if u.owner == bOwner || u.hp <= 0 {
                    continue;
                }
                let dSq = bCenter.distanceSquared(u.pos);
                if dSq <= rangeSq && best.is_none_or(|(bd, _, _, _)| dSq < bd) {
                    best = Some((dSq, u.id, u.pos, u.armor));
                }
            }
            if let Some((_, tid, tPos, tArmor)) = best {
                let dmg = computeDamage(def.attack, 100, tArmor);
                unitDmg.push((tid, dmg));
                self.buildings[bi].attackCooldown = def.attackCooldownTicks;
                self.combatEvents.push(CombatEvent {
                    tick: self.tick,
                    owner: bOwner,
                    from: bCenter,
                    to: tPos,
                    isRanged: true,
                    isSiege: false,
                });
            }
        }

        // Aplica danos às unidades e remove mortas.
        for (uid, dmg) in unitDmg {
            if let Some(u) = self.unitMut(uid) {
                u.hp -= dmg;
            }
        }
        self.units.retain(|u| u.hp > 0);

        // Aplica danos às construções; Centros Urbanos chegam a 0 HP e ficam capturáveis!
        let mut destroyedBuildings: Vec<BuildingId> = Vec::new();
        for (bid, dmg) in bldDmg {
            if let Ok(idx) = self.buildings.binary_search_by_key(&bid, |b| b.id) {
                let isCity = self.buildings[idx].cityId.is_some();
                self.buildings[idx].hp = (self.buildings[idx].hp - dmg).max(0);
                if !isCity && self.buildings[idx].hp == 0 {
                    destroyedBuildings.push(bid);
                }
            }
        }
        if !destroyedBuildings.is_empty() {
            for bid in &destroyedBuildings {
                if let Some(b) = self.building(*bid).cloned() {
                    self.markFootprint(&b, false);
                }
            }
            self.buildings.retain(|b| !destroyedBuildings.contains(&b.id));
            self.recomputeTerritory();
        }

        // 4. Captura de Cidades (quando Centro Urbano está com 0 HP e há tropa inimiga próxima).
        let mut capturedCities: Vec<(CityId, PlayerId)> = Vec::new();
        for ci in 0..self.cities.len() {
            let city = &self.cities[ci];
            let Some(b) = self.building(city.buildingId) else { continue };
            if b.hp > 0 {
                self.cities[ci].captureProgress = 0;
                self.cities[ci].capturingPlayer = None;
                continue;
            }
            let cPos = b.centerPos();
            let cOwner = city.owner;
            let defenderNear = self.units.iter().any(|u| {
                u.owner == cOwner
                    && u.class != UnitClass::Citizen
                    && u.pos.distance(cPos) <= Fx::from_num(4)
            });
            if defenderNear {
                continue;
            }
            let attacker = self
                .units
                .iter()
                .find(|u| {
                    u.owner != cOwner
                        && u.class != UnitClass::Citizen
                        && u.pos.distance(cPos) <= Fx::from_num(5)
                })
                .map(|u| u.owner);

            if let Some(capturer) = attacker {
                if self.cities[ci].capturingPlayer != Some(capturer) {
                    self.cities[ci].capturingPlayer = Some(capturer);
                    self.cities[ci].captureProgress = 0;
                }
                self.cities[ci].captureProgress = (self.cities[ci].captureProgress + 5).min(100);
                if self.cities[ci].captureProgress >= 100 {
                    capturedCities.push((self.cities[ci].id, capturer));
                }
            } else if self.cities[ci].captureProgress > 0 {
                self.cities[ci].captureProgress = self.cities[ci].captureProgress.saturating_sub(2);
            }
        }

        for (cid, newOwner) in capturedCities {
            self.transferCity(cid, newOwner);
        }
    }

    fn transferCity(&mut self, cityId: CityId, newOwner: PlayerId) {
        let Some(cIdx) = self.cities.iter().position(|c| c.id == cityId) else { return };
        let oldOwner = self.cities[cIdx].owner;
        let centerTile = self.cities[cIdx].centerTile;
        let radius = self.cities[cIdx].baseRadius as i32;
        let cityBid = self.cities[cIdx].buildingId;

        self.cities[cIdx].owner = newOwner;
        self.cities[cIdx].captureProgress = 0;
        self.cities[cIdx].capturingPlayer = None;

        // Converte o Centro Urbano (com 40% de HP restaurado) e prédios periféricos da cidade.
        for b in &mut self.buildings {
            if b.id == cityBid {
                b.owner = newOwner;
                b.hp = (b.maxHp * 40) / 100;
                b.trainQueue.clear();
            } else if b.owner == oldOwner && b.cityId.is_none() {
                let (bx, by) = b.centerTile();
                let dx = bx as i32 - centerTile.0 as i32;
                let dy = by as i32 - centerTile.1 as i32;
                if dx * dx + dy * dy <= radius * radius {
                    b.owner = newOwner;
                    b.trainQueue.clear();
                }
            }
        }

        self.recomputeTerritory();
        self.refreshAllFog();

        if self.cityCount(oldOwner) == 0 && self.cities.len() > 0 {
            self.winner = Some(newOwner);
        }
    }

    fn stepAttrition(&mut self) {
        let mut died = false;
        for i in 0..self.units.len() {
            if self.units[i].class == UnitClass::Citizen {
                self.units[i].inEnemyTerritory = false;
                continue;
            }
            let owner = self.units[i].owner;
            let pos = self.units[i].pos;
            let tileOwner = self.territory.ownerAtPos(&self.map, pos);
            if let Some(hostile) = tileOwner.filter(|&o| o != owner) {
                self.units[i].inEnemyTerritory = true;
                self.units[i].attritionTimer += 1;
                if self.units[i].attritionTimer >= 40 {
                    self.units[i].attritionTimer = 0;
                    let reduction = self
                        .player(owner)
                        .map_or(0, |p| p.attritionReductionPct(&self.data));
                    let enemyBonus = self
                        .player(hostile)
                        .map_or(0, |p| p.enemyAttritionBonusPct(&self.data));
                    let dmg = ((4 * (100 + enemyBonus) * (100 - reduction)) / 10_000).max(1);
                    self.units[i].hp -= dmg;
                    if self.units[i].hp <= 0 {
                        died = true;
                    }
                }
            } else {
                self.units[i].inEnemyTerritory = false;
                self.units[i].attritionTimer = 0;
            }
        }
        if died {
            self.units.retain(|u| u.hp > 0);
        }
    }

    // --- Helpers internos ---

    fn spawnUnitInternal(&mut self, owner: PlayerId, kind: &str, pos: Vec2Fx) -> UnitId {
        let def = self
            .data
            .unit(kind)
            .cloned()
            .unwrap_or_else(|| self.data.units[0].clone());
        let (maxHp, attack, armor, age) = self
            .player(owner)
            .map(|p| {
                let (h, a, r) = p.effectiveUnitStats(&self.data, &def);
                (h, a, r, p.age)
            })
            .unwrap_or((def.hp, def.attack, def.armor, 0));

        let id = UnitId(self.nextUnitId);
        self.nextUnitId += 1;
        let speed = if kind == "infantry" && self.cities.is_empty() {
            // Preserva exatamente velocidade 3 tiles/s nos cenários simples da fase 0/1.
            Fx::from_num(3)
        } else {
            fxFromHundredths(def.speedHundredths)
        };
        self.units.push(Unit {
            id,
            owner,
            kind: def.id,
            class: def.class,
            pos,
            prevPos: pos,
            facing: Vec2Fx::fromInts(1, 0),
            target: None,
            speed,
            hp: maxHp,
            maxHp,
            attack,
            armor,
            range: fxFromHundredths(def.rangeHundredths),
            attackCooldown: 0,
            attackCooldownMax: def.attackCooldownTicks,
            visionTiles: def.visionTiles,
            order: UnitOrder::Idle,
            formation: FormationKind::Box,
            eraTier: age,
            inEnemyTerritory: false,
            attritionTimer: 0,
            workAnim: WorkAnim::None,
            workCycle: 0,
            returningToDropoff: false,
        });
        id
    }

    fn spawnCompletedBuilding(
        &mut self,
        owner: PlayerId,
        kind: &str,
        tile: (u32, u32),
        wonderId: Option<String>,
    ) -> Option<BuildingId> {
        let def = self.data.building(kind)?.clone();
        let bid = BuildingId(self.nextBuildingId);
        self.nextBuildingId += 1;
        let workerSlots = self.computeWorkerSlots(&def.id, tile, def.footprint);
        let building = Building {
            id: bid,
            owner,
            kind: def.id,
            wonderId,
            tile,
            footprint: def.footprint,
            hp: def.hp,
            maxHp: def.hp,
            armor: def.armor,
            buildProgress: def.buildTicks,
            buildRequired: def.buildTicks,
            workerSlots,
            trainQueue: Vec::new(),
            attackCooldown: 0,
            cityId: None,
        };
        self.markFootprint(&building, true);
        self.buildings.push(building);
        self.onBuildingCompleted(bid);
        Some(bid)
    }

    fn onBuildingCompleted(&mut self, bid: BuildingId) {
        let Some(b) = self.building(bid).cloned() else { return };
        let Some(def) = self.data.building(&b.kind).cloned() else { return };

        if def.isCityCenter && b.cityId.is_none() {
            let cid = CityId(self.nextCityId);
            self.nextCityId += 1;
            let cityName = self.nextCityName(b.owner);
            self.cities.push(City {
                id: cid,
                owner: b.owner,
                name: cityName,
                centerTile: b.centerTile(),
                buildingId: bid,
                baseRadius: def.borderRadius,
                captureProgress: 0,
                capturingPlayer: None,
            });
            if let Ok(idx) = self.buildings.binary_search_by_key(&bid, |x| x.id) {
                self.buildings[idx].cityId = Some(cid);
            }
        }

        if def.isWonder {
            if let Some(wId) = &b.wonderId {
                if let Some(p) = self.playerMut(b.owner) {
                    if !p.wondersBuilt.iter().any(|x| x == wId) {
                        p.wondersBuilt.push(wId.clone());
                    }
                }
                self.reapplyPlayerUpgrades(b.owner);
            }
        }
    }

    fn nextCityName(&mut self, owner: PlayerId) -> String {
        let idx = self.player(owner).map_or(0, |p| p.citiesFounded as usize);
        if let Some(p) = self.playerMut(owner) {
            p.citiesFounded += 1;
        }
        if let Some(nation) = self.player(owner).and_then(|p| p.nation(&self.data)) {
            if let Some(name) = nation.cityNames.get(idx) {
                return name.clone();
            }
            return format!("{} {}", nation.name, idx + 1);
        }
        format!("Cidade {}", idx + 1)
    }

    fn reapplyPlayerUpgrades(&mut self, owner: PlayerId) {
        let Some(player) = self.player(owner).cloned() else { return };
        for u in &mut self.units {
            if u.owner != owner {
                continue;
            }
            let Some(def) = self.data.unit(&u.kind) else { continue };
            let (newMaxHp, newAtk, newArmor) = player.effectiveUnitStats(&self.data, def);
            let hpDelta = (newMaxHp - u.maxHp).max(0);
            u.maxHp = newMaxHp;
            u.hp = (u.hp + hpDelta).min(newMaxHp);
            u.attack = newAtk;
            u.armor = newArmor;
            u.eraTier = player.age;
        }
    }

    fn markFootprint(&mut self, b: &Building, solid: bool) {
        // Fazendas e Mercados são caminháveis para trabalhadores circularem no interior.
        if b.kind != "farm" && b.kind != "market" {
            for dy in 0..b.footprint {
                for dx in 0..b.footprint {
                    let tx = b.tile.0 + dx;
                    let ty = b.tile.1 + dy;
                    if tx < self.map.width && ty < self.map.height {
                        self.blockedTiles[(ty * self.map.width + tx) as usize] = solid;
                    }
                }
            }
        }
        self.buildingVersion += 1;
    }

    /// Escolhe o prédio preferido se tiver vaga (ou o cidadão já estiver nele), senão outro do mesmo tipo com vaga.
    fn pickBuildingWithFreeSlot(
        &self,
        owner: PlayerId,
        kind: &str,
        preferred: Option<BuildingId>,
        unitId: UnitId,
    ) -> Option<BuildingId> {
        let hasSlotFor = |bid: BuildingId| -> bool {
            let Some(b) = self.building(bid) else { return false };
            if b.owner != owner || b.kind != kind || !b.isComplete() {
                return false;
            }
            let alreadyThere = self.unit(unitId).is_some_and(|u| {
                matches!(u.order, UnitOrder::Gather { building: Some(id), .. } if id == bid)
            });
            alreadyThere || self.buildingAssignedWorkers(bid) < b.workerSlots
        };
        if let Some(pref) = preferred {
            if hasSlotFor(pref) {
                return Some(pref);
            }
        }
        self.buildings
            .iter()
            .find(|b| hasSlotFor(b.id))
            .map(|b| b.id)
    }

    fn recomputeTerritory(&mut self) {
        let mut sources: Vec<(PlayerId, (u32, u32), u32)> = Vec::new();
        for city in &self.cities {
            let bonus = self
                .player(city.owner)
                .map_or(0, |p| p.borderRadiusBonus(&self.data));
            sources.push((city.owner, city.centerTile, city.baseRadius + bonus));
        }
        for b in &self.buildings {
            if !b.isComplete() || b.cityId.is_some() {
                continue;
            }
            if let Some(def) = self.data.building(&b.kind) {
                if def.borderRadius > 0 {
                    sources.push((b.owner, b.centerTile(), def.borderRadius));
                }
            }
        }
        self.territory.recompute(&sources);
    }

    fn refreshAllFog(&mut self) {
        let (w, h) = (self.map.width, self.map.height);
        for pi in 0..self.players.len() {
            let pid = self.players[pi].id;
            let visionBonus = self.players[pi].visionBonus(&self.data);
            let revealAll = self.players[pi].mapRevealed;

            let mut sources: Vec<((u32, u32), u32)> = Vec::new();
            for u in &self.units {
                if u.owner == pid {
                    sources.push((self.posToTile(u.pos), u.visionTiles + visionBonus));
                }
            }
            for b in &self.buildings {
                if b.owner == pid {
                    let baseVis = self.data.building(&b.kind).map_or(6, |d| d.visionTiles);
                    sources.push((b.centerTile(), baseVis + visionBonus));
                }
            }

            updateFog(w, h, &mut self.players[pi].fog, &sources, revealAll);
            self.players[pi].fogVersion += 1;
        }
    }

    fn posToTile(&self, pos: Vec2Fx) -> (u32, u32) {
        let clamped = self.map.clampPoint(pos);
        let tx = (clamped.x.floor().to_num::<i32>()).clamp(0, self.map.width.saturating_sub(1) as i32)
            as u32;
        let ty = (clamped.y.floor().to_num::<i32>()).clamp(0, self.map.height.saturating_sub(1) as i32)
            as u32;
        (tx, ty)
    }

    /// Escolhe um local espaçoso para capital (longe das bordas do mapa e cercado de terra passável).
    fn findSpaciousCapitalSite(&self, preferred: (u32, u32)) -> (u32, u32) {
        let margin = 8u32.min(self.map.width / 5).min(self.map.height / 5);
        let maxX = self.map.width.saturating_sub(margin + 3);
        let maxY = self.map.height.saturating_sub(margin + 3);
        let mut best: Option<(i32, u32, u32)> = None;

        for ty in margin..=maxY {
            for tx in margin..=maxX {
                let mut fits = true;
                for dy in 0..3 {
                    for dx in 0..3 {
                        if !self.isTileWalkable(tx + dx, ty + dy) {
                            fits = false;
                            break;
                        }
                    }
                    if !fits {
                        break;
                    }
                }
                if !fits {
                    continue;
                }
                // Conta tiles passáveis num entorno 9x9 e penaliza distância ao quadrante desejado.
                let mut land = 0i32;
                for dy in -3i32..=5 {
                    for dx in -3i32..=5 {
                        let nx = tx as i32 + dx;
                        let ny = ty as i32 + dy;
                        if nx >= 0
                            && ny >= 0
                            && self.isTileWalkable(nx as u32, ny as u32)
                        {
                            land += 1;
                        }
                    }
                }
                let dpx = tx as i32 - preferred.0 as i32;
                let dpy = ty as i32 - preferred.1 as i32;
                let distPenalty = (dpx * dpx + dpy * dpy) / 8;
                let score = land * 4 - distPenalty;
                if best.is_none_or(|(bs, _, _)| score > bs) {
                    best = Some((score, tx, ty));
                }
            }
        }
        best.map(|(_, tx, ty)| (tx, ty))
            .unwrap_or_else(|| self.findBuildSiteNear(preferred, 3))
    }

    /// Tiles de floresta já ocupados por lenhadores do jogador.
    fn claimedForestTiles(&self, owner: PlayerId) -> Vec<(u32, u32)> {
        self.units
            .iter()
            .filter_map(|u| {
                if u.owner != owner {
                    return None;
                }
                if let UnitOrder::Gather { resource: ResourceKind::Wood, tile, .. } = u.order {
                    if self.map.tile(tile.0, tile.1) == Some(TerrainType::Forest) {
                        return Some(tile);
                    }
                }
                None
            })
            .collect()
    }

    /// Escolhe um tile de floresta alcançável perto de `center`, preferindo distância >= 2 de outros lenhadores.
    fn findSpacedForestTile(
        &self,
        center: (u32, u32),
        maxRadius: i32,
        avoid: &[(u32, u32)],
    ) -> Option<(u32, u32)> {
        let mut candidates: Vec<(i32, u32, u32)> = Vec::new();
        for dy in -maxRadius..=maxRadius {
            for dx in -maxRadius..=maxRadius {
                let dSq = dx * dx + dy * dy;
                if dSq > maxRadius * maxRadius {
                    continue;
                }
                let nx = center.0 as i32 + dx;
                let ny = center.1 as i32 + dy;
                if nx < 0 || ny < 0 || nx >= self.map.width as i32 || ny >= self.map.height as i32 {
                    continue;
                }
                let (ux, uy) = (nx as u32, ny as u32);
                if self.map.tile(ux, uy) != Some(TerrainType::Forest) {
                    continue;
                }
                let hasWalkableAdj = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(ax, ay)| {
                    let tx = nx + ax;
                    let ty = ny + ay;
                    tx >= 0 && ty >= 0 && self.isTileWalkable(tx as u32, ty as u32)
                });
                if hasWalkableAdj {
                    candidates.push((dSq, uy, ux));
                }
            }
        }
        if candidates.is_empty() {
            return None;
        }
        candidates.sort_unstable();

        // 1ª preferência: tile de floresta afastado (>= 2 tiles) de qualquer lenhador já alocado.
        if let Some(&(_, uy, ux)) = candidates.iter().find(|&&(_, uy, ux)| {
            avoid.iter().all(|&(ax, ay)| {
                let dx = ux as i32 - ax as i32;
                let dy = uy as i32 - ay as i32;
                dx * dx + dy * dy >= 4
            })
        }) {
            return Some((ux, uy));
        }

        // 2ª preferência: qualquer tile de floresta ainda não usado.
        if let Some(&(_, uy, ux)) =
            candidates.iter().find(|&&(_, uy, ux)| !avoid.contains(&(ux, uy)))
        {
            return Some((ux, uy));
        }

        let (_, uy, ux) = candidates[0];
        Some((ux, uy))
    }

    /// Encontra o tile de floresta mais próximo de `center` que tenha pelo menos um vizinho caminhável.
    #[cfg(test)]
    fn findNearestGatherableForest(&self, center: (u32, u32), maxRadius: i32) -> Option<(u32, u32)> {
        self.findSpacedForestTile(center, maxRadius, &[])
    }

    /// Encontra a construção aliada completa mais próxima de determinado tipo.
    fn findNearestFriendlyBuilding(
        &self,
        owner: PlayerId,
        kind: &str,
        tile: (u32, u32),
        maxRadius: i32,
    ) -> Option<BuildingId> {
        let maxSq = maxRadius * maxRadius;
        let mut best: Option<(i32, BuildingId)> = None;
        for b in &self.buildings {
            if b.owner != owner || b.kind != kind || !b.isComplete() {
                continue;
            }
            let (bx, by) = b.centerTile();
            let dx = bx as i32 - tile.0 as i32;
            let dy = by as i32 - tile.1 as i32;
            let dSq = dx * dx + dy * dy;
            if dSq <= maxSq && best.is_none_or(|(bd, _)| dSq < bd) {
                best = Some((dSq, b.id));
            }
        }
        best.map(|(_, id)| id)
    }

    /// Busca determinística por uma área quadrada `footprint x footprint` passável perto de `center`.
    fn findBuildSiteNear(&self, center: (u32, u32), footprint: u32) -> (u32, u32) {
        let fits = |tx: u32, ty: u32| -> bool {
            if tx + footprint > self.map.width || ty + footprint > self.map.height {
                return false;
            }
            for dy in 0..footprint {
                for dx in 0..footprint {
                    let (ux, uy) = (tx + dx, ty + dy);
                    if !self.isTileWalkable(ux, uy) || self.buildingAtTile(ux, uy).is_some() {
                        return false;
                    }
                }
            }
            true
        };

        let cx = center.0.min(self.map.width.saturating_sub(footprint));
        let cy = center.1.min(self.map.height.saturating_sub(footprint));
        if fits(cx, cy) {
            return (cx, cy);
        }
        let maxR = self.map.width.max(self.map.height) as i32;
        for r in 1..=maxR {
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx.abs().max(dy.abs()) != r {
                        continue;
                    }
                    let nx = cx as i32 + dx;
                    let ny = cy as i32 + dy;
                    if nx < 0 || ny < 0 {
                        continue;
                    }
                    if fits(nx as u32, ny as u32) {
                        return (nx as u32, ny as u32);
                    }
                }
            }
        }
        (cx, cy)
    }
}

/// Ponto de trabalho determinístico DENTRO das fileiras de plantação de uma fazenda 2x2
/// (evitando o canto do celeiro), alternando a cada 3 segundos para o fazendeiro circular no campo.
fn farmWorkSpot(farm: &Building, unitId: UnitId, tick: u64) -> Vec2Fx {
    const SPOTS: [(i32, i32); 4] = [
        (45, 52),
        (95, 148),
        (52, 148),
        (110, 95),
    ];
    let idx = ((tick / 60 + u64::from(unitId.0)) % 4) as usize;
    let (ox, oy) = SPOTS[idx];
    Vec2Fx::new(
        Fx::from_num(farm.tile.0) + fxFromHundredths(ox),
        Fx::from_num(farm.tile.1) + fxFromHundredths(oy),
    )
}

/// Ponto de trabalho DENTRO do pátio comercial de um Mercado 2x2, alternando entre as barracas.
fn marketWorkSpot(market: &Building, unitId: UnitId, tick: u64) -> Vec2Fx {
    const SPOTS: [(i32, i32); 4] = [
        (72, 92),
        (128, 92),
        (100, 128),
        (100, 74),
    ];
    let idx = ((tick / 50 + u64::from(unitId.0) * 2) % 4) as usize;
    let (ox, oy) = SPOTS[idx];
    Vec2Fx::new(
        Fx::from_num(market.tile.0) + fxFromHundredths(ox),
        Fx::from_num(market.tile.1) + fxFromHundredths(oy),
    )
}

/// Hash FNV-1a: fixo entre plataformas e execuções, ao contrário do `DefaultHasher`.
struct Fnv1a(u64);

impl Default for Fnv1a {
    fn default() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
}

impl Hasher for Fnv1a {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runScenario() -> SimWorld {
        let mut world = SimWorld::new(MapGrid::new(64, 64));
        world.step(&[
            Command::SpawnUnit { pos: Vec2Fx::fromInts(2, 2) },
            Command::SpawnUnit { pos: Vec2Fx::fromInts(10, 5) },
        ]);
        world.step(&[Command::MoveUnit { unit: UnitId(0), target: Vec2Fx::fromInts(8, 10) }]);
        for tick in 0..600 {
            let commands = if tick == 30 {
                vec![Command::MoveUnit { unit: UnitId(1), target: Vec2Fx::fromInts(500, -3) }]
            } else {
                Vec::new()
            };
            world.step(&commands);
        }
        world
    }

    #[test]
    fn unitReachesTargetAndStops() {
        let world = runScenario();
        let unit = world.unit(UnitId(0)).unwrap();
        assert_eq!(unit.pos, Vec2Fx::fromInts(8, 10));
        assert_eq!(unit.target, None);
    }

    #[test]
    fn targetsAreClampedToMap() {
        let world = runScenario();
        assert_eq!(world.unit(UnitId(1)).unwrap().pos, Vec2Fx::fromInts(64, 0));
    }

    #[test]
    fn sameCommandsGiveSameChecksum() {
        assert_eq!(runScenario().checksum(), runScenario().checksum());
    }

    #[test]
    fn checksumReflectsState() {
        let mut other = runScenario();
        other.step(&[Command::MoveUnit { unit: UnitId(0), target: Vec2Fx::fromInts(1, 1) }]);
        let mut same = runScenario();
        same.step(&[]);
        assert_ne!(other.checksum(), same.checksum());
    }

    #[test]
    fn unitNeverEntersImpassableTerrain() {
        let map = MapGrid::generate(64, 64, 42);
        let mut world = SimWorld::new(map);
        world.step(&[Command::SpawnUnit { pos: Vec2Fx::fromInts(32, 32) }]);
        assert!(world.map.isPassablePos(world.unit(UnitId(0)).unwrap().pos));

        for ty in (0..64).step_by(7) {
            for tx in (0..64).step_by(7) {
                if !world.map.isPassableTile(tx, ty) {
                    world.step(&[Command::MoveUnit {
                        unit: UnitId(0),
                        target: Vec2Fx::fromInts(tx as i32, ty as i32),
                    }]);
                    for _ in 0..80 {
                        world.step(&[]);
                        let pos = world.unit(UnitId(0)).unwrap().pos;
                        assert!(world.map.isPassablePos(pos));
                    }
                }
            }
        }
    }

    #[test]
    fn spawnUnitSnapsToPassableTile() {
        let map = MapGrid::generate(64, 64, 42);
        let mut blocked = None;
        for ty in 0..64 {
            for tx in 0..64 {
                if !map.isPassableTile(tx, ty) {
                    blocked = Some(Vec2Fx::fromInts(tx as i32, ty as i32));
                    break;
                }
            }
            if blocked.is_some() {
                break;
            }
        }
        let mut world = SimWorld::new(map);
        world.step(&[Command::SpawnUnit { pos: blocked.unwrap() }]);
        assert!(world.map.isPassablePos(world.unit(UnitId(0)).unwrap().pos));
    }

    #[test]
    fn phase3EconomyGathersAndRespectsCommerceCap() {
        let data = GameData::loadDefault();
        let mut world = SimWorld::newMatch(MapGrid::new(48, 48), data, "egito", "roma");
        let startFood = world.player(PlayerId(0)).unwrap().resources.food;

        // Coloca muitos cidadãos coletando na fazenda para testar o teto de comércio.
        let farm = world
            .buildings
            .iter()
            .find(|b| b.owner == PlayerId(0) && b.kind == "farm")
            .unwrap()
            .clone();
        for _ in 0..8 {
            let uid = world.spawnUnitInternal(PlayerId(0), "citizen", farm.centerPos());
            world.step(&[Command::GatherAt {
                owner: PlayerId(0),
                units: vec![uid],
                tile: farm.centerTile(),
            }]);
        }
        for _ in 0..120 {
            world.step(&[]);
        }
        let p0 = world.player(PlayerId(0)).unwrap();
        assert!(p0.resources.food > startFood, "fazenda deve gerar comida");
        assert!(p0.gatherRates.food <= p0.commerceCap, "taxa deve respeitar limite de comércio");
    }

    #[test]
    fn phase4TerritoryBordersGrowAndApplyAttrition() {
        let data = GameData::loadDefault();
        let mut world = SimWorld::newMatch(MapGrid::new(48, 48), data, "roma", "grecia");
        // Remove tropas inimigas para isolar o teste de fronteira e atrito puro.
        world.units.retain(|u| u.owner == PlayerId(0));

        let enemyCity = world.cities.iter().find(|c| c.owner == PlayerId(1)).unwrap().clone();
        // Território ao redor da cidade inimiga pertence ao jogador 1.
        assert_eq!(
            world.territory.ownerAtTile(enemyCity.centerTile.0, enemyCity.centerTile.1),
            Some(PlayerId(1))
        );

        // Fronteira cresce ao pesquisar tecnologia Cívica (civ_1: +2 raio).
        let ownedBefore = world
            .territory
            .owners()
            .iter()
            .filter(|&&o| o == Some(PlayerId(0)))
            .count();
        world.step(&[Command::ResearchTech { owner: PlayerId(0), techId: "civ_1".into() }]);
        let ownedAfter = world
            .territory
            .owners()
            .iter()
            .filter(|&&o| o == Some(PlayerId(0)))
            .count();
        assert!(ownedAfter > ownedBefore, "fronteira deve crescer com pesquisa civica");

        // Unidade militar do jogador 0 na borda do solo inimigo (fora do alcance da torre) sofre atrito.
        let invPos = Vec2Fx::tileCenter(enemyCity.centerTile.0 - 9, enemyCity.centerTile.1);
        let invader = world.spawnUnitInternal(PlayerId(0), "infantry", invPos);
        let startHp = world.unit(invader).unwrap().hp;
        for _ in 0..85 {
            world.step(&[]);
        }
        let u = world.unit(invader).unwrap();
        assert!(u.inEnemyTerritory);
        assert!(u.hp < startHp, "unidade em território inimigo deve perder HP por atrito");
    }

    #[test]
    fn phase5CombatAndCityCaptureWork() {
        let data = GameData::loadDefault();
        let mut world = SimWorld::newMatch(MapGrid::new(48, 48), data, "roma", "persia");
        // Remove defensores do jogador 1 e reduz HP da cidade inimiga para testar captura.
        world.units.retain(|u| u.owner == PlayerId(0));
        let enemyCity = world.cities.iter().find(|c| c.owner == PlayerId(1)).unwrap().clone();
        let bIdx = world.buildings.iter().position(|b| b.id == enemyCity.buildingId).unwrap();
        world.buildings[bIdx].hp = 15;
        let cPos = world.buildings[bIdx].centerPos();

        // Spawna unidade de cerco e infantaria ao lado da cidade inimiga.
        let siege = world.spawnUnitInternal(
            PlayerId(0),
            "siege",
            Vec2Fx::new(cPos.x - Fx::from_num(3), cPos.y),
        );
        let inf = world.spawnUnitInternal(
            PlayerId(0),
            "infantry",
            Vec2Fx::new(cPos.x - Fx::from_num(2), cPos.y),
        );
        world.step(&[Command::AttackBuilding {
            owner: PlayerId(0),
            units: vec![siege, inf],
            target: enemyCity.buildingId,
        }]);

        for _ in 0..120 {
            world.step(&[]);
        }
        let captured = world.city(enemyCity.id).unwrap();
        assert_eq!(captured.owner, PlayerId(0), "cidade deve ser capturada ao chegar em 0 HP");
        assert_eq!(world.winner, Some(PlayerId(0)));
    }

    #[test]
    fn phase6And7TechAgeAndWondersUpgradeEmpire() {
        let data = GameData::loadDefault();
        let mut world = SimWorld::newMatch(MapGrid::new(48, 48), data, "grecia", "china");
        // Recursos amplos para testar pesquisa, avanço de era e maravilha.
        world.playerMut(PlayerId(0)).unwrap().resources = ResourceBundle {
            food: 2000,
            wood: 2000,
            metal: 2000,
            wealth: 2000,
            knowledge: 2000,
        };

        // Pesquisa 2 tecnologias -> habilita Era Clássica.
        world.step(&[
            Command::ResearchTech { owner: PlayerId(0), techId: "mil_1".into() },
            Command::ResearchTech { owner: PlayerId(0), techId: "civ_1".into() },
        ]);
        assert!(world.canAdvanceAge(PlayerId(0)));
        let infBefore = world
            .units
            .iter()
            .find(|u| u.owner == PlayerId(0) && u.class == UnitClass::Infantry)
            .unwrap()
            .maxHp;

        world.step(&[Command::AdvanceAge { owner: PlayerId(0) }]);
        assert_eq!(world.player(PlayerId(0)).unwrap().age, 1);

        let infAfter = world
            .units
            .iter()
            .find(|u| u.owner == PlayerId(0) && u.class == UnitClass::Infantry)
            .unwrap();
        assert_eq!(infAfter.eraTier, 1);
        assert!(infAfter.maxHp > infBefore, "avanço de era deve melhorar unidades existentes");

        // Constrói uma Maravilha (Piramides) e verifica conclusão + bônus.
        let city0 = world.cities.iter().find(|c| c.owner == PlayerId(0)).unwrap().centerTile;
        let wonderSite = world.findBuildSiteNear((city0.0 + 4, city0.1 + 4), 3);
        let cit = world
            .units
            .iter()
            .find(|u| u.owner == PlayerId(0) && u.class == UnitClass::Citizen)
            .unwrap()
            .id;
        world.step(&[Command::ConstructBuilding {
            owner: PlayerId(0),
            builders: vec![cit],
            kind: "wonder".into(),
            tile: wonderSite,
            wonderId: Some("pyramids".into()),
        }]);
        assert!(world.isWonderClaimed("pyramids"));

        for _ in 0..400 {
            world.step(&[]);
        }
        assert!(
            world
                .player(PlayerId(0))
                .unwrap()
                .wondersBuilt
                .contains(&"pyramids".to_string())
        );
    }

    #[test]
    fn farmersWorkInsideFarmAndLumberjacksChopForest() {
        let data = GameData::loadDefault();
        let map = MapGrid::generate(64, 64, 42);
        let mut world = SimWorld::newMatch(map, data, "roma", "grecia");

        // 1. Fazendeiro entra DENTRO do footprint da fazenda e anima Farming.
        let farm = world
            .buildings
            .iter()
            .find(|b| b.owner == PlayerId(0) && b.kind == "farm")
            .unwrap()
            .clone();
        let farmerId = world
            .units
            .iter()
            .find(|u| u.owner == PlayerId(0) && matches!(u.order, UnitOrder::Gather { resource: ResourceKind::Food, .. }))
            .unwrap()
            .id;

        let mut sawFarmingInside = false;
        for _ in 0..160 {
            world.step(&[]);
            let u = world.unit(farmerId).unwrap();
            let (tx, ty) = world.posToTile(u.pos);
            if farm.occupiesTile(tx, ty) && u.workAnim == WorkAnim::Farming {
                sawFarmingInside = true;
                break;
            }
        }
        assert!(sawFarmingInside, "fazendeiro deve trabalhar dentro da fazenda com animação");

        // 2. Madeireira próxima a uma floresta envia o lenhador para cortar na floresta e voltar.
        let p0City = world.cities.iter().find(|c| c.owner == PlayerId(0)).unwrap().centerTile;
        let forestTile = world.findNearestGatherableForest(p0City, 20).expect("mapa tem floresta");
        let campSite = world.findBuildSiteNear((forestTile.0.saturating_sub(2), forestTile.1.saturating_sub(2)), 2);
        let campId = world.spawnCompletedBuilding(PlayerId(0), "lumber_camp", campSite, None).unwrap();
        let jackStart = snapWalkable(
            &world.map,
            &world.blockedTiles,
            Vec2Fx::tileCenter(campSite.0, campSite.1),
        );
        let jackId = world.spawnUnitInternal(PlayerId(0), "citizen", jackStart);
        world.step(&[Command::GatherAt {
            owner: PlayerId(0),
            units: vec![jackId],
            tile: campSite,
        }]);

        let mut sawChoppingAtForest = false;
        let mut sawReturningToCamp = false;
        for _ in 0..260 {
            world.step(&[]);
            let u = world.unit(jackId).unwrap();
            if let UnitOrder::Gather { tile, building, .. } = u.order {
                assert_eq!(building, Some(campId));
                if u.workAnim == WorkAnim::Chopping
                    && world.map.tile(tile.0, tile.1) == Some(TerrainType::Forest)
                {
                    sawChoppingAtForest = true;
                }
            }
            if u.returningToDropoff {
                sawReturningToCamp = true;
            }
        }
        assert!(sawChoppingAtForest, "lenhador da madeireira deve cortar na floresta");
        assert!(sawReturningToCamp, "lenhador deve carregar toras de volta para a madeireira");
    }

    #[test]
    fn phase8AiExpandsEconomicallyTrainsAndAttacks() {
        let data = GameData::loadDefault();
        let map = MapGrid::generate(64, 64, 42);
        let mut world = SimWorld::newMatchWithAi(map, data, "roma", "grecia", AiDifficulty::Hard);
        let startBuildings = world.buildings.iter().filter(|b| b.owner == PlayerId(1)).count();
        let startUnits = world.units.iter().filter(|u| u.owner == PlayerId(1)).count();

        // Simula 90 segundos (1800 ticks) de IA ativa.
        let mut sawAttackOrder = false;
        for _ in 0..1800 {
            world.step(&[]);
            if world.units.iter().any(|u| {
                u.owner == PlayerId(1)
                    && matches!(u.order, UnitOrder::AttackUnit(_) | UnitOrder::AttackBuilding(_))
            }) {
                sawAttackOrder = true;
            }
        }
        let endBuildings = world.buildings.iter().filter(|b| b.owner == PlayerId(1)).count();
        let endUnits = world.units.iter().filter(|u| u.owner == PlayerId(1)).count();
        let p1 = world.player(PlayerId(1)).unwrap();

        assert!(endBuildings > startBuildings, "IA deve construir novas estruturas");
        assert!(endUnits > startUnits || sawAttackOrder, "IA deve treinar tropas e atacar");
        assert!(!p1.researchedTechs.is_empty(), "IA deve pesquisar tecnologias");
        assert!(sawAttackOrder, "IA deve lançar ondas de ataque contra o jogador");
    }

    #[test]
    fn phase9SaveAndLoadPreservesExactDeterminism() {
        let data = GameData::loadDefault();
        let map = MapGrid::generate(48, 48, 42);
        let mut original =
            SimWorld::newMatchWithAi(map, data.clone(), "egito", "persia", AiDifficulty::Normal);
        for _ in 0..240 {
            original.step(&[]);
        }

        let ronStr = crate::save::serializeWorld(&original).expect("serialização deve funcionar");
        let mut loaded =
            crate::save::deserializeWorld(&ronStr, data).expect("desserialização deve funcionar");
        assert_eq!(original.checksum(), loaded.checksum());

        // Avança ambos por mais 150 ticks: devem continuar bit-a-bit idênticos!
        for _ in 0..150 {
            original.step(&[]);
            loaded.step(&[]);
        }
        assert_eq!(original.checksum(), loaded.checksum());
    }

    #[test]
    fn workerLimitsCityCapsMarketAnimationAndRepairWork() {
        let data = GameData::loadDefault();
        let map = MapGrid::generate(64, 64, 42);
        let mut world = SimWorld::newMatch(map, data, "roma", "grecia");
        world.playerMut(PlayerId(0)).unwrap().resources = ResourceBundle {
            food: 2000,
            wood: 2000,
            metal: 2000,
            wealth: 2000,
            knowledge: 2000,
        };

        // 1. Fazenda aceita no máximo 1 trabalhador.
        let farm = world
            .buildings
            .iter()
            .find(|b| b.owner == PlayerId(0) && b.kind == "farm")
            .unwrap()
            .clone();
        assert_eq!(farm.workerSlots, 1);
        let extraCit = world.spawnUnitInternal(PlayerId(0), "citizen", farm.centerPos());
        world.step(&[Command::GatherAt {
            owner: PlayerId(0),
            units: vec![extraCit],
            tile: farm.centerTile(),
        }]);
        assert_eq!(world.buildingAssignedWorkers(farm.id), 1, "fazenda deve limitar a 1 trabalhador");

        // 2. Limite de 1 Mercado por cidade e funcionário trabalha DENTRO do mercado com WorkAnim::Market.
        let city0 = world.cities.iter().find(|c| c.owner == PlayerId(0)).unwrap().centerTile;
        let marketSite = world.findBuildSiteNear((city0.0 + 3, city0.1), 2);
        let marketId = world.spawnCompletedBuilding(PlayerId(0), "market", marketSite, None).unwrap();
        assert_eq!(world.building(marketId).unwrap().workerSlots, 2);
        // Com 1 cidade, não pode construir um segundo Mercado (limite 1 por cidade).
        let marketSite2 = world.findBuildSiteNear((city0.0.saturating_sub(4), city0.1), 2);
        assert!(!world.canPlaceBuilding(PlayerId(0), "market", marketSite2, None));

        world.step(&[Command::GatherAt {
            owner: PlayerId(0),
            units: vec![extraCit],
            tile: marketSite,
        }]);
        let mut sawMarketInside = false;
        for _ in 0..140 {
            world.step(&[]);
            let u = world.unit(extraCit).unwrap();
            let (tx, ty) = world.posToTile(u.pos);
            if world.building(marketId).unwrap().occupiesTile(tx, ty)
                && u.workAnim == WorkAnim::Market
            {
                sawMarketInside = true;
                break;
            }
        }
        assert!(sawMarketInside, "funcionario do mercado deve trabalhar dentro do mercado com animacao");

        // 3. Reforma/reparo restaura HP de prédio danificado.
        let barracksId = world
            .buildings
            .iter()
            .find(|b| b.owner == PlayerId(0) && b.kind == "barracks")
            .unwrap()
            .id;
        let bIdx = world.buildings.iter().position(|b| b.id == barracksId).unwrap();
        world.buildings[bIdx].hp = 100;
        world.step(&[Command::RepairBuilding {
            owner: PlayerId(0),
            builders: vec![extraCit],
            target: barracksId,
        }]);
        for _ in 0..160 {
            world.step(&[]);
        }
        assert!(world.building(barracksId).unwrap().hp > 100, "reforma deve restaurar HP da construcao");
    }
}
