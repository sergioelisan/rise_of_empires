use std::path::PathBuf;

use bevy::picking::events::{PointerOut, PointerOver, PointerPress};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use roeRender::{
    CommandQueue, HudVisible, PlayerControlState, SimPaused, SimState, WorldResetCounter,
};
use roeSim::{
    AiDifficulty, GameData, MapGrid, SimWorld, loadWorldFromFile, saveWorldToFile,
};

use crate::audio::{Sfx, SoundBank, ToastNotification};

const MAP_SIZE: u32 = 64;
const QUICKSAVE_PATH: &str = "saves/quicksave.ron";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenMode {
    MainMenu,
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
        startInMenu: bool,
        playerNationIdx: usize,
        enemyNationIdx: usize,
        aiDifficulty: AiDifficulty,
        mapSeed: u64,
    ) -> Self {
        Self {
            mode: if startInMenu { ScreenMode::MainMenu } else { ScreenMode::Playing },
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
    hudVisible.0 = menu.mode != ScreenMode::MainMenu;
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
    control: Res<PlayerControlState>,
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
            ScreenMode::MainMenu => {}
        }
    }

    if keys.just_pressed(KeyCode::Escape) {
        if menu.mode == ScreenMode::Paused {
            menu.resumePlaying();
            bank.play(Sfx::Click, now);
        } else if menu.mode == ScreenMode::Playing
            && control.placingBuilding.is_none()
            && control.selectedUnits.is_empty()
            && control.selectedBuilding.is_none()
        {
            menu.openPause();
            bank.play(Sfx::Click, now);
        }
    }

    if keys.just_pressed(KeyCode::F5) && menu.mode != ScreenMode::MainMenu {
        performSave(&sim.0, &mut bank, &mut toast, now);
    }

    if keys.just_pressed(KeyCode::F9) {
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
        let base = if matches!(action, MenuAction::StartMatch | MenuAction::ResumeMatch) {
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
            let data = GameData::loadDefault();
            let p0 = data.nations[menu.playerNationIdx % data.nations.len()].id.clone();
            let p1 = data.nations[menu.enemyNationIdx % data.nations.len()].id.clone();
            let map = MapGrid::generate(MAP_SIZE, MAP_SIZE, menu.mapSeed);
            sim.0 = SimWorld::newMatchWithAi(map, data, &p0, &p1, menu.aiDifficulty);
            queue.0.clear();
            reset.0 += 1;
            menu.resumePlaying();
            toast.show(format!(
                "Partida iniciada: {} vs {} (IA {})",
                p0.to_uppercase(),
                p1.to_uppercase(),
                menu.aiDifficulty.label()
            ));
        }
        MenuAction::ResumeMatch => {
            menu.resumePlaying();
        }
        MenuAction::SaveGame => {
            performSave(&sim.0, &mut bank, &mut toast, now);
            menu.dirty = true;
        }
        MenuAction::LoadGame => {
            if performLoad(&mut sim.0, &mut queue, &mut reset, &mut bank, &mut toast, now) {
                menu.resumePlaying();
            }
        }
        MenuAction::ReturnToMainMenu => {
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
    match saveWorldToFile(world, &PathBuf::from(QUICKSAVE_PATH)) {
        Ok(()) => {
            bank.play(Sfx::Complete, nowSecs);
            toast.show(format!("Partida salva em {} (tick {})", QUICKSAVE_PATH, world.tick));
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
    let path = PathBuf::from(QUICKSAVE_PATH);
    if !path.exists() {
        toast.show("Nenhum save encontrado em saves/quicksave.ron");
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
    mut menu: ResMut<AppMenuState>,
    rootQuery: Single<(Entity, &mut Node), With<MenuOverlayRoot>>,
) {
    if !menu.dirty {
        return;
    }
    menu.dirty = false;

    let (rootEntity, mut rootNode) = rootQuery.into_inner();
    commands.entity(rootEntity).despawn_children();

    if menu.mode == ScreenMode::Playing {
        rootNode.display = Display::None;
        return;
    }
    rootNode.display = Display::Flex;

    let data = &sim.0.data;
    let pNation = &data.nations[menu.playerNationIdx % data.nations.len()];
    let eNation = &data.nations[menu.enemyNationIdx % data.nations.len()];
    let hasSave = PathBuf::from(QUICKSAVE_PATH).exists();

    commands.entity(rootEntity).with_children(|overlay| {
        overlay
            .spawn((
                Node {
                    width: px(460),
                    padding: UiRect::all(px(22)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(10),
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
                            Text::new("RTS Historico  |  Escolha sua civilizacao e inicie a conquista"),
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

                        spawnMenuButton(card, "INICIAR PARTIDA", MenuAction::StartMatch, true);
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
                    ScreenMode::Playing => {}
                }
            });
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
