//! The screen state for the main gameplay.
use super::*;
use bevy_seedling::prelude::*;
use bevy_third_person_camera::ThirdPersonCamera;


pub(super) fn plugin(app: &mut App) {
    app.insert_resource(Modals(Vec::default()))
        .add_systems(OnEnter(Screen::Gameplay), spawn_gameplay_ui)
        .add_systems(
            Update,
            sync_gameplay_lock.run_if(in_state(Screen::Gameplay)),
        )
        .add_observer(toggle_pause)
        .add_observer(trigger_menu_toggle_on_esc)
        .add_observer(toggle_mute);
}

fn spawn_gameplay_ui() {}

/// Declarative cursor/input lock. Runs every frame.
/// Gameplay is blocked when: paused, or any entity with [`BlocksGameplay`] exists.
/// When blocked: cursor unlocked, PlayerCtx removed.
/// When unblocked: cursor locked, PlayerCtx restored.
fn sync_gameplay_lock(
    blockers: Query<(), With<BlocksGameplay>>,
    state: Res<GameState>,
    player: Query<Entity, With<Player>>,
    mut cam: Query<&mut ThirdPersonCamera>,
    mut commands: Commands,
) {
    let should_lock = !state.paused && blockers.is_empty();

    if let Ok(mut cam) = cam.single_mut() {
        cam.cursor_lock_active = should_lock;
    }

    if let Ok(entity) = player.single() {
        if should_lock {
            commands.entity(entity).insert(PlayerCtx);
        } else {
            commands.entity(entity).remove::<PlayerCtx>();
        }
    }
}

fn toggle_pause(
    _: On<TogglePause>,
    mut time: ResMut<Time<Virtual>>,
    mut state: ResMut<GameState>,
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
    // PlayerCtx and cursor lock are handled by sync_gameplay_lock
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
