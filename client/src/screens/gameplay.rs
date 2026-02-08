//! The screen state for the main gameplay.
use super::*;
use bevy_seedling::prelude::*;


pub(super) fn plugin(app: &mut App) {
    app.insert_resource(Modals(Vec::default()))
        .add_systems(OnEnter(Screen::Gameplay), spawn_gameplay_ui)
        .add_observer(toggle_pause)
        .add_observer(trigger_menu_toggle_on_esc)
        .add_observer(toggle_mute);
}

fn spawn_gameplay_ui() {}

fn toggle_pause(
    _: On<TogglePause>,
    mut time: ResMut<Time<Virtual>>,
    mut state: ResMut<GameState>,
    player: Query<Entity, With<Player>>,
    mut commands: Commands,
    mode: Res<GameMode>,
) {
    let is_multiplayer = *mode == GameMode::Multiplayer;

    if time.is_paused() || state.paused {
        if !is_multiplayer {
            time.unpause();
        }
    } else if !is_multiplayer {
        time.pause();
    }

    state.paused = !state.paused;

    if let Ok(entity) = player.single() {
        if state.paused {
            commands.entity(entity).remove::<PlayerCtx>();
        } else {
            commands.entity(entity).insert(PlayerCtx);
        }
    }
}

fn toggle_mute(
    _: On<ToggleMute>,
    settings: ResMut<Settings>,
    mut state: ResMut<GameState>,
    mut music: Single<&mut VolumeNode, (With<MusicPool>, Without<SfxBus>)>,
    mut sfx: Single<&mut VolumeNode, (With<SfxBus>, Without<MusicPool>)>,
) {
    if state.muted {
        music.volume = settings.music();
        sfx.volume = settings.sfx();
    } else {
        music.volume = Volume::SILENT;
        sfx.volume = Volume::SILENT;
    }
    state.muted = !state.muted;
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
