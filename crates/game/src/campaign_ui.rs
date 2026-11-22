use std::path::PathBuf;

use bevy::picking::events::{PointerOut, PointerOver, PointerPress};
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use roeRender::{
    CommandQueue, NetStepManaged, PlayerControlState, SimState, WorldResetCounter,
};
use roeSim::{CampaignState, GameData, MapGrid, PlayerId, SimWorld};

use crate::audio::{Sfx, SoundBank, ToastNotification};
use crate::menu::{AppMenuState, ScreenMode};
use crate::netplay::NetPlayState;

const CAMPAIGN_MAP_SIZE: u32 = 64;

pub fn campaignSavePath() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("rise-of-empires/saves/campaign.ron");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return PathBuf::from(home).join(".local/share/rise-of-empires/saves/campaign.ron");
        }
    }
    PathBuf::from("saves/campaign.ron")
}

/// Estado da Campanha de Conquista em andamento (mapa estratégico por turnos -> batalhas RTS).
#[derive(Resource, Default)]
pub struct ActiveCampaignState {
    pub campaign: Option<CampaignState>,
    pub dirty: bool,
}

impl ActiveCampaignState {
    pub fn openOrCreate(&mut self, playerNation: &str) {
        if self.campaign.is_none() {
            let path = campaignSavePath();
            self.campaign = CampaignState::loadFromFile(&path)
                .ok()
                .or_else(|| Some(CampaignState::new(playerNation)));
        }
        self.dirty = true;
    }

    pub fn startFresh(&mut self, playerNation: &str) {
        self.campaign = Some(CampaignState::new(playerNation));
        self.dirty = true;
    }
}

#[derive(Component)]
struct CampaignOverlayRoot;

#[derive(Component)]
struct CampaignBattleReturnBtn;

#[derive(Component)]
struct CampaignBattleReturnText;

#[derive(Component, Clone)]
enum CampaignAction {
    InvadeProvince(usize),
    SaveCampaign,
    RestartCampaign,
    ReturnToMainMenu,
    FinishBattleAndReturn,
}

const BG_SCREEN: Color = Color::srgba(0.05, 0.08, 0.13, 0.98);
const MAP_OCEAN: Color = Color::srgba(0.08, 0.16, 0.25, 0.98);
const GOLD_BORDER: Color = Color::srgba(0.84, 0.70, 0.32, 0.92);
const PROV_OWNED: Color = Color::srgba(0.14, 0.36, 0.22, 0.96);
const PROV_INVADABLE: Color = Color::srgba(0.46, 0.20, 0.14, 0.96);
const PROV_INVADABLE_HOVER: Color = Color::srgba(0.64, 0.28, 0.18, 0.98);
const PROV_LOCKED: Color = Color::srgba(0.14, 0.17, 0.22, 0.86);
const BTN_BG: Color = Color::srgba(0.16, 0.22, 0.32, 0.98);
const BTN_HOVER: Color = Color::srgba(0.26, 0.36, 0.50, 0.98);

pub struct CampaignPlugin;

impl Plugin for CampaignPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveCampaignState>()
            .add_systems(Startup, spawnCampaignRoots)
            .add_observer(onCampaignButtonPress)
            .add_observer(onCampaignButtonOver)
            .add_observer(onCampaignButtonOut)
            .add_systems(Update, (rebuildCampaignMapUi, syncCampaignBattleReturnButton).chain());
    }
}

fn spawnCampaignRoots(mut commands: Commands) {
    commands.spawn((
        CampaignOverlayRoot,
        Hovered::default(),
        Node {
            position_type: PositionType::Absolute,
            top: px(0),
            bottom: px(0),
            left: px(0),
            right: px(0),
            padding: UiRect::all(px(18)),
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::SpaceBetween,
            row_gap: px(12),
            display: Display::None,
            ..default()
        },
        BackgroundColor(BG_SCREEN),
        ZIndex(75),
    ));

    // Botão contextual exibido durante uma batalha da campanha assim que há vencedor.
    commands
        .spawn((
            CampaignBattleReturnBtn,
            Hovered::default(),
            CampaignAction::FinishBattleAndReturn,
            Node {
                position_type: PositionType::Absolute,
                top: px(164),
                left: px(16),
                padding: UiRect::axes(px(16), px(10)),
                border: UiRect::all(px(2.0)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(PROV_OWNED),
            BorderColor::all(GOLD_BORDER),
            ZIndex(50),
        ))
        .with_children(|b| {
            b.spawn((
                CampaignBattleReturnText,
                Text::new("CONQUISTAR PROVINCIA E VOLTAR AO MAPA"),
                TextFont { font_size: FontSize::Px(15.0), ..default() },
                TextColor(Color::srgb(0.98, 0.94, 0.72)),
            ));
        });
}

fn onCampaignButtonOver(
    over: On<PointerOver>,
    mut query: Query<(&CampaignAction, &mut BackgroundColor)>,
) {
    if let Ok((action, mut bg)) = query.get_mut(over.entity) {
        *bg = BackgroundColor(match action {
            CampaignAction::InvadeProvince(_) => PROV_INVADABLE_HOVER,
            CampaignAction::FinishBattleAndReturn => Color::srgba(0.22, 0.52, 0.30, 0.98),
            _ => BTN_HOVER,
        });
    }
}

fn onCampaignButtonOut(
    out: On<PointerOut>,
    mut query: Query<(&CampaignAction, &mut BackgroundColor)>,
) {
    if let Ok((action, mut bg)) = query.get_mut(out.entity) {
        *bg = BackgroundColor(match action {
            CampaignAction::InvadeProvince(_) => PROV_INVADABLE,
            CampaignAction::FinishBattleAndReturn => PROV_OWNED,
            _ => BTN_BG,
        });
    }
}

fn onCampaignButtonPress(
    mut press: On<PointerPress>,
    time: Res<Time>,
    mut campState: ResMut<ActiveCampaignState>,
    mut sim: ResMut<SimState>,
    mut queue: ResMut<CommandQueue>,
    mut reset: ResMut<WorldResetCounter>,
    mut control: ResMut<PlayerControlState>,
    mut netManaged: ResMut<NetStepManaged>,
    mut net: ResMut<NetPlayState>,
    mut menu: ResMut<AppMenuState>,
    mut bank: ResMut<SoundBank>,
    mut toast: ResMut<ToastNotification>,
    query: Query<&CampaignAction>,
) {
    let Ok(action) = query.get(press.entity) else { return };
    press.propagate(false);
    control.uiConsumedPointer = true;
    let now = time.elapsed_secs();
    bank.play(Sfx::Click, now);

    match action {
        CampaignAction::InvadeProvince(provId) => {
            let Some(camp) = campState.campaign.as_mut() else { return };
            if !camp.canInvade(*provId) {
                return;
            }
            let Some(prov) = camp.provinces.get(*provId).cloned() else { return };
            camp.activeBattleProvince = Some(prov.id);

            net.disconnect(&mut netManaged, &mut control);
            let data = GameData::loadDefault();
            let map = MapGrid::generate(CAMPAIGN_MAP_SIZE, CAMPAIGN_MAP_SIZE, prov.mapSeed);
            let mut world = SimWorld::newMatchWithAi(
                map,
                data,
                &camp.playerNation,
                &prov.nationId,
                prov.difficulty,
            );
            world.applyCampaignBonuses(PlayerId(0), camp);
            sim.0 = world;
            queue.0.clear();
            control.localPlayer = PlayerId(0);
            reset.0 += 1;
            menu.resumePlaying();
            bank.play(Sfx::AgeUp, now);
            toast.show(format!(
                "Invasao de {}! Bonus de imperio aplicados.",
                prov.name
            ));
        }
        CampaignAction::SaveCampaign => {
            if let Some(camp) = &campState.campaign {
                match camp.saveToFile(&campaignSavePath()) {
                    Ok(()) => {
                        bank.play(Sfx::Complete, now);
                        toast.show("Campanha de Conquista salva!");
                    }
                    Err(e) => toast.show(format!("Erro ao salvar campanha: {e}")),
                }
            }
        }
        CampaignAction::RestartCampaign => {
            let nation = campState
                .campaign
                .as_ref()
                .map(|c| c.playerNation.clone())
                .unwrap_or_else(|| "roma".into());
            campState.startFresh(&nation);
            toast.show("Nova Campanha de Conquista iniciada!");
        }
        CampaignAction::ReturnToMainMenu => {
            menu.openMainMenu();
        }
        CampaignAction::FinishBattleAndReturn => {
            if let Some(camp) = campState.campaign.as_mut() {
                if let Some(pid) = camp.activeBattleProvince.take() {
                    if sim.0.winner == Some(PlayerId(0)) {
                        let provName = camp
                            .provinces
                            .get(pid)
                            .map(|p| p.name.clone())
                            .unwrap_or_default();
                        camp.conquerProvince(pid);
                        let _ = camp.saveToFile(&campaignSavePath());
                        bank.play(Sfx::Victory, now);
                        toast.show(format!(
                            "Provincia {} conquistada! Seu imperio expandiu.",
                            provName
                        ));
                    } else {
                        camp.turn += 1;
                        toast.show("Recuada estrategica: reorganize seu imperio e tente novamente.");
                    }
                }
            }
            campState.dirty = true;
            menu.openCampaignMap();
        }
    }
}

fn syncCampaignBattleReturnButton(
    menu: Res<AppMenuState>,
    sim: Res<SimState>,
    campState: Res<ActiveCampaignState>,
    mut btnNode: Single<&mut Node, With<CampaignBattleReturnBtn>>,
    mut btnText: Single<&mut Text, With<CampaignBattleReturnText>>,
) {
    let inCampaignBattle = menu.mode == ScreenMode::Playing
        && campState
            .campaign
            .as_ref()
            .and_then(|c| c.activeBattleProvince)
            .is_some();

    if inCampaignBattle && sim.0.winner.is_some() {
        btnNode.display = Display::Flex;
        btnText.0 = if sim.0.winner == Some(PlayerId(0)) {
            "CONQUISTAR PROVINCIA E VOLTAR AO MAPA >>".into()
        } else {
            "RECUAR PARA O MAPA DA CAMPANHA >>".into()
        };
    } else {
        btnNode.display = Display::None;
    }
}

fn rebuildCampaignMapUi(
    mut commands: Commands,
    menu: Res<AppMenuState>,
    sim: Res<SimState>,
    mut campState: ResMut<ActiveCampaignState>,
    mut lastMode: Local<Option<ScreenMode>>,
    rootQuery: Single<(Entity, &mut Node), With<CampaignOverlayRoot>>,
) {
    let modeChanged = *lastMode != Some(menu.mode);
    *lastMode = Some(menu.mode);
    if !modeChanged && !campState.dirty {
        return;
    }
    campState.dirty = false;

    let (rootEntity, mut rootNode) = rootQuery.into_inner();
    commands.entity(rootEntity).despawn_children();

    if menu.mode != ScreenMode::CampaignMap {
        rootNode.display = Display::None;
        return;
    }
    rootNode.display = Display::Flex;

    let Some(camp) = campState.campaign.as_ref() else {
        return;
    };
    let data = &sim.0.data;
    let nationName = data
        .nation(&camp.playerNation)
        .map_or(camp.playerNation.as_str(), |n| n.name.as_str());
    let (res, inf, cav, cit) = camp.accumulatedBonuses();

    commands.entity(rootEntity).with_children(|root| {
        // --- Cabeçalho da Campanha ---
        root.spawn((
            Node {
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                padding: UiRect::axes(px(14), px(10)),
                border: UiRect::all(px(1.5)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.08, 0.11, 0.17, 0.96)),
            BorderColor::all(GOLD_BORDER),
        ))
        .with_children(|top| {
            top.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(3),
                ..default()
            })
            .with_children(|titles| {
                titles.spawn((
                    Text::new(format!(
                        "CAMPANHA DE CONQUISTA  |  IMPERIO: {}",
                        nationName.to_uppercase()
                    )),
                    TextFont { font_size: FontSize::Px(22.0), ..default() },
                    TextColor(Color::srgb(0.96, 0.84, 0.42)),
                ));
                titles.spawn((
                    Text::new(format!(
                        "Turno {}  |  Provincias controladas: {}/{}  |  Clique em uma provincia adjacente em vermelho para invadir!",
                        camp.turn,
                        camp.ownedCount(),
                        camp.provinces.len()
                    )),
                    TextFont { font_size: FontSize::Px(13.0), ..default() },
                    TextColor(Color::srgb(0.82, 0.86, 0.92)),
                ));
            });

            top.spawn(Node { column_gap: px(8), ..default() })
                .with_children(|btns| {
                    spawnToolbarBtn(btns, "SALVAR CAMPANHA", CampaignAction::SaveCampaign);
                    spawnToolbarBtn(btns, "REINICIAR", CampaignAction::RestartCampaign);
                    spawnToolbarBtn(btns, "MENU PRINCIPAL", CampaignAction::ReturnToMainMenu);
                });
        });

        // --- Mapa Estratégico das 8 Províncias ---
        root.spawn((
            Node {
                position_type: PositionType::Relative,
                width: percent(100),
                flex_grow: 1.0,
                min_height: px(420),
                border: UiRect::all(px(2.0)),
                overflow: Overflow::clip(),
                ..default()
            },
            BackgroundColor(MAP_OCEAN),
            BorderColor::all(GOLD_BORDER),
        ))
        .with_children(|canvas| {
            // Faixa decorativa de continente/mar ao fundo
            canvas.spawn((
                Text::new("MAPA DO MUNDO ANTIGO  (8 PROVINCIAS)"),
                TextFont { font_size: FontSize::Px(12.0), ..default() },
                TextColor(Color::srgba(0.60, 0.72, 0.84, 0.55)),
                Node {
                    position_type: PositionType::Absolute,
                    top: px(10),
                    left: px(14),
                    ..default()
                },
            ));

            for prov in &camp.provinces {
                let invadable = camp.canInvade(prov.id);
                let leftPct = ((prov.posNorm.0 as f32 / 10.0) - 8.5).clamp(2.0, 80.0);
                let topPct = ((prov.posNorm.1 as f32 / 10.0) - 9.0).clamp(6.0, 74.0);
                let defNation = data
                    .nation(&prov.nationId)
                    .map_or(prov.nationId.as_str(), |n| n.name.as_str());
                let neighborsTxt: String = prov
                    .neighbors
                    .iter()
                    .filter_map(|&nid| camp.provinces.get(nid).map(|n| n.name.as_str()))
                    .collect::<Vec<_>>()
                    .join(", ");

                let bg = if prov.playerOwned {
                    PROV_OWNED
                } else if invadable {
                    PROV_INVADABLE
                } else {
                    PROV_LOCKED
                };
                let border = if prov.playerOwned {
                    Color::srgb(0.32, 0.86, 0.44)
                } else if invadable {
                    Color::srgb(0.98, 0.74, 0.28)
                } else {
                    Color::srgba(0.42, 0.46, 0.54, 0.65)
                };

                let mut card = canvas.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: percent(leftPct),
                        top: percent(topPct),
                        width: px(198),
                        padding: UiRect::all(px(9)),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(3),
                        border: UiRect::all(px(if invadable { 2.5 } else { 1.5 })),
                        ..default()
                    },
                    BackgroundColor(bg),
                    BorderColor::all(border),
                ));
                if invadable {
                    card.insert((Hovered::default(), CampaignAction::InvadeProvince(prov.id)));
                }
                card.with_children(|c| {
                    let badge = if prov.playerOwned {
                        "[CONQUISTADA]"
                    } else if invadable {
                        "[INVADIR ->]"
                    } else {
                        "[DISTANTE]"
                    };
                    c.spawn((
                        Text::new(format!("{} {}", prov.name.to_uppercase(), badge)),
                        TextFont { font_size: FontSize::Px(13.0), ..default() },
                        TextColor(if prov.playerOwned {
                            Color::srgb(0.68, 0.98, 0.72)
                        } else if invadable {
                            Color::srgb(1.0, 0.88, 0.44)
                        } else {
                            Color::srgb(0.72, 0.76, 0.82)
                        }),
                    ));
                    c.spawn((
                        Text::new(if prov.playerOwned {
                            format!("Controlada por {}", nationName)
                        } else {
                            format!("Defesa: {} (IA {})", defNation, prov.difficulty.label())
                        }),
                        TextFont { font_size: FontSize::Px(11.0), ..default() },
                        TextColor(Color::srgb(0.90, 0.92, 0.96)),
                    ));
                    c.spawn((
                        Text::new(format!("Bonus: {}", prov.bonus.label())),
                        TextFont { font_size: FontSize::Px(11.0), ..default() },
                        TextColor(Color::srgb(0.95, 0.86, 0.58)),
                    ));
                    c.spawn((
                        Text::new(format!("Rotas: {}", neighborsTxt)),
                        TextFont { font_size: FontSize::Px(10.0), ..default() },
                        TextColor(Color::srgb(0.70, 0.78, 0.86)),
                    ));
                });
            }
        });

        // --- Rodapé com Bônus Acumulados do Império ---
        root.spawn((
            Node {
                padding: UiRect::axes(px(14), px(10)),
                justify_content: JustifyContent::SpaceBetween,
                align_items: AlignItems::Center,
                border: UiRect::all(px(1.5)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.08, 0.11, 0.17, 0.96)),
            BorderColor::all(GOLD_BORDER),
        ))
        .with_children(|foot| {
            let bonusSummary = format!(
                "BONUS ACUMULADOS NAS BATALHAS:  +{} Alim  +{} Mad  +{} Fer  +{} Riq  |  Tropas extras: +{} Inf  +{} Cav  +{} Cid",
                res.food, res.wood, res.metal, res.wealth, inf, cav, cit
            );
            foot.spawn((
                Text::new(if camp.isComplete() {
                    format!("IMPERIO UNIFICADO! TODAS AS 8 PROVINCIAS CONQUISTADAS!  |  {bonusSummary}")
                } else {
                    bonusSummary
                }),
                TextFont { font_size: FontSize::Px(13.0), ..default() },
                TextColor(if camp.isComplete() {
                    Color::srgb(0.45, 0.96, 0.56)
                } else {
                    Color::srgb(0.94, 0.88, 0.66)
                }),
            ));
        });
    });
}

fn spawnToolbarBtn(parent: &mut ChildSpawnerCommands, label: &str, action: CampaignAction) {
    parent
        .spawn((
            Hovered::default(),
            action,
            Node {
                padding: UiRect::axes(px(11), px(7)),
                border: UiRect::all(px(1.5)),
                ..default()
            },
            BackgroundColor(BTN_BG),
            BorderColor::all(GOLD_BORDER),
        ))
        .with_children(|b| {
            b.spawn((
                Text::new(label),
                TextFont { font_size: FontSize::Px(12.5), ..default() },
                TextColor(Color::srgb(0.96, 0.96, 0.98)),
            ));
        });
}
