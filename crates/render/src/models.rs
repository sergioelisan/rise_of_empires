use std::f32::consts::TAU;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use roeSim::UnitClass;

/// Construtor de malhas low-poly com sombreamento facetado (normal por triângulo) e cor por vértice.
#[derive(Default)]
pub struct LowPolyBuilder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
}

impl LowPolyBuilder {
    pub fn isEmpty(&self) -> bool {
        self.positions.is_empty()
    }

    pub fn pushTri(&mut self, a: Vec3, b: Vec3, c: Vec3, color: [f32; 4]) {
        let normal = (b - a).cross(c - a).normalize_or_zero().to_array();
        self.positions.extend_from_slice(&[a.to_array(), b.to_array(), c.to_array()]);
        self.normals.extend_from_slice(&[normal, normal, normal]);
        self.colors.extend_from_slice(&[color, color, color]);
    }

    pub fn pushQuad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, color: [f32; 4]) {
        self.pushTri(a, b, c, color);
        self.pushTri(a, c, d, color);
    }

    /// Paralelepípedo alinhado aos eixos, centrado em `center` com dimensões totais `size`.
    pub fn addBox(&mut self, center: Vec3, size: Vec3, color: [f32; 4]) {
        let h = size * 0.5;
        let (x0, x1) = (center.x - h.x, center.x + h.x);
        let (y0, y1) = (center.y - h.y, center.y + h.y);
        let (z0, z1) = (center.z - h.z, center.z + h.z);

        let p000 = Vec3::new(x0, y0, z0);
        let p100 = Vec3::new(x1, y0, z0);
        let p110 = Vec3::new(x1, y1, z0);
        let p010 = Vec3::new(x0, y1, z0);
        let p001 = Vec3::new(x0, y0, z1);
        let p101 = Vec3::new(x1, y0, z1);
        let p111 = Vec3::new(x1, y1, z1);
        let p011 = Vec3::new(x0, y1, z1);

        // +Y (topo)
        self.pushQuad(p010, p011, p111, p110, tint(color, 1.08));
        // -Y (base)
        self.pushQuad(p000, p100, p101, p001, tint(color, 0.72));
        // +Z (frente)
        self.pushQuad(p001, p101, p111, p011, color);
        // -Z (trás)
        self.pushQuad(p100, p000, p010, p110, tint(color, 0.90));
        // +X (direita)
        self.pushQuad(p101, p100, p110, p111, tint(color, 0.95));
        // -X (esquerda)
        self.pushQuad(p000, p001, p011, p010, tint(color, 0.86));
    }

    pub fn addTaperedPrism(
        &mut self,
        base: Vec3,
        rBottom: f32,
        rTop: f32,
        height: f32,
        sides: u32,
        yaw: f32,
        color: [f32; 4],
    ) {
        for i in 0..sides {
            let a0 = yaw + TAU * i as f32 / sides as f32;
            let a1 = yaw + TAU * ((i + 1) % sides) as f32 / sides as f32;
            let b0 = base + Vec3::new(a0.cos() * rBottom, 0.0, a0.sin() * rBottom);
            let b1 = base + Vec3::new(a1.cos() * rBottom, 0.0, a1.sin() * rBottom);
            let t0 = base + Vec3::new(a0.cos() * rTop, height, a0.sin() * rTop);
            let t1 = base + Vec3::new(a1.cos() * rTop, height, a1.sin() * rTop);
            self.pushTri(b0, t0, b1, color);
            self.pushTri(b1, t0, t1, color);
        }
        // Tampa superior.
        if rTop > 0.001 {
            let topCenter = base + Vec3::Y * height;
            for i in 0..sides {
                let a0 = yaw + TAU * i as f32 / sides as f32;
                let a1 = yaw + TAU * ((i + 1) % sides) as f32 / sides as f32;
                let t0 = base + Vec3::new(a0.cos() * rTop, height, a0.sin() * rTop);
                let t1 = base + Vec3::new(a1.cos() * rTop, height, a1.sin() * rTop);
                self.pushTri(t0, topCenter, t1, tint(color, 1.06));
            }
        }
    }

    pub fn addCone(
        &mut self,
        base: Vec3,
        radius: f32,
        height: f32,
        sides: u32,
        yaw: f32,
        color: [f32; 4],
    ) {
        let apex = base + Vec3::Y * height;
        for i in 0..sides {
            let a0 = yaw + TAU * i as f32 / sides as f32;
            let a1 = yaw + TAU * ((i + 1) % sides) as f32 / sides as f32;
            let p0 = base + Vec3::new(a0.cos() * radius, 0.0, a0.sin() * radius);
            let p1 = base + Vec3::new(a1.cos() * radius, 0.0, a1.sin() * radius);
            self.pushTri(p0, apex, p1, color);
        }
    }

    pub fn addFacetedCrown(
        &mut self,
        center: Vec3,
        radius: f32,
        halfHeight: f32,
        yaw: f32,
        color: [f32; 4],
    ) {
        const SIDES: usize = 6;
        let top = center + Vec3::Y * halfHeight;
        let bottom = center - Vec3::Y * (halfHeight * 0.78);
        let mut ring = [Vec3::ZERO; SIDES];
        for (i, slot) in ring.iter_mut().enumerate() {
            let angle = yaw + TAU * i as f32 / SIDES as f32;
            let wobble = if i % 2 == 0 { 1.05 } else { 0.88 };
            let yJitter = if i % 2 == 0 { 0.08 * halfHeight } else { -0.06 * halfHeight };
            *slot = center
                + Vec3::new(angle.cos() * radius * wobble, yJitter, angle.sin() * radius * wobble);
        }
        for i in 0..SIDES {
            let p0 = ring[i];
            let p1 = ring[(i + 1) % SIDES];
            self.pushTri(p0, top, p1, color);
            self.pushTri(p1, bottom, p0, tint(color, 0.85));
        }
    }

    /// Telhado piramidal de 4 águas sobre uma base retangular em `baseY`.
    pub fn addPyramidRoof(
        &mut self,
        centerXZ: Vec2,
        baseY: f32,
        sizeXZ: Vec2,
        height: f32,
        color: [f32; 4],
    ) {
        let hx = sizeXZ.x * 0.5;
        let hz = sizeXZ.y * 0.5;
        let p0 = Vec3::new(centerXZ.x - hx, baseY, centerXZ.y - hz);
        let p1 = Vec3::new(centerXZ.x + hx, baseY, centerXZ.y - hz);
        let p2 = Vec3::new(centerXZ.x + hx, baseY, centerXZ.y + hz);
        let p3 = Vec3::new(centerXZ.x - hx, baseY, centerXZ.y + hz);
        let apex = Vec3::new(centerXZ.x, baseY + height, centerXZ.y);

        self.pushTri(p0, p3, apex, tint(color, 0.92));
        self.pushTri(p3, p2, apex, color);
        self.pushTri(p2, p1, apex, tint(color, 1.05));
        self.pushTri(p1, p0, apex, tint(color, 0.88));
    }

    /// Telhado de duas águas com cumeeira ao longo do eixo X.
    pub fn addGabledRoof(
        &mut self,
        centerXZ: Vec2,
        baseY: f32,
        sizeXZ: Vec2,
        height: f32,
        color: [f32; 4],
    ) {
        let hx = sizeXZ.x * 0.5;
        let hz = sizeXZ.y * 0.5;
        let p00 = Vec3::new(centerXZ.x - hx, baseY, centerXZ.y - hz);
        let p10 = Vec3::new(centerXZ.x + hx, baseY, centerXZ.y - hz);
        let p11 = Vec3::new(centerXZ.x + hx, baseY, centerXZ.y + hz);
        let p01 = Vec3::new(centerXZ.x - hx, baseY, centerXZ.y + hz);
        let r0 = Vec3::new(centerXZ.x - hx, baseY + height, centerXZ.y);
        let r1 = Vec3::new(centerXZ.x + hx, baseY + height, centerXZ.y);

        // Duas águas
        self.pushQuad(p01, p11, r1, r0, color);
        self.pushQuad(p10, p00, r0, r1, tint(color, 0.90));
        // Empenas
        self.pushTri(p00, p01, r0, tint(color, 0.82));
        self.pushTri(p11, p10, r1, tint(color, 0.86));
    }

    pub fn build(self) -> Mesh {
        let indices: Vec<u32> = (0..self.positions.len() as u32).collect();
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.colors);
        mesh.insert_indices(Indices::U32(indices));
        mesh
    }
}

pub fn rgbToLinear(rgb: (u8, u8, u8)) -> [f32; 4] {
    let c = Color::srgb_u8(rgb.0, rgb.1, rgb.2).to_linear();
    [c.red, c.green, c.blue, 1.0]
}

fn tint(c: [f32; 4], factor: f32) -> [f32; 4] {
    [
        (c[0] * factor).clamp(0.0, 1.0),
        (c[1] * factor).clamp(0.0, 1.0),
        (c[2] * factor).clamp(0.0, 1.0),
        c[3],
    ]
}

const SKIN: [f32; 4] = [0.78, 0.56, 0.40, 1.0];
const WOOD: [f32; 4] = [0.28, 0.16, 0.08, 1.0];
const IRON: [f32; 4] = [0.42, 0.45, 0.49, 1.0];
const BRONZE: [f32; 4] = [0.64, 0.45, 0.16, 1.0];
const GOLD: [f32; 4] = [0.85, 0.68, 0.18, 1.0];
const DARK_LEATHER: [f32; 4] = [0.18, 0.11, 0.07, 1.0];

// ============================================================================
// MODELOS LOW-POLY DE UNIDADES (evoluem com a era + unidades únicas)
// Convenção: unidade olha para +X no espaço local.
// ============================================================================

pub fn buildUnitMesh(
    class: UnitClass,
    kind: &str,
    age: u8,
    primary: [f32; 4],
    accent: [f32; 4],
) -> Mesh {
    let mut b = LowPolyBuilder::default();
    match class {
        UnitClass::Citizen => buildCitizen(&mut b, primary, accent),
        UnitClass::Infantry => buildInfantry(&mut b, kind, age, primary, accent),
        UnitClass::Archer => buildArcher(&mut b, kind, age, primary, accent),
        UnitClass::Cavalry => buildCavalry(&mut b, kind, age, primary, accent),
        UnitClass::Siege => buildSiege(&mut b, age, primary, accent),
        UnitClass::Naval => buildNaval(&mut b, kind, age, primary, accent),
        UnitClass::Air => buildAir(&mut b, age, primary, accent),
    }
    b.build()
}

fn buildHumanoidBase(b: &mut LowPolyBuilder, tunic: [f32; 4], pants: [f32; 4], scale: f32) {
    // Pernas
    b.addBox(Vec3::new(0.0, 0.16 * scale, -0.07 * scale), Vec3::new(0.11, 0.32, 0.10) * scale, pants);
    b.addBox(Vec3::new(0.0, 0.16 * scale, 0.07 * scale), Vec3::new(0.11, 0.32, 0.10) * scale, pants);
    // Torso
    b.addBox(Vec3::new(0.0, 0.48 * scale, 0.0), Vec3::new(0.24, 0.36, 0.28) * scale, tunic);
    // Cabeça
    b.addBox(Vec3::new(0.0, 0.78 * scale, 0.0), Vec3::new(0.18, 0.19, 0.18) * scale, SKIN);
}

fn buildCitizen(b: &mut LowPolyBuilder, primary: [f32; 4], accent: [f32; 4]) {
    let tunic = tint(primary, 0.85);
    buildHumanoidBase(b, tunic, DARK_LEATHER, 0.92);
    // Chapéu de palha / trabalho
    b.addCone(Vec3::new(0.0, 0.78, 0.0), 0.19, 0.14, 6, 0.0, [0.76, 0.62, 0.28, 1.0]);
    // Mochila de coleta nas costas
    b.addBox(Vec3::new(-0.16, 0.44, 0.0), Vec3::new(0.12, 0.24, 0.20), accent);
    // Braço + ferramenta (machado/enxada/picareta) bem visível durante a animação de trabalho
    b.addBox(Vec3::new(0.16, 0.46, 0.17), Vec3::new(0.16, 0.08, 0.08), tunic);
    b.addBox(Vec3::new(0.28, 0.54, 0.17), Vec3::new(0.06, 0.56, 0.05), WOOD);
    b.addBox(Vec3::new(0.35, 0.78, 0.17), Vec3::new(0.16, 0.11, 0.06), IRON);
}

fn buildInfantry(
    b: &mut LowPolyBuilder,
    kind: &str,
    age: u8,
    primary: [f32; 4],
    accent: [f32; 4],
) {
    let armorMetal = if age >= 2 { IRON } else { BRONZE };
    let torsoCol = if age >= 2 { tint(primary, 0.9) } else { primary };
    buildHumanoidBase(b, torsoCol, DARK_LEATHER, 1.0);

    // Capacete / cobertura por era e unidade única
    match kind {
        "legionary" => {
            b.addBox(Vec3::new(0.0, 0.90, 0.0), Vec3::new(0.21, 0.10, 0.21), IRON);
            b.addBox(Vec3::new(0.0, 0.98, 0.0), Vec3::new(0.24, 0.09, 0.06), primary);
        }
        "hoplite" => {
            b.addBox(Vec3::new(0.0, 0.90, 0.0), Vec3::new(0.21, 0.12, 0.21), BRONZE);
            b.addBox(Vec3::new(-0.02, 1.01, 0.0), Vec3::new(0.26, 0.10, 0.07), accent);
        }
        "immortal" => {
            b.addTaperedPrism(Vec3::new(0.0, 0.86, 0.0), 0.11, 0.13, 0.18, 6, 0.0, GOLD);
        }
        _ => match age {
            0 => b.addBox(Vec3::new(0.0, 0.89, 0.0), Vec3::new(0.20, 0.08, 0.20), DARK_LEATHER),
            1 => {
                b.addBox(Vec3::new(0.0, 0.90, 0.0), Vec3::new(0.21, 0.10, 0.21), BRONZE);
                b.addBox(Vec3::new(0.0, 0.97, 0.0), Vec3::new(0.16, 0.07, 0.05), accent);
            }
            2 => {
                b.addCone(Vec3::new(0.0, 0.86, 0.0), 0.14, 0.18, 6, 0.0, IRON);
                // Ombreiras de placa
                b.addBox(Vec3::new(0.0, 0.62, 0.0), Vec3::new(0.28, 0.10, 0.36), IRON);
            }
            _ => {
                // Chapéu tricórnio (Era da Pólvora)
                b.addBox(Vec3::new(0.0, 0.90, 0.0), Vec3::new(0.26, 0.06, 0.26), DARK_LEATHER);
                b.addBox(Vec3::new(0.0, 0.95, 0.0), Vec3::new(0.16, 0.08, 0.16), primary);
            }
        },
    }

    // Escudo (ausente no mosqueteiro da Era da Pólvora)
    if age < 3 || kind == "legionary" || kind == "hoplite" {
        let (shieldW, shieldH, shieldCol) = match kind {
            "legionary" => (0.06, 0.50, primary),
            "hoplite" => (0.06, 0.42, BRONZE),
            _ => (0.06, 0.36, accent),
        };
        b.addBox(Vec3::new(0.16, 0.46, -0.18), Vec3::new(shieldW, shieldH, 0.30), shieldCol);
        b.addBox(Vec3::new(0.19, 0.46, -0.18), Vec3::new(0.03, 0.10, 0.10), GOLD);
    }

    // Arma (lança -> espada -> pique -> mosquete)
    if age >= 3 && kind != "legionary" && kind != "hoplite" {
        // Mosquete
        b.addBox(Vec3::new(0.32, 0.52, 0.12), Vec3::new(0.68, 0.06, 0.06), WOOD);
        b.addBox(Vec3::new(0.55, 0.55, 0.12), Vec3::new(0.45, 0.04, 0.04), IRON);
    } else if age == 2 {
        // Pique longo
        b.addBox(Vec3::new(0.20, 0.72, 0.18), Vec3::new(0.05, 1.15, 0.05), WOOD);
        b.addCone(Vec3::new(0.20, 1.28, 0.18), 0.05, 0.18, 4, 0.0, IRON);
    } else if age == 1 || kind == "legionary" {
        // Espada curta / gládio
        b.addBox(Vec3::new(0.32, 0.48, 0.18), Vec3::new(0.42, 0.05, 0.05), armorMetal);
        b.addBox(Vec3::new(0.14, 0.48, 0.18), Vec3::new(0.05, 0.14, 0.05), GOLD);
    } else {
        // Lança
        b.addBox(Vec3::new(0.18, 0.64, 0.18), Vec3::new(0.05, 0.95, 0.05), WOOD);
        b.addCone(Vec3::new(0.18, 1.10, 0.18), 0.05, 0.15, 4, 0.0, armorMetal);
    }
}

fn buildArcher(
    b: &mut LowPolyBuilder,
    kind: &str,
    age: u8,
    primary: [f32; 4],
    accent: [f32; 4],
) {
    buildHumanoidBase(b, primary, DARK_LEATHER, 0.95);
    // Capuz / elmo leve
    b.addCone(Vec3::new(0.0, 0.84, 0.0), 0.13, 0.14, 5, 0.0, accent);
    // Aljava nas costas
    b.addBox(Vec3::new(-0.16, 0.52, -0.06), Vec3::new(0.08, 0.30, 0.10), WOOD);

    if kind == "chu_ko_nu" || age == 2 {
        // Besta / besta de repetição
        b.addBox(Vec3::new(0.26, 0.50, 0.06), Vec3::new(0.38, 0.07, 0.08), WOOD);
        b.addBox(Vec3::new(0.36, 0.50, 0.06), Vec3::new(0.06, 0.05, 0.42), IRON);
    } else if age >= 3 {
        // Carabina / atirador
        b.addBox(Vec3::new(0.34, 0.52, 0.08), Vec3::new(0.68, 0.05, 0.06), WOOD);
        b.addBox(Vec3::new(0.56, 0.54, 0.08), Vec3::new(0.42, 0.04, 0.04), IRON);
    } else {
        // Arco longo
        b.addBox(Vec3::new(0.24, 0.52, 0.10), Vec3::new(0.06, 0.58, 0.05), WOOD);
        b.addBox(Vec3::new(0.20, 0.52, 0.10), Vec3::new(0.26, 0.03, 0.03), accent);
    }
}

fn buildCavalry(
    b: &mut LowPolyBuilder,
    kind: &str,
    age: u8,
    primary: [f32; 4],
    accent: [f32; 4],
) {
    let horseCol = [0.40, 0.24, 0.13, 1.0];
    // Corpo do cavalo
    b.addBox(Vec3::new(0.0, 0.54, 0.0), Vec3::new(0.76, 0.30, 0.30), horseCol);
    // 4 patas
    for (lx, lz) in [(-0.26, -0.10), (-0.26, 0.10), (0.26, -0.10), (0.26, 0.10)] {
        b.addBox(Vec3::new(lx, 0.22, lz), Vec3::new(0.09, 0.44, 0.09), tint(horseCol, 0.85));
    }
    // Pescoço e cabeça
    b.addBox(Vec3::new(0.38, 0.72, 0.0), Vec3::new(0.16, 0.28, 0.16), horseCol);
    b.addBox(Vec3::new(0.48, 0.82, 0.0), Vec3::new(0.22, 0.14, 0.14), tint(horseCol, 0.92));

    // Barda / sela da era
    let caparison = if age >= 2 { IRON } else { primary };
    b.addBox(Vec3::new(0.0, 0.62, 0.0), Vec3::new(0.42, 0.18, 0.34), caparison);

    if kind == "war_chariot" {
        // Carro de guerra atrás do cavalo
        b.addBox(Vec3::new(-0.56, 0.38, 0.0), Vec3::new(0.42, 0.30, 0.44), GOLD);
        b.addTaperedPrism(Vec3::new(-0.56, 0.20, -0.26), 0.22, 0.22, 0.06, 8, 0.0, WOOD);
        b.addTaperedPrism(Vec3::new(-0.56, 0.20, 0.20), 0.22, 0.22, 0.06, 8, 0.0, WOOD);
        // Condutor no carro
        b.addBox(Vec3::new(-0.52, 0.74, 0.0), Vec3::new(0.20, 0.34, 0.22), primary);
        b.addBox(Vec3::new(-0.52, 0.98, 0.0), Vec3::new(0.16, 0.16, 0.16), SKIN);
    } else {
        // Cavaleiro
        b.addBox(Vec3::new(-0.02, 0.86, 0.0), Vec3::new(0.22, 0.34, 0.24), primary);
        b.addBox(Vec3::new(-0.02, 1.12, 0.0), Vec3::new(0.16, 0.17, 0.16), SKIN);
        b.addCone(Vec3::new(-0.02, 1.19, 0.0), 0.12, 0.15, 6, 0.0, if age >= 2 { IRON } else { accent });
        // Lança de cavalaria
        b.addBox(Vec3::new(0.42, 0.88, 0.18), Vec3::new(0.95, 0.05, 0.05), WOOD);
        b.addBox(Vec3::new(0.86, 0.88, 0.18), Vec3::new(0.16, 0.07, 0.07), IRON);
    }
}

fn buildSiege(b: &mut LowPolyBuilder, age: u8, primary: [f32; 4], accent: [f32; 4]) {
    // Base com rodas
    b.addBox(Vec3::new(0.0, 0.26, 0.0), Vec3::new(0.95, 0.14, 0.62), WOOD);
    for (wx, wz) in [(-0.32, -0.34), (-0.32, 0.34), (0.32, -0.34), (0.32, 0.34)] {
        b.addBox(Vec3::new(wx, 0.18, wz), Vec3::new(0.24, 0.28, 0.08), DARK_LEATHER);
    }

    match age {
        0 => {
            // Aríete coberto
            b.addGabledRoof(Vec2::ZERO, 0.34, Vec2::new(0.92, 0.66), 0.38, tint(WOOD, 1.15));
            b.addBox(Vec3::new(0.32, 0.34, 0.0), Vec3::new(0.60, 0.14, 0.14), IRON);
        }
        1 => {
            // Balista
            b.addBox(Vec3::new(0.0, 0.44, 0.0), Vec3::new(0.82, 0.12, 0.18), WOOD);
            b.addBox(Vec3::new(0.24, 0.46, 0.0), Vec3::new(0.12, 0.12, 0.88), tint(WOOD, 1.2));
            b.addBox(Vec3::new(0.44, 0.50, 0.0), Vec3::new(0.50, 0.05, 0.05), IRON);
            b.addBox(Vec3::new(-0.32, 0.52, 0.0), Vec3::new(0.14, 0.18, 0.30), primary);
        }
        2 => {
            // Trabuco
            b.addBox(Vec3::new(0.0, 0.62, -0.22), Vec3::new(0.12, 0.72, 0.10), WOOD);
            b.addBox(Vec3::new(0.0, 0.62, 0.22), Vec3::new(0.12, 0.72, 0.10), WOOD);
            b.addBox(Vec3::new(0.0, 0.94, 0.0), Vec3::new(0.10, 0.10, 0.52), IRON);
            b.addBox(Vec3::new(-0.18, 0.96, 0.0), Vec3::new(1.05, 0.09, 0.09), tint(WOOD, 1.15));
            b.addBox(Vec3::new(0.28, 0.68, 0.0), Vec3::new(0.24, 0.24, 0.26), accent);
        }
        _ => {
            // Canhão de bronze (Era da Pólvora)
            b.addBox(Vec3::new(-0.08, 0.36, 0.0), Vec3::new(0.75, 0.16, 0.34), primary);
            b.addBox(Vec3::new(0.20, 0.52, 0.0), Vec3::new(0.92, 0.22, 0.22), BRONZE);
            b.addBox(Vec3::new(0.64, 0.52, 0.0), Vec3::new(0.08, 0.26, 0.26), GOLD);
        }
    }
}

fn buildNaval(
    b: &mut LowPolyBuilder,
    kind: &str,
    age: u8,
    primary: [f32; 4],
    accent: [f32; 4],
) {
    let hullWood = tint(WOOD, 1.05);
    let deckWood = tint(WOOD, 1.30);
    let sailWhite = [0.92, 0.89, 0.80, 1.0];

    if kind == "warship" || age >= 2 && kind != "galley" {
        // Galeão / Navio de Guerra com castelo de popa, 2 mastros e canhões laterais.
        b.addBox(Vec3::new(0.0, 0.22, 0.0), Vec3::new(1.55, 0.34, 0.72), hullWood);
        b.addBox(Vec3::new(0.84, 0.24, 0.0), Vec3::new(0.32, 0.26, 0.44), hullWood);
        b.addBox(Vec3::new(-0.56, 0.46, 0.0), Vec3::new(0.46, 0.24, 0.66), deckWood);
        b.addBox(Vec3::new(0.0, 0.40, 0.0), Vec3::new(1.45, 0.06, 0.66), deckWood);
        // Gurupés na proa
        b.addBox(Vec3::new(1.08, 0.42, 0.0), Vec3::new(0.44, 0.06, 0.06), WOOD);
        // Canhões laterais
        for cx in [-0.25, 0.12, 0.45] {
            b.addBox(Vec3::new(cx, 0.30, 0.39), Vec3::new(0.09, 0.09, 0.12), IRON);
            b.addBox(Vec3::new(cx, 0.30, -0.39), Vec3::new(0.09, 0.09, 0.12), IRON);
        }
        // Mastro principal + vela com faixa da nação
        b.addBox(Vec3::new(0.10, 0.98, 0.0), Vec3::new(0.08, 1.25, 0.08), WOOD);
        b.addBox(Vec3::new(0.18, 1.05, 0.0), Vec3::new(0.06, 0.72, 0.92), sailWhite);
        b.addBox(Vec3::new(0.20, 1.05, 0.0), Vec3::new(0.04, 0.24, 0.94), primary);
        // Mastro secundário na popa
        b.addBox(Vec3::new(-0.48, 0.88, 0.0), Vec3::new(0.07, 0.92, 0.07), WOOD);
        b.addBox(Vec3::new(-0.42, 0.95, 0.0), Vec3::new(0.05, 0.52, 0.66), accent);
        // Estandarte no topo
        b.addBox(Vec3::new(0.24, 1.58, 0.0), Vec3::new(0.28, 0.14, 0.04), GOLD);
    } else {
        // Galera clássica: casco esguio, aríete de bronze, remos laterais e vela quadrada.
        b.addBox(Vec3::new(0.0, 0.16, 0.0), Vec3::new(1.38, 0.24, 0.52), hullWood);
        b.addBox(Vec3::new(0.76, 0.14, 0.0), Vec3::new(0.30, 0.14, 0.22), BRONZE);
        b.addBox(Vec3::new(0.0, 0.29, 0.0), Vec3::new(1.25, 0.05, 0.46), deckWood);
        // Remos
        for ox in [-0.36, -0.12, 0.12, 0.36] {
            b.addBox(Vec3::new(ox, 0.14, 0.42), Vec3::new(0.05, 0.04, 0.42), deckWood);
            b.addBox(Vec3::new(ox, 0.14, -0.42), Vec3::new(0.05, 0.04, 0.42), deckWood);
        }
        // Mastro e vela
        b.addBox(Vec3::new(0.05, 0.78, 0.0), Vec3::new(0.07, 1.02, 0.07), WOOD);
        b.addBox(Vec3::new(0.12, 0.84, 0.0), Vec3::new(0.05, 0.58, 0.74), sailWhite);
        b.addBox(Vec3::new(0.14, 0.84, 0.0), Vec3::new(0.04, 0.22, 0.76), primary);
        b.addBox(Vec3::new(0.18, 1.28, 0.0), Vec3::new(0.24, 0.12, 0.04), accent);
    }
}

fn buildAir(b: &mut LowPolyBuilder, age: u8, primary: [f32; 4], accent: [f32; 4]) {
    let canvas = [0.88, 0.84, 0.74, 1.0];
    if age >= 3 {
        // Biplano da Era da Pólvora/Industrial: fuselagem, asas duplas e hélice frontal.
        b.addBox(Vec3::new(0.0, 0.20, 0.0), Vec3::new(1.25, 0.22, 0.24), primary);
        b.addBox(Vec3::new(0.56, 0.20, 0.0), Vec3::new(0.18, 0.20, 0.22), IRON);
        // Hélice frontal
        b.addBox(Vec3::new(0.68, 0.20, 0.0), Vec3::new(0.04, 0.42, 0.06), WOOD);
        b.addBox(Vec3::new(0.68, 0.20, 0.0), Vec3::new(0.04, 0.06, 0.42), WOOD);
        // Asa inferior e asa superior
        b.addBox(Vec3::new(0.14, 0.12, 0.0), Vec3::new(0.36, 0.05, 1.56), canvas);
        b.addBox(Vec3::new(0.16, 0.48, 0.0), Vec3::new(0.38, 0.05, 1.66), accent);
        // Montantes entre asas
        for sz in [-0.52, 0.52] {
            b.addBox(Vec3::new(0.15, 0.30, sz), Vec3::new(0.04, 0.36, 0.04), WOOD);
        }
        // Cauda e leme
        b.addBox(Vec3::new(-0.56, 0.24, 0.0), Vec3::new(0.24, 0.05, 0.58), canvas);
        b.addBox(Vec3::new(-0.56, 0.36, 0.0), Vec3::new(0.22, 0.24, 0.05), primary);
    } else {
        // Dirigível de reconhecimento/bombardeio: balão alongado + gôndola suspensa.
        b.addBox(Vec3::new(0.0, 0.72, 0.0), Vec3::new(1.38, 0.56, 0.62), canvas);
        b.addBox(Vec3::new(0.76, 0.72, 0.0), Vec3::new(0.28, 0.40, 0.44), canvas);
        b.addBox(Vec3::new(-0.76, 0.72, 0.0), Vec3::new(0.28, 0.40, 0.44), canvas);
        // Faixa da nação no balão
        b.addBox(Vec3::new(0.0, 0.72, 0.0), Vec3::new(0.42, 0.59, 0.65), primary);
        // Leme e estabilizadores
        b.addBox(Vec3::new(-0.84, 0.72, 0.0), Vec3::new(0.26, 0.48, 0.05), accent);
        b.addBox(Vec3::new(-0.84, 0.72, 0.0), Vec3::new(0.26, 0.05, 0.52), accent);
        // Gôndola e cabos
        b.addBox(Vec3::new(0.0, 0.14, 0.0), Vec3::new(0.68, 0.22, 0.28), WOOD);
        b.addBox(Vec3::new(0.0, 0.20, 0.0), Vec3::new(0.56, 0.08, 0.30), primary);
        for sx in [-0.22, 0.22] {
            b.addBox(Vec3::new(sx, 0.36, -0.10), Vec3::new(0.03, 0.28, 0.03), DARK_LEATHER);
            b.addBox(Vec3::new(sx, 0.36, 0.10), Vec3::new(0.03, 0.28, 0.03), DARK_LEATHER);
        }
    }
}

// ============================================================================
// MODELOS LOW-POLY DE CONSTRUÇÕES E MARAVILHAS (mudam visualmente por Era!)
// ============================================================================

struct EraPalette {
    wall: [f32; 4],
    trim: [f32; 4],
    roof: [f32; 4],
}

fn eraPalette(age: u8) -> EraPalette {
    match age {
        0 => EraPalette {
            wall: [0.68, 0.56, 0.38, 1.0],  // adobe / arenito
            trim: [0.42, 0.28, 0.16, 1.0],  // madeira bruta
            roof: [0.64, 0.50, 0.22, 1.0],  // palha / ocre
        },
        1 => EraPalette {
            wall: [0.84, 0.80, 0.72, 1.0],  // mármore / calcário
            trim: [0.72, 0.66, 0.56, 1.0],  // colunas claras
            roof: [0.68, 0.28, 0.18, 1.0],  // telha terracota
        },
        2 => EraPalette {
            wall: [0.56, 0.56, 0.58, 1.0],  // pedra medieval
            trim: [0.36, 0.36, 0.40, 1.0],  // ameias escuras
            roof: [0.24, 0.30, 0.40, 1.0],  // ardósia
        },
        _ => EraPalette {
            wall: [0.66, 0.38, 0.28, 1.0],  // tijolo imperial
            trim: [0.82, 0.78, 0.70, 1.0],  // cantaria clara
            roof: [0.20, 0.34, 0.32, 1.0],  // cobre oxidado / mansarda
        },
    }
}

pub fn buildBuildingMesh(
    kind: &str,
    wonderId: Option<&str>,
    footprint: u32,
    age: u8,
    isComplete: bool,
    primary: [f32; 4],
    accent: [f32; 4],
) -> Mesh {
    let mut b = LowPolyBuilder::default();
    let span = footprint as f32;

    if !isComplete {
        buildScaffold(&mut b, span, primary);
        return b.build();
    }

    let pal = eraPalette(age);
    if kind == "wonder" {
        buildWonder(&mut b, wonderId.unwrap_or("pyramids"), span, primary, accent);
        return b.build();
    }

    // Fundação leve para assentar em terrenos inclinados.
    b.addBox(
        Vec3::new(0.0, -0.10, 0.0),
        Vec3::new(span * 0.92, 0.36, span * 0.92),
        tint(pal.trim, 0.85),
    );

    match kind {
        "city_center" => {
            // Salão central + alas laterais + torre de vigia + bandeira da nação.
            b.addBox(Vec3::new(0.0, 0.55, 0.0), Vec3::new(1.75, 1.05, 1.55), pal.wall);
            b.addGabledRoof(Vec2::ZERO, 1.08, Vec2::new(1.90, 1.70), 0.65, pal.roof);
            // Torre lateral
            b.addBox(Vec3::new(-0.85, 0.95, -0.70), Vec3::new(0.68, 1.85, 0.68), pal.wall);
            b.addPyramidRoof(Vec2::new(-0.85, -0.70), 1.88, Vec2::new(0.80, 0.80), 0.55, primary);
            // Mastro e estandarte
            b.addBox(Vec3::new(0.95, 1.05, 0.85), Vec3::new(0.07, 2.05, 0.07), WOOD);
            b.addBox(Vec3::new(1.18, 1.85, 0.85), Vec3::new(0.45, 0.28, 0.05), primary);
            b.addBox(Vec3::new(1.18, 1.85, 0.85), Vec3::new(0.18, 0.14, 0.07), accent);
            // Colunas na Era Clássica+
            if age >= 1 {
                for ix in [-0.6, 0.0, 0.6] {
                    b.addBox(Vec3::new(ix, 0.52, 0.84), Vec3::new(0.14, 0.96, 0.14), pal.trim);
                }
            }
        }
        "farm" => {
            // Campo cultivado + celeiro pequeno
            b.addBox(Vec3::new(0.0, 0.09, 0.0), Vec3::new(1.78, 0.06, 1.78), [0.34, 0.24, 0.12, 1.0]);
            for row in [-0.55, -0.18, 0.18, 0.55] {
                b.addBox(
                    Vec3::new(-0.22, 0.20, row),
                    Vec3::new(1.10, 0.18, 0.18),
                    [0.76, 0.62, 0.18, 1.0],
                );
            }
            b.addBox(Vec3::new(0.52, 0.36, -0.42), Vec3::new(0.60, 0.54, 0.66), pal.wall);
            b.addGabledRoof(Vec2::new(0.52, -0.42), 0.63, Vec2::new(0.70, 0.74), 0.30, pal.roof);
        }
        "lumber_camp" => {
            b.addBox(Vec3::new(-0.25, 0.40, -0.20), Vec3::new(0.95, 0.65, 0.85), WOOD);
            b.addGabledRoof(Vec2::new(-0.25, -0.20), 0.72, Vec2::new(1.10, 0.95), 0.34, pal.roof);
            // Pilha de toras
            for i in 0..3 {
                b.addBox(
                    Vec3::new(0.45, 0.16 + i as f32 * 0.14, 0.32),
                    Vec3::new(0.68, 0.13, 0.28 - i as f32 * 0.06),
                    tint(WOOD, 1.25),
                );
            }
            b.addBox(Vec3::new(-0.70, 0.50, 0.55), Vec3::new(0.06, 0.90, 0.06), primary);
        }
        "mine" => {
            // Arco de pedra da mina + pilha de minério
            b.addBox(Vec3::new(0.0, 0.45, -0.15), Vec3::new(1.25, 0.82, 0.95), [0.36, 0.35, 0.34, 1.0]);
            b.addBox(Vec3::new(0.0, 0.34, 0.34), Vec3::new(0.56, 0.52, 0.12), [0.06, 0.06, 0.07, 1.0]);
            b.addBox(Vec3::new(0.0, 0.66, 0.38), Vec3::new(0.76, 0.12, 0.16), WOOD);
            b.addFacetedCrown(Vec3::new(0.52, 0.22, 0.42), 0.26, 0.20, 0.4, IRON);
            b.addFacetedCrown(Vec3::new(-0.48, 0.20, 0.45), 0.20, 0.16, 1.2, GOLD);
        }
        "market" => {
            // Pátio aberto pavimentado com barracas coloridas ao redor para os mercadores circularem dentro!
            b.addBox(Vec3::new(0.0, 0.09, 0.0), Vec3::new(1.86, 0.06, 1.86), [0.58, 0.54, 0.46, 1.0]);
            // Barraca traseira esquerda
            b.addBox(Vec3::new(-0.56, 0.34, -0.62), Vec3::new(0.68, 0.46, 0.44), WOOD);
            b.addGabledRoof(Vec2::new(-0.56, -0.62), 0.58, Vec2::new(0.76, 0.54), 0.24, primary);
            // Barraca traseira direita
            b.addBox(Vec3::new(0.56, 0.34, -0.62), Vec3::new(0.68, 0.46, 0.44), WOOD);
            b.addGabledRoof(Vec2::new(0.56, -0.62), 0.58, Vec2::new(0.76, 0.54), 0.24, accent);
            // Barraca lateral esquerda + balcão de moedas
            b.addBox(Vec3::new(-0.68, 0.30, 0.46), Vec3::new(0.42, 0.38, 0.66), WOOD);
            b.addGabledRoof(Vec2::new(-0.68, 0.46), 0.50, Vec2::new(0.50, 0.74), 0.20, accent);
            // Pilha de caixas/moedas no canto direito
            b.addBox(Vec3::new(0.64, 0.22, 0.56), Vec3::new(0.32, 0.24, 0.32), GOLD);
            b.addBox(Vec3::new(0.72, 0.66, -0.76), Vec3::new(0.06, 1.20, 0.06), WOOD);
            b.addBox(Vec3::new(0.90, 1.12, -0.76), Vec3::new(0.34, 0.20, 0.05), primary);
        }
        "temple" => {
            // Santuário clássico sobre degraus com colunas e braseiro dourado.
            b.addBox(Vec3::new(0.0, 0.16, 0.0), Vec3::new(1.78, 0.22, 1.66), pal.trim);
            b.addBox(Vec3::new(0.0, 0.62, -0.05), Vec3::new(1.22, 0.76, 1.05), pal.wall);
            for ix in [-0.72, -0.24, 0.24, 0.72] {
                b.addBox(Vec3::new(ix, 0.62, 0.66), Vec3::new(0.13, 0.86, 0.13), [0.90, 0.88, 0.82, 1.0]);
            }
            b.addGabledRoof(Vec2::ZERO, 1.04, Vec2::new(1.72, 1.64), 0.46, primary);
            b.addCone(Vec3::new(0.0, 1.48, 0.0), 0.20, 0.34, 6, 0.0, GOLD);
        }
        "library" => {
            // Edifício colunado com cúpula/frontão de saber
            b.addBox(Vec3::new(0.0, 0.52, 0.0), Vec3::new(1.45, 0.92, 1.35), pal.wall);
            b.addPyramidRoof(Vec2::ZERO, 0.98, Vec2::new(1.60, 1.50), 0.55, [0.18, 0.36, 0.64, 1.0]);
            b.addTaperedPrism(Vec3::new(0.0, 1.20, 0.0), 0.42, 0.20, 0.38, 8, 0.0, GOLD);
            for ix in [-0.52, 0.0, 0.52] {
                b.addBox(Vec3::new(ix, 0.48, 0.72), Vec3::new(0.13, 0.85, 0.13), pal.trim);
            }
        }
        "barracks" => {
            b.addBox(Vec3::new(0.0, 0.48, -0.10), Vec3::new(1.50, 0.86, 1.25), pal.wall);
            b.addGabledRoof(Vec2::new(0.0, -0.10), 0.91, Vec2::new(1.62, 1.38), 0.44, pal.roof);
            // Escudo e bandeira militar na fachada
            b.addBox(Vec3::new(0.0, 0.56, 0.56), Vec3::new(0.36, 0.42, 0.08), primary);
            b.addBox(Vec3::new(-0.68, 0.88, 0.58), Vec3::new(0.07, 1.60, 0.07), WOOD);
            b.addBox(Vec3::new(-0.46, 1.50, 0.58), Vec3::new(0.40, 0.24, 0.05), primary);
        }
        "stable" => {
            b.addBox(Vec3::new(-0.15, 0.42, -0.15), Vec3::new(1.35, 0.72, 1.15), tint(WOOD, 1.18));
            b.addGabledRoof(Vec2::new(-0.15, -0.15), 0.78, Vec2::new(1.50, 1.28), 0.45, pal.roof);
            // Cercado de cavalaria
            b.addBox(Vec3::new(0.55, 0.22, 0.52), Vec3::new(0.65, 0.08, 0.08), WOOD);
            b.addBox(Vec3::new(0.84, 0.22, 0.15), Vec3::new(0.08, 0.08, 0.80), WOOD);
            b.addBox(Vec3::new(0.68, 0.65, -0.68), Vec3::new(0.07, 1.15, 0.07), primary);
        }
        "siege_workshop" => {
            b.addBox(Vec3::new(0.0, 0.48, 0.0), Vec3::new(1.50, 0.82, 1.35), pal.wall);
            b.addGabledRoof(Vec2::ZERO, 0.90, Vec2::new(1.62, 1.48), 0.42, tint(WOOD, 0.95));
            // Guindaste de cerco
            b.addBox(Vec3::new(0.62, 0.86, 0.58), Vec3::new(0.10, 1.55, 0.10), WOOD);
            b.addBox(Vec3::new(0.32, 1.52, 0.58), Vec3::new(0.75, 0.10, 0.10), IRON);
        }
        "tower" => {
            b.addBox(Vec3::new(0.0, 0.92, 0.0), Vec3::new(0.66, 1.75, 0.66), pal.wall);
            b.addBox(Vec3::new(0.0, 1.84, 0.0), Vec3::new(0.82, 0.18, 0.82), pal.trim);
            b.addPyramidRoof(Vec2::ZERO, 1.94, Vec2::new(0.84, 0.84), 0.48, primary);
        }
        "dock" => {
            // Píer de madeira + oficina náutica + guindaste costeiro.
            b.addBox(Vec3::new(0.0, 0.16, 0.20), Vec3::new(1.86, 0.12, 1.56), tint(WOOD, 1.18));
            for (px, pz) in [(-0.78, -0.45), (0.78, -0.45), (-0.78, 0.82), (0.78, 0.82)] {
                b.addBox(Vec3::new(px, -0.10, pz), Vec3::new(0.14, 0.68, 0.14), WOOD);
            }
            // Galpão de barcos
            b.addBox(Vec3::new(-0.36, 0.52, -0.25), Vec3::new(0.98, 0.64, 0.84), pal.wall);
            b.addGabledRoof(Vec2::new(-0.36, -0.25), 0.84, Vec2::new(1.10, 0.94), 0.36, primary);
            // Guindaste portuário
            b.addBox(Vec3::new(0.58, 0.72, 0.36), Vec3::new(0.10, 1.15, 0.10), WOOD);
            b.addBox(Vec3::new(0.32, 1.24, 0.36), Vec3::new(0.64, 0.09, 0.09), IRON);
            b.addBox(Vec3::new(-0.74, 0.92, 0.64), Vec3::new(0.06, 1.35, 0.06), WOOD);
            b.addBox(Vec3::new(-0.56, 1.48, 0.64), Vec3::new(0.36, 0.22, 0.05), accent);
        }
        "airfield" => {
            // Pista pavimentada + hangar de aeronaves + biruta.
            b.addBox(Vec3::new(0.0, 0.10, 0.44), Vec3::new(1.86, 0.06, 0.76), [0.34, 0.36, 0.38, 1.0]);
            for sx in [-0.62, 0.0, 0.62] {
                b.addBox(Vec3::new(sx, 0.14, 0.44), Vec3::new(0.30, 0.02, 0.08), GOLD);
            }
            // Hangar amplo
            b.addBox(Vec3::new(-0.18, 0.56, -0.34), Vec3::new(1.42, 0.84, 0.92), pal.wall);
            b.addGabledRoof(Vec2::new(-0.18, -0.34), 0.98, Vec2::new(1.54, 1.02), 0.42, IRON);
            // Torre de controle / biruta
            b.addBox(Vec3::new(0.72, 0.78, -0.45), Vec3::new(0.32, 1.35, 0.32), pal.trim);
            b.addBox(Vec3::new(0.72, 1.48, -0.45), Vec3::new(0.42, 0.18, 0.42), primary);
            b.addBox(Vec3::new(0.84, 1.66, -0.18), Vec3::new(0.28, 0.12, 0.12), accent);
        }
        _ => {
            b.addBox(Vec3::new(0.0, 0.45, 0.0), Vec3::new(span * 0.72, 0.8, span * 0.72), pal.wall);
            b.addPyramidRoof(Vec2::ZERO, 0.85, Vec2::splat(span * 0.80), 0.40, primary);
        }
    }

    b.build()
}

fn buildScaffold(b: &mut LowPolyBuilder, span: f32, primary: [f32; 4]) {
    let s = span * 0.82;
    b.addBox(Vec3::new(0.0, 0.12, 0.0), Vec3::new(s, 0.24, s), [0.46, 0.42, 0.36, 1.0]);
    let h = s * 0.46;
    for (x, z) in [(-h, -h), (h, -h), (-h, h), (h, h)] {
        b.addBox(Vec3::new(x, 0.55, z), Vec3::new(0.08, 1.10, 0.08), WOOD);
    }
    b.addBox(Vec3::new(0.0, 0.85, -h), Vec3::new(s, 0.07, 0.07), tint(WOOD, 1.2));
    b.addBox(Vec3::new(0.0, 0.85, h), Vec3::new(s, 0.07, 0.07), tint(WOOD, 1.2));
    b.addBox(Vec3::new(0.0, 1.08, 0.0), Vec3::new(0.32, 0.20, 0.05), primary);
}

fn buildWonder(
    b: &mut LowPolyBuilder,
    wonderId: &str,
    span: f32,
    primary: [f32; 4],
    accent: [f32; 4],
) {
    let sandstone = [0.80, 0.68, 0.40, 1.0];
    let marble = [0.88, 0.86, 0.80, 1.0];

    b.addBox(Vec3::new(0.0, -0.08, 0.0), Vec3::new(span * 0.96, 0.36, span * 0.96), tint(marble, 0.78));

    match wonderId {
        "pyramids" => {
            // Grande Pirâmide escalonada com topo dourado.
            for tier in 0..5 {
                let s = 2.60 - tier as f32 * 0.44;
                b.addBox(Vec3::new(0.0, 0.24 + tier as f32 * 0.36, 0.0), Vec3::new(s, 0.36, s), sandstone);
            }
            b.addPyramidRoof(Vec2::ZERO, 2.04, Vec2::splat(0.65), 0.58, GOLD);
        }
        "colossus" => {
            // Dois pedestais + estátua colossal de bronze com tocha dourada.
            b.addBox(Vec3::new(-0.55, 0.45, 0.0), Vec3::new(0.65, 0.90, 0.75), marble);
            b.addBox(Vec3::new(0.55, 0.45, 0.0), Vec3::new(0.65, 0.90, 0.75), marble);
            b.addBox(Vec3::new(-0.35, 1.30, 0.0), Vec3::new(0.28, 0.90, 0.30), BRONZE);
            b.addBox(Vec3::new(0.35, 1.30, 0.0), Vec3::new(0.28, 0.90, 0.30), BRONZE);
            b.addBox(Vec3::new(0.0, 2.05, 0.0), Vec3::new(0.75, 0.85, 0.46), BRONZE);
            b.addBox(Vec3::new(0.0, 2.66, 0.0), Vec3::new(0.34, 0.36, 0.34), BRONZE);
            b.addBox(Vec3::new(0.52, 2.55, 0.0), Vec3::new(0.18, 0.75, 0.18), BRONZE);
            b.addFacetedCrown(Vec3::new(0.52, 3.05, 0.0), 0.24, 0.26, 0.0, GOLD);
        }
        "great_library" => {
            // Templo monumental de mármore com colunas e cúpula azul/ouro.
            b.addBox(Vec3::new(0.0, 0.65, 0.0), Vec3::new(2.35, 1.15, 1.95), marble);
            for ix in [-0.95, -0.48, 0.0, 0.48, 0.95] {
                b.addBox(Vec3::new(ix, 0.62, 1.05), Vec3::new(0.16, 1.12, 0.16), tint(marble, 1.05));
            }
            b.addGabledRoof(Vec2::ZERO, 1.24, Vec2::new(2.55, 2.25), 0.65, [0.16, 0.38, 0.68, 1.0]);
            b.addTaperedPrism(Vec3::new(0.0, 1.65, 0.0), 0.56, 0.28, 0.52, 8, 0.0, GOLD);
        }
        "colosseum" => {
            // Anfiteatro de 3 anéis em pedra clara.
            b.addTaperedPrism(Vec3::new(0.0, 0.10, 0.0), 1.32, 1.28, 0.65, 10, 0.0, sandstone);
            b.addTaperedPrism(Vec3::new(0.0, 0.75, 0.0), 1.22, 1.18, 0.55, 10, 0.0, marble);
            b.addTaperedPrism(Vec3::new(0.0, 1.30, 0.0), 1.14, 1.10, 0.40, 10, 0.0, primary);
        }
        _ => {
            // Pavilhão imperial Terracota com telhados curvos de jade e dourado.
            b.addBox(Vec3::new(0.0, 0.52, 0.0), Vec3::new(2.15, 0.85, 2.15), primary);
            b.addPyramidRoof(Vec2::ZERO, 0.95, Vec2::splat(2.55), 0.55, [0.14, 0.48, 0.36, 1.0]);
            b.addBox(Vec3::new(0.0, 1.48, 0.0), Vec3::new(1.30, 0.62, 1.30), accent);
            b.addPyramidRoof(Vec2::ZERO, 1.80, Vec2::splat(1.75), 0.55, [0.14, 0.48, 0.36, 1.0]);
            b.addCone(Vec3::new(0.0, 2.32, 0.0), 0.20, 0.45, 6, 0.0, GOLD);
        }
    }
}
