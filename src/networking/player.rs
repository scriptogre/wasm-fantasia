//! Player-specific networking logic

use super::generated::player_table::PlayerTableAccess;
use super::{
    BufferedInboundState, LagBuffers, LagSimulator, PendingOutboundUpdate, PositionSyncTimer,
    SpacetimeDbConnection,
};
use crate::asset_loading::Models;
use crate::combat::{ArcSlash, ArcSlashAssets, AttackState, Combatant, Enemy, Health};
use crate::models::{AnimationState, Player as LocalPlayer};
use crate::player::find_animation_player_descendant;
use crate::rules::{Stat, Stats};
use avian3d::prelude::Collider;
use bevy::prelude::*;
use spacetimedb_sdk::{DbContext, Table};
use std::collections::HashMap;
use std::time::Duration;

/// Component marking an entity as a remote player
#[derive(Component, Clone, Debug)]
pub struct RemotePlayer {
    pub identity: spacetimedb_sdk::Identity,
}

/// Component storing the target position for interpolation
#[derive(Component, Clone, Debug)]
pub struct InterpolatedPosition {
    pub target: Vec3,
    pub target_rotation: f32,
}

/// Animation data for remote players.
/// `anim_player_entity` is None until the GLTF scene loads and we discover the AnimationPlayer.
#[derive(Component, Default)]
pub struct RemoteAnimations {
    pub animations: HashMap<String, AnimationNodeIndex>,
    pub current_anim: String,
    pub last_attack_seq: u32,
    pub anim_player_entity: Option<Entity>,
}

/// Animations remote players use
const REMOTE_ANIMATIONS: &[&str] = &[
    "Idle_Loop",
    "Jog_Fwd_Loop",
    "Sprint_Loop",
    "Jump_Start",
    "Jump_Land",
    "Jump_Loop",
    "Crouch_Fwd_Loop",
    "Crouch_Idle_Loop",
    "Slide_Start",
    "Slide_Loop",
    "Slide_Exit",
    "Hit_Chest",
    "Punch_Jab",
    "Punch_Cross",
    "Melee_Hook",
];

/// System to spawn remote players when they appear in the database
pub fn spawn_remote_players(
    conn: Res<SpacetimeDbConnection>,
    models: Option<Res<Models>>,
    gltf_assets: Res<Assets<Gltf>>,
    mut commands: Commands,
    existing_players: Query<&RemotePlayer>,
) {
    let Some(models) = models else { return };
    let Some(gltf) = gltf_assets.get(&models.player) else {
        return;
    };

    let our_identity = conn.conn.try_identity();

    for player in conn.conn.db.player().iter() {
        if Some(player.identity) == our_identity {
            continue;
        }
        if !player.online {
            continue;
        }
        if existing_players
            .iter()
            .any(|rp| rp.identity == player.identity)
        {
            continue;
        }

        info!(
            "Spawning remote player: {:?} at ({}, {}, {})",
            player.name, player.x, player.y, player.z
        );

        let mesh = SceneRoot(gltf.scenes[0].clone());

        commands
            .spawn((
                RemotePlayer {
                    identity: player.identity,
                },
                InterpolatedPosition {
                    target: Vec3::new(player.x, player.y, player.z),
                    target_rotation: player.rot_y,
                },
                RemoteAnimations::default(),
                Transform::from_xyz(player.x, player.y, player.z)
                    .with_rotation(Quat::from_rotation_y(player.rot_y)),
                InheritedVisibility::default(),
                // Combat components so local hit detection works
                Health::new(player.max_health),
                Combatant,
                Enemy,
                Collider::capsule(0.35, 1.3),
                Stats::new()
                    .with(Stat::MaxHealth, player.max_health)
                    .with(Stat::Health, player.health),
            ))
            .with_children(|parent| {
                parent.spawn((Transform::from_xyz(0.0, -1.0, 0.0), mesh));
            });
    }
}

/// System: discover AnimationPlayer descendants for remote players whose scenes just loaded.
/// Builds the animation graph and stores the entity reference.
pub fn setup_remote_animations(
    models: Option<Res<Models>>,
    gltf_assets: Res<Assets<Gltf>>,
    children_q: Query<&Children>,
    anim_players: Query<Entity, With<AnimationPlayer>>,
    mut remote_q: Query<(Entity, &mut RemoteAnimations)>,
    mut commands: Commands,
    mut animation_graphs: ResMut<Assets<AnimationGraph>>,
    mut animation_clips: ResMut<Assets<AnimationClip>>,
) {
    let Some(models) = models else { return };
    let Some(gltf) = gltf_assets.get(&models.player) else {
        return;
    };

    for (remote_entity, mut remote_anim) in remote_q.iter_mut() {
        // Skip already initialized
        if remote_anim.anim_player_entity.is_some() {
            continue;
        }

        // Search descendants for AnimationPlayer
        let Some(anim_entity) =
            find_animation_player_descendant(remote_entity, &children_q, &anim_players)
        else {
            continue; // Scene not loaded yet
        };

        // Build animation graph
        let mut graph = AnimationGraph::new();
        let root_node = graph.root;
        let mut anim_map = HashMap::new();

        for (name, clip_handle) in gltf.named_animations.iter() {
            if !REMOTE_ANIMATIONS.contains(&name.as_ref()) {
                continue;
            }
            let Some(original_clip) = animation_clips.get(clip_handle) else {
                continue;
            };
            let clip = original_clip.clone();
            let modified_handle = animation_clips.add(clip);
            let node_index = graph.add_clip(modified_handle, 1.0, root_node);
            anim_map.insert(name.to_string(), node_index);
        }

        info!("Initialized remote player animations ({} clips)", anim_map.len());

        commands.entity(anim_entity).insert((
            AnimationGraphHandle(animation_graphs.add(graph)),
            AnimationTransitions::new(),
        ));

        remote_anim.animations = anim_map;
        remote_anim.anim_player_entity = Some(anim_entity);

        // Start with idle
        remote_anim.current_anim = String::new();
    }
}

/// System to drive remote player animations based on anim_state from server.
/// Also spawns arc slash VFX when a new attack is detected.
pub fn animate_remote_players(
    conn: Res<SpacetimeDbConnection>,
    mut remote_q: Query<(&RemotePlayer, &mut RemoteAnimations, &Transform)>,
    mut animation_query: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    arc_assets: Option<Res<ArcSlashAssets>>,
    mut commands: Commands,
) {
    const BLEND: Duration = Duration::from_millis(150);

    for (rp, mut remote_anim, transform) in remote_q.iter_mut() {
        let Some(anim_entity) = remote_anim.anim_player_entity else {
            continue; // Not initialized yet
        };

        let Some(player) = conn.conn.db.player().identity().find(&rp.identity) else {
            continue;
        };

        // Check for new attack (attack_seq changed)
        let new_attack = player.attack_seq != remote_anim.last_attack_seq
            && player.attack_seq > 0
            && !player.attack_anim.is_empty();

        if new_attack {
            remote_anim.last_attack_seq = player.attack_seq;
            remote_anim.current_anim = format!("attack:{}", player.attack_seq);

            // Spawn arc slash at remote player's position
            if let Some(ref assets) = arc_assets {
                let pos = transform.translation + Vec3::Y * 0.8;
                commands.spawn((
                    ArcSlash {
                        timer: 0.0,
                        duration: 0.15,
                        start_scale: Vec3::new(0.3, 1.0, 0.3),
                    },
                    Mesh3d(assets.mesh.clone()),
                    MeshMaterial3d(assets.material.clone()),
                    Transform::from_translation(pos)
                        .with_rotation(transform.rotation)
                        .with_scale(Vec3::new(0.3, 1.0, 0.3)),
                ));
            }
        } else if player.anim_state == remote_anim.current_anim {
            continue;
        } else {
            remote_anim.current_anim = player.anim_state.clone();
        }

        // Determine clip: attacks use attack_anim directly, movement uses anim_state mapping
        let (clip_name, speed, should_loop) = if new_attack {
            let speed = if player.attack_anim == "Melee_Hook" { 1.1 } else { 1.3 };
            (player.attack_anim.as_str(), speed, false)
        } else {
            match player.anim_state.as_str() {
                "Idle" => ("Idle_Loop", 1.0, true),
                "Walk" => ("Jog_Fwd_Loop", 0.6, true),
                "Run" => ("Sprint_Loop", 1.5, true),
                "Jump" => ("Jump_Loop", 0.5, true),
                "Crouch" => ("Crouch_Idle_Loop", 1.0, true),
                "Hit" => ("Hit_Chest", 1.0, false),
                "Slide" => ("Slide_Loop", 1.0, true),
                "Attack" => continue, // Wait for attack_seq to change
                _ => ("Idle_Loop", 1.0, true),
            }
        };

        let Some(index) = remote_anim.animations.get(clip_name) else {
            continue;
        };

        let Ok((mut anim_player, mut transitions)) = animation_query.get_mut(anim_entity) else {
            continue;
        };

        let active = transitions.play(&mut anim_player, *index, BLEND).set_speed(speed);
        if should_loop {
            active.repeat();
        }
    }
}

/// System to buffer incoming remote player updates from SpacetimeDB
pub fn buffer_inbound_updates(
    conn: Res<SpacetimeDbConnection>,
    lag: Res<LagSimulator>,
    mut buffers: ResMut<LagBuffers>,
) {
    if lag.inbound_delay_ms == 0 && lag.packet_loss_chance == 0.0 {
        return;
    }

    let now = std::time::Instant::now();

    for player in conn.conn.db.player().iter() {
        if !player.online {
            continue;
        }

        if lag.packet_loss_chance > 0.0 && rand::random::<f32>() < lag.packet_loss_chance {
            continue;
        }

        buffers.inbound_buffer.insert(
            player.identity,
            BufferedInboundState {
                x: player.x,
                y: player.y,
                z: player.z,
                rot_y: player.rot_y,
                received_at: now,
            },
        );
    }
}

/// System to update remote player positions from SpacetimeDB
pub fn update_remote_players(
    conn: Res<SpacetimeDbConnection>,
    lag: Res<LagSimulator>,
    buffers: Res<LagBuffers>,
    mut query: Query<(&RemotePlayer, &mut InterpolatedPosition)>,
) {
    let now = std::time::Instant::now();

    for (rp, mut interp) in query.iter_mut() {
        if lag.inbound_delay_ms == 0 && lag.packet_loss_chance == 0.0 {
            if let Some(player) = conn.conn.db.player().identity().find(&rp.identity) {
                interp.target = Vec3::new(player.x, player.y, player.z);
                interp.target_rotation = player.rot_y;
            }
        } else {
            if let Some(state) = buffers.inbound_buffer.get(&rp.identity) {
                let elapsed = now.duration_since(state.received_at).as_millis() as u64;
                if elapsed >= lag.inbound_delay_ms {
                    interp.target = Vec3::new(state.x, state.y, state.z);
                    interp.target_rotation = state.rot_y;
                }
            }
        }
    }
}

/// System to despawn remote players when they disconnect
pub fn despawn_remote_players(
    conn: Res<SpacetimeDbConnection>,
    mut commands: Commands,
    query: Query<(Entity, &RemotePlayer)>,
) {
    for (entity, rp) in query.iter() {
        let should_despawn = conn
            .conn
            .db
            .player()
            .identity()
            .find(&rp.identity)
            .map(|p| !p.online)
            .unwrap_or(true);

        if should_despawn {
            info!("Despawning remote player: {:?}", rp.identity);
            commands.entity(entity).despawn();
        }
    }
}

/// System to interpolate remote player positions
pub fn interpolate_positions(
    mut query: Query<(&mut Transform, &InterpolatedPosition), With<RemotePlayer>>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    const LERP_SPEED: f32 = 10.0;

    for (mut transform, interp) in query.iter_mut() {
        transform.translation = transform.translation.lerp(interp.target, LERP_SPEED * dt);

        let current_y = transform.rotation.to_euler(EulerRot::YXZ).0;
        let rot_lerp = current_y + (interp.target_rotation - current_y) * (LERP_SPEED * dt);
        transform.rotation = Quat::from_rotation_y(rot_lerp);
    }
}

/// Map local AnimationState to movement animation string (no attack info).
fn animation_state_to_string(state: &AnimationState) -> &'static str {
    match state {
        AnimationState::StandIdle => "Idle",
        AnimationState::Run(_) => "Walk",
        AnimationState::Sprint(_) => "Run",
        AnimationState::JumpStart | AnimationState::JumpLoop | AnimationState::JumpLand | AnimationState::Fall => "Jump",
        AnimationState::Crouch(_) | AnimationState::CrouchIdle => "Crouch",
        AnimationState::SlideStart | AnimationState::SlideLoop | AnimationState::SlideExit => "Slide",
        AnimationState::KnockBack => "Hit",
        AnimationState::Attack => "Attack",
        AnimationState::Climb(_) => "Jump",
        AnimationState::WallSlide | AnimationState::WallJump => "Jump",
    }
}

/// Determine attack animation clip name from AttackState.
fn attack_anim_name(attack: &AttackState) -> &'static str {
    if attack.is_crit {
        "Melee_Hook"
    } else if attack.attack_count % 2 == 1 {
        "Punch_Jab"
    } else {
        "Punch_Cross"
    }
}

/// System to send local player position to the server
pub fn send_local_position(
    lag: Res<LagSimulator>,
    mut buffers: ResMut<LagBuffers>,
    mut timer: ResMut<PositionSyncTimer>,
    time: Res<Time>,
    query: Query<(&Transform, &LocalPlayer, Option<&AttackState>), With<LocalPlayer>>,
) {
    timer.timer.tick(time.delta());

    if !timer.timer.just_finished() {
        return;
    }

    let Ok((transform, player, attack_state)) = query.single() else {
        return;
    };

    let pos = transform.translation;
    let rot_y = transform.rotation.to_euler(EulerRot::YXZ).0;
    let anim_state = animation_state_to_string(&player.animation_state).to_string();

    let (attack_seq, attack_anim) = if let Some(attack) = attack_state {
        (attack.attack_count, attack_anim_name(attack).to_string())
    } else {
        (0, String::new())
    };

    let update = PendingOutboundUpdate {
        x: pos.x,
        y: pos.y,
        z: pos.z,
        rot_y,
        anim_state,
        attack_seq,
        attack_anim,
        send_at: if lag.outbound_delay_ms == 0 && lag.packet_loss_chance == 0.0 {
            std::time::Instant::now()
        } else {
            std::time::Instant::now() + std::time::Duration::from_millis(lag.outbound_delay_ms)
        },
    };

    buffers.outbound_queue.push(update);
}
