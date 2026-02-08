//! The screen state for the main gameplay.
use super::*;
use bevy_seedling::prelude::*;

use crate::combat::{spawn_combat_stacks_display, spawn_player_health_bar};

pub(super) fn plugin(app: &mut App) {
    app.insert_resource(Modals(Vec::default()))
        .add_systems(OnEnter(Screen::Gameplay), spawn_gameplay_ui)
        .add_observer(toggle_pause)
        .add_observer(trigger_menu_toggle_on_esc)
        .add_observer(toggle_mute);
}

markers!(PauseIcon, MuteIcon, GameplayUi);

fn spawn_gameplay_ui(mut cmds: Commands, textures: Res<Textures>, _settings: Res<Settings>) {
    // info!("settings on gameplay enter:{settings:?}");
    let opts = Props::default().hidden().width(Vw(5.0)).height(Vw(5.0));
    cmds.spawn((
        DespawnOnExit(Screen::Gameplay),
        GameplayUi,
        ui_root("Gameplay Ui"),
        children![
            // mute/pause icons
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Start,
                    justify_content: JustifyContent::Start,
                    position_type: PositionType::Absolute,
                    top: Px(0.0),
                    left: Vw(47.5),
                    ..Default::default()
                },
                children![
                    (icon(opts.clone().image(textures.pause.clone())), PauseIcon),
                    (icon(opts.clone().image(textures.mute.clone())), MuteIcon),
                ]
            ),
        ],
    ));

    // Player health bar and combat stacks
    spawn_player_health_bar(&mut cmds);
    spawn_combat_stacks_display(&mut cmds);
}

fn toggle_pause(
    _: On<TogglePause>,
    mut time: ResMut<Time<Virtual>>,
    mut state: ResMut<GameState>,
    mut pause_label: Query<&mut Node, With<PauseIcon>>,
    player: Query<Entity, With<Player>>,
    mut commands: Commands,
    #[cfg(feature = "multiplayer")] multiplayer: Option<
        Res<crate::networking::SpacetimeDbConnection>,
    >,
) {
    #[cfg(feature = "multiplayer")]
    let is_multiplayer = multiplayer.is_some();
    #[cfg(not(feature = "multiplayer"))]
    let is_multiplayer = false;

    if let Ok(mut label) = pause_label.single_mut() {
        if time.is_paused() || state.paused {
            if !is_multiplayer {
                time.unpause();
            }
            label.display = Display::None;
        } else {
            if !is_multiplayer {
                time.pause();
            }
            label.display = Display::Flex;
        }
    }

    state.paused = !state.paused;

    // Toggle player input context: removing PlayerCtx disables all gameplay
    // actions at the source — no per-handler checks needed.
    if let Ok(entity) = player.single() {
        if state.paused {
            commands.entity(entity).remove::<PlayerCtx>();
        } else {
            commands.entity(entity).insert(PlayerCtx);
        }
    }

    info!("paused: {}", state.paused);
}

fn toggle_mute(
    _: On<ToggleMute>,
    settings: ResMut<Settings>,
    mut state: ResMut<GameState>,
    mut label: Query<&mut Node, With<MuteIcon>>,
    mut music: Single<&mut VolumeNode, (With<MusicPool>, Without<SfxBus>)>,
    mut sfx: Single<&mut VolumeNode, (With<SfxBus>, Without<MusicPool>)>,
) {
    if let Ok(mut node) = label.single_mut() {
        if state.muted {
            music.volume = settings.music();
            sfx.volume = settings.sfx();
            node.display = Display::None;
        } else {
            music.volume = Volume::SILENT;
            sfx.volume = Volume::SILENT;
            node.display = Display::Flex;
        }
    }
    state.muted = !state.muted;
    info!("muted: {}", state.muted);
}

// ============================ UI ============================

fn trigger_menu_toggle_on_esc(
    on: On<Back>,
    mut commands: Commands,
    screen: Res<State<Screen>>,
    modals: If<ResMut<Modals>>,
) {
    if *screen.get() != Screen::Gameplay {
        return;
    }

    if modals.is_empty() {
        commands.trigger(NewModal {
            entity: on.entity,
            modal: Modal::Main,
        });
    } else {
        commands.entity(on.entity).trigger(PopModal);
    }
}
