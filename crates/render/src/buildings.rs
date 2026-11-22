use bevy::light::NotShadowCaster;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use roeSim::{BuildingId, FogState, PlayerId};

use crate::bridge::{SimState, WorldResetCounter};
use crate::input::PlayerControlState;
use crate::models::buildBuildingMesh;
use crate::terrain::playerColors;
use crate::terrainHeightF32;

#[derive(Component)]
pub struct BuildingView {
    pub id: BuildingId,
    pub meshKey: BuildingMeshKey,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct BuildingMeshKey {
    kind: String,
    wonderId: Option<String>,
    owner: PlayerId,
    age: u8,
    complete: bool,
}

#[derive(Component)]
struct BuildingSelectRing(BuildingId);

#[derive(Component)]
struct BuildingHpBg(BuildingId);

#[derive(Component)]
struct BuildingHpFill(BuildingId);

#[derive(Resource, Default)]
struct BuildingViews(HashMap<BuildingId, BuildingVisualBundle>);

struct BuildingVisualBundle {
    root: Entity,
    ring: Entity,
    hpBg: Entity,
    hpFill: Entity,
}

#[derive(Resource, Default)]
struct BuildingMeshCache {
    meshes: HashMap<BuildingMeshKey, Handle<Mesh>>,
    sharedMat: Option<Handle<StandardMaterial>>,
    ringMesh: Option<Handle<Mesh>>,
    ringMat: Option<Handle<StandardMaterial>>,
    hpMesh: Option<Handle<Mesh>>,
    hpBgMat: Option<Handle<StandardMaterial>>,
    hpFillMat: Option<Handle<StandardMaterial>>,
}

pub struct BuildingsPlugin;

impl Plugin for BuildingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BuildingViews>()
            .init_resource::<BuildingMeshCache>()
            .add_systems(Startup, initBuildingAssets)
            .add_systems(Update, (syncBuildingViews, updateBuildingVisuals).chain());
    }
}

fn initBuildingAssets(
    mut cache: ResMut<BuildingMeshCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    cache.sharedMat = Some(materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.88,
        reflectance: 0.10,
        ..default()
    }));
    cache.ringMesh = Some(meshes.add(Annulus::new(0.88, 1.04)));
    cache.ringMat = Some(materials.add(StandardMaterial {
        base_color: Color::srgba(0.22, 0.90, 0.95, 0.90),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    }));
    cache.hpMesh = Some(meshes.add(Cuboid::new(1.80, 0.12, 0.12)));
    cache.hpBgMat = Some(materials.add(StandardMaterial {
        base_color: Color::srgb(0.10, 0.10, 0.12),
        unlit: true,
        ..default()
    }));
    cache.hpFillMat = Some(materials.add(StandardMaterial {
        base_color: Color::srgb(0.95, 0.78, 0.18),
        unlit: true,
        ..default()
    }));
}

fn syncBuildingViews(
    mut commands: Commands,
    sim: Res<SimState>,
    reset: Res<WorldResetCounter>,
    mut lastReset: Local<u64>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: ResMut<BuildingMeshCache>,
    mut views: ResMut<BuildingViews>,
    mut bQuery: Query<(&mut BuildingView, &mut Mesh3d)>,
) {
    let resetHappened = *lastReset != reset.0;
    if resetHappened {
        *lastReset = reset.0;
        for (_, bundle) in views.0.drain() {
            commands.entity(bundle.root).despawn();
            commands.entity(bundle.ring).despawn();
            commands.entity(bundle.hpBg).despawn();
            commands.entity(bundle.hpFill).despawn();
        }
        cache.meshes.clear();
    }
    if !sim.is_changed() && !resetHappened {
        return;
    }
    let world = &sim.0;
    let Some(sharedMat) = cache.sharedMat.clone() else { return };
    let Some(ringMesh) = cache.ringMesh.clone() else { return };
    let Some(ringMat) = cache.ringMat.clone() else { return };
    let Some(hpMesh) = cache.hpMesh.clone() else { return };
    let Some(hpBgMat) = cache.hpBgMat.clone() else { return };
    let Some(hpFillMat) = cache.hpFillMat.clone() else { return };

    for b in &world.buildings {
        let age = world.player(b.owner).map_or(0, |p| p.age);
        let key = BuildingMeshKey {
            kind: b.kind.clone(),
            wonderId: b.wonderId.clone(),
            owner: b.owner,
            age,
            complete: b.isComplete(),
        };
        let meshHandle = cache
            .meshes
            .entry(key.clone())
            .or_insert_with(|| {
                let (primary, accent) = playerColors(world, b.owner);
                meshes.add(buildBuildingMesh(
                    &b.kind,
                    b.wonderId.as_deref(),
                    b.footprint,
                    age,
                    b.isComplete(),
                    primary,
                    accent,
                ))
            })
            .clone();

        if let Some(bundle) = views.0.get(&b.id) {
            if let Ok((mut bv, mut mesh3d)) = bQuery.get_mut(bundle.root) {
                if bv.meshKey != key {
                    bv.meshKey = key;
                    mesh3d.0 = meshHandle;
                }
            }
            continue;
        }

        let (cx, cz) = b.centerPos().toF32();
        let cy = terrainHeightF32(&world.map, cx, cz);
        let pos = Vec3::new(cx, cy, cz);

        let root = commands
            .spawn((
                BuildingView { id: b.id, meshKey: key },
                Mesh3d(meshHandle),
                MeshMaterial3d(sharedMat.clone()),
                Transform::from_translation(pos),
            ))
            .id();

        let ringScale = b.footprint as f32 * 0.65;
        let ring = commands
            .spawn((
                BuildingSelectRing(b.id),
                Mesh3d(ringMesh.clone()),
                MeshMaterial3d(ringMat.clone()),
                Transform::from_translation(pos + Vec3::Y * 0.08)
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                    .with_scale(Vec3::splat(ringScale)),
                Visibility::Hidden,
                NotShadowCaster,
            ))
            .id();

        let barY = if b.footprint >= 3 { 2.85 } else { 2.15 };
        let barRot = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);
        let hpBg = commands
            .spawn((
                BuildingHpBg(b.id),
                Mesh3d(hpMesh.clone()),
                MeshMaterial3d(hpBgMat.clone()),
                Transform::from_translation(pos + Vec3::Y * barY).with_rotation(barRot),
                Visibility::Hidden,
                NotShadowCaster,
            ))
            .id();
        let hpFill = commands
            .spawn((
                BuildingHpFill(b.id),
                Mesh3d(hpMesh.clone()),
                MeshMaterial3d(hpFillMat.clone()),
                Transform::from_translation(pos + Vec3::Y * (barY + 0.03)).with_rotation(barRot),
                Visibility::Hidden,
                NotShadowCaster,
            ))
            .id();

        views.0.insert(b.id, BuildingVisualBundle { root, ring, hpBg, hpFill });
    }

    views.0.retain(|id, bundle| {
        let alive = world.building(*id).is_some();
        if !alive {
            commands.entity(bundle.root).despawn();
            commands.entity(bundle.ring).despawn();
            commands.entity(bundle.hpBg).despawn();
            commands.entity(bundle.hpFill).despawn();
        }
        alive
    });
}

fn updateBuildingVisuals(
    sim: Res<SimState>,
    control: Res<PlayerControlState>,
    mut bQuery: Query<(&BuildingView, &mut Visibility)>,
    mut ringQuery: Query<
        (&BuildingSelectRing, &mut Visibility),
        (Without<BuildingView>, Without<BuildingHpBg>, Without<BuildingHpFill>),
    >,
    mut hpBgQuery: Query<
        (&BuildingHpBg, &mut Visibility),
        (Without<BuildingView>, Without<BuildingSelectRing>, Without<BuildingHpFill>),
    >,
    mut hpFillQuery: Query<
        (&BuildingHpFill, &mut Transform, &mut Visibility),
        (Without<BuildingView>, Without<BuildingSelectRing>, Without<BuildingHpBg>),
    >,
) {
    let world = &sim.0;

    let isBuildingVisible = |id: BuildingId| -> bool {
        let Some(b) = world.building(id) else { return false };
        if b.owner == control.localPlayer || !control.showFog {
            return true;
        }
        let (tx, ty) = b.centerTile();
        world.fogAtTile(control.localPlayer, tx, ty) != FogState::Unexplored
    };

    for (bv, mut vis) in &mut bQuery {
        *vis = if isBuildingVisible(bv.id) { Visibility::Visible } else { Visibility::Hidden };
    }

    for (ring, mut vis) in &mut ringQuery {
        let selected = control.selectedBuilding == Some(ring.0);
        *vis = if selected && isBuildingVisible(ring.0) {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }

    for (bg, mut vis) in &mut hpBgQuery {
        let Some(b) = world.building(bg.0) else { continue };
        let show = isBuildingVisible(bg.0)
            && (!b.isComplete() || b.hp < b.maxHp || control.selectedBuilding == Some(b.id));
        *vis = if show { Visibility::Visible } else { Visibility::Hidden };
    }

    for (fill, mut transform, mut vis) in &mut hpFillQuery {
        let Some(b) = world.building(fill.0) else { continue };
        let show = isBuildingVisible(fill.0)
            && (!b.isComplete() || b.hp < b.maxHp || control.selectedBuilding == Some(b.id));
        *vis = if show { Visibility::Visible } else { Visibility::Hidden };
        let ratio = if !b.isComplete() {
            (b.buildProgress as f32 / b.buildRequired.max(1) as f32).clamp(0.05, 1.0)
        } else {
            (b.hp as f32 / b.maxHp.max(1) as f32).clamp(0.03, 1.0)
        };
        transform.scale = Vec3::new(ratio, 1.05, 1.05);
    }
}
