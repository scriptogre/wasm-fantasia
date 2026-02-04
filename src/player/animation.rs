use super::*;
use crate::combat::AttackState;
use bevy_tnua::{TnuaAnimatingState, TnuaAnimatingStateDirective, builtins::*};

mod anim_knobs {
    pub const GENERAL_SPEED: f32 = 0.1;
    pub const CROUCH_ANIMATION_SPEED: f32 = 2.2;
}

pub fn prepare_animations(
    _: On<SceneInstanceReady>,
    models: Res<Models>,
    gltf_assets: Res<Assets<Gltf>>,
    animation_player: Query<Entity, With<AnimationPlayer>>,
    mut player: Query<&mut Player>,
    mut commands: Commands,
    mut animation_graphs: ResMut<Assets<AnimationGraph>>,
) {
    let Some(gltf) = gltf_assets.get(&models.player) else {
        return;
    };
    let Ok(animation_player) = animation_player.single() else {
        return;
    };
    let Ok(mut player) = player.single_mut() else {
        return;
    };

    let mut graph = AnimationGraph::new();
    let root_node = graph.root;

    // Create flat animation graph
    info!("Loading {} animations:", gltf.named_animations.len());
    for (name, clip) in gltf.named_animations.iter() {
        info!("  - {}", name);
        let node_index = graph.add_clip(clip.clone(), 1.0, root_node);
        player.animations.insert(name.to_string(), node_index);
    }

    // TODO: check if it still works on the second gamepad
    commands.entity(animation_player).insert((
        AnimationGraphHandle(animation_graphs.add(graph)),
        AnimationTransitions::new(),
    ));
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
        &TnuaController,
        &mut Player,
        &mut TnuaAnimatingState<AnimationState>,
        Option<&AttackState>,
    )>,
    mut animation_query: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
) {
    // An actual game should match the animation player and the controller. Here we cheat for
    // simplicity and use the only controller and only player.
    let Ok((controller, mut player, mut animating_state, attack_state)) = player_q.single_mut()
    else {
        return;
    };
    let Ok((mut animation_player, mut transitions)) = animation_query.single_mut() else {
        return;
    };

    // Blend duration for smooth transitions
    const BLEND_DURATION: Duration = Duration::from_millis(150);

    // Check if player is attacking - override Tnua animation
    if let Some(attack) = attack_state {
        if attack.attacking {
            player.animation_state = AnimationState::Attack;
            let animating_directive = animating_state.update_by_discriminant(AnimationState::Attack);

            match animating_directive {
                TnuaAnimatingStateDirective::Alter { .. } => {
                    // 2-hit punch combo: jab -> cross
                    let anim_name = if attack.attack_count % 2 == 0 {
                        "Punch_Jab"
                    } else {
                        "Punch_Cross"
                    };
                    if let Some(index) = player.animations.get(anim_name) {
                        // Start slow (wind-up)
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(1.5);
                    }
                }
                TnuaAnimatingStateDirective::Maintain { .. } => {
                    // Accelerate animation: slow wind-up -> fast strike
                    // Frame 0-5: wind-up (1.5x), Frame 5-15: accelerate (up to 5x)
                    let frame = attack.attack_frame;
                    let speed = if frame < 5 {
                        1.5 // Slow wind-up
                    } else if frame < 15 {
                        // Accelerate from 1.5 to 5.0
                        1.5 + (frame - 5) as f32 * 0.35
                    } else {
                        5.0 // Fast follow-through
                    };

                    for (_, anim) in animation_player.playing_animations_mut() {
                        anim.set_speed(speed);
                    }
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
        // TODO: replace roll with actual dash
        Some(TnuaBuiltinDash::NAME) => AnimationState::Dash,
        Some(TnuaBuiltinWallSlide::NAME) => AnimationState::WallSlide,
        Some("walljump") => AnimationState::WallJump,
        Some(other) => panic!("Unknown action {other}"),
        None => {
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
                    if basis_speed > cfg.player.movement.speed {
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
            // Jumping and dashing can be chained, we want to start a new jump/dash animation
            // when one jump/dash is chained to another.
            AnimationState::JumpStart | AnimationState::Dash => {
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
                    if let Some(index) = player.animations.get("Idle_Loop") {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(1.0)
                            .repeat();
                    }
                }
                AnimationState::Run(speed) => {
                    if let Some(index) = player.animations.get("Jog_Fwd_Loop") {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(*speed)
                            .repeat();
                    }
                }
                AnimationState::Sprint(speed) => {
                    if let Some(index) = player.animations.get("Sprint_Loop") {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(*speed * 3.0)
                            .repeat();
                    }
                }
                AnimationState::JumpStart => {
                    if let Some(index) = player.animations.get("Jump_Start") {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(0.01);
                    }
                }
                AnimationState::JumpLand => {
                    if let Some(index) = player.animations.get("Jump_Land") {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(0.01);
                    }
                }
                AnimationState::JumpLoop => {
                    if let Some(index) = player.animations.get("Jump_Loop") {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(0.5)
                            .repeat();
                    }
                }
                AnimationState::WallJump => {
                    if let Some(index) = player.animations.get("Jump_Start") {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(2.0);
                    }
                }
                AnimationState::WallSlide => {
                    if let Some(index) = player.animations.get("Jump_Loop") {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(1.0)
                            .repeat();
                    }
                }
                AnimationState::Fall => {
                    if let Some(index) = player.animations.get("Jump_Loop") {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(1.0)
                            .repeat();
                    }
                }
                AnimationState::Crouch(speed) => {
                    if let Some(index) = player.animations.get("Crouch_Fwd_Loop") {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(*speed * anim_knobs::CROUCH_ANIMATION_SPEED)
                            .repeat();
                    }
                }
                AnimationState::CrouchIdle => {
                    if let Some(index) = player.animations.get("Crouch_Idle_Loop") {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(1.0)
                            .repeat();
                    }
                }
                AnimationState::Dash => {
                    if let Some(index) = player.animations.get("Roll") {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(3.0);
                    }
                }
                AnimationState::KnockBack => {
                    if let Some(index) = player.animations.get("Hit_Chest") {
                        transitions
                            .play(&mut animation_player, *index, BLEND_DURATION)
                            .set_speed(1.0);
                    }
                }
                AnimationState::Climb(speed) => {
                    if let Some(index) = player.animations.get("Jump_Loop") {
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
