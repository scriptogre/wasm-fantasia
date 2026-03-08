//! The screen state for the main gameplay.
use super::*;
use bevy_seedling::prelude::*;
use bevy_third_person_camera::ThirdPersonCamera;

use crate::networking::generated::{
    pause_world_reducer::pause_world, resume_world_reducer::resume_world,
};

pub(super) fn plugin(app: &mut App) {
    app.insert_resource(Modals(Vec::default()))
        .add_systems(PostStartup, mark_startup_entities_persistent)
        .add_systems(OnEnter(Screen::Gameplay), spawn_gameplay_ui)
        .add_systems(
            OnExit(Screen::Gameplay),
            (unpause_server_on_exit, strip_input_contexts, cleanup_gameplay_entities)
                .chain()
                .run_if(not(is_entering_game_over))
                .in_set(GameplayCleanup),
        )
        .add_systems(
            OnExit(Screen::GameOver),
            (unpause_server_on_exit, strip_input_contexts, cleanup_gameplay_entities)
                .chain()
                .in_set(GameplayCleanup),
        )
        .add_systems(
            Update,
            sync_gameplay_lock.run_if(in_state(Screen::Gameplay)),
        )
        .add_systems(OnEnter(PauseState::Paused), on_pause)
        .add_systems(OnExit(PauseState::Paused), on_unpause)
        .add_observer(toggle_pause)
        .add_observer(trigger_menu_toggle_on_esc)
        .add_observer(toggle_mute);
}

/// Runs once after Startup — marks every existing entity as [`Persistent`]
/// so it survives gameplay exit cleanup.
fn mark_startup_entities_persistent(
    all_entities: Query<Entity, Without<Persistent>>,
    mut commands: Commands,
) {
    for entity in all_entities.iter() {
        commands.entity(entity).insert(Persistent);
    }
}

/// Ensure the server is unpaused when leaving gameplay.
/// Covers all exit paths (Main Menu, disconnect, etc.) so the server tick
/// isn't left frozen when the player returns.
fn unpause_server_on_exit(
    pause: Res<State<PauseState>>,
    mode: Res<GameMode>,
    conn: Option<Res<crate::networking::SpacetimeDbConnection>>,
    mut next_pause: ResMut<NextState<PauseState>>,
) {
    if *pause.get() == PauseState::Paused && *mode != GameMode::Multiplayer {
        if let Some(conn) = conn {
            let _ = conn.conn.reducers.resume_world();
        }
    }
    next_pause.set(PauseState::Running);
}

/// Nuclear cleanup on gameplay exit: despawn every root entity that wasn't
/// marked [`Persistent`]. Filters out `ChildOf` to avoid double-despawn
/// warnings (`despawn()` is recursive in Bevy 0.17), and `FirewheelNode`
/// because bevy_seedling's audio graph holds internal references that
/// outlive the ECS entity — let the audio system manage its own lifecycle.
fn cleanup_gameplay_entities(
    entities: Query<
        Entity,
        (
            Without<Persistent>,
            Without<ChildOf>,
            Without<FirewheelNode>,
        ),
    >,
    mut commands: Commands,
) {
    for entity in entities.iter() {
        commands.entity(entity).try_despawn();
    }
}

fn spawn_gameplay_ui() {}

/// Remove input contexts before nuclear despawn so their `On<Remove>` observers
/// fire while entities are still fully alive (avoids stale-entity panics from
/// `despawn_related::<Actions<_>>`).
fn strip_input_contexts(
    players: Query<Entity, With<PlayerCtx>>,
    modals: Query<Entity, With<ModalCtx>>,
    mut commands: Commands,
) {
    for entity in players.iter() {
        commands.entity(entity).remove::<PlayerCtx>();
    }
    for entity in modals.iter() {
        commands.entity(entity).remove::<ModalCtx>();
    }
}

/// Declarative cursor/input lock. Runs every frame during gameplay.
/// Gameplay is blocked when: paused, or any entity with [`BlocksGameplay`] exists.
/// When blocked: cursor unlocked, PlayerCtx removed.
/// When unblocked: cursor locked, PlayerCtx restored.
fn sync_gameplay_lock(
    blockers: Query<(), With<BlocksGameplay>>,
    pause: Res<State<PauseState>>,
    player: Query<Entity, With<Player>>,
    mut cam: Query<&mut ThirdPersonCamera>,
    mut commands: Commands,
) {
    let should_lock = *pause.get() != PauseState::Paused && blockers.is_empty();

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

fn on_pause(
    mode: Res<GameMode>,
    conn: Option<Res<crate::networking::SpacetimeDbConnection>>,
    mut time: ResMut<Time<Virtual>>,
) {
    if *mode != GameMode::Multiplayer {
        time.pause();
        if let Some(conn) = conn {
            let _ = conn.conn.reducers.pause_world();
        }
    }
}

fn on_unpause(
    mode: Res<GameMode>,
    conn: Option<Res<crate::networking::SpacetimeDbConnection>>,
    mut time: ResMut<Time<Virtual>>,
) {
    if *mode != GameMode::Multiplayer {
        time.unpause();
        if let Some(conn) = conn {
            let _ = conn.conn.reducers.resume_world();
        }
    }
}

fn toggle_pause(
    _: On<TogglePause>,
    pause: Res<State<PauseState>>,
    mut next_pause: ResMut<NextState<PauseState>>,
) {
    match pause.get() {
        PauseState::Paused => next_pause.set(PauseState::Running),
        PauseState::Running => next_pause.set(PauseState::Paused),
    }
}

fn toggle_mute(
    _: On<ToggleMute>,
    settings: ResMut<Settings>,
    mut session: ResMut<Session>,
    mut music: Single<&mut VolumeNode, (With<MusicPool>, Without<SoundEffectsBus>)>,
    mut sfx: Single<&mut VolumeNode, (With<SoundEffectsBus>, Without<MusicPool>)>,
) {
    if session.muted {
        music.volume = settings.music();
        sfx.volume = settings.sfx();
    } else {
        music.volume = Volume::SILENT;
        sfx.volume = Volume::SILENT;
    }
    session.muted = !session.muted;
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
