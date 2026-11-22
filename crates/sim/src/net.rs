use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

use serde::{Deserialize, Serialize};

use crate::command::{Command, PlayerId};

/// Ticks executados por turno de lockstep (4 ticks a 20 Hz = 200 ms por turno).
pub const LOCKSTEP_TICKS_PER_TURN: u32 = 4;
/// Turnos de latência de entrada para absorver jitter de rede.
pub const LOCKSTEP_INPUT_DELAY: u64 = 1;
/// Porta padrão TCP para partidas locais/LAN.
pub const DEFAULT_NET_PORT: u16 = 7878;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatchHandshake {
    pub mapSeed: u64,
    pub hostNation: String,
    pub clientNation: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NetMessage {
    Handshake(MatchHandshake),
    Turn {
        turn: u64,
        player: PlayerId,
        commands: Vec<Command>,
        checksum: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetRole {
    Host,
    Client,
}

/// Sessão TCP lockstep não-bloqueante para partidas 1v1 determinísticas.
/// Ambos os pares trocam `NetMessage::Turn` (em RON por linha) e comparam `checksum()` a cada turno.
pub struct LockstepSession {
    pub role: NetRole,
    pub localPlayer: PlayerId,
    pub currentTurn: u64,
    pub tickInTurn: u32,
    pub desyncDetected: bool,
    pub lastVerifiedChecksum: u64,
    pub handshake: Option<MatchHandshake>,
    listener: Option<TcpListener>,
    stream: Option<TcpStream>,
    readBuf: String,
    localQueue: BTreeMap<u64, (Vec<Command>, u64)>,
    remoteQueue: BTreeMap<u64, (Vec<Command>, u64)>,
}

impl LockstepSession {
    /// Inicia um servidor TCP não-bloqueante aguardando um oponente.
    pub fn host(bindAddr: &str, handshake: MatchHandshake) -> Result<Self, String> {
        let listener =
            TcpListener::bind(bindAddr).map_err(|e| format!("falha ao abrir {bindAddr}: {e}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| format!("falha ao configurar nonblocking: {e}"))?;
        let mut session = Self {
            role: NetRole::Host,
            localPlayer: PlayerId(0),
            currentTurn: 0,
            tickInTurn: 0,
            desyncDetected: false,
            lastVerifiedChecksum: 0,
            handshake: Some(handshake),
            listener: Some(listener),
            stream: None,
            readBuf: String::new(),
            localQueue: BTreeMap::new(),
            remoteQueue: BTreeMap::new(),
        };
        session.seedInitialDelayTurns();
        Ok(session)
    }

    /// Conecta a um host TCP existente e inicia em modo cliente (`PlayerId(1)`).
    pub fn connect(addr: &str) -> Result<Self, String> {
        let stream =
            TcpStream::connect(addr).map_err(|e| format!("falha ao conectar em {addr}: {e}"))?;
        let _ = stream.set_nodelay(true);
        stream
            .set_nonblocking(true)
            .map_err(|e| format!("falha ao configurar nonblocking: {e}"))?;
        let mut session = Self {
            role: NetRole::Client,
            localPlayer: PlayerId(1),
            currentTurn: 0,
            tickInTurn: 0,
            desyncDetected: false,
            lastVerifiedChecksum: 0,
            handshake: None,
            listener: None,
            stream: Some(stream),
            readBuf: String::new(),
            localQueue: BTreeMap::new(),
            remoteQueue: BTreeMap::new(),
        };
        session.seedInitialDelayTurns();
        Ok(session)
    }

    pub fn localAddr(&self) -> Option<String> {
        self.listener
            .as_ref()
            .and_then(|l| l.local_addr().ok())
            .map(|a| a.to_string())
    }

    pub fn isConnected(&self) -> bool {
        self.stream.is_some()
    }

    pub fn isReadyToPlay(&self) -> bool {
        self.stream.is_some() && self.handshake.is_some()
    }

    fn seedInitialDelayTurns(&mut self) {
        for t in 0..LOCKSTEP_INPUT_DELAY {
            self.localQueue.insert(t, (Vec::new(), 0));
            self.remoteQueue.insert(t, (Vec::new(), 0));
        }
    }

    /// Aceita conexão pendente (se Host) e lê todos os pacotes disponíveis do socket.
    pub fn pollNetwork(&mut self) -> Result<(), String> {
        if self.stream.is_none() {
            if let Some(listener) = &self.listener {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nodelay(true);
                        stream
                            .set_nonblocking(true)
                            .map_err(|e| format!("nonblocking peer: {e}"))?;
                        self.stream = Some(stream);
                        if let Some(hs) = self.handshake.clone() {
                            self.sendMessage(&NetMessage::Handshake(hs))?;
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(format!("erro no accept: {e}")),
                }
            }
        }

        let mut incoming = Vec::new();
        if let Some(stream) = self.stream.as_ref() {
            let mut reader = BufReader::new(stream);
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => return Err("conexao encerrada pelo par remoto".into()),
                    Ok(_) => {
                        self.readBuf.push_str(&line);
                        if self.readBuf.ends_with('\n') {
                            let raw = std::mem::take(&mut self.readBuf);
                            let trimmed = raw.trim();
                            if !trimmed.is_empty() {
                                let msg: NetMessage = ron::from_str(trimmed)
                                    .map_err(|e| format!("pacote invalido: {e}"))?;
                                incoming.push(msg);
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) => return Err(format!("erro de leitura TCP: {e}")),
                }
            }
        }
        for msg in incoming {
            self.handleIncoming(msg);
        }
        Ok(())
    }

    fn handleIncoming(&mut self, msg: NetMessage) {
        match msg {
            NetMessage::Handshake(hs) => {
                self.handshake = Some(hs);
            }
            NetMessage::Turn { turn, commands, checksum, .. } => {
                self.remoteQueue.insert(turn, (commands, checksum));
            }
        }
    }

    fn sendMessage(&mut self, msg: &NetMessage) -> Result<(), String> {
        let Some(stream) = self.stream.as_mut() else {
            return Err("sem conexao ativa".into());
        };
        let mut encoded =
            ron::to_string(msg).map_err(|e| format!("erro ao serializar pacote: {e}"))?;
        encoded.push('\n');
        stream
            .write_all(encoded.as_bytes())
            .map_err(|e| format!("erro ao enviar pacote: {e}"))?;
        let _ = stream.flush();
        Ok(())
    }

    /// Agenda os comandos locais para `currentTurn + LOCKSTEP_INPUT_DELAY` e envia ao par remoto.
    pub fn submitLocalTurn(
        &mut self,
        commands: Vec<Command>,
        checksum: u64,
    ) -> Result<(), String> {
        let targetTurn = self.currentTurn + LOCKSTEP_INPUT_DELAY;
        if self.localQueue.contains_key(&targetTurn) {
            return Ok(());
        }
        self.localQueue.insert(targetTurn, (commands.clone(), checksum));
        self.sendMessage(&NetMessage::Turn {
            turn: targetTurn,
            player: self.localPlayer,
            commands,
            checksum,
        })
    }

    /// Tenta consumir o turno atual quando os pacotes de ambos os jogadores já chegaram.
    /// Retorna os comandos mesclados em ordem determinística (`PlayerId(0)` antes de `PlayerId(1)`).
    pub fn tryConsumeReadyTurn(&mut self) -> Option<Vec<Command>> {
        if !self.localQueue.contains_key(&self.currentTurn)
            || !self.remoteQueue.contains_key(&self.currentTurn)
        {
            return None;
        }
        let (localCmds, localSum) = self.localQueue.remove(&self.currentTurn).unwrap();
        let (remoteCmds, remoteSum) = self.remoteQueue.remove(&self.currentTurn).unwrap();

        if self.currentTurn >= LOCKSTEP_INPUT_DELAY && localSum != 0 && remoteSum != 0 {
            if localSum != remoteSum {
                self.desyncDetected = true;
            } else {
                self.lastVerifiedChecksum = localSum;
            }
        }

        let mut merged = Vec::with_capacity(localCmds.len() + remoteCmds.len());
        if self.localPlayer == PlayerId(0) {
            merged.extend(localCmds);
            merged.extend(remoteCmds);
        } else {
            merged.extend(remoteCmds);
            merged.extend(localCmds);
        }
        self.currentTurn += 1;
        Some(merged)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::AiDifficulty;
    use crate::data::{FormationKind, GameData};
    use crate::map::MapGrid;
    use crate::math::Vec2Fx;
    use crate::world::SimWorld;

    #[test]
    fn phase10LockstepLoopbackStaysBitExact() {
        let hs = MatchHandshake {
            mapSeed: 77,
            hostNation: "roma".into(),
            clientNation: "grecia".into(),
        };
        let mut host = LockstepSession::host("127.0.0.1:0", hs.clone()).unwrap();
        let addr = host.localAddr().unwrap();
        let mut client = LockstepSession::connect(&addr).unwrap();

        // Conclui handshake.
        for _ in 0..40 {
            host.pollNetwork().unwrap();
            client.pollNetwork().unwrap();
            if host.isReadyToPlay() && client.isReadyToPlay() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(host.isReadyToPlay() && client.isReadyToPlay());

        let data = GameData::loadDefault();
        let clientHs = client.handshake.clone().unwrap();
        let mut worldHost = SimWorld::newMatchWithAi(
            MapGrid::generate(48, 48, hs.mapSeed),
            data.clone(),
            &hs.hostNation,
            &hs.clientNation,
            AiDifficulty::Passive,
        );
        let mut worldClient = SimWorld::newMatchWithAi(
            MapGrid::generate(48, 48, clientHs.mapSeed),
            data,
            &clientHs.hostNation,
            &clientHs.clientNation,
            AiDifficulty::Passive,
        );
        assert_eq!(worldHost.checksum(), worldClient.checksum());

        // Executa 8 turnos lockstep (32 ticks) com comandos distintos de cada jogador.
        for stepTurn in 0..8u64 {
            let hostCmds = if stepTurn == 1 {
                let u0 = worldHost.units.iter().find(|u| u.owner == PlayerId(0)).unwrap().id;
                vec![Command::MoveUnits {
                    owner: PlayerId(0),
                    units: vec![u0],
                    target: Vec2Fx::fromInts(24, 24),
                    formation: FormationKind::Line,
                }]
            } else {
                Vec::new()
            };
            let clientCmds = if stepTurn == 2 {
                let u1 = worldClient.units.iter().find(|u| u.owner == PlayerId(1)).unwrap().id;
                vec![Command::MoveUnits {
                    owner: PlayerId(1),
                    units: vec![u1],
                    target: Vec2Fx::fromInts(24, 24),
                    formation: FormationKind::Wedge,
                }]
            } else {
                Vec::new()
            };

            host.submitLocalTurn(hostCmds, worldHost.checksum()).unwrap();
            client.submitLocalTurn(clientCmds, worldClient.checksum()).unwrap();

            let (mut hMerged, mut cMerged) = (None, None);
            for _ in 0..80 {
                host.pollNetwork().unwrap();
                client.pollNetwork().unwrap();
                if hMerged.is_none() {
                    hMerged = host.tryConsumeReadyTurn();
                }
                if cMerged.is_none() {
                    cMerged = client.tryConsumeReadyTurn();
                }
                if hMerged.is_some() && cMerged.is_some() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            let hCmds = hMerged.expect("host deve receber turno");
            let cCmds = cMerged.expect("client deve receber turno");

            for sub in 0..LOCKSTEP_TICKS_PER_TURN {
                let sliceH: &[Command] = if sub == 0 { &hCmds } else { &[] };
                let sliceC: &[Command] = if sub == 0 { &cCmds } else { &[] };
                worldHost.step(sliceH);
                worldClient.step(sliceC);
            }
            assert_eq!(worldHost.checksum(), worldClient.checksum());
        }
        assert!(!host.desyncDetected);
        assert!(!client.desyncDetected);
    }
}
