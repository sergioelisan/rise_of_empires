//! Núcleo determinístico da simulação.
//!
//! Regras desta crate (necessárias para replays e multiplayer lockstep):
//! - nada de `f32`/`f64` no estado ou na lógica: só ponto fixo (`Fx`);
//! - nada de relógio, aleatoriedade externa ou ordem de iteração de `HashMap`;
//! - o estado só muda em `SimWorld::step`, a partir de `Command`s.

pub mod ai;
pub mod combat;
pub mod command;
pub mod data;
pub mod map;
pub mod math;
pub mod pathfinding;
pub mod save;
pub mod territory;
pub mod world;

pub use ai::AiDifficulty;
pub use combat::CombatEvent;
pub use command::{BuildingId, CityId, Command, PlayerId, UnitId};
pub use data::{
    AgeDef, BuildingDef, FormationKind, GameData, NationDef, ResourceBundle, ResourceKind,
    TechBranch, TechDef, TechEffect, UnitClass, UnitDef, WonderDef,
};
pub use map::{MapGrid, TerrainType};
pub use math::{Fx, Vec2Fx, fxFromHundredths};
pub use save::{deserializeWorld, loadWorldFromFile, saveWorldToFile, serializeWorld};
pub use territory::{City, FogState, TerritoryMap};
pub use world::{Building, PlayerState, SimWorld, TrainOrder, Unit, UnitOrder, WorkAnim};

/// Passos de simulação por segundo.
pub const TICKS_PER_SECOND: u32 = 20;
