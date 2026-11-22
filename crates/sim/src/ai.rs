use serde::{Deserialize, Serialize};

use crate::command::{Command, PlayerId, UnitId};
use crate::data::{FormationKind, ResourceKind, UnitClass};
use crate::map::TerrainType;
use crate::math::{Fx, Vec2Fx};
use crate::world::{SimWorld, UnitOrder};

/// Nível de dificuldade da IA determinística (gera exclusivamente `Command`s).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AiDifficulty {
    Passive,
    Easy,
    #[default]
    Normal,
    Hard,
}

impl AiDifficulty {
    pub fn label(self) -> &'static str {
        match self {
            Self::Passive => "Passiva",
            Self::Easy => "Facil",
            Self::Normal => "Normal",
            Self::Hard => "Dificil",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Passive => Self::Easy,
            Self::Easy => Self::Normal,
            Self::Normal => Self::Hard,
            Self::Hard => Self::Passive,
        }
    }

    fn decisionInterval(self) -> u64 {
        match self {
            Self::Passive => u64::MAX,
            Self::Easy => 60,
            Self::Normal => 30,
            Self::Hard => 20,
        }
    }

    fn targetCitizens(self) -> usize {
        match self {
            Self::Passive => 0,
            Self::Easy => 5,
            Self::Normal => 8,
            Self::Hard => 11,
        }
    }

    fn attackWaveSize(self) -> usize {
        match self {
            Self::Passive => usize::MAX,
            Self::Easy => 4,
            Self::Normal => 5,
            Self::Hard => 6,
        }
    }

    fn maxCities(self) -> usize {
        match self {
            Self::Passive => 1,
            Self::Easy => 2,
            Self::Normal => 2,
            Self::Hard => 3,
        }
    }
}

/// Avalia o estado da simulação de forma 100% determinística e retorna os comandos da IA para o tick.
pub fn decideAiCommands(world: &SimWorld, pid: PlayerId, diff: AiDifficulty) -> Vec<Command> {
    if diff == AiDifficulty::Passive || world.winner.is_some() {
        return Vec::new();
    }
    let interval = diff.decisionInterval();
    if world.tick == 0 || (world.tick + u64::from(pid.0) * 7) % interval != 0 {
        return Vec::new();
    }

    let Some(player) = world.player(pid) else { return Vec::new() };
    let myCities: Vec<_> = world.cities.iter().filter(|c| c.owner == pid).cloned().collect();
    if myCities.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut spent = player.resources;

    // 1. Atribuir cidadãos ociosos a obras inacabadas ou coleta equilibrada.
    let idleCitizens: Vec<UnitId> = world
        .units
        .iter()
        .filter(|u| u.owner == pid && u.class == UnitClass::Citizen && u.order == UnitOrder::Idle)
        .map(|u| u.id)
        .collect();

    if !idleCitizens.is_empty() {
        if let Some(unfinished) = world.buildings.iter().find(|b| b.owner == pid && !b.isComplete()) {
            out.push(Command::GatherAt {
                owner: pid,
                units: idleCitizens,
                tile: unfinished.centerTile(),
            });
        } else {
            for uid in idleCitizens {
                if let Some(tile) = pickBestGatherTile(world, pid, myCities[0].centerTile) {
                    out.push(Command::GatherAt { owner: pid, units: vec![uid], tile });
                }
            }
        }
    }

    let citizenCount = world
        .units
        .iter()
        .filter(|u| u.owner == pid && u.class == UnitClass::Citizen)
        .count();
    let militaryCount = world
        .units
        .iter()
        .filter(|u| u.owner == pid && u.class != UnitClass::Citizen)
        .count();
    let maxArmy = diff.attackWaveSize() + 5;

    // 2. Pesquisas e avanço de Era (mantém equilíbrio entre economia, pesquisa e exército).
    if world.canAdvanceAge(pid) {
        if let Some(cost) = world.effectiveAgeCost(pid) {
            if spent.canAfford(&cost) {
                spent.subtract(&cost);
                out.push(Command::AdvanceAge { owner: pid });
            }
        }
    } else if player.researchedTechs.is_empty() || militaryCount >= diff.attackWaveSize() {
        const TECH_PRIORITY: [&str; 12] = [
            "com_1", "civ_1", "mil_1", "sci_1", "com_2", "mil_2", "civ_2", "sci_2", "mil_3",
            "com_3", "civ_3", "sci_3",
        ];
        for techId in TECH_PRIORITY {
            if world.canResearchTech(pid, techId) {
                if let Some(cost) = world.effectiveTechCost(pid, techId) {
                    if spent.canAfford(&cost) {
                        spent.subtract(&cost);
                        out.push(Command::ResearchTech { owner: pid, techId: techId.into() });
                        break;
                    }
                }
            }
        }
    }

    // 3. Treino prioritário de cidadãos iniciais e formação da primeira onda militar.
    let queuedCitizens: usize = world
        .buildings
        .iter()
        .filter(|b| b.owner == pid && b.kind == "city_center")
        .map(|b| b.trainQueue.len())
        .sum();

    if citizenCount + queuedCitizens < diff.targetCitizens()
        && (citizenCount < 3 || militaryCount >= diff.attackWaveSize())
    {
        if let Some(cc) = world.buildings.iter().find(|b| {
            b.owner == pid
                && b.kind == "city_center"
                && b.isComplete()
                && b.hp > 0
                && b.trainQueue.len() < 2
        }) {
            if let Some(def) = world.data.unit("citizen") {
                if spent.canAfford(&def.cost) {
                    spent.subtract(&def.cost);
                    out.push(Command::TrainUnit {
                        owner: pid,
                        building: cc.id,
                        unitKind: "citizen".into(),
                    });
                }
            }
        }
    }

    // Garante 2 Fazendas cedo (já que cada fazenda aceita 1 trabalhador) antes de gastar toda a madeira no quartel.
    let needEarlyFarm = world
        .buildings
        .iter()
        .filter(|b| b.owner == pid && b.kind == "farm")
        .count()
        < 2;
    if needEarlyFarm && !world.buildings.iter().any(|b| b.owner == pid && !b.isComplete()) {
        if let Some(builderId) = pickAiBuilder(world, pid) {
            if let Some(cost) = world.effectiveBuildingCost(pid, "farm", None) {
                if spent.canAfford(&cost) {
                    if let Some(site) =
                        findAiPlacementTile(world, pid, "farm", None, myCities[0].centerTile)
                    {
                        spent.subtract(&cost);
                        out.push(Command::ConstructBuilding {
                            owner: pid,
                            builders: vec![builderId],
                            kind: "farm".into(),
                            tile: site,
                            wonderId: None,
                        });
                    }
                }
            }
        }
    }

    if militaryCount < maxArmy {
        let infCount = countClass(world, pid, UnitClass::Infantry);
        let archCount = countClass(world, pid, UnitClass::Archer);
        let cavCount = countClass(world, pid, UnitClass::Cavalry);
        let siegeCount = countClass(world, pid, UnitClass::Siege);
        let uniqueId = player.nation(&world.data).map(|n| n.uniqueUnitId.as_str());

        for b in world
            .buildings
            .iter()
            .filter(|b| b.owner == pid && b.isComplete() && b.hp > 0 && b.trainQueue.len() < 2)
        {
            let trainable = world.data.trainableUnitsFor(&b.kind, &player.nationId, player.age);
            if trainable.is_empty() || b.kind == "city_center" {
                continue;
            }
            let chosen = match b.kind.as_str() {
                "barracks" => {
                    if let Some(uDef) = uniqueId
                        .and_then(|uid| trainable.iter().find(|d| d.id == uid).copied())
                        .filter(|_| (infCount + archCount) % 2 == 0)
                    {
                        Some(uDef)
                    } else if infCount <= archCount {
                        trainable.iter().find(|d| d.id == "infantry").copied()
                    } else {
                        trainable.iter().find(|d| d.id == "archer").copied()
                    }
                }
                "stable" => uniqueId
                    .and_then(|uid| trainable.iter().find(|d| d.id == uid).copied())
                    .or_else(|| trainable.iter().find(|d| d.id == "cavalry").copied())
                    .filter(|_| cavCount <= infCount + 1),
                "siege_workshop" => trainable
                    .iter()
                    .find(|d| d.id == "siege")
                    .copied()
                    .filter(|_| siegeCount < 2),
                "dock" => trainable
                    .iter()
                    .find(|d| d.id == "warship")
                    .or_else(|| trainable.iter().find(|d| d.id == "galley"))
                    .copied()
                    .filter(|_| countClass(world, pid, UnitClass::Naval) < 3),
                "airfield" => trainable
                    .iter()
                    .find(|d| d.id == "airship")
                    .copied()
                    .filter(|_| countClass(world, pid, UnitClass::Air) < 2),
                _ => None,
            };

            if let Some(uDef) = chosen {
                if spent.canAfford(&uDef.cost) {
                    spent.subtract(&uDef.cost);
                    out.push(Command::TrainUnit {
                        owner: pid,
                        building: b.id,
                        unitKind: uDef.id.clone(),
                    });
                }
            }
        }
    }

    // 4. Construções econômicas, militares, expansão de cidades e maravilhas.
    let hasUnfinished = world.buildings.iter().any(|b| b.owner == pid && !b.isComplete());
    let anyBuilder = pickAiBuilder(world, pid);

    if !hasUnfinished {
        if let Some(builderId) = anyBuilder {
            let countKind = |k: &str| {
                world.buildings.iter().filter(|b| b.owner == pid && b.kind == k).count()
            };
            let capitalTile = myCities[0].centerTile;

            let desiredBuild: Option<(&str, Option<&str>, (u32, u32))> = if countKind("lumber_camp") == 0 {
                let nearForest = findEdgeForestNear(world, capitalTile, 16).unwrap_or(capitalTile);
                Some(("lumber_camp", None, nearForest))
            } else if countKind("farm") < myCities.len() + 1 {
                Some(("farm", None, capitalTile))
            } else if countKind("mine") == 0 {
                Some(("mine", None, capitalTile))
            } else if countKind("market") == 0 {
                Some(("market", None, capitalTile))
            } else if countKind("temple") == 0 {
                Some(("temple", None, capitalTile))
            } else if player.age >= 1 && countKind("library") == 0 {
                Some(("library", None, capitalTile))
            } else if countKind("barracks") == 0 {
                Some(("barracks", None, capitalTile))
            } else if countKind("stable") == 0 {
                Some(("stable", None, capitalTile))
            } else if player.age >= 1 && countKind("siege_workshop") == 0 {
                Some(("siege_workshop", None, capitalTile))
            } else if player.age >= 2 && countKind("airfield") == 0 {
                Some(("airfield", None, capitalTile))
            } else if myCities.len() < diff.maxCities() && citizenCount >= 5 {
                let mapMid = (world.map.width / 2, world.map.height / 2);
                let expandAnchor = (
                    (capitalTile.0 + mapMid.0) / 2,
                    (capitalTile.1 + mapMid.1) / 2,
                );
                Some(("city_center", None, expandAnchor))
            } else if matches!(diff, AiDifficulty::Normal | AiDifficulty::Hard)
                && player.wondersBuilt.is_empty()
            {
                world
                    .data
                    .wonders
                    .iter()
                    .find(|w| w.minAge <= player.age && !world.isWonderClaimed(&w.id))
                    .map(|w| ("wonder", Some(w.id.as_str()), capitalTile))
            } else {
                None
            };

            if let Some((kind, wonderId, anchor)) = desiredBuild {
                if let Some(cost) = world.effectiveBuildingCost(pid, kind, wonderId) {
                    if spent.canAfford(&cost) {
                        if let Some(site) = findAiPlacementTile(world, pid, kind, wonderId, anchor) {
                            spent.subtract(&cost);
                            out.push(Command::ConstructBuilding {
                                owner: pid,
                                builders: vec![builderId],
                                kind: kind.into(),
                                tile: site,
                                wonderId: wonderId.map(str::to_string),
                            });
                        }
                    }
                }
            }
        }
    }

    // 5. Defesa ativa: se houver inimigo dentro do território da IA, envia tropas ociosas para defender.
    let intruder = world.units.iter().find(|u| {
        u.owner != pid && world.territory.ownerAtPos(&world.map, u.pos) == Some(pid)
    });
    let idleMilitary: Vec<UnitId> = world
        .units
        .iter()
        .filter(|u| u.owner == pid && u.class != UnitClass::Citizen && u.order == UnitOrder::Idle)
        .map(|u| u.id)
        .collect();

    if let Some(enemy) = intruder {
        if !idleMilitary.is_empty() {
            out.push(Command::AttackUnit {
                owner: pid,
                units: idleMilitary,
                target: enemy.id,
            });
            return out;
        }
    }

    // 6. Onda de ataque coordenada quando acumula exército suficiente.
    if idleMilitary.len() >= diff.attackWaveSize() {
        let capPos = Vec2Fx::tileCenter(myCities[0].centerTile.0, myCities[0].centerTile.1);
        // Alvo prioritário: cidade inimiga mais próxima.
        let enemyCity = world
            .cities
            .iter()
            .filter(|c| c.owner != pid)
            .min_by_key(|c| {
                capPos.distanceSquared(Vec2Fx::tileCenter(c.centerTile.0, c.centerTile.1))
            });

        if let Some(targetCity) = enemyCity {
            let targetBuilding = world.building(targetCity.buildingId);
            if targetBuilding.is_some_and(|b| b.hp > 0) {
                out.push(Command::AttackBuilding {
                    owner: pid,
                    units: idleMilitary,
                    target: targetCity.buildingId,
                });
            } else {
                // Cidade já está com 0 HP: marcha até ela para capturar!
                out.push(Command::MoveUnits {
                    owner: pid,
                    units: idleMilitary,
                    target: Vec2Fx::tileCenter(targetCity.centerTile.0, targetCity.centerTile.1),
                    formation: FormationKind::Wedge,
                });
            }
        } else if let Some(enemyUnit) = world.units.iter().find(|u| u.owner != pid) {
            out.push(Command::AttackUnit {
                owner: pid,
                units: idleMilitary,
                target: enemyUnit.id,
            });
        }
    }

    out
}

fn pickAiBuilder(world: &SimWorld, pid: PlayerId) -> Option<UnitId> {
    world
        .units
        .iter()
        .find(|u| u.owner == pid && u.class == UnitClass::Citizen && u.order == UnitOrder::Idle)
        .or_else(|| {
            world
                .units
                .iter()
                .rev()
                .find(|u| u.owner == pid && u.class == UnitClass::Citizen)
        })
        .map(|u| u.id)
}

fn countClass(world: &SimWorld, pid: PlayerId, class: UnitClass) -> usize {
    world.units.iter().filter(|u| u.owner == pid && u.class == class).count()
}

fn pickBestGatherTile(world: &SimWorld, pid: PlayerId, capitalTile: (u32, u32)) -> Option<(u32, u32)> {
    let player = world.player(pid)?;
    // Prioriza Comida e Madeira (base da economia e do exército) até atingirem >= 48/min;
    // depois equilibra Metal, Riqueza e Conhecimento.
    let mut order = vec![
        (player.gatherRates.food / 2, ResourceKind::Food),
        (player.gatherRates.wood / 2, ResourceKind::Wood),
        (player.gatherRates.metal + 12, ResourceKind::Metal),
        (player.gatherRates.wealth + 12, ResourceKind::Wealth),
    ];
    if player.age >= 1 {
        order.push((player.gatherRates.knowledge + 8, ResourceKind::Knowledge));
    }
    order.sort_by_key(|&(score, kind)| (score, kind));

    for (_, res) in order {
        let wantedKind = match res {
            ResourceKind::Food => "farm",
            ResourceKind::Wood => "lumber_camp",
            ResourceKind::Metal => "mine",
            ResourceKind::Wealth => "market",
            ResourceKind::Knowledge => "library",
        };
        if let Some(b) = world.buildings.iter().find(|b| {
            b.owner == pid
                && b.kind == wantedKind
                && b.isComplete()
                && world.buildingAssignedWorkers(b.id) < b.workerSlots
        }) {
            return Some(b.centerTile());
        }
        if res == ResourceKind::Wood {
            if let Some(f) = findEdgeForestNear(world, capitalTile, 18) {
                return Some(f);
            }
        }
    }
    None
}

fn findEdgeForestNear(world: &SimWorld, center: (u32, u32), maxRadius: i32) -> Option<(u32, u32)> {
    let mut best: Option<(i32, u32, u32)> = None;
    for dy in -maxRadius..=maxRadius {
        for dx in -maxRadius..=maxRadius {
            let dSq = dx * dx + dy * dy;
            if dSq > maxRadius * maxRadius {
                continue;
            }
            let nx = center.0 as i32 + dx;
            let ny = center.1 as i32 + dy;
            if nx < 0 || ny < 0 || nx >= world.map.width as i32 || ny >= world.map.height as i32 {
                continue;
            }
            let (ux, uy) = (nx as u32, ny as u32);
            if world.map.tile(ux, uy) == Some(TerrainType::Forest) {
                let cand = (dSq, uy, ux);
                if best.is_none_or(|b| cand < b) {
                    best = Some(cand);
                }
            }
        }
    }
    best.map(|(_, uy, ux)| (ux, uy))
}

fn findAiPlacementTile(
    world: &SimWorld,
    pid: PlayerId,
    kind: &str,
    wonderId: Option<&str>,
    anchor: (u32, u32),
) -> Option<(u32, u32)> {
    let maxR: i32 = if kind == "city_center" { 20 } else { 14 };
    for r in 2..=maxR {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs().max(dy.abs()) != r {
                    continue;
                }
                let nx = anchor.0 as i32 + dx;
                let ny = anchor.1 as i32 + dy;
                if nx < 1 || ny < 1 {
                    continue;
                }
                let tile = (nx as u32, ny as u32);
                if world.canPlaceBuilding(pid, kind, tile, wonderId) {
                    // Mantém uma margem de 1 tile entre prédios da IA para não bloquear unidades.
                    let distOk = world.buildings.iter().all(|b| {
                        b.centerPos().distance(Vec2Fx::tileCenter(tile.0, tile.1)) >= Fx::from_num(3)
                    });
                    if distOk || r >= 8 {
                        return Some(tile);
                    }
                }
            }
        }
    }
    None
}
