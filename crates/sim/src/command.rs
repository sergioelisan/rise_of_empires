use serde::{Deserialize, Serialize};

use crate::data::FormationKind;
use crate::math::Vec2Fx;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PlayerId(pub u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct UnitId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BuildingId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CityId(pub u32);

/// Única forma de alterar a simulação de fora. No multiplayer, é isto que trafega na rede.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Comando simples herdado da fase 0 (spawna unidade genérica do jogador 0).
    SpawnUnit { pos: Vec2Fx },
    /// Spawna uma unidade tipada de qualquer jogador.
    SpawnTypedUnit { owner: PlayerId, kind: String, pos: Vec2Fx },
    /// Movimento simples de uma unidade (compatível com testes da fase 0/1).
    MoveUnit { unit: UnitId, target: Vec2Fx },
    /// Movimento em grupo com formação e pathfinding por flow field.
    MoveUnits {
        owner: PlayerId,
        units: Vec<UnitId>,
        target: Vec2Fx,
        formation: FormationKind,
    },
    /// Ordena ataque a uma unidade inimiga.
    AttackUnit { owner: PlayerId, units: Vec<UnitId>, target: UnitId },
    /// Ordena ataque/cerco a uma construção ou cidade inimiga.
    AttackBuilding { owner: PlayerId, units: Vec<UnitId>, target: BuildingId },
    /// Envia cidadãos para coletar recurso num tile ou construção de coleta.
    GatherAt { owner: PlayerId, units: Vec<UnitId>, tile: (u32, u32) },
    /// Inicia a construção de um edifício ou maravilha e envia cidadãos para construir.
    ConstructBuilding {
        owner: PlayerId,
        builders: Vec<UnitId>,
        kind: String,
        tile: (u32, u32),
        wonderId: Option<String>,
    },
    /// Envia cidadãos para reformar/reparar uma construção aliada danificada.
    RepairBuilding {
        owner: PlayerId,
        builders: Vec<UnitId>,
        target: BuildingId,
    },
    /// Adiciona uma unidade à fila de treino de uma construção.
    TrainUnit { owner: PlayerId, building: BuildingId, unitKind: String },
    /// Pesquisa uma tecnologia da árvore.
    ResearchTech { owner: PlayerId, techId: String },
    /// Avança o império para a próxima era histórica.
    AdvanceAge { owner: PlayerId },
    /// Altera a formação de um grupo de unidades.
    SetFormation { owner: PlayerId, units: Vec<UnitId>, formation: FormationKind },
}
