use bevy::prelude::*;
use roeRender::{
    CommandQueue, NetStepManaged, PlayerControlState, SimPaused, SimState, WorldResetCounter,
};
use roeSim::{
    AiDifficulty, Command, GameData, LOCKSTEP_TICKS_PER_TURN, LockstepSession, MapGrid,
    MatchHandshake, NetRole, PlayerId, SimWorld,
};

use crate::audio::{Sfx, SoundBank, ToastNotification};
use crate::menu::{AppMenuState, ScreenMode};

const NET_MAP_SIZE: u32 = 64;

/// Estado da sessão Multiplayer Lockstep (TCP 1v1 determinístico com validação de checksum).
#[derive(Resource, Default)]
pub struct NetPlayState {
    pub session: Option<LockstepSession>,
    pub statusText: String,
    pub matchStarted: bool,
    pendingTurnCmds: Vec<Command>,
    ticksRemainingInTurn: u32,
}

impl NetPlayState {
    pub fn startHost(&mut self, bindAddr: &str, handshake: MatchHandshake) -> Result<(), String> {
        let session = LockstepSession::host(bindAddr, handshake)?;
        let addr = session.localAddr().unwrap_or_else(|| bindAddr.to_string());
        self.session = Some(session);
        self.statusText = format!("Hospedando em {addr} — aguardando oponente conectar...");
        self.matchStarted = false;
        self.pendingTurnCmds.clear();
        self.ticksRemainingInTurn = 0;
        Ok(())
    }

    pub fn startClient(&mut self, addr: &str) -> Result<(), String> {
        let session = LockstepSession::connect(addr)?;
        self.session = Some(session);
        self.statusText = format!("Conectado a {addr} — recebendo dados da partida...");
        self.matchStarted = false;
        self.pendingTurnCmds.clear();
        self.ticksRemainingInTurn = 0;
        Ok(())
    }

    pub fn disconnect(
        &mut self,
        netManaged: &mut NetStepManaged,
        control: &mut PlayerControlState,
    ) {
        self.session = None;
        self.statusText.clear();
        self.matchStarted = false;
        self.pendingTurnCmds.clear();
        self.ticksRemainingInTurn = 0;
        netManaged.0 = false;
        control.localPlayer = PlayerId(0);
    }

    pub fn hudBadge(&self) -> Option<String> {
        let s = self.session.as_ref()?;
        let role = if s.role == NetRole::Host { "HOST" } else { "CLIENT" };
        if s.desyncDetected {
            Some(format!("LOCKSTEP [{role}] DESSINCRONIZACAO!"))
        } else {
            Some(format!("LOCKSTEP [{role}] T:{} OK", s.currentTurn))
        }
    }
}

pub struct NetPlayPlugin;

impl Plugin for NetPlayPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NetPlayState>()
            .add_systems(Update, pollNetLobbyAndSession)
            .add_systems(FixedUpdate, stepNetLockstep);
    }
}

fn pollNetLobbyAndSession(
    time: Res<Time>,
    mut net: ResMut<NetPlayState>,
    mut sim: ResMut<SimState>,
    mut queue: ResMut<CommandQueue>,
    mut reset: ResMut<WorldResetCounter>,
    mut netManaged: ResMut<NetStepManaged>,
    mut control: ResMut<PlayerControlState>,
    mut menu: ResMut<AppMenuState>,
    mut bank: ResMut<SoundBank>,
    mut toast: ResMut<ToastNotification>,
) {
    let netRef = &mut *net;
    let Some(session) = netRef.session.as_mut() else {
        return;
    };

    if let Err(err) = session.pollNetwork() {
        let msg = format!("Rede encerrada: {err}");
        netRef.disconnect(&mut netManaged, &mut control);
        toast.show(msg);
        if menu.mode == ScreenMode::NetLobby {
            menu.openMainMenu();
        }
        return;
    }

    if !netRef.matchStarted && session.isReadyToPlay() {
        let Some(hs) = session.handshake.clone() else {
            return;
        };
        let localPlayer = session.localPlayer;
        let roleLabel = if session.role == NetRole::Host { "Host (P1)" } else { "Cliente (P2)" };

        let data = GameData::loadDefault();
        let map = MapGrid::generate(NET_MAP_SIZE, NET_MAP_SIZE, hs.mapSeed);
        sim.0 = SimWorld::newMatchWithAi(
            map,
            data,
            &hs.hostNation,
            &hs.clientNation,
            AiDifficulty::Passive,
        );
        queue.0.clear();
        control.localPlayer = localPlayer;
        control.selectedUnits.clear();
        control.selectedBuilding = None;
        control.placingBuilding = None;
        netManaged.0 = true;
        reset.0 += 1;

        netRef.matchStarted = true;
        netRef.pendingTurnCmds.clear();
        netRef.ticksRemainingInTurn = 0;
        netRef.statusText = format!("Lockstep ativo ({roleLabel})");
        menu.resumePlaying();
        bank.play(Sfx::AgeUp, time.elapsed_secs());
        toast.show(format!(
            "Multiplayer Lockstep iniciado ({roleLabel}): {} vs {}",
            hs.hostNation.to_uppercase(),
            hs.clientNation.to_uppercase()
        ));
    }
}

fn stepNetLockstep(
    mut net: ResMut<NetPlayState>,
    mut sim: ResMut<SimState>,
    mut queue: ResMut<CommandQueue>,
    paused: Res<SimPaused>,
    mut netManaged: ResMut<NetStepManaged>,
    mut control: ResMut<PlayerControlState>,
    mut toast: ResMut<ToastNotification>,
) {
    let netRef = &mut *net;
    if !netManaged.0 || !netRef.matchStarted || paused.0 {
        return;
    }

    let Some(session) = netRef.session.as_mut() else {
        netManaged.0 = false;
        return;
    };

    if let Err(err) = session.pollNetwork() {
        toast.show(format!("Conexao multiplayer perdida: {err}"));
        netRef.disconnect(&mut netManaged, &mut control);
        return;
    }

    if netRef.ticksRemainingInTurn == 0 {
        let localCmds = std::mem::take(&mut queue.0);
        let checksum = sim.0.checksum();
        if let Err(err) = session.submitLocalTurn(localCmds, checksum) {
            toast.show(format!("Erro ao enviar turno lockstep: {err}"));
            netRef.disconnect(&mut netManaged, &mut control);
            return;
        }
        if let Some(merged) = session.tryConsumeReadyTurn() {
            if session.desyncDetected {
                toast.show("ALERTA: Dessincronizacao detectada no Lockstep!");
            }
            netRef.pendingTurnCmds = merged;
            netRef.ticksRemainingInTurn = LOCKSTEP_TICKS_PER_TURN;
        } else {
            // Aguardando pacote do par remoto para este turno.
            return;
        }
    }

    if netRef.ticksRemainingInTurn > 0 {
        if netRef.ticksRemainingInTurn == LOCKSTEP_TICKS_PER_TURN {
            let cmds = std::mem::take(&mut netRef.pendingTurnCmds);
            sim.0.step(&cmds);
        } else {
            sim.0.step(&[]);
        }
        netRef.ticksRemainingInTurn -= 1;
    }
}
