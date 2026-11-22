use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use roeSim::{
    BuildingId, Command, FormationKind, Fx, MapGrid, MovementDomain, PlayerId, TerrainType,
    UnitClass, UnitId, Vec2Fx,
};

use crate::bridge::{CommandQueue, SimPaused, SimState, WorldResetCounter};
use crate::camera::IsoCamera;
use crate::{simToWorld, terrainHeightF32};

/// Modo explícito acionado pelos botões de ação (Mover, Atacar, Reformar).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CommandMode {
    #[default]
    Default,
    Move,
    Attack,
    Repair,
}

/// Estado interativo do jogador local (seleção, formação, posicionamento e névoa).
#[derive(Resource)]
pub struct PlayerControlState {
    pub localPlayer: PlayerId,
    pub selectedUnits: Vec<UnitId>,
    pub selectedBuilding: Option<BuildingId>,
    pub activeFormation: FormationKind,
    /// `(buildingKind, wonderId)` quando o jogador está escolhendo onde construir.
    pub placingBuilding: Option<(String, Option<String>)>,
    pub commandMode: CommandMode,
    pub showFog: bool,
    pub hoveredTile: Option<(u32, u32)>,
    pub uiConsumedPointer: bool,
}

impl Default for PlayerControlState {
    fn default() -> Self {
        Self {
            localPlayer: PlayerId(0),
            selectedUnits: Vec::new(),
            selectedBuilding: None,
            activeFormation: FormationKind::Box,
            placingBuilding: None,
            commandMode: CommandMode::Default,
            showFog: true,
            hoveredTile: None,
            uiConsumedPointer: false,
        }
    }
}

#[derive(Resource, Default)]
struct DragState {
    startScreen: Option<Vec2>,
    currentScreen: Vec2,
}

#[derive(Component)]
struct SelectionBoxUi;

#[derive(Component)]
struct GhostPreview;

#[derive(Component)]
struct OrderMarker {
    age: f32,
    duration: f32,
}

#[derive(Resource)]
struct InputVisualAssets {
    validMat: Handle<StandardMaterial>,
    invalidMat: Handle<StandardMaterial>,
    markerMesh: Handle<Mesh>,
    moveMarkerMat: Handle<StandardMaterial>,
    attackMarkerMat: Handle<StandardMaterial>,
}

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerControlState>()
            .init_resource::<DragState>()
            .add_systems(Startup, spawnInputVisuals)
            .add_systems(
                Update,
                (
                    pruneDeadSelections,
                    handleHotkeys,
                    updateHoveredAndGhost,
                    handleMouseSelectionAndOrders,
                    updateOrderMarkers,
                )
                    .chain(),
            );
    }
}

fn spawnInputVisuals(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let ghostMesh = meshes.add(Cuboid::new(1.0, 0.35, 1.0));
    let validMat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.18, 0.88, 0.34, 0.50),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let invalidMat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.92, 0.20, 0.18, 0.52),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let markerMesh = meshes.add(Annulus::new(0.24, 0.36));
    let moveMarkerMat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.22, 0.96, 0.42, 0.88),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let attackMarkerMat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.96, 0.24, 0.20, 0.88),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    commands.spawn((
        GhostPreview,
        Mesh3d(ghostMesh.clone()),
        MeshMaterial3d(validMat.clone()),
        Transform::IDENTITY,
        Visibility::Hidden,
        NotShadowCaster,
    ));

    // Retângulo 2D da caixa de seleção.
    commands.spawn((
        SelectionBoxUi,
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(0),
            width: px(0),
            height: px(0),
            border: UiRect::all(px(1.5)),
            display: Display::None,
            ..default()
        },
        BackgroundColor(Color::srgba(0.16, 0.86, 0.36, 0.18)),
        BorderColor::all(Color::srgba(0.26, 0.96, 0.48, 0.92)),
        ZIndex(50),
    ));

    commands.insert_resource(InputVisualAssets {
        validMat,
        invalidMat,
        markerMesh,
        moveMarkerMat,
        attackMarkerMat,
    });
}

fn pruneDeadSelections(
    sim: Res<SimState>,
    reset: Res<WorldResetCounter>,
    mut lastReset: Local<u64>,
    mut control: ResMut<PlayerControlState>,
) {
    if *lastReset != reset.0 {
        *lastReset = reset.0;
        control.selectedUnits.clear();
        control.selectedBuilding = None;
        control.placingBuilding = None;
    }
    let world = &sim.0;
    let localPlayer = control.localPlayer;
    control
        .selectedUnits
        .retain(|id| world.unit(*id).is_some_and(|u| u.owner == localPlayer));
    if let Some(bid) = control.selectedBuilding {
        if world.building(bid).is_none_or(|b| b.owner != localPlayer) {
            control.selectedBuilding = None;
        }
    }
}

fn handleHotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    sim: Res<SimState>,
    paused: Res<SimPaused>,
    mut control: ResMut<PlayerControlState>,
    mut queue: ResMut<CommandQueue>,
    mut camera: Single<&mut IsoCamera>,
) {
    if paused.0 {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        if control.placingBuilding.is_some() {
            control.placingBuilding = None;
        } else if control.commandMode != CommandMode::Default {
            control.commandMode = CommandMode::Default;
        } else {
            control.selectedUnits.clear();
            control.selectedBuilding = None;
        }
    }
    if keys.just_pressed(KeyCode::KeyF) {
        control.showFog = !control.showFog;
    }
    if keys.just_pressed(KeyCode::KeyG) {
        control.activeFormation = match control.activeFormation {
            FormationKind::Box => FormationKind::Line,
            FormationKind::Line => FormationKind::Wedge,
            FormationKind::Wedge => FormationKind::Box,
        };
        if !control.selectedUnits.is_empty() {
            queue.0.push(Command::SetFormation {
                owner: control.localPlayer,
                units: control.selectedUnits.clone(),
                formation: control.activeFormation,
            });
        }
    }
    if keys.just_pressed(KeyCode::KeyH) {
        if let Some(city) = sim.0.cities.iter().find(|c| c.owner == control.localPlayer) {
            camera.focus = Vec3::new(city.centerTile.0 as f32 + 0.5, 0.0, city.centerTile.1 as f32 + 0.5);
        }
    }
    if keys.just_pressed(KeyCode::KeyE) {
        if let Some(city) = sim.0.cities.iter().find(|c| c.owner != control.localPlayer) {
            camera.focus = Vec3::new(city.centerTile.0 as f32 + 0.5, 0.0, city.centerTile.1 as f32 + 0.5);
        }
    }
}

fn updateHoveredAndGhost(
    window: Single<&Window, With<PrimaryWindow>>,
    cameraQuery: Single<(&Camera, &GlobalTransform), With<IsoCamera>>,
    sim: Res<SimState>,
    assets: Res<InputVisualAssets>,
    mut control: ResMut<PlayerControlState>,
    mut ghostQuery: Single<
        (&mut Transform, &mut Visibility, &mut MeshMaterial3d<StandardMaterial>),
        With<GhostPreview>,
    >,
) {
    let (camera, camTransform) = *cameraQuery;
    let world = &sim.0;
    let Some(cursor) = window.cursor_position() else {
        control.hoveredTile = None;
        *ghostQuery.1 = Visibility::Hidden;
        return;
    };

    let ground = screenToGround(camera, camTransform, &world.map, cursor);
    control.hoveredTile = ground.map(|g| {
        let tx = (g.x.floor() as i32).clamp(0, world.map.width.saturating_sub(1) as i32) as u32;
        let ty = (g.z.floor() as i32).clamp(0, world.map.height.saturating_sub(1) as i32) as u32;
        (tx, ty)
    });

    if let (Some((kind, wonderId)), Some((tx, ty))) =
        (&control.placingBuilding, control.hoveredTile)
    {
        if let Some(def) = world.data.building(kind) {
            let fp = def.footprint;
            let originX = tx.saturating_sub(fp / 2).min(world.map.width.saturating_sub(fp));
            let originY = ty.saturating_sub(fp / 2).min(world.map.height.saturating_sub(fp));
            let valid = world.canPlaceBuilding(
                control.localPlayer,
                kind,
                (originX, originY),
                wonderId.as_deref(),
            );
            let cx = originX as f32 + fp as f32 * 0.5;
            let cz = originY as f32 + fp as f32 * 0.5;
            let cy = terrainHeightF32(&world.map, cx, cz);

            ghostQuery.0.translation = Vec3::new(cx, cy + 0.22, cz);
            ghostQuery.0.scale = Vec3::new(fp as f32 * 0.96, 1.0, fp as f32 * 0.96);
            *ghostQuery.1 = Visibility::Visible;
            ghostQuery.2.0 =
                if valid { assets.validMat.clone() } else { assets.invalidMat.clone() };
            return;
        }
    }
    *ghostQuery.1 = Visibility::Hidden;
}

fn handleMouseSelectionAndOrders(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    window: Single<&Window, With<PrimaryWindow>>,
    cameraQuery: Single<(&Camera, &GlobalTransform), With<IsoCamera>>,
    sim: Res<SimState>,
    paused: Res<SimPaused>,
    assets: Res<InputVisualAssets>,
    mut drag: ResMut<DragState>,
    mut control: ResMut<PlayerControlState>,
    mut queue: ResMut<CommandQueue>,
    mut boxNode: Single<&mut Node, With<SelectionBoxUi>>,
) {
    if paused.0 {
        drag.startScreen = None;
        boxNode.display = Display::None;
        return;
    }
    let (camera, camTransform) = *cameraQuery;
    let world = &sim.0;
    let Some(cursor) = window.cursor_position() else { return };

    // Se um botão da HUD acabou de consumir o clique, ignora interação com o terreno neste quadro.
    if control.uiConsumedPointer {
        control.uiConsumedPointer = false;
        drag.startScreen = None;
        boxNode.display = Display::None;
        return;
    }

    // --- Botão esquerdo: modo de ação (Mover/Atacar/Reformar), posicionamento OU seleção ---
    if mouse.just_pressed(MouseButton::Left) {
        if control.commandMode != CommandMode::Default && !control.selectedUnits.is_empty() {
            if let Some(ground) = screenToGround(camera, camTransform, &world.map, cursor) {
                let tx = (ground.x.floor() as i32)
                    .clamp(0, world.map.width.saturating_sub(1) as i32) as u32;
                let ty = (ground.z.floor() as i32)
                    .clamp(0, world.map.height.saturating_sub(1) as i32) as u32;

                match control.commandMode {
                    CommandMode::Move => {
                        let targetFx = Vec2Fx::new(
                            Fx::from_num((ground.x * 100.0).round() as i32) / Fx::from_num(100),
                            Fx::from_num((ground.z * 100.0).round() as i32) / Fx::from_num(100),
                        );
                        queue.0.push(Command::MoveUnits {
                            owner: control.localPlayer,
                            units: control.selectedUnits.clone(),
                            target: targetFx,
                            formation: control.activeFormation,
                        });
                        spawnMarker(&mut commands, &assets, ground, false);
                    }
                    CommandMode::Attack => {
                        let mut hitEnemy = false;
                        for u in &world.units {
                            if u.owner == control.localPlayer {
                                continue;
                            }
                            let uPos = simToWorld(&world.map, u.pos);
                            if Vec2::new(uPos.x - ground.x, uPos.z - ground.z).length() <= 1.35 {
                                queue.0.push(Command::AttackUnit {
                                    owner: control.localPlayer,
                                    units: control.selectedUnits.clone(),
                                    target: u.id,
                                });
                                spawnMarker(&mut commands, &assets, uPos, true);
                                hitEnemy = true;
                                break;
                            }
                        }
                        if !hitEnemy {
                            if let Some(b) = world.buildingAtTile(tx, ty) {
                                if b.owner != control.localPlayer {
                                    queue.0.push(Command::AttackBuilding {
                                        owner: control.localPlayer,
                                        units: control.selectedUnits.clone(),
                                        target: b.id,
                                    });
                                    spawnMarker(&mut commands, &assets, ground, true);
                                    hitEnemy = true;
                                }
                            }
                        }
                        if !hitEnemy {
                            let targetFx = Vec2Fx::new(
                                Fx::from_num((ground.x * 100.0).round() as i32) / Fx::from_num(100),
                                Fx::from_num((ground.z * 100.0).round() as i32) / Fx::from_num(100),
                            );
                            queue.0.push(Command::MoveUnits {
                                owner: control.localPlayer,
                                units: control.selectedUnits.clone(),
                                target: targetFx,
                                formation: control.activeFormation,
                            });
                            spawnMarker(&mut commands, &assets, ground, true);
                        }
                    }
                    CommandMode::Repair => {
                        if let Some(b) = world.buildingAtTile(tx, ty) {
                            if b.owner == control.localPlayer && (!b.isComplete() || b.hp < b.maxHp) {
                                queue.0.push(Command::RepairBuilding {
                                    owner: control.localPlayer,
                                    builders: control.selectedUnits.clone(),
                                    target: b.id,
                                });
                                spawnMarker(&mut commands, &assets, ground, false);
                            }
                        }
                    }
                    CommandMode::Default => {}
                }
            }
            control.commandMode = CommandMode::Default;
            return;
        }

        if let Some((kind, wonderId)) = control.placingBuilding.clone() {
            if let Some((tx, ty)) = control.hoveredTile {
                if let Some(def) = world.data.building(&kind) {
                    let fp = def.footprint;
                    let originX = tx.saturating_sub(fp / 2).min(world.map.width.saturating_sub(fp));
                    let originY = ty.saturating_sub(fp / 2).min(world.map.height.saturating_sub(fp));
                    if world.canPlaceBuilding(
                        control.localPlayer,
                        &kind,
                        (originX, originY),
                        wonderId.as_deref(),
                    ) {
                        let mut builders: Vec<UnitId> = control
                            .selectedUnits
                            .iter()
                            .copied()
                            .filter(|id| {
                                world.unit(*id).is_some_and(|u| u.class == UnitClass::Citizen)
                            })
                            .collect();
                        if builders.is_empty() {
                            // Seleciona automaticamente o cidadão aliado mais próximo.
                            let targetPos = Vec2Fx::tileCenter(originX, originY);
                            if let Some(nearest) = world
                                .units
                                .iter()
                                .filter(|u| {
                                    u.owner == control.localPlayer && u.class == UnitClass::Citizen
                                })
                                .min_by_key(|u| u.pos.distanceSquared(targetPos))
                            {
                                builders.push(nearest.id);
                            }
                        }
                        queue.0.push(Command::ConstructBuilding {
                            owner: control.localPlayer,
                            builders,
                            kind,
                            tile: (originX, originY),
                            wonderId,
                        });
                        if !keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
                            control.placingBuilding = None;
                        }
                    }
                }
            }
            return;
        }

        drag.startScreen = Some(cursor);
        drag.currentScreen = cursor;
    }

    if mouse.pressed(MouseButton::Left) {
        if let Some(start) = drag.startScreen {
            drag.currentScreen = cursor;
            let diff = cursor - start;
            if diff.length() > 6.0 {
                let min = start.min(cursor);
                let size = (cursor - start).abs();
                boxNode.display = Display::Flex;
                boxNode.left = px(min.x);
                boxNode.top = px(min.y);
                boxNode.width = px(size.x);
                boxNode.height = px(size.y);
            } else {
                boxNode.display = Display::None;
            }
        }
    }

    if mouse.just_released(MouseButton::Left) {
        boxNode.display = Display::None;
        if let Some(start) = drag.startScreen.take() {
            let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
            let isBox = (cursor - start).length() > 8.0;

            if isBox {
                let min = start.min(cursor);
                let max = start.max(cursor);
                if !shift {
                    control.selectedUnits.clear();
                }
                for u in &world.units {
                    if u.owner != control.localPlayer {
                        continue;
                    }
                    let groundPos = simToWorld(&world.map, u.pos);
                    let visualPos = match u.domain {
                        MovementDomain::Air => Vec3::new(groundPos.x, groundPos.y.max(0.02) + 3.6, groundPos.z),
                        MovementDomain::Naval => Vec3::new(groundPos.x, groundPos.y.max(0.02) + 0.55, groundPos.z),
                        MovementDomain::Land => groundPos + Vec3::Y * 0.45,
                    };
                    let inBox = [visualPos, groundPos + Vec3::Y * 0.25].iter().any(|&p| {
                        camera.world_to_viewport(camTransform, p).is_ok_and(|sp| {
                            sp.x >= min.x && sp.x <= max.x && sp.y >= min.y && sp.y <= max.y
                        })
                    });
                    if inBox && !control.selectedUnits.contains(&u.id) {
                        control.selectedUnits.push(u.id);
                    }
                }
                if !control.selectedUnits.is_empty() {
                    control.selectedBuilding = None;
                }
            } else if let Some(ground) = screenToGround(camera, camTransform, &world.map, cursor) {
                // Clique simples: tenta selecionar unidade aliada próxima, senão construção aliada.
                let mut pickedUnit: Option<(f32, UnitId)> = None;
                for u in &world.units {
                    if u.owner != control.localPlayer {
                        continue;
                    }
                    let uPos = simToWorld(&world.map, u.pos);
                    let mut dist = Vec2::new(uPos.x - ground.x, uPos.z - ground.z).length();
                    if u.domain == MovementDomain::Air {
                        let airPos = Vec3::new(uPos.x, uPos.y.max(0.02) + 3.6, uPos.z);
                        if let Ok(sp) = camera.world_to_viewport(camTransform, airPos) {
                            let pxDist = sp.distance(cursor);
                            if pxDist <= 30.0 {
                                dist = dist.min(pxDist / 30.0);
                            }
                        }
                    }
                    if dist <= 1.20 && pickedUnit.is_none_or(|(bd, _)| dist < bd) {
                        pickedUnit = Some((dist, u.id));
                    }
                }

                if let Some((_, uid)) = pickedUnit {
                    control.selectedBuilding = None;
                    if shift {
                        if let Some(pos) = control.selectedUnits.iter().position(|&id| id == uid) {
                            control.selectedUnits.remove(pos);
                        } else {
                            control.selectedUnits.push(uid);
                        }
                    } else {
                        control.selectedUnits = vec![uid];
                    }
                } else {
                    let tx =
                        (ground.x.floor() as i32).clamp(0, world.map.width.saturating_sub(1) as i32)
                            as u32;
                    let ty = (ground.z.floor() as i32)
                        .clamp(0, world.map.height.saturating_sub(1) as i32)
                        as u32;
                    if let Some(b) = world
                        .buildingAtTile(tx, ty)
                        .filter(|b| b.owner == control.localPlayer)
                    {
                        control.selectedUnits.clear();
                        control.selectedBuilding = Some(b.id);
                    } else if !shift {
                        control.selectedUnits.clear();
                        control.selectedBuilding = None;
                    }
                }
            }
        }
    }

    // --- Botão direito: cancela posicionamento/modo OU ordem contextual inteligente ---
    if mouse.just_pressed(MouseButton::Right) {
        if control.placingBuilding.is_some() || control.commandMode != CommandMode::Default {
            control.placingBuilding = None;
            control.commandMode = CommandMode::Default;
            return;
        }
        if control.selectedUnits.is_empty() {
            return;
        }
        let Some(ground) = screenToGround(camera, camTransform, &world.map, cursor) else { return };

        // 1. Inimigo sob o cursor -> AttackUnit
        let mut enemyUnit: Option<(f32, UnitId, Vec3)> = None;
        for u in &world.units {
            if u.owner == control.localPlayer {
                continue;
            }
            if control.showFog && !world.isPosVisibleTo(control.localPlayer, u.pos) {
                continue;
            }
            let uPos = simToWorld(&world.map, u.pos);
            let mut dist = Vec2::new(uPos.x - ground.x, uPos.z - ground.z).length();
            if u.domain == MovementDomain::Air {
                let airPos = Vec3::new(uPos.x, uPos.y.max(0.02) + 3.6, uPos.z);
                if let Ok(sp) = camera.world_to_viewport(camTransform, airPos) {
                    let pxDist = sp.distance(cursor);
                    if pxDist <= 30.0 {
                        dist = dist.min(pxDist / 30.0);
                    }
                }
            }
            if dist <= 1.25 && enemyUnit.is_none_or(|(bd, _, _)| dist < bd) {
                enemyUnit = Some((dist, u.id, uPos));
            }
        }
        if let Some((_, targetId, pos)) = enemyUnit {
            queue.0.push(Command::AttackUnit {
                owner: control.localPlayer,
                units: control.selectedUnits.clone(),
                target: targetId,
            });
            spawnMarker(&mut commands, &assets, pos, true);
            return;
        }

        let tx = (ground.x.floor() as i32).clamp(0, world.map.width.saturating_sub(1) as i32) as u32;
        let ty = (ground.z.floor() as i32).clamp(0, world.map.height.saturating_sub(1) as i32) as u32;

        // 2. Construção inimiga sob o cursor -> AttackBuilding
        if let Some(b) = world.buildingAtTile(tx, ty) {
            if b.owner != control.localPlayer {
                queue.0.push(Command::AttackBuilding {
                    owner: control.localPlayer,
                    units: control.selectedUnits.clone(),
                    target: b.id,
                });
                let (cx, cz) = b.centerPos().toF32();
                let cy = terrainHeightF32(&world.map, cx, cz);
                spawnMarker(&mut commands, &assets, Vec3::new(cx, cy, cz), true);
                return;
            }
        }

        // 3. Cidadãos clicando em prédio aliado de recurso/obra ou floresta/montanha -> GatherAt
        let hasCitizen = control
            .selectedUnits
            .iter()
            .any(|id| world.unit(*id).is_some_and(|u| u.class == UnitClass::Citizen));
        if hasCitizen {
            let isGatherableBuilding = world.buildingAtTile(tx, ty).is_some_and(|b| {
                b.owner == control.localPlayer
                    && (!b.isComplete()
                        || b.hp < b.maxHp
                        || world.data.building(&b.kind).and_then(|d| d.producesResource).is_some())
            });
            let isGatherableTerrain = matches!(
                world.map.tile(tx, ty),
                Some(TerrainType::Forest | TerrainType::Hill | TerrainType::Mountain)
            );
            if isGatherableBuilding || isGatherableTerrain {
                queue.0.push(Command::GatherAt {
                    owner: control.localPlayer,
                    units: control.selectedUnits.clone(),
                    tile: (tx, ty),
                });
                spawnMarker(&mut commands, &assets, ground, false);
                return;
            }
        }

        // 4. Ordem padrão: mover em formação usando Flow Field!
        let targetFx = Vec2Fx::new(
            Fx::from_num((ground.x * 100.0).round() as i32) / Fx::from_num(100),
            Fx::from_num((ground.z * 100.0).round() as i32) / Fx::from_num(100),
        );
        queue.0.push(Command::MoveUnits {
            owner: control.localPlayer,
            units: control.selectedUnits.clone(),
            target: targetFx,
            formation: control.activeFormation,
        });
        spawnMarker(&mut commands, &assets, ground, false);
    }
}

fn spawnMarker(commands: &mut Commands, assets: &InputVisualAssets, pos: Vec3, isAttack: bool) {
    commands.spawn((
        OrderMarker { age: 0.0, duration: 0.45 },
        Mesh3d(assets.markerMesh.clone()),
        MeshMaterial3d(if isAttack {
            assets.attackMarkerMat.clone()
        } else {
            assets.moveMarkerMat.clone()
        }),
        Transform::from_translation(pos + Vec3::Y * 0.10)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        NotShadowCaster,
    ));
}

fn updateOrderMarkers(
    mut commands: Commands,
    time: Res<Time>,
    mut query: Query<(Entity, &mut OrderMarker, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (entity, mut marker, mut transform) in &mut query {
        marker.age += dt;
        let t = (marker.age / marker.duration).clamp(0.0, 1.0);
        transform.scale = Vec3::splat(0.55 + t * 1.4);
        if marker.age >= marker.duration {
            commands.entity(entity).despawn();
        }
    }
}

/// Converte posição do mouse na tela para o ponto exato na malha do terreno com relevo.
pub fn screenToGround(
    camera: &Camera,
    camTransform: &GlobalTransform,
    map: &MapGrid,
    cursorPos: Vec2,
) -> Option<Vec3> {
    let ray = camera.viewport_to_world(camTransform, cursorPos).ok()?;
    let dir = *ray.direction;
    if dir.y.abs() < 1e-4 {
        return None;
    }

    // Estima intervalo próximo ao plano y = 0 e faz marcha + refinamento binário no heightmap.
    let tZero = -ray.origin.y / dir.y;
    let tStart = (tZero - 25.0).max(0.0);
    let tEnd = tZero + 25.0;
    let steps = 50;
    let step = (tEnd - tStart) / steps as f32;

    let mut prevT = tStart;
    let mut prevAbove = {
        let p = ray.origin + dir * prevT;
        p.y - terrainHeightF32(map, p.x, p.z)
    };

    for i in 1..=steps {
        let curT = tStart + step * i as f32;
        let p = ray.origin + dir * curT;
        let diff = p.y - terrainHeightF32(map, p.x, p.z);
        if prevAbove >= 0.0 && diff <= 0.0 {
            // Refinamento binário de 8 iterações (< 1 cm de erro).
            let (mut lo, mut hi) = (prevT, curT);
            for _ in 0..8 {
                let mid = 0.5 * (lo + hi);
                let mp = ray.origin + dir * mid;
                if mp.y >= terrainHeightF32(map, mp.x, mp.z) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let hit = ray.origin + dir * (0.5 * (lo + hi));
            let clampedX = hit.x.clamp(0.0, map.width as f32);
            let clampedZ = hit.z.clamp(0.0, map.height as f32);
            let y = terrainHeightF32(map, clampedX, clampedZ);
            return Some(Vec3::new(clampedX, y, clampedZ));
        }
        prevT = curT;
        prevAbove = diff;
    }

    let p = ray.origin + dir * tZero;
    let clampedX = p.x.clamp(0.0, map.width as f32);
    let clampedZ = p.z.clamp(0.0, map.height as f32);
    Some(Vec3::new(clampedX, terrainHeightF32(map, clampedX, clampedZ), clampedZ))
}
