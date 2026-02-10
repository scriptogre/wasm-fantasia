use super::*;
use crate::asset_loading::Models;
use crate::models::SpawnEnemy;
use crate::player::{Animation, find_animation_player_descendant};
use bevy::scene::SceneInstanceReady;
use bevy_enhanced_input::prelude::Start;
use std::time::Duration;

/// Enemies beyond this distance from the camera have their animations paused.
const ANIMATION_CULL_DISTANCE: f32 = 30.0;
const ANIMATION_CULL_DISTANCE_SQ: f32 = ANIMATION_CULL_DISTANCE * ANIMATION_CULL_DISTANCE;

pub fn plugin(app: &mut App) {
    app.add_observer(spawn_enemy_in_front)
        .add_observer(on_enemy_added)
        .add_systems(
            Update,
            (
                cull_enemy_animations.run_if(in_state(Screen::Gameplay)),
                animate_enemies
                    .in_set(PostPhysicsAppSystems::PlayAnimations)
                    .run_if(in_state(Screen::Gameplay)),
            ),
        );
}

// =============================================================================
// Spawn trigger (E key / server request)
// =============================================================================

/// Spawn a pack of enemies via server reducer.
/// All game modes go through SpacetimeDB when connected.
fn spawn_enemy_in_front(
    _on: On<Start<SpawnEnemy>>,
    player: Query<&Transform, With<Player>>,
    #[cfg(feature = "multiplayer")] conn: Option<Res<crate::networking::SpacetimeDbConnection>>,
) {
    let Ok(player_transform) = player.single() else {
        return;
    };

    let forward = player_transform.forward();
    let pos = player_transform.translation;

    #[cfg(feature = "multiplayer")]
    if let Some(conn) = conn {
        use spacetimedb_sdk::DbContext;
        if conn.conn.is_active() {
            crate::networking::combat::server_spawn_enemies(&conn, pos, forward.as_vec3());
            debug!("Requested enemies from server");
            return;
        }
    }

    warn!("No server connection — cannot spawn enemies");
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

    // Remove capsule mesh if present (reconciler may have added it)
    commands
        .entity(entity)
        .remove::<Mesh3d>()
        .remove::<MeshMaterial3d<StandardMaterial>>();

    // Insert behavior components + visibility
    commands.entity(entity).insert((
        EnemyBehavior::default(),
        EnemyAnimations::default(),
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
            // Share the original clip handle — each AnimationPlayer tracks its own
            // playback state independently, so the clip data itself can be shared.
            let node_index = graph.add_clip(clip_handle.clone(), 1.0, root_node);
            enemy_animations.animations.insert(anim, node_index);
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
// Distance-based animation culling — pause animations for far enemies
// =============================================================================

fn cull_enemy_animations(
    enemies: Query<(&Transform, &EnemyAnimations), With<Enemy>>,
    camera: Query<&Transform, (With<SceneCamera>, Without<Enemy>)>,
    mut animation_players: Query<&mut AnimationPlayer>,
) {
    let Ok(camera_transform) = camera.single() else {
        return;
    };
    let camera_pos = camera_transform.translation;

    for (transform, anims) in &enemies {
        let Some(anim_entity) = anims.animation_player_entity else {
            continue;
        };
        let Ok(mut anim_player) = animation_players.get_mut(anim_entity) else {
            continue;
        };

        let distance_sq = transform.translation.distance_squared(camera_pos);
        if distance_sq > ANIMATION_CULL_DISTANCE_SQ {
            anim_player.pause_all();
        } else {
            anim_player.resume_all();
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
