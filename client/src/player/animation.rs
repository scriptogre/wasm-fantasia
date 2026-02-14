use super::*;
use crate::combat::AttackState;
use crate::player::control::{GroundPoundState, JumpCharge, LandingStun, RollingState};
use crate::rules::{Stat, Stats};
use bevy_tnua::{TnuaAnimatingState, TnuaAnimatingStateDirective};

mod anim_knobs {
    pub const GENERAL_SPEED: f32 = 0.1;
    pub const CROUCH_ANIMATION_SPEED: f32 = 2.2;
}

/// Track which attack animation is playing to detect new attacks reliably
#[derive(Component, Default)]
pub struct AttackAnimationState {
    /// Last attack count we started an animation for
    pub last_attack_count: u32,
}

/// GLTF animation clips the game uses. Single source of truth for both local and remote players.
/// Unused clips are skipped during loading to save memory (especially on WASM).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
pub enum Animation {
    Idle,
    JogFwd,
    Sprint,
    JumpStart,
    JumpLand,
    JumpLoop,
    NinjaJumpStart,
    NinjaJumpIdle,
    NinjaJumpLand,
    Roll,
    CrouchFwd,
    CrouchIdle,
    HitChest,
    PunchJab,
    PunchCross,
    MeleeHook,
    ZombieIdle,
    ZombieWalkForward,
    ZombieScratch,
}

impl Animation {
    /// All variants — used for loading and validation.
    pub const ALL: &[Animation] = &[
        Self::Idle,
        Self::JogFwd,
        Self::Sprint,
        Self::JumpStart,
        Self::JumpLand,
        Self::JumpLoop,
        Self::NinjaJumpStart,
        Self::NinjaJumpIdle,
        Self::NinjaJumpLand,
        Self::Roll,
        Self::CrouchFwd,
        Self::CrouchIdle,
        Self::HitChest,
        Self::PunchJab,
        Self::PunchCross,
        Self::MeleeHook,
        Self::ZombieIdle,
        Self::ZombieWalkForward,
        Self::ZombieScratch,
    ];

    /// Maps to the clip name inside the GLTF file.
    pub fn clip_name(self) -> &'static str {
        match self {
            Self::Idle => "Idle_Loop",
            Self::JogFwd => "Jog_Fwd_Loop",
            Self::Sprint => "Sprint_Loop",
            Self::JumpStart => "Jump_Start",
            Self::JumpLand => "Jump_Land",
            Self::JumpLoop => "Jump_Loop",
            Self::NinjaJumpStart => "NinjaJump_Start",
            Self::NinjaJumpIdle => "NinjaJump_Idle_Loop",
            Self::NinjaJumpLand => "NinjaJump_Land",
            Self::Roll => "Roll",
            Self::CrouchFwd => "Crouch_Fwd_Loop",
            Self::CrouchIdle => "Crouch_Idle_Loop",
            Self::HitChest => "Hit_Chest",
            Self::PunchJab => "Punch_Jab",
            Self::PunchCross => "Punch_Cross",
            Self::MeleeHook => "Melee_Hook",
            Self::ZombieIdle => "Zombie_Idle_Loop",
            Self::ZombieWalkForward => "Zombie_Walk_Fwd_Loop",
            Self::ZombieScratch => "Zombie_Scratch",
        }
    }

    /// Reverse lookup: GLTF clip name → enum variant.
    pub fn from_clip_name(name: &str) -> Option<Self> {
        Self::ALL.iter().find(|a| a.clip_name() == name).copied()
    }
}

/// Recursively find the first entity with AnimationPlayer in a subtree.
pub fn find_animation_player_descendant(
    entity: Entity,
    children_q: &Query<&Children>,
    anim_players: &Query<Entity, With<AnimationPlayer>>,
) -> Option<Entity> {
    if anim_players.get(entity).is_ok() {
        return Some(entity);
    }
    if let Ok(children) = children_q.get(entity) {
        for child in children.iter() {
            if let Some(found) = find_animation_player_descendant(child, children_q, anim_players) {
                return Some(found);
            }
        }
    }
    None
}

pub fn prepare_animations(
    on: On<SceneInstanceReady>,
    models: Res<Models>,
    gltf_assets: Res<Assets<Gltf>>,
    children_q: Query<&Children>,
    anim_players: Query<Entity, With<AnimationPlayer>>,
    parents: Query<&ChildOf>,
    mut player: Query<&mut Player>,
    mut commands: Commands,
    mut animation_graphs: ResMut<Assets<AnimationGraph>>,
    mut animation_clips: ResMut<Assets<AnimationClip>>,
) {
    let Some(gltf) = gltf_assets.get(&models.player) else {
        return;
    };

    // Find AnimationPlayer as descendant of the scene entity that just loaded
    let scene_entity = on.entity;
    let Some(animation_player) =
        find_animation_player_descendant(scene_entity, &children_q, &anim_players)
    else {
        return;
    };

    // Walk up to find the Player entity (scene entity -> player entity)
    let player_entity = if let Ok(parent) = parents.get(scene_entity) {
        parent.parent()
    } else {
        scene_entity
    };
    let Ok(mut player) = player.get_mut(player_entity) else {
        return;
    };

    let mut graph = AnimationGraph::new();
    let root_node = graph.root;

    // Create flat animation graph (only load animations we actually use)
    for (name, clip_handle) in gltf.named_animations.iter() {
        let Some(anim) = Animation::from_clip_name(name) else {
            continue;
        };

        let Some(original_clip) = animation_clips.get(clip_handle) else {
            continue;
        };

        let clip = original_clip.clone();
        let modified_handle = animation_clips.add(clip);
        let node_index = graph.add_clip(modified_handle, 1.0, root_node);
        player.animations.insert(anim, node_index);
    }

    info!("Loaded {} animations", player.animations.len());

    // Debug: warn about any expected animations missing from the model
    #[cfg(debug_assertions)]
    for anim in Animation::ALL {
        if !player.animations.contains_key(anim) {
            warn!(
                "Animation {:?} ({}) not found in player model",
                anim,
                anim.clip_name()
            );
        }
    }

    player.anim_player_entity = Some(animation_player);

    let idle_node = player.animations.get(&Animation::Idle).copied();
    let graph_handle = animation_graphs.add(graph);

    commands.entity(animation_player).insert((
        AnimationGraphHandle(graph_handle),
        AnimationTransitions::new(),
    ));

    // Start idle animation immediately to avoid T-pose on first frame
    if let Some(index) = idle_node {
        commands
            .entity(animation_player)
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
}

/// Tnua takes the heavy lifting with blending animations, but it leads to most of the animation
/// being hidden behind tnua systems. Not for everyone, but definittely worth it as tnua implements
/// more actions
/// <https://github.com/idanarye/bevy-tnua/blob/main/demos/src/character_animating_systems/platformer_animating_systems.rs>
///
/// Note: if you are not interested in using tnua you can just delete
/// all tnua related stuff and it should still work
pub fn animating(
    cfg: Res<Config>,
    mut player_q: Query<(
        &TnuaController<ControlScheme>,
        &mut Player,
        &mut TnuaAnimatingState<AnimationState>,
        Option<&AttackState>,
        Option<&Stats>,
        &mut AttackAnimationState,
        &JumpCharge,
        Option<&RollingState>,
        Option<&LandingStun>,
        Option<&GroundPoundState>,
    )>,
    mut animation_query: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    let Ok((
        controller,
        mut player,
        mut animating_state,
        attack_state,
        stats,
        mut attack_anim,
        jump_charge,
        rolling_state,
        landing_stun,
        ground_pound,
    )) = player_q.single_mut()
    else {
        return;
    };

    // Get attack speed multiplier from rules (0.0 means "not set", treat as 1.0)
    let speed_mult = stats
        .map(|s| {
            let speed = s.get(&Stat::AttackSpeed);
            if speed == 0.0 { 1.0 } else { speed }
        })
        .unwrap_or(1.0);

    // Look up the specific AnimationPlayer for this player entity
    let Some(anim_entity) = player.anim_player_entity else {
        return;
    };
    let Ok((mut animation_player, mut transitions)) = animation_query.get_mut(anim_entity) else {
        return;
    };

    // Blend duration for smooth transitions
    const BLEND_DURATION: Duration = Duration::from_millis(150);

    // Check if player is attacking - override Tnua animation
    if let Some(attack) = attack_state {
        if attack.is_attacking() {
            player.animation_state = AnimationState::Attack;
            // Keep TnuaAnimatingState in sync (for when attack ends)
            animating_state.update_by_discriminant(AnimationState::Attack);

            // Select animation: hook for crits, alternate jab/cross for normal attacks
            let anim = if attack.is_crit {
                Animation::MeleeHook
            } else if attack.attack_count % 2 == 1 {
                Animation::PunchJab
            } else {
                Animation::PunchCross
            };

            // Detect new attack by comparing attack_count (gameplay truth)
            // This is reliable regardless of system ordering or TnuaAnimatingState bugs
            let is_new_attack = attack.attack_count != attack_anim.last_attack_count;

            if is_new_attack {
                attack_anim.last_attack_count = attack.attack_count;

                if let Some(index) = player.animations.get(&anim) {
                    // Start at base speed - wind-up should look normal
                    // Hook is slightly slower for dramatic effect
                    let start_speed = if attack.is_crit { 1.1 } else { 1.3 };
                    transitions
                        .play(&mut animation_player, *index, BLEND_DURATION)
                        .set_speed(start_speed);
                }
            } else {
                // Speed curve: keep wind-up/impact readable, speed up recovery
                // This ensures punches always reach full extension visually
                let progress = attack.progress();

                // Wind-up to impact (0-55%): minimal speed boost
                // Recovery (55-100%): heavy speed boost from stacks
                let base_speed = if attack.is_crit { 1.1 } else { 1.3 };
                let anim_speed = if progress < 0.55 {
                    // Wind-up and impact: slight boost only
                    base_speed + (speed_mult - 1.0) * 0.25
                } else {
                    // Recovery: full speed boost kicks in
                    base_speed * speed_mult
                };

                for (_, anim) in animation_player.playing_animations_mut() {
                    anim.set_speed(anim_speed);
                }
            }
            return;
        }
    }

    // Dodge roll: force Roll animation while rolling.
    if rolling_state.is_some() {
        player.animation_state = AnimationState::Roll;
        let directive = animating_state.update_by_discriminant(AnimationState::Roll);
        if let TnuaAnimatingStateDirective::Alter { .. } = directive {
            if let Some(index) = player.animations.get(&Animation::Roll) {
                transitions
                    .play(&mut animation_player, *index, Duration::from_millis(120))
                    .set_speed(1.3);
            }
        }
        return;
    }

    // Ground pound: diving animation while slamming down.
    if ground_pound.is_some() {
        player.animation_state = AnimationState::GroundPound;
        let directive = animating_state.update_by_discriminant(AnimationState::GroundPound);
        if let TnuaAnimatingStateDirective::Alter { .. } = directive {
            if let Some(index) = player.animations.get(&Animation::NinjaJumpStart) {
                transitions
                    .play(&mut animation_player, *index, Duration::from_millis(80))
                    .set_speed(1.5);
            }
        }
        return;
    }

    // Landing stun: force landing animation on impact — slow crouch-in, fast snap-out.
    if let Some(stun) = landing_stun {
        player.animation_state = AnimationState::LandingStun;
        let directive = animating_state.update_by_discriminant(AnimationState::LandingStun);
        match directive {
            TnuaAnimatingStateDirective::Alter { .. } => {
                if let Some(index) = player.animations.get(&Animation::NinjaJumpLand) {
                    transitions
                        .play(&mut animation_player, *index, Duration::from_millis(50))
                        .set_speed(0.6);
                }
            }
            TnuaAnimatingStateDirective::Maintain { .. } => {
                // Ramp from 0.6x (dramatic impact) to 1.8x (fast recovery)
                let frac = stun.timer.fraction();
                let speed = 0.6 + 1.2 * frac;
                for (_, active_animation) in animation_player.playing_animations_mut() {
                    active_animation.set_speed(speed);
                }
            }
        }
        return;
    }

    // Charge jump: wind-up pose while storing energy — only show when grounded.
    // Uses JumpStart animation slowed down to convey coiling power.
    if jump_charge.charging && controller.basis_memory.standing_on_entity().is_some() {
        player.animation_state = AnimationState::JumpStart;
        let directive = animating_state.update_by_discriminant(AnimationState::JumpStart);
        match directive {
            TnuaAnimatingStateDirective::Alter { .. } => {
                if let Some(index) = player.animations.get(&Animation::NinjaJumpStart) {
                    transitions
                        .play(&mut animation_player, *index, Duration::from_millis(100))
                        .set_speed(0.3);
                }
            }
            TnuaAnimatingStateDirective::Maintain { .. } => {
                // Hold near the coiled-down pose — slow down further as charge builds
                let charge_t = (jump_charge.charge_time
                    / crate::player::control::MAX_CHARGE_TIME)
                    .clamp(0.0, 1.0);
                // Start at 0.3x, slow to near-freeze at 0.05x as energy builds
                let speed = 0.3 - 0.25 * charge_t;
                for (_, active_animation) in animation_player.playing_animations_mut() {
                    active_animation.set_speed(speed);
                }
            }
        }
        return;
    }

    // Here we use the data from TnuaController to determine what the character is currently doing,
    // so that we can later use that information to decide which animation to play.
    let current_animation = match controller.current_action.as_ref() {
        Some(ControlSchemeActionState::Knockback(state)) => match &state.memory {
            TnuaBuiltinKnockbackMemory::Shove => AnimationState::KnockBack,
            TnuaBuiltinKnockbackMemory::Pushback { .. } => AnimationState::KnockBack,
        },
        Some(ControlSchemeActionState::Crouch(state)) => {
            // In case of crouch, we need the state of the basis to determine - based on
            // the speed - if the character is just crouching or also crawling.
            let basis_speed = controller.basis_memory.running_velocity.length();
            let speed = Some(basis_speed)
                .filter(|speed| cfg.player.movement.idle_to_run_threshold < *speed);
            let is_crouching = controller.basis_memory.standing_offset.y < 0.05;
            match (speed, is_crouching) {
                (None, false) => AnimationState::StandIdle,
                (None, true) => match &state.memory {
                    TnuaBuiltinCrouchMemory::Maintaining => AnimationState::CrouchIdle,
                    // TODO: have rise animation
                    TnuaBuiltinCrouchMemory::Rising => AnimationState::CrouchIdle,
                    // TODO: sink animation
                    TnuaBuiltinCrouchMemory::Sinking => AnimationState::CrouchIdle,
                },
                (Some(speed), false) => AnimationState::Run(speed),
                // TODO: place to handle slide here
                (Some(speed), true) => AnimationState::Crouch(speed),
            }
        }
        Some(ControlSchemeActionState::Jump(state)) => {
            // Depending on the state of the jump, we need to decide if we want to play the jump
            // animation or the fall animation.
            match &state.memory {
                TnuaBuiltinJumpMemory::NoJump => return,
                TnuaBuiltinJumpMemory::StartingJump { .. } => AnimationState::JumpStart,
                TnuaBuiltinJumpMemory::SlowDownTooFastSlopeJump { .. } => AnimationState::JumpStart,
                TnuaBuiltinJumpMemory::MaintainingJump { .. } => AnimationState::JumpLoop,
                TnuaBuiltinJumpMemory::StoppedMaintainingJump => AnimationState::JumpLand,
                TnuaBuiltinJumpMemory::FallSection => AnimationState::Fall,
            }
        }
        Some(ControlSchemeActionState::Climb(state)) => {
            let TnuaBuiltinClimbMemory::Climbing {
                climbing_velocity, ..
            } = &state.memory
            else {
                return;
            };
            AnimationState::Climb(0.3 * climbing_velocity.dot(Vec3::Y))
        }
        Some(ControlSchemeActionState::Dash(_)) => {
            // Dash action may still fire from Tnua internals; treat as no-op
            AnimationState::StandIdle
        }
        Some(ControlSchemeActionState::WallSlide(_)) => AnimationState::WallSlide,
        None => {
            // If there is no action going on, we'll base the animation on the state of the basis.
            if controller.basis_memory.standing_on_entity().is_none() {
                AnimationState::Fall
            } else {
                let basis_speed = controller.basis_memory.running_velocity.length();
                if basis_speed > cfg.player.movement.idle_to_run_threshold {
                    let speed = anim_knobs::GENERAL_SPEED * basis_speed;
                    // Use sprint animation when at 90%+ of max speed
                    if basis_speed >= cfg.player.movement.speed * 0.9 {
                        AnimationState::Sprint(speed)
                    } else {
                        AnimationState::Run(speed)
                    }
                } else {
                    AnimationState::StandIdle
                }
            }
        }
    };

    // Update player animation state, it could be useful in some systems
    player.animation_state = current_animation.clone();
    let animating_directive = animating_state.update_by_discriminant(current_animation);

    match animating_directive {
        // `Maintain` means that we did not switch to a different variant, so there is no need to change animations.
        TnuaAnimatingStateDirective::Maintain { state } => match state {
            // Some animation states have parameters, that we may want to use to control the
            // animation (without necessarily replacing it). In this case - control the speed
            // of the animation based on the speed of the movement.
            AnimationState::Run(speed)
            | AnimationState::Sprint(speed)
            | AnimationState::Crouch(speed)
            | AnimationState::Climb(speed) => {
                for (_, active_animation) in animation_player.playing_animations_mut() {
                    active_animation.set_speed(*speed);
                }
            }
            // Jumping can be chained, we want to start a new jump animation
            // when one jump is chained to another.
            AnimationState::JumpStart => {
                if controller.action_flow_status().just_starting().is_some() {
                    animation_player.seek_all_by(0.0);
                }
            }
            // For other animations we don't have anything special to do - so we just let them continue.
            _ => {}
        },
        TnuaAnimatingStateDirective::Alter {
            old_state: _,
            state,
        } => {
            // Use transitions for smooth blending between animations
            match state {
                AnimationState::StandIdle => {
                    if let Some(index) = player.animations.get(&Animation::Idle) {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(1.0)
                            .repeat();
                    }
                }
                AnimationState::Run(speed) => {
                    if let Some(index) = player.animations.get(&Animation::JogFwd) {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(*speed)
                            .repeat();
                    }
                }
                AnimationState::Sprint(speed) => {
                    if let Some(index) = player.animations.get(&Animation::Sprint) {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(*speed * 3.0)
                            .repeat();
                    }
                }
                AnimationState::JumpStart => {
                    if let Some(index) = player.animations.get(&Animation::NinjaJumpStart) {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(1.5);
                    }
                }
                AnimationState::JumpLand => {
                    if let Some(index) = player.animations.get(&Animation::NinjaJumpLand) {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(1.5);
                    }
                }
                AnimationState::JumpLoop => {
                    if let Some(index) = player.animations.get(&Animation::NinjaJumpIdle) {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(1.0)
                            .repeat();
                    }
                }
                AnimationState::WallJump => {
                    if let Some(index) = player.animations.get(&Animation::NinjaJumpStart) {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(2.0);
                    }
                }
                AnimationState::WallSlide => {
                    if let Some(index) = player.animations.get(&Animation::NinjaJumpIdle) {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(1.0)
                            .repeat();
                    }
                }
                AnimationState::Fall => {
                    if let Some(index) = player.animations.get(&Animation::NinjaJumpIdle) {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(1.0)
                            .repeat();
                    }
                }
                AnimationState::Crouch(speed) => {
                    if let Some(index) = player.animations.get(&Animation::CrouchFwd) {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(*speed * anim_knobs::CROUCH_ANIMATION_SPEED)
                            .repeat();
                    }
                }
                AnimationState::CrouchIdle => {
                    if let Some(index) = player.animations.get(&Animation::CrouchIdle) {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(1.0)
                            .repeat();
                    }
                }
                AnimationState::Roll => {
                    // Handled in early return above
                }
                AnimationState::LandingStun => {
                    // Handled in early return above
                }
                AnimationState::KnockBack => {
                    if let Some(index) = player.animations.get(&Animation::HitChest) {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(1.0);
                    }
                }
                AnimationState::Climb(speed) => {
                    if let Some(index) = player.animations.get(&Animation::NinjaJumpIdle) {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(*speed)
                            .repeat();
                    }
                }
                AnimationState::Attack | AnimationState::GroundPound => {
                    // Handled in early return above
                }
            }
        }
    }
}
