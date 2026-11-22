use bevy::camera::ScalingMode;
use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::bridge::{SimPaused, SimState, WorldResetCounter};

/// Câmera ortográfica de ângulo fixo, no estilo dos RTS isométricos clássicos:
/// o mapa aparece como losango e os tiles ficam na proporção 2:1.
#[derive(Component)]
pub struct IsoCamera {
    /// Ponto do chão no centro da tela.
    pub focus: Vec3,
    /// Altura visível da tela, em unidades de mundo (zoom).
    pub viewHeight: f32,
    pub edgeScroll: bool,
}

const YAW: f32 = std::f32::consts::FRAC_PI_4;
/// 30° acima do horizonte: sen(30°) = 0,5 dá exatamente os tiles 2:1.
const ELEVATION: f32 = std::f32::consts::FRAC_PI_6;
const CAMERA_DISTANCE: f32 = 200.0;
const MIN_VIEW_HEIGHT: f32 = 8.0;
const MAX_VIEW_HEIGHT: f32 = 60.0;
const EDGE_SCROLL_MARGIN: f32 = 6.0;

pub struct IsoCameraPlugin;

impl Plugin for IsoCameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawnCamera)
            .add_systems(Update, (resetCameraOnWorldChange, controlCamera, applyCamera).chain());
    }
}

fn resetCameraOnWorldChange(
    sim: Res<SimState>,
    reset: Res<WorldResetCounter>,
    mut lastReset: Local<u64>,
    mut camera: Single<&mut IsoCamera>,
) {
    if *lastReset == reset.0 {
        return;
    }
    *lastReset = reset.0;
    let map = &sim.0.map;
    camera.focus = sim
        .0
        .cities
        .iter()
        .find(|c| c.owner == roeSim::PlayerId(0))
        .map(|c| Vec3::new(c.centerTile.0 as f32 + 0.5, 0.0, c.centerTile.1 as f32 + 0.5))
        .unwrap_or_else(|| Vec3::new(map.width as f32 / 2.0, 0.0, map.height as f32 / 2.0));
}

fn spawnCamera(mut commands: Commands, sim: Res<SimState>) {
    let map = &sim.0.map;
    let focus = sim
        .0
        .cities
        .iter()
        .find(|c| c.owner == roeSim::PlayerId(0))
        .map(|c| Vec3::new(c.centerTile.0 as f32 + 0.5, 0.0, c.centerTile.1 as f32 + 0.5))
        .unwrap_or_else(|| Vec3::new(map.width as f32 / 2.0, 0.0, map.height as f32 / 2.0));
    commands.spawn((
        Camera3d::default(),
        Projection::from(OrthographicProjection {
            far: CAMERA_DISTANCE * 4.0,
            ..OrthographicProjection::default_3d()
        }),
        IsoCamera { focus, viewHeight: 24.0, edgeScroll: true },
    ));
}

fn controlCamera(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    window: Single<&Window, With<PrimaryWindow>>,
    sim: Res<SimState>,
    paused: Res<SimPaused>,
    mut camera: Single<&mut IsoCamera>,
) {
    if paused.0 {
        return;
    }
    let mut pan = Vec2::ZERO;
    if keys.any_pressed([KeyCode::KeyA, KeyCode::ArrowLeft]) {
        pan.x -= 1.0;
    }
    if keys.any_pressed([KeyCode::KeyD, KeyCode::ArrowRight]) {
        pan.x += 1.0;
    }
    if keys.any_pressed([KeyCode::KeyW, KeyCode::ArrowUp]) {
        pan.y += 1.0;
    }
    if keys.any_pressed([KeyCode::KeyS, KeyCode::ArrowDown]) {
        pan.y -= 1.0;
    }
    if camera.edgeScroll && window.focused {
        if let Some(cursor) = window.cursor_position() {
            if cursor.x <= EDGE_SCROLL_MARGIN {
                pan.x -= 1.0;
            }
            if cursor.x >= window.width() - EDGE_SCROLL_MARGIN {
                pan.x += 1.0;
            }
            if cursor.y <= EDGE_SCROLL_MARGIN {
                pan.y += 1.0;
            }
            if cursor.y >= window.height() - EDGE_SCROLL_MARGIN {
                pan.y -= 1.0;
            }
        }
    }

    if pan != Vec2::ZERO {
        let pan = pan.clamp(Vec2::NEG_ONE, Vec2::ONE);
        let screenRight = Vec3::new(YAW.cos(), 0.0, -YAW.sin());
        let screenUp = Vec3::new(-YAW.sin(), 0.0, -YAW.cos());
        // O chão é visto inclinado: andar "para cima" na tela exige mais distância no mundo.
        let groundMove = screenRight * pan.x + screenUp * (pan.y / ELEVATION.sin());
        let speed = camera.viewHeight * 0.9;
        camera.focus += groundMove * speed * time.delta_secs();
    }

    let map = &sim.0.map;
    camera.focus.x = camera.focus.x.clamp(0.0, map.width as f32);
    camera.focus.z = camera.focus.z.clamp(0.0, map.height as f32);

    let notches = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
    };
    if notches != 0.0 {
        camera.viewHeight =
            (camera.viewHeight * 0.9_f32.powf(notches)).clamp(MIN_VIEW_HEIGHT, MAX_VIEW_HEIGHT);
    }
}

fn applyCamera(mut query: Single<(&IsoCamera, &mut Transform, &mut Projection), Changed<IsoCamera>>) {
    let (camera, transform, projection) = &mut *query;
    let offset = Vec3::new(
        YAW.sin() * ELEVATION.cos(),
        ELEVATION.sin(),
        YAW.cos() * ELEVATION.cos(),
    ) * CAMERA_DISTANCE;
    **transform = Transform::from_translation(camera.focus + offset).looking_at(camera.focus, Vec3::Y);
    if let Projection::Orthographic(ortho) = &mut **projection {
        ortho.scaling_mode = ScalingMode::FixedVertical { viewport_height: camera.viewHeight };
    }
}
