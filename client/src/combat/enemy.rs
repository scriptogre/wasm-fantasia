use super::*;
use crate::asset_loading::Models;
use crate::models::SpawnEnemy;
use crate::player::Animation;
use crate::rules::{Stat, Stats};
use bevy::gltf::Gltf;
use bevy::scene::SceneInstanceReady;
use bevy_enhanced_input::prelude::Start;
use std::collections::HashMap;
use std::time::Duration;
use wasm_fantasia_shared::combat::{defaults, enemy_ai_decision, EnemyBehaviorKind};

/// Shared assets for all enemies — scene handle, red material, and animation clips.
/// Bevy shares underlying mesh/material data on the GPU when multiple entities
/// reference the same `Handle<Scene>`, enabling automatic draw call batching.
#[derive(Resource)]
struct SharedEnemyAssets {
    scene: Handle<Scene>,
    material: Handle<StandardMaterial>,
    animation_clips: HashMap<Animation, AnimationNodeIndex>,
    animation_graph: Handle<AnimationGraph>,
}

/// Distance beyond which enemy animations are paused to save CPU.
const ANIMATION_CULL_DISTANCE: f32 = 30.0;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(Screen::Gameplay), setup_shared_enemy_assets)
        .add_observer(spawn_enemy_in_front)
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
                cull_distant_enemy_animations
                    .run_if(in_state(Screen::Gameplay)),
            ),
        );
}

fn setup_shared_enemy_assets(
    mut commands: Commands,
    models: Res<Models>,
    gltf_assets: Res<Assets<Gltf>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut animation_graphs: ResMut<Assets<AnimationGraph>>,
    mut animation_clips: ResMut<Assets<AnimationClip>>,
) {
    let Some(gltf) = gltf_assets.get(&models.player) else {
        warn!("Player GLTF not loaded when setting up enemy assets");
        return;
    };

    let scene = gltf.scenes[0].clone();

    // Build a shared animation graph with zombie clips
    let mut graph = AnimationGraph::new();
    let root_node = graph.root;
    let mut clips = HashMap::new();

    let zombie_animations = [
        Animation::ZombieIdle,
        Animation::ZombieWalkForward,
        Animation::ZombieScratch,
    ];

    for anim in &zombie_animations {
        let clip_name = anim.clip_name();
        if let Some(clip_handle) = gltf.named_animations.get(clip_name) {
            if let Some(original_clip) = animation_clips.get(clip_handle) {
                let clip = original_clip.clone();
                let modified_handle = animation_clips.add(clip);
                let node_index = graph.add_clip(modified_handle, 1.0, root_node);
                clips.insert(*anim, node_index);
            }
        }
    }

    info!("Enemy animation graph: {} zombie clips loaded", clips.len());

    let material = materials.add(StandardMaterial {
        base_color: crate::ui::colors::HEALTH_RED,
        ..default()
    });

    let graph_handle = animation_graphs.add(graph);

    commands.insert_resource(SharedEnemyAssets {
        scene,
        material,
        animation_clips: clips,
        animation_graph: graph_handle,
    });
}

// =============================================================================
// Spawn trigger (E key / server request)
// =============================================================================

/// Spawn a horde of enemies around the player when E is pressed.
/// In multiplayer: calls server reducer so all clients see the enemies.
/// Offline: spawns locally.
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
                debug!("Requested enemies from server");
                return;
            }
        }
    }

    // Offline fallback: spawn locally
    // TODO(server-abstraction): spawn logic is duplicated in server's spawn_enemies reducer.
    let count = 80 + (rand::random::<u32>() % 41); // 80–120 enemies

    for i in 0..count {
        let angle = rand::random::<f32>() * std::f32::consts::TAU;
        let radius = defaults::ENEMY_SPAWN_RADIUS_MIN
            + rand::random::<f32>()
                * (defaults::ENEMY_SPAWN_RADIUS_MAX - defaults::ENEMY_SPAWN_RADIUS_MIN);
        let spawn_pos = pos + Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius);

        commands.spawn((
            Name::new(format!("TestEnemy_{}", i)),
            Transform::from_translation(spawn_pos),
            Health::new(defaults::ENEMY_HEALTH),
            Enemy,
            Combatant,
            Stats::new()
                .with(Stat::MaxHealth, defaults::ENEMY_HEALTH)
                .with(Stat::Health, defaults::ENEMY_HEALTH),
        ));
    }

    debug!("Spawned {} enemies locally", count);
}

// =============================================================================
// On<Add, Enemy> — attach animated scene to any Enemy entity
// =============================================================================

fn on_enemy_added(
    on: On<Add, Enemy>,
    shared: Option<Res<SharedEnemyAssets>>,
    mut commands: Commands,
) {
    let Some(shared) = shared else {
        warn!("SharedEnemyAssets not available when enemy added");
        return;
    };

    let enemy_entity = on.entity;

    commands.entity(enemy_entity).insert((
        EnemyBehavior::default(),
        EnemyAi::default(),
        InheritedVisibility::default(),
    ));

    // Spawn animated scene as child (offset y=-1.0 for model origin)
    let child_id = commands
        .spawn((
            Transform::from_xyz(0.0, -1.0, 0.0),
            SceneRoot(shared.scene.clone()),
        ))
        .observe(prepare_enemy_scene)
        .id();

    commands.entity(enemy_entity).add_children(&[child_id]);
}

// =============================================================================
// Scene ready — set up animation player and replace materials
// =============================================================================

fn prepare_enemy_scene(
    on: On<SceneInstanceReady>,
    shared: Option<Res<SharedEnemyAssets>>,
    children_q: Query<&Children>,
    anim_players: Query<Entity, With<AnimationPlayer>>,
    parents: Query<&ChildOf>,
    mesh_material_q: Query<Entity, With<MeshMaterial3d<StandardMaterial>>>,
    mut commands: Commands,
) {
    let Some(shared) = shared else {
        return;
    };

    let scene_entity = on.entity;

    // Find the AnimationPlayer descendant
    let Some(animation_player_entity) =
        crate::player::find_animation_player_descendant(scene_entity, &children_q, &anim_players)
    else {
        warn!("No AnimationPlayer found in enemy scene");
        return;
    };

    // Walk up to the Enemy entity (scene child -> enemy parent)
    let enemy_entity = if let Ok(parent) = parents.get(scene_entity) {
        parent.parent()
    } else {
        scene_entity
    };

    // Set up animation graph and transitions on the AnimationPlayer
    let idle_node = shared.animation_clips.get(&Animation::ZombieIdle).copied();

    commands.entity(animation_player_entity).insert((
        AnimationGraphHandle(shared.animation_graph.clone()),
        AnimationTransitions::new(),
    ));

    // Start idle animation immediately to avoid T-pose
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

    // Store animation data on the enemy entity
    commands.entity(enemy_entity).insert(EnemyAnimations {
        animations: shared.animation_clips.clone(),
        animation_player_entity: Some(animation_player_entity),
        current_animation: Some(Animation::ZombieIdle),
    });

    // Replace all materials with shared red material
    replace_materials_recursive(
        scene_entity,
        &children_q,
        &mesh_material_q,
        &shared.material,
        &mut commands,
    );
}

fn replace_materials_recursive(
    entity: Entity,
    children_q: &Query<&Children>,
    mesh_material_q: &Query<Entity, With<MeshMaterial3d<StandardMaterial>>>,
    material: &Handle<StandardMaterial>,
    commands: &mut Commands,
) {
    if mesh_material_q.get(entity).is_ok() {
        commands
            .entity(entity)
            .insert(MeshMaterial3d(material.clone()));
    }
    if let Ok(children) = children_q.get(entity) {
        for child in children.iter() {
            replace_materials_recursive(child, children_q, mesh_material_q, material, commands);
        }
    }
}

// =============================================================================
// Animate enemies based on behavior state
// =============================================================================

fn animate_enemies(
    enemies: Query<(&EnemyBehavior, &EnemyAnimations), Changed<EnemyBehavior>>,
    mut animation_query: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    const BLEND_DURATION: Duration = Duration::from_millis(200);

    for (behavior, enemy_anims) in &enemies {
        let Some(anim_entity) = enemy_anims.animation_player_entity else {
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

        let Some(&node_index) = enemy_anims.animations.get(&target_animation) else {
            continue;
        };

        match behavior {
            EnemyBehavior::Idle | EnemyBehavior::Chase => {
                transitions
                    .play(&mut anim_player, node_index, BLEND_DURATION)
                    .repeat();
            }
            EnemyBehavior::Attack => {
                // Attack plays once (no repeat)
                transitions.play(&mut anim_player, node_index, BLEND_DURATION);
            }
        }
    }
}

// =============================================================================
// Distance-based animation culling
// =============================================================================

fn cull_distant_enemy_animations(
    enemies: Query<(&EnemyAnimations, &Transform), With<Enemy>>,
    mut animation_query: Query<&mut AnimationPlayer>,
    camera: Query<&Transform, (With<SceneCamera>, Without<Enemy>)>,
) {
    let Ok(camera_transform) = camera.single() else {
        return;
    };
    let camera_pos = camera_transform.translation;

    for (enemy_anims, enemy_transform) in &enemies {
        let Some(anim_entity) = enemy_anims.animation_player_entity else {
            continue;
        };
        let Ok(mut anim_player) = animation_query.get_mut(anim_entity) else {
            continue;
        };

        let distance = camera_pos.distance(enemy_transform.translation);

        if distance > ANIMATION_CULL_DISTANCE {
            // Pause animation for distant enemies
            for (_, active_animation) in anim_player.playing_animations_mut() {
                active_animation.set_speed(0.0);
            }
        } else {
            // Restore animation speed for nearby enemies
            for (_, active_animation) in anim_player.playing_animations_mut() {
                if active_animation.speed() == 0.0 {
                    active_animation.set_speed(1.0);
                }
            }
        }
    }
}

// =============================================================================
// Singleplayer AI — chase and attack (with spatial hash for O(n) separation)
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

    // Collect alive enemy positions for spatial hash separation
    let alive_positions: Vec<(Entity, Vec3)> = enemies
        .iter()
        .filter_map(|(entity, transform, _, _, health)| {
            if health.is_dead() {
                None
            } else {
                Some((entity, transform.translation))
            }
        })
        .collect();

    // Build spatial hash grid for O(n) enemy separation
    let cell_size = defaults::ENEMY_SEPARATION_RADIUS;
    let inv_cell_size = 1.0 / cell_size;
    let mut grid: HashMap<(i32, i32), Vec<(Entity, Vec3)>> =
        HashMap::with_capacity(alive_positions.len());

    for &(entity, pos) in &alive_positions {
        let cell = (
            (pos.x * inv_cell_size).floor() as i32,
            (pos.z * inv_cell_size).floor() as i32,
        );
        grid.entry(cell).or_default().push((entity, pos));
    }

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

        let decision = enemy_ai_decision(distance, ai.attack_cooldown.is_finished());

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

        // Enemy-enemy separation — spatial hash O(n) lookup
        let cell_x = (enemy_pos.x * inv_cell_size).floor() as i32;
        let cell_z = (enemy_pos.z * inv_cell_size).floor() as i32;
        let mut separation = Vec3::ZERO;

        for dx in -1..=1 {
            for dz in -1..=1 {
                if let Some(cell_entities) = grid.get(&(cell_x + dx, cell_z + dz)) {
                    for &(other_entity, other_pos) in cell_entities {
                        if other_entity == entity {
                            continue;
                        }
                        let diff = Vec3::new(
                            enemy_pos.x - other_pos.x,
                            0.0,
                            enemy_pos.z - other_pos.z,
                        );
                        let dist = diff.length();
                        if dist < defaults::ENEMY_SEPARATION_RADIUS && dist > 0.01 {
                            separation +=
                                diff.normalize() * (1.0 - dist / defaults::ENEMY_SEPARATION_RADIUS);
                        }
                    }
                }
            }
        }

        if separation.length() > 0.01 {
            transform.translation +=
                separation.normalize() * defaults::ENEMY_SEPARATION_STRENGTH * dt;
        }

        // Reset cooldown on attack
        if decision == EnemyBehaviorKind::Attack {
            ai.attack_cooldown.reset();
        }
    }
}
