use crate::combat::{AttackState, Combatant, Health, PlayerCombatant};
use crate::rule_presets;
use crate::rules::{Stat, Stats};
use crate::*;
use avian3d::prelude::*;
use bevy::scene::SceneInstanceReady;
use bevy_enhanced_input::prelude::*;
use bevy_third_person_camera::*;
use bevy_tnua::TnuaAnimatingState;
use bevy_tnua::builtins::*;
use bevy_tnua::control_helpers::{TnuaActionSlots, TnuaAirActionsPlugin};
use bevy_tnua::prelude::*;
use bevy_tnua_avian3d::*;
use std::collections::HashMap;
use std::time::Duration;
use wasm_fantasia_shared::combat::defaults;

// ── Tnua Control Scheme ─────────────────────────────────────────────

#[derive(TnuaScheme)]
#[scheme(basis = TnuaBuiltinWalk)]
pub enum ControlScheme {
    Jump(TnuaBuiltinJump),
    Dash(TnuaBuiltinDash),
    Crouch(TnuaBuiltinCrouch),
    Knockback(TnuaBuiltinKnockback),
    Climb(TnuaBuiltinClimb),
    WallSlide(TnuaBuiltinWallSlide),
}

#[derive(TnuaActionSlots)]
#[slots(scheme = ControlScheme)]
pub struct AirActionSlots {
    #[slots(Jump)]
    jump: usize,
    #[slots(Dash)]
    dash: usize,
}

mod animation;
pub mod control;
mod sound;

pub use animation::*;

/// This plugin handles player related stuff like movement, shooting
/// Player logic is only active during the State `Screen::Playing`
pub fn plugin(app: &mut App) {
    app.add_plugins((
        TnuaControllerPlugin::<ControlScheme>::new(FixedUpdate),
        TnuaAvian3dPlugin::new(FixedUpdate),
        TnuaAirActionsPlugin::<AirActionSlots>::new(FixedUpdate),
        control::plugin,
        sound::plugin,
    ));

    app.add_plugins(ThirdPersonCameraPlugin).configure_sets(
        PostUpdate,
        bevy_third_person_camera::CameraSyncSet.before(TransformSystems::Propagate),
    );

    app.add_systems(OnEnter(Screen::Gameplay), spawn_player)
        .add_systems(
            Update,
            (
                animating.in_set(TnuaUserControlsSystems),
                animate_remote_players.in_set(PostPhysicsAppSystems::PlayAnimations),
                sync_debug_colliders,
            )
                .run_if(in_state(Screen::Gameplay)),
        )
        .add_observer(player_post_spawn)
        .add_observer(on_remote_player_added);
}

pub fn spawn_player(
    cfg: Res<Config>,
    models: Res<Models>,
    gltf_assets: Res<Assets<Gltf>>,
    mut commands: Commands,
    mut control_scheme_configs: ResMut<Assets<ControlSchemeConfig>>,
    // DEBUG
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) -> Result {
    let Some(gltf) = gltf_assets.get(&models.player) else {
        return Ok(());
    };

    let mesh = SceneRoot(gltf.scenes[0].clone());
    let pos = Vec3::from(cfg.player.spawn_pos);
    let pos = Transform::from_translation(pos);
    let player = Player {
        speed: cfg.player.movement.speed,
        animation_state: AnimationState::StandIdle,
        ..default()
    };
    let collider = Collider::capsule(cfg.player.hitbox.radius, cfg.player.hitbox.height);

    commands
        .spawn((
            pos,
            player,
            ThirdPersonCameraTarget,
            // PlayerCtx is NOT inserted here — sync_gameplay_lock adds it
            // when no BlocksGameplay entities exist and the game isn't paused.
            // tnua character control bundles
            (
                TnuaController::<ControlScheme>::default(),
                TnuaConfig::<ControlScheme>(control_scheme_configs.add(ControlSchemeConfig {
                    basis: TnuaBuiltinWalkConfig {
                        // speed=1.0 so desired_motion carries the full velocity
                        speed: 1.0,
                        float_height: 0.15,
                        cling_distance: 0.20,
                        spring_strength: 500.0,
                        spring_dampening: 1.0,
                        acceleration: 80.0,
                        air_acceleration: 60.0,
                        free_fall_extra_gravity: 60.0,
                        tilt_offset_angvel: 7.0,
                        tilt_offset_angacl: 700.0,
                        turning_angvel: 12.0,
                        ..default()
                    },
                    jump: TnuaBuiltinJumpConfig {
                        height: control::MIN_JUMP_HEIGHT,
                        takeoff_extra_gravity: 20.0,
                        fall_extra_gravity: 60.0,
                        shorten_extra_gravity: 10.0,
                        peak_prevention_at_upward_velocity: 2.0,
                        peak_prevention_extra_gravity: 25.0,
                        reschedule_cooldown: Some(0.05),
                        disable_force_forward_after_peak: false,
                        ..default()
                    },
                    dash: TnuaBuiltinDashConfig {
                        speed: 12.0,
                        ..default()
                    },
                    crouch: TnuaBuiltinCrouchConfig {
                        float_offset: 0.0,
                        height_change_impulse_for_duration: 0.1,
                        height_change_impulse_limit: 80.0,
                    },
                    knockback: TnuaBuiltinKnockbackConfig::default(),
                    climb: TnuaBuiltinClimbConfig::default(),
                    wall_slide: TnuaBuiltinWallSlideConfig::default(),
                })),
                // Tnua can fix the rotation, but the character will still get rotated before it can do so.
                // By locking the rotation we can prevent this.
                LockedAxes::ROTATION_LOCKED.unlock_rotation_y(),
                TnuaAnimatingState::<AnimationState>::default(),
                animation::AttackAnimationState::default(),
                // A sensor shape is not strictly necessary, but without it we'll get weird results.
                TnuaAvian3dSensorShape(collider.clone()),
            ),
            // physics
            (
                collider,
                RigidBody::Dynamic,
                Friction::ZERO.with_combine_rule(CoefficientCombine::Multiply),
            ),
            // other player related components
            (
                JumpTimer(Timer::from_seconds(cfg.timers.jump, TimerMode::Repeating)),
                StepTimer(Timer::from_seconds(cfg.timers.step, TimerMode::Repeating)),
                control::JumpCharge::default(),
                control::AirborneTracker::default(),
                InheritedVisibility::default(), // silence the warning because of adding SceneRoot as a child
            ),
            // combat components
            (
                Health::new(100.0),
                AttackState::new(0.15), // Fast attack chaining
                Combatant,
                PlayerCombatant,
            ),
            // rules system - base stats + shared rules
            Stats::new()
                .with(Stat::MaxHealth, defaults::HEALTH)
                .with(Stat::Health, defaults::HEALTH)
                .with(Stat::AttackDamage, defaults::ATTACK_DAMAGE)
                .with(Stat::Knockback, defaults::KNOCKBACK)
                .with(Stat::AttackRange, defaults::ATTACK_RANGE)
                .with(Stat::AttackArc, defaults::ATTACK_ARC)
                .with(Stat::CritChance, defaults::CRIT_CHANCE)
                .with(Stat::CritMultiplier, defaults::CRIT_MULTIPLIER),
            rule_presets::rules_bundle(wasm_fantasia_shared::presets::default_player_rules()),
        ))
        // spawn character mesh as child to adjust mesh position relative to the player origin
        .with_children(|parent| {
            let mut e = parent.spawn((Transform::from_xyz(0.0, -1.0, 0.0), mesh));
            e.observe(prepare_animations);

            let collider_mesh = meshes.add(Capsule3d::new(
                cfg.player.hitbox.radius,
                cfg.player.hitbox.height,
            ));
            parent.spawn((
                DebugCollider,
                Mesh3d(collider_mesh),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: crate::ui::colors::NEUTRAL200.with_alpha(0.1),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    ..default()
                })),
                Transform::from_xyz(0.0, -0.1, 0.0),
                Visibility::Hidden,
            ));
        });

    Ok(())
}

#[derive(Component)]
struct DebugCollider;

fn sync_debug_colliders(
    state: Res<Session>,
    mut colliders: Query<&mut Visibility, With<DebugCollider>>,
) {
    let vis = if state.debug_ui {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut v in &mut colliders {
        *v = vis;
    }
}

fn player_post_spawn(on: On<Add, Player>, mut players: Query<&mut Player>) {
    if let Ok(mut p) = players.get_mut(on.entity) {
        p.id = on.entity; // update player id with spawned entity
    }
}

// =============================================================================
// Remote players — GLTF model, animations, debug collider
// =============================================================================

/// Marker for remote (non-local) player entities.
#[derive(Component)]
pub struct RemotePlayer;

/// Animation state for a remote player, driven by server-synced data.
#[derive(Component, Default)]
struct RemotePlayerAnimations {
    animations: HashMap<Animation, AnimationNodeIndex>,
    animation_player_entity: Option<Entity>,
    current_animation: Option<Animation>,
    last_attack_sequence: u32,
    /// Time (elapsed secs) when the current attack animation should yield to movement.
    attack_playing_until: f32,
}

const ATTACK_ANIMATION_DURATION: f32 = 0.4;

fn on_remote_player_added(
    on: On<Add, RemotePlayer>,
    cfg: Res<Config>,
    models: Res<Models>,
    gltf_assets: Res<Assets<Gltf>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut commands: Commands,
) {
    let entity = on.entity;

    commands.entity(entity).insert((
        RemotePlayerAnimations::default(),
        InheritedVisibility::default(),
    ));

    let Some(gltf) = gltf_assets.get(&models.player) else {
        warn!("Player GLTF not loaded when remote player spawned");
        return;
    };

    let scene = SceneRoot(gltf.scenes[0].clone());
    commands.entity(entity).with_children(|parent| {
        let mut child = parent.spawn((Transform::from_xyz(0.0, -1.0, 0.0), scene));
        child.observe(prepare_remote_player_scene);

        // Debug collider visualization (toggled by sync_debug_colliders)
        let collider_mesh = meshes.add(Capsule3d::new(
            cfg.player.hitbox.radius,
            cfg.player.hitbox.height,
        ));
        parent.spawn((
            DebugCollider,
            Mesh3d(collider_mesh),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.2, 0.6, 1.0, 0.15),
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })),
            Transform::from_xyz(0.0, -0.1, 0.0),
            Visibility::Hidden,
        ));
    });
}

fn prepare_remote_player_scene(
    on: On<SceneInstanceReady>,
    models: Res<Models>,
    gltf_assets: Res<Assets<Gltf>>,
    children_q: Query<&Children>,
    anim_players: Query<Entity, With<AnimationPlayer>>,
    parents: Query<&ChildOf>,
    mut remote_q: Query<&mut RemotePlayerAnimations>,
    mut commands: Commands,
    mut animation_graphs: ResMut<Assets<AnimationGraph>>,
) {
    let scene_entity = on.entity;

    let Some(gltf) = gltf_assets.get(&models.player) else {
        return;
    };

    let Some(animation_player_entity) =
        find_animation_player_descendant(scene_entity, &children_q, &anim_players)
    else {
        return;
    };

    // Walk up to the RemotePlayer entity (scene entity → remote player entity)
    let remote_entity = if let Ok(parent) = parents.get(scene_entity) {
        parent.parent()
    } else {
        scene_entity
    };

    let Ok(mut anims) = remote_q.get_mut(remote_entity) else {
        return;
    };

    // Build animation graph with all player clips (same set as local player)
    let mut graph = AnimationGraph::new();
    let root_node = graph.root;

    for (name, clip_handle) in gltf.named_animations.iter() {
        let Some(anim) = Animation::from_clip_name(name) else {
            continue;
        };
        let node_index = graph.add_clip(clip_handle.clone(), 1.0, root_node);
        anims.animations.insert(anim, node_index);
    }

    anims.animation_player_entity = Some(animation_player_entity);

    let idle_node = anims.animations.get(&Animation::Idle).copied();
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

    anims.current_animation = Some(Animation::Idle);
}

/// Drive remote player animations from server-synced state.
/// Uses `AnimationState::from_server_name()` + `playback()` — the same source
/// of truth the local player's animation system is built around.
fn animate_remote_players(
    time: Res<Time>,
    mut remotes: Query<(
        &crate::networking::RemotePlayerState,
        &mut RemotePlayerAnimations,
    )>,
    mut animation_query: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    const BLEND_DURATION: Duration = Duration::from_millis(150);
    let now = time.elapsed_secs();

    for (state, mut anims) in &mut remotes {
        let Some(anim_entity) = anims.animation_player_entity else {
            continue;
        };
        let Ok((mut anim_player, mut transitions)) = animation_query.get_mut(anim_entity) else {
            continue;
        };

        // Detect new attack (takes priority over movement animations)
        if state.attack_sequence != anims.last_attack_sequence && state.attack_sequence > 0 {
            anims.last_attack_sequence = state.attack_sequence;

            if let Some(attack_anim) = Animation::from_clip_name(&state.attack_animation) {
                if let Some(&index) = anims.animations.get(&attack_anim) {
                    anims.current_animation = Some(attack_anim);
                    anims.attack_playing_until = now + ATTACK_ANIMATION_DURATION;
                    transitions
                        .play(&mut anim_player, index, BLEND_DURATION)
                        .set_speed(1.3);
                    continue;
                }
            }
        }

        // Don't interrupt a playing attack animation
        if now < anims.attack_playing_until {
            continue;
        }

        // Movement animation via shared AnimationState mapping
        let anim_state = AnimationState::from_server_name(&state.animation_state);
        let (clip, speed, looping) = anim_state.playback();

        if anims.current_animation == Some(clip) {
            continue;
        }

        let Some(&index) = anims.animations.get(&clip) else {
            continue;
        };

        anims.current_animation = Some(clip);
        let active = transitions.play(&mut anim_player, index, BLEND_DURATION);
        active.set_speed(speed);
        if looping {
            active.repeat();
        }
    }
}
