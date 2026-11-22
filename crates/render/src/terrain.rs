use std::f32::consts::TAU;

use bevy::asset::RenderAssetUsages;
use bevy::light::{CascadeShadowConfigBuilder, DirectionalLightShadowMap, NotShadowCaster};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use roeSim::{FogState, MapGrid, PlayerId, TerrainType};

use crate::bridge::{SimState, WorldResetCounter};
use crate::input::PlayerControlState;
use crate::models::{LowPolyBuilder, rgbToLinear};
use crate::terrainHeightF32;

const PIXELS_PER_TILE: u32 = 16;
const WATER_PIXELS_PER_TILE: u32 = 8;
const OVERLAY_PIXELS_PER_TILE: u32 = 4;

/// Marca todas as entidades geradas pelo terreno/água/árvores/overlay para reconstrução ao trocar de mapa.
#[derive(Component)]
struct TerrainScoped;

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (spawnTerrain, spawnBorderFogOverlay, spawnWater, spawnProps, spawnLights),
        )
        .add_systems(Update, (rebuildTerrainOnReset, updateBorderFogOverlay).chain());
    }
}

fn rebuildTerrainOnReset(
    mut commands: Commands,
    reset: Res<WorldResetCounter>,
    mut lastReset: Local<u64>,
    sim: Res<SimState>,
    control: Res<PlayerControlState>,
    existing: Query<Entity, With<TerrainScoped>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if *lastReset == reset.0 {
        return;
    }
    *lastReset = reset.0;
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    spawnTerrainMesh(&mut commands, &sim.0.map, &mut meshes, &mut images, &mut materials);
    spawnOverlayMesh(&mut commands, &sim.0, &control, &mut meshes, &mut images, &mut materials);
    spawnWaterMesh(&mut commands, &sim.0.map, &mut meshes, &mut images, &mut materials);
    spawnPropsMeshes(&mut commands, &sim.0.map, &mut meshes, &mut materials);
}

#[derive(Resource)]
struct OverlayState {
    imageHandle: Handle<Image>,
    lastTerritoryVersion: u64,
    lastFogVersion: u64,
    lastShowFog: bool,
    lastLocalPlayer: PlayerId,
}

fn spawnTerrain(
    mut commands: Commands,
    sim: Res<SimState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    spawnTerrainMesh(&mut commands, &sim.0.map, &mut meshes, &mut images, &mut materials);
}

fn spawnTerrainMesh(
    commands: &mut Commands,
    map: &MapGrid,
    meshes: &mut ResMut<Assets<Mesh>>,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let mesh = meshes.add(buildTerrainMesh(map, 0.0));
    let texture = images.add(makeTerrainTexture(map));

    commands.spawn((
        TerrainScoped,
        Mesh3d(mesh),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(texture),
            perceptual_roughness: 0.96,
            reflectance: 0.06,
            ..default()
        })),
        Transform::IDENTITY,
    ));
}

/// Camada dinâmica sobre o terreno para desenhar fronteiras nacionais (Fase 4) e névoa de guerra (Fase 5).
fn spawnBorderFogOverlay(
    mut commands: Commands,
    sim: Res<SimState>,
    control: Res<PlayerControlState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    spawnOverlayMesh(&mut commands, &sim.0, &control, &mut meshes, &mut images, &mut materials);
}

fn spawnOverlayMesh(
    commands: &mut Commands,
    world: &roeSim::SimWorld,
    control: &PlayerControlState,
    meshes: &mut ResMut<Assets<Mesh>>,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let map = &world.map;
    let mesh = meshes.add(buildTerrainMesh(map, 0.04));
    let image = makeOverlayTexture(world, control.localPlayer, control.showFog);
    let imageHandle = images.add(image);

    commands.spawn((
        TerrainScoped,
        Mesh3d(mesh),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(imageHandle.clone()),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        })),
        Transform::IDENTITY,
        NotShadowCaster,
    ));

    let fogVersion = world.player(control.localPlayer).map_or(0, |p| p.fogVersion);
    commands.insert_resource(OverlayState {
        imageHandle,
        lastTerritoryVersion: world.territory.version,
        lastFogVersion: fogVersion,
        lastShowFog: control.showFog,
        lastLocalPlayer: control.localPlayer,
    });
}

fn updateBorderFogOverlay(
    sim: Res<SimState>,
    control: Res<PlayerControlState>,
    mut overlay: ResMut<OverlayState>,
    mut images: ResMut<Assets<Image>>,
) {
    let world = &sim.0;
    let fogVersion = world.player(control.localPlayer).map_or(0, |p| p.fogVersion);
    if overlay.lastTerritoryVersion == world.territory.version
        && overlay.lastFogVersion == fogVersion
        && overlay.lastShowFog == control.showFog
        && overlay.lastLocalPlayer == control.localPlayer
    {
        return;
    }

    if let Some(mut img) = images.get_mut(&overlay.imageHandle) {
        *img = makeOverlayTexture(world, control.localPlayer, control.showFog);
    }
    overlay.lastTerritoryVersion = world.territory.version;
    overlay.lastFogVersion = fogVersion;
    overlay.lastShowFog = control.showFog;
    overlay.lastLocalPlayer = control.localPlayer;
}

fn makeOverlayTexture(world: &roeSim::SimWorld, localPlayer: PlayerId, showFog: bool) -> Image {
    let map = &world.map;
    let (w, h) = (map.width, map.height);
    let (texW, texH) = (w * OVERLAY_PIXELS_PER_TILE, h * OVERLAY_PIXELS_PER_TILE);
    let mut data = vec![0u8; (texW * texH * 4) as usize];

    // Pré-coleta cores de fronteira por PlayerId.
    let nationRgb = |pid: PlayerId| -> [f32; 3] {
        if let Some(n) = world.player(pid).and_then(|p| p.nation(&world.data)) {
            let (r, g, b) = n.primaryColorRgb;
            [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
        } else if pid == PlayerId(0) {
            [0.20, 0.55, 0.92]
        } else {
            [0.86, 0.22, 0.22]
        }
    };

    for ty in 0..h {
        for tx in 0..w {
            let fog = if showFog {
                world.fogAtTile(localPlayer, tx, ty)
            } else {
                FogState::Visible
            };
            let owner = world.territory.ownerAtTile(tx, ty);
            let leftDiff = tx > 0 && world.territory.ownerAtTile(tx - 1, ty) != owner;
            let rightDiff = tx + 1 < w && world.territory.ownerAtTile(tx + 1, ty) != owner;
            let upDiff = ty > 0 && world.territory.ownerAtTile(tx, ty - 1) != owner;
            let downDiff = ty + 1 < h && world.territory.ownerAtTile(tx, ty + 1) != owner;

            for subY in 0..OVERLAY_PIXELS_PER_TILE {
                for subX in 0..OVERLAY_PIXELS_PER_TILE {
                    let px = tx * OVERLAY_PIXELS_PER_TILE + subX;
                    let py = ty * OVERLAY_PIXELS_PER_TILE + subY;
                    let idx = ((py * texW + px) * 4) as usize;

                    let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);

                    // Fronteiras aparecem em qualquer tile já explorado ou visível.
                    if fog != FogState::Unexplored {
                        if let Some(pid) = owner {
                            let col = nationRgb(pid);
                            let onBorder = (subX == 0 && leftDiff)
                                || (subX == OVERLAY_PIXELS_PER_TILE - 1 && rightDiff)
                                || (subY == 0 && upDiff)
                                || (subY == OVERLAY_PIXELS_PER_TILE - 1 && downDiff);
                            if onBorder {
                                r = col[0];
                                g = col[1];
                                b = col[2];
                                a = 0.88;
                            } else {
                                r = col[0];
                                g = col[1];
                                b = col[2];
                                a = 0.09;
                            }
                        }
                    }

                    // Aplica véu da névoa de guerra por cima.
                    match fog {
                        FogState::Unexplored => {
                            r = 0.05;
                            g = 0.07;
                            b = 0.10;
                            a = 0.90;
                        }
                        FogState::Explored => {
                            // Mistura escurecimento de 42% preservando o traço da fronteira.
                            let fogA = 0.42;
                            let outA = fogA + a * (1.0 - fogA);
                            if outA > 0.001 {
                                r = (0.04 * fogA + r * a * (1.0 - fogA)) / outA;
                                g = (0.06 * fogA + g * a * (1.0 - fogA)) / outA;
                                b = (0.09 * fogA + b * a * (1.0 - fogA)) / outA;
                            }
                            a = outA;
                        }
                        FogState::Visible => {}
                    }

                    data[idx] = toByte(r);
                    data[idx + 1] = toByte(g);
                    data[idx + 2] = toByte(b);
                    data[idx + 3] = toByte(a);
                }
            }
        }
    }

    Image::new(
        Extent3d { width: texW, height: texH, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
}

fn spawnWater(
    mut commands: Commands,
    sim: Res<SimState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    spawnWaterMesh(&mut commands, &sim.0.map, &mut meshes, &mut images, &mut materials);
}

fn spawnWaterMesh(
    commands: &mut Commands,
    map: &MapGrid,
    meshes: &mut ResMut<Assets<Mesh>>,
    images: &mut ResMut<Assets<Image>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let seaLevel = MapGrid::SEA_LEVEL.to_num::<f32>();
    let texture = images.add(makeWaterTexture(map));

    commands.spawn((
        TerrainScoped,
        Mesh3d(meshes.add(Plane3d::default().mesh().size(map.width as f32, map.height as f32))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(texture),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.16,
            reflectance: 0.55,
            ..default()
        })),
        Transform::from_xyz(map.width as f32 / 2.0, seaLevel, map.height as f32 / 2.0),
        NotShadowCaster,
    ));
}

/// Popula florestas com árvores low-poly e encostas/montanhas com rochas em malhas combinadas.
fn spawnProps(
    mut commands: Commands,
    sim: Res<SimState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    spawnPropsMeshes(&mut commands, &sim.0.map, &mut meshes, &mut materials);
}

fn spawnPropsMeshes(
    commands: &mut Commands,
    map: &MapGrid,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let seed = (map.seed as u32) ^ ((map.seed >> 32) as u32);

    let mut trunks = LowPolyBuilder::default();
    let mut foliage = LowPolyBuilder::default();
    let mut rocks = LowPolyBuilder::default();

    for ty in 0..map.height {
        for tx in 0..map.width {
            let Some(kind) = map.tile(tx, ty) else { continue };
            match kind {
                TerrainType::Forest => {
                    let treeCount = 2 + (hashU32(tx, ty, seed ^ 0x11) % 2);
                    for i in 0..treeCount {
                        let s = seed.wrapping_add(i.wrapping_mul(0x9E37_79B9));
                        let ox = 0.18 + 0.64 * hash01(tx, ty, s ^ 0xA1);
                        let oz = 0.18 + 0.64 * hash01(tx, ty, s ^ 0xB2);
                        let wx = tx as f32 + ox;
                        let wz = ty as f32 + oz;
                        let wy = terrainHeightF32(map, wx, wz);
                        if wy < 0.12 {
                            continue;
                        }

                        let scale = 0.78 + 0.44 * hash01(tx, ty, s ^ 0xC3);
                        let yaw = TAU * hash01(tx, ty, s ^ 0xD4);
                        let isPine = hash01(tx, ty, s ^ 0xE5) < 0.58;
                        let shade = 0.85 + 0.30 * hash01(tx, ty, s ^ 0xF6);

                        let base = Vec3::new(wx, wy - 0.08, wz);
                        if isPine {
                            addPineTree(&mut trunks, &mut foliage, base, scale, yaw, shade);
                        } else {
                            addBroadleafTree(&mut trunks, &mut foliage, base, scale, yaw, shade);
                        }
                    }
                }
                TerrainType::Mountain => {
                    if hash01(tx, ty, seed ^ 0x3333) < 0.42 {
                        let wx = tx as f32 + 0.25 + 0.5 * hash01(tx, ty, seed ^ 0x4444);
                        let wz = ty as f32 + 0.25 + 0.5 * hash01(tx, ty, seed ^ 0x5555);
                        let wy = terrainHeightF32(map, wx, wz);
                        let scale = 0.24 + 0.32 * hash01(tx, ty, seed ^ 0x6666);
                        let yaw = TAU * hash01(tx, ty, seed ^ 0x7777);
                        let tone = 0.34 + 0.14 * hash01(tx, ty, seed ^ 0x8888);
                        let snow = smoothstep(2.6, 3.3, wy);
                        let color = [
                            lerp(tone * 1.04, 0.86, snow),
                            lerp(tone * 1.00, 0.88, snow),
                            lerp(tone * 0.96, 0.91, snow),
                            1.0,
                        ];
                        addRock(&mut rocks, Vec3::new(wx, wy - 0.06, wz), scale, yaw, color);
                    }
                }
                TerrainType::Hill => {
                    if hash01(tx, ty, seed ^ 0x9999) < 0.10 {
                        let wx = tx as f32 + 0.25 + 0.5 * hash01(tx, ty, seed ^ 0xAAAA);
                        let wz = ty as f32 + 0.25 + 0.5 * hash01(tx, ty, seed ^ 0xBBBB);
                        let wy = terrainHeightF32(map, wx, wz);
                        let scale = 0.16 + 0.18 * hash01(tx, ty, seed ^ 0xCCCC);
                        let yaw = TAU * hash01(tx, ty, seed ^ 0xDDDD);
                        let tone = 0.36 + 0.12 * hash01(tx, ty, seed ^ 0xEEEE);
                        addRock(
                            &mut rocks,
                            Vec3::new(wx, wy - 0.05, wz),
                            scale,
                            yaw,
                            [tone * 1.05, tone * 1.02, tone * 0.94, 1.0],
                        );
                    }
                }
                _ => {}
            }
        }
    }

    if !trunks.isEmpty() {
        commands.spawn((
            TerrainScoped,
            Mesh3d(meshes.add(trunks.build())),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 0.95,
                reflectance: 0.05,
                ..default()
            })),
        ));
    }
    if !foliage.isEmpty() {
        commands.spawn((
            TerrainScoped,
            Mesh3d(meshes.add(foliage.build())),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 0.88,
                reflectance: 0.08,
                ..default()
            })),
        ));
    }
    if !rocks.isEmpty() {
        commands.spawn((
            TerrainScoped,
            Mesh3d(meshes.add(rocks.build())),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 0.92,
                reflectance: 0.10,
                ..default()
            })),
        ));
    }
}

fn spawnLights(mut commands: Commands) {
    commands.insert_resource(ClearColor(Color::srgb(0.11, 0.16, 0.22)));
    commands.insert_resource(DirectionalLightShadowMap { size: 4096 });
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.66, 0.78, 0.98),
        brightness: 450.0,
        ..default()
    });

    let cascades = CascadeShadowConfigBuilder {
        num_cascades: 2,
        minimum_distance: 130.0,
        first_cascade_far_bound: 205.0,
        maximum_distance: 275.0,
        overlap_proportion: 0.15,
    }
    .build();

    commands.spawn((
        DirectionalLight {
            color: Color::srgb(1.0, 0.96, 0.88),
            illuminance: 13_500.0,
            shadow_maps_enabled: true,
            ..default()
        },
        cascades,
        Transform::from_xyz(-12.0, 16.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn buildTerrainMesh(map: &MapGrid, yOffset: f32) -> Mesh {
    let w = map.width;
    let h = map.height;
    let stride = w + 1;
    let vertexCount = (stride * (h + 1)) as usize;

    let mut positions = Vec::with_capacity(vertexCount);
    let mut normals = Vec::with_capacity(vertexCount);
    let mut uvs = Vec::with_capacity(vertexCount);

    for vy in 0..=h {
        for vx in 0..=w {
            let y = map.vertexHeight(vx, vy).to_num::<f32>() + yOffset;
            positions.push([vx as f32, y, vy as f32]);

            let hL = map.vertexHeight(vx.saturating_sub(1), vy).to_num::<f32>();
            let hR = map.vertexHeight((vx + 1).min(w), vy).to_num::<f32>();
            let hU = map.vertexHeight(vx, vy.saturating_sub(1)).to_num::<f32>();
            let hD = map.vertexHeight(vx, (vy + 1).min(h)).to_num::<f32>();
            let dx = if vx == 0 || vx == w { 1.0 } else { 2.0 };
            let dz = if vy == 0 || vy == h { 1.0 } else { 2.0 };
            let normal = Vec3::new((hL - hR) / dx, 1.0, (hU - hD) / dz).normalize();
            normals.push(normal.to_array());

            uvs.push([vx as f32 / w as f32, vy as f32 / h as f32]);
        }
    }

    let mut indices = Vec::with_capacity((w * h * 6) as usize);
    for ty in 0..h {
        for tx in 0..w {
            let tl = ty * stride + tx;
            let tr = tl + 1;
            let bl = (ty + 1) * stride + tx;
            let br = bl + 1;
            indices.extend_from_slice(&[tl, bl, tr, tr, bl, br]);
        }
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn makeTerrainTexture(map: &MapGrid) -> Image {
    let (texWidth, texHeight) = (map.width * PIXELS_PER_TILE, map.height * PIXELS_PER_TILE);
    let seed = (map.seed as u32) ^ ((map.seed >> 32) as u32);
    let mut data = Vec::with_capacity((texWidth * texHeight * 4) as usize);

    for py in 0..texHeight {
        for px in 0..texWidth {
            let wx = (px as f32 + 0.5) / PIXELS_PER_TILE as f32;
            let wz = (py as f32 + 0.5) / PIXELS_PER_TILE as f32;

            let h = terrainHeightF32(map, wx, wz);
            let hX = terrainHeightF32(map, wx + 0.35, wz) - terrainHeightF32(map, wx - 0.35, wz);
            let hZ = terrainHeightF32(map, wx, wz + 0.35) - terrainHeightF32(map, wx, wz - 0.35);
            let slope = (hX * hX + hZ * hZ).sqrt() / 0.7;

            let patches = valueNoise(wx / 9.0, wz / 9.0, seed ^ 11);
            let clumps = valueNoise(wx / 2.2, wz / 2.2, seed ^ 23);
            let grain = hash01(px, py, seed ^ 37);
            let dryness = valueNoise(wx / 14.0, wz / 14.0, seed ^ 51);
            let detail = 0.80 + 0.22 * patches + 0.10 * clumps + 0.07 * grain;

            let deepBed = Vec3::new(0.08, 0.20, 0.27);
            let shallowBed = Vec3::new(0.46, 0.52, 0.42);
            let wetSand = Vec3::new(0.56, 0.50, 0.35);
            let drySand = Vec3::new(0.76, 0.69, 0.49);
            let grass = Vec3::new(0.24 + 0.14 * dryness, 0.44 + 0.04 * dryness, 0.16);
            let forestFloor = Vec3::new(0.16 + 0.06 * clumps, 0.30 + 0.05 * patches, 0.11);
            let hillGrass = Vec3::new(0.35 + 0.10 * dryness, 0.43, 0.19);
            let rock = Vec3::new(0.42 + 0.07 * clumps, 0.40 + 0.06 * clumps, 0.38 + 0.05 * clumps);
            let snow = Vec3::new(0.88, 0.90, 0.93);

            let forestWeight = sampleForestWeight(map, wx, wz);

            let mut color = if h < 0.0 {
                let depthT = smoothstep(-0.95, -0.02, h);
                deepBed.lerp(shallowBed, depthT)
            } else {
                let sandToGrass = smoothstep(0.08, 0.28, h);
                let wetToDry = smoothstep(0.0, 0.09, h);
                let sandCol = wetSand.lerp(drySand, wetToDry);

                let lowland = grass.lerp(forestFloor, forestWeight * 0.82);
                let hillBlend = smoothstep(1.05, 1.85, h);
                let land = sandCol.lerp(lowland.lerp(hillGrass, hillBlend), sandToGrass);

                let rockFromHeight = smoothstep(1.85, 2.45, h);
                let rockFromSlope = smoothstep(0.52, 0.95, slope);
                let rockBlend = rockFromHeight.max(rockFromSlope * 0.9);
                let stony = land.lerp(rock, rockBlend);

                let snowLine = 2.60 + 0.30 * (patches - 0.5);
                let snowBlend = smoothstep(snowLine, snowLine + 0.45, h)
                    * (1.0 - smoothstep(0.95, 1.6, slope) * 0.55);
                stony.lerp(snow, snowBlend)
            };

            color *= detail;

            let onGrid = px % PIXELS_PER_TILE == 0 || py % PIXELS_PER_TILE == 0;
            if onGrid && h >= 0.05 && h < 2.0 && forestWeight < 0.45 {
                color *= 0.94;
            }

            data.extend_from_slice(&[toByte(color.x), toByte(color.y), toByte(color.z), 255]);
        }
    }

    Image::new(
        Extent3d { width: texWidth, height: texHeight, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

fn makeWaterTexture(map: &MapGrid) -> Image {
    let (texWidth, texHeight) = (
        map.width * WATER_PIXELS_PER_TILE,
        map.height * WATER_PIXELS_PER_TILE,
    );
    let seed = (map.seed as u32) ^ 0x7A31_95C2;
    let mut data = Vec::with_capacity((texWidth * texHeight * 4) as usize);

    for py in 0..texHeight {
        for px in 0..texWidth {
            let wx = (px as f32 + 0.5) / WATER_PIXELS_PER_TILE as f32;
            let wz = (py as f32 + 0.5) / WATER_PIXELS_PER_TILE as f32;

            let depth = MapGrid::SEA_LEVEL.to_num::<f32>() - terrainHeightF32(map, wx, wz);
            if depth <= -0.02 {
                data.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }

            let rippleA = valueNoise(wx * 0.85, wz * 1.45, seed ^ 19);
            let rippleB = valueNoise(wx * 2.4, wz * 2.1, seed ^ 71);
            let wave = 0.86 + 0.18 * rippleA + 0.10 * rippleB;

            let shallow = Vec3::new(0.11, 0.44, 0.52);
            let deep = Vec3::new(0.04, 0.17, 0.36);
            let depthT = smoothstep(0.0, 0.75, depth);
            let mut color = shallow.lerp(deep, depthT) * wave;

            let foam = (1.0 - smoothstep(0.0, 0.12, depth)) * (0.45 + 0.55 * rippleB);
            color = color.lerp(Vec3::new(0.84, 0.92, 0.95), foam * 0.65);

            let alpha = lerp(0.45, 0.90, smoothstep(-0.02, 0.55, depth));
            data.extend_from_slice(&[
                toByte(color.x),
                toByte(color.y),
                toByte(color.z),
                toByte(alpha),
            ]);
        }
    }

    Image::new(
        Extent3d { width: texWidth, height: texHeight, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

fn sampleForestWeight(map: &MapGrid, wx: f32, wz: f32) -> f32 {
    let gx = (wx - 0.5).clamp(0.0, (map.width - 1) as f32);
    let gz = (wz - 0.5).clamp(0.0, (map.height - 1) as f32);
    let x0 = gx.floor() as u32;
    let z0 = gz.floor() as u32;
    let x1 = (x0 + 1).min(map.width - 1);
    let z1 = (z0 + 1).min(map.height - 1);
    let fx = smooth(gx - x0 as f32);
    let fz = smooth(gz - z0 as f32);

    let isForest = |tx, tz| {
        if map.tile(tx, tz) == Some(TerrainType::Forest) { 1.0 } else { 0.0 }
    };
    let top = lerp(isForest(x0, z0), isForest(x1, z0), fx);
    let bottom = lerp(isForest(x0, z1), isForest(x1, z1), fx);
    lerp(top, bottom, fz)
}

fn addPineTree(
    trunks: &mut LowPolyBuilder,
    foliage: &mut LowPolyBuilder,
    base: Vec3,
    scale: f32,
    yaw: f32,
    shade: f32,
) {
    let trunkH = 0.42 * scale;
    let bark = [0.28 * shade, 0.18 * shade, 0.11 * shade, 1.0];
    trunks.addTaperedPrism(base, 0.08 * scale, 0.05 * scale, trunkH, 5, yaw, bark);

    let layers = [
        (0.30, 0.44, 0.56, 0.90),
        (0.64, 0.35, 0.50, 1.00),
        (0.95, 0.24, 0.44, 1.12),
    ];
    for (idx, (yOff, radius, height, tint)) in layers.into_iter().enumerate() {
        let s = shade * tint;
        let color = [0.11 * s, 0.29 * s, 0.14 * s, 1.0];
        foliage.addCone(
            base + Vec3::Y * (yOff * scale),
            radius * scale,
            height * scale,
            6,
            yaw + idx as f32 * 0.52,
            color,
        );
    }
}

fn addBroadleafTree(
    trunks: &mut LowPolyBuilder,
    foliage: &mut LowPolyBuilder,
    base: Vec3,
    scale: f32,
    yaw: f32,
    shade: f32,
) {
    let trunkH = 0.52 * scale;
    let bark = [0.32 * shade, 0.21 * shade, 0.13 * shade, 1.0];
    trunks.addTaperedPrism(base, 0.09 * scale, 0.055 * scale, trunkH, 5, yaw, bark);

    let mainColor = [0.18 * shade, 0.38 * shade, 0.12 * shade, 1.0];
    let topColor = [0.23 * shade, 0.45 * shade, 0.14 * shade, 1.0];

    let center = base + Vec3::Y * (0.78 * scale);
    foliage.addFacetedCrown(center, 0.38 * scale, 0.34 * scale, yaw, mainColor);

    let side = Vec3::new(yaw.cos() * 0.14 * scale, 0.16 * scale, yaw.sin() * 0.14 * scale);
    foliage.addFacetedCrown(center + side, 0.26 * scale, 0.25 * scale, yaw + 1.1, topColor);
}

fn addRock(builder: &mut LowPolyBuilder, base: Vec3, scale: f32, yaw: f32, color: [f32; 4]) {
    let center = base + Vec3::Y * (scale * 0.42);
    builder.addFacetedCrown(center, scale, scale * 0.65, yaw, color);
}

/// Cor linear da civilização de um jogador (usada por unidades e construções).
pub fn playerColors(world: &roeSim::SimWorld, pid: PlayerId) -> ([f32; 4], [f32; 4]) {
    if let Some(n) = world.player(pid).and_then(|p| p.nation(&world.data)) {
        (rgbToLinear(n.primaryColorRgb), rgbToLinear(n.accentColorRgb))
    } else if pid == PlayerId(0) {
        (rgbToLinear((44, 118, 212)), rgbToLinear((230, 194, 72)))
    } else {
        (rgbToLinear((196, 48, 48)), rgbToLinear((224, 184, 72)))
    }
}

fn toByte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0) as u8
}

fn valueNoise(x: f32, y: f32, seed: u32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (smooth(x - x0), smooth(y - y0));
    let (ix, iy) = (x0 as i32 as u32, y0 as i32 as u32);
    let top = lerp(hash01(ix, iy, seed), hash01(ix.wrapping_add(1), iy, seed), fx);
    let bottom = lerp(
        hash01(ix, iy.wrapping_add(1), seed),
        hash01(ix.wrapping_add(1), iy.wrapping_add(1), seed),
        fx,
    );
    lerp(top, bottom, fy)
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    smooth(t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn hashU32(x: u32, y: u32, seed: u32) -> u32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77) ^ seed.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    h
}

fn hash01(x: u32, y: u32, seed: u32) -> f32 {
    (hashU32(x, y, seed) >> 8) as f32 / (1u32 << 24) as f32
}
