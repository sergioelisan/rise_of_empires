use bevy::light::NotShadowCaster;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use roeSim::{PlayerId, UnitId, WorkAnim};

use crate::bridge::{SimState, WorldResetCounter};
use crate::input::PlayerControlState;
use crate::models::buildUnitMesh;
use crate::terrain::playerColors;
use crate::{simToWorld, terrainHeightF32};

/// Liga uma entidade visual à unidade correspondente na simulação.
#[derive(Component)]
pub struct UnitView {
    pub id: UnitId,
    pub meshKey: (String, PlayerId, u8),
}

#[derive(Component)]
struct SelectionRing(UnitId);

#[derive(Component)]
struct AttritionRing(UnitId);

#[derive(Component)]
struct CarriedLog(UnitId);

#[derive(Component)]
struct UnitHpFill(UnitId);

#[derive(Component)]
struct UnitHpBg(UnitId);

#[derive(Component)]
struct CombatTracer {
    from: Vec3,
    to: Vec3,
    age: f32,
    duration: f32,
}

#[derive(Component)]
struct WorkParticle {
    velocity: Vec3,
    age: f32,
    duration: f32,
}

#[derive(Resource, Default)]
struct UnitViews(HashMap<UnitId, UnitVisualBundle>);

struct UnitVisualBundle {
    root: Entity,
    ring: Entity,
    attrition: Entity,
    carriedLog: Entity,
    hpBg: Entity,
    hpFill: Entity,
}

#[derive(Resource, Default)]
struct UnitMeshCache {
    meshes: HashMap<(String, PlayerId, u8), Handle<Mesh>>,
    sharedMaterial: Option<Handle<StandardMaterial>>,
    ringMesh: Option<Handle<Mesh>>,
    selectMat: Option<Handle<StandardMaterial>>,
    attritionMat: Option<Handle<StandardMaterial>>,
    logMesh: Option<Handle<Mesh>>,
    logMat: Option<Handle<StandardMaterial>>,
    hpBgMesh: Option<Handle<Mesh>>,
    hpBgMat: Option<Handle<StandardMaterial>>,
    hpFillMat: Option<Handle<StandardMaterial>>,
    tracerMesh: Option<Handle<Mesh>>,
    arrowMat: Option<Handle<StandardMaterial>>,
    siegeMat: Option<Handle<StandardMaterial>>,
    chipMesh: Option<Handle<Mesh>>,
    woodChipMat: Option<Handle<StandardMaterial>>,
    stoneSparkMat: Option<Handle<StandardMaterial>>,
    wheatChipMat: Option<Handle<StandardMaterial>>,
    lastEventTick: u64,
    lastParticleTick: u64,
}

pub struct UnitsPlugin;

impl Plugin for UnitsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<UnitViews>()
            .init_resource::<UnitMeshCache>()
            .add_systems(Startup, initUnitAssets)
            .add_systems(
                Update,
                (
                    syncUnitViews,
                    interpolateUnits,
                    spawnCombatTracers,
                    updateCombatTracers,
                    spawnWorkParticles,
                    updateWorkParticles,
                )
                    .chain(),
            );
    }
}

fn initUnitAssets(
    mut cache: ResMut<UnitMeshCache>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    cache.sharedMaterial = Some(materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.78,
        reflectance: 0.14,
        ..default()
    }));
    cache.ringMesh = Some(meshes.add(Annulus::new(0.36, 0.48)));
    cache.selectMat = Some(materials.add(StandardMaterial {
        base_color: Color::srgba(0.16, 0.92, 0.34, 0.90),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    }));
    cache.attritionMat = Some(materials.add(StandardMaterial {
        base_color: Color::srgba(0.95, 0.18, 0.16, 0.85),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    }));
    cache.logMesh = Some(meshes.add(Cuboid::new(0.42, 0.12, 0.16)));
    cache.logMat = Some(materials.add(StandardMaterial {
        base_color: Color::srgb(0.42, 0.25, 0.12),
        perceptual_roughness: 0.92,
        ..default()
    }));
    cache.hpBgMesh = Some(meshes.add(Cuboid::new(0.70, 0.08, 0.08)));
    cache.hpBgMat = Some(materials.add(StandardMaterial {
        base_color: Color::srgb(0.12, 0.12, 0.14),
        unlit: true,
        ..default()
    }));
    cache.hpFillMat = Some(materials.add(StandardMaterial {
        base_color: Color::srgb(0.20, 0.86, 0.32),
        unlit: true,
        ..default()
    }));
    cache.tracerMesh = Some(meshes.add(Sphere::new(0.08)));
    cache.arrowMat = Some(materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.88, 0.42),
        unlit: true,
        ..default()
    }));
    cache.siegeMat = Some(materials.add(StandardMaterial {
        base_color: Color::srgb(0.98, 0.42, 0.12),
        unlit: true,
        ..default()
    }));
    cache.chipMesh = Some(meshes.add(Cuboid::new(0.07, 0.07, 0.07)));
    cache.woodChipMat = Some(materials.add(StandardMaterial {
        base_color: Color::srgb(0.56, 0.36, 0.16),
        unlit: true,
        ..default()
    }));
    cache.stoneSparkMat = Some(materials.add(StandardMaterial {
        base_color: Color::srgb(0.95, 0.86, 0.46),
        unlit: true,
        ..default()
    }));
    cache.wheatChipMat = Some(materials.add(StandardMaterial {
        base_color: Color::srgb(0.88, 0.74, 0.24),
        unlit: true,
        ..default()
    }));
}

fn syncUnitViews(
    mut commands: Commands,
    sim: Res<SimState>,
    reset: Res<WorldResetCounter>,
    mut lastReset: Local<u64>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut cache: ResMut<UnitMeshCache>,
    mut views: ResMut<UnitViews>,
    mut unitQuery: Query<(&mut UnitView, &mut Mesh3d)>,
) {
    let resetHappened = *lastReset != reset.0;
    if resetHappened {
        *lastReset = reset.0;
        for (_, bundle) in views.0.drain() {
            commands.entity(bundle.root).despawn();
            commands.entity(bundle.ring).despawn();
            commands.entity(bundle.attrition).despawn();
            commands.entity(bundle.carriedLog).despawn();
            commands.entity(bundle.hpBg).despawn();
            commands.entity(bundle.hpFill).despawn();
        }
        cache.meshes.clear();
        cache.lastEventTick = 0;
        cache.lastParticleTick = 0;
    }
    if !sim.is_changed() && !resetHappened {
        return;
    }
    let world = &sim.0;
    let Some(sharedMat) = cache.sharedMaterial.clone() else { return };
    let Some(ringMesh) = cache.ringMesh.clone() else { return };
    let Some(selectMat) = cache.selectMat.clone() else { return };
    let Some(attritionMat) = cache.attritionMat.clone() else { return };
    let Some(logMesh) = cache.logMesh.clone() else { return };
    let Some(logMat) = cache.logMat.clone() else { return };
    let Some(hpBgMesh) = cache.hpBgMesh.clone() else { return };
    let Some(hpBgMat) = cache.hpBgMat.clone() else { return };
    let Some(hpFillMat) = cache.hpFillMat.clone() else { return };

    for unit in &world.units {
        let key = (unit.kind.clone(), unit.owner, unit.eraTier);
        let unitMesh = cache
            .meshes
            .entry(key.clone())
            .or_insert_with(|| {
                let (primary, accent) = playerColors(world, unit.owner);
                meshes.add(buildUnitMesh(unit.class, &unit.kind, unit.eraTier, primary, accent))
            })
            .clone();

        if let Some(bundle) = views.0.get(&unit.id) {
            if let Ok((mut uv, mut mesh3d)) = unitQuery.get_mut(bundle.root) {
                if uv.meshKey != key {
                    uv.meshKey = key;
                    mesh3d.0 = unitMesh;
                }
            }
            continue;
        }

        let pos = simToWorld(&world.map, unit.pos);
        let root = commands
            .spawn((
                UnitView { id: unit.id, meshKey: key },
                Mesh3d(unitMesh),
                MeshMaterial3d(sharedMat.clone()),
                Transform::from_translation(pos),
            ))
            .id();

        let ring = commands
            .spawn((
                SelectionRing(unit.id),
                Mesh3d(ringMesh.clone()),
                MeshMaterial3d(selectMat.clone()),
                Transform::from_translation(pos + Vec3::Y * 0.06)
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                Visibility::Hidden,
                NotShadowCaster,
            ))
            .id();

        let attrition = commands
            .spawn((
                AttritionRing(unit.id),
                Mesh3d(ringMesh.clone()),
                MeshMaterial3d(attritionMat.clone()),
                Transform::from_translation(pos + Vec3::Y * 0.07)
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
                    .with_scale(Vec3::splat(1.22)),
                Visibility::Hidden,
                NotShadowCaster,
            ))
            .id();

        let carriedLog = commands
            .spawn((
                CarriedLog(unit.id),
                Mesh3d(logMesh.clone()),
                MeshMaterial3d(logMat.clone()),
                Transform::from_translation(pos + Vec3::Y * 0.62),
                Visibility::Hidden,
                NotShadowCaster,
            ))
            .id();

        let hpBg = commands
            .spawn((
                UnitHpBg(unit.id),
                Mesh3d(hpBgMesh.clone()),
                MeshMaterial3d(hpBgMat.clone()),
                Transform::from_translation(pos + Vec3::Y * 1.45),
                Visibility::Hidden,
                NotShadowCaster,
            ))
            .id();

        let hpFill = commands
            .spawn((
                UnitHpFill(unit.id),
                Mesh3d(hpBgMesh.clone()),
                MeshMaterial3d(hpFillMat.clone()),
                Transform::from_translation(pos + Vec3::Y * 1.47),
                Visibility::Hidden,
                NotShadowCaster,
            ))
            .id();

        views
            .0
            .insert(unit.id, UnitVisualBundle { root, ring, attrition, carriedLog, hpBg, hpFill });
    }

    views.0.retain(|id, bundle| {
        let alive = world.unit(*id).is_some();
        if !alive {
            commands.entity(bundle.root).despawn();
            commands.entity(bundle.ring).despawn();
            commands.entity(bundle.attrition).despawn();
            commands.entity(bundle.carriedLog).despawn();
            commands.entity(bundle.hpBg).despawn();
            commands.entity(bundle.hpFill).despawn();
        }
        alive
    });
}

fn interpolateUnits(
    sim: Res<SimState>,
    control: Res<PlayerControlState>,
    time: Res<Time>,
    fixedTime: Res<Time<Fixed>>,
    mut unitQuery: Query<(&UnitView, &mut Transform, &mut Visibility)>,
    mut ringQuery: Query<
        (&SelectionRing, &mut Transform, &mut Visibility),
        (
            Without<UnitView>,
            Without<AttritionRing>,
            Without<CarriedLog>,
            Without<UnitHpBg>,
            Without<UnitHpFill>,
        ),
    >,
    mut attrQuery: Query<
        (&AttritionRing, &mut Transform, &mut Visibility),
        (
            Without<UnitView>,
            Without<SelectionRing>,
            Without<CarriedLog>,
            Without<UnitHpBg>,
            Without<UnitHpFill>,
        ),
    >,
    mut logQuery: Query<
        (&CarriedLog, &mut Transform, &mut Visibility),
        (
            Without<UnitView>,
            Without<SelectionRing>,
            Without<AttritionRing>,
            Without<UnitHpBg>,
            Without<UnitHpFill>,
        ),
    >,
    mut hpBgQuery: Query<
        (&UnitHpBg, &mut Transform, &mut Visibility),
        (
            Without<UnitView>,
            Without<SelectionRing>,
            Without<AttritionRing>,
            Without<CarriedLog>,
            Without<UnitHpFill>,
        ),
    >,
    mut hpFillQuery: Query<
        (&UnitHpFill, &mut Transform, &mut Visibility),
        (
            Without<UnitView>,
            Without<SelectionRing>,
            Without<AttritionRing>,
            Without<CarriedLog>,
            Without<UnitHpBg>,
        ),
    >,
) {
    let alpha = fixedTime.overstep_fraction();
    let elapsed = time.elapsed_secs();
    let world = &sim.0;
    let map = &world.map;

    let interpPos = |id: UnitId| -> Option<(Vec3, bool)> {
        let unit = world.unit(id)?;
        let visible = unit.owner == control.localPlayer
            || !control.showFog
            || world.isPosVisibleTo(control.localPlayer, unit.pos);
        let (x0, z0) = unit.prevPos.toF32();
        let (x1, z1) = unit.pos.toF32();
        let x = x0 + (x1 - x0) * alpha;
        let z = z0 + (z1 - z0) * alpha;
        let y = terrainHeightF32(map, x, z);
        Some((Vec3::new(x, y, z), visible))
    };

    for (view, mut transform, mut vis) in &mut unitQuery {
        let Some((pos, visible)) = interpPos(view.id) else { continue };
        *vis = if visible { Visibility::Visible } else { Visibility::Hidden };

        let mut animOffset = Vec3::ZERO;
        let mut baseYaw = 0.0f32;
        let mut extraYaw = 0.0f32;
        let mut tiltForward = 0.0f32;
        let mut swaySide = 0.0f32;

        if let Some(unit) = world.unit(view.id) {
            let (fx, fz) = unit.facing.toF32();
            if fx * fx + fz * fz > 0.001 {
                baseYaw = (-fz).atan2(fx);
            }

            let phase = view.id.0 as f32 * 1.73;
            let moving = unit.pos != unit.prevPos;

            match unit.workAnim {
                WorkAnim::Chopping => {
                    // Movimento rítmico de machado: ergue devagar e golpeia com firmeza.
                    let s = (elapsed * 7.5 + phase).sin();
                    let strike = s.max(0.0).powf(2.0);
                    tiltForward = -0.14 + strike * 0.56;
                    extraYaw = -0.18 + strike * 0.38;
                    animOffset.y = (1.0 - strike) * 0.05;
                }
                WorkAnim::Mining => {
                    let s = (elapsed * 6.8 + phase).sin();
                    let strike = s.max(0.0).powf(2.2);
                    tiltForward = -0.18 + strike * 0.62;
                    animOffset.y = (1.0 - strike) * 0.06;
                }
                WorkAnim::Building => {
                    // Martelada rápida na obra.
                    let s = (elapsed * 10.5 + phase).sin();
                    let strike = s.max(0.0).powf(1.8);
                    tiltForward = strike * 0.42;
                    extraYaw = (elapsed * 5.2 + phase).sin() * 0.16;
                    animOffset.y = strike * 0.04;
                }
                WorkAnim::Farming => {
                    // Curvar e trabalhar nas fileiras de trigo dentro da fazenda.
                    let wave = (elapsed * 5.2 + phase).sin();
                    tiltForward = 0.22 + 0.24 * (0.5 + 0.5 * wave);
                    swaySide = (elapsed * 2.6 + phase).sin() * 0.14;
                    animOffset.y = -0.04 * (0.5 + 0.5 * wave);
                }
                WorkAnim::Market => {
                    // Mercador negociando e movimentando mercadorias dentro do pátio do mercado.
                    let talk = (elapsed * 6.4 + phase).sin();
                    extraYaw = (elapsed * 3.1 + phase).sin() * 0.34;
                    tiltForward = 0.10 + 0.14 * talk.abs();
                    animOffset.y = talk.abs() * 0.055;
                }
                WorkAnim::Working => {
                    let wave = (elapsed * 4.5 + phase).sin();
                    tiltForward = 0.08 + 0.10 * wave;
                    extraYaw = (elapsed * 2.2 + phase).sin() * 0.18;
                }
                WorkAnim::None => {
                    if moving {
                        // Passo animado ao caminhar/cavalgar.
                        let stride = (elapsed * 13.0 + phase).sin();
                        animOffset.y = stride.abs() * 0.065;
                        swaySide = stride * 0.065;
                    } else if unit.attackCooldown > unit.attackCooldownMax.saturating_sub(5)
                        && unit.attackCooldownMax > 0
                    {
                        // Investida rápida ao atacar.
                        tiltForward = 0.34;
                    }
                }
            }
        }

        transform.translation = pos + animOffset;
        // Modelo olha para +X: inclinar para frente = rotação negativa em Z local; balanço lateral = X local.
        transform.rotation = Quat::from_rotation_y(baseYaw + extraYaw)
            * Quat::from_rotation_z(-tiltForward)
            * Quat::from_rotation_x(swaySide);
    }

    for (ring, mut transform, mut vis) in &mut ringQuery {
        let Some((pos, visible)) = interpPos(ring.0) else { continue };
        let selected = control.selectedUnits.contains(&ring.0);
        *vis = if visible && selected { Visibility::Visible } else { Visibility::Hidden };
        transform.translation = pos + Vec3::Y * 0.06;
    }

    for (attr, mut transform, mut vis) in &mut attrQuery {
        let Some((pos, visible)) = interpPos(attr.0) else { continue };
        let inEnemy = world.unit(attr.0).is_some_and(|u| u.inEnemyTerritory);
        *vis = if visible && inEnemy { Visibility::Visible } else { Visibility::Hidden };
        transform.translation = pos + Vec3::Y * 0.07;
    }

    for (log, mut transform, mut vis) in &mut logQuery {
        let Some((pos, visible)) = interpPos(log.0) else { continue };
        let Some(unit) = world.unit(log.0) else { continue };
        let carrying = visible && unit.returningToDropoff;
        *vis = if carrying { Visibility::Visible } else { Visibility::Hidden };
        let (fx, fz) = unit.facing.toF32();
        let yaw = if fx * fx + fz * fz > 0.001 { (-fz).atan2(fx) } else { 0.0 };
        let forward = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
        transform.translation = pos + Vec3::Y * 0.56 + forward * 0.22;
        transform.rotation = Quat::from_rotation_y(yaw + std::f32::consts::FRAC_PI_2);
    }

    let barRot = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);
    for (bg, mut transform, mut vis) in &mut hpBgQuery {
        let Some((pos, visible)) = interpPos(bg.0) else { continue };
        let showBar = visible
            && world
                .unit(bg.0)
                .is_some_and(|u| u.hp < u.maxHp || control.selectedUnits.contains(&u.id));
        *vis = if showBar { Visibility::Visible } else { Visibility::Hidden };
        transform.translation = pos + Vec3::Y * 1.45;
        transform.rotation = barRot;
    }

    for (fill, mut transform, mut vis) in &mut hpFillQuery {
        let Some((pos, visible)) = interpPos(fill.0) else { continue };
        let Some(unit) = world.unit(fill.0) else { continue };
        let showBar = visible && (unit.hp < unit.maxHp || control.selectedUnits.contains(&unit.id));
        *vis = if showBar { Visibility::Visible } else { Visibility::Hidden };
        let ratio = (unit.hp as f32 / unit.maxHp.max(1) as f32).clamp(0.05, 1.0);
        transform.translation = pos + Vec3::Y * 1.48;
        transform.rotation = barRot;
        transform.scale = Vec3::new(ratio, 1.05, 1.05);
    }
}

/// Emite pequenas lascas/faíscas periódicas quando cidadãos estão cortando, minerando, construindo ou colhendo.
fn spawnWorkParticles(
    mut commands: Commands,
    sim: Res<SimState>,
    control: Res<PlayerControlState>,
    mut cache: ResMut<UnitMeshCache>,
) {
    let world = &sim.0;
    if world.tick < cache.lastParticleTick + 9 {
        return;
    }
    cache.lastParticleTick = world.tick;

    let Some(chipMesh) = cache.chipMesh.clone() else { return };
    let Some(woodMat) = cache.woodChipMat.clone() else { return };
    let Some(stoneMat) = cache.stoneSparkMat.clone() else { return };
    let Some(wheatMat) = cache.wheatChipMat.clone() else { return };

    for u in &world.units {
        if u.workAnim == WorkAnim::None || u.workAnim == WorkAnim::Working {
            continue;
        }
        if control.showFog
            && u.owner != control.localPlayer
            && !world.isPosVisibleTo(control.localPlayer, u.pos)
        {
            continue;
        }
        let mat = match u.workAnim {
            WorkAnim::Chopping => woodMat.clone(),
            WorkAnim::Mining | WorkAnim::Building => stoneMat.clone(),
            WorkAnim::Farming | WorkAnim::Market => wheatMat.clone(),
            _ => continue,
        };
        let base = simToWorld(&world.map, u.pos);
        let (fx, fz) = u.facing.toF32();
        let fwd = Vec3::new(fx, 0.0, fz).normalize_or_zero();
        let origin = base + fwd * 0.36 + Vec3::Y * 0.45;

        for k in 0..2u32 {
            let angle = ((world.tick as u32).wrapping_add(u.id.0 * 31 + k * 97) % 360) as f32
                * std::f32::consts::TAU
                / 360.0;
            let vel = Vec3::new(angle.cos() * 0.9, 1.8 + k as f32 * 0.4, angle.sin() * 0.9);
            commands.spawn((
                WorkParticle { velocity: vel, age: 0.0, duration: 0.36 },
                Mesh3d(chipMesh.clone()),
                MeshMaterial3d(mat.clone()),
                Transform::from_translation(origin),
                NotShadowCaster,
            ));
        }
    }
}

fn updateWorkParticles(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut WorkParticle, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (entity, mut p, mut transform) in &mut query {
        p.age += dt;
        p.velocity.y -= 7.5 * dt;
        transform.translation += p.velocity * dt;
        let shrink = (1.0 - p.age / p.duration).clamp(0.05, 1.0);
        transform.scale = Vec3::splat(shrink);
        if p.age >= p.duration {
            commands.entity(entity).despawn();
        }
    }
}

fn spawnCombatTracers(
    mut commands: Commands,
    sim: Res<SimState>,
    control: Res<PlayerControlState>,
    mut cache: ResMut<UnitMeshCache>,
) {
    let world = &sim.0;
    let Some(mesh) = cache.tracerMesh.clone() else { return };
    let Some(arrowMat) = cache.arrowMat.clone() else { return };
    let Some(siegeMat) = cache.siegeMat.clone() else { return };

    let mut maxSeen = cache.lastEventTick;
    for ev in &world.combatEvents {
        if ev.tick <= cache.lastEventTick || !ev.isRanged {
            continue;
        }
        maxSeen = maxSeen.max(ev.tick);
        if control.showFog
            && !world.isPosVisibleTo(control.localPlayer, ev.from)
            && !world.isPosVisibleTo(control.localPlayer, ev.to)
        {
            continue;
        }
        let from = simToWorld(&world.map, ev.from) + Vec3::Y * 0.85;
        let to = simToWorld(&world.map, ev.to) + Vec3::Y * 0.65;
        let scale = if ev.isSiege { 2.2 } else { 1.0 };
        commands.spawn((
            CombatTracer { from, to, age: 0.0, duration: if ev.isSiege { 0.45 } else { 0.28 } },
            Mesh3d(mesh.clone()),
            MeshMaterial3d(if ev.isSiege { siegeMat.clone() } else { arrowMat.clone() }),
            Transform::from_translation(from).with_scale(Vec3::splat(scale)),
            NotShadowCaster,
        ));
    }
    cache.lastEventTick = maxSeen;
}

fn updateCombatTracers(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut CombatTracer, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (entity, mut tracer, mut transform) in &mut query {
        tracer.age += dt;
        let t = (tracer.age / tracer.duration).clamp(0.0, 1.0);
        let arc = 4.0 * t * (1.0 - t) * 1.1;
        transform.translation = tracer.from.lerp(tracer.to, t) + Vec3::Y * arc;
        if tracer.age >= tracer.duration {
            commands.entity(entity).despawn();
        }
    }
}
