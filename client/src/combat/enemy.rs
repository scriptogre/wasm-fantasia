use super::*;
use crate::asset_loading::Models;
use crate::models::SpawnEnemy;
use crate::player::{Animation, find_animation_player_descendant};
use crate::rules::{Stat, Stats};
use avian3d::prelude::{Collider, LockedAxes, Mass, RigidBody};
use bevy::scene::SceneInstanceReady;
use bevy_enhanced_input::prelude::Start;
use std::time::Duration;
use wasm_fantasia_shared::combat::{defaults, enemy_ai_decision, EnemyBehaviorKind};

pub fn plugin(app: &mut App) {
    app.add_observer(spawn_enemy_in_front)
        .add_observer(on_enemy_added)
        .add_systems(
            Update,
            (
                enemy_ai
                    .run_if(in_state(Screen::Gameplay))
                    .run_if(not(is_paused)),
                animate_enemies
                    .in_set(PostPhysicsAppSystems::PlayAnimations)
                    .run_if(in_state(Screen::Gameplay)),
            ),
        );
}

// =============================================================================
// Spawn trigger (E key / server request)
// =============================================================================

/// Spawn a pack of enemies in front of the player when E is pressed.
/// In multiplayer: calls server reducer so all clients see the enemies.
/// Offline: spawns locally like before.
fn spawn_enemy_in_front(
    _on: On<Start<SpawnEnemy>>,
    player: Query<&Transform, With<Player>>,
    mode: Res<GameMode>,
    #[cfg(feature = "multiplayer")] conn: Option<Res<crate::networking::SpacetimeDbConnection>>,
    mut commands: Commands,
) {
    let Ok(player_transform) = player.single() else {
        return;
    };

    let forward = player_transform.forward();
    let pos = player_transform.translation;

    // Suppress unused warning when multiplayer feature is off
    let _ = &mode;

    // If multiplayer mode and server is reachable, spawn via server
    #[cfg(feature = "multiplayer")]
    if *mode == GameMode::Multiplayer {
        if let Some(conn) = conn {
            use spacetimedb_sdk::DbContext;
            if conn.conn.is_active() {
                crate::networking::combat::server_spawn_enemies(&conn, pos, forward.as_vec3());
                debug!("Requested 5 enemies from server");
                return;
            }
        }
    }

    // Offline fallback: spawn locally
    let right = player_transform.right();
    let base_pos = pos + *forward * 5.0;

    let offsets = [
        Vec3::ZERO,
        *right * 1.5 + *forward * -0.5,
        *right * -1.5 + *forward * -0.5,
        *right * 2.5 + *forward * -1.5,
        *right * -2.5 + *forward * -1.5,
    ];

    for (i, offset) in offsets.iter().enumerate() {
        let spawn_pos = base_pos + *offset;

        commands.spawn((
            Name::new(format!("TestEnemy_{}", i)),
            DespawnOnExit(Screen::Gameplay),
            Transform::from_translation(spawn_pos),
            Health::new(defaults::ENEMY_HEALTH),
            Enemy,
            Combatant,
            Stats::new()
                .with(Stat::MaxHealth, defaults::ENEMY_HEALTH)
                .with(Stat::Health, defaults::ENEMY_HEALTH),
            Collider::capsule(0.5, 1.0),
            RigidBody::Kinematic,
            LockedAxes::ROTATION_LOCKED,
            Mass(500.0),
        ));
    }

    debug!("Spawned 5 enemies locally");
}

// =============================================================================
// On<Add, Enemy> — attach GLTF model + animation setup to any Enemy entity
// =============================================================================

fn on_enemy_added(
    on: On<Add, Enemy>,
    models: Res<Models>,
    gltf_assets: Res<Assets<Gltf>>,
    mut commands: Commands,
) {
    let entity = on.entity;

    // Remove capsule mesh if present (both SP spawn and MP reconciler may have added it)
    commands
        .entity(entity)
        .remove::<Mesh3d>()
        .remove::<MeshMaterial3d<StandardMaterial>>();

    // Insert behavior/AI components + visibility
    commands.entity(entity).insert((
        EnemyBehavior::default(),
        EnemyAnimations::default(),
        EnemyAi::default(),
        InheritedVisibility::default(),
    ));

    let Some(gltf) = gltf_assets.get(&models.player) else {
        warn!("Player GLTF not loaded when enemy spawned");
        return;
    };

    let scene = SceneRoot(gltf.scenes[0].clone());
    commands.entity(entity).with_children(|parent| {
        let mut child = parent.spawn((Transform::from_xyz(0.0, -1.0, 0.0), scene));
        child.observe(prepare_enemy_scene);
    });
}

// =============================================================================
// Scene ready — wire up animation graph + red material
// =============================================================================

fn prepare_enemy_scene(
    on: On<SceneInstanceReady>,
    models: Res<Models>,
    gltf_assets: Res<Assets<Gltf>>,
    children_q: Query<&Children>,
    anim_players: Query<Entity, With<AnimationPlayer>>,
    parents: Query<&ChildOf>,
    mut enemy_q: Query<&mut EnemyAnimations>,
    mut commands: Commands,
    mut animation_graphs: ResMut<Assets<AnimationGraph>>,
    mut animation_clips: ResMut<Assets<AnimationClip>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mesh_materials: Query<Entity, With<MeshMaterial3d<StandardMaterial>>>,
) {
    let scene_entity = on.entity;

    let Some(gltf) = gltf_assets.get(&models.player) else {
        return;
    };

    // Find AnimationPlayer descendant
    let Some(animation_player_entity) =
        find_animation_player_descendant(scene_entity, &children_q, &anim_players)
    else {
        return;
    };

    // Walk up to the Enemy entity (scene entity → enemy entity)
    let enemy_entity = if let Ok(parent) = parents.get(scene_entity) {
        parent.parent()
    } else {
        scene_entity
    };

    let Ok(mut enemy_animations) = enemy_q.get_mut(enemy_entity) else {
        return;
    };

    // Build animation graph with only zombie clips
    let mut graph = AnimationGraph::new();
    let root_node = graph.root;

    let zombie_clips = [
        Animation::ZombieIdle,
        Animation::ZombieWalkForward,
        Animation::ZombieScratch,
    ];

    for anim in zombie_clips {
        if let Some(clip_handle) = gltf.named_animations.get(anim.clip_name()) {
            if let Some(original_clip) = animation_clips.get(clip_handle) {
                let clip = original_clip.clone();
                let modified_handle = animation_clips.add(clip);
                let node_index = graph.add_clip(modified_handle, 1.0, root_node);
                enemy_animations.animations.insert(anim, node_index);
            }
        }
    }

    enemy_animations.animation_player_entity = Some(animation_player_entity);

    let idle_node = enemy_animations
        .animations
        .get(&Animation::ZombieIdle)
        .copied();
    let graph_handle = animation_graphs.add(graph);

    commands.entity(animation_player_entity).insert((
        AnimationGraphHandle(graph_handle),
        AnimationTransitions::new(),
    ));

    // Start idle animation immediately
    if let Some(index) = idle_node {
        commands
            .entity(animation_player_entity)
            .queue(move |mut entity: EntityWorldMut| {
                let Some(mut transitions) = entity.take::<AnimationTransitions>() else {
                    return;
                };
                if let Some(mut player) = entity.get_mut::<AnimationPlayer>() {
                    transitions
                        .play(&mut player, index, Duration::ZERO)
                        .repeat();
                }
                entity.insert(transitions);
            });
    }

    enemy_animations.current_animation = Some(Animation::ZombieIdle);

    // Replace all descendant materials with flat red
    let red_material = materials.add(StandardMaterial {
        base_color: crate::ui::colors::HEALTH_RED,
        ..default()
    });

    fn recolor_descendants(
        entity: Entity,
        children_q: &Query<&Children>,
        mesh_materials: &Query<Entity, With<MeshMaterial3d<StandardMaterial>>>,
        commands: &mut Commands,
        material: &Handle<StandardMaterial>,
    ) {
        if mesh_materials.get(entity).is_ok() {
            commands
                .entity(entity)
                .insert(MeshMaterial3d(material.clone()));
        }
        if let Ok(children) = children_q.get(entity) {
            for child in children.iter() {
                recolor_descendants(child, children_q, mesh_materials, commands, material);
            }
        }
    }

    recolor_descendants(
        scene_entity,
        &children_q,
        &mesh_materials,
        &mut commands,
        &red_material,
    );
}

// =============================================================================
// Singleplayer AI — chase and attack
// =============================================================================
//
// TODO(server-abstraction): This system duplicates the decision + movement logic
// that also lives in the server's `game_tick` reducer. When the SP/MP backend
// trait lands, both code paths collapse into a single `GameServer::tick_enemies`
// implementation — SP writes directly to ECS, MP calls SpacetimeDB reducers.
// The shared decision function `enemy_ai_decision()` (shared/src/combat.rs)
// already centralises the state-machine; what remains duplicated is the
// movement application and facing logic.

fn enemy_ai(
    time: Res<Time>,
    mut enemies: Query<
        (
            Entity,
            &mut Transform,
            &mut EnemyBehavior,
            &mut EnemyAi,
            &Health,
        ),
        With<Enemy>,
    >,
    #[cfg(feature = "multiplayer")] server_ids: Query<&crate::networking::ServerId>,
    player_q: Query<&Transform, (With<Player>, Without<Enemy>)>,
) {
    let Ok(player_transform) = player_q.single() else {
        return;
    };
    let player_pos = player_transform.translation;
    let dt = time.delta_secs();

    for (entity, mut transform, mut behavior, mut ai, health) in &mut enemies {
        // Skip server-driven enemies in multiplayer — the reconciler handles those
        #[cfg(feature = "multiplayer")]
        if server_ids.get(entity).is_ok() {
            continue;
        }
        let _ = entity; // suppress unused warning in SP builds

        if health.is_dead() {
            continue;
        }

        ai.attack_cooldown.tick(time.delta());

        let enemy_pos = transform.translation;
        let to_player = Vec3::new(
            player_pos.x - enemy_pos.x,
            0.0,
            player_pos.z - enemy_pos.z,
        );
        let distance = to_player.length();

        let decision = enemy_ai_decision(distance, ai.attack_cooldown.finished());

        *behavior = match decision {
            EnemyBehaviorKind::Idle => EnemyBehavior::Idle,
            EnemyBehaviorKind::Chase => EnemyBehavior::Chase,
            EnemyBehaviorKind::Attack => EnemyBehavior::Attack,
        };

        // Face the player when in range
        if decision != EnemyBehaviorKind::Idle && distance > 0.01 {
            let direction = to_player.normalize();
            let target_rotation = Quat::from_rotation_y(f32::atan2(-direction.x, -direction.z));
            transform.rotation = transform.rotation.slerp(target_rotation, (dt * 8.0).min(1.0));
        }

        // Move toward player when chasing
        if decision == EnemyBehaviorKind::Chase && distance > 0.01 {
            let move_dir = to_player.normalize();
            transform.translation += move_dir * defaults::ENEMY_WALK_SPEED * dt;
        }

        // Reset cooldown on attack
        if decision == EnemyBehaviorKind::Attack {
            ai.attack_cooldown.reset();
        }
    }
}

// =============================================================================
// Animation driver — maps EnemyBehavior to zombie clips (all enemies)
// =============================================================================

fn animate_enemies(
    mut enemies: Query<(&EnemyBehavior, &mut EnemyAnimations)>,
    mut animation_query: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    const BLEND_DURATION: Duration = Duration::from_millis(200);

    for (behavior, mut anims) in &mut enemies {
        let Some(anim_entity) = anims.animation_player_entity else {
            continue;
        };
        let Ok((mut anim_player, mut transitions)) = animation_query.get_mut(anim_entity) else {
            continue;
        };

        let target_animation = match behavior {
            EnemyBehavior::Idle => Animation::ZombieIdle,
            EnemyBehavior::Chase => Animation::ZombieWalkForward,
            EnemyBehavior::Attack => Animation::ZombieScratch,
        };

        if anims.current_animation == Some(target_animation) {
            continue;
        }

        let Some(&index) = anims.animations.get(&target_animation) else {
            continue;
        };

        anims.current_animation = Some(target_animation);

        match behavior {
            EnemyBehavior::Attack => {
                transitions
                    .play(&mut anim_player, index, BLEND_DURATION)
                    .set_speed(1.0);
            }
            _ => {
                transitions
                    .play(&mut anim_player, index, BLEND_DURATION)
                    .set_speed(1.0)
                    .repeat();
            }
        }
    }
}
