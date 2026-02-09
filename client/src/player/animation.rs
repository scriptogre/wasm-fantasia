use super::*;
use crate::combat::AttackState;
use crate::rules::{Stat, Stats};
use bevy_tnua::{
    TnuaAnimatingState, TnuaAnimatingStateDirective,
    builtins::{TnuaBuiltinDashState, *},
};

mod anim_knobs {
    pub const GENERAL_SPEED: f32 = 0.1;
    pub const CROUCH_ANIMATION_SPEED: f32 = 2.2;
}

/// Track dash animation phase timing (Tnua's states transition too fast)
#[derive(Component, Default)]
pub struct DashAnimationState {
    pub active: bool,
    pub timer: f32,
}

/// Track which attack animation is playing to detect new attacks reliably
#[derive(Component, Default)]
pub struct AttackAnimationState {
    /// Last attack count we started an animation for
    pub last_attack_count: u32,
}

const SLIDE_START_DURATION: f32 = 0.05; // How long to play SlideStart before SlideLoop

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
    CrouchFwd,
    CrouchIdle,
    SlideStart,
    SlideLoop,
    SlideExit,
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
        Self::CrouchFwd,
        Self::CrouchIdle,
        Self::SlideStart,
        Self::SlideLoop,
        Self::SlideExit,
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
            Self::CrouchFwd => "Crouch_Fwd_Loop",
            Self::CrouchIdle => "Crouch_Idle_Loop",
            Self::SlideStart => "Slide_Start",
            Self::SlideLoop => "Slide_Loop",
            Self::SlideExit => "Slide_Exit",
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
            warn!("Animation {:?} ({}) not found in player model", anim, anim.clip_name());
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
        commands.entity(animation_player).queue(move |mut entity: EntityWorldMut| {
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
    time: Res<Time>,
    mut player_q: Query<(
        &TnuaController,
        &mut Player,
        &mut TnuaAnimatingState<AnimationState>,
        Option<&AttackState>,
        Option<&Stats>,
        &mut DashAnimationState,
        &mut AttackAnimationState,
    )>,
    mut animation_query: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    // An actual game should match the animation player and the controller. Here we cheat for
    // simplicity and use the only controller and only player.
    let Ok((
        controller,
        mut player,
        mut animating_state,
        attack_state,
        stats,
        mut dash_anim,
        mut attack_anim,
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
    // Dash takes visual priority over attack (attack hit still triggers via timer)
    let dashing = controller.action_name() == Some(TnuaBuiltinDash::NAME);
    if let Some(attack) = attack_state {
        if attack.is_attacking() && !dashing {
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

    // First check Tnua animation directive
    // Here we use the data from TnuaController to determine what the character is currently doing,
    // so that we can later use that information to decide which animation to play.
    // First we look at the `action_name` to determine which action (if at all) the character is currently performing:
    let current_animation = match controller.action_name() {
        Some(TnuaBuiltinKnockback::NAME) => {
            let (_, knockback_state) = controller
                .concrete_action::<TnuaBuiltinKnockback>()
                .expect("action name mismatch: Knockback");
            match knockback_state {
                TnuaBuiltinKnockbackState::Shove => AnimationState::KnockBack,
                TnuaBuiltinKnockbackState::Pushback { .. } => AnimationState::KnockBack,
            }
        }
        Some(TnuaBuiltinCrouch::NAME) => {
            let (_, crouch_state) = controller
                .concrete_action::<TnuaBuiltinCrouch>()
                .expect("action name mismatch: Crouch");
            // In case of crouch, we need the state of the basis to determine - based on
            // the speed - if the charcter is just crouching or also crawling.
            let Some((_, basis_state)) = controller.concrete_basis::<TnuaBuiltinWalk>() else {
                return;
            };
            let basis_speed = basis_state.running_velocity.length();
            let speed = Some(basis_speed)
                .filter(|speed| cfg.player.movement.idle_to_run_threshold < *speed);
            let is_crouching = basis_state.standing_offset.y < 0.05;
            // info!(
            //     "CROUCH: {is_crouching} speed: {basis_speed}, state:{crouch_state:?}, standing_offset: {}",
            //     basis_state.standing_offset.y
            // );
            match (speed, is_crouching) {
                (None, false) => AnimationState::StandIdle,
                (None, true) => match crouch_state {
                    TnuaBuiltinCrouchState::Maintaining => AnimationState::CrouchIdle,
                    // TODO: have rise animation
                    TnuaBuiltinCrouchState::Rising => AnimationState::CrouchIdle,
                    // TODO: sink animation
                    TnuaBuiltinCrouchState::Sinking => AnimationState::CrouchIdle,
                },
                (Some(speed), false) => AnimationState::Run(speed),
                // TODO: place to handle slide here
                (Some(speed), true) => AnimationState::Crouch(speed),
            }
        }
        // Unless you provide the action names yourself, prefer matching against the `NAME` const
        // of the `TnuaAction` trait. Once `type_name` is stabilized as `const` Tnua will use it to
        // generate these names automatically, which may result in a change to the name.
        Some(TnuaBuiltinJump::NAME) => {
            // In case of jump, we want to cast it so that we can get the concrete jump state.
            let (_, jump_state) = controller
                .concrete_action::<TnuaBuiltinJump>()
                .expect("action name mismatch: Jump");
            // Depending on the state of the jump, we need to decide if we want to play the jump
            // animation or the fall animation.
            match jump_state {
                TnuaBuiltinJumpState::NoJump => return,
                TnuaBuiltinJumpState::StartingJump { .. } => AnimationState::JumpStart,
                TnuaBuiltinJumpState::SlowDownTooFastSlopeJump { .. } => AnimationState::JumpStart,
                TnuaBuiltinJumpState::MaintainingJump { .. } => AnimationState::JumpLoop,
                TnuaBuiltinJumpState::StoppedMaintainingJump => AnimationState::JumpLand,
                TnuaBuiltinJumpState::FallSection => AnimationState::Fall,
            }
        }
        Some(TnuaBuiltinClimb::NAME) => {
            let Some((_, action_state)) = controller.concrete_action::<TnuaBuiltinClimb>() else {
                return;
            };
            let TnuaBuiltinClimbState::Climbing { climbing_velocity } = action_state else {
                return;
            };
            AnimationState::Climb(0.3 * climbing_velocity.dot(Vec3::Y))
        }
        Some(TnuaBuiltinDash::NAME) => {
            let (_, dash_state) = controller
                .concrete_action::<TnuaBuiltinDash>()
                .expect("action name mismatch: Dash");

            // Track dash timing ourselves since Tnua transitions too fast
            if !dash_anim.active {
                // Just started dashing
                dash_anim.active = true;
                dash_anim.timer = 0.0;
            } else {
                dash_anim.timer += time.delta_secs();
            }

            match dash_state {
                TnuaBuiltinDashState::PreDash => AnimationState::SlideStart,
                TnuaBuiltinDashState::During { .. } if dash_anim.timer < SLIDE_START_DURATION => {
                    AnimationState::SlideStart
                }
                TnuaBuiltinDashState::During { .. } => AnimationState::SlideLoop,
                TnuaBuiltinDashState::Braking { .. } => AnimationState::SlideExit,
            }
        }
        Some(TnuaBuiltinWallSlide::NAME) => {
            dash_anim.active = false; // Reset dash tracker
            AnimationState::WallSlide
        }
        Some("walljump") => {
            dash_anim.active = false;
            AnimationState::WallJump
        }
        Some(other) => panic!("Unknown action {other}"),
        None => {
            dash_anim.active = false; // Reset dash tracker
            // If there is no action going on, we'll base the animation on the state of the basis.
            let Some((_, basis_state)) = controller.concrete_basis::<TnuaBuiltinWalk>() else {
                return;
            };
            if basis_state.standing_on_entity().is_none() {
                AnimationState::Fall
            } else {
                let basis_speed = basis_state.running_velocity.length();
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
            // Slide states - let them play through naturally
            AnimationState::SlideStart | AnimationState::SlideLoop | AnimationState::SlideExit => {}
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
                    if let Some(index) = player.animations.get(&Animation::JumpStart) {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(0.01);
                    }
                }
                AnimationState::JumpLand => {
                    if let Some(index) = player.animations.get(&Animation::JumpLand) {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(0.01);
                    }
                }
                AnimationState::JumpLoop => {
                    if let Some(index) = player.animations.get(&Animation::JumpLoop) {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(0.5)
                            .repeat();
                    }
                }
                AnimationState::WallJump => {
                    if let Some(index) = player.animations.get(&Animation::JumpStart) {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(2.0);
                    }
                }
                AnimationState::WallSlide => {
                    if let Some(index) = player.animations.get(&Animation::JumpLoop) {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(1.0)
                            .repeat();
                    }
                }
                AnimationState::Fall => {
                    if let Some(index) = player.animations.get(&Animation::JumpLoop) {
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
                AnimationState::SlideStart => {
                    if let Some(index) = player.animations.get(&Animation::SlideStart) {
                        transitions
                            .play(&mut animation_player, *index, Duration::from_millis(30))
                            .set_speed(2.5); // Fast wind-up
                    }
                }
                AnimationState::SlideLoop => {
                    if let Some(index) = player.animations.get(&Animation::SlideLoop) {
                        transitions
                            .play(&mut animation_player, *index, Duration::from_millis(50))
                            .set_speed(1.0)
                            .repeat();
                    }
                }
                AnimationState::SlideExit => {
                    if let Some(index) = player.animations.get(&Animation::SlideExit) {
                        transitions
                            .play(&mut animation_player, *index, Duration::from_millis(50))
                            .set_speed(1.2);
                    }
                }
                AnimationState::KnockBack => {
                    if let Some(index) = player.animations.get(&Animation::HitChest) {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(1.0);
                    }
                }
                AnimationState::Climb(speed) => {
                    if let Some(index) = player.animations.get(&Animation::JumpLoop) {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(*speed)
                            .repeat();
                    }
                }
                AnimationState::Attack => {
                    // Handled in early return above
                }
            }
        }
    }
}
