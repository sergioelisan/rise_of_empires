use std::path::PathBuf;

use bevy::picking::events::{PointerOut, PointerOver, PointerPress};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use roeRender::{
    CommandQueue, HudVisible, NetStepManaged, PlayerControlState, SimPaused, SimState,
    WorldResetCounter,
};
use roeSim::{
    AiDifficulty, DEFAULT_NET_PORT, GameData, MapGrid, MatchHandshake, PlayerId, SimWorld,
    loadWorldFromFile, saveWorldToFile,
};

use crate::audio::{Sfx, SoundBank, ToastNotification};
use crate::campaign_ui::ActiveCampaignState;
use crate::netplay::NetPlayState;

const MAP_SIZE: u32 = 64;

fn quicksavePath() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("rise-of-empires/saves/quicksave.ron");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return PathBuf::from(home).join(".local/share/rise-of-empires/saves/quicksave.ron");
        }
    }
    PathBuf::from("saves/quicksave.ron")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenMode {
    MainMenu,
    CampaignMap,
    NetLobby,
    Playing,
    Paused,
}

#[derive(Resource)]
pub struct AppMenuState {
    pub mode: ScreenMode,
    pub playerNationIdx: usize,
    pub enemyNationIdx: usize,
    pub aiDifficulty: AiDifficulty,
    pub mapSeed: u64,
    dirty: bool,
}

impl AppMenuState {
    pub fn new(
        initialMode: ScreenMode,
        playerNationIdx: usize,
        enemyNationIdx: usize,
        aiDifficulty: AiDifficulty,
        mapSeed: u64,
    ) -> Self {
        Self {
            mode: initialMode,
            playerNationIdx,
            enemyNationIdx,
            aiDifficulty,
            mapSeed,
            dirty: true,
        }
    }

    pub fn openPause(&mut self) {
        self.mode = ScreenMode::Paused;
        self.dirty = true;
    }

    pub fn resumePlaying(&mut self) {
        self.mode = ScreenMode::Playing;
        self.dirty = true;
    }

    pub fn openMainMenu(&mut self) {
        self.mode = ScreenMode::MainMenu;
        self.dirty = true;
    }

    pub fn openCampaignMap(&mut self) {
        self.mode = ScreenMode::CampaignMap;
        self.dirty = true;
    }

    pub fn openNetLobby(&mut self) {
        self.mode = ScreenMode::NetLobby;
        self.dirty = true;
    }
}

#[derive(Component)]
struct MenuOverlayRoot;

#[derive(Component, Clone)]
enum MenuAction {
    CyclePlayerNation,
    CycleEnemyNation,
    CycleDifficulty,
    NextSeed,
    StartMatch,
    OpenCampaign,
    HostMultiplayer,
    JoinMultiplayer,
    CancelNetLobby,
    ResumeMatch,
    SaveGame,
    LoadGame,
    ReturnToMainMenu,
    QuitGame,
}

const OVERLAY_BG: Color = Color::srgba(0.04, 0.05, 0.08, 0.72);
const CARD_BG: Color = Color::srgba(0.08, 0.10, 0.15, 0.96);
const GOLD_BORDER: Color = Color::srgba(0.82, 0.68, 0.30, 0.90);
const BTN_BG: Color = Color::srgba(0.16, 0.21, 0.29, 0.98);
const BTN_PRIMARY: Color = Color::srgba(0.18, 0.42, 0.26, 0.98);
const BTN_HOVER: Color = Color::srgba(0.28, 0.36, 0.48, 0.98);

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawnMenuOverlay)
            .add_observer(onMenuButtonPress)
            .add_observer(onMenuButtonOver)
            .add_observer(onMenuButtonOut)
            .add_systems(
                Update,
                (
                    syncPauseAndHudState,
                    handleMenuAndSaveHotkeys,
                    blockPointerOverMenu,
                    rebuildMenuUi,
                )
                    .chain(),
            );
    }
}

fn spawnMenuOverlay(mut commands: Commands) {
    commands.spawn((
        MenuOverlayRoot,
        Hovered::default(),
        Node {
            position_type: PositionType::Absolute,
            top: px(0),
            bottom: px(0),
            left: px(0),
            right: px(0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            display: Display::None,
            ..default()
        },
        BackgroundColor(OVERLAY_BG),
        ZIndex(80),
    ));
}

fn syncPauseAndHudState(
    menu: Res<AppMenuState>,
    mut paused: ResMut<SimPaused>,
    mut hudVisible: ResMut<HudVisible>,
) {
    paused.0 = menu.mode != ScreenMode::Playing;
    hudVisible.0 = matches!(menu.mode, ScreenMode::Playing | ScreenMode::Paused);
}

fn blockPointerOverMenu(
    menu: Res<AppMenuState>,
    mut control: ResMut<PlayerControlState>,
) {
    if menu.mode != ScreenMode::Playing {
        control.uiConsumedPointer = true;
    }
}

fn handleMenuAndSaveHotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut menu: ResMut<AppMenuState>,
    mut sim: ResMut<SimState>,
    mut queue: ResMut<CommandQueue>,
    mut reset: ResMut<WorldResetCounter>,
    mut bank: ResMut<SoundBank>,
    mut toast: ResMut<ToastNotification>,
    mut net: ResMut<NetPlayState>,
    mut netManaged: ResMut<NetStepManaged>,
    mut control: ResMut<PlayerControlState>,
) {
    let now = time.elapsed_secs();

    if keys.just_pressed(KeyCode::F10) {
        match menu.mode {
            ScreenMode::Playing => {
                menu.openPause();
                bank.play(Sfx::Click, now);
            }
            ScreenMode::Paused => {
                menu.resumePlaying();
                bank.play(Sfx::Click, now);
            }
            ScreenMode::MainMenu | ScreenMode::CampaignMap | ScreenMode::NetLobby => {}
        }
    }

    if keys.just_pressed(KeyCode::Escape) {
        match menu.mode {
            ScreenMode::Paused => {
                menu.resumePlaying();
                bank.play(Sfx::Click, now);
            }
            ScreenMode::CampaignMap => {
                menu.openMainMenu();
                bank.play(Sfx::Click, now);
            }
            ScreenMode::NetLobby => {
                net.disconnect(&mut netManaged, &mut control);
                menu.openMainMenu();
                bank.play(Sfx::Click, now);
            }
            ScreenMode::Playing
                if control.placingBuilding.is_none()
                    && control.selectedUnits.is_empty()
                    && control.selectedBuilding.is_none() =>
            {
                menu.openPause();
                bank.play(Sfx::Click, now);
            }
            _ => {}
        }
    }

    if keys.just_pressed(KeyCode::F5) && matches!(menu.mode, ScreenMode::Playing | ScreenMode::Paused) {
        performSave(&sim.0, &mut bank, &mut toast, now);
    }

    if keys.just_pressed(KeyCode::F9) && menu.mode != ScreenMode::NetLobby {
        net.disconnect(&mut netManaged, &mut control);
        if performLoad(&mut sim.0, &mut queue, &mut reset, &mut bank, &mut toast, now) {
            menu.resumePlaying();
        }
    }
}

fn onMenuButtonOver(
    over: On<PointerOver>,
    mut query: Query<&mut BackgroundColor, With<MenuAction>>,
) {
    if let Ok(mut bg) = query.get_mut(over.entity) {
        *bg = BackgroundColor(BTN_HOVER);
    }
}

fn onMenuButtonOut(
    out: On<PointerOut>,
    mut query: Query<(&MenuAction, &mut BackgroundColor)>,
) {
    if let Ok((action, mut bg)) = query.get_mut(out.entity) {
        let base = if matches!(
            action,
            MenuAction::StartMatch | MenuAction::ResumeMatch | MenuAction::OpenCampaign
        ) {
            BTN_PRIMARY
        } else {
            BTN_BG
        };
        *bg = BackgroundColor(base);
    }
}

fn onMenuButtonPress(
    mut press: On<PointerPress>,
    time: Res<Time>,
    mut menu: ResMut<AppMenuState>,
    mut sim: ResMut<SimState>,
    mut queue: ResMut<CommandQueue>,
    mut reset: ResMut<WorldResetCounter>,
    mut bank: ResMut<SoundBank>,
    mut toast: ResMut<ToastNotification>,
    mut net: ResMut<NetPlayState>,
    mut netManaged: ResMut<NetStepManaged>,
    mut control: ResMut<PlayerControlState>,
    mut campState: ResMut<ActiveCampaignState>,
    mut exit: MessageWriter<AppExit>,
    query: Query<&MenuAction>,
) {
    let Ok(action) = query.get(press.entity) else { return };
    press.propagate(false);
    let now = time.elapsed_secs();
    bank.play(Sfx::Click, now);

    let nationCount = sim.0.data.nations.len().max(1);
    match action {
        MenuAction::CyclePlayerNation => {
            menu.playerNationIdx = (menu.playerNationIdx + 1) % nationCount;
            if menu.playerNationIdx == menu.enemyNationIdx {
                menu.enemyNationIdx = (menu.enemyNationIdx + 1) % nationCount;
            }
            menu.dirty = true;
        }
        MenuAction::CycleEnemyNation => {
            menu.enemyNationIdx = (menu.enemyNationIdx + 1) % nationCount;
            if menu.enemyNationIdx == menu.playerNationIdx {
                menu.enemyNationIdx = (menu.enemyNationIdx + 1) % nationCount;
            }
            menu.dirty = true;
        }
        MenuAction::CycleDifficulty => {
            menu.aiDifficulty = menu.aiDifficulty.next();
            menu.dirty = true;
        }
        MenuAction::NextSeed => {
            menu.mapSeed = menu.mapSeed.wrapping_mul(6364136223846793005).wrapping_add(17) % 99_999 + 1;
            menu.dirty = true;
        }
        MenuAction::StartMatch => {
            net.disconnect(&mut netManaged, &mut control);
            if let Some(c) = campState.campaign.as_mut() {
                c.activeBattleProvince = None;
            }
            let data = GameData::loadDefault();
            let p0 = data.nations[menu.playerNationIdx % data.nations.len()].id.clone();
            let p1 = data.nations[menu.enemyNationIdx % data.nations.len()].id.clone();
            let map = MapGrid::generate(MAP_SIZE, MAP_SIZE, menu.mapSeed);
            sim.0 = SimWorld::newMatchWithAi(map, data, &p0, &p1, menu.aiDifficulty);
            queue.0.clear();
            control.localPlayer = PlayerId(0);
            reset.0 += 1;
            menu.resumePlaying();
            toast.show(format!(
                "Partida iniciada: {} vs {} (IA {})",
                p0.to_uppercase(),
                p1.to_uppercase(),
                menu.aiDifficulty.label()
            ));
        }
        MenuAction::OpenCampaign => {
            net.disconnect(&mut netManaged, &mut control);
            let p0 = sim.0.data.nations[menu.playerNationIdx % nationCount].id.clone();
            campState.openOrCreate(&p0);
            menu.openCampaignMap();
        }
        MenuAction::HostMultiplayer => {
            let p0 = sim.0.data.nations[menu.playerNationIdx % nationCount].id.clone();
            let p1 = sim.0.data.nations[menu.enemyNationIdx % nationCount].id.clone();
            let bind = std::env::var("ROE_BIND")
                .unwrap_or_else(|_| format!("0.0.0.0:{}", DEFAULT_NET_PORT));
            let hs = MatchHandshake { mapSeed: menu.mapSeed, hostNation: p0, clientNation: p1 };
            match net.startHost(&bind, hs) {
                Ok(()) => {
                    menu.openNetLobby();
                    toast.show(format!("Hospedando Lockstep TCP em {bind}"));
                }
                Err(e) => toast.show(format!("Erro ao hospedar: {e}")),
            }
        }
        MenuAction::JoinMultiplayer => {
            let addr = std::env::var("ROE_JOIN")
                .unwrap_or_else(|_| format!("127.0.0.1:{}", DEFAULT_NET_PORT));
            match net.startClient(&addr) {
                Ok(()) => {
                    menu.openNetLobby();
                    toast.show(format!("Conectando a {addr}..."));
                }
                Err(e) => toast.show(format!("Falha ao conectar em {addr}: {e}")),
            }
        }
        MenuAction::CancelNetLobby => {
            net.disconnect(&mut netManaged, &mut control);
            menu.openMainMenu();
        }
        MenuAction::ResumeMatch => {
            menu.resumePlaying();
        }
        MenuAction::SaveGame => {
            performSave(&sim.0, &mut bank, &mut toast, now);
            menu.dirty = true;
        }
        MenuAction::LoadGame => {
            net.disconnect(&mut netManaged, &mut control);
            if performLoad(&mut sim.0, &mut queue, &mut reset, &mut bank, &mut toast, now) {
                menu.resumePlaying();
            }
        }
        MenuAction::ReturnToMainMenu => {
            net.disconnect(&mut netManaged, &mut control);
            if let Some(c) = campState.campaign.as_mut() {
                c.activeBattleProvince = None;
            }
            menu.openMainMenu();
        }
        MenuAction::QuitGame => {
            exit.write(AppExit::Success);
        }
    }
}

pub fn performSave(
    world: &SimWorld,
    bank: &mut SoundBank,
    toast: &mut ToastNotification,
    nowSecs: f32,
) {
    let path = quicksavePath();
    match saveWorldToFile(world, &path) {
        Ok(()) => {
            bank.play(Sfx::Complete, nowSecs);
            toast.show(format!("Partida salva (tick {})", world.tick));
        }
        Err(err) => {
            toast.show(format!("Erro ao salvar: {}", err));
        }
    }
}

fn performLoad(
    world: &mut SimWorld,
    queue: &mut CommandQueue,
    reset: &mut WorldResetCounter,
    bank: &mut SoundBank,
    toast: &mut ToastNotification,
    nowSecs: f32,
) -> bool {
    let path = quicksavePath();
    if !path.exists() {
        toast.show("Nenhum save encontrado");
        return false;
    }
    match loadWorldFromFile(&path, GameData::loadDefault()) {
        Ok(loaded) => {
            let tick = loaded.tick;
            *world = loaded;
            queue.0.clear();
            reset.0 += 1;
            bank.play(Sfx::AgeUp, nowSecs);
            toast.show(format!("Save carregado com sucesso (tick {})!", tick));
            true
        }
        Err(err) => {
            toast.show(format!("Erro ao carregar save: {}", err));
            false
        }
    }
}

fn rebuildMenuUi(
    mut commands: Commands,
    sim: Res<SimState>,
    net: Res<NetPlayState>,
    mut menu: ResMut<AppMenuState>,
    rootQuery: Single<(Entity, &mut Node), With<MenuOverlayRoot>>,
) {
    let lobbyUpdate = menu.mode == ScreenMode::NetLobby && net.is_changed();
    if !menu.dirty && !lobbyUpdate {
        return;
    }
    menu.dirty = false;

    let (rootEntity, mut rootNode) = rootQuery.into_inner();
    commands.entity(rootEntity).despawn_children();

    if matches!(menu.mode, ScreenMode::Playing | ScreenMode::CampaignMap) {
        rootNode.display = Display::None;
        return;
    }
    rootNode.display = Display::Flex;

    let data = &sim.0.data;
    let pNation = &data.nations[menu.playerNationIdx % data.nations.len()];
    let eNation = &data.nations[menu.enemyNationIdx % data.nations.len()];
    let hasSave = quicksavePath().exists();
    let joinAddr = std::env::var("ROE_JOIN")
        .unwrap_or_else(|_| format!("127.0.0.1:{}", DEFAULT_NET_PORT));

    commands.entity(rootEntity).with_children(|overlay| {
        overlay
            .spawn((
                Node {
                    width: px(500),
                    padding: UiRect::all(px(22)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(9),
                    border: UiRect::all(px(2.0)),
                    ..default()
                },
                BackgroundColor(CARD_BG),
                BorderColor::all(GOLD_BORDER),
            ))
            .with_children(|card| {
                match menu.mode {
                    ScreenMode::MainMenu => {
                        card.spawn((
                            Text::new("RISE OF EMPIRES"),
                            TextFont { font_size: FontSize::Px(28.0), ..default() },
                            TextColor(Color::srgb(0.96, 0.84, 0.42)),
                        ));
                        card.spawn((
                            Text::new("RTS Historico  |  Terra, Mar, Ar, Campanha e Lockstep TCP"),
                            TextFont { font_size: FontSize::Px(13.0), ..default() },
                            TextColor(Color::srgb(0.76, 0.80, 0.86)),
                        ));

                        spawnMenuButton(
                            card,
                            &format!("Sua Civilizacao: {}  (clique para trocar)", pNation.name),
                            MenuAction::CyclePlayerNation,
                            false,
                        );
                        card.spawn((
                            Text::new(format!("  Bonus: {}", pNation.summary)),
                            TextFont { font_size: FontSize::Px(12.0), ..default() },
                            TextColor(Color::srgb(0.88, 0.86, 0.72)),
                        ));

                        spawnMenuButton(
                            card,
                            &format!("Civilizacao Oponente: {}  (clique para trocar)", eNation.name),
                            MenuAction::CycleEnemyNation,
                            false,
                        );
                        spawnMenuButton(
                            card,
                            &format!("Dificuldade da IA: {}", menu.aiDifficulty.label()),
                            MenuAction::CycleDifficulty,
                            false,
                        );
                        spawnMenuButton(
                            card,
                            &format!("Semente do Mapa: {}  (gerar outra)", menu.mapSeed),
                            MenuAction::NextSeed,
                            false,
                        );

                        spawnMenuButton(card, "INICIAR PARTIDA RAPIDA", MenuAction::StartMatch, true);
                        spawnMenuButton(
                            card,
                            "CAMPANHA DE CONQUISTA (MAPA 8 PROVINCIAS)",
                            MenuAction::OpenCampaign,
                            true,
                        );
                        card.spawn(Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: px(8),
                            ..default()
                        })
                        .with_children(|row| {
                            spawnMenuButtonFlex(
                                row,
                                &format!("HOSPEDAR LAN (:{})", DEFAULT_NET_PORT),
                                MenuAction::HostMultiplayer,
                            );
                            spawnMenuButtonFlex(
                                row,
                                &format!("CONECTAR ({joinAddr})"),
                                MenuAction::JoinMultiplayer,
                            );
                        });
                        if hasSave {
                            spawnMenuButton(
                                card,
                                "CARREGAR ULTIMO SAVE (F9)",
                                MenuAction::LoadGame,
                                false,
                            );
                        }
                        spawnMenuButton(card, "SAIR", MenuAction::QuitGame, false);
                    }
                    ScreenMode::NetLobby => {
                        card.spawn((
                            Text::new("MULTIPLAYER LOCKSTEP (TCP 1v1)"),
                            TextFont { font_size: FontSize::Px(22.0), ..default() },
                            TextColor(Color::srgb(0.96, 0.84, 0.42)),
                        ));
                        card.spawn((
                            Text::new(net.statusText.clone()),
                            TextFont { font_size: FontSize::Px(14.0), ..default() },
                            TextColor(Color::srgb(0.86, 0.94, 0.98)),
                        ));
                        card.spawn((
                            Text::new(format!(
                                "Dica: o segundo jogador pode clicar em 'CONECTAR' ou definir ROE_JOIN=IP:{}",
                                DEFAULT_NET_PORT
                            )),
                            TextFont { font_size: FontSize::Px(12.0), ..default() },
                            TextColor(Color::srgb(0.72, 0.78, 0.86)),
                        ));
                        spawnMenuButton(
                            card,
                            "CANCELAR E VOLTAR AO MENU",
                            MenuAction::CancelNetLobby,
                            false,
                        );
                    }
                    ScreenMode::Paused => {
                        card.spawn((
                            Text::new("JOGO PAUSADO"),
                            TextFont { font_size: FontSize::Px(24.0), ..default() },
                            TextColor(Color::srgb(0.96, 0.84, 0.42)),
                        ));
                        card.spawn((
                            Text::new(format!(
                                "Tick {}  |  Checksum {:016x}",
                                sim.0.tick,
                                sim.0.checksum()
                            )),
                            TextFont { font_size: FontSize::Px(12.0), ..default() },
                            TextColor(Color::srgb(0.74, 0.78, 0.84)),
                        ));
                        spawnMenuButton(card, "CONTINUAR PARTIDA", MenuAction::ResumeMatch, true);
                        spawnMenuButton(card, "SALVAR PARTIDA (F5)", MenuAction::SaveGame, false);
                        if hasSave {
                            spawnMenuButton(
                                card,
                                "CARREGAR ULTIMO SAVE (F9)",
                                MenuAction::LoadGame,
                                false,
                            );
                        }
                        spawnMenuButton(
                            card,
                            "VOLTAR AO MENU PRINCIPAL",
                            MenuAction::ReturnToMainMenu,
                            false,
                        );
                        spawnMenuButton(card, "SAIR DO JOGO", MenuAction::QuitGame, false);
                    }
                    ScreenMode::Playing | ScreenMode::CampaignMap => {}
                }
            });
    });
}

fn spawnMenuButtonFlex(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    action: MenuAction,
) {
    parent
        .spawn((
            Hovered::default(),
            action,
            Node {
                flex_grow: 1.0,
                padding: UiRect::axes(px(10), px(9)),
                border: UiRect::all(px(1.5)),
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(BTN_BG),
            BorderColor::all(GOLD_BORDER),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                TextFont { font_size: FontSize::Px(13.0), ..default() },
                TextColor(Color::srgb(0.96, 0.96, 0.98)),
            ));
        });
}

fn spawnMenuButton(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    action: MenuAction,
    primary: bool,
) {
    parent
        .spawn((
            Hovered::default(),
            action,
            Node {
                padding: UiRect::axes(px(12), px(9)),
                border: UiRect::all(px(1.5)),
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(if primary { BTN_PRIMARY } else { BTN_BG }),
            BorderColor::all(GOLD_BORDER),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                TextFont { font_size: FontSize::Px(14.0), ..default() },
                TextColor(Color::srgb(0.96, 0.96, 0.98)),
            ));
        });
}
