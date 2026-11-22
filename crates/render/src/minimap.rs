use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;
use roeSim::{Command, FogState, Fx, PlayerId, TerrainType, Vec2Fx};

use crate::bridge::{CommandQueue, HudVisible, SimPaused, SimState};
use crate::camera::IsoCamera;
use crate::input::PlayerControlState;

const MINIMAP_PX: f32 = 168.0;
const MINIMAP_LEFT: f32 = 10.0;
const MINIMAP_BOTTOM: f32 = 96.0;

#[derive(Component)]
struct MinimapContainer;

#[derive(Resource)]
struct MinimapState {
    imageHandle: Handle<Image>,
    timer: Timer,
}

pub struct MinimapPlugin;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawnMinimap)
            .add_systems(Update, (updateMinimapVisibility, handleMinimapInput, refreshMinimapImage).chain());
    }
}

fn spawnMinimap(
    mut commands: Commands,
    sim: Res<SimState>,
    control: Res<PlayerControlState>,
    mut images: ResMut<Assets<Image>>,
) {
    let img = renderMinimapImage(&sim.0, control.localPlayer, control.showFog, None);
    let handle = images.add(img);

    commands
        .spawn((
            MinimapContainer,
            Node {
                position_type: PositionType::Absolute,
                left: px(MINIMAP_LEFT),
                bottom: px(MINIMAP_BOTTOM),
                width: px(MINIMAP_PX + 4.0),
                height: px(MINIMAP_PX + 4.0),
                border: UiRect::all(px(2.0)),
                padding: UiRect::all(px(0.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.06, 0.09, 0.96)),
            BorderColor::all(Color::srgba(0.76, 0.63, 0.30, 0.88)),
            ZIndex(25),
        ))
        .with_children(|frame| {
            frame.spawn((
                ImageNode::new(handle.clone()),
                Node { width: px(MINIMAP_PX), height: px(MINIMAP_PX), ..default() },
            ));
        });

    commands.insert_resource(MinimapState {
        imageHandle: handle,
        timer: Timer::from_seconds(0.10, TimerMode::Repeating),
    });
}

fn updateMinimapVisibility(
    hudVisible: Res<HudVisible>,
    mut node: Single<&mut Node, With<MinimapContainer>>,
) {
    node.display = if hudVisible.0 { Display::Flex } else { Display::None };
}

fn handleMinimapInput(
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    sim: Res<SimState>,
    paused: Res<SimPaused>,
    hudVisible: Res<HudVisible>,
    mut control: ResMut<PlayerControlState>,
    mut queue: ResMut<CommandQueue>,
    mut camera: Single<&mut IsoCamera>,
) {
    if paused.0 || !hudVisible.0 {
        return;
    }
    let Some(cursor) = window.cursor_position() else { return };
    let winH = window.height();
    let minX = MINIMAP_LEFT + 2.0;
    let maxX = minX + MINIMAP_PX;
    let maxY = winH - MINIMAP_BOTTOM - 2.0;
    let minY = maxY - MINIMAP_PX;

    if cursor.x < minX || cursor.x > maxX || cursor.y < minY || cursor.y > maxY {
        return;
    }

    // Impede que cliques no minimapa vazem para o terreno 3D atrás.
    control.uiConsumedPointer = true;

    let u = ((cursor.x - minX) / MINIMAP_PX).clamp(0.0, 1.0);
    let v = ((cursor.y - minY) / MINIMAP_PX).clamp(0.0, 1.0);
    let map = &sim.0.map;
    let worldX = u * map.width as f32;
    let worldZ = v * map.height as f32;

    if mouse.pressed(MouseButton::Left) {
        camera.focus.x = worldX;
        camera.focus.z = worldZ;
    } else if mouse.just_pressed(MouseButton::Right) && !control.selectedUnits.is_empty() {
        let targetFx = Vec2Fx::new(
            Fx::from_num((worldX * 100.0).round() as i32) / Fx::from_num(100),
            Fx::from_num((worldZ * 100.0).round() as i32) / Fx::from_num(100),
        );
        queue.0.push(Command::MoveUnits {
            owner: control.localPlayer,
            units: control.selectedUnits.clone(),
            target: targetFx,
            formation: control.activeFormation,
        });
    }
}

fn refreshMinimapImage(
    time: Res<Time>,
    sim: Res<SimState>,
    control: Res<PlayerControlState>,
    hudVisible: Res<HudVisible>,
    camera: Single<&IsoCamera>,
    mut minimap: ResMut<MinimapState>,
    mut images: ResMut<Assets<Image>>,
) {
    if !hudVisible.0 {
        return;
    }
    minimap.timer.tick(time.delta());
    if !minimap.timer.just_finished() {
        return;
    }
    if let Some(mut img) = images.get_mut(&minimap.imageHandle) {
        *img = renderMinimapImage(
            &sim.0,
            control.localPlayer,
            control.showFog,
            Some((camera.focus.x, camera.focus.z, camera.viewHeight)),
        );
    }
}

fn renderMinimapImage(
    world: &roeSim::SimWorld,
    localPlayer: PlayerId,
    showFog: bool,
    camView: Option<(f32, f32, f32)>,
) -> Image {
    let map = &world.map;
    let (w, h) = (map.width.max(1), map.height.max(1));
    let scale = 2u32;
    let (imgW, imgH) = (w * scale, h * scale);
    let mut data = vec![0u8; (imgW * imgH * 4) as usize];

    let nationRgb = |pid: PlayerId| -> [u8; 3] {
        if let Some(n) = world.player(pid).and_then(|p| p.nation(&world.data)) {
            [n.primaryColorRgb.0, n.primaryColorRgb.1, n.primaryColorRgb.2]
        } else if pid == PlayerId(0) {
            [60, 140, 235]
        } else {
            [220, 64, 64]
        }
    };

    let setPixel = |buf: &mut [u8], px: u32, py: u32, rgb: [u8; 3]| {
        if px < imgW && py < imgH {
            let idx = ((py * imgW + px) * 4) as usize;
            buf[idx] = rgb[0];
            buf[idx + 1] = rgb[1];
            buf[idx + 2] = rgb[2];
            buf[idx + 3] = 255;
        }
    };

    // 1. Terreno + fronteiras + névoa de guerra.
    for ty in 0..h {
        for tx in 0..w {
            let fog = if showFog {
                world.fogAtTile(localPlayer, tx, ty)
            } else {
                FogState::Visible
            };
            let base: [u8; 3] = match map.tile(tx, ty).unwrap_or(TerrainType::Grass) {
                TerrainType::Water => [26, 76, 128],
                TerrainType::Sand => [194, 174, 122],
                TerrainType::Grass => [76, 128, 56],
                TerrainType::Forest => [38, 84, 38],
                TerrainType::Hill => [116, 132, 68],
                TerrainType::Mountain => [146, 144, 142],
            };

            let owner = world.territory.ownerAtTile(tx, ty);
            let isBorder = owner.is_some()
                && ((tx > 0 && world.territory.ownerAtTile(tx - 1, ty) != owner)
                    || (tx + 1 < w && world.territory.ownerAtTile(tx + 1, ty) != owner)
                    || (ty > 0 && world.territory.ownerAtTile(tx, ty - 1) != owner)
                    || (ty + 1 < h && world.territory.ownerAtTile(tx, ty + 1) != owner));

            let mut col = if fog == FogState::Unexplored {
                [14, 18, 24]
            } else if isBorder {
                nationRgb(owner.unwrap())
            } else if let Some(pid) = owner {
                let nc = nationRgb(pid);
                [
                    ((u16::from(base[0]) * 4 + u16::from(nc[0])) / 5) as u8,
                    ((u16::from(base[1]) * 4 + u16::from(nc[1])) / 5) as u8,
                    ((u16::from(base[2]) * 4 + u16::from(nc[2])) / 5) as u8,
                ]
            } else {
                base
            };

            if fog == FogState::Explored {
                col = [col[0] / 2, col[1] / 2, col[2] / 2];
            }

            for sy in 0..scale {
                for sx in 0..scale {
                    setPixel(&mut data, tx * scale + sx, ty * scale + sy, col);
                }
            }
        }
    }

    // 2. Construções, Cidades e Maravilhas.
    for b in &world.buildings {
        let (bx, by) = b.centerTile();
        if showFog
            && b.owner != localPlayer
            && world.fogAtTile(localPlayer, bx, by) == FogState::Unexplored
        {
            continue;
        }
        let col = if b.wonderId.is_some() {
            [248, 214, 68]
        } else if b.cityId.is_some() {
            let nc = nationRgb(b.owner);
            [nc[0].saturating_add(45), nc[1].saturating_add(45), nc[2].saturating_add(45)]
        } else {
            nationRgb(b.owner)
        };
        let half = if b.cityId.is_some() || b.wonderId.is_some() { 2i32 } else { 1i32 };
        let cx = (bx * scale + 1) as i32;
        let cy = (by * scale + 1) as i32;
        for dy in -half..=half {
            for dx in -half..=half {
                let px = cx + dx;
                let py = cy + dy;
                if px >= 0 && py >= 0 {
                    setPixel(&mut data, px as u32, py as u32, col);
                }
            }
        }
    }

    // 3. Unidades visíveis.
    for u in &world.units {
        if showFog && u.owner != localPlayer && !world.isPosVisibleTo(localPlayer, u.pos) {
            continue;
        }
        let (ux, uz) = u.pos.toF32();
        let px = ((ux * scale as f32).round() as i32).clamp(0, imgW as i32 - 1) as u32;
        let py = ((uz * scale as f32).round() as i32).clamp(0, imgH as i32 - 1) as u32;
        let col = if u.owner == localPlayer { [255, 255, 255] } else { [255, 90, 90] };
        setPixel(&mut data, px, py, col);
        setPixel(&mut data, (px + 1).min(imgW - 1), py, nationRgb(u.owner));
    }

    // 4. Retângulo branco do foco atual da câmera.
    if let Some((fx, fz, viewH)) = camView {
        let halfW = (viewH * 0.72 * scale as f32) as i32;
        let halfH = (viewH * 0.56 * scale as f32) as i32;
        let cx = (fx * scale as f32) as i32;
        let cy = (fz * scale as f32) as i32;
        let x0 = (cx - halfW).clamp(0, imgW as i32 - 1) as u32;
        let x1 = (cx + halfW).clamp(0, imgW as i32 - 1) as u32;
        let y0 = (cy - halfH).clamp(0, imgH as i32 - 1) as u32;
        let y1 = (cy + halfH).clamp(0, imgH as i32 - 1) as u32;
        let white = [245, 245, 245];
        for x in x0..=x1 {
            setPixel(&mut data, x, y0, white);
            setPixel(&mut data, x, y1, white);
        }
        for y in y0..=y1 {
            setPixel(&mut data, x0, y, white);
            setPixel(&mut data, x1, y, white);
        }
    }

    Image::new(
        Extent3d { width: imgW, height: imgH, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
}
