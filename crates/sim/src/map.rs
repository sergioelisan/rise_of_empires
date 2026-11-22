use serde::{Deserialize, Serialize};

use crate::math::{Fx, Vec2Fx, fxVecSerde};

/// Tipos de terreno da simulação. Determinam passabilidade e, nas fases seguintes, recursos.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TerrainType {
    Water,
    Sand,
    Grass,
    Forest,
    Hill,
    Mountain,
}

impl TerrainType {
    /// Unidades terrestres podem atravessar este tile.
    pub fn isPassable(self) -> bool {
        matches!(self, Self::Sand | Self::Grass | Self::Hill)
    }
}

/// Grade determinística do mapa: alturas nos vértices `(width + 1) x (height + 1)`
/// e tipos de terreno nos tiles `width x height`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MapGrid {
    pub width: u32,
    pub height: u32,
    pub seed: u64,
    #[serde(with = "fxVecSerde")]
    heights: Vec<Fx>,
    tiles: Vec<TerrainType>,
}

impl MapGrid {
    /// Nível da superfície da água em unidades de mundo.
    pub const SEA_LEVEL: Fx = Fx::ZERO;

    /// Mapa plano de grama (útil para testes).
    pub fn new(width: u32, height: u32) -> Self {
        let vertexCount = ((width + 1) * (height + 1)) as usize;
        let tileCount = (width * height) as usize;
        Self {
            width,
            height,
            seed: 0,
            heights: vec![Fx::ZERO; vertexCount],
            tiles: vec![TerrainType::Grass; tileCount],
        }
    }

    /// Gera relevo, água, praias, florestas, colinas e montanhas de forma 100% determinística.
    pub fn generate(width: u32, height: u32, seed: u64) -> Self {
        let stride = width + 1;
        let mut heights = Vec::with_capacity((stride * (height + 1)) as usize);

        // Escalas do ruído em tiles (ponto fixo).
        let baseScale = Fx::from_num(18);
        let ridgeScale = Fx::from_num(14);
        let forestScale = Fx::from_num(10);

        for vy in 0..=height {
            for vx in 0..=width {
                let x = Fx::from_num(vx);
                let y = Fx::from_num(vy);

                let base = fbmFx(x / baseScale, y / baseScale, 4, seed ^ 0x1A2B_3C4D_5E6F_7081);
                let ridgeNoise = fbmFx(x / ridgeScale, y / ridgeScale, 3, seed ^ 0x9E37_79B9_7F4A_7C15);
                let ridge = Fx::ONE - (Fx::from_num(2) * ridgeNoise - Fx::ONE).abs();

                // Mistura o relevo base com cristas para dar picos definidos nas áreas altas.
                let ridgeWeight = Fx::from_num(22) / Fx::from_num(100);
                let raw = base * (Fx::ONE - ridgeWeight) + (base * ridge) * ridgeWeight;

                heights.push(shapeElevation(raw));
            }
        }

        let sandLimit = Fx::from_num(22) / Fx::from_num(100);
        let shoreRim = Fx::from_num(10) / Fx::from_num(100);
        let puddleDepth = -(Fx::from_num(3) / Fx::from_num(100));
        let hillHeight = Fx::from_num(110) / Fx::from_num(100);
        let mountainHeight = Fx::from_num(210) / Fx::from_num(100);
        let steepSlope = Fx::from_num(95) / Fx::from_num(100);
        let forestThreshold = Fx::from_num(54) / Fx::from_num(100);
        let half = Fx::ONE / Fx::from_num(2);

        let mut tiles = Vec::with_capacity((width * height) as usize);
        for ty in 0..height {
            for tx in 0..width {
                let h00 = heights[(ty * stride + tx) as usize];
                let h10 = heights[(ty * stride + tx + 1) as usize];
                let h01 = heights[((ty + 1) * stride + tx) as usize];
                let h11 = heights[((ty + 1) * stride + tx + 1) as usize];

                let avgH = (h00 + h10 + h01 + h11) / Fx::from_num(4);
                let minH = h00.min(h10).min(h01).min(h11);
                let maxH = h00.max(h10).max(h01).max(h11);
                let slope = maxH - minH;

                let cx = Fx::from_num(tx) + half;
                let cy = Fx::from_num(ty) + half;
                let moisture = fbmFx(
                    cx / forestScale,
                    cy / forestScale,
                    3,
                    seed ^ 0x517C_C1B7_2722_0A95,
                );

                let kind = if avgH < Self::SEA_LEVEL || minH < puddleDepth {
                    TerrainType::Water
                } else if minH <= Self::SEA_LEVEL + shoreRim || avgH < sandLimit {
                    TerrainType::Sand
                } else if avgH >= mountainHeight || slope >= steepSlope {
                    TerrainType::Mountain
                } else if avgH >= hillHeight {
                    TerrainType::Hill
                } else if moisture > forestThreshold {
                    TerrainType::Forest
                } else {
                    TerrainType::Grass
                };

                tiles.push(kind);
            }
        }

        Self { width, height, seed, heights, tiles }
    }

    /// Altura do vértice `(vx, vy)` da malha do terreno.
    pub fn vertexHeight(&self, vx: u32, vy: u32) -> Fx {
        let cx = vx.min(self.width);
        let cy = vy.min(self.height);
        self.heights[(cy * (self.width + 1) + cx) as usize]
    }

    /// Altura interpolada bilinearmente em qualquer ponto do mapa.
    pub fn heightAt(&self, p: Vec2Fx) -> Fx {
        if self.width == 0 || self.height == 0 {
            return Fx::ZERO;
        }
        let clamped = self.clampPoint(p);
        let maxX = self.width - 1;
        let maxY = self.height - 1;

        let tx = (clamped.x.floor().to_num::<i32>()).clamp(0, maxX as i32) as u32;
        let ty = (clamped.y.floor().to_num::<i32>()).clamp(0, maxY as i32) as u32;

        let fx = (clamped.x - Fx::from_num(tx)).clamp(Fx::ZERO, Fx::ONE);
        let fy = (clamped.y - Fx::from_num(ty)).clamp(Fx::ZERO, Fx::ONE);

        let h00 = self.vertexHeight(tx, ty);
        let h10 = self.vertexHeight(tx + 1, ty);
        let h01 = self.vertexHeight(tx, ty + 1);
        let h11 = self.vertexHeight(tx + 1, ty + 1);

        let top = lerpFx(h00, h10, fx);
        let bottom = lerpFx(h01, h11, fx);
        lerpFx(top, bottom, fy)
    }

    pub fn tile(&self, tx: u32, ty: u32) -> Option<TerrainType> {
        if tx < self.width && ty < self.height {
            Some(self.tiles[(ty * self.width + tx) as usize])
        } else {
            None
        }
    }

    pub fn tileAtPos(&self, p: Vec2Fx) -> Option<TerrainType> {
        if self.width == 0 || self.height == 0 {
            return None;
        }
        let clamped = self.clampPoint(p);
        let tx = (clamped.x.floor().to_num::<i32>()).clamp(0, self.width as i32 - 1) as u32;
        let ty = (clamped.y.floor().to_num::<i32>()).clamp(0, self.height as i32 - 1) as u32;
        self.tile(tx, ty)
    }

    pub fn isPassableTile(&self, tx: u32, ty: u32) -> bool {
        self.tile(tx, ty).is_some_and(TerrainType::isPassable)
    }

    pub fn isPassablePos(&self, p: Vec2Fx) -> bool {
        self.tileAtPos(p).is_some_and(TerrainType::isPassable)
    }

    pub fn tiles(&self) -> &[TerrainType] {
        &self.tiles
    }

    pub fn heights(&self) -> &[Fx] {
        &self.heights
    }

    pub fn countTerrain(&self, kind: TerrainType) -> usize {
        self.tiles.iter().filter(|&&t| t == kind).count()
    }

    /// Prende um ponto dentro dos limites do mapa.
    pub fn clampPoint(&self, p: Vec2Fx) -> Vec2Fx {
        Vec2Fx::new(
            p.x.clamp(Fx::ZERO, Fx::from_num(self.width)),
            p.y.clamp(Fx::ZERO, Fx::from_num(self.height)),
        )
    }

    /// Busca determinística pelo centro do tile passável mais próximo de `p`.
    pub fn findNearestPassable(&self, p: Vec2Fx) -> Option<Vec2Fx> {
        if self.width == 0 || self.height == 0 {
            return None;
        }
        let clamped = self.clampPoint(p);
        let cx = (clamped.x.floor().to_num::<i32>()).clamp(0, self.width as i32 - 1);
        let cy = (clamped.y.floor().to_num::<i32>()).clamp(0, self.height as i32 - 1);
        let half = Fx::ONE / Fx::from_num(2);

        if self.isPassableTile(cx as u32, cy as u32) {
            return Some(Vec2Fx::new(Fx::from_num(cx) + half, Fx::from_num(cy) + half));
        }

        let maxRadius = self.width.max(self.height) as i32;
        for radius in 1..=maxRadius {
            let mut best: Option<(i32, u32, u32)> = None;
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx.abs().max(dy.abs()) != radius {
                        continue;
                    }
                    let tx = cx + dx;
                    let ty = cy + dy;
                    if tx < 0 || ty < 0 || tx >= self.width as i32 || ty >= self.height as i32 {
                        continue;
                    }
                    if self.isPassableTile(tx as u32, ty as u32) {
                        let distSq = dx * dx + dy * dy;
                        let candidate = (distSq, ty as u32, tx as u32);
                        if best.is_none_or(|b| candidate < b) {
                            best = Some(candidate);
                        }
                    }
                }
            }
            if let Some((_, ty, tx)) = best {
                return Some(Vec2Fx::new(Fx::from_num(tx) + half, Fx::from_num(ty) + half));
            }
        }
        None
    }
}

/// Transforma ruído normalizado `[0, 1]` em elevação com planícies amplas, bacias e picos.
fn shapeElevation(raw: Fx) -> Fx {
    let sea = Fx::from_num(32) / Fx::from_num(100);
    let plainTop = Fx::from_num(54) / Fx::from_num(100);
    let hillTop = Fx::from_num(64) / Fx::from_num(100);

    if raw < sea {
        // Fundo da água: desce até ~ -1.3.
        let t = (sea - raw) / sea;
        -(t * Fx::from_num(13) / Fx::from_num(10))
    } else if raw < plainTop {
        // Planícies suaves: 0.0 .. 1.05.
        let t = (raw - sea) / (plainTop - sea);
        t * Fx::from_num(105) / Fx::from_num(100)
    } else if raw < hillTop {
        // Colinas: 1.05 .. 2.10.
        let t = (raw - plainTop) / (hillTop - plainTop);
        Fx::from_num(105) / Fx::from_num(100) + t * Fx::from_num(105) / Fx::from_num(100)
    } else {
        // Montanhas: sobe com inclinação acentuada de 2.10 até ~ 5.2.
        let t = ((raw - hillTop) / (Fx::ONE - hillTop)).clamp(Fx::ZERO, Fx::ONE);
        let peak = t + t * t;
        Fx::from_num(210) / Fx::from_num(100) + peak * Fx::from_num(155) / Fx::from_num(100)
    }
}

/// Ruído fractal (fBm) em ponto fixo, normalizado para `[0, 1]`.
fn fbmFx(x: Fx, y: Fx, octaves: u32, seed: u64) -> Fx {
    let mut total = Fx::ZERO;
    let mut amplitude = Fx::ONE;
    let mut frequency = Fx::ONE;
    let mut norm = Fx::ZERO;
    let two = Fx::from_num(2);

    for octave in 0..octaves {
        let octaveSeed = seed.wrapping_add(u64::from(octave).wrapping_mul(0x9E37_79B9_7F4A_7C15));
        total += valueNoiseFx(x * frequency, y * frequency, octaveSeed) * amplitude;
        norm += amplitude;
        amplitude /= two;
        frequency *= two;
    }

    if norm > Fx::ZERO { total / norm } else { Fx::ZERO }
}

/// Ruído de valor 2D em ponto fixo com interpolação suave, em `[0, 1]`.
fn valueNoiseFx(x: Fx, y: Fx, seed: u64) -> Fx {
    let x0 = x.floor().to_num::<i32>();
    let y0 = y.floor().to_num::<i32>();
    let fx = smoothFx(x - Fx::from_num(x0));
    let fy = smoothFx(y - Fx::from_num(y0));

    let n00 = hashUnitFx(x0, y0, seed);
    let n10 = hashUnitFx(x0 + 1, y0, seed);
    let n01 = hashUnitFx(x0, y0 + 1, seed);
    let n11 = hashUnitFx(x0 + 1, y0 + 1, seed);

    let top = lerpFx(n00, n10, fx);
    let bottom = lerpFx(n01, n11, fx);
    lerpFx(top, bottom, fy)
}

fn smoothFx(t: Fx) -> Fx {
    t * t * (Fx::from_num(3) - Fx::from_num(2) * t)
}

fn lerpFx(a: Fx, b: Fx, t: Fx) -> Fx {
    a + (b - a) * t
}

/// Hash determinístico `(x, y, seed) -> [0, 1)` usando os 32 bits fracionários de `I32F32`.
fn hashUnitFx(x: i32, y: i32, seed: u64) -> Fx {
    let mut h = seed
        ^ (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    // Bits altos no intervalo [0, 2^32) viram diretamente fração em [0, 1).
    Fx::from_bits((h >> 32) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sameSeedGivesIdenticalMap() {
        let a = MapGrid::generate(64, 64, 42);
        let b = MapGrid::generate(64, 64, 42);
        assert_eq!(a.heights(), b.heights());
        assert_eq!(a.tiles(), b.tiles());
    }

    #[test]
    fn differentSeedsGiveDifferentMaps() {
        let a = MapGrid::generate(32, 32, 42);
        let b = MapGrid::generate(32, 32, 43);
        assert_ne!(a.heights(), b.heights());
    }

    #[test]
    fn generatedMapHasAllTerrainTypes() {
        let map = MapGrid::generate(64, 64, 42);
        assert!(map.countTerrain(TerrainType::Water) > 0, "sem água");
        assert!(map.countTerrain(TerrainType::Sand) > 0, "sem praia/areia");
        assert!(map.countTerrain(TerrainType::Grass) > 0, "sem grama");
        assert!(map.countTerrain(TerrainType::Forest) > 0, "sem floresta");
        assert!(map.countTerrain(TerrainType::Hill) > 0, "sem colina");
        assert!(map.countTerrain(TerrainType::Mountain) > 0, "sem montanha");
    }

    #[test]
    fn passabilityRulesMatchTerrainTypes() {
        assert!(!TerrainType::Water.isPassable());
        assert!(TerrainType::Sand.isPassable());
        assert!(TerrainType::Grass.isPassable());
        assert!(!TerrainType::Forest.isPassable());
        assert!(TerrainType::Hill.isPassable());
        assert!(!TerrainType::Mountain.isPassable());
    }

    #[test]
    fn heightAtInterpolatesCorners() {
        let mut map = MapGrid::new(2, 2);
        // Vértices do tile (0, 0): 0, 2, 4, 6 -> centro deve ser 3.
        map.heights[0] = Fx::from_num(0);
        map.heights[1] = Fx::from_num(2);
        map.heights[3] = Fx::from_num(4);
        map.heights[4] = Fx::from_num(6);

        assert_eq!(map.heightAt(Vec2Fx::fromInts(0, 0)), Fx::from_num(0));
        assert_eq!(map.heightAt(Vec2Fx::fromInts(1, 0)), Fx::from_num(2));
        let half = Fx::ONE / Fx::from_num(2);
        assert_eq!(map.heightAt(Vec2Fx::new(half, half)), Fx::from_num(3));
    }

    #[test]
    fn findNearestPassableAvoidsImpassable() {
        let mut map = MapGrid::new(5, 5);
        map.tiles[2 * 5 + 2] = TerrainType::Water;
        let found = map.findNearestPassable(Vec2Fx::fromInts(2, 2)).unwrap();
        assert!(map.isPassablePos(found));
        assert_ne!(map.tileAtPos(found), Some(TerrainType::Water));
    }
}
