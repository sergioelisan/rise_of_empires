use serde::{Deserialize, Serialize};

use crate::command::PlayerId;
use crate::data::UnitClass;
use crate::math::Vec2Fx;

/// Evento visual determinístico emitido quando uma unidade ou torre dispara/golpeia.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombatEvent {
    pub tick: u64,
    pub owner: PlayerId,
    pub from: Vec2Fx,
    pub to: Vec2Fx,
    pub isRanged: bool,
    pub isSiege: bool,
}

/// Matriz de counters clássica de RTS em porcentagem (100 = neutro).
/// - Infantaria vence Cavalaria (160%), sofre contra Arqueiros (80%)
/// - Cavalaria vence Arqueiros (160%) e Cerco (175%), sofre contra Infantaria (70%)
/// - Arqueiros vencem Infantaria (150%), sofrem contra Cavalaria (75%)
/// - Cerco é especialista contra construções (250%), vulnerável em combate direto
pub fn counterMultiplierPct(attacker: UnitClass, defender: UnitClass) -> i32 {
    use UnitClass::*;
    match (attacker, defender) {
        (Infantry, Cavalry) => 160,
        (Infantry, Archer) => 80,
        (Cavalry, Archer) => 160,
        (Cavalry, Siege) => 175,
        (Cavalry, Infantry) => 70,
        (Archer, Infantry) => 150,
        (Archer, Cavalry) => 75,
        (Archer, Air) => 135,
        (Siege, Cavalry) => 65,
        (Naval, Infantry) | (Naval, Archer) => 125,
        (Air, Siege) | (Air, Naval) => 145,
        (Citizen, _) => 50,
        _ => 100,
    }
}

/// Multiplicador de dano contra construções e cidades.
pub fn buildingCounterMultiplierPct(attacker: UnitClass) -> i32 {
    match attacker {
        UnitClass::Siege => 250,
        UnitClass::Naval => 165,
        UnitClass::Air => 150,
        UnitClass::Citizen => 45,
        _ => 100,
    }
}

/// Calcula dano efetivo em inteiros (sempre ao menos 1 ponto de dano).
pub fn computeDamage(rawAttack: i32, multiplierPct: i32, targetArmor: i32) -> i32 {
    let scaled = (rawAttack * multiplierPct) / 100;
    (scaled - targetArmor).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counterTriangleWorks() {
        assert!(counterMultiplierPct(UnitClass::Infantry, UnitClass::Cavalry) > 100);
        assert!(counterMultiplierPct(UnitClass::Cavalry, UnitClass::Archer) > 100);
        assert!(counterMultiplierPct(UnitClass::Archer, UnitClass::Infantry) > 100);
        assert!(buildingCounterMultiplierPct(UnitClass::Siege) >= 200);
    }

    #[test]
    fn damageIsAlwaysAtLeastOne() {
        assert_eq!(computeDamage(5, 50, 99), 1);
        assert_eq!(computeDamage(20, 150, 5), 25);
    }
}
