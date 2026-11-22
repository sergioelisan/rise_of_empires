use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::data::{FormationKind, MovementDomain};
use crate::map::{MapGrid, TerrainType};
use crate::math::{Fx, Vec2Fx};

/// Campo de fluxo (Flow Field) determinístico para um destino `(tx, ty)` e domínio (`Land`/`Naval`/`Air`).
#[derive(Clone, Debug)]
pub struct FlowField {
    pub width: u32,
    pub height: u32,
    pub target: (u32, u32),
    pub domain: MovementDomain,
    distances: Vec<u32>,
    dirs: Vec<(i8, i8)>,
}

const INF: u32 = u32::MAX;
const CARDINAL_COST: u32 = 10;
const DIAGONAL_COST: u32 = 14;

/// 8 direções em ordem determinística (cardeais primeiro, depois diagonais).
const NEIGHBORS: [(i32, i32, u32); 8] = [
    (1, 0, CARDINAL_COST),
    (-1, 0, CARDINAL_COST),
    (0, 1, CARDINAL_COST),
    (0, -1, CARDINAL_COST),
    (1, 1, DIAGONAL_COST),
    (1, -1, DIAGONAL_COST),
    (-1, 1, DIAGONAL_COST),
    (-1, -1, DIAGONAL_COST),
];

pub fn isDomainTilePassable(
    map: &MapGrid,
    blocked: &[bool],
    domain: MovementDomain,
    tx: u32,
    ty: u32,
) -> bool {
    if tx >= map.width || ty >= map.height {
        return false;
    }
    match domain {
        MovementDomain::Land => {
            let idx = (ty * map.width + tx) as usize;
            map.isPassableTile(tx, ty) && !blocked.get(idx).copied().unwrap_or(false)
        }
        MovementDomain::Naval => map.tile(tx, ty) == Some(TerrainType::Water),
        MovementDomain::Air => true,
    }
}

impl FlowField {
    pub fn compute(map: &MapGrid, blocked: &[bool], target: (u32, u32)) -> Self {
        Self::computeForDomain(map, blocked, MovementDomain::Land, target)
    }

    pub fn computeForDomain(
        map: &MapGrid,
        blocked: &[bool],
        domain: MovementDomain,
        target: (u32, u32),
    ) -> Self {
        let w = map.width;
        let h = map.height;
        let total = (w * h) as usize;
        let mut distances = vec![INF; total];
        let mut dirs = vec![(0i8, 0i8); total];

        if w == 0 || h == 0 || target.0 >= w || target.1 >= h {
            return Self { width: w, height: h, target, domain, distances, dirs };
        }

        let isWalkable = |tx: u32, ty: u32| isDomainTilePassable(map, blocked, domain, tx, ty);

        let mut heap: BinaryHeap<Reverse<(u32, u32, u32)>> = BinaryHeap::new();
        let targetIdx = (target.1 * w + target.0) as usize;
        distances[targetIdx] = 0;
        heap.push(Reverse((0, target.1, target.0)));

        while let Some(Reverse((dist, cy, cx))) = heap.pop() {
            let idx = (cy * w + cx) as usize;
            if dist > distances[idx] {
                continue;
            }
            for (dx, dy, stepCost) in NEIGHBORS {
                let nx = cx as i32 + dx;
                let ny = cy as i32 + dy;
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                let (ux, uy) = (nx as u32, ny as u32);
                if !isWalkable(ux, uy) {
                    continue;
                }
                if dx != 0 && dy != 0 {
                    if !isWalkable(ux, cy) || !isWalkable(cx, uy) {
                        continue;
                    }
                }
                let nIdx = (uy * w + ux) as usize;
                let nextDist = dist.saturating_add(stepCost);
                if nextDist < distances[nIdx] {
                    distances[nIdx] = nextDist;
                    heap.push(Reverse((nextDist, uy, ux)));
                }
            }
        }

        for ty in 0..h {
            for tx in 0..w {
                let idx = (ty * w + tx) as usize;
                if distances[idx] == INF || (tx, ty) == target {
                    continue;
                }
                let mut bestDist = distances[idx];
                let mut bestDir = (0i8, 0i8);
                for (dx, dy, _) in NEIGHBORS {
                    let nx = tx as i32 + dx;
                    let ny = ty as i32 + dy;
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    let (ux, uy) = (nx as u32, ny as u32);
                    if dx != 0 && dy != 0 && (!isWalkable(ux, ty) || !isWalkable(tx, uy)) {
                        continue;
                    }
                    let nDist = distances[(uy * w + ux) as usize];
                    if nDist < bestDist {
                        bestDist = nDist;
                        bestDir = (dx as i8, dy as i8);
                    }
                }
                dirs[idx] = bestDir;
            }
        }

        Self { width: w, height: h, target, domain, distances, dirs }
    }

    pub fn directionAt(&self, tx: u32, ty: u32) -> (i8, i8) {
        if tx >= self.width || ty >= self.height {
            return (0, 0);
        }
        self.dirs[(ty * self.width + tx) as usize]
    }

    pub fn distanceAt(&self, tx: u32, ty: u32) -> Option<u32> {
        if tx >= self.width || ty >= self.height {
            return None;
        }
        let d = self.distances[(ty * self.width + tx) as usize];
        if d == INF { None } else { Some(d) }
    }
}

/// Cache determinístico de flow fields, invalidado quando construções mudam o bloqueio.
#[derive(Clone, Debug, Default)]
pub struct FlowFieldCache {
    version: u64,
    fields: Vec<FlowField>,
}

const MAX_CACHED_FIELDS: usize = 32;

impl FlowFieldCache {
    pub fn getOrCompute(
        &mut self,
        version: u64,
        map: &MapGrid,
        blocked: &[bool],
        domain: MovementDomain,
        target: (u32, u32),
    ) -> &FlowField {
        if self.version != version {
            self.fields.clear();
            self.version = version;
        }
        if let Some(pos) = self.fields.iter().position(|f| f.target == target && f.domain == domain)
        {
            return &self.fields[pos];
        }
        if self.fields.len() >= MAX_CACHED_FIELDS {
            self.fields.remove(0);
        }
        self.fields.push(FlowField::computeForDomain(map, blocked, domain, target));
        self.fields.last().unwrap()
    }
}

/// Calcula posições de destino em formação (`Box`, `Line`, `Wedge`) alinhadas à direção da marcha.
pub fn computeFormationTargets(
    map: &MapGrid,
    blocked: &[bool],
    domain: MovementDomain,
    count: usize,
    groupCenter: Vec2Fx,
    target: Vec2Fx,
    formation: FormationKind,
) -> Vec<Vec2Fx> {
    if count == 0 {
        return Vec::new();
    }
    if count == 1 {
        return vec![snapDomainWalkable(map, blocked, domain, target)];
    }

    let forward = {
        let d = (target - groupCenter).normalizeOrZero();
        if d == Vec2Fx::ZERO { Vec2Fx::fromInts(1, 0) } else { d }
    };
    let right = forward.perpendicular();
    let spacing = Fx::from_num(115) / Fx::from_num(100);

    let mut slots = Vec::with_capacity(count);
    match formation {
        FormationKind::Box => {
            let cols = integerSqrtCeil(count) as i32;
            let rows = ((count as i32) + cols - 1) / cols;
            for i in 0..count as i32 {
                let col = i % cols;
                let row = i / cols;
                let sideOff = Fx::from_num(col * 2 - (cols - 1)) / Fx::from_num(2) * spacing;
                let backOff = Fx::from_num(row * 2 - (rows - 1)) / Fx::from_num(2) * spacing;
                let pos = target + right * sideOff - forward * backOff;
                slots.push(snapDomainWalkable(map, blocked, domain, pos));
            }
        }
        FormationKind::Line => {
            let n = count as i32;
            for i in 0..n {
                let sideOff = Fx::from_num(i * 2 - (n - 1)) / Fx::from_num(2) * spacing;
                let pos = target + right * sideOff;
                slots.push(snapDomainWalkable(map, blocked, domain, pos));
            }
        }
        FormationKind::Wedge => {
            for i in 0..count {
                if i == 0 {
                    slots.push(snapDomainWalkable(map, blocked, domain, target));
                } else {
                    let rank = ((i + 1) / 2) as i32;
                    let sign = if i % 2 == 1 { 1 } else { -1 };
                    let sideOff = Fx::from_num(rank * sign) * spacing;
                    let backOff = Fx::from_num(rank) * spacing;
                    let pos = target + right * sideOff - forward * backOff;
                    slots.push(snapDomainWalkable(map, blocked, domain, pos));
                }
            }
        }
    }
    slots
}

pub fn snapWalkable(map: &MapGrid, blocked: &[bool], pos: Vec2Fx) -> Vec2Fx {
    snapDomainWalkable(map, blocked, MovementDomain::Land, pos)
}

pub fn snapDomainWalkable(
    map: &MapGrid,
    blocked: &[bool],
    domain: MovementDomain,
    pos: Vec2Fx,
) -> Vec2Fx {
    let clamped = map.clampPoint(pos);
    if domain == MovementDomain::Air {
        return clamped;
    }
    let tx = (clamped.x.floor().to_num::<i32>()).clamp(0, map.width.saturating_sub(1) as i32) as u32;
    let ty = (clamped.y.floor().to_num::<i32>()).clamp(0, map.height.saturating_sub(1) as i32) as u32;
    if isDomainTilePassable(map, blocked, domain, tx, ty) {
        return clamped;
    }

    let maxRadius = map.width.max(map.height) as i32;
    for r in 1..=maxRadius.min(20) {
        let mut best: Option<(i32, u32, u32)> = None;
        for dy in -r..=r {
            for dx in -r..=r {
                if dx.abs().max(dy.abs()) != r {
                    continue;
                }
                let nx = tx as i32 + dx;
                let ny = ty as i32 + dy;
                if nx < 0 || ny < 0 || nx >= map.width as i32 || ny >= map.height as i32 {
                    continue;
                }
                let (ux, uy) = (nx as u32, ny as u32);
                if isDomainTilePassable(map, blocked, domain, ux, uy) {
                    let cand = (dx * dx + dy * dy, uy, ux);
                    if best.is_none_or(|b| cand < b) {
                        best = Some(cand);
                    }
                }
            }
        }
        if let Some((_, uy, ux)) = best {
            return Vec2Fx::tileCenter(ux, uy);
        }
    }
    clamped
}

fn integerSqrtCeil(n: usize) -> usize {
    let mut r = 1;
    while r * r < n {
        r += 1;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flowFieldRoutesAroundWall() {
        let map = MapGrid::new(12, 12);
        let mut blocked = vec![false; 12 * 12];
        for y in 0..10 {
            blocked[y * 12 + 6] = true;
        }
        let field = FlowField::compute(&map, &blocked, (10, 2));
        let (mut cx, mut cy) = (2u32, 2u32);
        for _ in 0..40 {
            if (cx, cy) == (10, 2) {
                break;
            }
            let (dx, dy) = field.directionAt(cx, cy);
            assert_ne!((dx, dy), (0, 0), "caminho interrompido em ({cx}, {cy})");
            cx = (cx as i32 + i32::from(dx)) as u32;
            cy = (cy as i32 + i32::from(dy)) as u32;
            assert!(!blocked[(cy * 12 + cx) as usize], "atravessou parede!");
        }
        assert_eq!((cx, cy), (10, 2));
    }

    #[test]
    fn formationsProduceDistinctSlots() {
        let map = MapGrid::new(32, 32);
        let blocked = vec![false; 32 * 32];
        let origin = Vec2Fx::fromInts(5, 5);
        let dest = Vec2Fx::fromInts(20, 20);
        for form in [FormationKind::Box, FormationKind::Line, FormationKind::Wedge] {
            let slots = computeFormationTargets(
                &map,
                &blocked,
                MovementDomain::Land,
                5,
                origin,
                dest,
                form,
            );
            assert_eq!(slots.len(), 5);
            for i in 0..slots.len() {
                for j in (i + 1)..slots.len() {
                    assert_ne!(slots[i], slots[j]);
                }
            }
        }
    }
}
