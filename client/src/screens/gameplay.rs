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
            (unpause_server_on_exit, cleanup_gameplay_entities)
                .chain()
                .in_set(GameplayCleanup),
        )
        .add_systems(
            Update,
            (
                sync_gameplay_lock.run_if(in_state(Screen::Gameplay)),
                sync_virtual_time,
            ),
        )
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
    mut session: ResMut<Session>,
    mode: Res<GameMode>,
    conn: Option<Res<crate::networking::SpacetimeDbConnection>>,
) {
    if session.paused && *mode != GameMode::Multiplayer {
        if let Some(conn) = conn {
            let _ = conn.conn.reducers.resume_world();
        }
    }
    session.paused = false;
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
        commands.entity(entity).despawn();
    }
}

fn spawn_gameplay_ui() {}

/// Declarative cursor/input lock. Runs every frame during gameplay.
/// Gameplay is blocked when: paused, or any entity with [`BlocksGameplay`] exists.
/// When blocked: cursor unlocked, PlayerCtx removed.
/// When unblocked: cursor locked, PlayerCtx restored.
fn sync_gameplay_lock(
    blockers: Query<(), With<BlocksGameplay>>,
    session: Res<Session>,
    player: Query<Entity, With<Player>>,
    mut cam: Query<&mut ThirdPersonCamera>,
    mut commands: Commands,
) {
    let should_lock = !session.paused && blockers.is_empty();

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

/// Keeps `Time<Virtual>` in sync with `session.paused`.
/// Runs globally so leaving gameplay with time paused always cleans up.
fn sync_virtual_time(session: Res<Session>, mode: Res<GameMode>, mut time: ResMut<Time<Virtual>>) {
    let should_pause = session.paused && *mode != GameMode::Multiplayer;
    if should_pause != time.is_paused() {
        if should_pause {
            time.pause();
        } else {
            time.unpause();
        }
    }
}

fn toggle_pause(
    _: On<TogglePause>,
    mut session: ResMut<Session>,
    mode: Res<GameMode>,
    conn: Option<Res<crate::networking::SpacetimeDbConnection>>,
) {
    session.paused = !session.paused;

    if *mode != GameMode::Multiplayer {
        if let Some(conn) = conn {
            let _ = if session.paused {
                conn.conn.reducers.pause_world()
            } else {
                conn.conn.reducers.resume_world()
            };
        }
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
