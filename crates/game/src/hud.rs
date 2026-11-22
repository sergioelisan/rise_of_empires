use bevy::picking::events::{PointerOut, PointerOver, PointerPress};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use roeRender::{CommandMode, CommandQueue, HudVisible, IsoCamera, PlayerControlState, SimState};
use roeSim::{Command, FormationKind, PlayerId, ResourceBundle, TechBranch, UnitClass};

use crate::audio::{Sfx, SoundBank, ToastNotification};
use crate::icons::IconAtlas;
use crate::menu::{AppMenuState, performSave};

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HudUiState>()
            .init_resource::<ActiveTooltip>()
            .add_systems(Startup, (initIcons, spawnHudLayout).chain())
            .add_observer(onHudButtonPress)
            .add_observer(onHudButtonOver)
            .add_observer(onHudButtonOut)
            .add_systems(
                Update,
                (
                    syncHudVisibility,
                    handleHudHotkeys,
                    blockPointerOverHud,
                    refreshTopBar,
                    refreshSelectionPanel,
                    refreshDrawers,
                    refreshToastBanner,
                    refreshTooltipCard,
                )
                    .chain(),
            );
    }
}

fn initIcons(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let atlas = IconAtlas::build(&mut images);
    commands.insert_resource(atlas);
}

#[derive(Resource, Default)]
struct HudUiState {
    showTechDrawer: bool,
    showWonderDrawer: bool,
    lastActionSig: String,
    lastDrawerSig: String,
}

#[derive(Resource, Default)]
struct ActiveTooltip {
    title: String,
    body: String,
    visible: bool,
}

#[derive(Component, Clone)]
struct ButtonTooltip {
    title: String,
    body: String,
}

#[derive(Component)]
struct HudRootBar;

#[derive(Component)]
struct TopInfoText;

#[derive(Component)]
struct ResFoodText;
#[derive(Component)]
struct ResWoodText;
#[derive(Component)]
struct ResMetalText;
#[derive(Component)]
struct ResWealthText;
#[derive(Component)]
struct ResKnowledgeText;
#[derive(Component)]
struct ResCapText;

#[derive(Component)]
struct SelectionInfoText;

#[derive(Component)]
struct ActionBarRoot;

#[derive(Component)]
struct DrawerRoot;

#[derive(Component)]
struct VictoryBanner;

#[derive(Component)]
struct ToastBanner;

#[derive(Component)]
struct TooltipCard;

#[derive(Component)]
struct TooltipTitleText;

#[derive(Component)]
struct TooltipBodyText;

#[derive(Component, Clone)]
enum HudAction {
    ToggleTech,
    ToggleWonders,
    ToggleFog,
    ToggleMute,
    QuickSave,
    OpenPauseMenu,
    FocusHome,
    FocusEnemy,
    EnterCommandMode(CommandMode),
    FocusBuildMenu,
    StartBuild { kind: String, wonderId: Option<String> },
    TrainUnit { building: roeSim::BuildingId, kind: String },
    SetFormation(FormationKind),
    ResearchTech(String),
    AdvanceAge,
}

const PANEL_BG: Color = Color::srgba(0.08, 0.10, 0.14, 0.92);
const PANEL_BORDER: Color = Color::srgba(0.76, 0.63, 0.30, 0.80);
const BTN_NORMAL: Color = Color::srgba(0.16, 0.20, 0.27, 0.96);
const BTN_HOVER: Color = Color::srgba(0.25, 0.31, 0.42, 0.98);
const BTN_ACTIVE: Color = Color::srgba(0.20, 0.44, 0.28, 0.98);
const BTN_DISABLED: Color = Color::srgba(0.12, 0.13, 0.16, 0.78);

fn spawnHudLayout(mut commands: Commands, icons: Res<IconAtlas>) {
    // --- Barra superior ---
    commands
        .spawn((
            HudRootBar,
            Node {
                position_type: PositionType::Absolute,
                top: px(0),
                left: px(0),
                right: px(0),
                padding: UiRect::axes(px(10), px(6)),
                flex_direction: FlexDirection::Column,
                row_gap: px(5),
                border: UiRect::bottom(px(1.5)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
            BorderColor::all(PANEL_BORDER),
            ZIndex(20),
        ))
        .with_children(|bar| {
            bar.spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                column_gap: px(6),
                ..default()
            })
            .with_children(|row| {
                row.spawn((
                    TopInfoText,
                    Text::new(""),
                    TextFont { font_size: FontSize::Px(12.5), ..default() },
                    TextColor(Color::srgb(0.95, 0.90, 0.72)),
                ));

                row.spawn(Node { column_gap: px(5), ..default() }).with_children(|btns| {
                    spawnSmallButton(btns, "[T] Pesquisas", None, HudAction::ToggleTech, true, None);
                    spawnSmallButton(
                        btns,
                        "[W] Maravilhas",
                        Some(icons.building("wonder")),
                        HudAction::ToggleWonders,
                        true,
                        None,
                    );
                    spawnSmallButton(btns, "[F] Nevoa", None, HudAction::ToggleFog, true, None);
                    spawnSmallButton(btns, "[M] Som", None, HudAction::ToggleMute, true, None);
                    spawnSmallButton(btns, "[H] Cap", None, HudAction::FocusHome, true, None);
                    spawnSmallButton(btns, "[E] Inim", None, HudAction::FocusEnemy, true, None);
                    spawnSmallButton(btns, "[F5] Salvar", None, HudAction::QuickSave, true, None);
                    spawnSmallButton(btns, "[Menu]", None, HudAction::OpenPauseMenu, true, None);
                });
            });

            // Linha 2: Ícones dos 5 recursos (maçã, tábua, lingote, moedas, livro) + valores/taxas
            bar.spawn(Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(16),
                ..default()
            })
            .with_children(|resRow| {
                spawnResourcePill(resRow, icons.food.clone(), ResFoodText);
                spawnResourcePill(resRow, icons.wood.clone(), ResWoodText);
                spawnResourcePill(resRow, icons.metal.clone(), ResMetalText);
                spawnResourcePill(resRow, icons.wealth.clone(), ResWealthText);
                spawnResourcePill(resRow, icons.knowledge.clone(), ResKnowledgeText);
                resRow.spawn((
                    ResCapText,
                    Text::new(""),
                    TextFont { font_size: FontSize::Px(12.5), ..default() },
                    TextColor(Color::srgb(0.84, 0.88, 0.94)),
                ));
            });
        });

    // --- Toast de notificações ---
    commands.spawn((
        ToastBanner,
        Text::new(""),
        TextFont { font_size: FontSize::Px(14.0), ..default() },
        TextColor(Color::srgb(0.98, 0.92, 0.62)),
        Node {
            position_type: PositionType::Absolute,
            top: px(72),
            left: px(16),
            padding: UiRect::axes(px(12), px(6)),
            display: Display::None,
            border: UiRect::all(px(1.5)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.08, 0.11, 0.16, 0.92)),
        BorderColor::all(PANEL_BORDER),
        ZIndex(45),
    ));

    // --- Banner de Vitória / Derrota ---
    commands.spawn((
        VictoryBanner,
        Text::new(""),
        TextFont { font_size: FontSize::Px(22.0), ..default() },
        TextColor(Color::srgb(1.0, 0.88, 0.34)),
        Node {
            position_type: PositionType::Absolute,
            top: px(112),
            left: px(16),
            padding: UiRect::axes(px(14), px(8)),
            display: Display::None,
            border: UiRect::all(px(2.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.10, 0.14, 0.10, 0.94)),
        BorderColor::all(PANEL_BORDER),
        ZIndex(40),
    ));

    // --- Gaveta lateral (Pesquisas / Era OU Maravilhas) ---
    commands.spawn((
        DrawerRoot,
        Node {
            position_type: PositionType::Absolute,
            top: px(70),
            right: px(10),
            width: px(380),
            padding: UiRect::all(px(10)),
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            border: UiRect::all(px(1.5)),
            display: Display::None,
            ..default()
        },
        BackgroundColor(PANEL_BG),
        BorderColor::all(PANEL_BORDER),
        ZIndex(30),
    ));

    // --- Card de Tooltip flutuante ao passar o mouse sobre edificações e ações ---
    commands
        .spawn((
            TooltipCard,
            Node {
                position_type: PositionType::Absolute,
                bottom: px(104),
                right: px(12),
                width: px(360),
                padding: UiRect::all(px(10)),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                border: UiRect::all(px(1.5)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.06, 0.08, 0.12, 0.96)),
            BorderColor::all(Color::srgba(0.88, 0.74, 0.34, 0.92)),
            ZIndex(55),
        ))
        .with_children(|card| {
            card.spawn((
                TooltipTitleText,
                Text::new(""),
                TextFont { font_size: FontSize::Px(14.0), ..default() },
                TextColor(Color::srgb(0.98, 0.86, 0.44)),
            ));
            card.spawn((
                TooltipBodyText,
                Text::new(""),
                TextFont { font_size: FontSize::Px(12.0), ..default() },
                TextColor(Color::srgb(0.90, 0.92, 0.96)),
            ));
        });

    // --- Painel Inferior: Seleção + Ações/Construções com ícones ---
    commands
        .spawn((
            HudRootBar,
            Node {
                position_type: PositionType::Absolute,
                bottom: px(0),
                left: px(0),
                right: px(0),
                padding: UiRect::axes(px(10), px(7)),
                flex_direction: FlexDirection::Row,
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::FlexStart,
                column_gap: px(12),
                border: UiRect::top(px(1.5)),
                ..default()
            },
            BackgroundColor(PANEL_BG),
            BorderColor::all(PANEL_BORDER),
            ZIndex(20),
        ))
        .with_children(|bottom| {
            bottom.spawn((
                SelectionInfoText,
                Text::new(""),
                TextFont { font_size: FontSize::Px(12.0), ..default() },
                TextColor(Color::srgb(0.90, 0.92, 0.94)),
                Node { min_width: px(290), max_width: px(370), ..default() },
            ));

            bottom.spawn((
                ActionBarRoot,
                Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: px(5),
                    row_gap: px(5),
                    justify_content: JustifyContent::FlexEnd,
                    flex_grow: 1.0,
                    ..default()
                },
            ));
        });
}

fn spawnResourcePill<T: Component>(
    parent: &mut ChildSpawnerCommands,
    icon: Handle<Image>,
    marker: T,
) {
    parent
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(5),
            ..default()
        })
        .with_children(|pill| {
            pill.spawn((
                ImageNode::new(icon),
                Node { width: px(18), height: px(18), ..default() },
            ));
            pill.spawn((
                marker,
                Text::new(""),
                TextFont { font_size: FontSize::Px(12.5), ..default() },
                TextColor(Color::srgb(0.92, 0.95, 0.98)),
            ));
        });
}

fn spawnSmallButton(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    icon: Option<Handle<Image>>,
    action: HudAction,
    enabled: bool,
    tooltip: Option<ButtonTooltip>,
) {
    let mut entityCmd = parent.spawn((
        Hovered::default(),
        action,
        Node {
            padding: UiRect::axes(px(7), px(4)),
            border: UiRect::all(px(1.0)),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(5),
            ..default()
        },
        BackgroundColor(if enabled { BTN_NORMAL } else { BTN_DISABLED }),
        BorderColor::all(if enabled {
            PANEL_BORDER
        } else {
            Color::srgba(0.45, 0.35, 0.35, 0.55)
        }),
    ));
    if let Some(tip) = tooltip {
        entityCmd.insert(tip);
    }
    entityCmd.with_children(|b| {
        if let Some(handle) = icon {
            b.spawn((
                ImageNode::new(handle),
                Node { width: px(18), height: px(18), ..default() },
            ));
        }
        b.spawn((
            Text::new(label),
            TextFont { font_size: FontSize::Px(11.5), ..default() },
            TextColor(if enabled {
                Color::srgb(0.95, 0.95, 0.96)
            } else {
                Color::srgb(0.62, 0.56, 0.56)
            }),
        ));
    });
}

fn syncHudVisibility(
    hudVisible: Res<HudVisible>,
    mut bars: Query<&mut Node, With<HudRootBar>>,
    mut ui: ResMut<HudUiState>,
    mut tooltip: ResMut<ActiveTooltip>,
) {
    let display = if hudVisible.0 { Display::Flex } else { Display::None };
    for mut node in &mut bars {
        node.display = display;
    }
    if !hudVisible.0 {
        ui.showTechDrawer = false;
        ui.showWonderDrawer = false;
        tooltip.visible = false;
    }
}

fn handleHudHotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    hudVisible: Res<HudVisible>,
    time: Res<Time>,
    mut ui: ResMut<HudUiState>,
    mut bank: ResMut<SoundBank>,
) {
    if !hudVisible.0 {
        return;
    }
    if keys.just_pressed(KeyCode::KeyT) {
        ui.showTechDrawer = !ui.showTechDrawer;
        if ui.showTechDrawer {
            ui.showWonderDrawer = false;
        }
        ui.lastDrawerSig.clear();
        bank.play(Sfx::Click, time.elapsed_secs());
    }
    if keys.just_pressed(KeyCode::KeyW) {
        ui.showWonderDrawer = !ui.showWonderDrawer;
        if ui.showWonderDrawer {
            ui.showTechDrawer = false;
        }
        ui.lastDrawerSig.clear();
        bank.play(Sfx::Click, time.elapsed_secs());
    }
}

fn blockPointerOverHud(
    mut control: ResMut<PlayerControlState>,
    hovered: Query<&Hovered, With<HudAction>>,
) {
    if hovered.iter().any(|h| h.get()) {
        control.uiConsumedPointer = true;
    }
}

fn onHudButtonOver(
    over: On<PointerOver>,
    mut tooltip: ResMut<ActiveTooltip>,
    mut query: Query<(&mut BackgroundColor, Option<&ButtonTooltip>), With<HudAction>>,
) {
    if let Ok((mut bg, tip)) = query.get_mut(over.entity) {
        *bg = BackgroundColor(BTN_HOVER);
        if let Some(tip) = tip {
            tooltip.title = tip.title.clone();
            tooltip.body = tip.body.clone();
            tooltip.visible = true;
        }
    }
}

fn onHudButtonOut(
    out: On<PointerOut>,
    mut tooltip: ResMut<ActiveTooltip>,
    mut query: Query<(&mut BackgroundColor, Option<&ButtonTooltip>), With<HudAction>>,
) {
    if let Ok((mut bg, tip)) = query.get_mut(out.entity) {
        *bg = BackgroundColor(BTN_NORMAL);
        if tip.is_some() {
            tooltip.visible = false;
        }
    }
}

fn onHudButtonPress(
    mut press: On<PointerPress>,
    time: Res<Time>,
    sim: Res<SimState>,
    mut ui: ResMut<HudUiState>,
    mut menu: ResMut<AppMenuState>,
    mut control: ResMut<PlayerControlState>,
    mut queue: ResMut<CommandQueue>,
    mut bank: ResMut<SoundBank>,
    mut toast: ResMut<ToastNotification>,
    mut tooltip: ResMut<ActiveTooltip>,
    mut camera: Single<&mut IsoCamera>,
    mut query: Query<(&HudAction, &mut BackgroundColor)>,
) {
    let Ok((action, mut bg)) = query.get_mut(press.entity) else { return };
    press.propagate(false);
    *bg = BackgroundColor(BTN_ACTIVE);
    control.uiConsumedPointer = true;
    let now = time.elapsed_secs();

    match action {
        HudAction::ToggleTech => {
            ui.showTechDrawer = !ui.showTechDrawer;
            if ui.showTechDrawer {
                ui.showWonderDrawer = false;
            }
            ui.lastDrawerSig.clear();
            bank.play(Sfx::Click, now);
        }
        HudAction::ToggleWonders => {
            ui.showWonderDrawer = !ui.showWonderDrawer;
            if ui.showWonderDrawer {
                ui.showTechDrawer = false;
            }
            ui.lastDrawerSig.clear();
            bank.play(Sfx::Click, now);
        }
        HudAction::ToggleFog => {
            control.showFog = !control.showFog;
            bank.play(Sfx::Click, now);
        }
        HudAction::ToggleMute => {
            bank.muted = !bank.muted;
            if !bank.muted {
                bank.play(Sfx::Click, now);
                toast.show("Som: ATIVADO");
            } else {
                toast.show("Som: MUDO");
            }
        }
        HudAction::QuickSave => {
            performSave(&sim.0, &mut bank, &mut toast, now);
        }
        HudAction::OpenPauseMenu => {
            menu.openPause();
            bank.play(Sfx::Click, now);
        }
        HudAction::FocusHome => {
            if let Some(c) = sim.0.cities.iter().find(|c| c.owner == control.localPlayer) {
                camera.focus = Vec3::new(c.centerTile.0 as f32 + 0.5, 0.0, c.centerTile.1 as f32 + 0.5);
            }
            bank.play(Sfx::Click, now);
        }
        HudAction::FocusEnemy => {
            if let Some(c) = sim.0.cities.iter().find(|c| c.owner != control.localPlayer) {
                camera.focus = Vec3::new(c.centerTile.0 as f32 + 0.5, 0.0, c.centerTile.1 as f32 + 0.5);
            }
            bank.play(Sfx::Click, now);
        }
        HudAction::EnterCommandMode(mode) => {
            control.placingBuilding = None;
            control.commandMode = *mode;
            bank.play(Sfx::Click, now);
            let msg = match mode {
                CommandMode::Move => "Modo MOVER: clique esquerdo no destino no mapa.",
                CommandMode::Attack => "Modo ATACAR: clique esquerdo no alvo inimigo.",
                CommandMode::Repair => "Modo REFORMAR: clique esquerdo numa construcao aliada danificada.",
                CommandMode::Default => "",
            };
            if !msg.is_empty() {
                toast.show(msg);
            }
        }
        HudAction::FocusBuildMenu => {
            control.selectedBuilding = None;
            control.commandMode = CommandMode::Default;
            ui.lastActionSig.clear();
            bank.play(Sfx::Click, now);
            toast.show("Escolha uma edificacao abaixo para construir.");
        }
        HudAction::StartBuild { kind, wonderId } => {
            control.commandMode = CommandMode::Default;
            control.placingBuilding = Some((kind.clone(), wonderId.clone()));
            ui.showWonderDrawer = false;
            ui.lastDrawerSig.clear();
            tooltip.visible = false;
            bank.play(Sfx::Build, now);
        }
        HudAction::TrainUnit { building, kind } => {
            queue.0.push(Command::TrainUnit {
                owner: control.localPlayer,
                building: *building,
                unitKind: kind.clone(),
            });
            bank.play(Sfx::Order, now);
        }
        HudAction::SetFormation(form) => {
            control.activeFormation = *form;
            if !control.selectedUnits.is_empty() {
                queue.0.push(Command::SetFormation {
                    owner: control.localPlayer,
                    units: control.selectedUnits.clone(),
                    formation: *form,
                });
            }
            bank.play(Sfx::Order, now);
        }
        HudAction::ResearchTech(techId) => {
            queue.0.push(Command::ResearchTech {
                owner: control.localPlayer,
                techId: techId.clone(),
            });
            ui.lastDrawerSig.clear();
            bank.play(Sfx::Click, now);
        }
        HudAction::AdvanceAge => {
            queue.0.push(Command::AdvanceAge { owner: control.localPlayer });
            ui.lastDrawerSig.clear();
            bank.play(Sfx::Click, now);
        }
    }
}

fn refreshTopBar(
    sim: Res<SimState>,
    control: Res<PlayerControlState>,
    bank: Res<SoundBank>,
    mut topInfo: Single<&mut Text, (With<TopInfoText>, Without<VictoryBanner>, Without<ResFoodText>, Without<ResWoodText>, Without<ResMetalText>, Without<ResWealthText>, Without<ResKnowledgeText>, Without<ResCapText>)>,
    mut foodText: Single<&mut Text, (With<ResFoodText>, Without<TopInfoText>, Without<VictoryBanner>, Without<ResWoodText>, Without<ResMetalText>, Without<ResWealthText>, Without<ResKnowledgeText>, Without<ResCapText>)>,
    mut woodText: Single<&mut Text, (With<ResWoodText>, Without<TopInfoText>, Without<VictoryBanner>, Without<ResFoodText>, Without<ResMetalText>, Without<ResWealthText>, Without<ResKnowledgeText>, Without<ResCapText>)>,
    mut metalText: Single<&mut Text, (With<ResMetalText>, Without<TopInfoText>, Without<VictoryBanner>, Without<ResFoodText>, Without<ResWoodText>, Without<ResWealthText>, Without<ResKnowledgeText>, Without<ResCapText>)>,
    mut wealthText: Single<&mut Text, (With<ResWealthText>, Without<TopInfoText>, Without<VictoryBanner>, Without<ResFoodText>, Without<ResWoodText>, Without<ResMetalText>, Without<ResKnowledgeText>, Without<ResCapText>)>,
    mut knowText: Single<&mut Text, (With<ResKnowledgeText>, Without<TopInfoText>, Without<VictoryBanner>, Without<ResFoodText>, Without<ResWoodText>, Without<ResMetalText>, Without<ResWealthText>, Without<ResCapText>)>,
    mut capText: Single<&mut Text, (With<ResCapText>, Without<TopInfoText>, Without<VictoryBanner>, Without<ResFoodText>, Without<ResWoodText>, Without<ResMetalText>, Without<ResWealthText>, Without<ResKnowledgeText>)>,
    mut banner: Single<(&mut Text, &mut Node), (With<VictoryBanner>, Without<TopInfoText>, Without<ResFoodText>, Without<ResWoodText>, Without<ResMetalText>, Without<ResWealthText>, Without<ResKnowledgeText>, Without<ResCapText>)>,
) {
    let world = &sim.0;
    let Some(p) = world.player(control.localPlayer) else { return };
    let nationName = p.nation(&world.data).map_or("Imperio", |n| n.name.as_str());
    let ageName = world.data.age(p.age).map_or("Era", |a| a.name.as_str());
    let myUnits = world.units.iter().filter(|u| u.owner == control.localPlayer).count();
    let myCities = world.cityCount(control.localPlayer);
    let fogTxt = if control.showFog { "ON" } else { "OFF" };
    let soundTxt = if bank.muted { "OFF" } else { "ON" };
    let aiTag = world
        .player(PlayerId(1))
        .map(|ep| {
            let eName = ep.nation(&world.data).map_or("IA", |n| n.name.as_str());
            let dName = ep.aiDifficulty.map_or("Passiva", |d| d.label());
            format!("vs {} [IA {}]", eName, dName)
        })
        .unwrap_or_default();

    topInfo.0 = format!(
        "{} ({})  |  {}  |  Cid: {}  Unid: {}  Pesq: {}  Marav: {}  |  Nevoa:{} Som:{}",
        nationName,
        ageName,
        aiTag,
        myCities,
        myUnits,
        p.researchedTechs.len(),
        p.wondersBuilt.len(),
        fogTxt,
        soundTxt,
    );

    let r = &p.resources;
    let g = &p.gatherRates;
    foodText.0 = format!("{} (+{})", r.food, g.food);
    woodText.0 = format!("{} (+{})", r.wood, g.wood);
    metalText.0 = format!("{} (+{})", r.metal, g.metal);
    wealthText.0 = format!("{} (+{})", r.wealth, g.wealth);
    knowText.0 = format!("{} (+{})", r.knowledge, g.knowledge);
    capText.0 = format!("[Limite coleta: +{}/min]", p.commerceCap);

    if let Some(winner) = world.winner {
        let winNation = world
            .player(winner)
            .and_then(|wp| wp.nation(&world.data))
            .map_or("Vencedor", |n| n.name.as_str());
        banner.0.0 = if winner == control.localPlayer {
            format!("VITORIA DE {}! Todas as cidades inimigas foram conquistadas.", winNation)
        } else {
            format!("DERROTA! {} conquistou seu imperio.", winNation)
        };
        banner.1.display = Display::Flex;
    } else {
        banner.1.display = Display::None;
    }
}

fn refreshToastBanner(
    toast: Res<ToastNotification>,
    mut banner: Single<(&mut Text, &mut Node), With<ToastBanner>>,
) {
    if toast.remainingSecs > 0.0 && !toast.message.is_empty() {
        banner.0.0 = toast.message.clone();
        banner.1.display = Display::Flex;
    } else {
        banner.1.display = Display::None;
    }
}

fn refreshTooltipCard(
    tooltip: Res<ActiveTooltip>,
    hudVisible: Res<HudVisible>,
    mut cardNode: Single<&mut Node, With<TooltipCard>>,
    mut titleText: Single<&mut Text, (With<TooltipTitleText>, Without<TooltipBodyText>)>,
    mut bodyText: Single<&mut Text, (With<TooltipBodyText>, Without<TooltipTitleText>)>,
) {
    if hudVisible.0 && tooltip.visible {
        cardNode.display = Display::Flex;
        titleText.0 = tooltip.title.clone();
        bodyText.0 = tooltip.body.clone();
    } else {
        cardNode.display = Display::None;
    }
}

fn refreshSelectionPanel(
    mut commands: Commands,
    sim: Res<SimState>,
    control: Res<PlayerControlState>,
    icons: Res<IconAtlas>,
    mut ui: ResMut<HudUiState>,
    mut selText: Single<&mut Text, With<SelectionInfoText>>,
    actionRoot: Single<Entity, With<ActionBarRoot>>,
) {
    let world = &sim.0;
    let Some(player) = world.player(control.localPlayer) else { return };

    if let Some((kind, wonderId)) = &control.placingBuilding {
        let label = wonderId
            .as_deref()
            .and_then(|w| world.data.wonder(w))
            .map(|w| w.name.as_str())
            .or_else(|| world.data.building(kind).map(|b| b.name.as_str()))
            .unwrap_or(kind);
        let extra = if kind == "lumber_camp" {
            if let Some(t) = control.hoveredTile {
                let slots = world.computeWorkerSlots("lumber_camp", t, 2);
                format!("\nVagas estimadas neste local: {} lenhador(es).", slots)
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        selText.0 = format!(
            "CONSTRUINDO: {}{}\nClique esquerdo no terreno verde para posicionar. Direito/ESC cancela.",
            label, extra
        );
    } else if control.commandMode != CommandMode::Default {
        selText.0 = match control.commandMode {
            CommandMode::Move => "MODO MOVER: clique esquerdo no destino no mapa (ESC cancela).".into(),
            CommandMode::Attack => "MODO ATACAR: clique esquerdo no inimigo ou terreno (ESC cancela).".into(),
            CommandMode::Repair => "MODO REFORMAR: clique esquerdo na construcao aliada danificada (ESC cancela).".into(),
            CommandMode::Default => String::new(),
        };
    } else if !control.selectedUnits.is_empty() {
        let mut citizens = 0;
        let mut attritionCount = 0;
        for &id in &control.selectedUnits {
            if let Some(u) = world.unit(id) {
                if u.class == UnitClass::Citizen {
                    citizens += 1;
                }
                if u.inEnemyTerritory {
                    attritionCount += 1;
                }
            }
        }
        let first = world.unit(control.selectedUnits[0]);
        let detail = if control.selectedUnits.len() == 1 {
            if let Some(u) = first {
                format!(
                    "{}  |  HP {}/{}  ATQ {}  ARM {}\nOrdem: {:?}",
                    world.data.unitDisplayName(&u.kind, u.eraTier),
                    u.hp,
                    u.maxHp,
                    u.attack,
                    u.armor,
                    u.order
                )
            } else {
                String::new()
            }
        } else {
            format!(
                "{} unidades selecionadas ({} cidadaos)  |  Form: {}",
                control.selectedUnits.len(),
                citizens,
                control.activeFormation.label()
            )
        };
        let warn = if attritionCount > 0 {
            format!("\nALERTA: {} unidade(s) sofrendo ATRITO em solo inimigo!", attritionCount)
        } else {
            "\nPasse o mouse nos icones para ver detalhes e limites.".to_string()
        };
        selText.0 = format!("{detail}{warn}");
    } else if let Some(bid) = control.selectedBuilding {
        if let Some(b) = world.building(bid) {
            let name = b
                .wonderId
                .as_deref()
                .and_then(|w| world.data.wonder(w))
                .map(|w| w.name.as_str())
                .or_else(|| world.data.building(&b.kind).map(|d| d.name.as_str()))
                .unwrap_or(&b.kind);
            let cityInfo = b
                .cityId
                .and_then(|cid| world.city(cid))
                .map(|c| format!(" ({})", c.name))
                .unwrap_or_default();
            let workersInfo = if b.workerSlots > 0 {
                format!(
                    "  |  Trabalhadores: {}/{}",
                    world.buildingAssignedWorkers(b.id),
                    b.workerSlots
                )
            } else {
                String::new()
            };
            let status = if !b.isComplete() {
                format!("Em construcao {}%", (b.buildProgress * 100) / b.buildRequired.max(1))
            } else if !b.trainQueue.is_empty() {
                format!("Treinando: {} na fila", b.trainQueue.len())
            } else if b.hp < b.maxHp {
                "Danificada (use cidadaos para Reformar)".to_string()
            } else {
                "Operacional".to_string()
            };
            selText.0 = format!(
                "{}{}  |  HP {}/{}{}\nStatus: {}",
                name, cityInfo, b.hp, b.maxHp, workersInfo, status
            );
        }
    } else {
        let nationSum = player.nation(&world.data).map_or("", |n| n.summary.as_str());
        selText.0 = format!(
            "Bonus {}: {}\nPasse o mouse nos icones das edificacoes para ver funcao, vagas e limites.",
            player.nation(&world.data).map_or("", |n| n.name.as_str()),
            nationSum
        );
    }

    let hasCitizen = control
        .selectedUnits
        .iter()
        .any(|id| world.unit(*id).is_some_and(|u| u.class == UnitClass::Citizen));
    let hasMilitary = control
        .selectedUnits
        .iter()
        .any(|id| world.unit(*id).is_some_and(|u| u.class != UnitClass::Citizen));
    let sig = format!(
        "b:{:?}|c:{}|m:{}|a:{}|f:{:?}|bc:{}|cc:{}|cm:{:?}",
        control.selectedBuilding,
        hasCitizen,
        hasMilitary,
        player.age,
        control.activeFormation,
        world.buildings.len(),
        world.cityCount(control.localPlayer),
        control.commandMode,
    );
    if sig == ui.lastActionSig {
        return;
    }
    ui.lastActionSig = sig;

    commands.entity(*actionRoot).despawn_children();
    commands.entity(*actionRoot).with_children(|bar| {
        if let Some(bid) = control.selectedBuilding {
            if let Some(b) = world.building(bid) {
                let trainable = world.data.trainableUnitsFor(&b.kind, &player.nationId, player.age);
                for uDef in trainable {
                    let display = world.data.unitDisplayName(&uDef.id, player.age);
                    let tag = if uDef.nationOnly.is_some() { "* " } else { "" };
                    let label = format!("{}{} ({})", tag, display, shortCost(&uDef.cost));
                    let tip = ButtonTooltip {
                        title: format!("Treinar {}", display),
                        body: format!(
                            "HP: {}  |  Ataque: {}  |  Armadura: {}\nCusto: {}",
                            uDef.hp,
                            uDef.attack,
                            uDef.armor,
                            shortCost(&uDef.cost)
                        ),
                    };
                    spawnSmallButton(
                        bar,
                        &label,
                        Some(if uDef.class == UnitClass::Citizen {
                            icons.actBuild.clone()
                        } else {
                            icons.actAttack.clone()
                        }),
                        HudAction::TrainUnit { building: bid, kind: uDef.id.clone() },
                        true,
                        Some(tip),
                    );
                }
            }
            return;
        }

        // Botões de ação com ícones desenhados (Mover, Atacar, Construir, Reformar).
        if !control.selectedUnits.is_empty() {
            spawnSmallButton(
                bar,
                "Mover",
                Some(icons.actMove.clone()),
                HudAction::EnterCommandMode(CommandMode::Move),
                true,
                Some(ButtonTooltip {
                    title: "Acao: Mover".into(),
                    body: "Ordena o grupo selecionado a marchar em formacao ate o local clicado (ou use clique direito).".into(),
                }),
            );
            if hasMilitary {
                spawnSmallButton(
                    bar,
                    "Atacar",
                    Some(icons.actAttack.clone()),
                    HudAction::EnterCommandMode(CommandMode::Attack),
                    true,
                    Some(ButtonTooltip {
                        title: "Acao: Atacar".into(),
                        body: "Ordena ataque direto a uma unidade ou construcao inimiga (ou clique direito no alvo).".into(),
                    }),
                );
                for form in [FormationKind::Box, FormationKind::Line, FormationKind::Wedge] {
                    let active = if control.activeFormation == form { "> " } else { "" };
                    spawnSmallButton(
                        bar,
                        &format!("{}{}", active, form.label()),
                        None,
                        HudAction::SetFormation(form),
                        true,
                        Some(ButtonTooltip {
                            title: format!("Formacao: {}", form.label()),
                            body: "Define como o grupo se organiza ao marchar ate o destino.".into(),
                        }),
                    );
                }
            }
            if hasCitizen {
                spawnSmallButton(
                    bar,
                    "Construir",
                    Some(icons.actBuild.clone()),
                    HudAction::FocusBuildMenu,
                    true,
                    Some(ButtonTooltip {
                        title: "Acao: Construir".into(),
                        body: "Escolha uma das edificacoes ao lado e clique no mapa dentro das suas fronteiras.".into(),
                    }),
                );
                spawnSmallButton(
                    bar,
                    "Reformar",
                    Some(icons.actRepair.clone()),
                    HudAction::EnterCommandMode(CommandMode::Repair),
                    true,
                    Some(ButtonTooltip {
                        title: "Acao: Reformar / Reparar".into(),
                        body: "Envia os cidadaos selecionados para reparar uma construcao aliada danificada ou concluir uma obra.".into(),
                    }),
                );
            }
        }

        // Botões das edificações com ícones próprios + tooltip completo (descrição, trabalhadores, limites).
        if hasCitizen || control.selectedUnits.is_empty() {
            for bDef in &world.data.buildings {
                if bDef.isWonder || bDef.minAge > player.age {
                    continue;
                }
                let owned = world.buildingCountOfKind(control.localPlayer, &bDef.id);
                let limit = world.buildingLimitFor(control.localPlayer, &bDef.id);
                let atLimit = limit.is_some_and(|max| owned >= max);
                let afford = player.resources.canAfford(&bDef.cost);
                let enabled = !atLimit && afford;

                let limitSuffix = limit.map(|max| format!(" [{}/{}]", owned, max)).unwrap_or_default();
                let label = format!("{}{}", bDef.name, limitSuffix);

                let workersLine = if bDef.id == "lumber_camp" {
                    "Trabalhadores: 1 a 5 (conforme o tamanho da mata proxima)".to_string()
                } else if bDef.maxWorkers > 0 {
                    format!("Trabalhadores: ate {} cidadao(s)", bDef.maxWorkers)
                } else {
                    "Trabalhadores: nenhum (estrutura militar/civica)".to_string()
                };
                let capLine = match (bDef.maxPerCity, limit) {
                    (Some(perCity), Some(maxNow)) => format!(
                        "\nLimite: {} por cidade (atual: {}/{} — funde mais cidades para ampliar)",
                        perCity, owned, maxNow
                    ),
                    _ => String::new(),
                };
                let tip = ButtonTooltip {
                    title: format!("{}  ({})", bDef.name, shortCost(&bDef.cost)),
                    body: format!("{}\n{}{}", bDef.description, workersLine, capLine),
                };

                spawnSmallButton(
                    bar,
                    &label,
                    Some(icons.building(&bDef.id)),
                    HudAction::StartBuild { kind: bDef.id.clone(), wonderId: None },
                    enabled,
                    Some(tip),
                );
            }
        }
    });
}

fn refreshDrawers(
    mut commands: Commands,
    sim: Res<SimState>,
    control: Res<PlayerControlState>,
    hudVisible: Res<HudVisible>,
    icons: Res<IconAtlas>,
    mut ui: ResMut<HudUiState>,
    drawerQuery: Single<(Entity, &mut Node), With<DrawerRoot>>,
) {
    let (drawerEntity, mut drawerNode) = drawerQuery.into_inner();
    if !hudVisible.0 || (!ui.showTechDrawer && !ui.showWonderDrawer) {
        drawerNode.display = Display::None;
        return;
    }
    drawerNode.display = Display::Flex;

    let world = &sim.0;
    let Some(player) = world.player(control.localPlayer) else { return };
    let sig = format!(
        "t:{}|w:{}|a:{}|r:{}|wb:{}|res:{:?}",
        ui.showTechDrawer,
        ui.showWonderDrawer,
        player.age,
        player.researchedTechs.len(),
        player.wondersBuilt.len(),
        (
            player.resources.food / 20,
            player.resources.wood / 20,
            player.resources.metal / 20,
            player.resources.wealth / 20,
            player.resources.knowledge / 20
        )
    );
    if sig == ui.lastDrawerSig {
        return;
    }
    ui.lastDrawerSig = sig;

    commands.entity(drawerEntity).despawn_children();
    commands.entity(drawerEntity).with_children(|panel| {
        if ui.showTechDrawer {
            panel.spawn((
                Text::new("ARVORE DE PESQUISAS E ERAS"),
                TextFont { font_size: FontSize::Px(14.0), ..default() },
                TextColor(Color::srgb(0.96, 0.85, 0.45)),
            ));

            if let Some(nextAge) = world.data.age(player.age + 1) {
                let cost = world
                    .effectiveAgeCost(control.localPlayer)
                    .unwrap_or(nextAge.cost);
                let ready = world.canAdvanceAge(control.localPlayer);
                let label = format!(
                    "AVANCAR -> {} (req {} techs | {})",
                    nextAge.name,
                    nextAge.requiredTechs,
                    shortCost(&cost)
                );
                spawnSmallButton(panel, &label, None, HudAction::AdvanceAge, ready, None);
            } else {
                panel.spawn((
                    Text::new("Era maxima alcancada!"),
                    TextFont { font_size: FontSize::Px(12.0), ..default() },
                    TextColor(Color::srgb(0.45, 0.90, 0.55)),
                ));
            }

            let branches = [
                (TechBranch::Military, "Militar"),
                (TechBranch::Civic, "Civico"),
                (TechBranch::Commerce, "Comercio"),
                (TechBranch::Science, "Ciencia"),
            ];
            for (branch, title) in branches {
                panel.spawn((
                    Text::new(format!("-- {} --", title)),
                    TextFont { font_size: FontSize::Px(12.0), ..default() },
                    TextColor(Color::srgb(0.75, 0.82, 0.92)),
                ));
                for tech in world.data.techs.iter().filter(|t| t.branch == branch) {
                    let done = player.researchedTechs.contains(&tech.id);
                    let can = world.canResearchTech(control.localPlayer, &tech.id);
                    let cost = world
                        .effectiveTechCost(control.localPlayer, &tech.id)
                        .unwrap_or(tech.cost);
                    let label = if done {
                        format!("[OK] {} (Era {})", tech.name, tech.minAge + 1)
                    } else {
                        format!("{} (Era {} | {})", tech.name, tech.minAge + 1, shortCost(&cost))
                    };
                    spawnSmallButton(
                        panel,
                        &label,
                        Some(icons.knowledge.clone()),
                        HudAction::ResearchTech(tech.id.clone()),
                        can,
                        None,
                    );
                }
            }
        } else if ui.showWonderDrawer {
            panel.spawn((
                Text::new("MARAVILHAS DO MUNDO (unicas por partida)"),
                TextFont { font_size: FontSize::Px(14.0), ..default() },
                TextColor(Color::srgb(0.96, 0.85, 0.45)),
            ));
            for wonder in &world.data.wonders {
                let claimed = world.isWonderClaimed(&wonder.id);
                let cost = world
                    .effectiveBuildingCost(control.localPlayer, "wonder", Some(&wonder.id))
                    .unwrap_or(wonder.cost);
                let ageOk = player.age >= wonder.minAge;
                let afford = player.resources.canAfford(&cost);
                let enabled = !claimed && ageOk && afford;
                let status = if claimed {
                    "[JA CONSTRUIDA]"
                } else if !ageOk {
                    "[ERA BLOQUEADA]"
                } else {
                    ""
                };
                let label = format!(
                    "{} {} (Era {} | {})\n  {}",
                    wonder.name,
                    status,
                    wonder.minAge + 1,
                    shortCost(&cost),
                    wonder.summary
                );
                spawnSmallButton(
                    panel,
                    &label,
                    Some(icons.building("wonder")),
                    HudAction::StartBuild {
                        kind: "wonder".into(),
                        wonderId: Some(wonder.id.clone()),
                    },
                    enabled,
                    None,
                );
            }
        }
    });
}

fn shortCost(c: &ResourceBundle) -> String {
    let mut parts = Vec::new();
    if c.food > 0 {
        parts.push(format!("{} Alim", c.food));
    }
    if c.wood > 0 {
        parts.push(format!("{} Mad", c.wood));
    }
    if c.metal > 0 {
        parts.push(format!("{} Fer", c.metal));
    }
    if c.wealth > 0 {
        parts.push(format!("{} Riq", c.wealth));
    }
    if c.knowledge > 0 {
        parts.push(format!("{} Con", c.knowledge));
    }
    if parts.is_empty() { "gratis".into() } else { parts.join(" ") }
}
