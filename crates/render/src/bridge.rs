use bevy::prelude::*;
use roeSim::{Command, SimWorld, TICKS_PER_SECOND};

/// Estado da simulação. Só `stepSim` (ou troca de partida/load) pode modificá-lo.
#[derive(Resource)]
pub struct SimState(pub SimWorld);

/// Comandos a aplicar no próximo tick.
#[derive(Resource, Default)]
pub struct CommandQueue(pub Vec<Command>);

/// Controla se a simulação está pausada (menu principal / menu de pausa).
#[derive(Resource, Default)]
pub struct SimPaused(pub bool);

/// Controla visibilidade de elementos de HUD de partida (ex.: minimapa oculto no menu principal).
#[derive(Resource)]
pub struct HudVisible(pub bool);

impl Default for HudVisible {
    fn default() -> Self {
        Self(true)
    }
}

/// Incrementado sempre que o mundo é substituído (Nova Partida ou Carregar Save),
/// sinalizando aos sistemas visuais para reconstruir terreno, unidades, prédios e câmera.
#[derive(Resource, Default)]
pub struct WorldResetCounter(pub u64);

/// Quando ativo (partida Multiplayer Lockstep), o avanço de `SimWorld` é controlado pelo
/// módulo de rede em turnos sincronizados em vez de `stepSim` local.
#[derive(Resource, Default)]
pub struct NetStepManaged(pub bool);

pub struct BridgePlugin;

impl Plugin for BridgePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CommandQueue>()
            .init_resource::<SimPaused>()
            .init_resource::<HudVisible>()
            .init_resource::<WorldResetCounter>()
            .init_resource::<NetStepManaged>()
            .insert_resource(Time::<Fixed>::from_hz(f64::from(TICKS_PER_SECOND)))
            .add_systems(FixedUpdate, stepSim);
    }
}

fn stepSim(
    mut sim: ResMut<SimState>,
    mut queue: ResMut<CommandQueue>,
    paused: Res<SimPaused>,
    netManaged: Res<NetStepManaged>,
) {
    if netManaged.0 {
        return;
    }
    if paused.0 {
        queue.0.clear();
        return;
    }
    let commands = std::mem::take(&mut queue.0);
    sim.0.step(&commands);
}
